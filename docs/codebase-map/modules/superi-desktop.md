---
module_id: superi-desktop
source_paths:
  - open/crates/superi-desktop
source_hash: dee6b6775deb474466a8c85be3e3dccba6288c4731636ce2d63e6145781712f2
source_files: 4
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-desktop` is the thin production native window host. It owns the winit event loop, window
surface lifecycle, operating-system input translation, AccessKit adapter, native presentation, and
bounded smoke mode. It composes retained UI and portable session services but does not own their
business state or rendering policy.

## Source inventory

- `open/crates/superi-desktop/Cargo.toml`: Declares the native binary, library seam, winit, AccessKit, UI, GPU, and session dependencies.
- `open/crates/superi-desktop/src/lib.rs`: Defines launch options, session-root selection, smoke reporting, and native application entry.
- `open/crates/superi-desktop/src/main.rs`: Parses bounded command-line options and launches the native host.
- `open/crates/superi-desktop/tests/native_host_contract.rs`: Proves argument parsing, isolated smoke state, and one successful native presentation.

## Public surface

The library exposes `DesktopLaunchOptions`, command-line parsing, and `run`. The binary accepts a
bounded `--smoke` path plus explicit session and legacy roots. Smoke mode reports exactly one
successful retained-scene presentation and exits.

## Architecture and data flow

The host resolves a platform session root before creating the event loop. It installs AccessKit
before making the window visible, creates one managed GPU instance and surface, builds a retained
foundation scene, and submits it through `superi-ui`. Window resize and scale changes rebuild the
scene. Pointer, key, text, focus, and accessibility actions normalize into the shared retained
interaction controller. A failed surface acquisition receives one bounded recovery attempt.

## Dependencies and consumers

The crate consumes `superi-ui`, `superi-session`, `superi-gpu`, `superi-core`, winit, AccessKit
winit integration, and platform path discovery. It is the shipped desktop executable and has no
webview, React, Tauri, browser runtime, or network dependency.

## Invariants and operational boundaries

- The native host remains thin and delegates scene, session, engine, and GPU policy.
- The window is not visible before accessibility wiring is installed.
- Smoke mode uses temporary session storage unless the caller explicitly supplies a root.
- Every operating-system action resolves to a current retained node before dispatch.
- One surface recovery attempt is allowed; repeated failure exits with classified evidence.

## Tests and verification

`native_host_contract.rs` exercises option parsing and the real native surface smoke route. The
checkpoint verifier runs locked build, tests, formatting, and strict Clippy. Manual visual judgment
uses captures from the same retained compositor through the private inspector.

## Current status and risks

The native host presents the Phase Infinity foundation and integrates AccessKit actions. Full menu,
clipboard, drag and drop, IME, multiple window, monitor color, and production session orchestration
remain later checkpoints.

## Maintenance notes

Keep this map synchronized with event-loop ownership, surface recovery, session root behavior,
input translation, accessibility adapter setup, and production host dependencies.
