//! # Leasehund
//!
//! A lightweight, embedded-friendly DHCP server implementation for Rust `no_std` environments.
//!
//! ## Overview
//!
//! Leasehund provides a minimal DHCP server implementation designed for embedded systems and
//! resource-constrained environments. It supports the core DHCP functionality needed for
//! automatic IP address assignment in local networks.
//!
//! ## Protocol Compliance
//!
//! Leasehund is compliant with [RFC 2131](https://www.rfc-editor.org/rfc/rfc2131) and [RFC 2132](https://www.rfc-editor.org/rfc/rfc2132),
//! including strict checking and emission of the DHCP magic cookie (0x63825363) in all packets as required by the standard.
//!
//! ## Features
//!
//! - **No-std compatible**: Designed for embedded systems without heap allocation
//! - **Embassy integration**: Built on top of Embassy async runtime and networking stack
//! - **Configurable IP pools**: Define custom IP address ranges for client assignment
//! - **Lease expiry**: Expired leases are automatically reclaimed before each allocation
//! - **IP reservation**: Offered IPs are reserved to prevent duplicate offers
//! - **Multiple DNS servers**: Support for up to N DNS servers (compile-time const generic)
//! - **Optional router configuration**: Router/gateway can be disabled if not needed
//! - **Builder pattern**: Fluent API for easy configuration
//! - **Memory efficient**: Uses heapless data structures with compile-time size limits
//!
//! ## Usage
//!
//! ### Basic Usage
//!
//! ```rust,no_run
//! use core::net::Ipv4Addr;
//! use leasehund::DhcpServer;
//! use embassy_net::Stack;
//!
//! # async fn example(stack: Stack<'static>) {
//! let mut server = DhcpServer::<32, 4>::new(
//!     Ipv4Addr::new(192, 168, 1, 1),    // Server IP
//!     Ipv4Addr::new(255, 255, 255, 0),  // Subnet mask
//!     Ipv4Addr::new(192, 168, 1, 1),    // Router/Gateway
//!     Ipv4Addr::new(8, 8, 8, 8),        // DNS server
//!     Ipv4Addr::new(192, 168, 1, 100),  // IP pool start
//!     Ipv4Addr::new(192, 168, 1, 200),  // IP pool end
//! );
//!
//! // Run the DHCP server (this will loop forever)
//! server.run(stack).await;
//! # }
//! ```
//!
//! ### Advanced Configuration
//!
//! ```rust,no_run
//! use core::net::Ipv4Addr;
//! use leasehund::{DhcpServer, DhcpConfigBuilder};
//! use embassy_net::Stack;
//!
//! # async fn example(stack: Stack<'static>) {
//! let config = DhcpConfigBuilder::<4>::new()
//!     .server_ip(Ipv4Addr::new(10, 0, 1, 1))
//!     .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
//!     .router(Ipv4Addr::new(10, 0, 1, 1))
//!     .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
//!     .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
//!     .add_dns_server(Ipv4Addr::new(8, 8, 8, 8))
//!     .ip_pool(
//!         Ipv4Addr::new(10, 0, 100, 1),
//!         Ipv4Addr::new(10, 0, 199, 254)
//!     )
//!     .lease_time(7200)
//!     .build();
//!
//! let mut server: DhcpServer<32, 4> = DhcpServer::with_config(config);
//! server.run(stack).await;
//! # }
//! ```
//!
//! ## Supported DHCP Messages
//!
//! - **DHCP Discover**: Client broadcast to find available DHCP servers
//! - **DHCP Offer**: Server response offering an IP address
//! - **DHCP Request**: Client request to lease a specific IP address
//! - **DHCP ACK**: Server acknowledgment of IP address lease
//! - **DHCP Release**: Client notification of IP address release
//! - **DHCP NAK**: Server refusal of a request for a wrong or unavailable address
//! - **DHCP Decline**: Client report of an address conflict; the address is withheld for a while
//! - **DHCP Inform**: Configuration request from a client with a manually set address
//!
//! ## Limitations
//!
//! - Maximum number of concurrent client leases is compile-time fixed via const generics
//! - IPv4 only
//! - Fixed UDP buffer sizes (1024 bytes)
//!
//! ## Network Configuration
//!
//! The server listens on UDP port 67 (standard DHCP server port) and sends responses
//! to port 68 (standard DHCP client port). Replies to requests forwarded by a relay agent
//! go back to the relay on port 67; replies to clients that already have an address
//! (renewals, `DHCPINFORM`) are unicast to it; everything else is broadcast, since the
//! client has no address yet.
//!
//! ## Memory Usage
//!
//! The server uses a fixed-size hash map to store lease information, with a maximum
//! number of entries set by the `MAX_CLIENTS` const generic parameter. Each lease entry contains:
//! - IPv4 address (4 bytes)
//! - MAC address (6 bytes)
//! - Lease expiration timestamp (8 bytes)
//!
//! ## Advanced Usage
//!
//! For manual transaction handling, use the `lease_one` method:
//! ```rust
//! use core::net::Ipv4Addr;
//! use leasehund::{DhcpServer, DHCPServerSocket, DHCPServerBuffers, TransactionEvent, DhcpConfigBuilder};
//! # use embassy_net::Stack;
//!
//! # async fn example(stack: Stack<'static>) {
//! let config = DhcpConfigBuilder::<4>::new()
//!     .server_ip(Ipv4Addr::new(10, 0, 1, 1))
//!     .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
//!     .router(Ipv4Addr::new(10, 0, 1, 1))
//!     .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
//!     .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
//!     .add_dns_server(Ipv4Addr::new(8, 8, 8, 8))
//!     .ip_pool(
//!         Ipv4Addr::new(10, 0, 100, 1),
//!         Ipv4Addr::new(10, 0, 199, 254)
//!     )
//!     .lease_time(7200)
//!     .build();
//! let mut server = DhcpServer::<32, 4>::with_config(config);
//! let mut buffers = DHCPServerBuffers::new();
//! let mut socket = DHCPServerSocket::new(stack, &mut buffers);
//! let _event: Result<TransactionEvent, _> = server.lease_one(&mut socket).await;
//! # }
//! ```

