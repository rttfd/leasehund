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
//! to port 68 (standard DHCP client port). All responses are sent as broadcast packets
//! to ensure compatibility with clients that don't yet have an IP address.
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
use hash32::{BuildHasherDefault, FnvHasher};
use heapless::{IndexMap, Vec};
use smoltcp::phy::PacketMeta;

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

/// Duration in milliseconds to reserve an offered IP before the client confirms.
const OFFER_RESERVATION_MS: u64 = 60_000;

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
    /// Lease time in seconds (default: 24 hours)
    pub lease_time: u32,
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
        }
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

    /// Builds the final configuration.
    #[must_use]
    pub fn build(self) -> DhcpConfig<MAX_DNS> {
        self.config
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
const DHCP_ACK: u8 = 5;
const DHCP_RELEASE: u8 = 7;

// DHCP Options (RFC 2132)
const OPTION_SUBNET_MASK: u8 = 1;
const OPTION_ROUTER: u8 = 3;
const OPTION_DNS_SERVER: u8 = 6;
const OPTION_LEASE_TIME: u8 = 51;
const OPTION_MESSAGE_TYPE: u8 = 53;
const OPTION_SERVER_ID: u8 = 54;
const OPTION_END: u8 = 255;

// The standard DHCP magic cookie (0x63825363).
// Required by RFC 2132 section 2.
const DHCP_MAGIC: [u8; 4] = [0x63, 0x82, 0x53, 0x63];

/// DHCP packet structure as defined in [RFC 2131](https://www.rfc-editor.org/rfc/rfc2131).
///
/// Packed to match the wire format exactly.
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

/// DHCP message kinds that can be admitted or denied by policy.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AdmissionEvent {
    /// Client is asking for an offer.
    Discover,
    /// Client is asking for a lease acknowledgment.
    Request,
    /// Client is releasing an existing lease.
    Release,
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
/// - `MAX_CLIENTS`: Maximum number of concurrent leases (compile-time fixed)
/// - `MAX_DNS`: Maximum number of DNS servers in configuration
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
> {
    config: DhcpConfig<MAX_DNS>,
    leases: IndexMap<[u8; 6], LeaseEntry, BuildHasherDefault<FnvHasher>, MAX_CLIENTS>,
}

impl<const MAX_CLIENTS: usize, const MAX_DNS: usize> DhcpServer<MAX_CLIENTS, MAX_DNS> {
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
        };
        Self {
            config,
            leases: IndexMap::new(),
        }
    }

    /// Creates a new DHCP server from a [`DhcpConfig`].
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
            leases: IndexMap::new(),
        }
    }

    /// Gets a reference to the current configuration.
    #[must_use]
    pub const fn config(&self) -> &DhcpConfig<MAX_DNS> {
        &self.config
    }

    /// Gets the current number of active leases.
    #[must_use]
    pub fn lease_count(&self) -> usize {
        self.leases.len()
    }

    /// Removes all expired leases from the lease table.
    pub fn purge_expired_leases(&mut self) {
        let now = embassy_time::Instant::now().as_millis();
        let mut expired: Vec<[u8; 6], MAX_CLIENTS> = Vec::new();
        for (mac, entry) in &self.leases {
            if entry.expires_at <= now {
                let _ = expired.push(*mac);
            }
        }
        for mac in &expired {
            self.leases.remove(mac);
        }
    }

    /// Checks if the IP pool is full.
    ///
    /// Call [`purge_expired_leases`](Self::purge_expired_leases) first
    /// to reclaim expired entries if desired.
    #[must_use]
    pub fn is_pool_full(&self) -> bool {
        let pool_size =
            u32::from(self.config.ip_pool_end) - u32::from(self.config.ip_pool_start) + 1;
        self.leases.len() >= (pool_size as usize).min(MAX_CLIENTS)
    }

    /// Finds the next available IP address in the configured pool.
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
    /// };
    /// let server: DhcpServer<32, 4> = DhcpServer::with_config(config);
    /// let next = server.get_next_available_ip();
    /// assert!(matches!(next, Some(ip) if ip == Ipv4Addr::new(10, 0, 0, 100)));
    /// ```
    pub fn get_next_available_ip(&self) -> Option<Ipv4Addr> {
        let start = u32::from(self.config.ip_pool_start);
        let end = u32::from(self.config.ip_pool_end);
        (start..=end)
            .map(Ipv4Addr::from)
            .find(|ip| !self.leases.values().any(|lease| lease.ip == *ip))
    }

    /// Parses the DHCP message type from the options field.
    fn parse_message_type(options: &[u8]) -> Option<u8> {
        let mut i = 0;
        while i < options.len() {
            match options[i] {
                OPTION_END => break,
                OPTION_MESSAGE_TYPE if i + 2 < options.len() => return Some(options[i + 2]),
                _ => {
                    if i + 1 < options.len() {
                        i += options[i + 1] as usize + 2;
                    } else {
                        break;
                    }
                }
            }
        }
        None
    }

    /// Appends standard DHCP options to a response packet.
    fn add_options(&self, packet: &mut Vec<u8, DHCP_PACKET_SIZE>, msg_type: u8) {
        packet
            .extend_from_slice(&[OPTION_MESSAGE_TYPE, 1, msg_type])
            .ok();
        packet.extend_from_slice(&[OPTION_SERVER_ID, 4]).ok();
        packet
            .extend_from_slice(&self.config.server_ip.octets())
            .ok();
        packet.extend_from_slice(&[OPTION_SUBNET_MASK, 4]).ok();
        packet
            .extend_from_slice(&self.config.subnet_mask.octets())
            .ok();

        if let Some(router) = self.config.router {
            packet.extend_from_slice(&[OPTION_ROUTER, 4]).ok();
            packet.extend_from_slice(&router.octets()).ok();
        }

        if !self.config.dns_servers.is_empty() {
            let dns_len = self.config.dns_servers.len() * 4;
            let dns_len_u8 = u8::try_from(dns_len).unwrap_or_default();
            packet
                .extend_from_slice(&[OPTION_DNS_SERVER, dns_len_u8])
                .ok();
            for dns in &self.config.dns_servers {
                packet.extend_from_slice(&dns.octets()).ok();
            }
        }

        packet.extend_from_slice(&[OPTION_LEASE_TIME, 4]).ok();
        packet
            .extend_from_slice(&self.config.lease_time.to_be_bytes())
            .ok();
        packet.extend_from_slice(&[OPTION_END]).ok();
    }

    /// Creates a DHCP response packet.
    fn make_response(&mut self, req: &DhcpPacket, msg_type: u8) -> Vec<u8, DHCP_PACKET_SIZE> {
        let mut resp = DhcpPacket {
            op: 2, // BOOTREPLY
            xid: req.xid,
            htype: 1,
            hlen: 6,
            magic: DHCP_MAGIC,
            ..Default::default()
        };
        resp.chaddr[..6].copy_from_slice(&req.chaddr[..6]);
        let mac = req.chaddr[..6].try_into().unwrap_or([0; 6]);

        match msg_type {
            DHCP_OFFER => {
                if let Some(ip) = self.get_next_available_ip() {
                    resp.yiaddr = ip.octets();
                    // Reserve with a short TTL to prevent duplicate offers.
                    let reservation = LeaseEntry {
                        ip,
                        mac,
                        expires_at: embassy_time::Instant::now().as_millis() + OFFER_RESERVATION_MS,
                    };
                    let _ = self.leases.insert(mac, reservation);
                }
            }
            DHCP_ACK => {
                if let Some(lease) = self.leases.get(&mac) {
                    resp.yiaddr = lease.ip.octets();
                } else if let Some(ip) = self.get_next_available_ip() {
                    resp.yiaddr = ip.octets();
                }
                // (Re-)insert with the full lease duration.
                let ip = Ipv4Addr::from(resp.yiaddr);
                let lease = LeaseEntry {
                    ip,
                    mac,
                    expires_at: embassy_time::Instant::now().as_millis()
                        + (u64::from(self.config.lease_time) * 1000),
                };
                let _ = self.leases.insert(mac, lease);
            }
            _ => {}
        }

        let mut bytes = Vec::<u8, DHCP_PACKET_SIZE>::new();
        // Safe serialization of a packed struct: transmute to a byte array
        // avoids alignment issues that from_raw_parts would have.
        let resp_bytes: [u8; FIXED_PART_SIZE] = unsafe { core::mem::transmute(resp) };
        bytes.extend_from_slice(&resp_bytes).ok();
        self.add_options(&mut bytes, msg_type);
        bytes
    }

    /// Handles an incoming DHCP packet.
    #[allow(clippy::future_not_send)]
    async fn handle_packet_with_filter<P>(
        &mut self,
        socket: &DHCPServerSocket<'_>,
        data: &[u8],
        allow_client: &mut P,
    ) -> Option<TransactionEvent>
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
    {
        // Reclaim expired leases before processing.
        self.purge_expired_leases();

        if data.len() < FIXED_PART_SIZE {
            return None;
        }

        // SAFETY: read_unaligned handles the packed struct without alignment requirements.
        // We verified data.len() >= FIXED_PART_SIZE above.
        let packet = unsafe { core::ptr::read_unaligned(data.as_ptr().cast::<DhcpPacket>()) };
        if packet.magic != DHCP_MAGIC {
            return None;
        }

        let options = &data[FIXED_PART_SIZE..];
        let msg_type = Self::parse_message_type(options)?;
        let mac: [u8; 6] = packet.chaddr[..6].try_into().unwrap_or([0; 6]);

        // Consolidate DISCOVER/REQUEST into a single .await point.
        let (resp, event) = match msg_type {
            DHCP_DISCOVER => {
                if !allow_client(mac, AdmissionEvent::Discover) {
                    return None;
                }
                (Some(self.make_response(&packet, DHCP_OFFER)), None)
            }
            DHCP_REQUEST => {
                if !allow_client(mac, AdmissionEvent::Request) {
                    return None;
                }
                let resp = self.make_response(&packet, DHCP_ACK);
                let event = self
                    .leases
                    .get(&mac)
                    .map(|entry| TransactionEvent::Leased(entry.ip, mac));
                (Some(resp), event)
            }
            DHCP_RELEASE => {
                let _ = allow_client(mac, AdmissionEvent::Release);
                let entry = self.leases.remove(&mac);
                return entry.map(|e| TransactionEvent::Released(e.ip, e.mac));
            }
            _ => return None,
        };

        if let Some(resp) = resp {
            let meta = embassy_net::udp::UdpMetadata {
                endpoint: (Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT).into(),
                local_address: None,
                meta: PacketMeta::default(),
            };
            let _ = socket.socket.send_to(&resp, meta).await;
        }

        event
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
    #[allow(clippy::future_not_send)]
    pub async fn lease_one_with_filter<P>(
        &mut self,
        socket: &mut DHCPServerSocket<'_>,
        mut allow_client: P,
    ) -> Result<TransactionEvent, RecvError>
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
    {
        loop {
            let mut buf = [0u8; DHCP_PACKET_SIZE];
            match socket.socket.recv_from(&mut buf).await {
                Ok((len, _)) => {
                    if let Some(event) = self
                        .handle_packet_with_filter(socket, &buf[..len], &mut allow_client)
                        .await
                    {
                        socket.socket.flush().await;
                        return Ok(event);
                    }
                }
                Err(e) => return Err(e),
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
    #[allow(clippy::future_not_send)]
    pub async fn run_with_filter_and_callback<P, F>(
        &mut self,
        stack: Stack<'_>,
        mut allow_client: P,
        mut callback: F,
    ) -> !
    where
        P: FnMut([u8; 6], AdmissionEvent) -> bool,
        F: FnMut(TransactionEvent),
    {
        let mut buffers = DHCPServerBuffers::new();
        let mut socket = DHCPServerSocket::new(stack, &mut buffers);
        loop {
            let mut buf = [0u8; DHCP_PACKET_SIZE];
            match socket.socket.recv_from(&mut buf).await {
                Ok((len, _)) => {
                    if let Some(event) = self
                        .handle_packet_with_filter(&socket, &buf[..len], &mut allow_client)
                        .await
                    {
                        callback(event);
                    }
                    socket.socket.flush().await;
                }
                Err(_) => Timer::after(Duration::from_millis(100)).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use core::net::Ipv4Addr;

    type TestServer = super::DhcpServer<2, 2>;
    type TestConfig = super::DhcpConfig<2>;
    type TestBuilder = super::DhcpConfigBuilder<2>;

    #[test]
    fn config_builder_basic() {
        let config = TestBuilder::new()
            .server_ip(Ipv4Addr::new(10, 0, 0, 1))
            .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
            .router(Ipv4Addr::new(10, 0, 0, 254))
            .add_dns_server(Ipv4Addr::new(8, 8, 8, 8))
            .ip_pool(Ipv4Addr::new(10, 0, 0, 100), Ipv4Addr::new(10, 0, 0, 200))
            .lease_time(3600)
            .build();
        assert_eq!(config.server_ip, Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(config.subnet_mask, Ipv4Addr::new(255, 255, 255, 0));
        assert_eq!(config.router, Some(Ipv4Addr::new(10, 0, 0, 254)));
        assert_eq!(config.dns_servers.len(), 1);
        assert_eq!(config.dns_servers[0], Ipv4Addr::new(8, 8, 8, 8));
        assert_eq!(config.ip_pool_start, Ipv4Addr::new(10, 0, 0, 100));
        assert_eq!(config.ip_pool_end, Ipv4Addr::new(10, 0, 0, 200));
        assert_eq!(config.lease_time, 3600);
    }

    #[test]
    fn config_builder_starts_with_no_dns() {
        let config = TestBuilder::new().build();
        assert!(config.dns_servers.is_empty());
    }

    #[test]
    fn config_builder_no_router() {
        let config = TestBuilder::new().no_router().build();
        assert_eq!(config.router, None);
    }

    #[test]
    fn server_new() {
        let server = TestServer::new(
            Ipv4Addr::new(192, 168, 1, 1),
            Ipv4Addr::new(255, 255, 255, 0),
            Ipv4Addr::new(192, 168, 1, 1),
            Ipv4Addr::new(8, 8, 4, 4),
            Ipv4Addr::new(192, 168, 1, 100),
            Ipv4Addr::new(192, 168, 1, 200),
        );
        let config = server.config();
        assert_eq!(config.server_ip, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(config.dns_servers.len(), 1);
        assert_eq!(config.dns_servers[0], Ipv4Addr::new(8, 8, 4, 4));
    }

    #[test]
    fn ip_pool_full() {
        let mut server = TestServer::new(
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(255, 255, 255, 0),
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(1, 1, 1, 1),
            Ipv4Addr::new(10, 0, 0, 100),
            Ipv4Addr::new(10, 0, 0, 101),
        );
        for i in 0..2 {
            let mac = [0, 0, 0, 0, 0, i];
            let lease = super::LeaseEntry {
                ip: Ipv4Addr::new(10, 0, 0, 100 + i),
                mac,
                expires_at: u64::MAX,
            };
            let _ = server.leases.insert(mac, lease);
        }
        assert!(server.is_pool_full());
    }

    #[test]
    fn config_default_values() {
        let config = TestConfig::default();
        assert_eq!(config.server_ip, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(config.subnet_mask, Ipv4Addr::new(255, 255, 255, 0));
        assert_eq!(config.router, Some(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(config.dns_servers.len(), 1);
        assert_eq!(config.dns_servers[0], Ipv4Addr::new(8, 8, 8, 8));
        assert_eq!(config.ip_pool_start, Ipv4Addr::new(192, 168, 1, 100));
        assert_eq!(config.ip_pool_end, Ipv4Addr::new(192, 168, 1, 200));
        assert_eq!(config.lease_time, super::DEFAULT_LEASE_TIME);
    }

    #[test]
    fn get_next_available_ip_empty() {
        let server = TestServer::new(
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(255, 255, 255, 0),
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(1, 1, 1, 1),
            Ipv4Addr::new(10, 0, 0, 100),
            Ipv4Addr::new(10, 0, 0, 101),
        );
        assert_eq!(
            server.get_next_available_ip(),
            Some(Ipv4Addr::new(10, 0, 0, 100))
        );
    }

    #[test]
    fn get_next_available_ip_skips_leased() {
        let mut server = TestServer::new(
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(255, 255, 255, 0),
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(1, 1, 1, 1),
            Ipv4Addr::new(10, 0, 0, 100),
            Ipv4Addr::new(10, 0, 0, 101),
        );
        let mac = [0, 0, 0, 0, 0, 1];
        let lease = super::LeaseEntry {
            ip: Ipv4Addr::new(10, 0, 0, 100),
            mac,
            expires_at: u64::MAX,
        };
        let _ = server.leases.insert(mac, lease);
        assert_eq!(
            server.get_next_available_ip(),
            Some(Ipv4Addr::new(10, 0, 0, 101))
        );
    }

    #[test]
    fn parse_message_type_discover() {
        let options = [
            super::OPTION_MESSAGE_TYPE,
            1,
            super::DHCP_DISCOVER,
            super::OPTION_END,
        ];
        assert_eq!(
            TestServer::parse_message_type(&options),
            Some(super::DHCP_DISCOVER)
        );
    }

    #[test]
    fn parse_message_type_empty() {
        assert_eq!(TestServer::parse_message_type(&[]), None);
    }

    #[test]
    fn parse_message_type_end_only() {
        assert_eq!(TestServer::parse_message_type(&[super::OPTION_END]), None);
    }
}
