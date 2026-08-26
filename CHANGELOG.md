# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

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

[Unreleased]: https://github.com/rttfd/leasehund/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/rttfd/leasehund/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/rttfd/leasehund/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/rttfd/leasehund/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/rttfd/leasehund/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/rttfd/leasehund/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/rttfd/leasehund/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/rttfd/leasehund/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/rttfd/leasehund/releases/tag/v0.1.0
