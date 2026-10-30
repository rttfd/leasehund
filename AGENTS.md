# AGENTS.md

Rules for anyone changing this crate. `CONTRIBUTING.md` covers the git workflow; this file covers
the code. They are the same rules I use for Rust apps: a library is somebody else's app core, so
nothing is waived because it's a library.

## Commands

Every task is a make target, locally and in CI. Never call `cargo` directly in docs or CI: add or
change a target instead. `make` alone lists them.

- `make ci`: everything CI runs (`fmt-check`, `check`, `lint`, `test`, `test-doc`, `doc-check`).
  Run it before every push.
- `make lint`: clippy, pedantic and nursery, on all targets and every feature set in
  `FEATURE_SETS`, warnings as errors.
- `make bench` / `make bench-save` / `make bench-compare` (`BASELINE=main`, `BENCH=<filter>`).

## Architecture: ports and adapters

- **Core** (`server/`, `config.rs`, `packet.rs`, `options.rs`, `pool.rs`, `lease.rs`,
  `event.rs`): DHCP itself, with no I/O and no clock. `DhcpServer::handle_packet` takes a
  datagram and the time in milliseconds and returns the reply and its destination. It builds
  without Embassy (`--no-default-features`), and its tests run that way too.
- **Adapter** (`embassy.rs`, the default `embassy` feature): the Embassy UDP socket, `run*`, and
  `lease_one*`. It drives the core only through its public API, never its internals.
- **The caller is the composition root.** Supporting another network stack means a new adapter
  module behind its own feature, not changes to the core.

## Modules and features

- Declare every module in `src/lib.rs` or its folder's `mod.rs`. `lib.rs` is the crate's table
  of contents: docs, module declarations and re-exports, nothing else. A module with parts is a
  folder with a `mod.rs` (`server/`), never `name.rs` next to `name/`.
- Put `#[cfg(feature = "...")]` on **module declarations**, not on items inside a file.
  Accepted exception: `#[cfg_attr(feature = "defmt", derive(defmt::Format))]`.
- Refer to the crate's own items as `leasehund::...` (`extern crate self as leasehund;` in
  `lib.rs`), never `crate::` or `super::`, in tests and doc links too. Inside private modules
  items are plain `pub`; only `pub(super)` remains where a part file shares with its parent.

## Errors and lints

- No `unwrap`, `expect` or `panic` outside tests: `deny` in `Cargo.toml`, opted out for tests in
  `clippy.toml`, and with `#![expect(...)]` at the top of a benchmark. A malformed packet is
  ignored and a socket that cannot bind is a `BindError`: never a panic on the caller's behalf.
- Fix lints instead of silencing them. If an exception is truly needed, use
  `#[expect(lint, reason = "...")]` with a real reason, never `#[allow]`.
- Public items document `# Errors` on anything returning `Result`, and carry `#[must_use]` where
  clippy asks for it.

## Tests and benchmarks

- Test the core through `prepare_packet_with_filter` / `handle_packet` with hand-built
  datagrams (`server/tests.rs`), not a real network. Name tests after the guarantee they check,
  including the unhappy paths.
- `benches/server.rs` covers the per-packet hot path through the public API. Update it when you
  change allocation or reply building, and compare against a saved baseline (`make bench-save`
  on `main`, then `make bench-compare` on your branch) instead of trusting a single run. The
  `profiling` profile is optimized with symbols, for `samply` and `cargo flamegraph`.
