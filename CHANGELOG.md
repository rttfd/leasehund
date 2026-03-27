# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
