---
module_id: superi-api
source_paths:
  - open/crates/superi-api
source_hash: a127af8011afcc0f05a70a2fd68a9f1f551536253301960c3ab3ba2f1de4aa30
source_files: 37
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-api` owns the transport-neutral public boundary for UI, scripting, extension, CLI, and
automation clients. Thirteen public slices are implemented: stateless API and optional project
version negotiation, media capability introspection, complete
engine capability and health introspection for adaptive clients, canonical editorial scenario
control through revision-fenced typed transactions and ordered full-state events, coherent
read-only integration validation, durable project settings inspection and optimistic mutation,
project crash recovery discovery, semantic comparison, durable restoration, exact dismissal,
authored audio automation inspection and transaction execution through the full engine dispatcher,
complete authored project control, deterministic local scripting, and complete editor replacement
state through one generic revision-fenced facade, and asynchronous job inspection, progress, cooperative control, and ordered
completion events over the engine-owned export queue, plus bounded ordered delivery for the complete
public event vocabulary. The additive schema `1.3.0` catalog classifies all 28 current methods into
16 commands and 12 queries, describes all eight events and 11 replacement resources, publishes the
complete error, capability, and permission vocabularies, and defines strict data-only JSON-RPC 2.0
envelopes. A host-injected nonserializable permission context denies protected filesystem, plugin,
and destructive operations by default, derives exact requirements from complete typed payloads,
and authorizes before conversion or dispatch.
The event stream adds API-owned registration, polling, backpressure, replay, and reconnect behavior
without duplicating engine event ownership. The local `superi-json` runtime validates exact
digest-bound source, aggregates nested permission requirements, and interprets its closed step
vocabulary only through the same project editor facade. Network transport, dynamic routing, push
delivery, public job submission and typed job results, and project database file commands remain
absent. The generic editor, scripting, and job facades project the engine's existing owners without duplicating
state, history, scheduling, execution, or event ownership.
The same canonical method, event, and resource registry now drives deterministic generated
TypeScript declarations, strongly typed maps, JSON-RPC envelopes, and a transport-neutral client.
The generator adds no hidden runtime, state owner, Tauri dependency, network path, or application UI.

## Source inventory

- `open/crates/superi-api/Cargo.toml`: Declares production `serde`, `serde_json`, `sha2`, `superi-core`, and
  `superi-engine` dependencies, enabling the engine's narrow test-support fixture for this crate's
  public contract tests. The opt-in `typescript-bindings` feature adds exact Specta 1.0.5 only for
  deterministic type derivation. Test-only `superi-media-io` and `superi-concurrency` support real
  registry and EngineControl contracts. The existing engine test-support seam supplies the narrow
  real persistence, integrity, autosave, and recovery proof without a direct API-to-project edge.
- `open/crates/superi-api/src/api.rs`: Implements public media and complete engine introspection
  snapshots, strict engine-neutral projection including project, device, sleep, and wake state,
  independent revisions, query state, and full-replacement change events.
- `open/crates/superi-api/src/audio_automation.rs`: Implements strict clip-gain targets, exact
  sample times and keyframes, Read, Write, Touch, and Latch modes, every ordered mutation, complete
  replacement snapshots, canonical typed clip IDs, engine conversion, and the dispatcher-backed
  public facade with pre-dispatch destructive-removal authorization.
- `open/crates/superi-api/src/commands.rs`: Defines versioned command and query classification on
  `ApiCommand`, mandatory permission mode, possible permission kinds, and exact requirement
  derivation for media, engine introspection, integration validation, asynchronous jobs, complete
  editor state, project settings, project recovery, audio automation, scenario, cancellation,
  removal, restore, and dismissal contracts.
- `open/crates/superi-api/src/editor.rs`: Owns the strict generic project command, action, timeline,
  graph, media, clip-mix, and extension DTOs, checked engine conversion, typed history resource and
  evidence, script prevalidation, and the dispatcher-backed apply, inspect, undo, redo, script,
  event, state-query, and persistence facade.
- `open/crates/superi-api/src/event_stream.rs`: Owns canonical stream and subscriber identities,
  finite retention and registration bounds, independent public sequencing, the closed eight-event
  union, command and observation correlation, immutable replay records, caller-held cursors,
  non-destructive polling, explicit permission-free classification for its three typed control
  methods, eviction and restart gaps, reset barriers, and the complete authoritative
  replacement-resource manifest.
- `open/crates/superi-api/src/events.rs`: Defines versioned `ApiEvent`, media and engine introspection change
  events, and ordered full-replacement scenario, generic project history, project settings,
  project recovery, audio automation, and asynchronous job state events.
- `open/crates/superi-api/src/jobs.rs`: Implements canonical opaque job handles, stable work kind,
  priority, status, progress, safe failure, dependency, and complete replacement snapshots, plus the
  dispatcher-backed query and cooperative control facade, host-only nonblocking runtime poll seam,
  and ordered engine event projection. It exposes result availability only as a Boolean and retains
  runtime executors and typed artifacts below the API boundary.
- `open/crates/superi-api/src/lib.rs`: Exposes API, audio automation, command, event stream, event,
  project editor, complete editor state, asynchronous jobs, version negotiation, permissions,
  settings, project recovery, scenario, public schema, local scripting, validation, and version
  modules, plus the feature-gated TypeScript renderer.
- `open/crates/superi-api/src/negotiation.rs`: Implements the permission-free stateless
  `superi.api.version.negotiate` query, strict bounded ascending client offers, highest common
  canonical SemVer and primitive selection, typed incompatibility dimensions, complete server
  support disclosure, and an independent API-owned projection of project compatibility.
- `open/crates/superi-api/src/permissions.rs`: Owns validated serializable rules and requirements,
  component-aware project-relative, Unix, Windows drive, and Windows UNC scopes, exact and recursive
  matching, plugin identity and capability delegation ceilings, destructive-operation scopes,
  explicit deny precedence, safe denial errors, and the nonserializable host authority context.
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
  references, capability and permission vocabularies, per-method permission metadata, local
  scripting, asynchronous jobs, event subscription, and complete editor-state resource registration
  without dynamic routing or engine state ownership.
- `open/crates/superi-api/src/scripting.rs`: Implements the bounded `superi-json` language, strict
  duplicate-key and nesting validation, exact-source SHA-256 checks, canonical script identities,
  one initial revision fence, a closed project-command and editor-state step union, payload-derived
  permission preflight, ordered typed traces, safe stopped results, and interpretation exclusively
  through `ProjectEditorApi`.
- `open/crates/superi-api/src/state.rs`: Owns the strict ten-domain complete editor replacement
  snapshot, canonical authored JSON resource envelopes, bounded extension descriptors, exact audio
  routing and continuity projection, public project identity and semantic hash accessors for script
  traces, and the dispatcher-backed read-only query facade.
- `open/crates/superi-api/src/typescript.rs`: Collects every canonical request, response, event,
  resource, and structured error type through Specta; validates exact registry parity; preserves
  reviewed core wire encodings; and renders recursive JSON, strongly typed public maps, JSON-RPC
  envelopes, and a transport-neutral client without filesystem access or run-specific identity.
- `open/crates/superi-api/src/validation.rs`: Strictly projects canonical engine integration state,
  nested introspection, exact lifecycle and recovery actions, workflow permits or denials, scenario
  reversal capacity, endpoint replacement state, and coherence findings through typed read-only
  accessors. It also provides the standalone starting-engine query owner used by the CLI and derives
  standalone construction failures from the shared user-safe projection.
- `open/crates/superi-api/src/version.rs`: Owns all domain, catalog, and error schema revisions plus
  permanent method and event names, including catalog revision `1.3.0`, its `1.0.0` through `1.3.0`
  release table, scripting schema `1.0.0`, `superi.project.script.run`, the version negotiation
  method, the complete `superi.jobs` vocabulary, the editor-state query, and event subscription
  open, close, and poll methods.
- `open/crates/superi-api/tests/async_jobs_contract.rs`: Proves strict handles, stable kind and
  weighted priority vocabulary, nonblocking progress and completion, every cooperative transition,
  cancel-all finality, deterministic ordering, dependency state, safe failure filtering, typed
  result non-exposure, ordered replacement events over the real engine queue, and fail-closed
  cancellation with unchanged state and events.
- `open/crates/superi-api/tests/audio_automation_contract.rs`: Covers strict canonical JSON, every
  public mutation, permanent names, typed facade behavior, complete correlated events, Touch mode,
  no-op suppression, conversion failure atomicity, and fail-closed removal with unchanged state and
  events through the real engine owner.
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
- `open/crates/superi-api/tests/event_stream_contract.rs`: Proves strict bounded configuration and
  identities, duplicate and exhausted registration, closed-union catalog parity, public sequence
  order, command and observation correlation, independent and idempotent replay, batch caps,
  eviction and restart recovery, complete resynchronization metadata, post-barrier delivery, close
  isolation, strict JSON, and real editor, introspection, and asynchronous job producers, with the
  editor producer granted only its exact generated project-relative media targets.
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
  details, raw source text, and denied restore or dismissal with unchanged files, state, and events.
- `open/crates/superi-api/tests/permissions_contract.rs`: Proves filesystem component matching,
  traversal and sibling resistance, platform separation, exact and recursive scopes, deny
  precedence, plugin identity and delegation ceilings, destructive-operation scoping, safe errors,
  deny-all defaults, unchanged project and scenario state on denial, and authorized event parity.
- `open/crates/superi-api/tests/public_schema_contract.rs`: Covers exact catalog category counts and
  names, domain versions, primitive revision, canonical ordering, deterministic strict round trips,
  invalid catalog rejection, typed JSON-RPC success and failure exclusivity, all recovery classes,
  diagnostic visibility filtering, last-valid resource references, the complete permission
  vocabulary, and exact metadata for every method.
- `open/crates/superi-api/tests/scenario_contract.rs`: Covers the strict canonical schema, complete
  state projection, exact undo plus redo evidence, structured last-valid-state failures, and
  negative proof that private paths and raw engine context values never serialize.
- `open/crates/superi-api/tests/scripting_contract.rs`: Proves permanent runtime identity, every
  bound, strict parsing, exact-source integrity, deterministic typed traces, real mutation and state
  inspection, media identity and fingerprint preservation, SQLite reload, project integrity,
  recovery comparison and restoration, initial and later conflict behavior, committed-prefix
  visibility, and nested permission denial before dispatch.
- `open/crates/superi-api/tests/editor_state_contract.rs`: Covers the permanent query and resource,
  strict deterministic ten-domain state, exact canonical payload identity, explicit optional
  runtime owners, project color intent, and no-poll playback and export observations.
- `open/crates/superi-api/tests/typescript_bindings_contract.rs`: Proves two-run deterministic
  rendering, exact coverage of every canonical method, event, and resource, required project,
  event, AI, error, map, and client declarations, and absence of checkout paths and timestamps.
- `open/crates/superi-api/tests/version_negotiation_contract.rs`: Proves the permanent query and
  schema, permission-free classification, strict offer ordering and bounds, SemVer build and
  pre-release behavior, canonical highest-common selection, both incompatibility dimensions,
  independent project outcomes, and strict JSON-RPC 2.0 framing.

## Public surface

The public schema catalog is schema `1.3.0` at query method `superi.api.schema.get`.
`PublicApiSchemaSnapshot` records stable primitive revision 1 and JSON-RPC `2.0`, then separates 16
mutating commands from 12 read-only queries and lists all eight current replacement events, 11
current replacement resources, one complete error vocabulary, one capability vocabulary, and one
permission vocabulary in canonical name order. `ApiCommand` declares method kind, schema version,
permission requirement mode, every possible permission kind, and exact requirement derivation with
no default classification. `ApiEvent` declares payload version, and `ApiResource` centrally
registers the current replacement resources. The explicit registration list is the supported
public surface and is validated for duplicate names, command and query overlap, incompatible
identity, malformed names, and primitive drift.

`JsonRpcRequest`, `JsonRpcSuccessResponse`, `JsonRpcFailureResponse`, and `JsonRpcResponse` provide
strict serializable JSON-RPC 2.0 shapes with string IDs and typed payloads. They are data contracts,
not a parser, dynamic dispatcher, or network server. `PublicApiError` carries one
stable application code and versioned structured data with core-owned category and recoverability,
safe title and action, reviewed actionable contexts, and an optional last-valid resource reference.
Diagnostic conversion copies only `UserSafeError` and fields explicitly marked user-safe. Direct
`Error` conversion requires caller-supplied reviewed contexts and never copies raw error contexts.

`ApiPermissionContext` is host authority and is deliberately not serializable. Its typed rules can
allow or deny exact or recursive filesystem scopes, exact or all plugin identities with bounded
capability delegation, and individual destructive operations. Deny rules override matching allows,
and no matching allow denies the protected request. Filesystem matching is lexical and
component-aware across project-relative, Unix, Windows drive, and Windows UNC syntax. It does not
claim symlink confinement or replace revalidation and operating-system containment at the actual I/O
owner. Permission failures expose only the principal, method, permission kind, and operation code,
never a target path, plugin identity, or delegated capability.

With feature `typescript-bindings`, `render_typescript_bindings` derives the same complete surface
into deterministic TypeScript. `SuperiMethodMap` binds each permanent method to its exact request
and response, `SuperiEventMap` binds ordered replacement events, and `SuperiResourceMap` binds
durable replacement state. `SuperiTransport` and `SuperiClient` are transport-neutral typed seams;
they start no process, open no socket, own no engine or project state, and do not assume Tauri.
Core semantic versions remain strings, structured error and availability values remain exact string
unions, diagnostic signed and unsigned integers remain decimal strings, recursive canonical JSON
remains JSON, and ordinary API integer fields retain their current JSON number representation.

The version negotiation surface is independent schema `1.0.0` at query
`superi.api.version.negotiate`. `NegotiateApiVersion` requires nonempty bounded client API offers in
strictly ascending SemVer precedence and nonzero primitive revisions in strictly ascending numeric
order. Equal-precedence build variants, duplicates, descending offers, empty lists, and unknown
fields fail before execution. `VersionNegotiationApi` selects the highest canonical server API
release with equal precedence and the highest exact primitive revision, reports both missing
dimensions when needed, and evaluates an optional complete project descriptor independently through
the engine's curated reexport of the project-owned compatibility table. It performs no I/O,
runtime downgrade, project open, state change, event, resource publication, or permission check.

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
available through the complete editor state resource rather than this minimum history resource.

The complete editor state surface is schema `1.0.0`, method `superi.editor.state.get`, and
replacement resource `superi.editor.state`. `EditorStateSnapshot` exposes explicit project,
timeline, graph, media, audio, color, effect, AI, playback, and export roots from one engine command
sequence. Canonical timeline, graph, and clip-mix documents retain exact canonical JSON bytes,
lengths, and SHA-256 identities. Audio state retains integral sample clocks, ordered semantic
channels, explicit routing or mute targets, and exact continuity seams. Detached, attached without
an observation, pending, and observed runtime owners remain distinct, while packets, frames,
samples, textures, extension payload bytes, and export result bytes remain private.

The local scripting surface is schema `1.0.0` at command `superi.project.script.run`. One request
carries exact UTF-8 `superi-json` source and its required lowercase SHA-256 digest. The strict
program names language and version, one canonical script identity, the expected initial project
revision, and one to 256 ordered steps drawn only from `superi.project.command.execute` and
`superi.editor.state.get`. Source is capped at 1,048,576 bytes, identifiers at 128 bytes, and JSON
nesting at 128 levels. The result preserves language, source, project identity, initial and final
revision and semantic hash evidence, ordered typed successful steps, optional user-safe failure,
the failed index, completed, rejected, or stopped status, and whether any prefix effect committed.
Source loading and filesystem confinement remain caller-owned.

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

The ordered event delivery surface is schema `1.0.0`, with commands
`superi.events.subscription.open` and `superi.events.subscription.close`, query
`superi.events.subscription.poll`, and control resource `superi.events.subscription`.
`EventStreamApi` owns one process-lifetime stream identity, one bounded deque of complete immutable
records, one bounded subscriber registry, and one independent nonzero public sequence. The closed
`PublicApiEvent` union covers exactly the eight cataloged replacement events and validates event to
snapshot revision agreement before publication. Command events retain the source event sequence,
engine command sequence, and caller transaction identity, while observed engine and media changes
retain their authoritative observation revision. JSON-RPC request identifiers are not event
correlation identifiers.

Subscribers keep their own cursors. Polling never advances or destroys server state, so retrying the
same cursor returns the same complete suffix and multiple subscribers progress independently. The
server caps each result by both the caller request and configured batch bound, evicts only complete
oldest records at the retention bound, and rejects future cursors. An evicted cursor or changed
stream identity returns no partial events. Instead it returns `ResyncRequired` with the exact gap
reason, current stream, reset barrier, and all ten authoritative state resources plus their stable
refresh method and query or command kind. The subscription control resource is excluded from that
manifest because clients reestablish it through open, close, and poll. Project history and scenario
state correctly name their typed `Inspect` command paths; the other eight state resources name
read-only queries.

## Architecture and data flow

Public schema discovery is entirely API-owned metadata over existing contracts:

```text
GetPublicApiSchema
  -> PublicApiSchemaApi
  -> explicit ApiCommand, ApiEvent, and ApiResource registrations
  -> permission mode and possible-kind metadata for every method
  -> validated canonical PublicApiSchemaSnapshot
  -> typed Rust client or superi-cli api schema
