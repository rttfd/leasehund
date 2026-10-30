[<img alt="github" src="https://img.shields.io/badge/github-rttfd/leasehund-37a8e0?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/rttfd/leasehund)
[<img alt="crates.io" src="https://img.shields.io/crates/v/leasehund.svg?style=for-the-badge&color=ff8b94&logo=rust" height="20">](https://crates.io/crates/leasehund)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-leasehund-bedc9c?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/leasehund)

![Dall-E generated leasehund image](https://raw.githubusercontent.com/rttfd/static/refs/heads/main/leasehund/leasehund.jpeg)

# Leasehund

A lightweight, embedded-friendly DHCP server implementation for Rust `no_std` environments.

## Overview

Leasehund provides a minimal DHCP server implementation designed for embedded systems and resource-constrained environments. It supports the core DHCP functionality needed for automatic IP address assignment in local networks. The server core is transport-free; an Embassy adapter (the default `embassy` feature) runs it on an Embassy network stack, and any other UDP stack can drive it through `handle_packet()`.

## Features

- **No-std compatible**: Designed for embedded systems without heap allocation
- **Transport-free core**: `DhcpServer::handle_packet()` turns a received datagram into a reply without any I/O
- **Embassy adapter**: `run()` and `lease_one()` drive the core over an Embassy socket (default `embassy` feature)
- **Configurable IP pools**: Define custom IP address ranges for client assignment
- **Lease expiry**: Expired leases are automatically reclaimed before each allocation
- **IP reservation**: Offered IPs are reserved to prevent duplicate offers
- **RFC 2131 request handling**: `DHCPNAK` for wrong addresses, server-identifier checks for multi-server networks, INIT-REBOOT and renewal handling, and graceful recovery after the server restarts
- **Static leases**: Pin a MAC address to a fixed IP with `add_static_lease()`
- **Address-conflict handling**: Addresses reported by `DHCPDECLINE` are withheld for 10 minutes
- **`DHCPINFORM`**: Configuration for clients with a manually set address
- **Several subnets**: Extra `SubnetPool`s serve clients behind DHCP relay agents, each with its own range, mask and router
- **Client host names**: The name a client sends (option 12) is kept with its lease and reported with lease events
- **Lease inspection**: `leases()`, `lease_for()` and `revoke()` for status pages and admin tools
- **Configuration validation**: `DhcpConfig::validate()` / `DhcpConfigBuilder::try_build()` catch pools outside the subnet and similar mistakes
- **Admission policy hooks**: Optionally suppress offers and acknowledgments by client MAC
- **Multiple DNS servers**: Support for up to N DNS servers (compile-time const generic)
- **Optional router configuration**: Router/gateway can be disabled if not needed
- **Builder pattern**: Fluent API for easy configuration
- **Memory efficient**: Uses heapless data structures with compile-time size limits

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
leasehund = "0.7"
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

    let mut server = DhcpServer::<32, 4>::new(
        Ipv4Addr::new(192, 168, 1, 1),    // Server IP
        Ipv4Addr::new(255, 255, 255, 0),  // Subnet mask
        Ipv4Addr::new(192, 168, 1, 1),    // Router/Gateway
        Ipv4Addr::new(8, 8, 8, 8),        // DNS server
        Ipv4Addr::new(192, 168, 1, 100),  // IP pool start
        Ipv4Addr::new(192, 168, 1, 200),  // IP pool end
    );

    // Runs forever; it only returns if UDP port 67 cannot be bound.
    let Err(error) = server.run(stack).await;
    error!("DHCP server cannot bind port 67: {:?}", error);
}
```

## Configuration

### Basic Configuration

The DHCP server requires the following configuration parameters:

| Parameter       | Description                      | Example         |
| --------------- | -------------------------------- | --------------- |
| `server_ip`     | IP address of the DHCP server    | `192.168.1.1`   |
| `subnet_mask`   | Network subnet mask              | `255.255.255.0` |
| `router`        | Default gateway IP address       | `192.168.1.1`   |
| `dns_server`    | DNS server IP address            | `8.8.8.8`       |
| `ip_pool_start` | First IP in the assignable range | `192.168.1.100` |
| `ip_pool_end`   | Last IP in the assignable range  | `192.168.1.200` |

### Advanced Configuration (Builder Pattern)

Use the builder API for multiple DNS servers and custom options:

```rust
use core::net::Ipv4Addr;
use leasehund::{DhcpConfigBuilder, DhcpServer};

let config = DhcpConfigBuilder::<4>::new()
    .server_ip(Ipv4Addr::new(10, 0, 1, 1))
    .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
    .router(Ipv4Addr::new(10, 0, 1, 1))
    .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
    .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
    .ip_pool(Ipv4Addr::new(10, 0, 100, 1), Ipv4Addr::new(10, 0, 199, 254))
    .lease_time(7200) // 2 hours
    .build();

let server: DhcpServer<32, 4> = DhcpServer::with_config(config);
```

**Note:** The builder starts with **no DNS servers**. Use `.add_dns_server()` to add them. The maximum number of concurrent leases and DNS servers are compile-time constants set via const generics (e.g., `DhcpServer::<32, 4>`).

## Supported DHCP Messages

| Message Type | Description                            | Server Response             |
| ------------ | -------------------------------------- | --------------------------- |
| **DISCOVER** | Client broadcast to find DHCP servers  | **OFFER** with the client's static, current, requested or next free IP; no response when nothing is free |
| **REQUEST**  | Client request for specific IP address | **ACK** confirming lease, or **NAK** if the address is wrong or taken; no response if it is meant for another server |
| **DECLINE**  | Client found the address already in use | Address withheld for 10 minutes (no response) |
| **RELEASE**  | Client releasing IP address            | Lease removal (no response) |
| **INFORM**   | Client with a fixed address asking for configuration | **ACK** with configuration, no lease |

## DHCP Options Supported

The server automatically includes these standard DHCP options in responses:

- **Option 1**: Subnet Mask
- **Option 3**: Router (Default Gateway)
- **Option 6**: Domain Name Server (DNS)
- **Option 15**: Domain Name (when configured with `domain_name()`)
- **Option 28**: Broadcast Address
- **Option 51**: IP Address Lease Time
- **Option 53**: DHCP Message Type
- **Option 54**: Server Identifier
- **Option 58 / 59**: Renewal (T1) and Rebinding (T2) Time

It reads options 12 (Host Name), 50 (Requested IP Address), 53 and 54 from clients.

## Advanced Usage

In case you need to handle lease/release events of each new client you can use the `lease_one` method:

```rust
use core::net::Ipv4Addr;
use leasehund::{DhcpServer, DhcpConfigBuilder, DHCPServerSocket, DHCPServerBuffers, TransactionEvent};
use embassy_net::Stack;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Initialize your embassy network stack here
    let stack = /* ... your network stack initialization ... */;

    let config = DhcpConfigBuilder::<4>::new()
        .server_ip(Ipv4Addr::new(10, 0, 1, 1))
        .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
        .router(Ipv4Addr::new(10, 0, 1, 1))
        .add_dns_server(Ipv4Addr::new(1, 1, 1, 1))
        .add_dns_server(Ipv4Addr::new(1, 0, 0, 1))
        .add_dns_server(Ipv4Addr::new(8, 8, 8, 8))
        .ip_pool(
            Ipv4Addr::new(10, 0, 100, 1),
            Ipv4Addr::new(10, 0, 199, 254)
        )
        .lease_time(7200)
        .build();

    let mut server: DhcpServer<32, 4> = DhcpServer::with_config(config);
    let mut buffers = DHCPServerBuffers::new();
    let Ok(mut socket) = DHCPServerSocket::new(stack, &mut buffers) else {
        error!("DHCP server cannot bind port 67");
        return;
    };
    loop {
        let Ok(event) = server.lease_one(&mut socket).await else {
            continue;
        };

        match event {
            TransactionEvent::Leased(ip, mac) => {
                info!("Leased IP: {} to MAC: {:02x?}", ip, mac);
            }
            TransactionEvent::Released(ip, mac) => {
                info!("Released IP: {} from MAC: {:02x?}", ip, mac);
            }
        }
    }
}
```

### Event Callbacks

If you want the convenience of `run()` but still need to react to lease events, use `run_with_callback()`:

```rust
use core::net::Ipv4Addr;
use leasehund::{DhcpServer, DhcpConfigBuilder, TransactionEvent};
use embassy_net::Stack;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let stack = /* ... your network stack initialization ... */;

    let config = DhcpConfigBuilder::<4>::new()
        .server_ip(Ipv4Addr::new(10, 0, 1, 1))
        .subnet_mask(Ipv4Addr::new(255, 255, 0, 0))
        .no_router()
        .add_dns_server(Ipv4Addr::new(8, 8, 8, 8))
        .ip_pool(Ipv4Addr::new(10, 0, 100, 1), Ipv4Addr::new(10, 0, 199, 254))
        .lease_time(7200)
        .build();

    let mut server: DhcpServer<32, 4> = DhcpServer::with_config(config);

    let Err(error) = server.run_with_callback(stack, |event| {
        match event {
            TransactionEvent::Leased(ip, mac) => {
                info!("New lease: {} -> {:02x?}", ip, mac);
            }
            TransactionEvent::Released(ip, mac) => {
                info!("Lease released: {} -> {:02x?}", ip, mac);
            }
        }
    }).await;
    error!("DHCP server cannot bind port 67: {:?}", error);
}
```

This is ideal for logging, metrics, or triggering side effects (e.g., opening a firewall pinhole) when a host gets a DHCP lease.

### Admission Policies

Use `run_with_filter_and_callback()` to decide whether a client may receive a DHCP offer or acknowledgment before the response is sent:

```rust
use leasehund::AdmissionEvent;

