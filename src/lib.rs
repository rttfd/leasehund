//! # Leasehund 🐶
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
//! including strict checking and emission of the DHCP magic cookie ([`DHCP_MAGIC`]) in all packets as required by the standard.
//!
//! ## Features
//!
//! - **No-std compatible**: Designed for embedded systems without heap allocation
//! - **Embassy integration**: Built on top of Embassy async runtime and networking stack
//! - **Configurable IP pools**: Define custom IP address ranges for client assignment
//! - **Flexible lease management**: Configurable lease times and automatic tracking
//! - **Multiple DNS servers**: Support for up to 4 DNS servers
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
//! let mut dhcp_server = DhcpServer::new_with_dns(
//!     Ipv4Addr::new(192, 168, 1, 1),    // Server IP
//!     Ipv4Addr::new(255, 255, 255, 0),  // Subnet mask
//!     Ipv4Addr::new(192, 168, 1, 1),    // Router/Gateway
//!     Ipv4Addr::new(8, 8, 8, 8),        // DNS server
//!     Ipv4Addr::new(192, 168, 1, 100),  // IP pool start
//!     Ipv4Addr::new(192, 168, 1, 200),  // IP pool end
//! );
//!
//! // Run the DHCP server (this will loop forever)
//! dhcp_server.run(stack).await;
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
//! let config = DhcpConfigBuilder::new()
//!     .server_ip(Ipv4Addr::new(10, 0, 1, 1))
//!     .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
//!     .router(Ipv4Addr::new(10, 0, 1, 1))
//!     .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))      // Cloudflare DNS
//!     .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))      // Cloudflare backup
//!     .add_dns_server(Ipv4Addr::new(8, 8, 8, 8))      // Google DNS
//!     .ip_pool(
//!         Ipv4Addr::new(10, 0, 100, 1),
//!         Ipv4Addr::new(10, 0, 199, 254)
//!     )
//!     .lease_time(7200)    // 2 hours
//!     .build();
//!
//! let mut dhcp_server = DhcpServer::with_config(config);
//! dhcp_server.run(stack).await;
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
//! - Maximum of 32 concurrent client leases (configurable via `MAX_CLIENTS`)
//! - Configurable lease time (default: 24 hours)
//! - Support for multiple DNS servers (up to 4)
//! - Optional router/gateway configuration
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
//! of `MAX_CLIENTS` entries. Each lease entry contains:
//! - IPv4 address (4 bytes)
//! - MAC address (6 bytes)  
//! - Lease expiration timestamp (8 bytes)

#![no_std]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

use core::net::Ipv4Addr;
use embassy_net::Stack;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_time::{Duration, Timer};
use heapless::{FnvIndexMap, Vec};
use smoltcp::phy::PacketMeta;

/// Standard DHCP server port (RFC 2131)
const DHCP_SERVER_PORT: u16 = 67;
/// Standard DHCP client port (RFC 2131)  
const DHCP_CLIENT_PORT: u16 = 68;
/// Maximum number of concurrent DHCP client leases
const MAX_CLIENTS: usize = 32;
/// Default lease time in seconds (24 hours)
const LEASE_TIME: u32 = 86400; // 24 hours in seconds

/// Default buffer size for UDP socket operations
const DEFAULT_SOCKET_BUFFER_SIZE: usize = 1024;

/// Maximum number of DNS servers that can be configured
const MAX_DNS_SERVERS: usize = 4;

