# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

## [0.7.0] - 2026-10-30

### Fixed

- A `DHCPDISCOVER` from a client that already held a lease was offered a different address, and its full lease was cut down to a 60-second offer reservation. It is now offered its current address and keeps its lease.
- When the pool or the lease table was exhausted, the server sent a `DHCPOFFER` (and could send a `DHCPACK`) for `0.0.0.0`, or for an address it could not record, which could then be handed out twice. It now stays silent for offers and answers requests with `DHCPNAK`.
- `DHCPREQUEST` was always acknowledged, ignoring the requested address and the server identifier. On a network with another DHCP server, a client that accepted the other server's offer was still ACKed by this one. Requests for another server are now ignored (and free our reservation), and requests for an address that is wrong, taken or outside the pool get a `DHCPNAK` (RFC 2131 section 4.3.2). A request without any address is ignored instead of being ACKed with the next free address.
- The server's own address, the router, and the subnet's network and broadcast addresses were offered when they fell inside the pool. They are now skipped.
- `is_pool_full()` and the address search underflowed (a panic in debug builds) when `ip_pool_start` was after `ip_pool_end`.
- PAD options (code 0) were read as options with a length byte, so a message type after padding was not found and the packet was dropped. Options are now parsed per RFC 2132, and a message-type option with the wrong length is rejected.
- Received datagrams larger than 576 bytes were truncated by the receive buffer, which `lease_one()` reported as an error and `run()` dropped. Packets are now received into a buffer as large as the socket's.
- Replies did not copy `giaddr` and `flags` from the request, and were always broadcast. Replies to relayed requests now go to the relay agent on port 67, renewals and `DHCPINFORM` answers go to the client's address, and a NAK through a relay sets the broadcast flag.
- Packets that are not client requests (`op` other than BOOTREQUEST) or not Ethernet (`htype`/`hlen`) were processed with a garbage MAC address. They are now ignored.
- More than 63 DNS servers produced a DNS option with length 0 followed by the addresses, corrupting the option list. At most 63 are sent, and every option is written whole or not at all.
- A `DHCPRELEASE` naming another server, or an address other than the client's lease, released the lease anyway. It is now ignored.

### Added

- `DHCPNAK`, `DHCPDECLINE` (the declined address is withheld for 10 minutes) and `DHCPINFORM` (configuration without a lease) support.
- Static leases: `DhcpServer::add_static_lease()` and `remove_static_lease()`, sized by a new third const generic `MAX_STATIC` (default 8). A static address may lie outside the pool and is never given to another client. While it is held by another client (leased before the static lease was added) or was declined, its client is served from the pool and moves to the static address at its next renewal.
- Address selection follows RFC 2131 section 4.3.1: the client's static lease, its current binding, the address it asks for (option 50), then the next free address.
- Clients that return after the server restarted (INIT-REBOOT or renewal with an address the server has no record of) are acknowledged when the address is free, so they keep their address.
- Lease inspection and management: `leases()`, `lease_for()` and `revoke()`, with the new `Lease` type.
- `DhcpConfig::validate()` and `DhcpConfigBuilder::try_build()` with `ConfigError`, which catch an inverted pool, a pool or router outside the subnet, a non-contiguous subnet mask, a zero lease time and an overlong domain name.
- New reply options: renewal (T1, option 58) and rebinding (T2, option 59) times, broadcast address (option 28), and domain name (option 15) via `DhcpConfig::domain_name` / `DhcpConfigBuilder::domain_name()`. A lease time of `u32::MAX` is an infinite lease.
- `AdmissionEvent::Inform`, so admission policies also cover `DHCPINFORM`.
- Multiple subnets through DHCP relay agents: `DhcpServer::add_pool()` adds a `SubnetPool` (address range, subnet mask, optional router) for clients on another subnet. Relayed requests are served from the pool whose subnet contains the relay's address (`giaddr`), with that subnet's mask, router and broadcast address; DNS servers, lease time and domain name are shared. Requests from relays on unknown subnets are ignored, overlapping pools are rejected with `ConfigError::OverlappingSubnet`, and a client that moves between subnets is refused its old address and gets one from the new pool. Sized by a fourth const generic, `MAX_POOLS` (default 4).
- Client host names: the host name option (12) is kept with the lease as `Lease::hostname` (a `Hostname`, at most `MAX_HOSTNAME_LEN` = 32 bytes, invalid UTF-8 and trailing NULs dropped). A client that omits it on a later request keeps its earlier name. `run_with_lease_callback()` and `lease_one_with_lease()` report the `Lease` alongside each `TransactionEvent`; the existing `TransactionEvent` is unchanged.

### Changed

- **BREAKING**: `DhcpConfig` has a new `domain_name: Option<&'static str>` field. Add `domain_name: None` when constructing it directly.
- **BREAKING**: `AdmissionEvent` is now `#[non_exhaustive]` and has a new `Inform` variant; add a wildcard arm to `match` expressions.
- `DhcpServer` has two new const generics, `MAX_STATIC` and `MAX_POOLS`, both with defaults, so `DhcpServer<32, 4>` keeps working.
- Static leases must be on the server's subnet or a pool's subnet (`StaticLeaseError::OutsideSubnet` otherwise), and are only handed out while the client is on that subnet.
- `is_pool_full()` now also reports a pool with no assignable address left (for example when the remaining addresses are reserved or declined).

## [0.6.0] - 2026-08-26

### Added