```

The catalog does not inspect private engine enums or create runtime ownership. Existing mutating
facades still dispatch to the canonical engine owners after permission authorization, while
read-only facades retain their current projection paths. JSON-RPC envelope types can carry those
typed values later without implementing method routing, network transport, push delivery,
or authentication.

Version negotiation is one stateless composition over immutable support contracts:

```text
NegotiateApiVersion strict client offers
  -> VersionNegotiationApi highest common API and primitive selection
  -> optional superi-project compatibility negotiation through the engine editor seam
  -> typed selection, all missing dimensions, server support, and optional project result
```

Ordered public delivery remains downstream of every existing typed producer:

```text
ProjectEditorApi, AsyncJobsApi, ProjectRecoveryApi, ProjectSettingsApi,
AudioAutomationApi, ScenarioApi, MediaCapabilitiesApi, or EngineIntrospectionApi
  -> one validated PublicApiEvent variant
  -> EventStreamApi independent public sequence and bounded immutable record
  -> PollEvents with caller-held stream identity, subscriber identity, and cursor
  -> complete EventBatch
     or ResyncRequired with reset barrier and replacement-resource manifest
```

The broker does not alter facade drains or engine queues. Source events remain owned and sequenced
by their current producers; the public sequence orders only delivery across the unified event
vocabulary. A stream restart is detectable because a new owner uses a new stable stream identity.
After a gap, clients refresh the complete manifest, adopt the returned barrier, and can receive
events published after that barrier without ambiguity.

Protected command flow is shared by every permission-bound facade:

```text
complete typed ApiCommand payload
  -> payload-derived canonical ApiPermissionRequirements
  -> host-injected ApiPermissionContext with deny precedence
  -> reject without conversion, dispatch, sequence, state, file, or event change
     or continue through the existing canonical engine owner
