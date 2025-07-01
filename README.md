[<img alt="github" src="https://img.shields.io/badge/github-rttfd/leasehund-37a8e0?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/rttfd/leasehund)
[<img alt="crates.io" src="https://img.shields.io/crates/v/leasehund.svg?style=for-the-badge&color=ff8b94&logo=rust" height="20">](https://crates.io/crates/leasehund)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-leasehund-bedc9c?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/leasehund)

![Dall-E generated leasehund image](https://raw.githubusercontent.com/rttfd/static/refs/heads/main/leasehund/leasehund.jpeg)

# Leasehund 🐶

A lightweight, embedded-friendly DHCP server implementation for Rust `no_std` environments.

## Overview

Leasehund provides a minimal DHCP server implementation designed for embedded systems and resource-constrained environments. Built on top of the Embassy async runtime, it supports the core DHCP functionality needed for automatic IP address assignment in local networks.

## Features

- **🚀 No-std compatible**: Designed for embedded systems without heap allocation
- **⚡ Embassy integration**: Built on top of Embassy async runtime and networking stack
- **🔧 Configurable IP pools**: Define custom IP address ranges for client assignment
- **📋 Lease management**: Automatic lease tracking with configurable timeouts
- **�️ Essential DHCP options**: Supports subnet mask, router, DNS server configuration
- **💾 Memory efficient**: Uses heapless data structures with compile-time size limits
- **🔒 Safe**: Written in safe Rust with comprehensive error handling

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
leasehund = "0.2.0"
```

## Usage

```rust
#![no_std]
#![no_main]

use core::net::Ipv4Addr;
use leasehund::DhcpServer;
use embassy_net::Stack;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Initialize your embassy network stack here
    let stack = /* ... your network stack initialization ... */;

    // Create DHCP server with explicit const generics
    let mut dhcp_server: DhcpServer<32, 4> = DhcpServer::new_with_dns(
        Ipv4Addr::new(192, 168, 1, 1),    // Server IP
        Ipv4Addr::new(255, 255, 255, 0),  // Subnet mask
        Ipv4Addr::new(192, 168, 1, 1),    // Router/Gateway
        Ipv4Addr::new(8, 8, 8, 8),        // DNS server
        Ipv4Addr::new(192, 168, 1, 100),  // IP pool start
        Ipv4Addr::new(192, 168, 1, 200),  // IP pool end
    );

    // Run the DHCP server (this will loop forever)
    dhcp_server.run(stack).await;
}
```

## Configuration

### Basic Configuration

The DHCP server requires the following configuration parameters:

| Parameter | Description | Example |
|-----------|-------------|---------|
| `server_ip` | IP address of the DHCP server | `192.168.1.1` |
| `subnet_mask` | Network subnet mask | `255.255.255.0` |
| `router` | Default gateway IP address | `192.168.1.1` |
| `dns_server` | DNS server IP address | `8.8.8.8` |
| `ip_pool_start` | First IP in the assignable range | `192.168.1.100` |
| `ip_pool_end` | Last IP in the assignable range | `192.168.1.200` |

### Advanced Configuration


### Advanced Configuration (Builder Pattern)

You can use the builder API to customize all key DHCP server parameters at runtime (const generics for sizing):

```rust
use core::net::Ipv4Addr;
use leasehund::{DhcpConfigBuilder, DhcpServer};

let config: leasehund::DhcpConfig<4> = DhcpConfigBuilder::<4>::new()
    .server_ip(Ipv4Addr::new(10, 0, 1, 1))
    .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
    .router(Ipv4Addr::new(10, 0, 1, 1))
    .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
    .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
    .ip_pool(Ipv4Addr::new(10, 0, 100, 1), Ipv4Addr::new(10, 0, 199, 254))
    .lease_time(7200) // 2 hours
    .socket_buffer_size(2048)
    .build();

let server: DhcpServer<32, 4> = DhcpServer::with_config(config);
```

**Note:** The maximum number of concurrent leases and DNS servers are now compile-time constants set via const generics (e.g., `DhcpServer::<32, 4>`).

## Supported DHCP Messages

| Message Type | Description | Server Response |
|--------------|-------------|-----------------|
| **DISCOVER** | Client broadcast to find DHCP servers | **OFFER** with available IP |
| **REQUEST** | Client request for specific IP address | **ACK** confirming lease |
| **RELEASE** | Client releasing IP address | Lease removal (no response) |

## DHCP Options Supported

The server automatically includes these standard DHCP options in responses:

- **Option 1**: Subnet Mask
- **Option 3**: Router (Default Gateway)
- **Option 6**: Domain Name Server (DNS)
- **Option 51**: IP Address Lease Time
- **Option 53**: DHCP Message Type
- **Option 54**: Server Identifier


## Protocol Compliance

Leasehund is compliant with [RFC 2131](https://www.rfc-editor.org/rfc/rfc2131) and [RFC 2132](https://www.rfc-editor.org/rfc/rfc2132). All DHCP packets include and check the required DHCP magic cookie (0x63825363, see [RFC 2132 section 2](https://www.rfc-editor.org/rfc/rfc2132#section-2)) for strict standards compliance.

## Architecture

### Memory Usage

The server uses fixed-size data structures to ensure predictable memory usage:

- **Lease Storage**: `FnvIndexMap` with maximum number of entries set by the const generic parameter (e.g., `DhcpServer::<32, 4>`, compile-time fixed)
- **Packet Buffers**: 1KB RX/TX buffers for UDP socket
- **Response Packets**: Maximum 576 bytes per DHCP response

### Network Protocol

- **Listen Port**: UDP 67 (standard DHCP server port)
- **Client Port**: UDP 68 (standard DHCP client port)
- **Broadcast**: All responses sent as broadcast packets for maximum compatibility
- **Packet Format**: RFC 2131 compliant DHCP packet structure

## Examples

### Simple Home Network

```rust
let dhcp_server: DhcpServer<32, 4> = DhcpServer::new_with_dns(
    Ipv4Addr::new(192, 168, 1, 1),    // Router IP
    Ipv4Addr::new(255, 255, 255, 0),  // /24 network
    Ipv4Addr::new(192, 168, 1, 1),    // Gateway
    Ipv4Addr::new(1, 1, 1, 1),        // Cloudflare DNS
    Ipv4Addr::new(192, 168, 1, 100),  // Pool start
    Ipv4Addr::new(192, 168, 1, 199),  // Pool end (100 addresses)
);
```

### Corporate Network

```rust
let dhcp_server: DhcpServer<32, 4> = DhcpServer::new_with_dns(
    Ipv4Addr::new(10, 0, 1, 1),       // Server IP
    Ipv4Addr::new(255, 255, 0, 0),    // /16 network
    Ipv4Addr::new(10, 0, 1, 1),       // Gateway
    Ipv4Addr::new(10, 0, 1, 2),       // Internal DNS
    Ipv4Addr::new(10, 0, 100, 1),     // Large pool start
    Ipv4Addr::new(10, 0, 199, 254),   // Large pool end
);
```

## Limitations

- **IPv4 Only**: IPv6 is not supported
- **Lease Time**: Configurable at runtime via `DhcpConfig`/`DhcpConfigBuilder` (default 24 hours)
- **Sizing**: Maximum clients and DNS servers are compile-time constants set via const generics (e.g., `DhcpServer::<32, 4>`)
- **Basic Options**: Limited to essential DHCP options
- **No Relay**: DHCP relay functionality not implemented
- **Client Limit**: Maximum of 32 concurrent clients (compile-time fixed, set via const generics, e.g., `DhcpServer::<32, 4>`)

## Requirements

- **Rust**: Edition 2024 or later
- **Embassy**: Compatible with Embassy async runtime
- **no_std**: Fully compatible with no_std environments
- **Memory**: Approximately 2KB RAM for lease storage and buffers

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

### Development Setup

```bash
git clone https://github.com/rttfd/leasehund.git
cd leasehund
cargo build
cargo test
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built with [Embassy](https://embassy.dev/) - Modern embedded framework for Rust
- Uses [smoltcp](https://github.com/smoltcp-rs/smoltcp) for network protocol implementation
- Inspired by the need for lightweight DHCP servers in embedded IoT applications

---

**Leasehund** - Because every good network needs a reliable dog to fetch IP addresses! 🐕‍🦺