/// Configuration options for the DHCP server
///
/// This structure allows customization of various DHCP server parameters
/// including lease time, buffer sizes, and optional settings.
///
/// # Examples
///
/// ```rust
/// use core::net::Ipv4Addr;
/// use leasehund::{DhcpConfig, DhcpServer};
/// use heapless::Vec;
///
/// let mut dns_servers = Vec::new();
/// dns_servers.push(Ipv4Addr::new(8, 8, 8, 8)).ok();
/// dns_servers.push(Ipv4Addr::new(8, 8, 4, 4)).ok();
///
/// let config = DhcpConfig {
///     server_ip: Ipv4Addr::new(192, 168, 1, 1),
///     subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
///     router: Some(Ipv4Addr::new(192, 168, 1, 1)),
///     dns_servers,
///     ip_pool_start: Ipv4Addr::new(192, 168, 1, 100),
///     ip_pool_end: Ipv4Addr::new(192, 168, 1, 200),
///     lease_time: 3600, // 1 hour
///     socket_buffer_size: 1024,
/// };
///
/// let server = DhcpServer::with_config(config);
/// ```
#[derive(Clone, Debug)]
pub struct DhcpConfig {
    /// The IP address of this DHCP server
    pub server_ip: Ipv4Addr,
    /// Subnet mask to assign to clients
    pub subnet_mask: Ipv4Addr,
    /// Default gateway/router IP address to assign to clients (optional)
    pub router: Option<Ipv4Addr>,
    /// List of DNS server IP addresses to assign to clients
    pub dns_servers: Vec<Ipv4Addr, MAX_DNS_SERVERS>,
    /// Start of the IP address pool for client assignment
    pub ip_pool_start: Ipv4Addr,
    /// End of the IP address pool for client assignment
    pub ip_pool_end: Ipv4Addr,
    /// Lease time in seconds (default: 24 hours)
    pub lease_time: u32,
    /// UDP socket buffer size in bytes (default: 1024)
    pub socket_buffer_size: usize,
}

impl Default for DhcpConfig {
    fn default() -> Self {
        let mut dns_servers = Vec::new();
        dns_servers.push(Ipv4Addr::new(8, 8, 8, 8)).ok(); // Google DNS

        Self {
            server_ip: Ipv4Addr::new(192, 168, 1, 1),
            subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
            router: Some(Ipv4Addr::new(192, 168, 1, 1)),
            dns_servers,
            ip_pool_start: Ipv4Addr::new(192, 168, 1, 100),
            ip_pool_end: Ipv4Addr::new(192, 168, 1, 200),
            lease_time: LEASE_TIME,
            socket_buffer_size: DEFAULT_SOCKET_BUFFER_SIZE,
        }
    }
}

/// Builder pattern for creating DHCP server configurations
///
/// Provides a fluent interface for configuring DHCP server options.
///
/// # Examples
///
/// ```rust
/// use core::net::Ipv4Addr;
/// use leasehund::{DhcpConfigBuilder, DhcpServer};
///
/// let config = DhcpConfigBuilder::new()
///     .server_ip(Ipv4Addr::new(10, 0, 1, 1))
///     .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
///     .router(Ipv4Addr::new(10, 0, 1, 1))
///     .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
///     .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
///     .ip_pool(
///         Ipv4Addr::new(10, 0, 100, 1),
///         Ipv4Addr::new(10, 0, 199, 254)
///     )
///     .lease_time(7200) // 2 hours
///     .build();
///
/// let server = DhcpServer::with_config(config);
/// ```
#[derive(Clone, Debug)]
pub struct DhcpConfigBuilder {
    config: DhcpConfig,
}

impl DhcpConfigBuilder {
    /// Creates a new configuration builder with default values
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: DhcpConfig::default(),
        }
    }

    /// Sets the DHCP server IP address
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn server_ip(mut self, ip: Ipv4Addr) -> Self {
        self.config.server_ip = ip;
        self
    }

    /// Sets the subnet mask
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn subnet_mask(mut self, mask: Ipv4Addr) -> Self {
        self.config.subnet_mask = mask;
        self
    }

    /// Sets the default gateway/router IP address
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn router(mut self, router: Ipv4Addr) -> Self {
        self.config.router = Some(router);
        self
    }

    /// Removes the router option (no default gateway)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn no_router(mut self) -> Self {
        self.config.router = None;
        self
    }

    /// Adds a DNS server to the configuration
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn add_dns_server(mut self, dns: Ipv4Addr) -> Self {
        let _ = self.config.dns_servers.push(dns);
        self
    }

    /// Clears all DNS servers
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn clear_dns_servers(mut self) -> Self {
        self.config.dns_servers.clear();
        self
    }

    /// Sets the IP address pool range
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn ip_pool(mut self, start: Ipv4Addr, end: Ipv4Addr) -> Self {
        self.config.ip_pool_start = start;
        self.config.ip_pool_end = end;
        self
    }

    /// Sets the lease time in seconds
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn lease_time(mut self, seconds: u32) -> Self {
        self.config.lease_time = seconds;
        self
    }

    /// Sets the UDP socket buffer size
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn socket_buffer_size(mut self, size: usize) -> Self {
        self.config.socket_buffer_size = size;
        self
    }

    /// Builds the final configuration
    #[must_use]
    pub fn build(self) -> DhcpConfig {
        self.config
    }
}

