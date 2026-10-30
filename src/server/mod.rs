//! The DHCP server core: lease allocation and request handling, free of any
//! network stack.

use core::net::{Ipv4Addr, SocketAddrV4};
use heapless::Vec;
use heapless::index_map::FnvIndexMap;
use leasehund::config::{ConfigError, DEFAULT_LEASE_TIME, DEFAULT_MAX_DNS_SERVERS, DhcpConfig};
use leasehund::event::{AdmissionEvent, TransactionEvent};
use leasehund::lease::{Hostname, Lease, LeaseEntry, StaticLeaseError};
use leasehund::packet::{
    ClientMessage, DHCP_ACK, DHCP_DECLINE, DHCP_DISCOVER, DHCP_INFORM, DHCP_NAK, DHCP_OFFER,
    DHCP_PACKET_SIZE, DHCP_RELEASE, DHCP_REQUEST,
};
use leasehund::pool::{Subnet, SubnetPool};

mod reply;
#[cfg(test)]
mod tests;

const DEFAULT_MAX_CLIENTS: usize = 32;
const DEFAULT_MAX_STATIC: usize = 8;
const DEFAULT_MAX_POOLS: usize = 4;
/// Duration in milliseconds to reserve an offered IP before the client confirms.
const OFFER_RESERVATION_MS: u64 = 60_000;
/// Duration in milliseconds an address declined by a client (`DHCPDECLINE`,
/// usually an address conflict) is withheld from the pool.
const DECLINE_HOLD_MS: u64 = 600_000;
/// Number of declined addresses remembered at once.
const MAX_DECLINED: usize = 8;

/// An action produced by processing a received DHCP packet.
struct PacketAction {
    response: Option<(Vec<u8, DHCP_PACKET_SIZE>, (Ipv4Addr, u16))>,
    /// A completed transaction and the lease it concerns.
    event: Option<(TransactionEvent, Lease)>,
}

impl PacketAction {
    const fn silent() -> Self {
        Self {
            response: None,
            event: None,
        }
    }
}

/// What the server does in answer to one received packet; see
/// [`DhcpServer::handle_packet`].
#[derive(Debug)]
pub struct Outcome {
    /// The reply to send, if any.
    pub reply: Option<Reply>,
    /// The lease or release transaction the packet completed, and the lease it
    /// concerns.
    pub event: Option<(TransactionEvent, Lease)>,
}

/// A reply datagram and where to send it.
#[derive(Debug)]
pub struct Reply {
    bytes: Vec<u8, DHCP_PACKET_SIZE>,
    destination: SocketAddrV4,
}

impl Reply {
    /// The datagram to send, from the server's port 67.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Where to send it (RFC 2131 section 4.1): the relay agent, the client's
    /// own address, or the broadcast address.
    #[must_use]
    pub const fn destination(&self) -> SocketAddrV4 {
        self.destination
    }
}

/// A lightweight DHCP server implementation for embedded systems.
///
/// # Type Parameters
///
/// - `MAX_CLIENTS`: Maximum number of concurrent leases (compile-time fixed). It must be a power
///   of two, as required by the underlying `heapless::FnvIndexMap`.
/// - `MAX_DNS`: Maximum number of DNS servers in configuration
/// - `MAX_STATIC`: Maximum number of static leases (fixed MAC to IP assignments)
/// - `MAX_POOLS`: Maximum number of additional [`SubnetPool`]s for relayed clients
///
/// # Examples
///
/// ```rust,no_run
/// use core::net::Ipv4Addr;
/// use leasehund::DhcpServer;
///
/// let server = DhcpServer::<32, 4>::new(
///     Ipv4Addr::new(192, 168, 1, 1),
///     Ipv4Addr::new(255, 255, 255, 0),
///     Ipv4Addr::new(192, 168, 1, 1),
///     Ipv4Addr::new(8, 8, 8, 8),
///     Ipv4Addr::new(192, 168, 1, 100),
///     Ipv4Addr::new(192, 168, 1, 200),
/// );
/// ```
///
/// ```rust,no_run
/// use core::net::Ipv4Addr;
/// use leasehund::{DhcpServer, DhcpConfigBuilder};
///
/// let config = DhcpConfigBuilder::<4>::new()
///     .server_ip(Ipv4Addr::new(10, 0, 1, 1))
///     .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
///     .router(Ipv4Addr::new(10, 0, 1, 1))
///     .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
///     .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
///     .ip_pool(Ipv4Addr::new(10, 0, 100, 1), Ipv4Addr::new(10, 0, 199, 254))
///     .lease_time(7200)
///     .build();
///
/// let server: DhcpServer<32, 4> = DhcpServer::with_config(config);
/// ```
pub struct DhcpServer<
    const MAX_CLIENTS: usize = DEFAULT_MAX_CLIENTS,
    const MAX_DNS: usize = DEFAULT_MAX_DNS_SERVERS,
    const MAX_STATIC: usize = DEFAULT_MAX_STATIC,
    const MAX_POOLS: usize = DEFAULT_MAX_POOLS,
