---
module_id: superi-cli
source_paths:
  - open/crates/superi-cli
source_hash: 1f7864f6dc7d4a2665f9e3d0ed070b41f9e3ff81936d60534ae7c0e889bd997a
source_files: 3
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-cli` is the workspace's headless executable boundary and is intended to become the first consumer of `superi-api`. Its implemented behavior is limited to printing a scaffold version line.

## Source inventory

- `open/crates/superi-cli/Cargo.toml`: Declares the binary crate, its `superi-core` and `superi-api` dependencies, and an `os-codecs` feature that forwards to `superi-api/os-codecs`.
- `open/crates/superi-cli/src/commands.rs`: Private documentation-only placeholder for future render and inspect commands routed through the public API.
- `open/crates/superi-cli/src/main.rs`: Implements the executable entry point and prints `superi <package-version>: scaffold (no engine yet)`.

## Public surface

This crate produces a binary, not a library. The observable surface is process startup and one stdout line containing the Cargo package version. It accepts no arguments, exposes no subcommands, returns no structured data, and does not initialize the API or engine. The private `commands` module exports nothing.

The Cargo feature `os-codecs` forwards feature activation through `superi-api` to `superi-engine`, causing those dependency paths to compile, but it does not change CLI runtime behavior.

## Architecture and data flow

`main` reads the compile-time `CARGO_PKG_VERSION` value through `env!` and passes the fixed scaffold text to `println!`. No user input, file, media, engine state, command dispatch, or API response participates.

## Dependencies and consumers

- `superi-core` and `superi-api` are declared dependencies, but neither is imported by the Rust source.
- The executable is an intended API consumer only. There is no current call to `MediaCapabilitiesApi` or any other API item.
- Workspace documentation invokes it with `cargo run -p superi-cli` and includes feature-build commands, but no Rust crate consumes this binary.

## Invariants and operational boundaries

- The executable is headless and deterministic for a fixed package version.
- The process uses no network, media, GPU, or persistent state.
- Feature forwarding preserves the codec feature boundary at build time. It does not expose codec discovery or operations at the command line.

## Tests and verification

The crate owns no unit or integration tests. A build verifies feature wiring, and running it can verify the single scaffold line, but there is no command, API, rendering, inspection, or exit-status contract test.

## Current status and risks

`commands.rs` is an explicit placeholder. The binary is runnable but is not a vertical slice and is not yet an actual consumer of the public API despite its manifest dependency and crate documentation.

## Maintenance notes

When commands are implemented, map argument parsing, command-to-API translation, output and error formats, cancellation, exit statuses, feature-specific behavior, and end-to-end tests. Keep the distinction between compiling an API dependency and exercising the API.