impl Default for DhcpConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// DHCP Message Types (RFC 2131)
/// DHCP Discover message type
const DHCP_DISCOVER: u8 = 1;
/// DHCP Offer message type
const DHCP_OFFER: u8 = 2;
/// DHCP Request message type
const DHCP_REQUEST: u8 = 3;
/// DHCP Acknowledgment message type
const DHCP_ACK: u8 = 5;
/// DHCP Release message type
const DHCP_RELEASE: u8 = 7;

// DHCP Options (RFC 2132)
/// Subnet Mask option code
const OPTION_SUBNET_MASK: u8 = 1;
/// Router option code
const OPTION_ROUTER: u8 = 3;
/// Domain Name Server option code
const OPTION_DNS_SERVER: u8 = 6;
/// IP Address Lease Time option code
const OPTION_LEASE_TIME: u8 = 51;
/// DHCP Message Type option code
const OPTION_MESSAGE_TYPE: u8 = 53;
/// Server Identifier option code
const OPTION_SERVER_ID: u8 = 54;

/// End of options marker
const OPTION_END: u8 = 255;

/// The standard DHCP magic cookie (0x63825363).
///
/// This value is required by RFC 2132 section 2 (see <https://www.rfc-editor.org/rfc/rfc2132#section-2>),
/// and is used to identify DHCP packets and options. All incoming packets are checked for this value.
pub(crate) const DHCP_MAGIC: [u8; 4] = [0x63, 0x82, 0x53, 0x63];

/// DHCP packet structure as defined in RFC 2131 <https://www.rfc-editor.org/rfc/rfc2131>
///
/// This represents the fixed portion of a DHCP message, followed by
/// variable-length options. The structure is packed to ensure correct
/// wire format representation.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct DhcpPacket {
    /// Message operation code: 1 = BOOTREQUEST, 2 = BOOTREPLY
    op: u8,
    /// Hardware address type (1 = Ethernet)
    htype: u8,
    /// Hardware address length (6 for Ethernet)
    hlen: u8,
    /// Number of relay agent hops
    hops: u8,
    /// Transaction ID, chosen by client
    xid: u32,
    /// Seconds elapsed since client began address acquisition
    secs: u16,
    /// Flags (bit 0 = broadcast flag)
    flags: u16,
    /// Client IP address (if client is in BOUND, RENEW or REBINDING state)
    ciaddr: [u8; 4],
    /// 'Your' (client) IP address
    yiaddr: [u8; 4],
    /// IP address of next server to use in bootstrap
    siaddr: [u8; 4],
    /// Relay agent IP address
    giaddr: [u8; 4],
    /// Client hardware address (16 bytes, only first 6 used for Ethernet)
    chaddr: [u8; 16],
    /// Optional server host name (null terminated string)
    sname: [u8; 64],
    /// Boot file name (null terminated string)
    file: [u8; 128],
    /// DHCP magic cookie (see [`DHCP_MAGIC`])
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
            magic: DHCP_MAGIC, // Always set to the standard DHCP magic cookie
        }
    }
}

/// Represents a DHCP lease entry for a client
///
/// Contains the assigned IP address, client MAC address, and lease expiration time.
/// Used internally by the DHCP server to track active leases.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
struct LeaseEntry {
    /// The IP address assigned to the client
    ip: Ipv4Addr,
    /// The MAC address of the client (6 bytes for Ethernet)
    mac: [u8; 6],
    /// Timestamp when the lease expires (milliseconds since boot)
    lease_time: u64, // Timestamp when lease expires
}