#![no_std]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

use core::net::Ipv4Addr;
use embassy_net::Stack;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_time::{Duration, Timer};
use heapless::Vec;
use heapless::index_map::FnvIndexMap;

mod options;

/// Reexported types from Embassy for convenience
pub use embassy_net::udp::RecvError;

/// Standard DHCP server port (RFC 2131)
const DHCP_SERVER_PORT: u16 = 67;
/// Standard DHCP client port (RFC 2131)
const DHCP_CLIENT_PORT: u16 = 68;

// Default values for DHCP server configuration
const DEFAULT_MAX_CLIENTS: usize = 32;
const DEFAULT_MAX_DNS_SERVERS: usize = 4;
const DEFAULT_LEASE_TIME: u32 = 86400; // 24 hours in seconds
const SOCKET_BUFFER_SIZE: usize = 1024;
const DEFAULT_MAX_STATIC: usize = 8;
const DEFAULT_MAX_POOLS: usize = 4;
/// Longest client host name kept with a lease, in bytes.
pub const MAX_HOSTNAME_LEN: usize = 32;

/// Duration in milliseconds to reserve an offered IP before the client confirms.
const OFFER_RESERVATION_MS: u64 = 60_000;
/// Duration in milliseconds an address declined by a client (`DHCPDECLINE`,
/// usually an address conflict) is withheld from the pool.
const DECLINE_HOLD_MS: u64 = 600_000;
/// Number of declined addresses remembered at once.
const MAX_DECLINED: usize = 8;

/// Configuration options for the DHCP server.
///
/// # Examples
///
/// ```rust
/// use core::net::Ipv4Addr;
/// use leasehund::{DhcpConfig, DhcpServer};
///
/// let mut dns_servers = heapless::Vec::<Ipv4Addr, 4>::new();
/// dns_servers.push(Ipv4Addr::new(8, 8, 8, 8)).ok();
/// dns_servers.push(Ipv4Addr::new(8, 8, 4, 4)).ok();
///
/// let config: DhcpConfig<4> = DhcpConfig {
///     server_ip: Ipv4Addr::new(192, 168, 1, 1),
///     subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
///     router: Some(Ipv4Addr::new(192, 168, 1, 1)),
///     dns_servers,
///     ip_pool_start: Ipv4Addr::new(192, 168, 1, 100),
///     ip_pool_end: Ipv4Addr::new(192, 168, 1, 200),
///     lease_time: 3600,
///     domain_name: None,
/// };
///
/// let server: DhcpServer<32, 4> = DhcpServer::with_config(config);
/// ```
#[derive(Clone, Debug)]
pub struct DhcpConfig<const MAX_DNS: usize = DEFAULT_MAX_DNS_SERVERS> {
    /// The IP address of this DHCP server
    pub server_ip: Ipv4Addr,
    /// Subnet mask to assign to clients
    pub subnet_mask: Ipv4Addr,
    /// Default gateway/router IP address to assign to clients (optional)
    pub router: Option<Ipv4Addr>,
    /// List of DNS server IP addresses to assign to clients
    pub dns_servers: heapless::Vec<Ipv4Addr, MAX_DNS>,
    /// Start of the IP address pool for client assignment
    pub ip_pool_start: Ipv4Addr,
    /// End of the IP address pool for client assignment
    pub ip_pool_end: Ipv4Addr,
    /// Lease time in seconds (default: 24 hours). `u32::MAX` means an
    /// infinite lease.
    pub lease_time: u32,
    /// Domain name handed to clients (option 15), for example `"lan"`.
    pub domain_name: Option<&'static str>,
}

impl<const MAX_DNS: usize> Default for DhcpConfig<MAX_DNS> {
    fn default() -> Self {
        let mut dns_servers = heapless::Vec::new();
        let _ = dns_servers.push(Ipv4Addr::new(8, 8, 8, 8));
        Self {
            server_ip: Ipv4Addr::new(192, 168, 1, 1),
            subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
            router: Some(Ipv4Addr::new(192, 168, 1, 1)),
            dns_servers,
            ip_pool_start: Ipv4Addr::new(192, 168, 1, 100),
            ip_pool_end: Ipv4Addr::new(192, 168, 1, 200),
            lease_time: DEFAULT_LEASE_TIME,
            domain_name: None,
        }
    }
}

/// A problem found by [`DhcpConfig::validate`].
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ConfigError {
    /// `ip_pool_start` is after `ip_pool_end`.
    PoolStartAfterEnd,
    /// The pool is not inside the server's subnet (`server_ip` and
    /// `subnet_mask`), so clients would get addresses they cannot use.
    PoolOutsideSubnet,
    /// The subnet mask is not a contiguous run of one bits.
    InvalidSubnetMask,
    /// The router is not inside the server's subnet. The builder starts with
    /// router `192.168.1.1`, so this usually means `.router()` or
    /// `.no_router()` was forgotten after changing the server address.
    RouterOutsideSubnet,
    /// A lease time of zero seconds.
    ZeroLeaseTime,
    /// The domain name is longer than the 255 bytes an option can hold.
    DomainNameTooLong,
    /// A [`SubnetPool`] overlaps the server's subnet or another pool.
    OverlappingSubnet,
    /// All `MAX_POOLS` additional pools are in use.
    TooManyPools,
}

