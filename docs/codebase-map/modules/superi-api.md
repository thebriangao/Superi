---
module_id: superi-api
source_paths:
  - open/crates/superi-api
source_hash: be1c415b49e99ef8765a9bbc3fb670119dead0414aea1981c6029f524600d1a1
source_files: 8
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-api` owns the transport-neutral public boundary intended for UI, scripting, extension, CLI, and automation clients. Its currently implemented vertical slice is media capability introspection: strict serialized snapshot types, one typed query, and one full-replacement change event. General command dispatch, scripting, transport envelopes, subscriptions, cancellation, transactions, and editor operations are not implemented.

## Source inventory

- `open/crates/superi-api/Cargo.toml`: Declares `serde`, `superi-core`, and `superi-engine` dependencies, test-only `serde_json` and `superi-media-io` dependencies, and the `os-codecs` feature forwarding to `superi-engine/os-codecs`.
- `open/crates/superi-api/src/api.rs`: Implements the public media capability model, engine-to-public conversion, current snapshot state, typed query execution, change detection, revisioning, and event creation.
- `open/crates/superi-api/src/commands.rs`: Defines the `ApiCommand` trait, `GetMediaCapabilities`, and its snapshot response.
- `open/crates/superi-api/src/events.rs`: Defines the `ApiEvent` trait and `MediaCapabilitiesChanged` full-replacement event.
- `open/crates/superi-api/src/lib.rs`: Publicly exposes the API, command, event, scripting, and version modules. Its crate-level `Status: skeleton` wording is now overbroad because the media capability path is implemented.
- `open/crates/superi-api/src/scripting.rs`: Explicit documentation-only placeholder for scripting and automation.
- `open/crates/superi-api/src/version.rs`: Owns the `2.0.0` media capability schema version and permanent method and event names.
- `open/crates/superi-api/tests/media_capabilities_contract.rs`: Provides five integration contracts covering deterministic policy projection, strict serialization, change events, detailed codec rows, and the assembled default registry.

## Public surface

The public modules are `api`, `commands`, `events`, `scripting`, and `version`.

`api` exposes:

- `MediaBackendTier`, with `Primary` and `Fallback` policy tiers.
- `CapabilityConstraint<T>`, distinguishing `NotApplicable`, `RuntimeNegotiated`, `Unreported`, and a concrete `Values` list.
- `ChromaSampling`, `HardwareAcceleration`, and tagged `MediaOperation` enums.
- `MediaCodecCapability`, `MediaBackendCapabilities`, `MediaOperationSupport`, and `MediaCapabilitiesSnapshot`, all with private fields and read-only getters.
- `MediaCapabilitiesApi`, with `new`, `execute`, and `synchronize` as the only construction and state-update path.

`commands` exposes `ApiCommand`, whose associated response and permanent method name describe a typed command, plus the empty strict parameter object `GetMediaCapabilities` and `GetMediaCapabilitiesResult`. `events` exposes `ApiEvent` and `MediaCapabilitiesChanged`. `version` exposes `MEDIA_CAPABILITIES_SCHEMA_VERSION`, `GET_MEDIA_CAPABILITIES_METHOD`, and `MEDIA_CAPABILITIES_CHANGED_EVENT`.

The method name is `superi.media.capabilities.get`, the event name is `superi.media.capabilities.changed`, and the snapshot schema is `2.0.0`. These are constants only; no JSON-RPC dispatcher or event channel consumes them in this crate.

## Architecture and data flow

An engine consumer first builds `superi_engine::introspection::MediaCapabilities`, normally from a media backend registry. `MediaCapabilitiesApi::new` converts that engine snapshot into API-owned serialized types and starts its API-local revision at zero. Conversion preserves the engine's stable backend and operation ordering while translating tiers, operations, hardware modes, correlated codec details, and constraint states without exposing media I/O implementation types.

`execute(GetMediaCapabilities)` clones the complete current snapshot into a typed response. `synchronize` converts a fresh engine snapshot using the current revision, compares schema, backends, and operations while intentionally ignoring revision, and returns `None` for an identical state. A changed state increments the revision with checked arithmetic, stores the new snapshot, and returns one `MediaCapabilitiesChanged` event containing that same full snapshot. Exhausting `u64` revisions produces a terminal internal `superi-api.media_capabilities/synchronize` error.

Bulk frames, audio, packets, and GPU resources do not cross this implemented boundary. The only data is capability metadata and stable identifiers.

## Dependencies and consumers

- `serde` supplies strict `Serialize` and `Deserialize` implementations. Public enums use stable snake-case representations, tagged enums use a `kind` field, structs reject unknown fields, and public enums are non-exhaustive for Rust callers.
- `superi-core` supplies the classified error model and `SemanticVersion`.
- `superi-engine` supplies the API-neutral capability snapshot that this crate projects into its stable public schema.
- `superi-media-io` and `serde_json` are dev dependencies used to build test registries and verify wire shapes and round trips.
- `superi-cli` declares this crate as a dependency and forwards `os-codecs`, but its source does not yet call any API item.
- The only current Rust consumers of the public API types are this crate's integration tests. No shell, UI, script runtime, extension host, local socket, or closed-tree client exists in the repository.

## Invariants and operational boundaries

- Snapshot revisions are API-local, monotonic on actual state changes, and unchanged by repeated synchronization with equal state.
- Change events are full replacements, not deltas. The event snapshot revision matches the queryable stored snapshot after synchronization.
- Backends expose stable identifiers, display names, priority, policy tier, operation declarations, hardware mode, and sorted correlated codec rows. Operation support separates primary and explicit-fallback backend lists.
- Private fields and crate-private response and event constructors prevent external Rust callers from manufacturing these records through normal constructors, while deserialization remains intentionally public.
- Strict deserialization rejects unknown fields at the defined objects and tagged enums. `#[non_exhaustive]` requires downstream Rust matches to retain a fallback arm.
- Capability introspection does not instantiate sources, decoders, or encoders because the engine snapshot is declaration-based.
- The `os-codecs` feature changes which engine registry backends may be assembled; the public schema and API state machine remain the same.