```

Current filesystem requirements are scenario import and generic project media path mutation. Plugin
requirements cover durable extension state, lifecycle, and explicit capability delegation.
Destructive requirements cover asynchronous job cancellation and removal, recovery restore and
dismissal, and audio automation lane or keyframe removal. Unprotected siblings remain explicitly
classified with no requirements instead of inheriting authority from a method-wide switch.

TypeScript generation shares the exact registration lists rather than rebuilding catalog names:

```text
canonical method, event, and resource registry
  -> validated PublicApiSchemaSnapshot
  -> feature-gated Specta type collection and reviewed core wire shadows
  -> deterministic TypeScript declarations and typed public maps
  -> committed artifact check and strict frontend smoke consumer
```

The renderer is pure and filesystem-free. The repository tool owns publication and drift checks,
and the frontend consumer owns only compile-time and transport-neutral client use.

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

The production project owner now has one strict facade for authored commands and complete state:

```text
ExecuteProjectCommand or GetEditorState
  -> ProjectEditorApi::execute through sealed ProjectEditorRequest
  -> superi-engine EngineCommandDispatcher
  -> ProjectHistoryOutcome plus optional ProjectStateChanged event
     or one eventless immutable EditorStateSnapshot
```

The engine state contains a real `ProjectDocument`, bounded session history, and typed authored
project mutations, unlike the fixed scenario model. API-owned mutation DTOs convert completely
before one dispatcher call and exactly one compound project transaction, so lower mutation
algorithms, history, sequencing, persistence, and event ownership remain unchanged. The read-only
request preserves the authoritative selected revision and projects only immutable authored data
plus observations already cached by the dispatcher. It does not poll workers, discover recovery
files, or create another mutable state owner. The facade's immutable project snapshot borrow remains
only the existing in-process persistence seam.

Local scripts compose that same facade rather than introducing a scripting mutation model:

```text
exact UTF-8 source plus required SHA-256
  -> strict bounded duplicate-key-aware superi-json parse and validation
  -> one initial project revision fence and complete nested permission preflight
  -> ProjectEditorApi sealed RunProjectScript request
  -> each supported step through the existing ExecuteProjectCommand or GetEditorState path
  -> ordered typed trace with initial and final semantic hash evidence