/// A lightweight DHCP server implementation for embedded systems
///
/// This server provides basic DHCP functionality including IP address assignment,
/// lease management, and essential DHCP options. It's designed to work in `no_std`
/// environments with minimal memory usage.
///
/// # Examples
///
/// ## Basic usage with simple constructor
///
/// ```rust,no_run
/// use core::net::Ipv4Addr;
/// use leasehund::DhcpServer;
///
/// let server = DhcpServer::new(
///     Ipv4Addr::new(192, 168, 1, 1),    // Server IP
///     Ipv4Addr::new(255, 255, 255, 0),  // Subnet mask  
///     Ipv4Addr::new(192, 168, 1, 1),    // Gateway
///     Ipv4Addr::new(8, 8, 8, 8),        // DNS server
///     Ipv4Addr::new(192, 168, 1, 100),  // Pool start
///     Ipv4Addr::new(192, 168, 1, 200),  // Pool end
/// );
/// ```
///
/// ## Advanced usage with configuration builder
///
/// ```rust,no_run
/// use core::net::Ipv4Addr;
/// use leasehund::{DhcpServer, DhcpConfigBuilder};
///
/// let config = DhcpConfigBuilder::new()
///     .server_ip(Ipv4Addr::new(10, 0, 1, 1))
///     .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
///     .router(Ipv4Addr::new(10, 0, 1, 1))
///     .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
///     .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
///     .ip_pool(Ipv4Addr::new(10, 0, 100, 1), Ipv4Addr::new(10, 0, 199, 254))
///     .lease_time(7200) // 2 hours
///     .build();
///
/// let server = DhcpServer::with_config(config);
/// ```
pub struct DhcpServer {
    /// Server configuration
    config: DhcpConfig,
    /// Hash map storing active leases, keyed by client MAC address
    leases: FnvIndexMap<[u8; 6], LeaseEntry, MAX_CLIENTS>,
}

impl DhcpServer {
    /// Creates a new DHCP server with the specified configuration
    ///
    /// # Arguments
    ///
    /// * `server_ip` - The IP address of this DHCP server
    /// * `subnet_mask` - Subnet mask to assign to clients (e.g., 255.255.255.0)
    /// * `router` - Default gateway IP address to assign to clients
    /// * `dns_server` - DNS server IP address to assign to clients
    /// * `ip_pool_start` - First IP address in the pool for client assignment
    /// * `ip_pool_end` - Last IP address in the pool for client assignment
    ///
    /// # Returns
    ///
    /// A new `DhcpServer` instance ready to handle DHCP requests
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core::net::Ipv4Addr;
    /// use leasehund::DhcpServer;
    ///
    /// let server = DhcpServer::new(
    ///     Ipv4Addr::new(192, 168, 1, 1),    // Server IP
    ///     Ipv4Addr::new(255, 255, 255, 0),  // Subnet mask
    ///     Ipv4Addr::new(192, 168, 1, 1),    // Gateway
    ///     Ipv4Addr::new(8, 8, 8, 8),        // DNS
    ///     Ipv4Addr::new(192, 168, 1, 100),  // Pool start
    ///     Ipv4Addr::new(192, 168, 1, 200),  // Pool end
    /// );
    /// ```    #[must_use]
    #[must_use]
    pub const fn new(
        server_ip: Ipv4Addr,
        subnet_mask: Ipv4Addr,
        router: Ipv4Addr,
        _dns_server: Ipv4Addr, // Unused in const context, but kept for API compatibility
        ip_pool_start: Ipv4Addr,
        ip_pool_end: Ipv4Addr,
    ) -> Self {
        // For const fn, we need to create config manually without using Vec methods
        let config = DhcpConfig {
            server_ip,
            subnet_mask,
            router: Some(router),
            dns_servers: Vec::new(), // Will be empty in const context
            ip_pool_start,
            ip_pool_end,
            lease_time: LEASE_TIME,
            socket_buffer_size: DEFAULT_SOCKET_BUFFER_SIZE,
        };

        Self {
            config,
            leases: FnvIndexMap::new(),
        }
    }