impl<const MAX_DNS: usize> DhcpConfig<MAX_DNS> {
    /// Checks the configuration for mistakes that would make the server hand
    /// out unusable leases.
    ///
    /// The server, router, network and broadcast addresses may lie inside the
    /// pool: they are never assigned.
    ///
    /// # Errors
    ///
    /// Returns the first [`ConfigError`] found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mask = u32::from(self.subnet_mask);
        if mask.leading_ones() + mask.trailing_zeros() != 32 {
            return Err(ConfigError::InvalidSubnetMask);
        }
        let start = u32::from(self.ip_pool_start);
        let end = u32::from(self.ip_pool_end);
        if start > end {
            return Err(ConfigError::PoolStartAfterEnd);
        }
        let network = u32::from(self.server_ip) & mask;
        if start & mask != network || end & mask != network {
            return Err(ConfigError::PoolOutsideSubnet);
        }
        if self
            .router
            .is_some_and(|router| u32::from(router) & mask != network)
        {
            return Err(ConfigError::RouterOutsideSubnet);
        }
        if self.lease_time == 0 {
            return Err(ConfigError::ZeroLeaseTime);
        }
        if self.domain_name.is_some_and(|name| name.len() > 255) {
            return Err(ConfigError::DomainNameTooLong);
        }
        Ok(())
    }
}

/// Builder pattern for creating DHCP server configurations.
///
/// The builder starts with **no DNS servers** configured. Use
/// [`add_dns_server`](Self::add_dns_server) to add them.
///
/// # Examples
///
/// ```rust
/// use core::net::Ipv4Addr;
/// use leasehund::{DhcpConfigBuilder, DhcpServer};
///
/// let config = DhcpConfigBuilder::<4>::new()
///     .server_ip(Ipv4Addr::new(10, 0, 1, 1))
///     .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
///     .router(Ipv4Addr::new(10, 0, 1, 1))
///     .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
///     .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
///     .ip_pool(
///         Ipv4Addr::new(10, 0, 100, 1),
///         Ipv4Addr::new(10, 0, 199, 254)
///     )
///     .lease_time(7200)
///     .build();
///
/// let server: DhcpServer<32, 4> = DhcpServer::with_config(config);
/// ```
#[derive(Clone, Debug)]
pub struct DhcpConfigBuilder<const MAX_DNS: usize = DEFAULT_MAX_DNS_SERVERS> {
    config: DhcpConfig<MAX_DNS>,
}

impl<const MAX_DNS: usize> DhcpConfigBuilder<MAX_DNS> {
    /// Creates a new configuration builder.
    ///
    /// Starts with sensible defaults but **no DNS servers**.
    #[must_use]
    pub fn new() -> Self {
        let mut config = DhcpConfig::default();
        config.dns_servers.clear();
        Self { config }
    }

    /// Sets the DHCP server IP address.
    #[must_use]
    pub const fn server_ip(mut self, ip: Ipv4Addr) -> Self {
        self.config.server_ip = ip;
        self
    }

    /// Sets the subnet mask.
    #[must_use]
    pub const fn subnet_mask(mut self, mask: Ipv4Addr) -> Self {
        self.config.subnet_mask = mask;
        self
    }

    /// Sets the default gateway/router IP address.
    #[must_use]
    pub const fn router(mut self, router: Ipv4Addr) -> Self {
        self.config.router = Some(router);
        self
    }

    /// Removes the router option (no default gateway).
    #[must_use]
    pub const fn no_router(mut self) -> Self {
        self.config.router = None;
        self
    }

    /// Adds a DNS server to the configuration.
    ///
    /// If the maximum number of DNS servers has been reached, the server
    /// is silently dropped.
    #[must_use]
    pub fn add_dns_server(mut self, dns: Ipv4Addr) -> Self {
        let _ = self.config.dns_servers.push(dns);
        self
    }

    /// Clears all DNS servers.
    #[must_use]
    pub fn clear_dns_servers(mut self) -> Self {
        self.config.dns_servers.clear();
        self
    }

    /// Sets the IP address pool range.
    #[must_use]
    pub const fn ip_pool(mut self, start: Ipv4Addr, end: Ipv4Addr) -> Self {
        self.config.ip_pool_start = start;
        self.config.ip_pool_end = end;
        self
    }

    /// Sets the lease time in seconds.
    #[must_use]
    pub const fn lease_time(mut self, seconds: u32) -> Self {
        self.config.lease_time = seconds;
        self
    }

    /// Sets the domain name handed to clients (option 15).
    #[must_use]
    pub const fn domain_name(mut self, name: &'static str) -> Self {
        self.config.domain_name = Some(name);
        self
    }

    /// Builds the final configuration.
    #[must_use]
    pub fn build(self) -> DhcpConfig<MAX_DNS> {
        self.config
    }

