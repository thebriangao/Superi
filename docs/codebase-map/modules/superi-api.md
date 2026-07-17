---
module_id: superi-api
source_paths:
  - open/crates/superi-api
source_hash: 7af5b8a69db0a7548cf606bdd4497680b5466b8c570ab4358efca20afa61c53f
source_files: 26
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-api` owns the transport-neutral public boundary for UI, scripting, extension, CLI, and
automation clients. Nine public slices are implemented: media capability introspection, complete
engine capability and health introspection for adaptive clients, canonical editorial scenario
control through revision-fenced typed transactions and ordered full-state events, coherent
read-only integration validation, durable project settings inspection and optimistic mutation,
project crash recovery discovery, semantic comparison, durable restoration, exact dismissal,
authored audio automation inspection and transaction execution through the full engine dispatcher,
complete authored project control through one generic revision-fenced command facade, and
asynchronous job inspection, progress, cooperative control, and ordered completion events over the
engine-owned export queue. One additive schema `1.0.0` catalog now classifies all 22 current
methods into 13 commands and nine queries, describes all eight events and nine replacement
resources, publishes the complete
error and capability vocabularies, and defines strict data-only JSON-RPC 2.0 envelopes. Wire
transport, routing, subscriptions, scripting, public job submission and typed job results, complete
C017 project state snapshots, and project database file commands remain absent. The
generic editor and job facades project the engine's existing owners without duplicating state,
history, scheduling, execution, or event ownership.

## Source inventory

- `open/crates/superi-api/Cargo.toml`: Declares production `serde`, `superi-core`, and
  `superi-engine` dependencies, enabling the engine's narrow test-support fixture for this crate's
  public contract tests, plus test-only `serde_json`, `sha2`, `superi-media-io`, and
  `superi-concurrency` for real EngineControl and recovery contracts.
- `open/crates/superi-api/src/api.rs`: Implements public media and complete engine introspection
  snapshots, strict engine-neutral projection including project, device, sleep, and wake state,
  independent revisions, query state, and full-replacement change events.
- `open/crates/superi-api/src/audio_automation.rs`: Implements strict clip-gain targets, exact
  sample times and keyframes, Read, Write, Touch, and Latch modes, every ordered mutation, complete
  replacement snapshots, canonical typed clip IDs, engine conversion, and the dispatcher-backed
  public facade.
- `open/crates/superi-api/src/commands.rs`: Defines versioned command and query classification on
  `ApiCommand`, media, engine introspection,
  integration validation, asynchronous jobs, project settings, project recovery, and audio
  automation query types,
  plus typed one-action scenario and ordered scenario, project settings, automation, compare,
  restore, dismiss, pause, resume, retry, cancel, cancel-all, and remove commands.
- `open/crates/superi-api/src/editor.rs`: Owns the strict generic project command, action, timeline,
  graph, media, clip-mix, and extension DTOs, checked engine conversion, typed history resource and
  evidence, and the dispatcher-backed apply, inspect, undo, redo, event, and persistence facade.
- `open/crates/superi-api/src/events.rs`: Defines versioned `ApiEvent`, media and engine introspection change
  events, and ordered full-replacement scenario, generic project history, project settings,
  project recovery, audio automation, and asynchronous job state events.
- `open/crates/superi-api/src/jobs.rs`: Implements canonical opaque job handles, stable work kind,
  priority, status, progress, safe failure, dependency, and complete replacement snapshots, plus the
  dispatcher-backed query and cooperative control facade, host-only nonblocking runtime poll seam,
  and ordered engine event projection. It exposes result availability only as a Boolean and retains
  runtime executors and typed artifacts below the API boundary.
- `open/crates/superi-api/src/lib.rs`: Exposes API, audio automation, command, event, project
  editor, asynchronous jobs, settings, project recovery, scenario, public schema, scripting,
  validation, and version modules.
- `open/crates/superi-api/src/project.rs`: Implements strict project setting values and mutations,
  complete replacement snapshots, and the caller-owned dispatcher facade for settings inspection,
  optimistic transactions, and ordered event draining.
- `open/crates/superi-api/src/recovery.rs`: Implements strict candidate, finding, comparison, and
  complete replacement snapshots plus one caller-owned facade that routes discover, compare,
  restore, dismiss, and ordered event draining through the engine dispatcher. It projects raw
  diagnostics into stable user-safe codes and never serializes paths or source chains.
- `open/crates/superi-api/src/scenario.rs`: Implements strict canonical action documents, public
  editorial state and graph projections, reversible operation evidence, structured failures, and
  the mutable dispatcher-backed `ScenarioApi` facade with optimistic transactions and event drain.
  Engine-backed failures retain stable component and operation labels plus last-valid state while
  deriving user-safe messages and dropping all raw context values.
- `open/crates/superi-api/src/schema.rs`: Owns the deterministic complete public catalog, schema
  descriptors, method and resource traits, typed schema discovery query, strict JSON-RPC 2.0
  request and response envelopes, safe structured errors, reviewed contexts, last-valid resource
  references, capability vocabulary, and asynchronous jobs resource registration without routing or
  runtime state ownership.
- `open/crates/superi-api/src/scripting.rs`: Placeholder for a scripting runtime.
- `open/crates/superi-api/src/validation.rs`: Strictly projects canonical engine integration state,
  nested introspection, exact lifecycle and recovery actions, workflow permits or denials, scenario
  reversal capacity, endpoint replacement state, and coherence findings through typed read-only
  accessors. It also provides the standalone starting-engine query owner used by the CLI and derives
  standalone construction failures from the shared user-safe projection.
- `open/crates/superi-api/src/version.rs`: Owns all domain, catalog, and error schema revisions plus
  permanent method and event names, including the complete `superi.jobs` vocabulary.
- `open/crates/superi-api/tests/async_jobs_contract.rs`: Proves strict handles, stable kind and
  weighted priority vocabulary, nonblocking progress and completion, every cooperative transition,
  cancel-all finality, deterministic ordering, dependency state, safe failure filtering, typed
  result non-exposure, and ordered replacement events over the real engine queue.
- `open/crates/superi-api/tests/audio_automation_contract.rs`: Covers strict canonical JSON, every
  public mutation, permanent names, typed facade behavior, complete correlated events, Touch mode,
  no-op suppression, and conversion failure atomicity through the real engine owner.
- `open/crates/superi-api/tests/media_capabilities_contract.rs`: Covers deterministic capability
  projection, strict serialization, change events, codec rows, and default registry integration.
- `open/crates/superi-api/tests/dispatcher_contract.rs`: Covers atomic public transactions,
  revision fences, strict command and event round trips, ordered full-state publication, one-unit
  undo, inspect behavior, permanent names, and legacy action compatibility.
- `open/crates/superi-api/tests/editor_contract.rs`: Locks all four generic project commands, six
  action groups, 15 timeline operations, eight graph mutations, three media mutations, four
  clip-mix mutations, and six extension mutations; proves strict decoding, pre-dispatch failure
  atomicity, one real mixed transaction, event correlation, database reload, undo, and redo.
- `open/crates/superi-api/tests/engine_introspection_contract.rs`: Drives the real engine registry,
  dispatcher, lifecycle, error coordinator, and resource arbiter through strict public query and
  replacement-event projection, independent revisions, degradation, recovery, and safe diagnostic
  serialization.
- `open/crates/superi-api/tests/integration_validation_contract.rs`: Covers strict versioned JSON,
  nested canonical introspection, exact startup, sleep, wake, and recovery action projection,
  coherent degraded workflow admission, and user-safe active failure state.
- `open/crates/superi-api/tests/project_settings_contract.rs`: Covers the real public-to-engine-to-
  project settings path, strict values and transactions, full replacement events, exact project
  revision correlation, and permanent namespaced contracts.
- `open/crates/superi-api/tests/project_recovery_contract.rs`: Covers the real public-to-engine-to-
  project recovery path, strict discover, compare, restore, dismiss, and event contracts, permanent
  names, path-shaped identity rejection, complete authored clip-mix comparison and restoration,
  active database publication, opaque candidates, and serialized exclusion of paths, SQLite
  details, and raw source text.
- `open/crates/superi-api/tests/public_schema_contract.rs`: Covers exact catalog category counts and
  names, domain versions, primitive revision, canonical ordering, deterministic strict round trips,
  invalid catalog rejection, typed JSON-RPC success and failure exclusivity, all recovery classes,
  diagnostic visibility filtering, and last-valid resource references.
- `open/crates/superi-api/tests/scenario_contract.rs`: Covers the strict canonical schema, complete
  state projection, exact undo plus redo evidence, structured last-valid-state failures, and
  negative proof that private paths and raw engine context values never serialize.

## Public surface

The public schema catalog is schema `1.0.0` at query method `superi.api.schema.get`.
`PublicApiSchemaSnapshot` records stable primitive revision 1 and JSON-RPC `2.0`, then separates 13
mutating commands from nine read-only queries and lists all eight current replacement events, nine
current replacement resources, one complete error vocabulary, and one capability vocabulary in
canonical name order. `ApiCommand` now declares method kind and schema version, `ApiEvent` declares
payload version, and `ApiResource` centrally registers the current replacement resources. The
explicit registration list is the supported public surface and is validated for duplicate names,
command and query overlap, incompatible identity, malformed names, and primitive drift.

`JsonRpcRequest`, `JsonRpcSuccessResponse`, `JsonRpcFailureResponse`, and `JsonRpcResponse` provide
strict serializable JSON-RPC 2.0 shapes with string IDs and typed payloads. They are data contracts,
not a parser, dispatcher, server, subscription channel, or retry owner. `PublicApiError` carries one
stable application code and versioned structured data with core-owned category and recoverability,
safe title and action, reviewed actionable contexts, and an optional last-valid resource reference.
Diagnostic conversion copies only `UserSafeError` and fields explicitly marked user-safe. Direct
`Error` conversion requires caller-supplied reviewed contexts and never copies raw error contexts.

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

The project settings surface is schema `1.0.0`, with methods
`superi.project.settings.get` and `superi.project.settings.transaction.execute` and event
`superi.project.settings.changed`. `ProjectSettingsSnapshot` carries project identity, the
authoritative document revision, and the complete canonical key map. Strict Boolean, integer, and
text values preserve the shared setting representation without coercion. Transactions carry one
bounded caller identity, an exact expected project revision, and ordered set or remove mutations.
The result and event both return the complete replacement snapshot.

The project recovery surface is schema `1.0.0`, with methods
`superi.project.recovery.get`, `superi.project.recovery.compare`,
`superi.project.recovery.restore`, and `superi.project.recovery.dismiss`, plus event
`superi.project.recovery.changed`. Every command carries a caller transaction identity; compare,
restore, and dismiss carry an exact catalog revision, while restore also carries the exact current
project revision. `ProjectRecoverySnapshot` exposes project and catalog revisions, opaque candidate
identities, captured revisions, and user-safe classified findings. Comparison reports typed
editorial, settings, authored clip-mix, selected-root, and graph differences. Restore and dismiss
results state whether the requested authority change occurred and return the complete replacement
snapshot.

The authored audio automation surface is schema `1.0.0`, with methods
`superi.audio.automation.get` and `superi.audio.automation.transaction.execute` and event
`superi.audio.automation.changed`. API-owned tagged values cover canonical clip-gain targets,
explicit sample coordinates and clocks, finite keyframes, Read, Write, Touch, and Latch modes, and
every ordered automation mutation. Results and events carry complete deterministic lane snapshots;
events include exact engine command, event, caller transaction, and audio automation revision
correlation.

The generic authored project surface is schema `1.0.0`, with command
`superi.project.command.execute`, event `superi.project.state.changed`, and replacement resource
`superi.project.history`. `ExecuteProjectCommand` carries one caller transaction identity, an exact
expected project revision, and apply, undo, redo, or inspect. Apply contains one bounded ordered
transaction over root selection, all 15 timeline edits, all eight graph mutations, all three
referenced-media mutations, all four clip-mix mutations, and all six durable extension mutations.
Results and events preserve engine command sequence, project revision, history depths, next action
kinds, ordered semantic evidence, and caller correlation. Complete project state snapshots remain
owned by C017 rather than this minimum history resource.

The asynchronous job surface is schema `1.0.0`, with query `superi.jobs.get`, commands
`superi.jobs.pause`, `superi.jobs.resume`, `superi.jobs.retry`, `superi.jobs.cancel`,
`superi.jobs.cancel_all`, and `superi.jobs.remove`, event `superi.jobs.changed`, and replacement
resource `superi.jobs`. `AsyncJobHandle` accepts only canonical lowercase `job:` identifiers.
Complete snapshots expose stable kind, 8:4:2:1 user-visible priority weights, every engine export
lifecycle state, attempt count, coherent unit progress, deterministic dependencies, reviewed safe
failure data, dependency failure state, result availability, retry eligibility, and finality.
Current engine projection produces only export kind at export priority, while the public enums keep
the stable general scheduling vocabulary. No public method submits work, polls the runtime, waits,
or returns a typed artifact.

## Architecture and data flow

Public schema discovery is entirely API-owned metadata over existing contracts:

```text
GetPublicApiSchema
  -> PublicApiSchemaApi
  -> explicit ApiCommand, ApiEvent, and ApiResource registrations
  -> validated canonical PublicApiSchemaSnapshot
  -> typed Rust client or superi-cli api schema