const BLOCKED_CLIENT: [u8; 6] = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];

// Construct `server` and initialize `stack` as shown above.
let Err(error) = server.run_with_filter_and_callback(
    stack,
    |mac, event| match event {
        AdmissionEvent::Discover | AdmissionEvent::Request | AdmissionEvent::Inform => {
            mac != BLOCKED_CLIENT
        }
        // Releases are processed regardless; `AdmissionEvent` is non-exhaustive.
        _ => true,
    },
    |_event| {
        // Handle completed lease and release events if needed.
    },
).await;
```

Returning `false` for `Discover`, `Request` or `Inform` silently drops the packet without sending an OFFER, ACK, or NAK. Releases are always processed. MAC addresses can be spoofed, so this hook provides policy control rather than an authentication or security boundary. For manual transaction handling, use `lease_one_with_filter()`.

### Static Leases and Lease Inspection

```rust
use core::net::Ipv4Addr;
use leasehund::DhcpServer;

let mut server = DhcpServer::<32, 4>::new(
    Ipv4Addr::new(192, 168, 1, 1),
    Ipv4Addr::new(255, 255, 255, 0),
    Ipv4Addr::new(192, 168, 1, 1),
    Ipv4Addr::new(8, 8, 8, 8),
    Ipv4Addr::new(192, 168, 1, 100),
    Ipv4Addr::new(192, 168, 1, 200),
);

