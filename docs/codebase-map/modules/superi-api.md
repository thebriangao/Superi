---
module_id: superi-api
source_paths:
  - open/crates/superi-api
source_hash: caf4fccb308f1c0195ecd375dc4e969a3d684f08c6292a63e9df554994ce4f9d
source_files: 14
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-api` owns the transport-neutral public boundary for UI, scripting, extension, CLI, and
automation clients. Four public slices are implemented: media capability introspection, complete
engine capability and health introspection for adaptive clients, canonical editorial scenario
control through revision-fenced typed transactions and ordered full-state events, and coherent
read-only integration validation. Wire transport, subscriptions, scripting, cancellation,
persistence, and broad editor operations remain absent.

## Source inventory

- `open/crates/superi-api/Cargo.toml`: Declares production `serde`, `superi-core`, and
  `superi-engine` dependencies plus test-only `serde_json`, `sha2`, `superi-media-io`, and
  `superi-concurrency` for the real EngineControl introspection and validation contracts.
- `open/crates/superi-api/src/api.rs`: Implements public media and complete engine introspection
  snapshots, strict engine-neutral projection including project, device, sleep, and wake state,
  independent revisions, query state, and full-replacement change events.
- `open/crates/superi-api/src/commands.rs`: Defines `ApiCommand`, media, engine introspection, and
  integration validation query types, plus typed one-action and ordered transaction scenario
  commands.
- `open/crates/superi-api/src/events.rs`: Defines `ApiEvent`, media and engine introspection change
  events, and the ordered full-replacement scenario state event.
- `open/crates/superi-api/src/lib.rs`: Exposes API, command, event, scenario, scripting, validation,
  and version modules.
- `open/crates/superi-api/src/scenario.rs`: Implements strict canonical action documents, public
  editorial state and graph projections, reversible operation evidence, structured failures, and
  the mutable dispatcher-backed `ScenarioApi` facade with optimistic transactions and event drain.
- `open/crates/superi-api/src/scripting.rs`: Placeholder for a scripting runtime.
- `open/crates/superi-api/src/validation.rs`: Strictly projects canonical engine integration state,
  nested introspection, exact lifecycle and recovery actions, workflow permits or denials, scenario
  reversal capacity, endpoint replacement state, and coherence findings through typed read-only
  accessors. It also provides the standalone starting-engine query owner used by the CLI.
- `open/crates/superi-api/src/version.rs`: Owns all four schema revisions and permanent method and
  event names.
- `open/crates/superi-api/tests/media_capabilities_contract.rs`: Covers deterministic capability
  projection, strict serialization, change events, codec rows, and default registry integration.
- `open/crates/superi-api/tests/dispatcher_contract.rs`: Covers atomic public transactions,
  revision fences, strict command and event round trips, ordered full-state publication, one-unit
  undo, inspect behavior, permanent names, and legacy action compatibility.
- `open/crates/superi-api/tests/engine_introspection_contract.rs`: Drives the real engine registry,
  dispatcher, lifecycle, error coordinator, and resource arbiter through strict public query and
  replacement-event projection, independent revisions, degradation, recovery, and safe diagnostic
  serialization.
- `open/crates/superi-api/tests/integration_validation_contract.rs`: Covers strict versioned JSON,
  nested canonical introspection, exact startup, sleep, wake, and recovery action projection,
  coherent degraded workflow admission, and user-safe active failure state.
- `open/crates/superi-api/tests/scenario_contract.rs`: Covers the strict canonical schema, complete
  state projection, exact undo plus redo evidence, and structured last-valid-state failures.

## Public surface

The media capability surface remains schema `2.0.0`, method
`superi.media.capabilities.get`, and event `superi.media.capabilities.changed`. It exposes strict
backend, operation, hardware, constraint, codec, snapshot, response, and full-replacement event
types through `MediaCapabilitiesApi`.

The complete engine introspection surface is schema `1.0.0`, method
`superi.engine.introspection.get`, and event `superi.engine.introspection.changed`. It exposes one
strict full snapshot containing nested media declarations, optional exact resource accounting,
lifecycle and recovery revisions, health, canonical subsystem state, coherent playback, rendering,
and export readiness, and user-safe active failure and recovery state. `EngineIntrospectionApi`
keeps one API-local full-state revision and a separate nested media revision, suppresses equal
synchronization, and emits only full replacement state.

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

The coherent engine integration validation surface is schema `1.0.0` and method
`superi.engine.integration.validation.get`. `GetEngineIntegrationValidation` is an unfiltered
read-only query whose result carries one complete `IntegrationValidationSnapshot`. The snapshot
nests the existing strict `EngineIntrospectionSnapshot`, then adds condition, coherence, canonical
scenario reversal, exact lifecycle and recovery action tokens, revision-scoped workflow admission,
playback transport, export queue, and finding state. `IntegrationValidationApi` owns only that
immutable public observation. Live UI and test hosts construct it from their dispatcher snapshot,
while `from_fresh_engine` provides the standalone starting-engine observation used by the CLI.

## Architecture and data flow

Media capability conversion remains declaration-only. Engine registry records are projected into
API-owned stable types, equal states do not advance API revision, and a semantic change emits one
full replacement.

Complete engine conversion starts from one dispatcher-composed immutable observation. API
projection maps every internal enum to an API-owned non-exhaustive value, copies exact resource
counts, preserves lifecycle, recovery, and resource revisions, derives no second readiness policy,
and copies only the core-reviewed user-safe failure projection. A capability-only change advances
both the nested media revision and the enclosing revision. Health, workflow, failure, recovery, or
resource changes advance only the enclosing revision. Equal observations emit no event.

```text
EngineCommandDispatcher introspection snapshot
  -> EngineIntrospectionApi
  -> GetEngineIntrospection full snapshot
  -> or EngineIntrospectionChanged full replacement event