    /// Builds the configuration after checking it with
    /// [`DhcpConfig::validate`].
    ///
    /// # Errors
    ///
    /// Returns the first [`ConfigError`] found.
    pub fn try_build(self) -> Result<DhcpConfig<MAX_DNS>, ConfigError> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl<const MAX_DNS: usize> Default for DhcpConfigBuilder<MAX_DNS> {
    fn default() -> Self {
        Self::new()
    }
}

// DHCP Message Types (RFC 2131)
const DHCP_DISCOVER: u8 = 1;
const DHCP_OFFER: u8 = 2;
const DHCP_REQUEST: u8 = 3;
const DHCP_DECLINE: u8 = 4;
const DHCP_ACK: u8 = 5;
const DHCP_NAK: u8 = 6;
const DHCP_RELEASE: u8 = 7;
const DHCP_INFORM: u8 = 8;

/// BOOTP operation codes (RFC 951).
const BOOTREQUEST: u8 = 1;
const BOOTREPLY: u8 = 2;
/// Hardware type and address length of Ethernet.
const HTYPE_ETHERNET: u8 = 1;
const HLEN_ETHERNET: u8 = 6;
/// The broadcast bit of the `flags` field.
const BROADCAST_FLAG: u16 = 0x8000;

// The standard DHCP magic cookie (0x63825363).
// Required by RFC 2132 section 2.
const DHCP_MAGIC: [u8; 4] = [0x63, 0x82, 0x53, 0x63];

/// DHCP packet structure as defined in [RFC 2131](https://www.rfc-editor.org/rfc/rfc2131).
///
/// Packed to match the wire format exactly. Multi-byte integers hold the raw
/// network-order bytes; convert with `from_be`/`to_be`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct DhcpPacket {
    op: u8,
    htype: u8,
    hlen: u8,
    hops: u8,
    xid: u32,
    secs: u16,
    flags: u16,
    ciaddr: [u8; 4],
    yiaddr: [u8; 4],
    siaddr: [u8; 4],
    giaddr: [u8; 4],
    chaddr: [u8; 16],
    sname: [u8; 64],
    file: [u8; 128],
    magic: [u8; 4],
}

impl Default for DhcpPacket {
    fn default() -> Self {
        Self {
            op: 0,
            htype: 0,
            hlen: 0,
            hops: 0,
            xid: 0,
            secs: 0,
            flags: 0,
            ciaddr: [0; 4],
            yiaddr: [0; 4],
            siaddr: [0; 4],
            giaddr: [0; 4],
            chaddr: [0; 16],
            sname: [0; 64],
            file: [0; 128],
            magic: DHCP_MAGIC,
        }
    }
}

const FIXED_PART_SIZE: usize = core::mem::size_of::<DhcpPacket>();
const OPTIONS_SIZE: usize = 335;
const DHCP_PACKET_SIZE: usize = FIXED_PART_SIZE + OPTIONS_SIZE + 1; // +1 for END option

/// A DHCP lease entry for a client.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
struct LeaseEntry {
    ip: Ipv4Addr,
    mac: [u8; 6],
    expires_at: u64,
    /// `false` while the address is only reserved for an outstanding offer.
    bound: bool,
    hostname: Option<Hostname>,
}

impl LeaseEntry {
    const fn to_lease(self) -> Lease {
        Lease {
            ip: self.ip,
            mac: self.mac,
            expires_at_ms: self.expires_at,
            bound: self.bound,
            hostname: self.hostname,
        }
    }
}

/// A client's host name (DHCP option 12), as sent by the client.
///
/// Names longer than [`MAX_HOSTNAME_LEN`] bytes are cut to that length (at a
/// character boundary). It is whatever the client claims, so do not trust it
/// for anything but display and logging.
#[derive(PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Hostname {
    bytes: [u8; MAX_HOSTNAME_LEN],
    len: u8,
}

impl Hostname {
    /// Parse the value of option 12: trailing NUL bytes are dropped, invalid
    /// UTF-8 ends the name, and an empty result is `None`.
    fn parse(value: &[u8]) -> Option<Self> {
        let value = &value[..value.len().min(MAX_HOSTNAME_LEN)];
        let valid = match core::str::from_utf8(value) {
            Ok(text) => text,
            // The prefix up to the error is valid UTF-8.
            Err(error) => core::str::from_utf8(&value[..error.valid_up_to()]).ok()?,
        };
        let name = valid.trim_end_matches('\0');
        if name.is_empty() {
            return None;
        }
        let mut bytes = [0; MAX_HOSTNAME_LEN];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        Some(Self {
            bytes,
            len: u8::try_from(name.len()).ok()?,
        })
    }

    /// The host name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // Only valid UTF-8 is ever stored.
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}

impl core::fmt::Debug for Hostname {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl core::fmt::Display for Hostname {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An additional address pool for clients on another subnet, reached through
/// a DHCP relay agent.
///
/// Requests forwarded by a relay carry the relay's address (`giaddr`). The
/// server picks the pool whose subnet contains that address and assigns from
/// it, with the pool's subnet mask and router. DNS servers, lease time and
/// domain name are shared with the main [`DhcpConfig`]. Requests from relays on
/// unknown subnets are ignored.
///
/// ```rust
/// use core::net::Ipv4Addr;
/// use leasehund::{DhcpServer, SubnetPool};
///
/// let mut server = DhcpServer::<32, 4>::new(
///     Ipv4Addr::new(192, 168, 1, 1),
///     Ipv4Addr::new(255, 255, 255, 0),
///     Ipv4Addr::new(192, 168, 1, 1),
///     Ipv4Addr::new(8, 8, 8, 8),
///     Ipv4Addr::new(192, 168, 1, 100),
///     Ipv4Addr::new(192, 168, 1, 200),
/// );
/// let sensors = SubnetPool::new(
///     Ipv4Addr::new(255, 255, 255, 0),
///     Ipv4Addr::new(192, 168, 2, 100),
///     Ipv4Addr::new(192, 168, 2, 200),
/// )
/// .with_router(Ipv4Addr::new(192, 168, 2, 1));
/// server.add_pool(sensors).unwrap();
/// ```
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SubnetPool {
    /// Subnet mask handed to the pool's clients.
    pub subnet_mask: Ipv4Addr,
    /// Default gateway handed to the pool's clients, usually the relay.
    pub router: Option<Ipv4Addr>,
    /// First address of the pool.
    pub ip_pool_start: Ipv4Addr,
    /// Last address of the pool.
    pub ip_pool_end: Ipv4Addr,
}

impl SubnetPool {
    /// A pool from `ip_pool_start` to `ip_pool_end` on the subnet given by
    /// `subnet_mask`, without a router.
    #[must_use]
    pub const fn new(
        subnet_mask: Ipv4Addr,
        ip_pool_start: Ipv4Addr,
        ip_pool_end: Ipv4Addr,
    ) -> Self {
        Self {
            subnet_mask,
            router: None,
            ip_pool_start,
            ip_pool_end,
        }
    }

