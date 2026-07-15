---
module_id: superi-api
source_paths:
  - open/crates/superi-api
source_hash: 5943c668d952464278454def433b8b9c65e53ebd630a42a00409e4338338c44a
source_files: 11
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-api` owns the transport-neutral public boundary for UI, scripting, extension, CLI, and
automation clients. Two public slices are implemented: media capability introspection and canonical
editorial scenario control through revision-fenced typed transactions and ordered full-state
events. Wire transport, subscriptions, scripting, cancellation, persistence, and broad editor
operations remain absent.

## Source inventory

- `open/crates/superi-api/Cargo.toml`: Declares production `serde`, `superi-core`, and
  `superi-engine` dependencies plus test-only `serde_json`, `sha2`, and `superi-media-io`.
- `open/crates/superi-api/src/api.rs`: Implements public media capability snapshots, query state,
  engine conversion, revisioning, and full-replacement change events.
- `open/crates/superi-api/src/commands.rs`: Defines `ApiCommand`, media capability query types, and
  typed one-action and ordered transaction scenario commands.
- `open/crates/superi-api/src/events.rs`: Defines `ApiEvent`, the media capability change event, and
  the ordered full-replacement scenario state event.
- `open/crates/superi-api/src/lib.rs`: Exposes API, command, event, scenario, scripting, and version
  modules.
- `open/crates/superi-api/src/scenario.rs`: Implements strict canonical action documents, public
  editorial state and graph projections, reversible operation evidence, structured failures, and
  the mutable dispatcher-backed `ScenarioApi` facade with optimistic transactions and event drain.
- `open/crates/superi-api/src/scripting.rs`: Placeholder for a scripting runtime.
- `open/crates/superi-api/src/version.rs`: Owns both schema revisions and permanent method and event
  names.
- `open/crates/superi-api/tests/media_capabilities_contract.rs`: Covers deterministic capability
  projection, strict serialization, change events, codec rows, and default registry integration.
- `open/crates/superi-api/tests/dispatcher_contract.rs`: Covers atomic public transactions,
  revision fences, strict command and event round trips, ordered full-state publication, one-unit
  undo, inspect behavior, permanent names, and legacy action compatibility.
- `open/crates/superi-api/tests/scenario_contract.rs`: Covers the strict canonical schema, complete
  state projection, exact undo plus redo evidence, and structured last-valid-state failures.

## Public surface

The media capability surface remains schema `2.0.0`, method
`superi.media.capabilities.get`, and event `superi.media.capabilities.changed`. It exposes strict
backend, operation, hardware, constraint, codec, snapshot, response, and full-replacement event
types through `MediaCapabilitiesApi`.

The canonical scenario surface is schema `1.0.0`. The compatibility one-action method remains
`superi.slice.scenario.action.execute`, while the atomic method is
`superi.slice.scenario.transaction.execute` and its ordered replacement event is
`superi.slice.scenario.state.changed`. `SliceAction` contains strict import, place, trim, mirror,
undo, redo, and inspect variants. Import carries exact fixture identity, manifest and payload
digests, rational rate, frame count, and extent. `ExecuteScenarioTransaction` adds caller-owned
transaction identity, an exact expected revision, and ordered actions. Its result carries the
successful command sequence, committed action kinds, and complete resulting state.
`ScenarioDocument` remains the bounded versioned ordered container for stored or scripted canonical
action lists.

`ScenarioStateSnapshot` exposes project identity, revision, phase, imported fixture state, named and
typed timeline state, complete graph nodes, ports, edges, transform parameters, implementation
identity, the active four-entry operation log, and history depths. `ScenarioFailure` serializes
category, recoverability, message, context frames, and the boxed last valid complete state.

## Architecture and data flow

Media capability conversion remains declaration-only. Engine registry records are projected into
API-owned stable types, equal states do not advance API revision, and a semantic change emits one
full replacement.

`ScenarioApi` owns one scenario-only `superi_engine::dispatcher::EngineCommandDispatcher`. It maps
each strict public transaction to one engine transaction, preserves the caller revision fence, and
projects ordered engine events into public `ScenarioStateChanged` values. Inspect returns current
state without mutation or an event. The one-action command wraps the same dispatcher transaction
path with a generated local identity. Public projection keeps bulk fixture bytes inside the engine
boundary and exposes only path, identity, metadata, editorial state, graph control state, operation
evidence, transaction identity, and sequencing.

Canonical public flow is:

```text
SliceAction list plus expected revision
  -> ExecuteScenarioTransaction
  -> ScenarioApi
  -> superi-engine EngineCommandDispatcher
  -> ScenarioTransactionResult plus ordered ScenarioStateChanged event
  -> or ScenarioFailure with complete last valid state and no event