```

The catalog does not inspect private engine enums or create runtime ownership. Existing mutating
facades still dispatch to the canonical engine owners, while read-only facades retain their current
projection paths. JSON-RPC envelope types can carry those typed values later without implementing
method routing, network transport, subscription delivery, permissions, or version negotiation.

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

The production project-history path now has one strict public adapter:

```text
ProjectHistoryCommand
  -> superi-engine EngineCommandDispatcher
  -> ProjectHistoryOutcome plus optional ProjectStateChanged event
  -> ProjectEditorApi
  -> ExecuteProjectCommandResult plus optional public ProjectStateChanged event
```

The engine state contains a real `ProjectDocument`, bounded session history, and typed authored
project mutations, unlike the fixed scenario model. API-owned DTOs convert completely before one
dispatcher call and exactly one compound project transaction, so lower mutation algorithms,
history, sequencing, persistence, and event ownership remain unchanged. The API's immutable project
snapshot borrow is only the existing in-process persistence seam, not a public wire snapshot model.

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

Project settings use the full authoritative dispatcher rather than a parallel API-owned state:

```text
GetProjectSettings or ordered settings transaction
  -> ProjectSettingsApi
  -> EngineCommandDispatcher with attached ProjectCommandHistory owning ProjectDocument
  -> project-owned validation and atomic document publication
  -> complete ProjectSettingsSnapshot result
  -> optional ordered ProjectSettingsChanged replacement event