- Optional DHCP admission-policy hooks via `AdmissionEvent`, `lease_one_with_filter()`, and `run_with_filter_and_callback()`. Callers can suppress `DHCPOFFER` and `DHCPACK` responses before they are sent while preserving existing default behavior.

### Changed

- Updated the optional `defmt` dependency to 1.1.1.
- Removed unnecessary direct `hash32` and `smoltcp` dependencies. Lease storage now uses `heapless::FnvIndexMap`, and UDP packet metadata uses Embassy's compatible default type.

## [0.5.2] - 2026-06-22

### Fixed

- Flush the UDP socket after handling packets in `DhcpServer::run_with_callback()`. Without this, DHCP OFFER/ACK responses could remain buffered and clients would self-assign link-local addresses instead of receiving leases.

## [0.5.1] - 2026-06-03

### Added

- `DhcpServer::run_with_callback()` — runs the DHCP server forever while invoking a caller-provided callback for every `TransactionEvent` (lease assigned, lease released). This allows downstream code to react to DHCP lifecycle events without replacing the built-in `run()` loop.

## [0.5.0] - 2026-05-24

### Breaking Changes

- **Removed `new_with_dns`**: `DhcpServer::new()` now takes the DNS server parameter directly and uses it (previously the `new()` constructor silently ignored it). Use `new()` everywhere you used `new_with_dns()`.
- **Removed `socket_buffer_size` from `DhcpConfig`**: This field was never used; UDP buffers are fixed at 1024 bytes.
- **`DhcpConfigBuilder::new()` starts with no DNS servers**: Previously pre-populated with `8.8.8.8`. Add DNS servers explicitly with `.add_dns_server()`.
- **`LeaseEntry.lease_time` renamed to `expires_at`**: Internal change, affects code that directly constructs `LeaseEntry` in tests.

### Added

- **Lease expiry enforcement**: Expired leases are automatically purged before each packet is processed. New public method `purge_expired_leases()`.
- **IP reservation on OFFER**: Offered IPs are reserved with a 60-second TTL to prevent duplicate offers to concurrent clients.
- Additional unit tests: `config_builder_starts_with_no_dns`, `config_builder_no_router`, `get_next_available_ip_empty`, `get_next_available_ip_skips_leased`, `parse_message_type_*`.

### Fixed

- **Unsound pointer cast**: Replaced `&*data.as_ptr().cast::<DhcpPacket>()` with `core::ptr::read_unaligned()` to avoid undefined behavior on unaligned UDP buffer data.
- **Unsound serialization**: Replaced `core::slice::from_raw_parts` with `core::mem::transmute` to a byte array for safe packed struct serialization.
- **Async bloat**: Consolidated duplicated `send_to().await` calls across DISCOVER/REQUEST match arms into a single suspend point, reducing the generated state machine size.

### Removed

- `DhcpServer::new_with_dns()` (use `new()` instead)
- `DhcpConfig::socket_buffer_size` field
- `DhcpConfigBuilder::socket_buffer_size()` method
- `#[allow(dead_code)]` on `LeaseEntry`
- `#[allow(clippy::unused_self)]` on static method `parse_message_type`

## [0.4.0] - 2026-03-27

### Changed

- Bumped MSRV to Rust 1.91
- Updated dependencies to latest versions
  - `embassy-net` 0.8.0 -> 0.9.0
  - `embassy-time` 0.5.0 -> 0.5.1
  - `heapless` 0.8.0 -> 0.9.2
  - `smoltcp` 0.12.0 -> 0.13.0
- Updated `heapless::FnvIndexMap` to `IndexMap` with explicit hasher (heapless 0.9 API change)
- Updated GitHub Actions to Node.js 24 compatible versions
  - `actions/checkout@v4` -> `v5`
  - `actions/cache@v4` -> `v5`
  - `actions/create-release@v1` -> `softprops/action-gh-release@v2`

### Added

- `hash32` dependency for `FnvHasher` (required by heapless 0.9 `IndexMap`)

## [0.3.0] - 2026-03-27

### Added

- Makefile with development commands (`make ci`, `make test`, `make publish`, etc.)
- CHANGELOG.md
- `DHCPServerBuffers` struct for pre-allocated UDP buffers (@kdimonych)
- `DHCPServerSocket` wrapper type for DHCP socket management (@kdimonych)
- `run_once` method for single DHCP transaction processing (@kdimonych)
- `lease_one` method for manual transaction handling (@kdimonych)

### Changed

- Updated `embassy-net` from 0.7.0 to 0.8.0 (@arctan2, @liebman)
- Updated `embassy-time` (@liebman)
- GitHub workflows now use Makefile commands for consistency
- Removed decorative emojis from codebase, keeping only status indicators
- Fixed doctest examples to compile correctly
- Improved documentation with more examples (@kdimonych)
- Refactored to remove magic constants (@kdimonych)

## [0.2.0]

### Changed

- Version bump

## [0.1.0]

### Added

- Initial release
- Basic DHCP server implementation for `no_std` environments
- Embassy async runtime integration
- Configurable IP pools and lease management
- Essential DHCP options support (subnet mask, router, DNS)

[Unreleased]: https://github.com/rttfd/leasehund/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/rttfd/leasehund/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/rttfd/leasehund/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/rttfd/leasehund/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/rttfd/leasehund/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/rttfd/leasehund/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/rttfd/leasehund/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/rttfd/leasehund/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/rttfd/leasehund/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/rttfd/leasehund/releases/tag/v0.1.0