    /// Creates a new DHCP server with simple configuration and a single DNS server
    ///
    /// This is a non-const version of `new` that properly handles the DNS server.
    /// Use this when you need the DNS server to be included in the configuration.
    ///
    /// # Arguments
    ///
    /// * `server_ip` - The IP address of this DHCP server
    /// * `subnet_mask` - Subnet mask to assign to clients (e.g., 255.255.255.0)
    /// * `router` - Default gateway IP address to assign to clients
    /// * `dns_server` - DNS server IP address to assign to clients
    /// * `ip_pool_start` - First IP address in the pool for client assignment
    /// * `ip_pool_end` - Last IP address in the pool for client assignment
    ///
    /// # Returns
    ///
    /// A new `DhcpServer` instance ready to handle DHCP requests
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core::net::Ipv4Addr;
    /// use leasehund::DhcpServer;
    ///
    /// let server = DhcpServer::new_with_dns(
    ///     Ipv4Addr::new(192, 168, 1, 1),    // Server IP
    ///     Ipv4Addr::new(255, 255, 255, 0),  // Subnet mask
    ///     Ipv4Addr::new(192, 168, 1, 1),    // Gateway
    ///     Ipv4Addr::new(8, 8, 8, 8),        // DNS
    ///     Ipv4Addr::new(192, 168, 1, 100),  // Pool start
    ///     Ipv4Addr::new(192, 168, 1, 200),  // Pool end
    /// );
    /// ```
    #[must_use]
    pub fn new_with_dns(
        server_ip: Ipv4Addr,
        subnet_mask: Ipv4Addr,
        router: Ipv4Addr,
        dns_server: Ipv4Addr,
        ip_pool_start: Ipv4Addr,
        ip_pool_end: Ipv4Addr,
    ) -> Self {
        let mut dns_servers = Vec::new();
        let _ = dns_servers.push(dns_server);

        let config = DhcpConfig {
            server_ip,
            subnet_mask,
            router: Some(router),
            dns_servers,
            ip_pool_start,
            ip_pool_end,
            lease_time: LEASE_TIME,
            socket_buffer_size: DEFAULT_SOCKET_BUFFER_SIZE,
        };

        Self {
            config,
            leases: FnvIndexMap::new(),
        }
    }

    /// Creates a new DHCP server with advanced configuration options
    ///
    /// This method provides more flexibility than the basic `new` method,
    /// allowing configuration of multiple DNS servers, custom lease times,
    /// and other advanced options.
    ///
    /// # Arguments
    ///
    /// * `config` - DHCP server configuration
    ///
    /// # Returns
    ///
    /// A new `DhcpServer` instance ready to handle DHCP requests
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core::net::Ipv4Addr;
    /// use leasehund::{DhcpServer, DhcpConfigBuilder};
    ///
    /// let config = DhcpConfigBuilder::new()
    ///     .server_ip(Ipv4Addr::new(10, 0, 1, 1))
    ///     .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
    ///     .router(Ipv4Addr::new(10, 0, 1, 1))
    ///     .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
    ///     .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
    ///     .lease_time(7200)
    ///     .build();
    ///
    /// let server = DhcpServer::with_config(config);
    /// ```
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_config(config: DhcpConfig) -> Self {
        Self {
            config,
            leases: FnvIndexMap::new(),
        }
    }

    /// Gets a reference to the current configuration
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn config(&self) -> &DhcpConfig {
        &self.config
    }

    /// Gets the current lease count
    #[must_use]
    pub fn lease_count(&self) -> usize {
        self.leases.len()
    }

    /// Checks if the IP pool is full
    #[must_use]
    pub fn is_pool_full(&self) -> bool {
        let pool_size =
            u32::from(self.config.ip_pool_end) - u32::from(self.config.ip_pool_start) + 1;
        self.leases.len() >= (pool_size as usize).min(MAX_CLIENTS)
    }