```

Inspection is read-only. A successful semantic transaction advances the project document once and
emits one event correlated to the command and caller transaction. A no-op preserves the project
revision and emits nothing. Failed or stale transactions leave project state, command sequencing,
and the event queue unchanged.

Project recovery uses the same full authoritative dispatcher rather than adapting project files
directly:

```text
Get, Compare, Restore, or Dismiss project recovery command
  -> ProjectRecoveryApi
  -> EngineCommandDispatcher with attached ProjectRecoveryCoordinator
  -> project-owned exact catalog and active database transaction
  -> strict ProjectRecoverySnapshot or semantic comparison result
  -> ordered ProjectRecoveryChanged replacement event for discover, restore, or dismiss
```

The public adapter parses only the canonical opaque `recovery-g` identity and passes exact optimistic
revisions into the engine command. Internal `FailureDiagnostic` values retain path and source-chain
evidence, while projection reconstructs a reviewed `UserSafeError` from category and recoverability
and adds only the stable recovery next-action code. Comparison emits no event. Discovery, restore,
and dismissal preserve caller transaction and engine command correlation in one complete event.

Audio automation uses a parallel facade pattern but retains audio ownership below engine:

```text
GetAudioAutomation or ordered automation transaction
  -> AudioAutomationApi
  -> EngineCommandDispatcher with attached AudioAutomationState
  -> audio-owned candidate validation and atomic publication
  -> complete AudioAutomationSnapshot result
  -> optional ordered AudioAutomationChanged replacement event
