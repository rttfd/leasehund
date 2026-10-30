//! Server configuration and its validation.

use core::net::Ipv4Addr;

/// Default number of DNS servers a configuration holds.
pub const DEFAULT_MAX_DNS_SERVERS: usize = 4;
/// Default lease time: 24 hours, in seconds.
pub const DEFAULT_LEASE_TIME: u32 = 86400;

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
    /// A [`SubnetPool`](leasehund::SubnetPool) overlaps the server's subnet or another pool.
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
