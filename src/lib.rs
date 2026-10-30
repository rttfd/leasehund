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
//! - **Transport-free core**: [`DhcpServer::handle_packet`] turns a received datagram into a reply
//!   without any I/O, so it runs over any UDP stack
//! - **Embassy adapter**: `run()` and `lease_one()` drive the core over an Embassy socket (the
//!   default `embassy` feature; disable default features for the core alone)
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
//! # async fn example(stack: Stack<'static>) -> Result<core::convert::Infallible, leasehund::BindError> {
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
//! server.run(stack).await
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
//! # async fn example(stack: Stack<'static>) -> Result<core::convert::Infallible, leasehund::BindError> {
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
//! server.run(stack).await
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
//! # async fn example(stack: Stack<'static>) -> Result<(), leasehund::BindError> {
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
//! let mut socket = DHCPServerSocket::new(stack, &mut buffers)?;
//! let _event: Result<TransactionEvent, _> = server.lease_one(&mut socket).await;
//! # Ok(())
//! # }
//! ```

#![no_std]

// Lets the crate refer to itself as `leasehund::`, like its users and doc examples do.
extern crate self as leasehund;

mod config;
#[cfg(feature = "embassy")]
mod embassy;
mod event;
mod lease;
mod options;
mod packet;
mod pool;
mod server;

pub use config::{ConfigError, DhcpConfig, DhcpConfigBuilder};
#[cfg(feature = "embassy")]
pub use embassy::{BindError, DHCPServerBuffers, DHCPServerSocket, RecvError};
pub use event::{AdmissionEvent, TransactionEvent};
pub use lease::{Hostname, Lease, MAX_HOSTNAME_LEN, StaticLeaseError};
pub use pool::SubnetPool;
pub use server::{DhcpServer, Outcome, Reply};