```

The API depends only on engine and converts every public mutation one for one through engine
re-exports. Canonical `clip:` identifiers are validated during JSON deserialization and again at
conversion. Inspection and semantic no-ops emit no event; stale, invalid, conversion, ownership,
and capacity failures leave the engine owner and public event stream unchanged.

Asynchronous jobs reuse the canonical dispatcher and its attached export queue:

```text
GetAsyncJobs or cooperative job control command
  -> AsyncJobsApi
  -> EngineCommandDispatcher ExecuteExportJob
  -> canonical EngineExportJobState replacement snapshot
  -> AsyncJobsResult plus ordered AsyncJobsChanged replacement events
```

Explicit query and control calls never wait for worker completion. A host control loop calls the
noncataloged `poll_runtime` seam to let the engine observe worker progress, cancellation
acknowledgement, and completion, then clients drain the same ordered dispatcher event envelopes.
The API verifies matching engine state and event revisions and maps every retained job in canonical
handle order. Executor bindings and typed results remain runtime-local; public state exposes only
`has_result` and reconstructs failure presentation from category and recoverability so raw engine
messages, contexts, paths, and source chains never serialize.

## Dependencies and consumers

- `serde` supplies strict snake-case and tagged serialized shapes with unknown-field rejection.
- `superi-core` supplies semantic versions, canonical names, stable primitive revision 1, feature
  availability, typed diagnostic fields, user-safe presentation, exact frame rates, and the
  classified error model.
- `superi-engine` supplies capability declarations, complete immutable health and readiness state,
  canonical transactional state, typed dispatch, ordered replacement events, and integration
  validation observations. It also re-exports project recovery candidate and comparison contracts,
  project setting mutations, audio automation vocabularies, and one curated editor construction
  seam, and owns the only production API-to-project settings and recovery paths, the API-to-audio
  automation command path, the generic authored project history path, and the canonical export
  queue projected as public asynchronous jobs. This public crate has no production project,
  timeline, graph, audio, or export executor dependency, and neither public facade becomes another
  state owner.
- `serde_json`, `sha2`, `superi-media-io`, and `superi-concurrency` are test dependencies for wire,
  digest, registry, and EngineControl contracts. The feature-gated engine test-support seam creates
  real file-backed recovery artifacts without adding API-to-project or API-to-timeline edges.
- `superi-cli` is the first production Rust consumer of `PublicApiSchemaApi`, `ScenarioApi`, and
  `IntegrationValidationApi`. Its schema process contract consumes the job registrations without
  reconstructing the catalog or projecting engine state directly. It does not yet expose job
  control commands.

No transport, UI, shell, scripting runtime, extension host, or closed-tier client is present.

## Invariants and operational boundaries

- Public types own their wire schema and do not expose media-I/O or GPU implementation types.
- Strict objects reject unknown fields. Non-exhaustive Rust enums require downstream fallback.
- The schema catalog is deterministic, complete for the current explicit registration list, sorted
  by permanent names, and rejects duplicates, command-query overlap, or incompatible schema and
  primitive identity.
- JSON-RPC request and response values require version `2.0`, string IDs, typed methods, and exactly
  one result or error branch. They do not imply transport, ordering, cancellation, or retry.
- Public error projection preserves all four recovery classes and actionable safe presentation.
  Raw summaries, source chains, internal and sensitive diagnostic fields, and raw context values
  never cross the schema boundary.
- Asynchronous job handles are canonical lowercase `job:` identifiers. Replacement snapshots are
  ordered by handle and carry the engine-owned revision without inventing a second queue owner.
- Public job query and controls are nonblocking on EngineControl. Pause and cancellation remain
  cooperative, resume and retry create fresh engine attempts, cancel-all targets every unfinished
  job, and removal is permitted only after finalization and dependent release.
- Public job failures contain only category, recoverability, stable safe code, title, and action.
  Typed artifacts, raw messages, contexts, paths, source chains, executor bindings, and control
  tokens remain private. `has_result` states only that the runtime retains a complete result.
- `poll_runtime` is a host integration seam, not a public method. It may emit the same ordered full
  replacement event as explicit controls but cannot become a second scheduler or transport poll
  contract.
- Inspect never changes revision or history.
- Generic project inspect requires the exact current project revision, advances only the successful
  command sequence, returns typed history state, and emits no event.
- Every generic apply converts the entire public value before dispatch and becomes one bounded
  compound transaction, one project revision, one history unit, and at most one correlated event.
- Invalid identifiers, exact time, finite numeric values, media paths, graph values, clip controls,
  extension records, empty batches, and bounds publish no partial state or event.
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
- Project settings expose only shared scalar values and complete replacement state. They do not
  expose `ProjectDocument`, SQLite, timeline, color, audio, cache, proxy, or render implementation
  types.
- Project settings inspection and mutation use the dispatcher-attached authoritative project.
  Optimistic revision conflicts, invalid keys, invalid value types, and cross-field failures publish
  no event. A successful no-op advances neither project nor event state.
- Audio automation target, mode, and mutation values use tagged snake-case schemas with strict
  unknown-field rejection. Clip IDs are canonical typed strings and times carry explicit signed
  sample plus positive integral clock fields.
- Public automation transactions map one for one to the audio-owned vocabulary through engine,
  retain the exact caller revision fence, and return complete replacement state. Inspection and
  semantic no-ops publish no event; conversion, stale, invalid, ownership, and capacity failures
  publish nothing. Automation events correlate event, command, transaction, and only the audio
  automation revision.
- Recovery commands accept only a strict opaque candidate identity. Public requests and replacement
  state never expose a path, database handle, SQLite detail, raw diagnostic message, context frame,
  or source-chain summary.
- Recovery findings preserve stable category, recoverability, user-safe code, title, action, and
  recovery-specific next action. Valid candidates remain usable beside degraded findings.
- Compare is read-only. Restore requires exact catalog and project revisions, and dismissal requires
  the exact catalog revision. Every failure before engine success leaves the public event stream and
  command correlation unchanged.

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
plus two redo actions, final revision 8, structured engine context, last-valid-state retention, and
serialized exclusion of a private missing-source path and raw context values.

Four public schema contracts prove the exact 13-command, nine-query, eight-event, and nine-resource
surface, current domain versions, catalog and error schema `1.0.0`, media schema `2.0.0`, stable
primitive revision 1, strict deterministic catalog round trips, invalid identity and duplicate
rejection, typed JSON-RPC exclusivity, all four recovery classes, user-safe diagnostic filtering,
and last-valid resource identity and revision. They include every public asynchronous job method,
event, and resource, but do not claim a transport server, dynamic method routing, subscriptions,
permissions, generated bindings, or negotiation.

Five asynchronous job integration contracts plus one exhaustive unit mapping drive the real
dispatcher-owned export queue. They prove canonical handle rejection, strict wire round trips,
stable kind and weighted priority vocabulary, nonblocking progress and ordered completion events,
pause, resume, retry, cancel, cancel-all, remove, dependency state, safe failure filtering,
deterministic handle order, and typed result non-exposure. They do not claim public submission,
runtime polling as a wire method, typed result retrieval, persistence across process restart, muxed
file publication, or subscription transport.

Three dispatcher contracts prove strict transaction and event JSON, permanent namespaced method and
event identity, failed trailing-action rollback, one-revision commit, one-unit undo, optimistic
conflict rejection, independent monotonic event order with exact originating-command correlation,
complete replacement state, inspect without an event, and compatibility action routing through the
same dispatcher.

Five generic editor contracts lock all 46 command and operation discriminants, reject guessed
fields and variants, and prove malformed ID, timebase, and non-finite conversion atomicity. A real
EngineControl fixture executes one six-action project transaction across graph, timeline, media,
clip mix, extension, and selected root state, then proves one revision, one history unit, one
correlated event, exact database reload, undo, and redo through the same public facade.

These tests do not prove wire delivery ordering, scripting, UI integration, media decoding, graph
pixel evaluation, native presentation, or runtime export publication.

Three integration validation contracts prove schema `1.0.0`, the permanent method name, strict
request and nested result decoding, canonical capability and health reuse, all six subsystem
projections, startup, sleep, wake, and recovery action state, denied transitional workflows,
coherent rendering degradation, unaffected playback, dependent rendering and export denial, and
safe active failure projection through the same typed accessors available to UI and test clients.
They do not claim a wire server, production UI, worker polling, platform rendering, or long-session
recovery soak.

Two project settings contracts drive a caller-owned full dispatcher with a real project document.
They prove schema `1.0.0`, strict JSON, complete defaults, an atomic two-key audio update, exact
project revision and command correlation, one full replacement event, and all three permanent
names. They do not claim wire delivery, database file operations, audio device reconfiguration, or
render execution.

Two project recovery contracts drive a caller-owned full dispatcher with a real active project
database and autosave namespace. They prove all four permanent method names, the permanent event
name, strict unknown-field rejection, opaque candidate identity, degraded corruption beside one
valid candidate, typed comparison including authored clip-mix state, durable restore, candidate
retention, exact later dismissal, complete replacement events, path-shaped identity rejection, and
JSON exclusion of absolute paths, SQLite messages, and raw source text.

Four audio automation contracts drive a caller-owned full dispatcher with real audio-owned state.
They prove schema `1.0.0`, all three permanent names, strict request, result, snapshot, and event
round trips, canonical clip identifiers, every supported mutation, Read, Write, Touch, and Latch
projection, Touch behavior through the facade, no-op suppression, exact revision correlation, and
conversion failure atomicity. They do not claim durable project storage, hardware control input,
wire delivery, physical audio output, or additional automation targets.

## Current status and risks

The API now has nine substantive domain surfaces plus one complete discovery catalog and strict
wire grammar, including one unified generic authored project command and one asynchronous job
control surface. Engine introspection gives
clients a coherent adaptation view without adding mutation
authority, and integration validation extends that same state with precise action and endpoint
evidence. Project settings retain exact durable scalar meaning, and project recovery now exposes one
complete strict discover, compare, restore, and dismiss surface without leaking file identity. The
API still does not expose project file open or save, complete C017 project snapshots, public job
submission or typed results, subscriptions, permissions, generated bindings, a scripting runtime,
or CLI editor execution.
Scenario schema 1 remains deliberately narrow and fixed to one canonical edit. Its reference
state proves transactional control semantics, not production timeline, graph, or media ownership.
Authored clip-gain automation is a substantive strict transaction and event surface over the engine
owner, but schema 1 does not persist lanes or address pan, sends, buses, effects, or plugins.
The generic editor facade now reaches the engine's project command-history owner for apply, undo,
redo, inspection, semantic evidence, and correlated replacement events. It intentionally exposes
only minimum history replacement state until C017 and leaves project file commands, live wire
routing, subscription delivery, scripting, and broader automation adaptation to later checkpoints.

Asynchronous job control is a substantive strict projection over the canonical engine export queue.
It keeps user-visible work responsive through nonblocking state replacement and cooperative
controls, but the queue and its retained typed results remain process-local, and another owner must
submit prepared export executors or publish final files.

Integration validation schema 1 provides one coherent read-only state for CLI, UI, and tests, but
it remains an in-process snapshot facade. The standalone helper creates a fresh starting engine for
the CLI; a production UI must supply fresh observations from its live dispatcher. Validation does
not supply transport, subscription delivery, endpoint mutation, or background polling.

The separate media, engine introspection, integration validation, scenario, catalog, and error
schema versions are correct for independent surfaces. Data-only JSON-RPC framing now exists, while
dynamic routing, version negotiation, authentication, subscription delivery, retry and idempotency
policy remain required before remote clients exist. Public scenario failure state
is boxed to keep result errors bounded while preserving the same serialized shape and now removes
raw context values.

## Maintenance notes

Preserve strict serialization, deterministic ordering, stable permanent names, last-valid-state
errors, revision fences, transaction and event agreement, and unknown-variant handling. Keep
the explicit catalog synchronized with every new command, query, event, resource, schema version,
and CLI discovery assertion. Preserve the safe error conversion boundary and require negative leak
proof before adding context fields. Do not treat JSON-RPC data values as a transport implementation.
Keep
project setting keys and validation project-owned, resolve them only in engine, and retain the API's
dependency on engine rather than adding a direct project edge. Engine command changes require
synchronized public projection, schema review, CLI consumer updates when applicable, and focused
JSON contracts. Do not expose packets, frames, textures, fixture bytes, database handles, or
engine-private history snapshots merely to simplify a client.
Keep audio automation wire values API-owned, tagged, strict, canonical, and mapped one for one
through engine re-exports. Preserve complete replacement snapshots, permanent names, isolated
revision correlation, and the API-to-engine-to-audio dependency direction. Adding a target or
mutation requires a schema review, both conversion directions, engine ownership, and real consumer
proof.
Keep recovery candidate identities opaque, project paths and raw diagnostics private, method and
event names permanent, catalog and project revision fences explicit, and replacement events fully
correlated to their engine command. Any new recovery field requires strict schema review and
negative serialization proof before it crosses this boundary.
Keep generic editor DTOs API-owned and strict, conversions complete before dispatch, and all
authored apply operations inside one engine-owned compound transaction. Add every new lower editor
operation to the explicit parity contract, public evidence projection, schema catalog, CLI schema
consumer, and real dispatcher proof without adding direct API dependencies on lower domain crates.

Keep asynchronous job handles canonical, all query and control paths routed through the attached
engine export queue, and every event a verified full replacement from the ordered engine envelope.
Do not catalog host polling, executor submission, waiting, or typed result access. Preserve exact
status mapping, deterministic handle and dependency ordering, unit progress, cooperative control,
and negative failure-leak proof whenever the engine queue evolves.