```

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

The validation query follows a separate read-only path:

```text
GetEngineIntegrationValidation
  -> EngineCommandDispatcher integration_validation_snapshot
  -> canonical EngineIntrospectionSnapshot plus exact validation extensions
  -> IntegrationValidationApi
  -> strict API-owned IntegrationValidationSnapshot
```

The facade retains one immutable public snapshot, not a mutable lifecycle, recovery, playback, or
export owner, and does not poll an endpoint. It preserves the canonical introspection projection,
exact action and recovery tokens, full replacement endpoint observations, stable unknown fallbacks,
and deterministic coherence findings from the engine result.

## Dependencies and consumers

- `serde` supplies strict snake-case and tagged serialized shapes with unknown-field rejection.
- `superi-core` supplies semantic versions, exact frame rates, and the classified error model.
- `superi-engine` supplies capability declarations, complete immutable health and readiness state,
  canonical transactional state, typed dispatch, ordered replacement events, and integration
  validation observations.
- `serde_json`, `sha2`, `superi-media-io`, and `superi-concurrency` are test dependencies for wire,
  digest, registry, and EngineControl ownership contracts.
- `superi-cli` is the first production Rust consumer of both `ScenarioApi` and
  `IntegrationValidationApi`. It never projects engine state directly.

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
- Complete introspection has one API-local outer revision. Nested media capabilities keep an
  independent revision, while lifecycle, recovery, and resource-owner revisions remain unchanged
  engine evidence.
- Public readiness exactly projects canonical engine admission. Public failure state excludes raw
  messages, sources, context frames, operation labels, failure sequences, and recovery tokens.
- Complete introspection is read-only adaptation state and cannot change project meaning or engine
  behavior.
- Scenario engine revisions advance on every successful mutation, undo, or redo; the operation log
  retains original mutation revisions.
- Integration validation is read-only. Snapshot construction and query execution advance no
  dispatcher command or event sequence and cannot change scenario, lifecycle, recovery, playback,
  export, capability, or resource state.
- Workflow permit or denial state, lifecycle actions, recovery tokens, and endpoint observations are
  projected exactly beside the nested canonical introspection state. Unknown future variants remain
  explicit.
- `os-codecs` changes engine registry assembly but not either public schema.

## Tests and verification

The media capability contracts retain five focused behaviors: deterministic ordering and strict
round trips, silent equal synchronization, one changed full-replacement event, detailed correlated
codec rows, and default software AV1 exposure.

The engine introspection contract drives normal startup, exact resource accounting, playback and
rendering degradation, active recovery, restored workflow admission, and media changes through the
real dispatcher. It proves permanent names, strict round trips, equal-state suppression, independent
outer and nested revisions, dependent rendering and export readiness, complete replacement events,
and the absence of private path, message, operation, and context data from public JSON.

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
pixel evaluation, native presentation, or runtime export publication.

Three integration validation contracts prove schema `1.0.0`, the permanent method name, strict
request and nested result decoding, canonical capability and health reuse, all six subsystem
projections, startup, sleep, wake, and recovery action state, denied transitional workflows,
coherent rendering degradation, unaffected playback, dependent rendering and export denial, and
safe active failure projection through the same typed accessors available to UI and test clients.
They do not claim a wire server, production UI, worker polling, platform rendering, or long-session
recovery soak.

## Current status and risks

The API now has four substantive surfaces, but it is still far from the promised unified
editor API. Engine introspection gives clients a coherent adaptation view without adding mutation
authority, and integration validation extends that same state with precise action and endpoint
evidence. Scenario schema 1 is deliberately narrow and fixed to one canonical edit. Its reference
state proves transactional control semantics, not production timeline, graph, or media ownership.

Integration validation schema 1 provides one coherent read-only state for CLI, UI, and tests, but
it remains an in-process snapshot facade. The standalone helper creates a fresh starting engine for
the CLI; a production UI must supply fresh observations from its live dispatcher. Validation does
not supply transport, subscription delivery, endpoint mutation, or background polling.

The separate media, engine introspection, integration validation, and scenario schema versions are
correct for independent surfaces. Wire framing, version negotiation, authentication, subscription
delivery, retry and idempotency policy, and cancellation remain required before remote clients
exist. Public scenario failure state is boxed to keep result errors bounded while preserving the
same serialized shape.

## Maintenance notes

Preserve strict serialization, deterministic ordering, stable permanent names, last-valid-state
errors, revision fences, transaction and event agreement, and unknown-variant handling. Engine
command changes require synchronized public projection, schema review, CLI consumer updates, and
focused JSON contracts. Do not expose packets, frames, textures, fixture bytes, or engine-private
history snapshots merely to simplify a client.
