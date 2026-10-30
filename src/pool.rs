//! Address pools: the main subnet's and those behind relay agents.

use core::net::Ipv4Addr;

/// An additional address pool for clients on another subnet, reached through
/// a DHCP relay agent.
///
/// Requests forwarded by a relay carry the relay's address (`giaddr`). The
/// server picks the pool whose subnet contains that address and assigns from
/// it, with the pool's subnet mask and router. DNS servers, lease time and
/// domain name are shared with the main [`DhcpConfig`](leasehund::DhcpConfig). Requests from relays on
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

/// The subnet a request is served from: the main one of [`DhcpConfig`](leasehund::DhcpConfig) or a
/// [`SubnetPool`].
#[derive(Clone, Copy, Debug)]
pub struct Subnet {
    pub network: u32,
    pub mask: u32,
    pub router: Option<Ipv4Addr>,
    pub start: u32,
    pub end: u32,
}

impl Subnet {
    pub const fn contains(&self, ip: Ipv4Addr) -> bool {
        ip.to_bits() & self.mask == self.network
    }

    pub const fn in_pool(&self, ip: Ipv4Addr) -> bool {
        let n = ip.to_bits();
        self.start <= n && n <= self.end
    }

    pub const fn broadcast(&self) -> Ipv4Addr {
        Ipv4Addr::from_bits(self.network | !self.mask)
    }

    /// /31 and /32 networks have no network or broadcast address.
    pub const fn is_network_or_broadcast(&self, ip: Ipv4Addr) -> bool {
        self.mask < 0xFFFF_FFFE
            && (ip.to_bits() == self.network || ip.to_bits() == self.broadcast().to_bits())
    }

    pub const fn overlaps(&self, other: &Self) -> bool {
        let common = self.mask & other.mask;
        self.network & common == other.network & common
    }
}
