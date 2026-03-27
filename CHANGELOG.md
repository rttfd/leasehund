# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-03-27

### Added
- Makefile with development commands (`make ci`, `make test`, `make publish`, etc.)
- CHANGELOG.md
- `DHCPServerBuffers` struct for pre-allocated UDP buffers
- `DHCPServerSocket` wrapper type for DHCP socket management
- `run_once` method for single DHCP transaction processing
- `lease_one` method for manual transaction handling
- Improved documentation with more examples
- Refactored to remove magic constants
- Updated to latest compatible crate versions

### Changed
- GitHub workflows now use Makefile commands for consistency
- Removed decorative emojis from codebase, keeping only status indicators (checkmarks/failures)
- Fixed doctest examples to compile correctly without `no_run`

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