> {
    config: DhcpConfig<MAX_DNS>,
    leases: FnvIndexMap<[u8; 6], LeaseEntry, MAX_CLIENTS>,
    static_leases: Vec<([u8; 6], Ipv4Addr), MAX_STATIC>,
    /// Pools for clients behind relay agents.
    pools: Vec<SubnetPool, MAX_POOLS>,
    /// Addresses a client reported as in use, with when they become usable again.
    declined: Vec<(Ipv4Addr, u64), MAX_DECLINED>,
}

impl<
    const MAX_CLIENTS: usize,
    const MAX_DNS: usize,
    const MAX_STATIC: usize,
    const MAX_POOLS: usize,
> DhcpServer<MAX_CLIENTS, MAX_DNS, MAX_STATIC, MAX_POOLS>
{
    /// Creates a new DHCP server with a single DNS server.
    ///
    /// For multiple DNS servers or advanced options, use
    /// [`DhcpConfigBuilder`](leasehund::DhcpConfigBuilder) with [`with_config`](Self::with_config).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core::net::Ipv4Addr;
    /// use leasehund::DhcpServer;
    ///
    /// let server = DhcpServer::<32, 4>::new(
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(255, 255, 255, 0),
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(8, 8, 8, 8),
    ///     Ipv4Addr::new(192, 168, 1, 100),
    ///     Ipv4Addr::new(192, 168, 1, 200),
    /// );
    /// ```
    #[must_use]
    pub fn new(
        server_ip: Ipv4Addr,
        subnet_mask: Ipv4Addr,
        router: Ipv4Addr,
        dns_server: Ipv4Addr,
        ip_pool_start: Ipv4Addr,
        ip_pool_end: Ipv4Addr,
    ) -> Self {
        let mut dns_servers = heapless::Vec::new();
        let _ = dns_servers.push(dns_server);
        let config = DhcpConfig::<MAX_DNS> {
            server_ip,
            subnet_mask,
            router: Some(router),
            dns_servers,
            ip_pool_start,
            ip_pool_end,
            lease_time: DEFAULT_LEASE_TIME,
            domain_name: None,
        };
        Self::with_config(config)
    }

    /// Creates a new DHCP server from a [`DhcpConfig`].
    ///
    /// The configuration is used as is; see [`DhcpConfig::validate`] to check
    /// it first.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core::net::Ipv4Addr;
    /// use leasehund::{DhcpServer, DhcpConfigBuilder};
    ///
    /// let config = DhcpConfigBuilder::<4>::new()
    ///     .server_ip(Ipv4Addr::new(10, 0, 1, 1))
    ///     .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
    ///     .router(Ipv4Addr::new(10, 0, 1, 1))
    ///     .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
    ///     .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
    ///     .lease_time(7200)
    ///     .build();
    ///
    /// let server: DhcpServer<32, 4> = DhcpServer::with_config(config);
    /// ```
    #[must_use]
    pub const fn with_config(config: DhcpConfig<MAX_DNS>) -> Self {
        Self {
            config,
            leases: FnvIndexMap::new(),
            static_leases: Vec::new(),
            pools: Vec::new(),
            declined: Vec::new(),
        }
    }

    /// Gets a reference to the current configuration.
    #[must_use]
    pub const fn config(&self) -> &DhcpConfig<MAX_DNS> {
        &self.config
    }

    /// Processes one datagram received on the server port, without any I/O.
    ///
    /// This is the transport-free core of the server: give it the payload of a
    /// UDP datagram sent to port 67 and the current time, and send
    /// [`Outcome::reply`] (if any) from port 67 to its
    /// [`destination`](Reply::destination). `now_ms` is a monotonic clock in
    /// milliseconds; lease expiry times ([`Lease::expires_at_ms`]) are on the
    /// same clock. The Embassy adapter (`run`, `lease_one`) does exactly this.
    ///
    /// `allow_client` is the admission policy, as for
    /// [`run_with_filter_and_callback`](Self::run_with_filter_and_callback).
    /// Packets that are not DHCP client requests are ignored.
    ///
    /// ```rust
    /// use core::net::{Ipv4Addr, SocketAddrV4};
    /// use leasehund::DhcpServer;
    ///
    /// # fn send(_bytes: &[u8], _to: SocketAddrV4) {}
    /// let mut server = DhcpServer::<32, 4>::new(
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(255, 255, 255, 0),
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(8, 8, 8, 8),
    ///     Ipv4Addr::new(192, 168, 1, 100),
    ///     Ipv4Addr::new(192, 168, 1, 200),
    /// );
    /// // A datagram received on port 67, from whatever network stack you use.
    /// let datagram = [0u8; 16];
    /// let now_ms = 0;
    ///
    /// let outcome = server.handle_packet(&datagram, now_ms, |_mac, _event| true);
    /// if let Some(reply) = &outcome.reply {
    ///     send(reply.bytes(), reply.destination());
    /// }
    /// // Too short to be a DHCP request, so it is ignored.
    /// assert!(outcome.reply.is_none() && outcome.event.is_none());
    /// ```
    pub fn handle_packet<P>(&mut self, packet: &[u8], now_ms: u64, mut allow_client: P) -> Outcome
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
    {
        let action = self.prepare_packet_with_filter(packet, &mut allow_client, now_ms);
        Outcome {
            reply: action.response.map(|(bytes, (ip, port))| Reply {
                bytes,
                destination: SocketAddrV4::new(ip, port),
            }),
            event: action.event,
        }
    }

    /// Removes all leases that expired by `now_ms` (see
    /// [`handle_packet`](Self::handle_packet) for the clock).
    pub fn purge_expired_leases_at(&mut self, now_ms: u64) {
        self.purge_expired_at(now_ms);
    }

    /// Gets the current number of active leases, including addresses reserved
    /// for outstanding offers.
    #[must_use]
    pub fn lease_count(&self) -> usize {
        self.leases.len()
    }

    /// Iterates over the current leases and offer reservations.
    ///
    /// Entries that expired since the last packet are still listed until
    /// [`purge_expired_leases`](Self::purge_expired_leases) runs.
    pub fn leases(&self) -> impl Iterator<Item = Lease> + '_ {
        self.leases.values().copied().map(LeaseEntry::to_lease)
    }

    /// The lease or offer reservation of the client with `mac`, if any.
    #[must_use]
    pub fn lease_for(&self, mac: [u8; 6]) -> Option<Lease> {
        self.leases.get(&mac).copied().map(LeaseEntry::to_lease)
    }

    /// Adds a pool for clients behind a relay agent on another subnet.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidSubnetMask`],
    /// [`ConfigError::PoolStartAfterEnd`], [`ConfigError::PoolOutsideSubnet`]
    /// (the start and end are on different subnets) or
    /// [`ConfigError::RouterOutsideSubnet`] for an inconsistent pool,
    /// [`ConfigError::OverlappingSubnet`] if its subnet overlaps the server's or
    /// another pool's, and [`ConfigError::TooManyPools`] if all `MAX_POOLS`
    /// slots are used.
    pub fn add_pool(&mut self, pool: SubnetPool) -> Result<(), ConfigError> {
        let mask = pool.subnet_mask.to_bits();
        if mask.leading_ones() + mask.trailing_zeros() != 32 {
            return Err(ConfigError::InvalidSubnetMask);
        }
        let subnet = Self::pool_subnet(&pool);
        if subnet.start > subnet.end {
            return Err(ConfigError::PoolStartAfterEnd);
        }
        if !subnet.contains(pool.ip_pool_end) {
            return Err(ConfigError::PoolOutsideSubnet);
        }
        if pool.router.is_some_and(|router| !subnet.contains(router)) {
            return Err(ConfigError::RouterOutsideSubnet);
        }
        if subnet.overlaps(&self.main_subnet())
            || self
                .pools
                .iter()
                .any(|other| subnet.overlaps(&Self::pool_subnet(other)))
        {
            return Err(ConfigError::OverlappingSubnet);
        }
        self.pools.push(pool).map_err(|_| ConfigError::TooManyPools)
    }

    /// The additional pools added with [`add_pool`](Self::add_pool).
    #[must_use]
    pub fn pools(&self) -> &[SubnetPool] {
        &self.pools
    }

    /// The server's own subnet, from the configuration.
    const fn main_subnet(&self) -> Subnet {
        let mask = self.config.subnet_mask.to_bits();
        Subnet {
            network: self.config.server_ip.to_bits() & mask,
            mask,
            router: self.config.router,
            start: self.config.ip_pool_start.to_bits(),
            end: self.config.ip_pool_end.to_bits(),
        }
    }

    const fn pool_subnet(pool: &SubnetPool) -> Subnet {
        let mask = pool.subnet_mask.to_bits();
        Subnet {
            network: pool.ip_pool_start.to_bits() & mask,
            mask,
            router: pool.router,
            start: pool.ip_pool_start.to_bits(),
            end: pool.ip_pool_end.to_bits(),
        }
    }

    /// The subnet to serve a request from: the relay's (by `giaddr`) or our
    /// own. `None` for a relay on a subnet we have no pool for.
    fn subnet_for(&self, giaddr: Option<Ipv4Addr>) -> Option<Subnet> {
        let main = self.main_subnet();
        match giaddr {
            None => Some(main),
            Some(relay) if main.contains(relay) => Some(main),
            Some(relay) => self
                .pools
                .iter()
                .map(Self::pool_subnet)
                .find(|subnet| subnet.contains(relay)),
        }
    }

    /// The subnet `ip` belongs to, if any.
    fn subnet_of(&self, ip: Ipv4Addr) -> Option<Subnet> {
        core::iter::once(self.main_subnet())
            .chain(self.pools.iter().map(Self::pool_subnet))
            .find(|subnet| subnet.contains(ip))
    }

    /// Forgets the lease of the client with `mac`, returning its address.
    ///
    /// The client is not told; it keeps using the address until its lease
    /// would have expired or it next renews (and is refused).
    pub fn revoke(&mut self, mac: [u8; 6]) -> Option<Ipv4Addr> {
        self.leases.remove(&mac).map(|entry| entry.ip)
    }

    /// Always assigns `ip` to the client with `mac`.
    ///
    /// The address does not have to be inside the dynamic pool, but it must be
    /// on the server's subnet or the subnet of a [`SubnetPool`] (add pools
    /// first). It is only handed out to the client while the client is on
    /// that subnet, and never to another client. Adding a static lease for a MAC that already
    /// has one replaces it.
    ///
    /// # Errors
    ///
    /// See [`StaticLeaseError`].
    pub fn add_static_lease(&mut self, mac: [u8; 6], ip: Ipv4Addr) -> Result<(), StaticLeaseError> {
        let subnet = self.subnet_of(ip).ok_or(StaticLeaseError::OutsideSubnet)?;
        if self.is_reserved_address(ip, &subnet) {
            return Err(StaticLeaseError::ReservedAddress);
        }
        if self
            .static_leases
            .iter()
            .any(|(other, static_ip)| *static_ip == ip && *other != mac)
        {
            return Err(StaticLeaseError::AddressTaken);
        }
        if let Some(slot) = self.static_leases.iter_mut().find(|(m, _)| *m == mac) {
            slot.1 = ip;
            return Ok(());
        }
        self.static_leases
            .push((mac, ip))
            .map_err(|_| StaticLeaseError::Full)
    }

    /// Removes the static lease of `mac`, returning its address.
    pub fn remove_static_lease(&mut self, mac: [u8; 6]) -> Option<Ipv4Addr> {
        let index = self.static_leases.iter().position(|(m, _)| *m == mac)?;
        Some(self.static_leases.swap_remove(index).1)
    }

    fn purge_expired_at(&mut self, now: u64) {
        let mut expired: Vec<[u8; 6], MAX_CLIENTS> = Vec::new();
        for (mac, entry) in &self.leases {
            if entry.expires_at <= now {
                let _ = expired.push(*mac);
            }
        }
        for mac in &expired {
            self.leases.remove(mac);
        }
        self.declined.retain(|(_, until)| *until > now);
    }

    /// Checks if the main IP pool (of [`DhcpConfig`]) is full.
    ///
    /// Call [`purge_expired_leases`](Self::purge_expired_leases) first
    /// to reclaim expired entries if desired.
    #[must_use]
    pub fn is_pool_full(&self) -> bool {
        self.leases.len() >= MAX_CLIENTS || self.get_next_available_ip().is_none()
    }

    /// Finds the next available IP address in the configured pool.
    ///
    /// The server's own address, the router, the network and broadcast
    /// addresses of the subnet, addresses held by leases, static leases and
    /// recently declined addresses are skipped.
    ///
    /// # Example
    ///
    /// ```rust
    /// use core::net::Ipv4Addr;
    /// use leasehund::{DhcpConfig, DhcpServer};
    ///
    /// let mut dns_servers = heapless::Vec::<Ipv4Addr, 4>::new();
    /// dns_servers.push(Ipv4Addr::new(1, 1, 1, 1)).ok();
    /// let config: DhcpConfig<4> = DhcpConfig {
    ///     server_ip: Ipv4Addr::new(10, 0, 0, 1),
    ///     subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
    ///     router: Some(Ipv4Addr::new(10, 0, 0, 1)),
    ///     dns_servers,
    ///     ip_pool_start: Ipv4Addr::new(10, 0, 0, 100),
    ///     ip_pool_end: Ipv4Addr::new(10, 0, 0, 102),
    ///     lease_time: 3600,
    ///     domain_name: None,
    /// };
    /// let server: DhcpServer<32, 4> = DhcpServer::with_config(config);
    /// let next = server.get_next_available_ip();
    /// assert!(matches!(next, Some(ip) if ip == Ipv4Addr::new(10, 0, 0, 100)));
    /// ```
    #[must_use]
    pub fn get_next_available_ip(&self) -> Option<Ipv4Addr> {
        let subnet = self.main_subnet();
        // An inverted pool is empty, not a panic.
        (subnet.start..=subnet.end)
            .map(Ipv4Addr::from)
            .find(|ip| self.is_assignable(*ip, None, &subnet))
    }

    /// The server, the subnet's router, and the subnet's network and
    /// broadcast addresses are never handed out.
    fn is_reserved_address(&self, ip: Ipv4Addr, subnet: &Subnet) -> bool {
        ip == self.config.server_ip
            || Some(ip) == subnet.router
            || ip.is_unspecified()
            || subnet.is_network_or_broadcast(ip)
    }

    /// Whether `ip` may be given to the client `mac` from the dynamic pool of
    /// `subnet` (`None`: to any new client).
    fn is_assignable(&self, ip: Ipv4Addr, mac: Option<[u8; 6]>, subnet: &Subnet) -> bool {
        subnet.in_pool(ip)
            && !self.is_reserved_address(ip, subnet)
            && !self.declined.iter().any(|(declined, _)| *declined == ip)
            && self
                .leases
                .values()
                .all(|lease| lease.ip != ip || Some(lease.mac) == mac)
            && self
                .static_leases
                .iter()
                .all(|(static_mac, static_ip)| *static_ip != ip || Some(*static_mac) == mac)
    }

    /// The static address of `mac` on `subnet`, unless another client
    /// currently holds it or it was recently declined.
    fn static_ip_for(&self, mac: [u8; 6], subnet: &Subnet) -> Option<Ipv4Addr> {
        let (_, ip) = self
            .static_leases
            .iter()
            .find(|(m, ip)| *m == mac && subnet.contains(*ip))?;
        let available = self
            .leases
            .values()
            .all(|lease| lease.ip != *ip || lease.mac == mac)
            && !self.declined.iter().any(|(declined, _)| declined == ip);
        available.then_some(*ip)
    }

    /// Choose the address to offer `mac` on `subnet` (RFC 2131 section
    /// 4.3.1): its static lease, its current binding, the address it asked
    /// for, or the next free one.
    fn address_for(
        &self,
        mac: [u8; 6],
        requested: Option<Ipv4Addr>,
        subnet: &Subnet,
    ) -> Option<Ipv4Addr> {
        if let Some(ip) = self.static_ip_for(mac, subnet) {
            return Some(ip);
        }
        if let Some(lease) = self.leases.get(&mac)
            && self.is_assignable(lease.ip, Some(mac), subnet)
        {
            return Some(lease.ip);
        }
        if let Some(ip) = requested
            && self.is_assignable(ip, Some(mac), subnet)
        {
            return Some(ip);
        }
        (subnet.start..=subnet.end)
            .map(Ipv4Addr::from)
            .find(|ip| self.is_assignable(*ip, Some(mac), subnet))
    }

    /// Whether `mac` may be bound to `ip` on `subnet` now (for a
    /// `DHCPREQUEST`).
    fn may_bind(&self, mac: [u8; 6], ip: Ipv4Addr, subnet: &Subnet) -> bool {
        if !subnet.contains(ip) {
            // For example a client that moved to another relay's subnet.
            return false;
        }
        // While its static address is held by another client or declined,
        // the client is served from the pool like any other.
        if let Some(static_ip) = self.static_ip_for(mac, subnet) {
            return static_ip == ip;
        }
        // No record, for example after the server restarted: accept the
        // address if it is free, so clients keep their addresses.
        self.leases.get(&mac).map_or_else(
            || self.is_assignable(ip, Some(mac), subnet),
            |lease| {
                (lease.ip == ip && !self.declined.iter().any(|(d, _)| *d == ip))
                    // A lease on another subnet does not hold the client back.
                    || (!subnet.contains(lease.ip) && self.is_assignable(ip, Some(mac), subnet))
            },
        )
    }

    /// Store a lease, returning `None` if the table is full.
    ///
    /// A client that sent no host name this time keeps the one it sent before.
    fn record(
        &mut self,
        mac: [u8; 6],
        ip: Ipv4Addr,
        expires_at: u64,
        bound: bool,
        hostname: Option<Hostname>,
    ) -> Option<Lease> {
        let hostname = hostname.or_else(|| self.leases.get(&mac).and_then(|lease| lease.hostname));
        let entry = LeaseEntry {
            ip,
            mac,
            expires_at,
            bound,
            hostname,
        };
        self.leases.insert(mac, entry).ok()?;
        Some(entry.to_lease())
    }

    fn lease_expiry(&self, now: u64) -> u64 {
        if self.config.lease_time == u32::MAX {
            u64::MAX
        } else {
            now.saturating_add(u64::from(self.config.lease_time) * 1000)
        }
    }

    /// Validates and processes an incoming DHCP packet before any response is sent.
    fn prepare_packet_with_filter<P>(
        &mut self,
        data: &[u8],
        allow_client: &mut P,
        now: u64,
    ) -> PacketAction
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
    {
        // Reclaim expired leases before processing.
        self.purge_expired_at(now);

        let Some(message) = ClientMessage::parse(data) else {
            return PacketAction::silent();
        };
        let mac = message.mac;
        // Messages that name another server are none of our business.
        let for_us = message
            .server_id
            .is_none_or(|id| id == self.config.server_ip);

        match message.msg_type {
            DHCP_DISCOVER => {
                let Some(subnet) = self.subnet_for(message.giaddr) else {
                    return PacketAction::silent();
                };
                if !allow_client(mac, AdmissionEvent::Discover) {
                    return PacketAction::silent();
                }
                self.offer(&message, &subnet, now)
            }
            DHCP_REQUEST => {
                if !for_us {
                    // The client chose another server's offer: free ours.
                    if self.leases.get(&mac).is_some_and(|lease| !lease.bound) {
                        self.leases.remove(&mac);
                    }
                    return PacketAction::silent();
                }
                let Some(subnet) = self.subnet_for(message.giaddr) else {
                    return PacketAction::silent();
                };
                if !allow_client(mac, AdmissionEvent::Request) {
                    return PacketAction::silent();
                }
                self.acknowledge(&message, &subnet, now)
            }
            DHCP_DECLINE => {
                if for_us && let Some(ip) = message.requested_ip {
                    if self.leases.get(&mac).is_some_and(|lease| lease.ip == ip) {
                        self.leases.remove(&mac);
                    }
                    self.hold_declined(ip, now);
                }
                PacketAction::silent()
            }
            DHCP_RELEASE => {
                if !for_us {
                    return PacketAction::silent();
                }
                let _ = allow_client(mac, AdmissionEvent::Release);
                let matches = self
                    .leases
                    .get(&mac)
                    .is_some_and(|lease| message.ciaddr.is_none_or(|ip| ip == lease.ip));
                let event = if matches {
                    self.leases.remove(&mac).map(|entry| {
                        (
                            TransactionEvent::Released(entry.ip, entry.mac),
                            entry.to_lease(),
                        )
                    })
                } else {
                    None
                };
                PacketAction {
                    response: None,
                    event,
                }
            }
            DHCP_INFORM => {
                let Some(subnet) = self.subnet_for(message.giaddr) else {
                    return PacketAction::silent();
                };
                if !allow_client(mac, AdmissionEvent::Inform) {
                    return PacketAction::silent();
                }
                PacketAction {
                    response: Some(self.reply(&message, DHCP_ACK, None, &subnet)),
                    event: None,
                }
            }
            _ => PacketAction::silent(),
        }
    }

    /// Answer a `DHCPDISCOVER` with an offer, reserving the address briefly.
    fn offer(&mut self, message: &ClientMessage, subnet: &Subnet, now: u64) -> PacketAction {
        let mac = message.mac;
        let Some(ip) = self.address_for(mac, message.requested_ip, subnet) else {
            // Nothing to offer: stay silent rather than offer 0.0.0.0.
            return PacketAction::silent();
        };
        // A client that is already bound to this address keeps its full lease.
        let already_bound = self
            .leases
            .get(&mac)
            .is_some_and(|lease| lease.bound && lease.ip == ip);
        let reserved = if already_bound {
            true
        } else {
            self.record(mac, ip, now + OFFER_RESERVATION_MS, false, message.hostname)
                .is_some()
        };
        if !reserved {
            // The lease table is full; an unrecorded offer could be made twice.
            return PacketAction::silent();
        }
        PacketAction {
            response: Some(self.reply(message, DHCP_OFFER, Some(ip), subnet)),
            event: None,
        }
    }

    /// Answer a `DHCPREQUEST` addressed to us with an ACK or a NAK
    /// (RFC 2131 section 4.3.2).
    fn acknowledge(&mut self, message: &ClientMessage, subnet: &Subnet, now: u64) -> PacketAction {
        let mac = message.mac;
        let target = if message.server_id.is_some() {
            // SELECTING: the requested address must be present.
            message.requested_ip
        } else {
            // INIT-REBOOT names the address in option 50; RENEWING and
            // REBINDING put it in ciaddr.
            message.requested_ip.or(message.ciaddr)
        };
        let Some(ip) = target else {
            return if message.server_id.is_some() {
                self.nak(message)
            } else {
                PacketAction::silent()
            };
        };

        let lease = if self.may_bind(mac, ip, subnet) {
            self.record(mac, ip, self.lease_expiry(now), true, message.hostname)
        } else {
            None
        };
        // Wrong address, address taken, or no room to record the lease: NAK.
        lease.map_or_else(
            || self.nak(message),
            |lease| PacketAction {
                response: Some(self.reply(message, DHCP_ACK, Some(ip), subnet)),
                event: Some((TransactionEvent::Leased(ip, mac), lease)),
            },
        )
    }

    fn nak(&self, message: &ClientMessage) -> PacketAction {
        PacketAction {
            response: Some(self.reply(message, DHCP_NAK, None, &self.main_subnet())),
            event: None,
        }
    }

    /// Remember a declined address so it is not offered for a while.
    fn hold_declined(&mut self, ip: Ipv4Addr, now: u64) {
        self.declined.retain(|(declined, _)| *declined != ip);
        if self.declined.is_full() {
            // Forget the address that becomes usable soonest.
            if let Some(index) = self
                .declined
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, until))| *until)
                .map(|(index, _)| index)
            {
                self.declined.swap_remove(index);
            }
        }
        let _ = self.declined.push((ip, now + DECLINE_HOLD_MS));
    }
}
