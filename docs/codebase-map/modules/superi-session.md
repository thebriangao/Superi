---
module_id: superi-session
source_paths:
  - open/crates/superi-session
source_hash: 607957722ad2b60bc40825070725878c93d4881f882f5e08ae5da98150efaf69
source_files: 15
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-session` owns portable application and project session services that are independent of a
particular window toolkit. It preserves the prior host's substantive lifecycle, engine connection,
project, capability, crash, platform, process, media preview, source monitoring, and generated API
transport behavior without retaining Tauri wrappers or webview assumptions.

## Source inventory

- `open/crates/superi-session/Cargo.toml`: Declares portable session dependencies and test support.
- `open/crates/superi-session/src/capabilities.rs`: Builds honest GPU, audio, codec, and local AI capability snapshots.
- `open/crates/superi-session/src/crash_diagnostics.rs`: Retains bounded private crash evidence and safe recovery projections.
- `open/crates/superi-session/src/engine.rs`: Owns managed EngineControl, playback, and bounded worker connections.
- `open/crates/superi-session/src/lib.rs`: Defines crate ownership and exports portable session modules.
- `open/crates/superi-session/src/lifecycle.rs`: Coordinates application and headless-engine lifecycle generations.
- `open/crates/superi-session/src/migration.rs`: Validates and publishes byte-preserving legacy private-state migration.
- `open/crates/superi-session/src/platform_adapters.rs`: Reports stable platform adapter families and contracts.
- `open/crates/superi-session/src/process_runtime.rs`: Retains explicit process-service ownership, tasks, snapshots, and ordered joins.
- `open/crates/superi-session/src/project_lifecycle.rs`: Owns durable project open, save, settings, media, annotations, identity, and history orchestration.
- `open/crates/superi-session/src/project_lifecycle/media_preview.rs`: Builds bounded exact local media preview artifacts.
- `open/crates/superi-session/src/project_lifecycle/source_monitor.rs`: Owns source-monitor observation and project routing.
- `open/crates/superi-session/src/project_lifecycle/source_monitoring.rs`: Runs bounded source monitoring and freshness updates.
- `open/crates/superi-session/src/transport.rs`: Provides bounded generated API command routing and ordered retained events.
- `open/crates/superi-session/tests/legacy_state_migration.rs`: Proves validation-first, byte-exact, idempotent, and fail-closed migration.

## Public surface

The crate exports platform-neutral lifecycle, engine, project, transport, capability, crash,
adapter, process-runtime, and migration types. The transport returns a boxed
`DesktopTransportError`, preserving the complete generated `PublicApiError` while keeping the
result boundary compact. The migration API accepts explicit legacy and destination roots and
publishes no partial state.

## Architecture and data flow

`ApplicationLifecycle` coordinates the headless engine participant. `LinkedEngineProcess` owns
EngineControl and Playback domain threads plus the bounded worker pool and exposes a cloneable
transport-neutral connection. `DesktopProjectState` owns durable project and media actions.
`DesktopTransportState` scopes client generations and pending work, routes generated requests
through the managed engine, and broadcasts one ordered retained event stream.

`DesktopProcessRuntime` records each retained service as one coherent phase, count, join, thread,
and summary update. Shutdown closes admission, attempts all joins, and preserves the first error
without skipping later cleanup. Legacy migration validates every source file before using hard-link
publication into a fresh destination, leaving source bytes untouched.

## Dependencies and consumers

The crate composes `superi-api`, `superi-engine`, `superi-project`, `superi-media-io`,
`superi-codecs-rs`, `superi-concurrency`, `superi-gpu`, `superi-audio`, `superi-ai`, and
`superi-core`. `superi-desktop` is its current production consumer. Future native hosts may reuse
the same services without importing a window toolkit into this crate.

## Invariants and operational boundaries

- Session services contain no Tauri, webview, React, or browser dependency.
- Engine mutation remains on the explicit EngineControl owner.
- Project and transport generations fence stale work.
- Public transport failures retain reviewed generated error context.
- Process shutdown attempts every owned join even after an earlier failure.
- Legacy migration validates all inputs before publication and never mutates the source.
- User private state is never written during isolated native smoke tests.

## Tests and verification

Module tests cover lifecycle, engine routing, project and media behavior, process ownership,
capability reporting, transport cancellation and event order, crash evidence, and platform
contracts. `legacy_state_migration.rs` covers byte preservation, idempotence, conflict rejection,
corruption rejection, and source immutability. Locked tests and strict Clippy are required.

## Current status and risks

The migrated session layer is substantive and consumed by the new native host. Some services remain
foundation seams rather than complete UI integrations. The hard-link migration publication model
requires source and destination to share a filesystem, which is appropriate for the one-time local
private-state transition and fails closed otherwise.

## Maintenance notes

Update this map for lifecycle participants, process services, project and transport APIs, migration
schema, platform adapters, media behavior, engine routing, or native host consumers. Preserve the
window-toolkit-neutral boundary.