// The printer always gets .50, even though it is outside the pool.
server
    .add_static_lease([0x00, 0x11, 0x22, 0x33, 0x44, 0x55], Ipv4Addr::new(192, 168, 1, 50))
    .unwrap();

for lease in server.leases() {
    // lease.ip, lease.mac, lease.expires_at_ms, lease.bound
}
```

### Serving Several Subnets Through Relays

Clients on other subnets reach the server through a DHCP relay agent (often the router), which
forwards their broadcasts with its own address in `giaddr`. Add a `SubnetPool` per subnet; the
server picks the pool containing the relay's address and replies through the relay:

```rust
use core::net::Ipv4Addr;
use leasehund::{DhcpServer, SubnetPool};

let mut server = DhcpServer::<32, 4>::new(
    Ipv4Addr::new(192, 168, 1, 1),
    Ipv4Addr::new(255, 255, 255, 0),
    Ipv4Addr::new(192, 168, 1, 1),
    Ipv4Addr::new(8, 8, 8, 8),
    Ipv4Addr::new(192, 168, 1, 100),
    Ipv4Addr::new(192, 168, 1, 200),
);
// Sensors on 192.168.2.0/24 behind the relay at 192.168.2.1.
server
    .add_pool(
        SubnetPool::new(
            Ipv4Addr::new(255, 255, 255, 0),
            Ipv4Addr::new(192, 168, 2, 100),
            Ipv4Addr::new(192, 168, 2, 200),
        )
        .with_router(Ipv4Addr::new(192, 168, 2, 1)),
    )
    .unwrap();