    /// Sets the router handed to the pool's clients.
    #[must_use]
    pub const fn with_router(mut self, router: Ipv4Addr) -> Self {
        self.router = Some(router);
        self
    }
}

/// The subnet a request is served from: the main one of [`DhcpConfig`] or a
/// [`SubnetPool`].
#[derive(Clone, Copy, Debug)]
struct Subnet {
    network: u32,
    mask: u32,
    router: Option<Ipv4Addr>,
    start: u32,
    end: u32,
}

impl Subnet {
    const fn contains(&self, ip: Ipv4Addr) -> bool {
        ip.to_bits() & self.mask == self.network
    }

    const fn in_pool(&self, ip: Ipv4Addr) -> bool {
        let n = ip.to_bits();
        self.start <= n && n <= self.end
    }

    const fn broadcast(&self) -> Ipv4Addr {
        Ipv4Addr::from_bits(self.network | !self.mask)
    }

    /// /31 and /32 networks have no network or broadcast address.
    const fn is_network_or_broadcast(&self, ip: Ipv4Addr) -> bool {
        self.mask < 0xFFFF_FFFE
            && (ip.to_bits() == self.network || ip.to_bits() == self.broadcast().to_bits())
    }

    const fn overlaps(&self, other: &Self) -> bool {
        let common = self.mask & other.mask;
        self.network & common == other.network & common
    }
}

/// A lease as reported by [`DhcpServer::leases`].
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Lease {
    /// The leased address.
    pub ip: Ipv4Addr,
    /// The client's MAC address.
    pub mac: [u8; 6],
    /// When the lease (or offer reservation) expires, in milliseconds of
    /// `embassy_time::Instant`. `u64::MAX` for an infinite lease.
    pub expires_at_ms: u64,
    /// `true` once the client accepted the lease (`DHCPACK` sent), `false`
    /// while the address is only reserved for an offer.
    pub bound: bool,
    /// The host name the client sent (option 12), if any.
    pub hostname: Option<Hostname>,
}

/// Why [`DhcpServer::add_static_lease`] failed.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum StaticLeaseError {
    /// All `MAX_STATIC` static lease slots are in use.
    Full,
    /// The address is already reserved for a different MAC address.
    AddressTaken,
    /// The address is not inside the server's subnet or any [`SubnetPool`],
    /// so it could never be handed out.
    OutsideSubnet,
    /// The address is the server's own, the router's, or the network or
    /// broadcast address of the subnet.
    ReservedAddress,
}

/// Pre-allocated UDP socket buffers and metadata for the DHCP server.
pub struct DHCPServerBuffers {
    rx_buffer: [u8; SOCKET_BUFFER_SIZE],
    tx_buffer: [u8; SOCKET_BUFFER_SIZE],
    rx_meta: [PacketMetadata; 16],
    tx_meta: [PacketMetadata; 16],
}

impl DHCPServerBuffers {
    /// Creates a new set of DHCP server buffers.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use leasehund::DHCPServerBuffers;
    /// let buffers = DHCPServerBuffers::new();
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rx_buffer: [0; SOCKET_BUFFER_SIZE],
            tx_buffer: [0; SOCKET_BUFFER_SIZE],
            rx_meta: [PacketMetadata::EMPTY; 16],
            tx_meta: [PacketMetadata::EMPTY; 16],
        }
    }
}

impl Default for DHCPServerBuffers {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a DHCP lease or release event.
///
/// # Examples
///
/// ```rust
/// use leasehund::TransactionEvent;
/// use core::net::Ipv4Addr;
/// let event = TransactionEvent::Leased(Ipv4Addr::new(192, 168, 1, 100), [0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
/// ```
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TransactionEvent {
    /// A new lease was assigned.
    Leased(Ipv4Addr, [u8; 6]),
    /// A client released its IP.
    Released(Ipv4Addr, [u8; 6]),
}

/// DHCP message kinds reported to an admission policy.
///
/// Returning `false` for [`Discover`](Self::Discover), [`Request`](Self::Request) or
/// [`Inform`](Self::Inform) silently suppresses the corresponding response. A client's MAC
/// address is not an authentication mechanism and can be spoofed, so admission policies should
/// not be treated as a security boundary.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum AdmissionEvent {
    /// Client is asking for an offer.
    Discover,
    /// Client is asking for a lease acknowledgment.
    Request,
    /// Client is releasing an existing lease.
    ///
    /// Releases are always processed; the policy's return value is ignored. Completed releases
    /// are also reported through [`TransactionEvent::Released`].
    Release,
    /// Client already has an address and asks only for configuration (`DHCPINFORM`).
    Inform,
}

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

/// The fields of a client message the server acts on.
struct ClientMessage {
    packet: DhcpPacket,
    msg_type: u8,
    mac: [u8; 6],
    ciaddr: Option<Ipv4Addr>,
    giaddr: Option<Ipv4Addr>,
    requested_ip: Option<Ipv4Addr>,
    server_id: Option<Ipv4Addr>,
    hostname: Option<Hostname>,
}