```

Projection preserves stable core identifiers as strings, exact rational rates, named timeline and
track identity, half-open ranges, full image-port graph topology, row-major binary64 mirror matrix,
sampling and edge behavior, and original mutation revisions. Unknown future engine enum variants
map to explicit public `Unknown` values instead of being silently interpreted.

## Dependencies and consumers

- `serde` supplies strict snake-case and tagged serialized shapes with unknown-field rejection.
- `superi-core` supplies semantic versions, exact frame rates, and the classified error model.
- `superi-engine` supplies capability declarations, canonical transactional state, typed dispatch,
  and ordered replacement events.
- `serde_json`, `sha2`, and `superi-media-io` are test dependencies for wire, digest, and registry
  contracts.
- `superi-cli` is the first production Rust consumer of `ScenarioApi`; it never reaches engine
  scenario state directly.

No transport, UI, shell, scripting runtime, extension host, or closed-tier client is present.

## Invariants and operational boundaries

- Public types own their wire schema and do not expose media-I/O or GPU implementation types.
- Strict objects reject unknown fields. Non-exhaustive Rust enums require downstream fallback.
- Inspect never changes revision or history.
- Rejected actions and transactions return the complete last valid state and do not mutate engine
  state, command sequencing, history, or the event stream.
- Public scenario mutations use an exact expected revision. A successful transaction is one engine
  revision and one undo unit and emits one matching full-replacement event.
- The compatibility action method routes through the transaction dispatcher and cannot diverge
  from automation behavior.
- Export is not a scenario action and cannot enter the mutation log.
- Canonical graph state is complete enough for control and inspection, while bulk media remains
  internal.
- Capability snapshot revisions remain API-local and advance only on actual capability changes.
- Scenario engine revisions advance on every successful mutation, undo, or redo; the operation log
  retains original mutation revisions.
- `os-codecs` changes engine registry assembly but not either public schema.

## Tests and verification

The media capability contracts retain five focused behaviors: deterministic ordering and strict
round trips, silent equal synchronization, one changed full-replacement event, detailed correlated
codec rows, and default software AV1 exposure.

The scenario contracts prove exact schema round trips, rejection of guessed fields and effects, the
permanent typed method name, full canonical import metadata, named timeline and ranges, complete
three-node graph with image ports, exact mirror parameters, four stable operation records, two undo
plus two redo actions, final revision 8, structured engine context, and last-valid-state retention.

Three dispatcher contracts prove strict transaction and event JSON, permanent namespaced method and
event identity, failed trailing-action rollback, one-revision commit, one-unit undo, optimistic
conflict rejection, independent monotonic event order with exact originating-command correlation,
complete replacement state, inspect without an event, and compatibility action routing through the
same dispatcher.

These tests do not prove wire delivery ordering, scripting, UI integration, media decoding, graph
pixel evaluation, or runtime export.

## Current status and risks

The API now has two substantive control surfaces, but it is still far from the promised unified
editor API. Scenario schema 1 is deliberately narrow and fixed to one canonical edit. Its reference
state proves transactional control semantics, not production timeline, graph, or media ownership.

The separate capability and scenario schema versions are correct for independent surfaces, and the
first scenario dispatcher is now implemented. Wire framing, version negotiation, authentication,
subscription delivery, retry and idempotency policy, and cancellation remain required before remote
clients exist. Public failure state is boxed to keep result errors bounded while preserving the
same serialized shape.

## Maintenance notes

Preserve strict serialization, deterministic ordering, stable permanent names, last-valid-state
errors, revision fences, transaction and event agreement, and unknown-variant handling. Engine
command changes require synchronized public projection, schema review, CLI consumer updates, and
focused JSON contracts. Do not expose packets, frames, textures, fixture bytes, or engine-private
history snapshots merely to simplify a client.