```

Pools may not overlap each other or the server's subnet. Up to `MAX_POOLS` (the fourth const
generic, default 4) can be added. Static leases on a pool's subnet work once the pool exists.

### Client Host Names

`run_with_lease_callback()` passes the `Lease` with every event; its `hostname` is the name the
client sent, if any (cut to 32 bytes):

```rust,ignore
let Err(error) = server.run_with_lease_callback(stack, |_, _| true, |event, lease| {
    if let (TransactionEvent::Leased(ip, _), Some(name)) = (event, lease.hostname) {
        // show "{name} -> {ip}" on a status page
    }
}).await;
```

`lease_one_with_lease()` does the same for manual transaction handling, and `leases()` /
`lease_for()` include the host name too.

### Without Embassy

The server core does no I/O. Disable default features to build it without Embassy, receive
datagrams on UDP port 67 with your own stack, and send each reply where it says:

```toml
[dependencies]
leasehund = { version = "0.7", default-features = false }
```

```rust,ignore
let outcome = server.handle_packet(&datagram, now_ms, |_mac, _event| true);
if let Some(reply) = &outcome.reply {
    udp.send_to(reply.bytes(), reply.destination()); // from port 67
}
if let Some((event, lease)) = outcome.event {
    // a lease was granted or released
}
```

`now_ms` is any monotonic clock in milliseconds; lease expiry times use the same clock.

### Validating the Configuration

```rust
use core::net::Ipv4Addr;
use leasehund::{ConfigError, DhcpConfigBuilder};

let result = DhcpConfigBuilder::<4>::new()
    .server_ip(Ipv4Addr::new(10, 0, 0, 1))
    .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
    .ip_pool(Ipv4Addr::new(10, 0, 0, 100), Ipv4Addr::new(10, 0, 0, 200))
    .try_build();
// The builder's default router (192.168.1.1) is not on 10.0.0.0/24.
assert_eq!(result.unwrap_err(), ConfigError::RouterOutsideSubnet);
```

## Protocol Compliance

Leasehund is compliant with [RFC 2131](https://www.rfc-editor.org/rfc/rfc2131) and [RFC 2132](https://www.rfc-editor.org/rfc/rfc2132). All DHCP packets include and check the required DHCP magic cookie (0x63825363, see [RFC 2132 section 2](https://www.rfc-editor.org/rfc/rfc2132#section-2)) for strict standards compliance.

## Architecture

### Memory Usage

The server uses fixed-size data structures to ensure predictable memory usage:

- **Lease Storage**: `FnvIndexMap` with maximum entries set by const generic (e.g., `DhcpServer::<32, 4>`); the client count must be a power of two
- **Static Leases**: Up to `MAX_STATIC` entries, the third const generic (default 8)
- **Packet Buffers**: 1KB RX/TX buffers for UDP socket
- **Response Packets**: Maximum 576 bytes per DHCP response

### Network Protocol

- **Listen Port**: UDP 67 (standard DHCP server port)
- **Client Port**: UDP 68 (standard DHCP client port)
- **Addressing**: Replies go to the relay agent (UDP 67) for relayed requests, to the client's address when it already has one (renewals, `DHCPINFORM`), and are broadcast otherwise
- **Packet Format**: RFC 2131 compliant DHCP packet structure

## Examples

### Simple Home Network

```rust
let server = DhcpServer::<32, 4>::new(
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
let server = DhcpServer::<32, 4>::new(
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
- **Sizing**: Maximum clients and DNS servers are compile-time constants set via const generics
- **Basic Options**: Limited to essential DHCP options
- **Leases are in memory**: They are lost when the device restarts; clients that come back with INIT-REBOOT keep their address if it is still free
- **Relay pools**: Additional subnets share the main configuration's DNS servers, lease time and domain name; requests from relays on subnets without a pool are ignored

## Requirements

- **Rust**: Edition 2024 or later
- **Embassy**: Optional; the default `embassy` feature adds the Embassy adapter
- **no_std**: Fully compatible with no_std environments
- **Memory**: Approximately 2KB RAM for lease storage and buffers

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

### Development Setup

```bash
git clone https://github.com/rttfd/leasehund.git
cd leasehund
make ci
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

**Leasehund** - Because every good network needs a reliable dog to fetch IP addresses!