## Tests and verification

`media_capabilities_contract.rs` verifies five implemented behaviors:

- Equal-priority backends and fallback tiers produce deterministic public ordering, strict command parameters reject unknown fields, and responses survive JSON round trips.
- Equal snapshots emit no event, changed backend declarations emit one named full-replacement event at revision one, and repeated equal synchronization is silent.
- A detailed capability-only change is detected and publishes hardware and constraint details.
- Decode and encode rows retain correlated profiles, levels, bit depths, and chroma sampling, and unknown nested fields are rejected.
- The default engine registry reaches the public snapshot with software AV1 encode details and schema version `2.0.0`.

The test backend deliberately returns errors from resource factories, so successful tests also prove that introspection uses declarations rather than factory creation. There is no test for revision exhaustion, a real transport, the forwarded `os-codecs` feature at the API level, or scripting.

## Current status and risks

Media capability querying and event payloads are implemented and test-backed. The rest of the promised unified automation API is absent, and `scripting.rs` remains an explicit placeholder. The constants describe JSON-RPC-style names but there are no request envelopes, dispatcher, ordered event stream, sequence numbers, subscriptions, project revisions, cancellation, transactions, or structured public error serialization in this crate.

The crate-level skeleton label understates the implemented media capability slice, while the broader crate description overstates all other editor control paths. The CLI dependency is not yet evidence of a real external consumer.

## Maintenance notes

When engine capability fields change, update the private conversion functions, strict public types, schema version policy, event comparison, and JSON contracts together. Preserve deterministic ordering and full-replacement semantics unless a versioned protocol change explicitly replaces them. Any new command or event should map its real dispatcher and consumer, not only add a name constant or placeholder module.