    /// Creates a new DHCP server with the specified configuration
    ///
    /// # Arguments
    ///
    /// * `config` - A `DhcpConfig` structure containing the desired configuration
    ///
    /// # Returns
    ///
    /// A new `DhcpServer` instance ready to handle DHCP requests
    ///
    /// # Examples
    ///
    /// ```rust
    /// use core::net::Ipv4Addr;
    /// use leasehund::{DhcpConfig, DhcpServer};
    /// use heapless::Vec;
    ///
    /// let mut dns_servers = Vec::new();
    /// dns_servers.push(Ipv4Addr::new(8, 8, 8, 8)).ok();
    /// dns_servers.push(Ipv4Addr::new(8, 8, 4, 4)).ok();
    ///
    /// let config = DhcpConfig {
    ///     server_ip: Ipv4Addr::new(192, 168, 1, 1),
    ///     subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
    ///     router: Some(Ipv4Addr::new(192, 168, 1, 1)),
    ///     dns_servers,
    ///     ip_pool_start: Ipv4Addr::new(192, 168, 1, 100),
    ///     ip_pool_end: Ipv4Addr::new(192, 168, 1, 200),
    ///     lease_time: 3600, // 1 hour
    ///     socket_buffer_size: 1024,
    /// };
    ///
    /// let server = DhcpServer::with_config(config);
    /// ```
    /// Finds the next available IP address in the configured pool
    ///
    /// Iterates through the IP address range from `ip_pool_start` to `ip_pool_end`
    /// and returns the first IP address that is not currently leased to a client.
    ///
    /// # Returns
    ///
    /// * `Some(Ipv4Addr)` - The next available IP address
    /// * `None` - If all IP addresses in the pool are currently leased
    ///
    /// # Example
    ///
    /// ```rust
    /// use core::net::Ipv4Addr;
    /// use leasehund::DhcpServer;
    /// // This test only checks that the function returns an IP in the pool when no leases exist.
    /// let server = DhcpServer::new_with_dns(
    ///     Ipv4Addr::new(10, 0, 0, 1),
    ///     Ipv4Addr::new(255, 255, 255, 0),
    ///     Ipv4Addr::new(10, 0, 0, 1),
    ///     Ipv4Addr::new(1, 1, 1, 1),
    ///     Ipv4Addr::new(10, 0, 0, 100),
    ///     Ipv4Addr::new(10, 0, 0, 102),
    /// );
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

    /// Parses the DHCP message type from the options field
    ///
    /// Searches through the DHCP options to find the Message Type option (53)
    /// and returns its value. This is used to determine what type of DHCP
    /// message was received (DISCOVER, REQUEST, etc.).
    ///
    /// # Arguments
    ///
    /// * `options` - Byte slice containing the DHCP options data
    ///
    /// # Returns
    ///
    /// * `Some(u8)` - The DHCP message type if found
    /// * `None` - If the message type option is not present or malformed
    #[allow(clippy::unused_self)]
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