```

An initial revision conflict rejects every step. A later command failure stops the remaining suffix
and returns the complete last valid project evidence while preserving any earlier independently
committed public commands and their ordinary events. Repeating source against an identical complete
initial runtime state produces the same interpretation. The runtime does not evaluate JavaScript,
load modules, open files, create a hidden transaction, rewrite project schemas, suppress inner
events, or claim whole-script atomicity.

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
- `serde_json` supplies bounded local source decoding and canonical typed JSON values, while the
  runtime's custom seed rejects duplicate object members and excessive nesting before execution.
- Exact `sha2` 0.10.9 binds every script request to the caller's exact UTF-8 source bytes.
- Exact optional Specta 1.0.5 derives TypeScript declarations only when the
  `typescript-bindings` feature is selected. It does not enter the default API or engine graph.
- `superi-core` supplies semantic versions, canonical names, stable primitive revision 1, feature
  availability, typed diagnostic fields, user-safe presentation, exact frame rates, and the
  classified error model.
- `superi-engine` supplies capability declarations, complete immutable health and readiness state,
  canonical transactional state, typed dispatch, ordered replacement events, and integration
  validation observations. It also re-exports project recovery candidate and comparison contracts,
  project setting mutations, audio automation vocabularies, and one curated editor construction
  seam. It owns the production API-to-project settings and recovery paths, the API-to-audio
  automation command path, the generic authored project history and immutable editor-state paths,
  and the canonical export queue projected as public asynchronous jobs. This public crate has no
  production project, timeline, graph, audio, or export executor dependency, and neither public
  facade becomes another state owner.
- `superi-media-io` and `superi-concurrency` are test dependencies for registry and EngineControl
  contracts. The feature-gated engine test-support seam exercises real project persistence,
  integrity, media, autosave, and recovery owners for the scripting contract without adding any
  direct API-to-project or API-to-timeline edge.
- `superi-cli` is the first production Rust consumer of `PublicApiSchemaApi`, `ScenarioApi`, and
  `IntegrationValidationApi`. Its schema process contract consumes the job registrations without
  reconstructing the catalog or projecting engine state directly. Its scenario consumer binds one
  exact read grant for the resolved canonical fixture path and no repository-wide authority. It does
  not yet expose job control commands.
- `superi-api-bindings` is the repository-only generator and drift-check consumer. The committed
  artifact is consumed by `ci/frontend-smoke` through strict TypeScript checking and a production
  Vite bundle.

No network transport, application UI, shell source loader, extension host, or closed-tier client is
present. The local script interpreter is an in-process public API consumer, not a process sandbox
or general-purpose language runtime. The event stream is a transport-neutral in-process owner that a later server can
expose, and the frontend smoke package is a real generated-contract consumer, not an application.

## Invariants and operational boundaries

- Public types own their wire schema and do not expose media-I/O or GPU implementation types.
- Strict objects reject unknown fields. Non-exhaustive Rust enums require downstream fallback.
- The schema catalog is deterministic, complete for the current explicit registration list, sorted
  by permanent names, and rejects duplicates, command-query overlap, or incompatible schema and
  primitive identity.
- Every `ApiCommand` must classify permission mode and possible kinds explicitly and derive a
  canonical requirement set from its complete typed payload. Protected facades authorize that set
  before conversion or dispatch, and constructors without a host context deny all protected
  operations.
- Permission evaluation is requirement-complete and fail closed. Every requirement needs a matching
  allow, any matching deny wins, recursive filesystem scopes match component boundaries rather than
  string prefixes, plugin scopes match canonical identities, and delegation cannot exceed the
  allowed capability ceiling.
- Permission denial advances no command sequence, project or subsystem revision, history, durable
  recovery file, or event stream. Safe errors omit filesystem targets, plugin identities, delegated
  capabilities, and policy rule contents.
- Filesystem authorization is a logical public API boundary over typed lexical paths. Actual I/O
  owners must still confine handles, resolve symlink behavior, and apply platform sandbox policy.
- TypeScript collection reuses those exact registration lists and validates method, event, and
  resource parity against a fresh catalog before rendering. New public entries cannot silently
  remain Rust-only.
- Generated output contains no timestamp or checkout identity and rejects forbidden Unicode dash
  characters. Core custom serialization is reflected explicitly rather than inferred from Rust
  memory layout, and wide diagnostic integers remain lossless decimal strings.
- The generated client is transport-neutral. It cannot own project state, bypass the public command
  surface, introduce a hidden AI artifact channel, or imply Tauri, network, retry, or subscription
  behavior that the supplied transport does not implement.
- JSON-RPC request and response values require version `2.0`, string IDs, typed methods, and exactly
  one result or error branch. Event ordering and replay come only from the event stream contracts,
  not JSON-RPC IDs.
- Stream and subscriber identities use bounded canonical lowercase ASCII and reject malformed wire
  values. Retention, registration, and batch bounds are nonzero and never exceed 4096.
- Public delivery sequences are nonzero, strictly increasing, independent from every engine event
  sequence, and fail terminally instead of wrapping. Retained records are immutable and evicted only
  as complete oldest units.
- Subscriber cursors are caller-held. Polling is non-destructive and idempotent for the same stream,
  subscriber, cursor, and limit. Closing one subscriber cannot advance, close, or consume another.
- Poll never returns an incomplete suffix. Eviction and stream restart return an explicit resync
  result with the latest sequence as reset barrier and the complete ten-resource state manifest.
  Events published after the barrier remain replayable.
- The public event union has exactly one typed variant per catalog event and no untyped fallback.
  Command correlation retains source event sequence, command sequence, and transaction identity;
  observation correlation retains the producing snapshot revision.
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
- Script source is exact digest-bound UTF-8 and must pass the source-size, duplicate-key, depth,
  language, identity, step-count, method, and nested request bounds before interpretation.
- Script method support is closed and versioned. Every step reuses the existing public request and
  response type, engine dispatcher, revision fence, permission boundary, event behavior, semantic
  hash, and persistence owner instead of introducing a parallel command path.
- The initial project revision fence runs before the first step. A later failure stops the suffix
  but cannot roll back or hide an earlier public command that already committed. The result makes
  committed-prefix visibility explicit through status, completed records, final revision, final
  semantic hash, and `effects_committed`.
- Nested permission requirements are parsed and unioned before any script step dispatch. Each inner
  command retains its ordinary authorization as defense in depth, and denial changes no command,
  project, file, recovery, or event state.
- Invalid identifiers, exact time, finite numeric values, media paths, graph values, clip controls,
  extension records, empty batches, and bounds publish no partial state or event.
- Complete editor state is captured by one dispatcher command against one selected project
  revision. The query emits no event and performs no recovery discovery, playback execution, export
  polling, or authored mutation.
- Canonical authored documents preserve exact bytes, lengths, and digests. The public result never
  includes bulk packets, frames, samples, textures, extension payloads, or typed export artifacts.
- Audio projection preserves integral sample clocks, ordered source and destination channel meaning,
  explicit routing and mute intent, and sample-exact continuity evidence. Retimed material that
  cannot be audited without rounding is explicit unsupported state rather than guessed continuity.
- Optional recovery, automation, playback, and export owners use explicit availability states.
  Missing attachment, attached without observation, pending work, and latest observed state cannot
  collapse into one ambiguous null value.
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

Four public schema contracts prove the exact 16-command, 12-query, eight-event, and 11-resource
surface, current domain versions, catalog schema `1.3.0`, error schema `1.0.0`, media schema `2.0.0`,
stable primitive revision 1, strict deterministic catalog round trips, invalid identity and
duplicate rejection, typed JSON-RPC exclusivity, all four recovery classes, user-safe diagnostic
filtering, last-valid resource identity and revision, the complete permission vocabulary, and exact
permission metadata for all 28 methods. They include local scripting, version negotiation, every
public asynchronous job and event
subscription method and resource, but do not claim a network transport server, dynamic method
routing, push delivery, or authentication.

Four version negotiation contracts prove request schema `1.0.0`, permanent method identity,
permission-free query classification, strict offer validation, canonical SemVer precedence despite
client build metadata, pre-release ordering, highest common API and primitive selection, both typed
missing dimensions, complete server support, independent project migration and future-format
results, strict unknown-field rejection, and JSON-RPC 2.0 request and response round trips.

Six event stream contracts plus one sequence-exhaustion unit test prove strict identities and wire
values, finite configuration, duplicate and capacity rejection, exact eight-event union parity,
strictly increasing independent public order, real command and observation correlation, immutable
record round trips, independent subscribers, retry idempotence, request and server batch caps,
whole-record retention, explicit eviction and restart gaps, complete ten-resource reconnect
metadata, reset barriers, post-barrier delivery, future-cursor rejection, and isolated close. The
integration tests publish events produced by real project editor, engine introspection, and
asynchronous job lifecycle paths. They do not claim network I/O, push delivery, persisted replay
across process lifetimes, or authorization.

Five focused permission contracts prove canonical and deserialized rule validation, deterministic
requirements, explicit deny precedence, fail-closed defaults, component-aware filesystem scopes,
project-relative traversal rejection, sibling-prefix resistance, platform separation, plugin
identity and delegation ceilings, destructive operation matching, safe errors, unchanged project
and scenario replacement state and events on denial, and authorized parity. The destructive-owner
integration contracts separately prove denied job cancellation, recovery restore and dismissal, and
audio automation removal preserve their complete state, files, and event streams.

Six asynchronous job integration contracts plus one exhaustive unit mapping drive the real

The TypeScript binding contract renders twice and proves exact text equality, every canonical name,
the generic project request and result, project replacement event, editable AI state, structured
error, method, event, and resource maps, and the typed client. The repository tool adds current,
missing, stale, nonmutating-check, deterministic-generation, and idempotent-publication proof. The
strict frontend smoke package imports the committed artifact, typechecks project command, event,
resource, AI, negotiation request and response, transport, and client use, and includes that
consumer in the production Vite bundle.

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

Five scripting contracts lock the `superi-json` language, schema, method, canonical file suffix,
source, step, identity, and depth bounds. They prove exact-source SHA-256 rejection, duplicate and
unknown object rejection, unsupported language rejection, empty and oversized program rejection,
deterministic equal traces from equal complete initial runtimes, mutation plus state-query steps,
media identity and fingerprint retention, exact SQLite reopen, verified project integrity, recovery
discovery, comparison, and restore, initial conflict with no effect, later conflict with committed
prefix visibility, stopped suffix exclusion, and nested filesystem or plugin denial before the
first dispatch. They prove an in-process bounded local language, not arbitrary code execution,
module loading, source-file ownership, an operating-system sandbox, or whole-script atomicity.

These tests do not prove wire delivery ordering, UI integration, media decoding, graph
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

Three project recovery contracts drive a caller-owned full dispatcher with a real active project
database and autosave namespace. They prove all four permanent method names, the permanent event
name, strict unknown-field rejection, opaque candidate identity, degraded corruption beside one
valid candidate, typed comparison including authored clip-mix state, durable restore, candidate
retention, exact later dismissal, complete replacement events, path-shaped identity rejection, and
JSON exclusion of absolute paths, SQLite messages, and raw source text.

Five audio automation contracts drive a caller-owned full dispatcher with real audio-owned state.
They prove schema `1.0.0`, all three permanent names, strict request, result, snapshot, and event
round trips, canonical clip identifiers, every supported mutation, Read, Write, Touch, and Latch
projection, Touch behavior through the facade, no-op suppression, exact revision correlation, and
conversion failure atomicity. They do not claim durable project storage, hardware control input,
wire delivery, physical audio output, or additional automation targets.

Three editor-state API contracts drive the full dispatcher and prove the permanent query and resource
names, strict schema `1.0.0`, deterministic exact canonical resource identity, all ten named roots,
one coherent project revision, explicit color policy, optional automation attachment, pending
playback, observed export state, strict unknown-field rejection, and exact JSON round trips without
bulk runtime payloads or hidden polling.

## Current status and risks

The API now has twelve substantive domain surfaces plus bounded ordered event delivery, one complete
discovery catalog, and strict wire grammar, including one unified generic authored project command
and editor-state facade, one asynchronous job control surface, and stateless version negotiation.
Engine introspection gives
clients a coherent adaptation view without adding mutation
authority, and integration validation extends that same state with precise action and endpoint
evidence. Project settings retain exact durable scalar meaning, and project recovery now exposes one
complete strict discover, compare, restore, and dismiss surface without leaking file identity. The
API still does not expose project file open or save, public job submission or typed results,
network transport, dynamic routing, push delivery, persisted replay, authentication,
operating-system sandboxing, script source loading, general-purpose code execution, or CLI editor execution.
Host-injected authorization now covers every current
caller-selected filesystem target, plugin state and delegation mutation, and destructive job,
recovery, and automation operation before dispatch.
Generated bindings are
implemented as a deterministic representation of the current in-process public surface, not as
wire routing or a desktop application.
Scenario schema 1 remains deliberately narrow and fixed to one canonical edit. Its reference
state proves transactional control semantics, not production timeline, graph, or media ownership.
Authored clip-gain automation is a substantive strict transaction and event surface over the engine
owner, but schema 1 does not persist lanes or address pan, sends, buses, effects, or plugins.
The generic editor facade now reaches the engine's project command-history owner for apply, undo,
redo, inspection, semantic evidence, and correlated replacement events. It intentionally exposes
minimum history replacement state beside the complete editor snapshot through the same sealed
request handler. Local scripting now consumes that handler with a strict digest-bound command
document and complete conflict trace. Project file commands, live wire routing, push delivery, and
broader automation adaptation remain later checkpoints.

Asynchronous job control is a substantive strict projection over the canonical engine export queue.
It keeps user-visible work responsive through nonblocking state replacement and cooperative
controls, but the queue and its retained typed results remain process-local, and another owner must
submit prepared export executors or publish final files.

Integration validation schema 1 provides one coherent read-only state for CLI, UI, and tests, but
it remains an in-process snapshot facade. The standalone helper creates a fresh starting engine for
the CLI; a production UI must supply fresh observations from its live dispatcher. Validation does
not supply transport, endpoint mutation, or background polling.

The separate negotiation, media, engine introspection, integration validation, scenario, event
stream, catalog, and error schema versions are correct for independent surfaces. The catalog is
`1.3.0` because local scripting and negotiation are additive while individual method and resource
schemas retain their own versions.
Data-only JSON-RPC framing, bounded retryable polling, and host-injected authorization now exist,
while network hosting, dynamic routing, authentication, persisted replay, and
push delivery remain required before remote clients exist. Public scenario failure state
is boxed to keep result errors bounded while preserving the same serialized shape and now removes
raw context values.

## Maintenance notes

Preserve strict serialization, deterministic ordering, stable permanent names, last-valid-state
errors, revision fences, transaction and event agreement, and unknown-variant handling. Keep
the explicit catalog synchronized with every new command, query, event, resource, schema version,
CLI discovery assertion, and generated TypeScript map. Regenerate the committed artifact and run
the nonmutating check after every public DTO or registry change. Preserve the safe error conversion
boundary and require negative leak
proof before adding context fields. Do not treat JSON-RPC data values as a transport implementation.
Keep API release offers strictly ordered by SemVer precedence, return canonical server versions,
retain all missing dimensions, and keep project compatibility delegated to `superi-project` through
the behavior-free engine seam. Negotiation must remain stateless and cannot imply a runtime protocol
downgrade that has not been implemented.
Keep the public event union in exact parity with the catalog and preserve separate source and public
sequence domains. Never advance subscriber state on poll, return events across a gap, reuse a stream
identity after owner restart, omit an authoritative state resource from reconnect metadata, or add
an untyped event fallback. New state resources must declare their exact refresh method and method
kind, and new events require strict correlation, replacement-revision validation, and real producer
proof.
Keep the permission context host-injected and nonserializable. Every new command or nested payload
variant must classify its permission mode and kinds, derive exact requirements before conversion,
publish matching catalog metadata, prove denial leaves all owned state and events unchanged, and
prove the authorized path retains parity. Keep lexical filesystem scopes honest about symlinks and
require actual I/O owners to revalidate and contain operating-system access.
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
Keep `superi-json` closed, bounded, exact-source digest checked, and interpreted only through the
sealed project editor request path. Adding a supported step requires the existing public method and
typed response, exact nested permission derivation before execution, catalog and TypeScript parity,
deterministic trace proof, conflict and partial-commit review, and real persistence or runtime
consumer evidence. Never add direct file loading, process execution, networking, ambient authority,
hidden retries, silent event draining, or a second authored-state model to the scripting module.
Keep asynchronous job handles canonical, all query and control paths routed through the attached
engine export queue, and every event a verified full replacement from the ordered engine envelope.
Do not catalog host polling, executor submission, waiting, or typed result access. Preserve exact
status mapping, deterministic handle and dependency ordering, unit progress, cooperative control,
and negative failure-leak proof whenever the engine queue evolves.

Keep complete editor state behind one dispatcher observation and one permanent public query. Add
domain state by extending the authoritative engine aggregate and strict public projection together,
preserve canonical authored document identity, keep optional owner freshness explicit, and never
copy bulk runtime payloads or opaque extension bytes into the replacement snapshot.