/// `None` for the unspecified address `0.0.0.0`.
fn non_zero(octets: [u8; 4]) -> Option<Ipv4Addr> {
    let ip = Ipv4Addr::from(octets);
    (!ip.is_unspecified()).then_some(ip)
}

/// Wrapper around the Embassy UDP socket for DHCP server use.
pub struct DHCPServerSocket<'a> {
    socket: UdpSocket<'a>,
}

impl<'a> DHCPServerSocket<'a> {
    /// Creates a new DHCP server UDP socket bound to port 67.
    ///
    /// # Panics
    ///
    /// Panics if the socket binding fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use leasehund::{DHCPServerSocket, DHCPServerBuffers};
    /// # use embassy_net::Stack;
    /// # fn example(stack: Stack<'static>) {
    /// let mut buffers = DHCPServerBuffers::new();
    /// let socket = DHCPServerSocket::new(stack, &mut buffers);
    /// # }
    /// ```
    #[must_use]
    pub fn new(stack: Stack<'a>, buffers: &'a mut DHCPServerBuffers) -> Self {
        let mut socket = UdpSocket::new(
            stack,
            &mut buffers.rx_meta,
            &mut buffers.rx_buffer,
            &mut buffers.tx_meta,
            &mut buffers.tx_buffer,
        );
        socket.bind(DHCP_SERVER_PORT).unwrap();
        Self { socket }
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
    /// [`DhcpConfigBuilder`] with [`with_config`](Self::with_config).
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

    /// Removes all expired leases from the lease table.
    pub fn purge_expired_leases(&mut self) {
        self.purge_expired_at(embassy_time::Instant::now().as_millis());
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
    /// currently holds it.
    fn static_ip_for(&self, mac: [u8; 6], subnet: &Subnet) -> Option<Ipv4Addr> {
        let (_, ip) = self
            .static_leases
            .iter()
            .find(|(m, ip)| *m == mac && subnet.contains(*ip))?;
        self.leases
            .values()
            .all(|lease| lease.ip != *ip || lease.mac == mac)
            .then_some(*ip)
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
        if self
            .static_leases
            .iter()
            .any(|(m, static_ip)| *m == mac && subnet.contains(*static_ip))
        {
            return self.static_ip_for(mac, subnet) == Some(ip);
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

    /// Store a lease, returning `false` if the table is full.
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

    /// Parse and sanity-check a datagram received on the server port.
    fn parse_client_message(data: &[u8]) -> Option<ClientMessage> {
        if data.len() < FIXED_PART_SIZE {
            return None;
        }
        // SAFETY: read_unaligned handles the packed struct without alignment requirements.
        // We verified data.len() >= FIXED_PART_SIZE above.
        let packet = unsafe { core::ptr::read_unaligned(data.as_ptr().cast::<DhcpPacket>()) };
        if packet.magic != DHCP_MAGIC
            || packet.op != BOOTREQUEST
            || packet.htype != HTYPE_ETHERNET
            || packet.hlen != HLEN_ETHERNET
        {
            return None;
        }

        let options = &data[FIXED_PART_SIZE..];
        let mut mac = [0; 6];
        mac.copy_from_slice(&packet.chaddr[..6]);
        Some(ClientMessage {
            packet,
            msg_type: options::message_type(options)?,
            mac,
            ciaddr: non_zero(packet.ciaddr),
            giaddr: non_zero(packet.giaddr),
            requested_ip: options::find_ipv4(options, options::REQUESTED_IP),
            server_id: options::find_ipv4(options, options::SERVER_ID),
            hostname: options::find(options, options::HOST_NAME).and_then(Hostname::parse),
        })
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

        let Some(message) = Self::parse_client_message(data) else {
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

    /// Where a reply goes (RFC 2131 section 4.1): to the relay agent if the
    /// request came through one, to the client's own address if it has one,
    /// otherwise broadcast.
    const fn destination(message: &ClientMessage, msg_type: u8) -> (Ipv4Addr, u16) {
        match (message.giaddr, message.ciaddr) {
            (Some(relay), _) => (relay, DHCP_SERVER_PORT),
            (None, Some(ciaddr)) if msg_type != DHCP_NAK => (ciaddr, DHCP_CLIENT_PORT),
            _ => (Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT),
        }
    }

    /// Build a reply of `msg_type` to `message`, assigning `yiaddr`.
    fn reply(
        &self,
        message: &ClientMessage,
        msg_type: u8,
        yiaddr: Option<Ipv4Addr>,
        subnet: &Subnet,
    ) -> (Vec<u8, DHCP_PACKET_SIZE>, (Ipv4Addr, u16)) {
        let request = &message.packet;
        let mut flags = u16::from_be(request.flags);
        if msg_type == DHCP_NAK && message.giaddr.is_some() {
            // A relay must broadcast a NAK: the client's address is unusable.
            flags |= BROADCAST_FLAG;
        }
        let mut resp = DhcpPacket {
            op: BOOTREPLY,
            htype: HTYPE_ETHERNET,
            hlen: HLEN_ETHERNET,
            xid: request.xid,
            flags: flags.to_be(),
            giaddr: request.giaddr,
            magic: DHCP_MAGIC,
            ..Default::default()
        };
        if msg_type == DHCP_ACK {
            resp.ciaddr = request.ciaddr;
        }
        resp.yiaddr = yiaddr.unwrap_or(Ipv4Addr::UNSPECIFIED).octets();
        resp.chaddr = request.chaddr;

        let mut bytes = Vec::<u8, DHCP_PACKET_SIZE>::new();
        // Safe serialization of a packed struct: transmute to a byte array
        // avoids alignment issues that from_raw_parts would have.
        let resp_bytes: [u8; FIXED_PART_SIZE] = unsafe { core::mem::transmute(resp) };
        bytes.extend_from_slice(&resp_bytes).ok();
        self.add_options(&mut bytes, msg_type, yiaddr.is_some(), subnet);
        (bytes, Self::destination(message, msg_type))
    }

    /// Appends the options of a reply. `assigns` is `false` for a NAK and for
    /// the ACK to a `DHCPINFORM`, which carry no lease.
    fn add_options(
        &self,
        packet: &mut Vec<u8, DHCP_PACKET_SIZE>,
        msg_type: u8,
        assigns: bool,
        subnet: &Subnet,
    ) {
        // Each option is written whole or not at all, with room kept for END.
        let mut push = |code: u8, value: &[u8]| {
            let Ok(len) = u8::try_from(value.len()) else {
                return;
            };
            if packet.len() + 2 + value.len() < packet.capacity() {
                let _ = packet.extend_from_slice(&[code, len]);
                let _ = packet.extend_from_slice(value);
            }
        };

        push(options::MESSAGE_TYPE, &[msg_type]);
        push(options::SERVER_ID, &self.config.server_ip.octets());
        if msg_type != DHCP_NAK {
            if assigns {
                let lease = self.config.lease_time;
                push(options::LEASE_TIME, &lease.to_be_bytes());
                if lease != u32::MAX {
                    // T1 at 50% and T2 at 87.5% of the lease (RFC 2131 section 4.4.5).
                    push(options::RENEWAL_TIME, &(lease / 2).to_be_bytes());
                    push(options::REBINDING_TIME, &(lease - lease / 8).to_be_bytes());
                }
            }
            push(options::SUBNET_MASK, &subnet.mask.to_be_bytes());
            if let Some(router) = subnet.router {
                push(options::ROUTER, &router.octets());
            }
            push(options::BROADCAST_ADDRESS, &subnet.broadcast().octets());
            if !self.config.dns_servers.is_empty() {
                // An option holds at most 255 bytes: 63 addresses.
                let mut dns = Vec::<u8, 252>::new();
                for server in self.config.dns_servers.iter().take(63) {
                    let _ = dns.extend_from_slice(&server.octets());
                }
                push(options::DNS_SERVER, &dns);
            }
            if let Some(domain) = self.config.domain_name {
                push(options::DOMAIN_NAME, domain.as_bytes());
            }
        }
        let _ = packet.push(options::END);
    }

    /// Handles an incoming DHCP packet.
    #[allow(clippy::future_not_send)]
    async fn handle_packet_with_filter<P>(
        &mut self,
        socket: &DHCPServerSocket<'_>,
        data: &[u8],
        allow_client: &mut P,
    ) -> Option<(TransactionEvent, Lease)>
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
    {
        let action = self.prepare_packet_with_filter(
            data,
            allow_client,
            embassy_time::Instant::now().as_millis(),
        );
        if let Some((resp, endpoint)) = action.response {
            let _ = socket.socket.send_to(&resp, endpoint).await;
        }
        action.event
    }

    /// Processes packets until a single lease or release transaction completes.
    ///
    /// # Errors
    ///
    /// Returns [`RecvError`] if there was an error receiving a packet.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use leasehund::{DhcpServer, DHCPServerBuffers, DHCPServerSocket, TransactionEvent};
    /// # use embassy_net::Stack;
    /// use core::net::Ipv4Addr;
    ///
    /// # async fn example(stack: Stack<'static>) {
    /// let mut server = DhcpServer::<32, 4>::new(
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(255, 255, 255, 0),
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(8, 8, 8, 8),
    ///     Ipv4Addr::new(192, 168, 1, 100),
    ///     Ipv4Addr::new(192, 168, 1, 200),
    /// );
    /// let mut buffers = DHCPServerBuffers::new();
    /// let mut socket = DHCPServerSocket::new(stack, &mut buffers);
    /// let _event: Result<TransactionEvent, _> = server.lease_one(&mut socket).await;
    /// # }
    /// ```
    #[allow(clippy::future_not_send)]
    pub async fn lease_one(
        &mut self,
        socket: &mut DHCPServerSocket<'_>,
    ) -> Result<TransactionEvent, RecvError> {
        self.lease_one_with_filter(socket, |_, _| true).await
    }

    /// Processes packets until a single lease or release transaction completes, while applying
    /// a caller-provided admission policy before OFFER/ACK responses are sent.
    ///
    /// Returning `false` for a discover or request silently drops that packet without sending an
    /// OFFER, ACK, or NAK. Releases are always processed even if the policy returns `false`.
    ///
    /// # Errors
    ///
    /// Returns [`RecvError`] if there was an error receiving a packet.
    #[allow(clippy::future_not_send)]
    pub async fn lease_one_with_filter<P>(
        &mut self,
        socket: &mut DHCPServerSocket<'_>,
        allow_client: P,
    ) -> Result<TransactionEvent, RecvError>
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
    {
        self.lease_one_with_lease(socket, allow_client)
            .await
            .map(|(event, _)| event)
    }

    /// Like [`lease_one_with_filter`](Self::lease_one_with_filter), also
    /// returning the [`Lease`] the transaction concerns: the new lease for
    /// [`TransactionEvent::Leased`], the removed one for
    /// [`TransactionEvent::Released`]. It carries details such as the client's
    /// host name.
    ///
    /// # Errors
    ///
    /// Returns [`RecvError`] if there was an error receiving a packet.
    #[allow(clippy::future_not_send)]
    pub async fn lease_one_with_lease<P>(
        &mut self,
        socket: &mut DHCPServerSocket<'_>,
        mut allow_client: P,
    ) -> Result<(TransactionEvent, Lease), RecvError>
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
    {
        loop {
            // As large as the socket buffer, so no datagram is ever truncated.
            let mut buf = [0u8; SOCKET_BUFFER_SIZE];
            let (len, _) = socket.socket.recv_from(&mut buf).await?;
            let event = self
                .handle_packet_with_filter(socket, &buf[..len], &mut allow_client)
                .await;
            socket.socket.flush().await;
            if let Some(event) = event {
                return Ok(event);
            }
        }
    }

    /// Runs the DHCP server forever on the provided network stack.
    ///
    /// # Panics
    ///
    /// Panics if the UDP socket cannot bind to port 67.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use embassy_net::Stack;
    /// use leasehund::DhcpServer;
    /// use core::net::Ipv4Addr;
    ///
    /// # async fn example(stack: Stack<'static>) {
    /// let mut server = DhcpServer::<32, 4>::new(
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(255, 255, 255, 0),
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(8, 8, 8, 8),
    ///     Ipv4Addr::new(192, 168, 1, 100),
    ///     Ipv4Addr::new(192, 168, 1, 200),
    /// );
    ///
    /// server.run(stack).await;
    /// # }
    /// ```
    #[allow(clippy::future_not_send)]
    pub async fn run(&mut self, stack: Stack<'_>) -> ! {
        self.run_with_filter_and_callback(stack, |_, _| true, |_| {})
            .await
    }

    /// Runs the DHCP server forever, invoking `callback` for every lease event.
    ///
    /// This is identical to [`run`](Self::run) except that `TransactionEvent`s
    /// are forwarded to the caller via `callback` instead of being discarded.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use embassy_net::Stack;
    /// use leasehund::{DhcpServer, TransactionEvent};
    /// use core::net::Ipv4Addr;
    ///
    /// # async fn example(stack: Stack<'static>) {
    /// let mut server = DhcpServer::<32, 4>::new(
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(255, 255, 255, 0),
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(8, 8, 8, 8),
    ///     Ipv4Addr::new(192, 168, 1, 100),
    ///     Ipv4Addr::new(192, 168, 1, 200),
    /// );
    ///
    /// server.run_with_callback(stack, |event| {
    ///     match event {
    ///         TransactionEvent::Leased(ip, mac) => {
    ///             // log or react to new lease
    ///         }
    ///         TransactionEvent::Released(ip, mac) => {
    ///             // log or react to release
    ///         }
    ///     }
    /// }).await;
    /// # }
    /// ```
    #[allow(clippy::future_not_send)]
    pub async fn run_with_callback<F>(&mut self, stack: Stack<'_>, callback: F) -> !
    where
        F: FnMut(TransactionEvent),
    {
        self.run_with_filter_and_callback(stack, |_, _| true, callback)
            .await
    }

    /// Runs the DHCP server forever, applying a caller-provided admission policy before OFFER/ACK
    /// responses are sent and invoking `callback` for every lease event that completes.
    ///
    /// Returning `false` for a discover or request silently drops that packet without sending an
    /// OFFER, ACK, or NAK. Releases are always processed even if the policy returns `false`.
    #[allow(clippy::future_not_send)]
    pub async fn run_with_filter_and_callback<P, F>(
        &mut self,
        stack: Stack<'_>,
        allow_client: P,
        mut callback: F,
    ) -> !
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
        F: FnMut(TransactionEvent),
    {
        self.run_with_lease_callback(stack, allow_client, |event, _| callback(event))
            .await
    }

    /// Like [`run_with_filter_and_callback`](Self::run_with_filter_and_callback),
    /// also passing the [`Lease`] each event concerns: the new lease for
    /// [`TransactionEvent::Leased`], the removed one for
    /// [`TransactionEvent::Released`].
    ///
    /// ```rust,no_run
    /// use core::net::Ipv4Addr;
    /// use leasehund::{DhcpServer, TransactionEvent};
    /// # use embassy_net::Stack;
    ///
    /// # async fn example(stack: Stack<'static>, mut server: DhcpServer) {
    /// server
    ///     .run_with_lease_callback(stack, |_, _| true, |event, lease| {
    ///         if let (TransactionEvent::Leased(ip, _), Some(name)) = (event, lease.hostname) {
    ///             // for example: show "printer.lan -> 192.168.1.50" on a display
    ///             let _ = (ip, name.as_str());
    ///         }
    ///     })
    ///     .await;
    /// # }
    /// ```
    #[allow(clippy::future_not_send)]
    pub async fn run_with_lease_callback<P, F>(
        &mut self,
        stack: Stack<'_>,
        mut allow_client: P,
        mut callback: F,
    ) -> !
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
        F: FnMut(TransactionEvent, &Lease),
    {
        let mut buffers = DHCPServerBuffers::new();
        let mut socket = DHCPServerSocket::new(stack, &mut buffers);
        loop {
            let mut buf = [0u8; SOCKET_BUFFER_SIZE];
            match socket.socket.recv_from(&mut buf).await {
                Ok((len, _)) => {
                    if let Some((event, lease)) = self
                        .handle_packet_with_filter(&socket, &buf[..len], &mut allow_client)
                        .await
                    {
                        callback(event, &lease);
                    }
                    socket.socket.flush().await;
                }
                Err(_) => Timer::after(Duration::from_millis(100)).await,
            }
        }
    }
}

#[cfg(test)]
mod tests;