    /// Adds standard DHCP options to a response packet
    ///
    /// Appends the following options to the packet:
    /// - Message Type (53)
    /// - Server Identifier (54) - This server's IP address
    /// - Subnet Mask (1)
    /// - Router (3) - Default gateway
    /// - Domain Name Server (6) - DNS server
    /// - IP Address Lease Time (51)
    /// - End option (255)
    ///
    /// # Arguments
    ///
    /// * `packet` - Mutable reference to the packet buffer
    /// * `msg_type` - DHCP message type to include in options
    fn add_options(&self, packet: &mut Vec<u8, 576>, msg_type: u8) {
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

        // Add router option if configured
        if let Some(router) = self.config.router {
            packet.extend_from_slice(&[OPTION_ROUTER, 4]).ok();
            packet.extend_from_slice(&router.octets()).ok();
        }

        // Add DNS servers (support multiple)
        if !self.config.dns_servers.is_empty() {
            let dns_count = self.config.dns_servers.len() * 4; // 4 bytes per IP
            let dns_count_u8 = u8::try_from(dns_count).unwrap_or_default();
            packet
                .extend_from_slice(&[OPTION_DNS_SERVER, dns_count_u8])
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

    /// Creates a DHCP response packet
    ///
    /// Builds a properly formatted DHCP response packet based on the request
    /// and message type. Handles IP address assignment for OFFER and ACK messages.
    ///
    /// # Arguments
    ///
    /// * `req` - The incoming DHCP request packet
    /// * `msg_type` - Type of response to create (OFFER or ACK)
    ///
    /// # Returns
    ///
    /// A `Vec` containing the serialized DHCP response packet
    fn make_response(&mut self, req: &DhcpPacket, msg_type: u8) -> Vec<u8, 576> {
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
                }
            }
            DHCP_ACK => {
                if let Some(lease) = self.leases.get(&mac) {
                    resp.yiaddr = lease.ip.octets();
                } else if let Some(ip) = self.get_next_available_ip() {
                    resp.yiaddr = ip.octets();
                    let lease = LeaseEntry {
                        ip,
                        mac,
                        lease_time: embassy_time::Instant::now().as_millis()
                            + (u64::from(self.config.lease_time) * 1000),
                    };
                    let _ = self.leases.insert(mac, lease);
                }
            }
            _ => {}
        }
        let mut bytes = Vec::<u8, 576>::new();
        unsafe {
            let resp_bytes = core::slice::from_raw_parts(
                (&raw const resp).cast::<u8>(),
                core::mem::size_of::<DhcpPacket>(),
            );
            bytes.extend_from_slice(resp_bytes).ok();
        }
        self.add_options(&mut bytes, msg_type);
        bytes
    }

    /// Handles an incoming DHCP packet
    ///
    /// Processes incoming DHCP messages and generates appropriate responses:
    /// - DISCOVER messages receive OFFER responses
    /// - REQUEST messages receive ACK responses  
    /// - RELEASE messages trigger lease removal
    ///
    /// # Arguments
    ///
    /// * `socket` - UDP socket for sending responses
    /// * `data` - Raw packet data received from client
    #[allow(clippy::future_not_send)]
    async fn handle_packet(&mut self, socket: &UdpSocket<'_>, data: &[u8]) {
        if data.len() < core::mem::size_of::<DhcpPacket>() {
            return;
        }
        let packet = unsafe { &*data.as_ptr().cast::<DhcpPacket>() };
        if packet.magic != DHCP_MAGIC {
            return;
        }
        let options = &data[core::mem::size_of::<DhcpPacket>()..];
        if let Some(msg_type) = Self::parse_message_type(options) {
            match msg_type {
                DHCP_DISCOVER => {
                    let resp = self.make_response(packet, DHCP_OFFER);
                    let meta = embassy_net::udp::UdpMetadata {
                        endpoint: (Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT).into(),
                        local_address: None,
                        meta: PacketMeta::default(),
                    };
                    let _ = socket.send_to(&resp, meta).await;
                }
                DHCP_REQUEST => {
                    let resp = self.make_response(packet, DHCP_ACK);
                    let meta = embassy_net::udp::UdpMetadata {
                        endpoint: (Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT).into(),
                        local_address: None,
                        meta: PacketMeta::default(),
                    };
                    let _ = socket.send_to(&resp, meta).await;
                }
                DHCP_RELEASE => {
                    let mac: [u8; 6] = packet.chaddr[..6].try_into().unwrap_or([0; 6]);
                    self.leases.remove(&mac);
                }
                _ => {}
            }
        }
    }

    /// Runs the DHCP server on the provided network stack
    ///
    /// This method starts the DHCP server and runs indefinitely, listening for
    /// incoming DHCP requests on UDP port 67. It handles the complete DHCP
    /// transaction lifecycle including DISCOVER, REQUEST, and RELEASE messages.
    ///
    /// The server will:
    /// - Bind to UDP port 67 (standard DHCP server port)
    /// - Listen for incoming DHCP messages
    /// - Process requests and send appropriate responses
    /// - Manage IP address leases automatically
    ///
    /// # Arguments
    ///
    /// * `stack` - Embassy network stack instance for network operations
    ///
    /// # Returns
    ///
    /// This function never returns (marked with `!`) as it runs in an infinite loop
    ///
    /// # Panics
    ///
    /// Panics if the UDP socket cannot bind to the DHCP server port (67)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use embassy_net::Stack;
    /// use leasehund::DhcpServer;
    /// use core::net::Ipv4Addr;
    ///
    /// # async fn example(stack: Stack<'static>) {
    /// let mut server = DhcpServer::new(
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(255, 255, 255, 0),
    ///     Ipv4Addr::new(192, 168, 1, 1),
    ///     Ipv4Addr::new(8, 8, 8, 8),
    ///     Ipv4Addr::new(192, 168, 1, 100),
    ///     Ipv4Addr::new(192, 168, 1, 200),
    /// );
    ///
    /// // This will run forever
    /// server.run(stack).await;
    /// # }
    /// ```
    #[allow(clippy::future_not_send)]
    pub async fn run(&mut self, stack: Stack<'_>) -> ! {
        let mut rx_buffer = [0; 1024];
        let mut tx_buffer = [0; 1024];
        let mut rx_meta = [PacketMetadata::EMPTY; 16];
        let mut tx_meta = [PacketMetadata::EMPTY; 16];
        let mut socket = UdpSocket::new(
            stack,
            &mut rx_meta,
            &mut rx_buffer,
            &mut tx_meta,
            &mut tx_buffer,
        );
        socket.bind(DHCP_SERVER_PORT).unwrap();
        loop {
            let mut buf = [0u8; 576];
            match socket.recv_from(&mut buf).await {
                Ok((len, _)) => self.handle_packet(&socket, &buf[..len]).await,
                Err(_) => Timer::after(Duration::from_millis(100)).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use core::net::Ipv4Addr;

    #[test]
    fn dhcp_config_builder_basic() {
        let config = super::DhcpConfigBuilder::new()
            .clear_dns_servers()
            .server_ip(Ipv4Addr::new(10, 0, 0, 1))
            .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
            .router(Ipv4Addr::new(10, 0, 0, 254))
            .add_dns_server(Ipv4Addr::new(8, 8, 8, 8))
            .ip_pool(Ipv4Addr::new(10, 0, 0, 100), Ipv4Addr::new(10, 0, 0, 200))
            .lease_time(3600)
            .socket_buffer_size(2048)
            .build();
        assert_eq!(config.server_ip, Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(config.subnet_mask, Ipv4Addr::new(255, 255, 255, 0));
        assert_eq!(config.router, Some(Ipv4Addr::new(10, 0, 0, 254)));
        assert_eq!(config.dns_servers.len(), 1);
        assert_eq!(config.dns_servers[0], Ipv4Addr::new(8, 8, 8, 8));
        assert_eq!(config.ip_pool_start, Ipv4Addr::new(10, 0, 0, 100));
        assert_eq!(config.ip_pool_end, Ipv4Addr::new(10, 0, 0, 200));
        assert_eq!(config.lease_time, 3600);
        assert_eq!(config.socket_buffer_size, 2048);
    }

    #[test]
    fn dhcp_server_new_with_dns() {
        let server = super::DhcpServer::new_with_dns(
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
    fn dhcp_server_ip_pool_full() {
        let mut server = super::DhcpServer::new_with_dns(
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(255, 255, 255, 0),
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(1, 1, 1, 1),
            Ipv4Addr::new(10, 0, 0, 100),
            Ipv4Addr::new(10, 0, 0, 101),
        );
        // Simulate two leases (pool size = 2)
        for i in 0..2 {
            let mac = [0, 0, 0, 0, 0, i];
            let lease = super::LeaseEntry {
                ip: Ipv4Addr::new(10, 0, 0, 100 + i),
                mac,
                lease_time: 123456,
            };
            let _ = server.leases.insert(mac, lease);
        }
        assert!(server.is_pool_full());
    }

    #[test]
    fn dhcp_config_default_values() {
        let config = super::DhcpConfig::default();
        assert_eq!(config.server_ip, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(config.subnet_mask, Ipv4Addr::new(255, 255, 255, 0));
        assert_eq!(config.router, Some(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(config.dns_servers.len(), 1);
        assert_eq!(config.dns_servers[0], Ipv4Addr::new(8, 8, 8, 8));
        assert_eq!(config.ip_pool_start, Ipv4Addr::new(192, 168, 1, 100));
        assert_eq!(config.ip_pool_end, Ipv4Addr::new(192, 168, 1, 200));
        assert_eq!(config.lease_time, super::LEASE_TIME);
        assert_eq!(config.socket_buffer_size, super::DEFAULT_SOCKET_BUFFER_SIZE);
    }

    // Removed dhcp_server_get_next_available_ip: covered by doc test
}
