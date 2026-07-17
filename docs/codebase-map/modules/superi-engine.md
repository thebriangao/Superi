---
module_id: superi-engine
source_paths:
  - open/crates/superi-engine
source_hash: b78ec364b53a8b7f7731454d46b8aaf2851d8f1b583a3fab0e095ac6364bcba0
source_files: 68
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-engine` is the open orchestration layer. Its substantive paths cover canonical editorial
command state, complete media backend registry assembly, transactional project snapshot or timeline
graph plus source and decoder preparation, capability and health introspection, CPU-decoded frame upload, exact viewport and
export color metadata branching, derived-media generation, transparent proxy resolution,
predictive cache population, foreground graph and display-color execution, bounded audio admission,
audio-master A/V coordination with bounded video correction and discontinuity recovery, monotonic
clock fallback, lossless viewport handoff, exact interactive transport control, coherent decode,
graph, delivery-color, audio, and elementary-stream export execution, deterministic OpenFX bundle
discovery and isolated-worker supervision, coherent plugin availability across playback, rendering,
and export, deterministic native audio plugin candidate discovery, strict isolated-worker
validation, checkpoint recovery and quarantine, per-node durable state records, prepared
timing-matched audio fallback, deterministic project, device, sleep, wake, shutdown, and restart lifecycle,
bounded logical export job orchestration, classified cross-subsystem failure propagation
and recovery, shared finite-resource arbitration across decode, GPU, cache, audio, AI, and export
work, atomic timeline plus clip-mix edits, typed resolution of durable project settings, bounded
compound project transactions across authored timeline, graph, media, audio, extension, and root
state, revisioned authored audio automation, and
engine-owned command history over that project surface. An engine-wide typed command dispatcher
coordinates canonical scenario transactions, lifecycle state changes, failure and recovery control,
coherent work admission, bounded cross-domain playback control, canonical logical export-job
control, revision-fenced project mutation plus undo and redo, authoritative project extension and
settings inspection and transactions, authored audio automation inspection and transactions, crash
recovery discovery, comparison, durable restore, and dismissal, read-only whole-engine
introspection and integration validation, deterministic semantic project diagnostics inspection,
one coherent complete editor-state inspection across authored and cached runtime owners, and
ordered replacement events. Validation composes the
canonical introspection snapshot with exact lifecycle, recovery, scenario, playback, and export
evidence without polling a runtime owner or creating a second mutable state model.
Native GPU presentation, timeline-owned decoded-audio rendering, export muxing and publication,
wire adaptation, and general project nodes remain incomplete.

The scenario command path remains a bounded reference owner for contract conformance. The project
history path instead wraps the real `superi-project` document, media command, extension command,
and settings owners, without replacing timeline, graph, media, color, render, persistence, or
muxing policy.
The selected immutable snapshot from that history path is also the real consumer input for the
project-owned autosave contract. Engine production source does not own autosave policy, publication,
retention, or scheduling. Its focused recovery coordinator instead owns the active file-backed
database attachment and composes the project recovery catalog with monotonic history restoration
through the existing dispatcher.

## Source inventory

- `open/crates/superi-engine/Cargo.toml`: Declares subsystem dependencies, optional codec features,
  production `sha2`, a narrow empty-project `test-support` feature for upper-tier public contracts,
  and test-only `pollster` plus `rusqlite` for exact project database fixtures.
- `open/crates/superi-engine/src/audio_plugins.rs`: Implements deterministic recursive VST3 bundle
  discovery and explicit Audio Unit candidate ingestion, strict isolated-worker contracts, bounded
  descriptor validation, retained scan and runtime failures, activation, state capture, restart and
  quarantine recovery, prepared process-bridge construction, and per-node project extension records.
- `open/crates/superi-engine/src/audio_mix.rs`: Applies timeline edit batches and audio clip-mix
  identity reconciliation against cloned subsystem states, publishing both only after timeline and
  mix revisions, fragment inheritance, replacement transfer, and removal all validate.
- `open/crates/superi-engine/src/av_sync.rs`: Composes the shared playback clock and A/V scheduler
  into one playback-domain engine owner with an explicit interactive policy, immutable frame
  timing, caller-owned drop readiness, nonblocking hold, present, drop, and recovery outcomes,
  applied discontinuity rebases, clock fallback and restoration, and coherent diagnostics.
- `open/crates/superi-engine/src/command.rs`: Implements canonical fixture identity, named timeline
  and trim state, complete mirror graph control state, typed operation evidence, bounded source
  validation, atomic bounded ordered transactions, monotonic revisions, and full-state undo plus
  redo where one transaction is one history unit.
- `open/crates/superi-engine/src/derived_media.rs`: Canonicalizes complete video or audio encoder
  settings, validates cache request identity, selects one explicit primary encoder, drives its
  nonblocking lifecycle to end of stream, hashes complete packet semantics, and publishes only a
  complete proxy or optimized-media payload through `superi-cache`.
- `open/crates/superi-engine/src/dispatcher.rs`: Implements bounded typed engine requests,
  optimistic scenario transactions, successful-command and event sequencing, complete replacement
  scenario, project, lifecycle, classified recovery, playback, and logical export events with
  originating-command correlation, canonical error-coordinator ownership, exact recovery tokens,
  lifecycle action routing, acknowledged sleep and wake commands, shutdown, restart, coherent work
  admission, exclusive optional project-history attachment, retained latest playback and export
  replacement state, plus a one-command nonblocking bridge from EngineControl to the playback
  owner. It composes declaration-only media state,
  optional exact resource accounting, and dispatcher-owned lifecycle and recovery state into
  read-only introspection and integration validation snapshots without advancing a command or event
  sequence. It optionally owns one authoritative project document, exposes immutable project
  snapshots, computes eventless versioned semantic project diagnostics from the selected history
  snapshot, routes settings inspection and optimistic transactions with dynamic no-op event
  reservation, optionally owns authored audio automation with the same dynamic event reservation,
  and optionally attaches one exact file-backed recovery coordinator whose discover, restore, and
  dismiss commands publish correlated complete recovery state.
- `open/crates/superi-engine/src/editor.rs`: Exposes the curated checked construction vocabulary
  needed by the upper-tier generic project API while adding no wire model, mutation algorithm,
  dispatcher, history, persistence owner, or lower dependency edge.
- `open/crates/superi-engine/src/editor_state.rs`: Captures one immutable project-history revision,
  deterministic diagnostics and settings, canonical timeline, graph, and clip-mix resources,
  bounded extension descriptors, exact audio routing and continuity, and explicit cached recovery,
  automation, playback, and export observations without polling or mutation.
- `open/crates/superi-engine/src/error.rs`: Implements EngineControl-owned classified failure
  propagation, immutable actionable records, bounded diagnostic history, exact recovery-intent
  validation, failed-recovery reclassification, and coherent lifecycle routing without copying
  subsystem readiness or workflow admission state.
- `open/crates/superi-engine/src/export_dispatch.rs`: Projects bounded logical export jobs into a
  stable command vocabulary, complete deterministic replacement state, lifecycle-admitted fresh
  attempts, and a typed runtime handle for executor preparation, result access, and blocking-safe
  shutdown while every queue mutation remains dispatcher owned.
- `open/crates/superi-engine/src/export_jobs.rs`: Implements a bounded EngineControl-owned logical
  export queue with immutable dependency declarations, export-priority worker attempts, nonblocking
  progress and result observation, cooperative pause and cancel, fresh-attempt resume and retry,
  classified failure retention, and explicit blocking-safe shutdown.
- `open/crates/superi-engine/src/export_queue.rs`: Implements lifecycle-admitted render and export
  transactions through prepared source reads, decode, shared immutable graph evaluation, explicit
  delivery color and audio stages, one-shot registry encoder selection, complete codec draining,
  exact semantic validation, tracked semantic progress, elementary packet publication, and
  reset-based recovery.
- `open/crates/superi-engine/src/frame_upload.rs`: Implements the media-I/O-to-GPU upload boundary
  for CPU-addressable decoded video and retains the frame's complete color pipeline beside the GPU owner.
- `open/crates/superi-engine/src/history.rs`: Implements one shared bounded immutable before-and-after
  snapshot store, stable typed media, extension, settings, and compound project mutations and
  history actions,
  revision-fenced apply, undo, and redo, oldest-entry eviction, failure and no-op branch
  preservation, the exclusive `ProjectCommandHistory` owner over one real `ProjectDocument`, and an
  internal prepare-then-commit recovery replacement that resets session history only after durable
  active-project publication.
- `open/crates/superi-engine/src/introspection.rs`: Implements deterministic API-neutral backend and
  codec capability records plus complete immutable engine snapshots containing optional exact
  resource accounting, lifecycle and recovery revisions, health, canonical subsystem state,
  coherent playback, rendering, and export readiness, and user-safe active failure projections.
- `open/crates/superi-engine/src/lib.rs`: Documents the implemented orchestration boundaries,
  exposes the engine modules including the curated editor seam, and conditionally exposes the
  hidden test-support fixture.
- `open/crates/superi-engine/src/lifecycle.rs`: Implements the EngineControl-owned lifecycle state
  machine, canonical subsystem dependency plan, exact action tokens, immutable generated snapshots,
  explicit project and device coordination, reverse sleep preparation, forward wake revalidation,
  coherent playback, rendering, and export admission, recoverable degradation, rollback, reverse
  teardown, teardown retry, and fresh-lifetime restart.
- `open/crates/superi-engine/src/media.rs`: Builds default and feature-gated media registries,
  including atomically preflighted primary registrations for all four in-tree container sources.
- `open/crates/superi-engine/src/nodes.rs`: Placeholder for media and graph nodes.
- `open/crates/superi-engine/src/playback.rs`: Defines playback-domain nonblocking prediction and
  foreground submission, complete decoded and scene semantic envelopes, shared-graph retained
  evaluation, validated cache admission, concrete CPU display conversion behind a generic output
  seam, cancellable distinct-deadline foreground work, bounded audio output and discontinuity
  status, audio-master and monotonic clock continuity, lossless viewport handoff, structured
  degradation, and weak worker-pool lifecycle boundaries.
- `open/crates/superi-engine/src/plugins.rs`: Implements deterministic recursive OpenFX bundle
  discovery, canonical bundle validation, platform-launcher coordination, per-plugin isolated host
  ownership, exact permission narrowing, classified failure retention, restart and quarantine
  recovery, active graph-registry rebuilding, and one availability resolver for playback,
  rendering, and export.
- `open/crates/superi-engine/src/project_settings.rs`: Resolves one authoritative project settings
  snapshot into existing exact timeline, timecode, color identity, audio timebase and ordered
  channel layout, cache budget, proxy quality, render format, alpha, target, and fingerprint types;
  it re-exports the project-owned mutation and transaction vocabulary for upper-tier dispatchers.
- `open/crates/superi-engine/src/project_recovery.rs`: Owns the typed discover, compare, restore,
  and dismiss command vocabulary, complete recovery replacement state, exact attachment of one
  file-backed active database and project recovery catalog, prepare-before-publish restoration, and
  infallible postpublication history replacement.
- `open/crates/superi-engine/src/project_transaction.rs`: Implements the bounded whole-project
  transaction vocabulary, one outer project publication, ordered action results, timeline and
  clip-mix reconciliation, extension commands, three-way retained graph preservation, per-action
  aggregate validation, and complete rollback on any late failure.
- `open/crates/superi-engine/src/proxy_substitution.rs`: Validates exact proxy purpose, source
  fingerprint, source revision, packet integrity, and stream metadata; translates cache quality to
  scheduler quality; consumes deterministic derived selection; lazily opens verified original
  media; and adapts complete generated packets to the codec-neutral `MediaSource` contract.
- `open/crates/superi-engine/src/render.rs`: Defines independent viewport and export color metadata branches from cached scene state, requiring correctly classified terminal display or output stages.
- `open/crates/superi-engine/src/resource_arbitration.rs`: Implements one thread-safe managed-byte
  envelope for decoded buffers, GPU payloads, caches, prepared audio, AI work, and exports with
  complete class configuration, protected floors, exact hard limits, noncloneable reservations,
  fixed-order cooperative reclaim, consumer-aware fallbacks, and deterministic snapshots.
- `open/crates/superi-engine/src/resources.rs`: Either compiles one reachable editorial timeline or
  clones the exact root compilation retained by an immutable `ProjectSnapshot`, then validates the
  exact caller-declared media and stream request set, adapts recognized project media paths into
  fingerprint-bound local source requests, binds and verifies project fingerprints, probes and
  opens each source, selects and creates each decoder once, retains policy evidence, and publishes
  one all-or-nothing owner bundle.
- `open/crates/superi-engine/src/test_support.rs`: Feature-gated hidden construction of a minimal
  valid project document and one real file-backed recovery dispatcher fixture for upper-tier API
  contract tests, without widening dependency direction.
- `open/crates/superi-engine/src/transport.rs`: Implements playback-domain pause, play, seek,
  superseding scrub, exact frame step, reduced signed rate, direction, half-open loop, rational
  clock cadence, prediction replanning, protected discontinuities, bounded ordinary late-frame
  dropping, explicit audio degradation, immutable transport observations, and the stable typed
  command vocabulary consumed by the engine bridge.
- `open/crates/superi-engine/src/validation.rs`: Projects immutable engine integration validation
  from canonical scenario, lifecycle, recovery, playback, and export state. It exposes precise
  condition, per-workflow permit or denial, endpoint attachment, latest replacement state,
  reversibility, and deterministic coherence findings without locks, waits, or duplicated mutable
  ownership.
- `open/crates/superi-engine/tests/av1_capability_contract.rs`: Default AV1 selection proof.
- `open/crates/superi-engine/tests/audio_editorial_mix_contract.rs`: Proves atomic timeline and
  clip-mix publication plus complete audio intent, source, record, identity, grouping, linking, and
  synchronization preservation through a real razor edit.
- `open/crates/superi-engine/tests/audio_automation_dispatcher_contract.rs`: Proves exclusive
  lifecycle-owned automation attachment, serialized inspection and mutation, no-op suppression,
  stale and invalid atomicity, full event-queue preflight, isolated revision correlation, and real
  clip-processor consumption of a returned replacement snapshot.
- `open/crates/superi-engine/tests/audio_plugin_supervision_contract.rs`: Proves deterministic VST3
  discovery, domain and worker-contract validation, compatible-version state recovery, fixed
  latency propagation, fault and quarantine transitions, per-node state identity, and exact project
  save and reopen through the real database.
- `open/crates/superi-engine/tests/av_sync_coordination_contract.rs`: Proves the interactive
  policy, playback-domain ownership, bounded hold and correction, protected and eligible-drop
  behavior, distinct live deadlines and intervals with immutable media timing, discontinuity
  recovery, continuous clock fallback and restoration, and deterministic statistics.
- `open/crates/superi-engine/tests/color_metadata_propagation_contract.rs`: Proves exact source interpretation, ordered transform history, cache identity, independent viewport and export intent, and invalid-order rejection across media, graph, timeline, cache, and engine boundaries.
- `open/crates/superi-engine/tests/derived_media_generation_contract.rs`: Proves complete and
  deterministic real AV1 generation, exact cache reuse, quality identity, settings mismatch
  rejection, cooperative cancellation, and original-source fallback.
- `open/crates/superi-engine/tests/error_recovery_contract.rs`: Proves retryable, degraded,
  user-correctable, and terminal propagation; source and context retention; separate safe
  projection; coherent playback, rendering, and export admission; exact recovery intent; failed
  recovery reclassification; stale-action rejection; bounded history; and EngineControl ownership.
- `open/crates/superi-engine/tests/error_dispatcher_contract.rs`: Proves dispatcher-owned
  operation-labeled failure reporting, independently revisioned full recovery state, coherent
  lifecycle admission, exact request and action validation, successful recovery, failed-recovery
  reclassification, stale-token rejection, inspection without events, and legacy command routing
  through the canonical coordinator.
- `open/crates/superi-engine/tests/export_job_queue_contract.rs`: Proves bounded logical identity,
  immutable dependency ordering, nonblocking progress and retained results, cooperative pause and
  cancel, fresh-attempt resume and retry, recovery-class policy, terminal propagation, safe removal,
  execution-domain enforcement, and blocking-safe shutdown. The pause contract waits for its
  separate executor-entry handshake after observing queue `Running` state, so parallel test
  scheduling cannot turn the intended cooperative loop into a teardown hang.
- `open/crates/superi-engine/tests/export_dispatcher_contract.rs`: Proves dispatcher-owned prepared
  submit, ordered complete replacement state, automated progress and completion polling, typed
  result retention, inspection, pause, fresh resume, dependency release, one-job and all-job cancel,
  degradation denial, recovery retry with a fresh permit, removal, and final-state-only blocking-safe
  shutdown.
- `open/crates/superi-engine/tests/dispatcher_contract.rs`: Proves atomic multi-action commit and
  rollback, revision fences, one-unit undo, bounded ordered replacement events, EngineControl
  enforcement, coherent normal and degraded work admission, recovery, stale permits, retained
  failure context, acknowledged sleep and wake event publication, restart, shutdown, inspection,
  exclusive project-history attachment, project command and event correlation, stale atomicity,
  no-op event suppression, unknown extension command dispatch and typed results, exact extension
  undo and redo, one correlated event for a compound project command, and
  event-backpressure preflight before project mutation. It also proves typed project diagnostics
  inspection through the full dispatcher, missing-project classification, successful command
  sequencing, no event or history mutation, authored-change sensitivity, and undo restoration of
  the prior semantic hash under a newer document revision.
- `open/crates/superi-engine/tests/engine_introspection_contract.rs`: Proves EngineControl-owned
  read-only observation, unchanged scenario and event state, deterministic capability and resource
  projection, coherent normal admission, playback-only, rendering-plus-export, and export-only
  degradation, visible recovery progress, restored readiness, and exclusion of private diagnostic
  details.
- `open/crates/superi-engine/tests/editor_state_contract.rs`: Proves missing-project classification,
  one coherent eventless revision, explicit optional owner attachment and freshness, multichannel
  routing with mute intent, and sample-exact gap and source-transition continuity evidence.
- `open/crates/superi-engine/tests/frame_upload_contract.rs`: Upload, ownership, storage, and budget
  proof.
- `open/crates/superi-engine/tests/integration_validation_contract.rs`: Proves coherent normal,
  degraded, recovering, and recovered projections, exact playback, rendering, and export admission,
  preserved scenario reversal evidence, rendering-only degradation, and retained export queue state.
- `open/crates/superi-engine/tests/lifecycle_contract.rs`: Proves deterministic startup and reverse
  teardown, generated shared snapshots, coherent workflow admission, retained project state,
  volatile device and workflow release, forward wake revalidation, critical wake, superseded power
  transitions, recoverable and terminal wake failure, shutdown during power transitions, isolated
  degradation and recovery, initialization rollback, stale action rejection, dependency-safe
  teardown retry, direct and stopped restart, and EngineControl ownership.
- `open/crates/superi-engine/tests/media_resource_acquisition_contract.rs`: Proves complete source
  registration, real WebM and AV1 preparation, exact published project graph retention after a
  direct graph edit, exact schema-0 project migration into that same retained graph, legacy timeline
  compilation, exact timing, precision, metadata, color and alpha semantics, strict request
  validation, explicit fallback evidence, cancellation, no exception retry, and fresh-context
  recovery.
- `open/crates/superi-engine/tests/opus_capability_contract.rs`: Default Opus selection proof.
- `open/crates/superi-engine/tests/os_codec_registry_contract.rs`: Feature-gated host registry proof.
- `open/crates/superi-engine/tests/proxy_substitution_contract.rs`: Proves real AV1 proxy
  substitution, exact and lower-quality choice, deterministic ties, replacement, seek preroll,
  strict freshness, lazy original fallback, identity mismatch rejection, and source-only delivery.
- `open/crates/superi-engine/tests/playback_prefetch_contract.rs`: Proves playback ownership,
  immediate nonblocking submission and polling, cooperative supersession, boundary-empty work,
  structured degraded failure, and later recovery.
- `open/crates/superi-engine/tests/playback_prefetch_graph_contract.rs`: Proves exact predicted
  frames populate and reuse the real budgeted cache through one immutable graph snapshot without
  changing foreground evaluator output.
- `open/crates/superi-engine/tests/playback_orchestration_contract.rs`: Proves one coherent decoded
  frame, graph, cache, color, audio, clock, worker, and viewport path across normal playback,
  early waiting, late correction, discontinuity recovery, backpressure without duplicate sync
  decisions, invalid scene degradation, cache reuse, and clock recovery.
- `open/crates/superi-engine/tests/plugin_supervision_contract.rs`: Proves deterministic nested
  bundle discovery, malformed and terminal launch containment, real child-process action isolation,
  permission narrowing, retryable and user-correctable failures, quarantine, restart recovery, and
  identical playback, rendering, and export availability snapshots.
- `open/crates/superi-engine/tests/render_export_orchestration_contract.rs`: Proves coherent paired
  video and audio export, deterministic fallback evidence, no exception retry, lifecycle degradation,
  partial-source recovery, encoder and terminal-delivery color agreement, exact WAVE PCM completion,
  rejection of real VP9 timing drift after complete WebM AV1 decode and shared graph evaluation, and
  completion through dispatcher-owned logical export commands over the real queue with exact
  semantic progress and retained typed artifact access.
- `open/crates/superi-engine/tests/resource_arbitration_contract.rs`: Proves complete configuration,
  opaque high-precision decoded-frame retention, exact RAII accounting, protected floors,
  fixed-order cooperative reclaim, classified callback failure, all six semantic fallback classes,
  class-ceiling recovery, concurrent hard limits, and recursive callback rejection.
- `open/crates/superi-engine/tests/playback_transport_contract.rs`: Proves exact seek and scrub
  supersession, pause and resume, frame stepping, fractional cadence, reverse looping, stale-work
  exclusion, bounded drops, protected intent, audio discontinuity, foreground and prediction
  degradation, viewport backpressure, recovery through the real playback owners, and bounded
  EngineControl-to-Playback command and event dispatch, plus retained latest playback replacement
  and failure state after ordered events are drained.
- `open/crates/superi-engine/tests/project_history_contract.rs`: Proves real project media mutation,
  full-project undo and redo, branch clearing only after a successful authored change, failure and
  no-op preservation, bounded oldest-entry eviction, monotonic revision exhaustion, schema-4 SQLite
  durability of the selected snapshot, and the real media resource consumer after reversal.
- `open/crates/superi-engine/tests/project_autosave_contract.rs`: Proves that immutable selected
  history state after authored apply, undo, and redo reaches the project-owned autosave command
  surface as complete current-schema recovery points, including exact revisions, settings, media
  references, editorial state, and retained graphs, without adding an engine autosave owner.
- `open/crates/superi-engine/tests/project_recovery_dispatcher_contract.rs`: Proves one
  EngineControl command surface for discovery, complete semantic comparison including authored
  clip-mix state, durable monotonic restore, and
  exact dismissal; active database reopen equality; empty restored session history; candidate
  retention; persistence-failure atomicity; externally replaced active-database conflict
  preservation; stale and terminal classification; and event-queue backpressure before every
  persistent mutation.
- `open/crates/superi-engine/tests/project_settings_dispatcher_contract.rs`: Proves authoritative
  project attachment, exact typed resolution for all six policy domains, optimistic transactions,
  no-op event suppression under a full queue, invalid and stale rollback, ordered replacement
  events, missing-project failures, EngineControl ownership, and settings undo plus redo through the
  shared project history owner without authored-audio drift.
- `open/crates/superi-engine/tests/project_transaction_contract.rs`: Proves one compound transaction
  across root, timeline, graph, media, authored audio, and plugin, effect, and AI extension state,
  one project revision and history entry, retained graph edits, exact save and reopen of authored
  audio and extensions, adjacent-block audible continuity, undo and redo, late failure rollback,
  and action bounds.
- `open/crates/superi-engine/tests/scenario_contract.rs`: Exact canonical state, atomicity, bounds,
  operation log, reversal proof, bounded oldest-entry eviction, and redo clearing after a successful
  post-undo branch.
- `open/crates/superi-engine/tests/vendor_codec_registry_contract.rs`: Explicit vendor registry
  proof.
- `open/crates/superi-engine/tests/vorbis_capability_contract.rs`: Default Vorbis selection proof.

## Public surface

`command` exposes the fixed canonical values, `ScenarioEngine`, `ScenarioAction`,
`ScenarioSnapshot`, phases, fixture state, timeline state, graph nodes and edges, mirror parameters,
implementation identity, typed operation arguments, and operation records. Supported mutations are
import, insert, trim, and horizontal mirror. Undo and redo are history actions. Export is
intentionally absent from engine mutations.

`editor` is a narrow upper-tier construction seam. It reexports the existing exact identifiers,
time values, editorial material, timeline edit operations, compiled graph mutation vocabulary,
media paths, clip-mix controls, extension state, compound project transactions, history commands,
dispatcher commands, snapshots, and database consumer needed by `superi-api::ProjectEditorApi`.
It owns no behavior and deliberately does not derive or define wire serialization for lower-domain
types. This keeps API conversion dependent only on `superi-engine` and `superi-core` while every
semantic check remains with its existing lower owner.

`history` exposes the production Rust project command-history surface:

- `ProjectMutation` wraps every authored `ProjectMediaCommand`, one
  `ProjectExtensionCommand`, and one bounded `CompoundProjectTransaction`, while
  `ProjectMutationKind` provides permanent typed classification for project settings, compound,
  every extension operation, path replacement, missing-state, and fingerprint-checked relink
  actions.
- `ProjectHistoryCommand` carries one exact expected document revision and a typed apply, undo, or
  redo action. `ProjectHistoryActionResult` preserves the project owner's semantic media or ordered
  extension command results, compound action results, and the mutation kind reversed or reapplied.
- `ProjectHistoryState` contains the selected immutable `ProjectSnapshot`, configured capacity,
  undo and redo depths, and the next available typed action on each branch.
- `ProjectHistoryOutcome` pairs that state with optional action evidence and an explicit authored
  state-change flag. Inspection therefore shares the same complete result shape without mutation.
- `ProjectCommandHistory` exclusively owns one `ProjectDocument`. The default capacity is 64,
  explicit capacity is bounded from 1 through 4096, and retained entries are session-local immutable
  before and after snapshots while only the selected project snapshot remains durable. Dispatcher
  settings transactions publish through this same document and enter the same history. Recovery
  prepares a cloned restored document behind the exact current revision and commits it with empty
  undo and redo branches only after the active project database publishes the same snapshot.

`dispatcher` exposes `EngineTransactionId`, `ScenarioTransaction`, `EngineCommandRequest`, the
non-exhaustive `EngineCommand` and `EngineCommandResult` vocabularies, classified
`EngineReportedFailure`, `EngineEventEnvelope`, `PlaybackCommandExecution`,
`PlaybackCommandExecutor`, and `EngineCommandDispatcher`. The coherent constructor binds scenario
and lifecycle control behind EngineControl, `new_with_playback_bridge` adds a bounded nonblocking
executor for the real playback owner, `attach_export_jobs` installs the canonical logical export
queue behind a type-erased controller, and `scenario_only` is the explicit compatibility owner used
by the narrow public reference API. `attach_project_history` installs exactly one exclusive
project command owner, and `project_history_state` reads its immutable typed state. The
`ExecuteProjectHistory` and `InspectProjectHistory` commands return `ProjectHistory` results;
authored changes publish a complete `ProjectStateChanged` replacement event with project revision,
command sequence, event sequence, and caller transaction correlation. `attach_project` installs the
same exclusive owner with default history capacity for the public settings path, and
`project_snapshot` returns its selected immutable project without exposing mutable ownership.
`InspectProjectDiagnostics` computes `superi-project::ProjectDiagnostics` from that exact selected
history snapshot and returns `EngineCommandResult::ProjectDiagnostics`. It advances only the
successful command sequence, emits no event, changes no history or project state, and reports
unavailable when no project owner is attached.
`InspectProjectSettings` and `ExecuteProjectSettings` return resolved settings state, record semantic
changes in the shared history, and publish correlated `ProjectSettingsChanged` replacement events.
`attach_audio_automation` installs one exclusive automation owner on the full dispatcher.
`InspectAudioAutomation` and `ExecuteAudioAutomation` return complete audio-owned snapshots;
semantic changes publish one `AudioAutomationStateChanged` event whose envelope sets only the audio
automation revision, while no-ops advance command order without reserving or emitting an event.
`attach_project_recovery` consumes one file-backed `ProjectDatabase` only when it exactly matches the
attached history snapshot. `ExecuteProjectRecovery` accepts typed discover, compare, restore, and
dismiss commands. It returns one `ProjectRecoveryOutcome`; discover, restore, and dismiss publish a
complete `ProjectRecoveryStateChanged` event, while comparison remains read-only and event-free.
`introspection_snapshot` composes current declaration-only
media capabilities, optional exact shared-resource accounting, and dispatcher-owned lifecycle and
recovery state without dispatching a command or publishing an event.
`integration_validation_snapshot` composes that exact introspection state with scenario reversal,
exact action and admission tokens, endpoint attachment, and retained replacement observations.
`standalone_integration_validation_snapshot` owns a temporary EngineControl domain and default
registry for the CLI, while application and test hosts use their existing dispatcher. Successful command and event sequences are independently
monotonic, each event identifies its originating command sequence, and event draining returns
complete ordered replacement state with scenario, project, lifecycle, playback, or export
correlation.

`editor_state` exposes the immutable `EditorStateSnapshot` aggregate and typed subrecords for
canonical authored documents, graph ownership, bounded extension identity, exact audio routing and
continuity, and optional runtime owner availability. `InspectEditorState` captures the selected
history state, project diagnostics and settings, cached recovery state, optional automation,
playback attachment and pending status, latest playback result and failure, export attachment, and
latest export replacement in one EngineControl command. It never discovers recovery files, executes
playback, polls export, mutates authored state, or emits an event. Canonical authored bytes remain
project, timeline, graph, and audio owned; this module stores their exact serialized identity for
one coherent public projection.

`project_settings` exposes `ProjectSettingsState`, `ResolvedProjectSettings`, typed timeline,
color, audio, cache, proxy, and render policy values, and the project-owned mutation and transaction
vocabulary. Resolution maps durable scalar settings into existing subsystem contracts without
rewriting authored timeline or graph state. Built-in ACEScg and pinned color identities remain
explicit; audio retains an exact sample timebase and ordered `ChannelLayout`; render policy retains
one deterministic `RenderSettingsFingerprint` for cache and execution identity.

`project_recovery` exposes stable `ProjectRecoveryOperation` and `ProjectRecoveryCommand` values,
opaque project-owned candidate identity, complete aggregate `ProjectRecoveryComparison`, complete
`ProjectRecoveryState`, and `ProjectRecoveryOutcome`. The internal coordinator binds the exact
project catalog to the authoritative history and active file-backed database. Restore fully loads
and prepares a monotonic candidate document, publishes it through `ProjectDatabase::replace`, then
commits the prepared in-memory document without a fallible step or retained prior-session history.

`validation` exposes `EngineIntegrationCondition`, stable coherence finding codes,
`EngineWorkflowValidation`, endpoint-specific playback and export validation state, and
`EngineIntegrationValidationSnapshot`. Each snapshot carries the canonical
`EngineIntrospectionSnapshot`, scenario, lifecycle, and recovery state, three exact workflow
admission results, latest retained endpoint replacement state, attachment evidence, and
deterministic agreement and ownership findings.

`export_dispatch` exposes the bounded `EngineExportJobSubmission` and
`EngineExportJobCommand` vocabulary, `EngineExportJobSnapshot` and revisioned full
`EngineExportJobState`, the permit-aware `EngineExportJobExecutor<R>` contract, and the typed
`EngineExportJobRuntime<R>` handle. Submit, automated poll, inspection, pause, resume, retry,
one-job or all-job cancel, and remove mutate or observe the existing `ExportJobQueue<R>` only through dispatcher
commands. Generic executors are prepared against stable job identities, each fresh attempt receives
the current export lifecycle permit, typed artifacts remain behind the runtime handle, and worker
joining remains explicit outside nonblocking EngineControl.

`frame_upload` exposes `UploadedVideoFrame` and `VideoFrameUploader` with explicit configuration,
shared pool construction, and exact color-pipeline access. `render` exposes `ViewportColorMetadata`
and `ExportColorMetadata`, which clone cached scene metadata and append a correctly typed terminal
stage without transforming pixels. `introspection` exposes engine-owned media backend, operation,
codec, constraint, and hardware records through `MediaCapabilities::from_registry`. It also exposes
`EngineIntrospectionSnapshot` with lifecycle phase and revisions, health, canonical subsystem
state, workflow availability and first blocker, safe active failure and recovery state, and optional
exact `EngineResourceStatus` accounting across all six resource classes. `media` exposes the
default registry with four primary source adapters and the feature-gated explicitly configured
vendor constructor.

`plugins` exposes canonical `OfxPluginBundle` validation, deterministic
`discover_ofx_bundles`, the strict `OfxWorkerLauncher` platform seam, classified
`PluginFailure` records, `PluginSupervisor`, and immutable `PluginWorkResolution`. The supervisor
owns one boxed effects host per launched bundle, rebuilds its active graph registry after every
lifecycle transition, contains bundle-local scan and worker failures, and resolves one graph plus
registry snapshot identically for playback, rendering, and export.

`audio_plugins` exposes `AudioPluginInstanceId`, validated candidate and failure records,
`AudioPluginDiscoveryReport`, deterministic `discover_vst3_bundles`, strict
`AudioPluginWorkerContract`, descriptor and launcher seams, lifecycle snapshots, and
`AudioPluginSupervisor`. It scans on BackgroundJob, contains each candidate's activation and runtime
failure, restores compatible saved state before launch, captures exact current state, retries a
faulted worker, quarantines repeated activation failure, and prepares the audio-owned isolated
bridge processor. Per-instance project helpers encode and decode one exact versioned extension
record keyed by the owning audio node rather than the reusable plugin component.

`resources` exposes explicit `MediaResourceRequest`, `DecoderResourceRequest`, and
`ResourceAcquisitionPolicy` inputs; stable source and decoder selection evidence; stateful acquired
source and decoder owners; and `TimelineResources`. `acquire_timeline_resources` preserves the
legacy editorial entry point and compiles one root plus its reachable nested timelines.
`acquire_project_resources` accepts an immutable `ProjectSnapshot` and clones its exact selected
`TimelineGraphCompilation` without recompiling, so published direct graph edits remain intact. Both
paths require exactly one request for each reachable linked media identity and share the same
source, decoder, cancellation, and all-or-nothing publication implementation. The consumer contract
now obtains that snapshot through `ProjectDatabase::open` from a supported schema-0 fixture, proving
that project migration cannot erase the graph revision before engine preparation.
`MediaResourceRequest::from_project_media` resolves a recognized filesystem target through the
project-owned codec, binds the stored `MediaId` and expected fingerprint into `SourceRequest`, and
rejects missing identities, explicit missing state, opaque targets, and nondeterministic relative
project-file context before acquisition.

`derived_media` exposes `EncodedDerivedMedia`, `derived_media_render_settings`, and
`generate_derived_media`. Settings derive from purpose, quality, stream, codec, timebase, complete
video representation and color or alpha meaning, or complete audio representation and channel
order. Generation accepts caller-prepared codec-neutral inputs and an external cache catalog,
returns immutable complete packets, and never exposes an encoder implementation type.

`proxy_substitution` exposes `ProxySubstitutionRequest`, `ResolvedMediaSource`, and
`resolve_proxy_source`. The request preserves the complete authoritative `SourceIdentity`, exact
source revision, scheduler quality, and fallback policy. Resolution returns one ordinary
`MediaSource` plus explicit `DerivedMediaSelection` evidence, regardless of whether reads delegate
to a generated packet adapter or the lazily opened original source.

`av_sync` exposes the validated eight-millisecond `interactive_av_sync_policy`, immutable
`AvSyncFrameTiming`, caller-owned `AvSyncFrameReadiness`, `AvSyncRecovery`, complete
`AvSyncOutcome`, and playback-owned `AvSyncCoordinator`. The coordinator constructs with an audio
master and either the engine policy or a caller-supplied validated policy. It exposes clock mode,
policy, scheduler statistics, exact clock reads and reanchoring, monotonic fallback, audio-master
restoration, and nonblocking coordination against either immutable media timing or a distinct live
deadline and duration. It applies discontinuity rebases through the concrete clock.

`export_queue` exposes exact route and request values, backend-selection evidence, video and audio
input provenance, complete elementary packet streams, and one immutable render-export artifact. Its
`ExportMediaSession` production implementation consumes an `AcquiredMediaSource` directly.
`SharedExportDecodedFrame` and `HeadlessGraphVideoRenderer` bind each decoded frame to the ordinary
immutable graph evaluator without copying pixel storage, while `ExportVideoDelivery<V>` and
`ExportAudioGraph` keep delivery color and prepared audio processing with their subsystem owners.
`render_and_export` admits one video and or one audio route, selects each encoder once, drives every
source, decoder, graph, delivery, audio, and encoder stage to completion, and publishes only complete
validated elementary streams. `render_and_export_tracked` preserves the same transaction while
advancing an indeterminate `JobProgress` at semantic admission, source, decode, graph, delivery,
audio, encode, validation, reset, and publication boundaries. Neither path exposes a muxed container
or persistence result.

`export_jobs` exposes `ExportJobQueueConfig`, stable `ExportJobStatus` codes,
`ExportJobSnapshot`, `ExportJobExecutionContext`, `ExportJobExecutor<R>`, and
`ExportJobQueue<R>`. EngineControl callers submit unique logical jobs with already-retained
dependencies, poll without waiting, observe immutable progress and failure context, retain typed
results, pause, resume, retry, cancel, or remove terminal jobs, and perform explicit blocking-safe
shutdown. Each attempt receives fresh generic job control, export-priority media operation control,
and progress while retaining the logical `JobId` across recovery.

`playback` exposes the `PlaybackPrefetchEvaluator` seam, concrete
`GraphPlaybackPrefetchEvaluator`, playback-owned `PlaybackPrefetcher`, and structured prediction
submission and completion reports. It also exposes `DecodedFrameMetadata`,
`PlaybackSceneFrame<V>`, `PlaybackCacheIdentity`, `PlaybackDisplayTransform<V>`,
`CpuPlaybackDisplayTransform`, `PlaybackViewportFrame<O>`, `PlaybackFrameEvaluator<O>`,
`GraphPlaybackFrameEvaluator<T, N, V, D>`, `PlaybackAudioOutput`, `PlaybackPoll`, and
`PlaybackOrchestrator<O>`. Foreground orchestration admits one exact frame at a time, executes it on
a playback-priority worker, validates cache identity and scene meaning, applies display color,
coordinates presentation from a caller-supplied shared-clock deadline and live duration, and retains
both a saturated viewport payload and its resolved synchronization decision. Poll outcomes expose
exact media timing, master position, drift, live display interval, correction, reason, and recovery
evidence. Foreground work can be cancelled without presenting a late result, and audio discard state
is inspectable. Submission, polling, audio admission, coordination, and clock switching require the
playback domain and never block.

`transport` exposes `PlaybackTransport<O>`, `PlaybackTransportCommand`, validated configuration,
exact mode and direction, `DroppedFramePolicy`, drop evidence, audio state, degraded-condition
codes, immutable snapshots, command execution, and nonblocking updates. It composes the existing
foreground and prefetch owners, maps frame cadence to the independent clock timebase with checked
rational arithmetic, and protects seek, scrub, step, resume, and loop-boundary frames from dropping.

`audio_mix` exposes `apply_edit_batch_with_clip_mix` for caller-owned `EditorialProject` and
`ClipMixState` values, and the narrower `reconcile_clip_mix_edit_batch` for an already validated
`EditBatchResult`. The combined operation returns the ordinary timeline result only after both
cloned states validate and publish.

`project_transaction` exposes `CompoundProjectAction`, `CompoundProjectTransaction`,
`CompoundProjectActionResult`, `CompoundProjectTransactionOutcome`,
`MAX_COMPOUND_PROJECT_ACTIONS`, and `execute_compound_project_transaction`. One nonempty sequence of
at most 64 actions can select the root, edit the timeline, mutate a retained graph, execute a media
or extension command, or mutate authored clip-mix state. Every action validates the unpublished
aggregate, and the complete sequence publishes exactly once or leaves the document unchanged.

`lifecycle` exposes `EngineLifecycle` as the single EngineControl owner. Canonical
`EngineSubsystem` values describe shared state, projects, devices, playback, rendering, and export.
`EngineLifecycleActionKind` names initialize, recover, prepare-sleep, wake, and teardown work.
Generated `EngineLifecycleSnapshot` values expose lifecycle phase and revision, engine state revision,
lifetime, health, ordered subsystem state, retained classified failure evidence, and one exact
pending action. Subsystem owners complete or fail the full `EngineLifecycleAction` token after work
on their legal domain. `EngineWorkKind` admission returns revision-scoped permits only when every
required subsystem is ready in the same running lifetime. The retained `LifecycleSignal` is the
lock-free observation path for latency-sensitive consumers. `begin_sleep` and `begin_wake` publish
acknowledged intent without doing subsystem work inline; `begin_shutdown` and `begin_restart`
supersede pending power work through new exact action revisions.

`error` exposes `EngineErrorCoordinator`, monotonic `EngineFailureSequence`, stable
`EngineFailureDisposition` and `EngineRecoveryRequest` codes, immutable `EngineFailureRecord` and
failure reports, exact `EngineRecoveryAction` tokens, and recovery start and completion reports.
Every returned report carries the canonical lifecycle snapshot that determined workflow admission.
Mutable active-failure and bounded-history bookkeeping remains EngineControl-owned and deliberately
non-`Sync`; immutable records and snapshots remain `Send + Sync`.

`resource_arbitration` exposes complete `ResourceArbitrationConfig` and `ResourceClassBudget`
values; exact `ResourceRequest` values for playback, render, export, background, and recovery;
`EngineResourceArbiter`; cooperative `ResourceReclaimer` registration; noncloneable
`ResourceReservation`; opaque `ArbitratedResource<T>` bindings; and deterministic admission,
degradation, reclaim, and snapshot evidence. The six stable classes are decoded buffers, GPU
memory, cache, audio, AI, and export. Lower subsystem allocators remain authoritative and callers
charge the same declared managed payload into this higher shared envelope.

The remaining placeholder module contains documentation only.

## Architecture and data flow

### Canonical editorial command state

The scenario engine begins with stable project identity and empty state. Import requires exact
`slice/video-cfr` version 1 metadata, lowercase manifest and payload digests, 24 fps, 96 frames, and
96 by 54 extent. It reads at most 64 MiB and independently verifies the payload digest. Placement
accepts only timeline frame zero. Trim accepts only source `[24, 72)`. The mirror action constructs
the exact source, transform, and output graph, two image edges, binary64 matrix, nearest sampling,
transparent black edges, and derived timeline identity.

Every successful transaction stores a complete before and after content snapshot plus its stable
typed operation records. The active log uses `slice.op.import`, `slice.op.insert`, `slice.op.trim`,
and `slice.op.effect`. All operations in one ordered transaction carry the same resulting revision,
and that transaction becomes one undo unit only after every action succeeds. Undo and redo restore
complete content without filesystem side effects or reimport and advance the global revision
monotonically. Rejected actions and failed trailing actions leave state and both history stacks
unchanged.

### Authored project command history

`ProjectCommandHistory` consumes one real `ProjectDocument` and uses the same generic bounded
`SnapshotHistory` core as `ScenarioEngine`. One apply command captures the current immutable project
snapshot, delegates its typed mutation to the project owner, and records before state, after state,
and stable mutation kind only when the document revision changes. An accepted semantic no-op returns
its typed result but does not add history or clear redo. A failed command changes neither branch nor
the project document.

Undo and redo inspect their retained target before mutation, ask `ProjectDocument::restore_snapshot`
to publish that complete semantic state behind the exact current revision fence, and move the entry
between branches only after restoration succeeds. Restored contents receive a fresh monotonic
document revision, so stale callers cannot regain authority when an old snapshot becomes selected.
The oldest undo entry is evicted when capacity is full, and the redo branch clears only after a
successful new authored change. History is process-local operational state: project SQLite
persistence stores the selected snapshot through its existing schema and never serializes stacks,
capacity, or command metadata.

The authored project mutation vocabulary contains all three real project media commands, every
project extension command, and the compound transaction. The same history owner also records
project settings transactions as `ProjectSettings` changes through the project-owned validation
path. Timeline, graph, media, extension, root selection, authored clip-mix, and settings changes all
enter this history surface and gain no implicit undo stack in their domain crates.

### Compound whole-project transactions

`execute_compound_project_transaction` enters one `ProjectDocument::edit` with the caller's exact
revision, then applies up to 64 nonempty ordered actions to the unpublished draft. Timeline edit
batches use `apply_edit_batch_with_clip_mix` so clip identity inheritance, transfer, and removal stay
atomic with editorial changes. Every retained timeline graph is then reconciled with the new
canonical compilation through the old canonical graph as a three-way merge base, preserving
nonconflicting parameters and custom topology while rejecting overlaps.

Graph batches use the ordinary checked `GraphTransaction`, media and extension actions reuse their
project draft command seams, clip-mix batches use the audio-owned revisioned state, and root
selection uses the project owner. Complete aggregate validation follows every action. A late failure discards all
earlier draft work and action results; success publishes one outer document revision, returns one
ordered result vector, and records one `Compound` history unit whose snapshot supports exact undo
and redo. Operational audio resources never enter that snapshot.

The autosave consumer contract takes the exact immutable snapshot selected after apply, undo, and
redo and sends it to `superi-project::ProjectAutosaveController::execute` with explicit elapsed
time. The project owner publishes and reopens each current-schema artifact, and equality proves that
history-selected revision, settings, referenced media, editorial meaning, retained graphs, and
opaque extension records all survive. This is a real cross-crate consumer proof, not an engine
dispatcher integration: a future
background I/O host must own the controller and route typed commands without blocking EngineControl,
Playback, Render, or Audio.

### Project crash recovery transaction

The dispatcher may attach one `ProjectRecoveryCoordinator` only after a real project history is
present and only when the supplied file-backed `ProjectDatabase` reloads to the exact selected
snapshot. The coordinator owns that database handle and the project-owned recovery catalog together,
so no restore path can publish durable state outside serialized EngineControl commands.

Discovery delegates exact namespace scanning and classification to `superi-project` and returns one
complete state beside the current project history. Comparison is read-only and uses the current
selected snapshot, including editorial, settings, authored clip-mix, root, and retained graph state.
Restore first revalidates and loads the candidate, computes that typed comparison evidence, clones
the current document, and applies `restore_snapshot` behind both catalog and project revision fences.
A changed prepared snapshot is published through `ProjectDatabase::replace` before the in-memory
history is swapped and reset. Persistent failure therefore leaves disk, history, candidate, command
sequence, and event stream unchanged. Dismiss delegates the exact synchronized tombstone transition
to the project owner and retains the active project unchanged.

Discover, restore, and dismiss reserve command and event sequence space plus bounded queue capacity
before filesystem mutation, then emit one complete replacement recovery state correlated to the
caller transaction. Compare advances only the successful command sequence and emits no event.
Candidates remain after restore until a separate revision-fenced dismissal succeeds.

### Complete editor state capture

`InspectEditorState` borrows the exact selected `ProjectHistoryState` once and derives project
diagnostics, resolved settings, canonical timeline, graph, and clip-mix documents, bounded extension
descriptors, and exact audio track evidence from that same immutable snapshot. Audio projection
retains integral sample clocks, ordered semantic source and destination channels, explicit route or
mute targets, and exact structural seam reports. Identity-mapped clips are audited without
rounding; a retimed clip that cannot be reduced to exact linear sample spans remains explicit
unsupported continuity while its canonical authored state stays available.

Optional owners remain data rather than aggregate failure. Automation and cached recovery are
attached or detached, while playback and export separately retain attachment, pending or
unobserved state, and their latest dispatcher-observed complete replacement. The inspector does not
poll a worker, execute a playback command, scan the filesystem, advance a project or subsystem
revision, reserve event capacity, or publish an event. The successful command sequence is the one
public resynchronization fence for every projected domain.

### Engine command dispatch and event publication

One `EngineCommandRequest` combines a validated caller transaction identity with one typed command.
Scenario execution requires an exact expected revision and one to 64 ordered actions. Lifecycle
inspection and mutation, operation-labeled classified failure, exact recovery start, completion,
failure and inspection, shutdown, restart, work admission, permit validation, stable interactive
transport control, attached project apply, undo, redo, recovery discovery, comparison, restoration,
dismissal, and inspection, and every logical export
queue action all route through the same dispatcher
rather than creating parallel control paths. The lifecycle-attached owner checks EngineControl
before observing or mutating authoritative state.

Before a state-changing command executes, the dispatcher reserves bounded event capacity and
checks command and event sequence exhaustion. Success advances the command sequence once and emits
one full replacement scenario, project, project-recovery, lifecycle, or recovery event when state changed. Project
commands publish no event for a semantic no-op, and event capacity is preflighted before the
document or either history branch can change. Recovery state has
its own monotonic revision and contains the coherent lifecycle snapshot, active failures in
canonical subsystem order, bounded diagnostic history, and the exact pending coordinator action.
A synchronous failure advances neither sequence and publishes no event. Playback is the explicit
cross-domain case: EngineControl
admits and orders one command, reserves its event, and publishes it through a capacity-one
nonblocking channel. `PlaybackCommandExecutor` applies it on the playback domain to the real
transport and returns complete state through the paired completion channel. Another state-changing
command cannot overtake that reserved event, while read-only inspection may continue. Accepted
playback failure still emits complete state plus bounded classified evidence because the transport
may have made externally visible discontinuity progress before reporting the failure. The queue
retains at most 64 events, transaction IDs are bounded to 128 bytes, and copied failure messages and
context-frame counts are bounded before lifecycle storage or event publication. Classified
operation labels share the coordinator's 128-byte bound.

The full dispatcher constructs and owns `EngineErrorCoordinator` beside lifecycle. Classified
commands route report, begin, complete, and failed-recovery reclassification through it. Legacy
runtime-failure and begin-recovery commands derive the same coordinator record and request, and a
legacy lifecycle action that addresses pending recovery resolves through the retained exact
coordinator token. Legacy results remain lifecycle shaped, but their events contain the composite
replacement state, so no command path can bypass recovery-intent, sequence, attempt, diagnostic, or
stale-action validation. Scenario-only mode owns neither lifecycle nor the error coordinator.

The export controller type-erases only the runtime result type, not queue ownership. Submit, resume,
and retry require a current export permit, and recovery attempts replace the prior permit before a
fresh worker attempt. Poll compares complete state around the canonical queue refresh and emits one
revisioned full replacement event for observed progress, status, dependency, result, or failure
changes. Inspection has no event, while pause, cancel, and removal remain available during
degradation. Executor closures and typed artifacts never enter the command or event stream.

The optional project owner follows the same stable dispatcher boundary. `InspectProjectSettings`
resolves one immutable snapshot without mutation or an event. `ExecuteProjectSettings` first
validates the transaction against a cloned project to determine whether semantic state can change,
then reserves event capacity only for a real change. Actual publication reuses
`ProjectCommandHistory::execute_settings_transaction`, which delegates to the project document,
records a semantic change in the shared history, resolves the resulting snapshot, and emits one
complete `ProjectSettingsChanged` replacement correlated to the caller transaction and project
revision. A no-op succeeds even when the event queue is full, advances no project revision, and
publishes nothing. Missing projects, off-domain calls, stale revisions, invalid values, and full
queues before a real mutation leave the authoritative project unchanged.

`InspectProjectDiagnostics` is another read-only command over that owner. The dispatcher borrows
the selected immutable history snapshot, delegates all canonical preparation and semantic hash
framing to `superi-project`, returns one typed complete diagnostics report, and commits only its
successful command sequence. It reserves no event capacity and does not alter the project revision,
undo or redo depth, selected snapshot, lifecycle, recovery state, or event queue. A synchronous
preparation or missing-owner failure advances no sequence.

Authored automation follows the same serialized publication law without entering project history.
The full dispatcher consumes one `AudioAutomationState` owner and rejects duplicate or scenario-only
attachment. Inspection returns a complete replacement snapshot with no event. A transaction first
executes the audio owner's exact candidate validation to determine semantic change, then reserves
event capacity only when needed. Successful mutation advances one audio revision and emits one
correlated replacement event; no-op, stale, invalid, missing-owner, off-domain, and full-queue paths
leave authoritative automation and the event stream unchanged. The dispatcher never performs this
work on the audio callback domain.

Settings resolution is read-only and preserves subsystem ownership. Timeline values become exact
`FrameRate` and `TimecodeFormat`; color remains built-in or pinned identity; audio becomes one
`Timebase` and ordered `ChannelLayout`; cache becomes automatic or an exact `CacheBudgetLimit`;
proxy becomes disabled or one `DerivedMediaQuality`; and render becomes exact rate, extent, pixel,
alpha, color-target, and stable fingerprint values. No owner is reconfigured merely by inspection.

### Coherent integration validation

The dispatcher retains the latest completed playback replacement, bounded playback failure, and
complete export queue replacement state as ordinary immutable observations. The playback bridge
updates that cache before publishing its ordered event, and export commands replace the cache only
after the canonical queue returns complete state. Event draining therefore cannot erase the latest
validation evidence.

`integration_validation_snapshot` starts with the exact canonical introspection projection, then
adds current scenario, lifecycle, error coordinator, playback, and export observations in one
EngineControl call. It derives the overall condition from the canonical lifecycle phase and active
recovery action, asks the lifecycle snapshot for each playback, rendering, and export admission
result, and reports exact permit or denial evidence. Deterministic findings reject introspection
state that disagrees with lifecycle, recovery, or workflow state, recovery state that disagrees with
the lifecycle action, and endpoint state that disagrees with attachment state. The inspector never
polls a worker, acquires a new runtime lock, mutates a revision, or creates a parallel health owner.

### Registry, introspection, and upload

Default registry construction atomically registers permissive Rust codecs plus primary priority-100
Matroska or WebM, MP4 or MOV, MXF, and WAV or AIFF sources. Source implementations and stable IDs
are constructed and preflighted before source mutation. `os-codecs` may add host-discovered codec
operations, and `vendor-codecs` requires explicit worker configuration.
Introspection reads declarations only, orders stable records, separates primary and fallback tiers,
and never constructs a source or codec. The dispatcher then composes those immutable declarations
with one optional resource-arbitration snapshot and its own recovery snapshot. Workflow readiness
is derived through `EngineLifecycleSnapshot::admit`, so playback, rendering, and export report the
same dependency decisions enforced for real work. Active diagnostics cross this seam only as the
core-owned `UserSafeError`; raw messages, sources, contexts, operation labels, and failure sequence
identity remain private. The read changes no scenario, lifecycle, recovery, command, or event state.

`VideoFrameUploader` accepts CPU storage, validates and borrows exact planes, asks the GPU uploader
for pooled textures, and preserves format, time, duration, metadata, complete color pipeline, and
allocation lifetime. It rejects GPU and external storage with a classified degraded error.

The color metadata integration path preserves source tags and ordered transforms through media,
graph, timeline, and cache wrappers, then derives independent display and delivery branches. Each
branch validates its terminal stage kind and leaves the cached scene state unchanged. Foreground
playback executes one CPU display transform, while render-export orchestration invokes a caller-owned
delivery transform and validates its complete output color and alpha meaning. Native GPU graph
binding, viewport presentation, and export readback remain absent.

### Subsystem lifecycle and coherent work admission

`EngineLifecycle` requires the EngineControl domain, validates the canonical dependency plan, and
stores all authoritative mutable lifecycle state in `DomainOwned`. Its first generated action asks
the shared-state owner to initialize. Exact action completion advances projects, devices, playback,
rendering, and export one at a time in dependency order. Projects and devices depend on shared
state; playback and rendering depend on projects plus devices; export depends on projects plus
rendering. After all six resources are owned, the engine participant acknowledges the shared
`LifecycleCoordinator` startup revision and the common phase becomes `Running`.

Every committed state change publishes a new generation-tagged immutable snapshot through
`SnapshotPublisher`. Playback requires shared state, projects, devices, and playback; rendering
requires shared state, projects, devices, and rendering; export requires shared state, projects,
devices, rendering, and export. One nonterminal subsystem failure therefore denies only its
transitive work while unrelated work remains admitted. Recovery uses another exact action token.
Successful recovery clears the subsystem blocker while the latest reported failure remains
inspectable for the rest of that lifetime. A terminal failure advances the shared lifecycle phase
to `Failed` and denies every workflow.

Sleep preparation stops admission immediately, then issues `PrepareSleep` actions in reverse
dependency order for export, rendering, playback, devices, and projects. Shared publication and
authoritative project resources remain owned while device and workflow resources are released.
Wake issues `Wake` actions in forward order for projects, devices, playback, rendering, and export,
including a critical wake received while already running. Repeated notifications are idempotent;
an opposite notification cancels only its exact pending action and rejects any late completion.
Nonterminal transition failure preserves conservative teardown ownership, completes to an explicit
degraded stable phase, and remains recoverable. Terminal transition failure enters `Failed`.

`EngineErrorCoordinator` borrows this lifecycle for each mutation instead of owning a second
subsystem model. A ready subsystem failure receives a monotonic sequence and bounded operation
label, appends stable subsystem, operation, sequence, and attempt context, and becomes one
`DiagnosticEvent` before the error moves into lifecycle state. The event retains the raw source
chain and ordered context internally while deriving `UserSafeError` only from core category and
recoverability. Reviewed subsystem, recoverability, and disposition fields are user-safe;
operation labels, sequence values, and attempt counters remain internal.

Retryable, degraded, user-correctable, and terminal errors map to retry, continue-degraded,
user-correction, and restart dispositions. Only `RetryOperation`, `RestoreSubsystem`, or
`ApplyUserCorrection`, respectively, may emit a lifecycle recovery action. A terminal disposition
cannot emit recovery in the current lifetime. Exact failure sequence, attempt, request, and
lifecycle action tokens reject stale completion before ordinary caller validation can mutate
state. Successful completion removes the active failure and emits a recovery event. Failed
completion flows through `EngineLifecycle::fail_action`, receives a new sequence and classification,
and remains active with the accumulated attempt count. History evicts the oldest event at its fixed
capacity while one active record per subsystem remains separately queryable.

Shutdown cancels an exact initialization, recovery, sleep, or wake action, normalizes resources
already released for sleep, and emits teardown actions only after every owned dependent has
released its resource. This produces the exact inverse of initialization for all resources still
owned. Initialization failure enters the same reverse path for already owned dependencies. A failed
teardown retains ownership and blocks its dependencies, while an independent branch may continue
stopping. Retry must release the retained resource before the engine can acknowledge `Stopped`.
Restart first completes this teardown, increments the engine lifetime, clears resolved failure and
transition state, and begins the canonical startup again. Action tokens and work permits carry
lifetime and revision identity, so late completion and stale work are rejected. EngineControl never
executes subsystem acquisition, quiescence, revalidation, or teardown inline and therefore remains
nonblocking. When the logical export queue is attached, the dispatcher rejects completion of its
export teardown action until all retained jobs are final. Stable all-job cancellation plus polling
settles that state before the blocking-safe runtime handle joins workers.

### Shared finite-resource arbitration

The engine constructs one validated configuration with an exact shared hard limit and exactly one
protected floor plus hard ceiling for every resource class. Protected floors may be borrowed while
unused, but global pressure cannot reclaim another class below its floor. A class-local ceiling may
ask its own registered owner to replace existing reservations. Admission is serialized, commits
class and shared accounting atomically, and returns either one exact noncloneable reservation or a
typed degradation with the remaining shortage and semantic fallback.

Pressure visits cache, AI, export, decode, GPU, and audio owners in fixed order after any required
same-class replacement. Each callback runs outside the accounting lock and can make progress only
by releasing its own RAII reservations. The arbiter ignores reported byte claims, measures live
counters after the callback, retains classified callback failure evidence, and continues to the
next eligible owner. A callback-local guard rejects recursive admission before it can wait on the
serialized gate. Audio reservations must be prepared outside the real-time callback.

`ArbitratedResource<T>` binds an admitted reservation to the caller's real value without inspecting,
copying, converting, or mutating it. Decode timing, pixel precision, metadata, color, and alpha
therefore remain owned by the media value. Cache pressure bypasses retention, playback decode
reduces lookahead, GPU and AI work defer, playback audio preserves its clock with timed silence,
and export pauses rather than publishing a partial or semantically changed result. Ordinary finite
pressure is scheduling state, not an automatic lifecycle failure; a caller may route a persistent
classified failure through the separate error coordinator.

### Project snapshot, timeline graph, and media preparation

The legacy caller supplies one validated `EditorialProject`, root timeline, immutable backend
registry, explicit request for each reachable linked media identity, fallback policy, and operation
context. That path first consumes `superi-timeline::compile_timeline`. The project path instead
supplies one immutable `superi-project::ProjectSnapshot` and clones its retained selected-root
compilation, including its exact editable graph revision and direct changes. It never recompiles the
snapshot. Both paths recursively identify every media clip in the same reachable nested-timeline
closure. Missing, duplicate, or extra media requests and empty or duplicate decoder sets fail before
any bundle is published.

For each media identity, the engine binds the project's persistent fingerprint when the request
omits it and rejects a conflicting caller identity. `BackendRegistry::probe_source` performs the
bounded content selection, the chosen adapter opens once, and the engine verifies the returned
media ID and fingerprint again. Each requested `StreamId` resolves to its complete opened
`StreamInfo`; optional audio representation is applied through the ordinary `DecoderConfig`; and
registry ranking selects exactly one decoder factory. Source and decoder selections retain stable
backend IDs, fallback candidates, fallback-use state, container confidence, and probe bounds.

Callers that begin from persistent project state can construct the request through
`MediaResourceRequest::from_project_media`. The project layer interprets the stored target and
resolves relative paths from the absolute `.superi` path; engine supplies the resulting local
`SourceLocation` to media I/O while preserving project identity and integrity evidence. This keeps
path portability in project, source mechanics in media I/O, and acquisition orchestration in engine.

The graph compilation, all opened sources, and all live decoders remain local until a final
cancellation check. An open or decoder factory error returns directly and never retries through
fallback candidates. The returned `TimelineResources` is therefore one shared preparation boundary
for later playback, render, and export orchestration without copying graph, time, pixel, metadata,
alpha, source, or decoder meanings. Render-export now consumes an acquired media owner from this
bundle, while playback source binding and direct use of its compiled graph remain later work.

### Derived-media generation

The caller prepares decoded frames or audio blocks, one exact `EncoderConfig`, and a matching
cache-owned `DerivedMediaRequest`. The engine derives canonical settings again and rejects a
mismatch before cache lookup. On an exact catalog hit it returns the immutable prior artifact
without constructing a codec. On a miss it selects the primary registered encoder with fallback
disabled, sends each input with cooperative operation checks, drains packets, flushes once, and
requires end of stream.

Packets remain local until the complete lifecycle succeeds and a final cancellation check passes.
The engine hashes stream identity, payload bytes, exact timing, keyframe state, and deterministically
ordered typed metadata, then returns the complete packet payload, digest, and nonzero encoded byte
length to the cache catalog for one exact publication. A failure publishes nothing, so a prior
artifact remains live or the authoritative original source remains the cache-declared fallback.
This path generates elementary packet media only. It does not render inputs, rescale quality, mux a
container, persist files, select proxies for playback, or mutate project state.

### Transparent proxy substitution

The caller supplies one authoritative source identity and revision, explicit scheduler quality and
fallback policy, available immutable derived artifacts, and a lazy original-source opener. The
engine first recreates the complete cache media identity from source ID and fingerprint. It admits
only proxy-purpose artifacts that match that identity and revision exactly, use a known quality,
contain complete nonempty packets in the declared stream and timebase, expose at least one timed
packet and keyframe, and construct valid source information.

The engine converts admitted cache qualities to the matching scheduler values and delegates exact,
nearest-lower, stable lowest-cache-ID tie, unavailable, and source-only decisions to
`superi-concurrency::DerivedMediaRequest::select`. A selected proxy becomes a packet-backed
`MediaSource` whose outward `SourceInfo` retains the authoritative original `SourceIdentity`, whose
stream carries encoder codec and timebase plus codec configuration when present, and whose reads
preserve exact generated packets. Exact seek returns the requested packet boundary while beginning
bounded decode at its preceding keyframe; prior and nearest keyframe seeks are deterministic.

When selection returns the source, the engine invokes the original opener only then and rejects an
opened source whose complete identity differs. Missing, stale, wrong-fingerprint, optimized,
unknown-quality, malformed, higher-only, and source-only cases therefore cannot silently replace
the original. The source-only policy is the explicit final-delivery boundary; valid proxies remain
available without changing which bytes final delivery reads.

### Predictive playback cache population

The playback owner derives a finite exact-frame plan through `superi-cache`, then submits it to
`PlaybackPrefetcher` with one stable job identity. Submission first verifies the playback domain,
cancels the prior generation, and returns immediately after nonblocking worker-pool admission.
Nearest critical requests execute before farther prediction and trailing work inside one
playback-priority cache job. Each frame boundary checks cooperative cancellation and advances exact
determinate progress only after successful evaluation.

`GraphPlaybackPrefetchEvaluator` converts each predicted time into the caller-bound output and
region request, evaluates the immutable `GraphEvaluationSnapshot` through
`OwnedHostFrameMemoryCache`, and discards only the returned handle. Existing exact values therefore
skip graph work; misses populate the same final and intermediate cache tiers used by foreground
evaluation. Failure stops only replaceable prefetch, retains its classified error in the polled
completion, and leaves transport, project meaning, source selection, and final-render output
unchanged. The controller stores a weak pool reference, so playback-thread destruction cannot own a
blocking worker shutdown.

### Foreground playback orchestration

The caller supplies a prepared immutable graph, exact output endpoint and region, a budgeted scene
cache, complete project, media, parameter, color, and render-setting identity, a display transform,
the producer and audio clock from one output buffer, a lifecycle-owned worker pool, and a bounded
viewport sender. `PlaybackOrchestrator` stores only weak pool ownership and admits exactly one
foreground frame at a time, leaving source selection, graph construction, transport controls, and
drop policy to their dedicated owners.

`PlaybackSceneFrame<V>` retains exact output timing, decoded representation and metadata provenance,
nonterminal scene color history, and alpha association. `GraphPlaybackFrameEvaluator` evaluates the
ordinary `GraphEvaluationSnapshot` through `OwnedHostFrameMemoryCache`. Its validating adapter
reuses and inserts only values whose timestamp and complete scene pipeline match the graph-owned
evaluation identity, then performs display conversion on the worker. The concrete CPU transform
accepts canonical binary16 working images and produces premultiplied binary32 display images plus a
terminal `ViewportColorMetadata` branch. The generic display seam preserves a future GPU-resident
payload without moving native presentation into the engine.

The audio producer admits complete borrowed interleaved frames or rejects the submission. Its paired
device consumer remains the only writer of `AudioMasterClock`. `AvSyncCoordinator` observes that
clock and passes the ready frame's exact PTS, nominal duration, and caller-owned readiness to the
shared `AvSyncScheduler`. The engine interactive policy uses an eight-millisecond tolerance,
forty-millisecond drop threshold, ten-second discontinuity threshold, twenty-millisecond maximum
video-only interval correction, and four-drop starvation cap. The lower-level reusable scheduler
default is unchanged.

Polling returns rendering, bounded clock hold, corrected or nominal presentation, explicit drop,
viewport backpressure, or classified failure without waiting. A scheduler rebase request is
rescaled exactly into the active clock timebase, applied once through `PlaybackClock::reanchor_at`,
then scheduled once more to return an aligned presentation carrying the original discontinuity.
Live display intervals never replace `PlaybackViewportFrame` PTS or nominal duration.

The current single-ready-frame owner supplies conservative nondrop evidence. A resolved
presentation is stored beside a frame returned by viewport backpressure, so retry cannot repeat a
correction, rebase, or scheduler statistic. Invalid graph or color output is not cached, clears only
that request, and permits later recovery. Explicit audio loss and restoration switch the concrete
clock source while preserving the current timeline position and the existing scheduler owner.

### Render and export orchestration

The caller supplies a revision-scoped lifecycle permit, exact source seek position, up to one video
and one audio route, one prepared `ExportMediaSession`, the immutable backend registry, required
graph, delivery, and audio stage owners, and an export-priority operation context. The production
session adapter operates directly on `AcquiredMediaSource`, so timeline resource preparation and
export share the same source, decoder, identity, and registry contracts.

Each route validates source kind and complete encoder representation before selecting one encoder
factory through normal registry ranking. Registered fallback is policy controlled and retained as
evidence, never used as an exception retry. The transaction resets decoders and encoders, seeks
exactly, rejects partial source reads, routes packets, drains decode outputs before and after flush,
processes video through the shared headless graph plus delivery seam or audio through its graph seam,
drains encoders before and after flush, and requires end of stream from every codec.

Decoded video is bound through one shared owner slot to the existing `PlaybackSceneFrame<V>` graph
value. Graph output must preserve exact timing, source provenance, nonterminal color history, alpha
meaning, and graph revision. Delivery must preserve timing and metadata, match the encoder format,
append a coherent terminal output-color history, and give the encoder that terminal color
interpretation. Audio must preserve exact sample timing,
metadata, graph identity, format, sample precision, and channel layout.

Each packet must retain nonempty data, configured stream and timebase, required input metadata, and
timing whose normalized exact interval union equals the submitted input union. Only after all routes
validate, completed state is reset, the caller remains uncancelled, and a fresh lifecycle snapshot
accepts the original permit does the engine return `RenderExportArtifact`. Any transaction failure
uses a fresh export context to reset all created encoders and selected decoders. Output remains
complete elementary packet streams in memory, not a muxed or persisted container.

### Export job orchestration

`ExportJobQueue<R>` is the EngineControl-owned logical scheduler around export work. Construction
fixes worker, pending, and retained-job bounds. Submission reserves one unique `JobId`, requires
every dependency to be an older retained job, and stores the executor separately from a worker
closure so temporary pool saturation does not lose reconstructible work. Those admission rules
make dependency edges acyclic without a separate graph traversal.

Polling resolves dependency history before dispatch. A ready attempt enters the shared bounded pool
as `JobKind::Export` at `JobPriority::Export` with fresh `JobControl`, `JobProgress`, and
`OperationContext` values. The executor can check both cooperative cancellation models and can call
`render_and_export_tracked` so decode, graph, delivery, audio, and encode work contributes semantic
progress. Completed typed values remain retained as `Arc<R>` and are never published for cancelled,
paused, failed, or dependency-failed attempts.

Pause and cancel request both cancellation tokens and settle only after the worker acknowledges the
request. Pause discards a raced result and leaves the logical job resumable. Resume and retry create
a new attempt with progress reset to revision zero. Retryable, degraded, and user-correctable errors
retain actionable context and permit retry; terminal errors do not. Recoverable failures remain
unresolved in dependency history, so dependents wait for a successful retry, while terminal failure,
cancel, and dependency failure finalize deterministic downstream outcomes. Terminal removal is
rejected while another retained job still names the candidate as a prerequisite. Normal control and
observation never join workers; explicit shutdown is restricted to a blocking-safe execution domain,
cancels unfinished work, drains completions, and joins the owned pool.

### Exact interactive transport

`PlaybackTransport` retains one exact frame range, playhead, signed reduced rate, optional loop,
clock and frame anchors, discontinuity epoch, foreground request, drop counters, and deterministic
job namespace. Seek, scrub, pause, resume, frame step, rate, direction, and loop changes cancel the
current foreground handle and prefetch generation, reanchor cadence at the current shared clock,
request callback-owned audio discard, and submit one protected exact target immediately.

Frame deadlines are distinct from scene timestamps. Checked cross-timebase rational conversion
rounds frame deadlines upward and clock-derived frame selection downward, so polling frequency does
not accumulate drift and no frame presents early. Loop endpoints and explicit user targets are
never droppable. The optional late policy cancels only ordinary playing work, caps consecutive
skips, and marks one visible frame as forced progress at the ceiling. A failed foreground frame
pauses advancement and discards queued audio until explicit recovery, natural end discards queued
tail audio, a failed newest prefetch generation remains replaceable, and viewport or audio
limitations stay visible in the immutable snapshot.

`PlaybackTransportCommand` names inspection, play, pause, seek, scrub lifecycle, loop, rate,
direction, and frame-step actions without carrying process-local `Instant` values or bulk media.
`execute_command` receives the executor-owned instant on Playback and returns complete state after
success. The engine bridge also snapshots after failure, preserves command and event correlation,
and prevents lifecycle degradation or another authored mutation from overtaking the in-flight
transport state event.

The output producer and callback coordinate discontinuities through atomic requested and applied
generations. Producer admission remains blocked until the callback clears its consumer-owned queue.
Inactive audio is muted, and non-normal or reverse playing audio is explicitly muted because the
current borrowed sample seam has no timestamped rate-conversion contract. Transport does not bind
decoded sources, perform A/V drift correction, or submit native GPU work. Render and export
orchestration consume separate exact-time inputs and remain unaffected by live transport policy.

### Native audio plugin lifecycle

Background-domain discovery recursively walks caller-selected roots, canonicalizes each `.vst3`
bundle, sorts by path, and retains malformed entries as candidate-scoped failures. Audio Unit
candidates enter through explicit typed component identifiers because platform registry enumeration
remains outside this module. Scan replaces the candidate table deterministically and does not load
native code.

Every launcher must declare a separate process, a positive bounded message capacity, a finite
deadline no greater than 300 seconds, restart support, and a bounded descriptor. Enable validates
identity and layout, accepts saved state only for the same format and component identifier, starts
the worker, obtains one bridge and exact descriptor, and publishes Ready only after all checks pass.
Capture asks the worker for exact current state and rebinds its identity plus native and transport
latency to the live descriptor. A first activation or runtime failure publishes Faulted and retains
the last valid checkpoint; recovery restarts from that checkpoint. A repeated activation failure
publishes Quarantined until an explicit clear.

Processor preparation converts the ready worker bridge into
`superi_audio::plugins::PreparedIsolatedAudioPlugin`, which owns callback-safe timing-matched dry
fallback while the supervisor retains lifecycle and checkpoint authority. Runtime observations must
arrive on EngineControl and can move a ready worker to Faulted without editing durable state. Each
project record uses an instance ID derived from its audio graph node, embeds the format-neutral state
envelope in a strict `superi.audio-plugin-state` extension, and round-trips through ordinary project
save, schema validation, and database reopen. Two nodes using the same component therefore retain
separate state and user intent across compatible installed upgrades.

### Editorial audio intent

The engine applies a timeline edit batch to a cloned project, reads its typed fragment, inserted,
and removed identities, and translates only clip identities that already own explicit audio
intent. Right fragments inherit complete controls, replacements transfer them, and whole removals
delete them. The same mutation batch is revision checked by `superi-audio`; only then does the
engine replace both caller-owned states. Stable-identity trims, moves, slips, and rolls require no
audio mutation, so their user intent remains attached without synthesis.

## Dependencies and consumers

- `superi-core` supplies errors, identifiers, geometry, exact time and timecode, shared settings,
  and pixel and channel values used directly by canonical commands, project settings resolution,
  introspection, upload, playback prediction, foreground pixel and alpha meaning, and
  retained lifecycle failure evidence. Resource arbitration consumes the same classified error
  vocabulary while keeping finite-capacity degradation typed and local. Error propagation
  additionally consumes core-owned
  recoverability, ordered contexts, full source-chain diagnostics, typed visibility fields, and the
  separate user-safe projection.
- `sha2` supplies bounded fixture payload identity and complete packet-content fingerprinting.
- `superi-media-io`, `superi-gpu`, and `superi-codecs-rs` support source and codec registry assembly,
  content probing, source and decoder preparation, declaration, upload, codec-neutral derived
  generation, the common proxy or original source interface, and complete render-export source,
  decoder, encoder, packet, timing, metadata, and operation lifecycles.
- Platform and vendor codec crates are feature-gated.
- Cache now supplies color metadata, media-neutral derived publication, bounded playback prediction,
  budgeted scene retention, and an owned host evaluator adapter. Graph supplies the immutable
  evaluation snapshot. Timeline supplies the retained editable graph compilation, reachable
  editorial source relationships for resource preparation, and reduced signed `PlaybackRate` used
  by transport cadence. Concurrency supplies proxy selection, playback ownership, worker priority,
  bounded export workers, job progress and control, dependency history, nonblocking completion, the
  playback clock, A/V drift measurement and scheduling policy, bounded handoffs, the shared lifecycle
  coordinator and signal, EngineControl ownership for lifecycle and error bookkeeping through
  `DomainOwned`, and immutable snapshot publication.
  Media I/O supplies exact decoded frame and audio block semantics, image supplies color and scene
  artifacts, color supplies CPU display execution, and audio supplies the prepared graph identity,
  bounded producer, callback-owned discard acknowledgement, actual presentation clock, bounded
  format-neutral plugin state, and the isolated timing-matched process bridge. Export
  reuses the graph snapshot and playback scene envelope, invokes explicit delivery and audio
  processing seams, and validates all returned semantics before encoding. Timeline and audio are
  also jointly consumed by the clip-mix edit transaction. Effects supplies the safe
  `IsolatedOfxAdapter` contract, typed requests, graph projection, and OpenFX plugin lifecycle state
  consumed by the existing isolated supervisor. Engine defines launcher seams and native bundle
  discovery but does not implement concrete platform OpenFX or audio plugin transports. AI remains a declared dependency without
  production command integration. Project supplies the implemented mutable whole-project document
  and immutable snapshots consumed by settings dispatch, command history, compound transactions,
  and resource preparation, including snapshots reconstructed by its schema migration path. It
  supplies the checked document edit and restoration seams, authored media commands, draft
  validation, authoritative settings keys and defaults, durable clip-mix aggregate, versioned
  semantic diagnostics over canonical prepared evidence, and the typed autosave controller
  exercised by the selected-history-state consumer contract. The native audio
  plugin supervisor uses ordinary versioned project extension records to persist one exact state
  envelope per audio node instance without persisting operational readiness. Project remains
  authoritative for settings keys, validation, defaults, persistence, autosave scheduling,
  recovery-point publication, pruning, recovery discovery, semantic comparison, candidate
  classification, and exact dismissal, while engine owns typed subsystem resolution, bounded
  session history, active-database attachment, and the durable restoration transaction.
  Test-only rusqlite creates the exact legacy database fixture without entering the engine runtime
  graph, and its referenced-media path adapter constructs local `SourceRequest` values. Engine
  lifecycle still names only abstract project quiescence, and engine implements neither persistence
  nor a parallel project model.
- `superi-api` consumes dispatcher transactions and events plus command, media capability, and
  complete engine introspection and integration validation snapshots, preserving public adaptation,
  validation, scenario, project settings, project recovery, and authored audio automation seams
  without exposing engine-private owners. The API depends only on engine for generic project
  control, complete editor-state capture, settings, recovery, audio automation, and asynchronous job
  control and therefore does
  not reverse the reviewed runtime graph. Logical export inspection, pause, resume, retry, cancel,
  cancel-all, and finalized removal now project through API-owned strict job contracts over the same
  dispatcher and canonical queue. Submission, host polling, waiting, executor bindings, typed
  artifact access, and playback mutation remain outside the public protocol. The latest playback and
  export replacement observations remain visible through the read-only validation and complete
  editor-state queries. Generic project history is projected through strict API-owned DTOs, typed
  evidence, and correlated replacement events, while recovery is projected through opaque
  identities and user-safe findings.
- Cache and GPU retain their own exact local budget enforcement, audio retains its preallocated
  callback-safe queue, media I/O retains decoded value and lifecycle ownership, and export retains
  logical job and publication ownership. The engine arbiter composes a higher shared managed-byte
  envelope without replacing or weakening any of those local owners.
- `superi-cli` reaches engine validation only through `superi-api`; the engine-owned standalone
  helper hosts its temporary EngineControl dispatcher without widening the CLI dependency tier.

## Invariants and operational boundaries

- Canonical arguments are exact; the reference engine is not a general editor model.
- Source validation is bounded, digest checked, and atomic.
- Four mutations have stable typed IDs and complete internal prior state.
- Scenario transactions contain one to 64 ordered actions, use an exact revision fence, publish one
  revision and one undo unit, and leave state, history, sequences, and events unchanged on failure.
- Project history accepts capacities from 1 through 4096 and defaults to 64. A successful authored
  project change records one immutable before-and-after unit, full capacity evicts only the oldest
  undo entry, and a new successful branch clears redo. Failure and semantic no-op preserve both
  branches exactly.
- Compound project transactions contain one to 64 ordered nonempty actions, validate after each
  action, publish one project revision and one history entry, and retain no partial action evidence
  on failure. Timeline reconciliation preserves nonconflicting direct graph edits and rejects
  overlapping canonical and retained changes.
- Extension mutations always delegate to the project-owned revisioned command surface. History,
  compound transactions, dispatch, undo, redo, persistence, and autosave preserve the complete
  opaque record; engine does not reinterpret payload bytes, expand capability grants, infer runtime
  readiness, or duplicate plugin supervisor state.
- Native audio plugin scanning, enablement, checkpoint capture, recovery, quarantine clearing,
  disablement, and processor preparation require BackgroundJob. Runtime bridge observation requires
  EngineControl. No lifecycle operation may run on Audio or treat a native callback as a control
  owner.
- Audio plugin workers must be separate processes with bounded messages and deadlines plus restart
  support. Descriptor identity, layout count, fixed native and transport latency, and captured state
  must validate before Ready or durable checkpoint publication. Failure is candidate-local, the
  first activation fault retains the last valid checkpoint, and repeated activation failure
  quarantines without blocking a healthy sibling.
- A durable audio plugin record is keyed by one `AudioPluginInstanceId`, not reusable component
  identity. It preserves exact format-neutral state bytes and saved identity evidence across a
  compatible version upgrade, while worker handles, bridges, telemetry, lifecycle state, and
  quarantine remain operational and absent from project data.
- Project apply, undo, and redo require the exact current document revision. Undo and redo restore
  complete project semantics through the project owner, publish a fresh monotonic revision, and move
  a history entry only after restoration succeeds. Revision exhaustion, project validation failure,
  stale input, and empty undo or redo publish nothing.
- Project history is session-local operational state and never enters project snapshots or SQLite.
  The selected immutable snapshot remains the only durable project state, so database reload starts
  with empty history around the loaded revision.
- Project diagnostics inspection requires the same attached history owner and EngineControl domain.
  It delegates semantic hashing to project, advances only the successful command sequence, emits no
  event, reserves no event capacity, and changes no project snapshot, document revision, history
  depth, lifecycle state, or recovery state. A missing project or preparation failure advances
  nothing.
- The returned semantic hash excludes the outer document revision, so undo or recovery can restore
  the prior hash under a newer optimistic revision. Engine must not add session history, command
  sequence, transaction ID, lifecycle state, runtime readiness, or event state to project-owned
  diagnostics.
- Complete editor-state inspection requires one attached project history and EngineControl. It
  borrows one selected snapshot, advances only the successful command sequence, emits no event, and
  performs no recovery discovery, playback execution, export polling, filesystem work, or authored
  mutation.
- Canonical timeline, graph, and clip-mix resources preserve exact bytes, lengths, revisions, and
  SHA-256 identity. Extension records expose bounded identity and payload evidence without copying
  opaque payload bytes into the aggregate.
- Audio state preserves exact sample clocks, ordered semantic channel layouts, explicit track and
  per-channel routing, mute intent, and exact seam classifications. Retimed material that cannot be
  reduced without rounding remains explicit unsupported continuity.
- Optional recovery, automation, playback, and export owners remain explicit. Attachment,
  unobserved state, pending playback, latest observed state, and bounded failure evidence cannot be
  inferred or collapsed by the aggregate.
- Autosave policy, elapsed time, retention, filesystem publication, and recovery-point ownership
  remain in `superi-project`. Engine supplies only an immutable selected snapshot in its consumer
  proof. No engine dispatcher, lifecycle action, worker, or execution domain may perform blocking
  autosave filesystem work inline.
- Project recovery requires the full lifecycle-attached EngineControl dispatcher, one attached
  project history, and one file-backed database that reloads to the exact selected snapshot. Only
  the project owner scans, opens, compares, or dismisses recovery files; engine coordinates the
  active database and history transaction without exposing paths.
- Recovery comparison covers the complete durable aggregate, including editorial, settings,
  authored clip-mix, selected-root, and retained graph state, while remaining read-only and event-free.
- Restore requires exact catalog and current project revisions. It prepares all fallible project
  validation before publication, publishes the monotonic snapshot before the infallible history
  swap, resets undo and redo only for a semantic change, and never dismisses the source candidate.
- Discover, restore, and dismiss reserve event capacity and sequence space before work. Any stale,
  persistence, terminal, ownership, or saturated-queue failure leaves active disk state, project
  history, recovery catalog, candidate files, command sequence, and events coherent.
- Transaction IDs, pending events, retained error messages, and copied context frames are hard
  bounded. Successful command and event sequences are independently monotonic, each event retains
  its originating command sequence, and both exhaustion paths are checked before mutation.
- Failure operation labels are nonempty and bounded to 128 UTF-8 bytes. Every successful report,
  recovery begin, completion, or failure advances one recovery-state revision and emits complete
  lifecycle, active-failure, diagnostic-history, and pending-action state. Inspection and rejected
  recovery intent emit no event.
- The dispatcher owns the canonical error coordinator. Legacy failure and recovery commands route
  through it and cannot call lifecycle directly around classification-specific request validation,
  failed-recovery reclassification, or exact stale-token rejection.
- The lifecycle-attached dispatcher requires EngineControl for inspection, command execution, and
  event draining. Scenario-only mode is an explicit narrow compatibility boundary. Playback
  execution crosses only the bounded bridge and requires Playback on the executor side.
- An attached project document remains the sole mutable settings owner. Inspection returns one
  resolved immutable replacement state; optimistic mutation delegates validation and publication to
  project and cannot create an engine-private settings model.
- A semantic settings mutation reserves event capacity before publication and emits one complete
  replacement event. Invalid, stale, off-domain, missing-project, or saturated-queue mutation
  attempts publish nothing. A no-op requires no event capacity and advances neither project nor
  event sequence.
- Authored automation requires one full-dispatcher attachment and EngineControl ownership.
  Inspection emits no event. A semantic transaction reserves event capacity before authoritative
  mutation and emits one complete snapshot with only `audio_automation_revision` correlated;
  no-op, stale, invalid, missing-owner, off-domain, and saturated-queue failures publish nothing.
- Runtime resolution is deterministic and read-only. Audio sample timing and ordered channel
  meaning, color identity, cache bounds, proxy quality, render rate, extent, pixel, alpha, and color
  target remain explicit typed values; inspection does not reconfigure any subsystem.
- At most one playback command may be pending. Its event capacity and sequence are reserved before
  enqueue, and no later state-changing command may overtake it. Every accepted playback command
  yields complete replacement state, including bounded failure evidence when execution fails.
- Export is not a project mutation and is not represented in engine history.
- Logical export submit, poll, inspect, pause, resume, retry, cancel, and remove route through the
  dispatcher. Submit, resume, and retry require fresh export admission; other controls remain
  available during degradation. Each observed state change publishes complete deterministic queue
  state with one monotonic export revision.
- Export executor preparation binds runtime resources but does not admit a job. Generic closures
  and typed results remain outside stable command and event values. All-job cancellation plus
  polling must make logical state final before worker joining, which remains an explicit
  blocking-safe operation outside EngineControl.
- Scenario and project undo and redo restore complete semantic state without reimport or filesystem
  effects. Graph, timeline, project, media, and persistence owners retain their validation and
  identity boundaries.
- Default registry construction is vendor free; host and vendor behavior remains opt-in.
- Default registry construction includes all four in-tree source backends as primary priority-100
  registrations, with stable identifiers preflighted before source registry mutation.
- Media introspection is declaration-only and has deterministic ordering.
- Complete introspection requires EngineControl, reads the dispatcher-owned recovery snapshot, and
  cannot advance command, event, scenario, lifecycle, or resource state. Optional resource state
  retains its independent revision and exact class order.
- Workflow readiness is derived from canonical lifecycle admission. Active failures expose only
  reviewed subsystem, disposition, recovery intent, progress, and `UserSafeError` fields; raw
  diagnostics and internal recovery tokens never cross the introspection boundary.
- Timeline resource preparation requires the exact reachable media request set and at least one
  unique explicit decoder stream for each source-bearing media request.
- Project resource preparation must clone the exact selected compilation from an immutable
  `ProjectSnapshot`, whether the snapshot came from current state or a supported migrated project.
  It cannot recompile and erase ordinary published graph edits.
- Project fingerprints are bound when omitted and rejected when conflicting. Opened source media ID
  and fingerprint are verified again before decoder construction.
- Timeline compilation, sources, decoders, and selection evidence publish together only after every
  checked step succeeds. Cancellation or failure drops the unpublished owners.
- Source and decoder fallback is caller policy and retained evidence, never an exception retry.
- Upload preserves source representation and supports CPU-addressable buffers only.
- Upload preserves exact color-pipeline metadata, and viewport or export intent branches cannot mutate cached scene state.
- A viewport terminal stage must be `Display`; an export terminal stage must be `Output`.
- Derived generation rederives and matches exact encoder settings before reuse or codec creation.
  Source identity, revision, purpose, and quality remain cache-owned request inputs.
- Codec fallback is disallowed, input and output loops poll cooperative cancellation, and no packet
  is published until flush reaches end of stream and complete packet hashing succeeds.
- Packet identity covers bytes, stream, timing, keyframe state, and every known typed metadata value
  in stable key order. Unknown future media formats or metadata variants fail closed.
- Derived packet media is replaceable and cannot mutate project, source, graph, or final-render
  meaning. Rendering, scaling, muxing, persistence, and playback orchestration remain separate
  owners.
- Proxy admission requires exact source ID, source fingerprint, revision, proxy purpose, known
  quality, packet stream and timebase, nonempty bytes, timing, keyframe access, and valid source
  metadata. Ineligible artifacts become ordinary source fallback candidates, never errors that hide
  the authoritative source.
- The scheduler is the single owner of exact, lower-quality, stable tie, unavailable, and
  source-only choice. Engine conversion covers all four current cache quality codes and fails closed
  for future unknown values.
- A proxy-backed source retains the complete immutable artifact for its read lifetime and exposes
  the authoritative original identity. The original opener runs only after source selection, and
  any identity mismatch is rejected.
- Prefetch submission and polling require the playback domain and never wait on worker completion.
  Worker-pool lifecycle remains externally owned by a blocking-safe coordinator.
- A newer prediction generation cooperatively cancels the prior generation. Cancellation and
  evaluation failure are observed only as cache completion state and cannot alter transport.
- Exact predicted frames use the shared graph snapshot and complete host cache identity. Reuse skips
  evaluator work but cannot change graph meaning, foreground frame value, or final-render output.
- Foreground playback admits one exact frame at a time, never waits on worker, clock, audio, or
  viewport progress, and preserves separate exact scene and shared-clock coordinates. Cancellation
  releases retained state and checks cooperative work again after evaluation so stale output cannot
  present.
- Decoded timing, format, metadata, color history, and alpha meaning remain immutable provenance.
  Cache admission requires the exact requested timestamp and complete nonterminal scene pipeline.
- Resource configuration contains all six classes exactly once. Every class hard limit fits within
  the shared hard limit, every protected floor fits within its class limit, and protected floors do
  not overcommit the shared total.
- Shared and class hard limits count exact caller-declared managed payload bytes. Reservations are
  noncloneable and release both counters through RAII; opaque resource binding cannot inspect or
  change timing, precision, metadata, color, alpha, or subsystem ownership.
- A single request larger than its shared or class hard limit degrades without reclaiming valid
  existing work because no release could make that request admissible.
- Unused protected floors are borrowable. Global reclaim never asks a different class to cross its
  protected floor, fixed reclaim order is cache, AI, export, decode, GPU, then audio, and measured
  reservation release is the only progress signal.
- Reclaim callbacks execute outside the accounting lock while admission remains serialized. Direct
  callback recursion is a retryable conflict, callback failures remain classified evidence, and
  audio callback code must not perform arbitration or allocation.
- Finite pressure selects semantic scheduling fallback only. Playback audio alone may preserve its
  clock with timed silence; export pauses, and no fallback changes decoded or rendered meaning.
- The device-owned sample clock is the normal master. Live A/V coordination occurs only on the
  playback domain and returns rather than sleeping for early video.
- Frame PTS and nominal duration remain immutable through hold, correction, drop, and rebase
  outcomes. Transport supplies a distinct anchor-derived live deadline and rate-adjusted duration;
  corrected display intervals remain live presentation metadata and cannot alter render or export
  timing.
- A late drop requires both caller-owned semantic eligibility and an immediately ready successor.
  The single-ready-frame foreground owner supplies neither and therefore preserves its frame.
- Discontinuity recovery applies one exact clock reanchor and retains the triggering signed drift.
  Transport control reanchoring and A/V recovery share that concrete clock owner, and viewport
  saturation returns the exact frame plus its resolved presentation without another scheduler
  observation.
- Audio admission is all-or-nothing. Discontinuity generations block admission until the callback
  clears its consumer-owned queue. A/V coordination never changes audio samples or the
  callback-owned clock counter.
- Export admits no more than one video and one audio route, requires an export-priority operation and
  a current export permit at admission and publication, and never retries a different encoder after
  registry selection.
- Export rejects partial source reads, wrong decoded kinds, incomplete codec drains, timing or
  metadata drift, format or precision drift, incoherent graph, color, alpha, or audio provenance,
  and any nonempty packet stream whose normalized exact interval union differs from its inputs.
- Export artifacts publish only after all decoders and encoders reach end of stream and completed
  state resets successfully. Failure recovery uses a fresh operation context to reset each created
  encoder and selected decoder, but cannot publish a partial result.
- Export results are in-memory elementary packet streams with explicit selection and input evidence.
  Container muxing, file publication, arbitrary stream counts, and native GPU readback are separate
  owners.
- Export jobs have explicit worker, pending, and retained bounds. One logical `JobId` remains unique
  for its full retained lifetime, including all resume and retry attempts.
- Export job dependencies are immutable, refer only to older retained jobs, and cannot be removed
  while a retained dependent names them. Recoverable failure leaves dependencies waiting; terminal
  failure, cancellation, and dependency failure finalize downstream history.
- Export job polling, progress, result observation, pause, resume, retry, cancel, and removal require
  EngineControl and never wait for worker completion. Pause and cancel are cooperative and discard
  raced success before publication.
- Resume and permitted retry use fresh controls and progress. Retryable, degraded, and
  user-correctable failures permit retry, while terminal failures retain context and deny retry.
- Export queue shutdown can block and is legal only from a blocking-safe domain. It cancels all
  unfinished attempts before draining and joining workers when called directly. The dispatcher
  runtime handle is stricter: one-job or all-job cancel plus polling must make every logical job
  final before shutdown, so joining changes no dispatcher-visible job state.
- Transport controls require playback ownership and preserve integral frame identity. Cadence uses
  fixed checked anchors, loop ranges are nonempty half-open subranges, and only ordinary playing
  frames may be dropped. User targets, loop boundaries, and the forced-progress frame are protected.
- Stable transport commands contain exact media values only. The playback executor supplies the
  process-local clock instant, and audio samples, frames, textures, and packets never enter the
  command or event bridge.
- Dropping changes scheduling only. It cannot alter graph evaluation, cache identity, decoded
  provenance, color or alpha meaning, authored state, or later render and export inputs.
- GPU ownership and pool lifetime remain tied to the originating device.
- Timeline and clip-mix publication is all-or-nothing across both expected revisions and typed edit
  outcomes. The engine dispatcher can route this kind of future command, but the current public
  scenario transaction does not make independent project, graph, cache, persistence, and output
  owners one atomic commit.
- Lifecycle mutation and publication require EngineControl, while subsystem acquisition and
  teardown occur outside the controller through exact immutable actions. Sleep quiescence and wake
  revalidation follow the same rule.
- An attached logical export queue must report every job final before the dispatcher accepts the
  exact export teardown completion token.
- Startup is dependency-first and teardown is dependent-first. A retained failed resource blocks
  teardown of its dependencies but not independent branches.
- Sleep preparation is dependent-first and wake is dependency-first. Shared state never receives a
  power action, project resources remain owned while sleeping, and device plus workflow resources
  must be reacquired before work admission resumes.
- Repeated power notifications are idempotent. An opposite power notification or shutdown
  invalidates only the exact pending transition token, and a late completion cannot advance the new
  transition.
- Action completion must match subsystem, kind, lifetime, and action revision exactly. Every
  committed state publishes a new immutable generation, and prior snapshots never mutate.
- Playback, rendering, and export permits come from one snapshot and require their complete
  transitive subsystem set to be ready in the same running lifetime.
- Nonterminal failures degrade only dependent work. Terminal failures close all admission until
  orderly teardown and a fresh lifetime complete.
- Recovery intent is determined by explicit recoverability, never inferred from error category.
  Retryable, degraded, and user-correctable failures accept only their matching stable request;
  terminal failures require a new lifetime. Sequence, attempt, request, and lifecycle action must
  all describe the same active failure.
- Raw diagnostic summaries, contexts, sources, operation labels, sequences, and attempts remain
  internal evidence. Presentation consumes only `UserSafeError` and reviewed user-safe fields.
  Diagnostic history is bounded and oldest-first eviction never removes the separately retained
  active failure.
- Successful recovery clears the subsystem's active failure and restores admission without erasing
  the latest reported failure evidence from the current lifetime snapshot.
- Integration validation is a read-only projection of canonical owners. It never polls endpoint
  workers, waits for progress, creates a second lifecycle or recovery state, or changes any command,
  event, lifecycle, scenario, playback, or export revision.
- Endpoint attachment and retained replacement state must agree. Recovery action state must agree
  with the lifecycle action token, and every workflow permit or denial comes directly from the same
  immutable lifecycle snapshot included in the result.
- The remaining placeholder module does not imply whole-engine transport, node registration, or
  concrete native OFX ABI transport.

## Tests and verification

Canonical scenario contracts prove the exact fixture metadata, names, half-open ranges, complete
graph topology and ports, mirror matrix, four operation IDs and original revisions, two undo plus
two redo recovery at revision 8, rejected-action atomicity, payload digest failure, and the 64 MiB
bound. They also prove the shared four-entry snapshot store evicts the oldest action and clears redo
only after a successful post-undo branch.

Ten general dispatcher contracts prove atomic multi-action rollback and commit, one revision and one undo
unit, optimistic conflict rejection, matching complete replacement state events, sequence ordering,
full-queue rejection before mutation, off-domain rejection before mutation, coherent playback,
rendering, and export admission through healthy and degraded states, recovery, stale permit
rejection, retained failure context, sleep and wake command events, permit invalidation across power
transitions, restart, shutdown, both scenario and lifecycle inspection, exclusive project-history
attachment, typed project inspection and execution, stale project command atomicity, exact project,
command, event, and transaction correlation, one compound-command replacement event, no-op event
suppression, and project plus history
preservation when event backpressure rejects a command. The same dispatcher proof sends an unknown
extension record through the real project history, observes its typed result and replacement event,
suppresses an equal no-op, rejects a stale command, and preserves exact bytes through undo, redo,
and project reload. The project diagnostics proof uses the full lifecycle dispatcher, rejects a
missing attachment without sequence movement, returns one typed eventless report, preserves exact
history state, changes its semantic hash after a media mutation, and restores the prior hash through
undo under a newer outer document revision.

Two audio automation dispatcher contracts prove exclusive lifecycle-owned state attachment,
inspection command ordering, one event per semantic transaction, stale and invalid atomicity,
no-op success with no event, a complete 64-event capacity boundary before mutation, isolated audio
revision correlation, and real `ClipMixProcessor` consumption of the returned snapshot.

Three project-history contracts execute the real project media command owner through apply, undo,
and redo. They prove full aggregate reversal, typed outcomes, monotonic revision fences, failed and
idempotent branch preservation, redo clearing after a new authored change, capacity validation,
oldest-entry eviction, and revision-exhaustion atomicity. The durability proof writes the selected
snapshot to schema-4 SQLite, reloads with empty session history, and passes the canonical AV1 fixture
through `acquire_project_resources` after reversal.

The downstream `superi-api` generic editor contract consumes only the curated `editor` seam. It
constructs a real project, drives one six-action compound command through EngineControl, checks
semantic action evidence and one correlated project event, persists and reloads the selected
snapshot, then performs public undo and redo. Its parity table covers every currently executable
timeline, graph, media, clip-mix, and extension operation without widening the engine dependency
graph or making lower types wire owners.

One project-autosave consumer contract applies a real media-path mutation through that history,
autosaves the selected snapshot, then repeats after undo and redo. It opens all three recovery-point
databases through the public project owner and requires exact snapshot equality, including durable
settings, referenced-media state, and unknown extension records, while generation evidence advances
from one through three.
This proves consumer compatibility only; it does not add engine scheduling, worker, dispatcher,
wire, or recovery-selection behavior.

Four project recovery dispatcher contracts attach a real file-backed active database and the exact
project autosave namespace. They prove discovery, read-only semantic comparison, monotonic durable
restore including authored clip-mix state with empty new-session history, source-candidate retention,
exact later dismissal, complete replacement event correlation, and exact database reopen equality.
Read-only persistence failure, an external active-file replacement after coordinator attachment,
stale fences, terminal revision exhaustion, and a full 64-event queue all preserve disk, history,
candidate, sequences, and events before authority changes. The external replacement is reported as
a user-correctable conflict, preserves the collaborator database byte for byte, and publishes no
history state or dispatcher event.

Three project settings dispatcher contracts attach one real project document and prove complete
default inspection, exact typed resolution across timeline, color, audio, cache, proxy, and render,
one atomic optimistic mutation, and an ordered full replacement event. They also prove no-op success
with a saturated event queue, invalid and stale rollback, full-queue rejection before a real change,
missing-project failure, EngineControl enforcement, and unchanged authored editorial state. They
also prove settings changes enter the shared project history, undo and redo restore complete
settings without authored-audio drift, and scenario-only history attachment cannot invoke the
full-dispatcher settings surface.

Three project-transaction contracts execute timeline, graph, media, clip-mix, plugin, effect, AI
artifact metadata, and root actions through the real aggregate. They prove one outer revision and
history unit, retained direct graph parameters, exact authored-audio and extension save and reopen,
prepared adjacent-block audible continuity, exact undo and redo, late invalid-action rollback with
unchanged history, and action bounds.

Three editor-state contracts use the full dispatcher to prove missing-project classification, one
coherent eventless selected revision, unchanged history and event state, explicit recovery,
automation, playback, and export availability, pending and latest runtime observations, canonical
resource identity, three-channel source meaning, stereo destination meaning, one explicit muted
route, and a sample-exact 100-sample record gap between different clip sources.

Two engine-introspection contracts use the real registry, dispatcher, lifecycle, error coordinator,
and resource arbiter. They prove read-only ownership, exact optional accounting, canonical ordering,
healthy admission, playback-only, rendering-plus-export, and export-only degradation, active
recovery visibility, restored readiness, and safe failure projection without private messages,
contexts, or paths.

Three integration validation contracts drive the lifecycle-attached dispatcher through startup,
sleep, wake, playback degradation, recovery start, recovery completion, rendering degradation, and
a committed scenario transaction followed by undo and redo. They prove exact condition changes,
canonical introspection, lifecycle, and recovery agreement, independent playback, rendering, and
export permits or denials, complete scenario reversal capacity, endpoint attachment, retained export
queue replacement state, and an empty finding set for coherent state. The playback transport bridge
contract additionally proves that draining its event does not erase the latest replacement snapshot
or bounded failure retained for validation.

Two error dispatcher contracts run the canonical coordinator through the full engine command owner.
They prove retryable and degraded failure state, unrelated-work admission, independent recovery
revision and event correlation, exact request validation, inspection without events, successful
recovery, legacy result compatibility, failed-recovery reclassification with a new sequence,
stale-token rejection without publication, and fresh retry completion.

Codec contracts prove default AV1, Opus, and Vorbis declarations, optional host exposure, and
explicit vendor registration. Upload contracts prove semantic preservation and cloned allocation
ownership when an adapter is available, classified unsupported storage, and retryable shared budget
failure. Color metadata contracts prove exact ICC and named-space retention, transform order,
source continuity, cache mismatch rejection, independent display and delivery intent, and invalid
stage ordering. GPU tests may skip without an adapter; capability tests prove declarations, not codec
execution.

The media resource acquisition contract proves the default registry exposes all four in-tree source
backends without changing codec ranking. It compiles the canonical timeline, probes and opens the
real WebM fixture, creates the real Rust AV1 decoder, and retains exact source fingerprint, stream,
packet and decoded-frame timing, 8-bit YUV representation, partially specified color tags, opaque
alpha, and metadata. The same contract creates an exact schema-0 project around a directly edited
graph, migrates it through the sole project database owner, proves full snapshot equality, and then
acquires the retained graph revision plus real media stream without recompilation. It resolves a
portable stored project target into that real WebM path,
retains the project `MediaId` and expected fingerprint, rejects opaque targets, missing identities,
explicit missing state, and relative project-file context, and proves exact request-set validation,
fallback-tier policy evidence, selected-factory failure without retry, pre-cancelled atomic failure,
and later success through a fresh operation context.

The two generation contracts and three substitution contracts run through the default registry and
real Rust AV1 encoder. They prove complete packet timing and metadata, deterministic content and
artifact identity across
independent catalogs, exact reuse that skips different input, explicit quality identity, settings
mismatch rejection, and cancellation before publication. Substitution then proves real packet reads
through `MediaSource`, exact and lower-quality choice, order-independent cache-ID ties, replacement,
exact seek preroll, stale and mismatched rejection, missing and higher-only fallback, original
identity verification, and source-only final delivery. This is a real encoder and source-interface
consumer but not a render, resize, mux, persistence, playback clock, or container-delivery proof.

Four playback prediction contracts prove playback-domain enforcement, immediate nonblocking
submission and polling, exact nearest-first execution, cooperative supersession between frames,
structured degraded failure, recovery on later work, and boundary-empty cancellation without a
replacement job. The graph integration contract uses the real immutable snapshot and budgeted host
cache to prove three exact predicted frames execute once, reuse without node calls, and return the
same foreground evaluator value.

Six A/V coordination contracts prove the engine policy remains inside the product scheduling
target, coordination rejects UI and unowned callers before statistics mutate, normal and early
video return nominal presentation or a bounded hold, moderate lateness shortens only the live video
interval, protected and last-ready frames remain visible, and explicit successor plus eligibility
evidence permits dropping. They also prove that distinct live deadlines and intervals preserve
immutable media timing, that a ten-second discontinuity reanchors once and presents the unchanged
frame in the same call, and that monotonic fallback plus audio restoration preserves one continuous
exact timeline.

The foreground playback integration contract constructs a real high-precision decoded frame,
immutable graph snapshot, budgeted scene cache, binary16 working image, CPU display transform,
lock-free audio producer and consumer, audio master clock, bounded worker pool, and one-slot viewport
handoff. It proves exact semantics through binary32 display output, bounded clock waiting, nominal
presentation, thirty-millisecond lateness correction, unchanged PTS and duration, cache reuse,
audio saturation, lossless viewport retry without a duplicate scheduler observation, invalid scene
rejection without cache poisoning, a greater-than-ten-second discontinuity recovery, later frame
recovery, and continuity across audio-clock loss and restoration.

The render-export integration contract constructs a paired decoded video and audio session, binds
video through the real immutable graph snapshot and shared playback scene envelope, invokes explicit
delivery and audio stages, selects registered encoders, drains every lifecycle, and proves exact
timing, metadata, precision, color, alpha, graph, and audio provenance. It proves fallback policy,
selected-factory failure without retry, stale lifecycle denial, rendering degradation, partial-read
recovery, and later fresh success. Production lanes open and decode the canonical WebM and WAVE
fixtures: WAVE PCM completes through the in-tree decoder and encoder with all 4,410 samples, while
the WebM AV1 path evaluates all 96 frames and rejects libvpx VP9 duration rounding before artifact
publication. A queue-backed paired route exercises the same real transaction as one logical export
job through dispatcher submit and automated poll commands. It proves exactly 11 semantic progress
units, ordered replacement events, and typed runtime artifact access only after completion.

Seven export-job contracts prove EngineControl ownership, configuration bounds, retained logical
identity, dependency admission and propagation, temporary worker saturation, nonblocking progress,
typed result retention, pause acknowledgement, fresh resume, cancel winning over raced success, all
four recoverability classes, retry unblocking a dependent, terminal retry denial, safe removal, and
blocking-safe shutdown. The pause proof separately acknowledges executor entry before requesting
cooperative interruption, preserving the runtime assertion without a scheduler race.

Three export dispatcher contracts run against that real queue. They prove prepared submit, complete
state and revision events, automated progress and completion observation, typed result retention,
inspection without publication, pause and fresh resume, dependency release, cancellation,
degradation denial, observation while degraded, recovery retry with a distinct lifecycle permit,
finalized removal, all-job cancellation, and final-state-only blocking-safe worker shutdown.

Five transport contracts use the real bounded worker pool, prefetcher, output producer and callback,
shared clock, and viewport handoff. They prove superseding seek and scrub generations, stale-result
exclusion, exact step and resume behavior, 0.5x and 1.5x frame deadlines, reverse loop wrapping,
unsupported-rate audio, bounded late skipping with forced progress, protected seeks, replaceable
prefetch failure, foreground failure, viewport saturation, and recovery without frame-identity
drift. The fifth contract moves one dispatcher bridge between its legal EngineControl and Playback
owners and proves admitted exact seek, ordered completion, overtaking rejection, degraded denial,
recovery, epoch correlation, and a failed command with complete replacement state.

The audio editorial contract drives a real `superi-timeline` razor operation through the combined
engine transaction. It proves exact source and record subdivision, retained and new identities,
group, link, and sync-lock inheritance, complete clip mix inheritance, and unchanged caller state
when the mix revision conflicts.

Nine lifecycle contracts drive the public EngineControl owner through exact action tokens. They
prove shared-state, project, device, playback, rendering, and export initialization order; healthy
common admission; reverse sleep quiescence; retained project meaning; volatile device and workflow
release; forward and critical wake revalidation; repeated notification idempotence; opposite
notification supersession; recoverable wake degradation and recovery; terminal wake failure;
shutdown during sleep or wake; playback-only and rendering-plus-export degradation; immutable prior
snapshots and stale permits; cross-thread immutable inspection; reverse teardown; direct and stopped
restart with fresh lifetimes; initialization rollback; stale completion rejection; dependency-safe
teardown failure and retry; and unchanged owner state after off-domain construction, inspection, and
mutation rejection.

Four error-recovery contracts drive the coordinator beside that lifecycle. They prove all four
recoverability dispositions and stable request codes, source-chain and ordered-context retention,
separate safe projection, playback-only and rendering-plus-export denial, unrelated-work
continuation, exact retry and correction completion, failed-recovery reclassification, stale
sequence rejection without lifecycle mutation, terminal all-work denial, bounded oldest-event
eviction, cross-thread immutable record safety, and off-domain coordinator rejection.

Ten resource-arbitration contracts bind a real `Rgba16Float` decoded frame with straight alpha,
exact timing, metadata, and color state to a reservation without semantic drift. They prove
complete and nonovercommitted configuration, exact grant and release revisions, class and shared
hard limits under sixteen concurrent callers, protected-floor borrowing and fixed reclaim order,
authoritative callback progress, retained classified failure, all six class fallbacks across
playback, render, export, background, and recovery consumers, class-ceiling retry, and rejection of
recursive callback admission. An individually impossible request proves immediate degradation
without evicting valid existing work.

The plugin supervision contract discovers nested bundles in stable canonical order, ignores OpenFX
special directories, and retains malformed or terminal launcher failures without blocking a healthy
sibling. Real child processes prove every adapter action crosses a process boundary; the contract
also proves permission-free scan, exact activation grants, retryable and user-correctable failures,
deadline quarantine, explicit clearing and restart, and equal playback, rendering, and export
resolution revisions in degraded and recovered states.

Three native audio plugin supervision contracts discover valid and malformed VST3 bundles in stable
order, reject wrong-domain discovery and in-process or oversized worker contracts, restore a saved
checkpoint from a compatible installed version, and publish exact native plus transport latency.
They capture current state, preserve two instances of one component as distinct project extension
records through real database save and reopen, retain the first activation fault, restart from the
checkpoint, quarantine a repeated activation failure, and deny recovery while quarantined.

## Current status and risks

Canonical command state is substantive and test-backed, but it is a reference boundary whose
implementation identity is disclosed as such. It validates fixture bytes without opening their
container or decoding frames. Timeline and graph state are exact control models but do not use the
production timeline owner or the generic `superi-graph` DAG store. Its bounded storage is now shared
with production project history, but its scenario model remains separate reference state.

The typed dispatcher is the substantive engine-wide control seam for current scenario, lifecycle,
classified failure and recovery, sleep, wake, shutdown, restart, work admission, capability and
health observation, interactive transport, logical export, typed project media, extension, and
compound mutation, project undo and redo, authoritative project settings inspection and mutation,
authored audio automation inspection and mutation, deterministic semantic project diagnostics
inspection, coherent complete editor-state inspection, and read-only integration validation
operations.
It also owns crash recovery discovery, complete
semantic comparison, durable restoration, and exact dismissal through one optional file-backed
coordinator. The optional project attachment owns one real `ProjectDocument`, returns complete
typed history state, records settings, media, extension, and compound
mutations in that shared history, and publishes correlated full replacement events for authored
changes. Its introspection and validation snapshots expose coherent adaptation and action evidence
without adding mutation authority. Its project diagnostics command exposes project-owned canonical
hash and component evidence without an event, history entry, or second semantic model.
Its editor-state command captures the same selected project plus cached optional runtime
observations in one eventless replacement aggregate without polling or bulk runtime payloads.
Playback crosses
one bounded nonblocking bridge to its real domain owner. Logical export state remains in the
canonical queue behind a type-erased dispatcher controller, while prepared executors receive fresh
revision-scoped permits and render-export revalidates one before publication. The public JSON
scenario projection remains intentionally narrow, while the separate generic project facade now
projects complete current authored-operation control and minimum typed history state through the
curated engine seam. The strict validation projection remains read-only coherent state.
The public asynchronous job facade now projects queue control and full replacement events without
moving scheduler, poll, executor, or typed-result ownership out of engine.

Typed project settings resolution and dispatcher control are substantive and test-backed. They
preserve durable authored scalar meaning and expose one real API path, but resolution does not
automatically reconfigure live timeline, color, audio, cache, proxy, or render owners. Project
database open and save commands and autosave scheduling remain outside engine and API. Recovery
commands are implemented and test-backed through the existing dispatcher. The lower project
database rejects an externally changed active generation before recovery publication; engine does
not duplicate that writer lock or file authority. No background autosave worker, open-project
shell, or CLI adapter is claimed.

Project extension integration is substantive and test-backed. History, compound transactions,
dispatch, ordered replacement events, persistence, save, autosave, undo, and redo preserve plugin,
auxiliary effect, AI artifact provenance, and unknown future records through the project-owned
command surface. The native audio plugin supervisor deliberately interprets only its own
`superi.audio-plugin-state` extension kind, validates the exact audio-owned envelope, and keeps live
readiness separate from the durable checkpoint. Unknown and other owned extension kinds remain
opaque. `PluginSupervisor` and `AudioPluginSupervisor` remain separate OpenFX and native audio
runtime availability owners.

One orchestration file remains a documentation-only placeholder. Source registration, timeline
media preparation, deterministic lifecycle, classified failure propagation and recovery,
shared finite-resource arbitration, foreground playback, interactive transport, render-export
execution, and bounded logical export scheduling are coherent and test-backed, but there is no
timeline-owned decoded-audio graph
  renderer, persistent cache lifecycle owner, native GPU viewport or export submission, packet muxer,
  output publisher, concrete platform OpenFX or audio plugin worker launchers, native OFX or audio
plugin IPC adapters, Audio Unit registry enumeration, heartbeat and kill integration, or GPU-handle
transport. Project
persistence exists in `superi-project`, while history stacks deliberately remain session-local. The
integration validator is substantive but observes only
canonical cached state and cannot replace a production soak, platform lane, or wire client. The effects-side OpenFX host contract
and engine-side bundle discovery, launch coordination, active registry rebuilding, containment,
recovery, and quarantine are substantive.
Native audio bundle discovery, worker-contract validation, checkpoint recovery, quarantine,
per-instance project persistence, and timing-matched processor preparation are substantive. Their
launcher and bridge traits are enforced integration boundaries, not concrete operating-system IPC or
sandbox implementations. Dynamic plug-in latency changes still require a control-side graph rebuild.
Playback prediction, foreground orchestration, and transport control are substantive, but they
accept caller-prepared graph, audio, cache, and viewport owners and are not a source session binder,
decoded-audio rate processor, or native presentation path. Engine A/V coordination consumes the
shared scheduler and actual audio clock, but physical A/V latency and drift remain hardware-lane
evidence. `TimelineResources` prepares the reachable sources, decoders, and graph, and its acquired
media owner now has a real export consumer. Its project entry point preserves one exact published
document compilation, and the request adapter now resolves project-owned filesystem targets through
the real local source path. Playback and export do not yet acquire the whole bundle directly from
the document owner.
Export still requires caller-supplied graph, delivery, and audio stage owners and returns elementary
streams without muxing or persistence. The export queue retains results in process memory, uses
cooperative checkpoints, and requires an explicit blocking-safe shutdown; it is not a persistent or
crash-recoverable scheduler. Its generic executor preparation and typed results remain process-local
runtime bindings. The stable API can inspect and control retained queue state, but no public or CLI
surface submits prepared work, polls the host runtime, waits, retrieves typed artifacts, or publishes
files. The derived-media driver
and resolver are synchronous and caller-owned, and no application or API path invokes them yet.
Clip-mix reconciliation, extension mutation, and compound project mutation are substantive and now
entered by the generic public API as well as Rust callers, but not by the playback controller.
Lifecycle is a production control-plane contract that now names project
and device boundaries across sleep and wake, but later platform callbacks, additional typed project
mutation adapters, native device owners, render submission, export
publication, and arbitration consumers still must perform concrete actions before acknowledging it.
The shared arbiter is a substantive opt-in
engine contract, but current playback, render-export, cache, GPU, audio, and AI owners do not yet
install their reclaimers or bind every live resource automatically.

## Maintenance notes

Keep fixed canonical state synchronized with `docs/vertical-slice.md`, the strict API projection,
CLI runner, dispatcher transaction and event contracts, and operation contracts. Keep transaction
identities bounded and public method and event names permanent, reserve sequence and event capacity
before mutation, route lifecycle state only through EngineControl, and preserve one-revision and
one-undo-unit transaction atomicity. Route every supported failure and recovery command through the
dispatcher-owned error coordinator, keep lifecycle as the only admission authority, publish the
complete independently revisioned recovery state, retain the exact pending recovery token, and do
not let legacy lifecycle actions bypass coordinator validation. Keep playback commands media-only
and transport neutral,
preserve the capacity-one nonblocking domain bridge, emit complete replacement state after every
accepted execution, and never allow another state mutation to overtake its reserved event. A new
production owner should replace the corresponding stub
through its real crate rather than growing this reference model into a competing system. Registry
or upload changes require updating their actual consumers and tests independently. Keep source
registration synchronized with all four media-I/O adapters. Keep legacy resource preparation bound
to the timeline compiler and project resource preparation bound to the exact retained snapshot
compilation, including snapshots returned after project schema migration. Both paths must preserve
the exact reachable media set, persistent source identity,
explicit decoder streams, one-shot selection, operation checks, and all-or-nothing publication.
Keep project request construction dependent on the project path codec, reject explicit missing
state before media I/O, and never infer `MediaId` or fingerprint identity from a filesystem path.
Keep all authored project actions behind `ProjectCommandHistory`, retain immutable before and after
snapshots only after successful semantic changes, preflight dispatcher event capacity before
mutation, preserve both branches on failure and no-op, and restore only through the project-owned
revision-fenced monotonic seam. Keep compound actions bounded, ordered, validated after each step,
and published by one outer project edit. Keep extension payloads opaque, preserve project-owned
capability, lifecycle, and failure meaning exactly, and never persist derived supervisor readiness
into authored extension state. Add later command adapters without placing history stacks
in project, timeline, graph, audio, API, or CLI owners.
Keep `editor` a behavior-free curated reexport seam for the public adapter. New authored operations
must stay implemented and validated by their lower owners, then be added to this seam only when the
API has a strict DTO, complete conversion, parity proof, and real dispatcher consumer.
Keep `InspectProjectDiagnostics` bound to the selected immutable history snapshot and delegate all
hash framing, component evidence, versioning, and exclusions to `superi-project`. Preserve its typed
complete result, successful command sequencing, missing-owner failure, eventless behavior, and zero
history or project mutation. Do not copy the project hash algorithm or build an engine-private
diagnostics digest.
Keep `InspectEditorState` bound to that same selected history snapshot and the dispatcher's already
retained optional observations. Preserve canonical authored resource bytes, exact audio clocks and
routing, explicit freshness, eventless behavior, and zero hidden polling or filesystem work. Add a
domain only through its authoritative owner and keep opaque payloads and typed runtime artifacts out
of the aggregate.
Keep the selected immutable snapshot directly consumable by the project-owned autosave command. A
future background I/O integration may route that public surface, but must not duplicate policy,
publication, naming, or retention in engine and must not block EngineControl, Playback, Render, or
Audio. Keep project recovery behind the one dispatcher-attached coordinator, preserve exact database
and history agreement at attachment, reserve sequences and events before filesystem mutation, and
publish the prepared active database before committing restored history. Recovery paths must retain
opaque identity, complete durable aggregate comparison, candidate-after-restore, and separate
explicit dismissal behavior. Treat a project-owned stale active generation as an atomic recovery
conflict and preserve collaborator bytes, selected history, the candidate, sequences, and events.
Keep settings keys, defaults, candidate validation, and persistence project-owned. Engine may only
resolve validated snapshots into existing subsystem types and dispatch transactions through the
same `ProjectCommandHistory` document owner. Preserve dynamic no-op event reservation, complete
replacement events, exact project revision correlation, and the API-to-engine-to-project dependency
direction.
Keep authored automation state audio-owned and lifecycle-attached only once. Preserve exact
candidate preflight, dynamic no-op event reservation, full replacement results and events, isolated
automation revision correlation, and the API-to-engine-to-audio dependency direction. Never route
control transactions through the callback domain or place prepared curves in dispatcher events.
Keep derived
request canonicalization synchronized with every media format field that can change encoder output,
and keep codec selection, cancellation, complete publication, proxy admission, scheduler-owned
quality choice, lazy source opening, and full identity verification explicit. Keep playback prefetch
domain-owned, nonblocking, cooperatively cancellable between exact frames, and bound to complete
cache identity. Keep transport frame and clock anchors separate, protect explicit and loop-boundary
frames, make dropping scheduling-only, and keep audio discard callback-owned. Keep lifecycle actions
nonblocking on EngineControl, add new canonical subsystems
only in dependency order, preserve exact token checks, update every work requirement that consumes
them, and never release a dependency while an owned dependent remains. Preserve reverse sleep and
forward wake ordering, retained project state, volatile device release, critical-wake revalidation,
idempotent repeated notification behavior, and exact-token invalidation when power transitions or
shutdown supersede one another. Keep foreground playback
single-flight and nonblocking, retain exact sink payloads under backpressure, validate scene time,
color, and alpha before retention, preserve immutable media timing through live corrections, cache
one resolved presentation across viewport backpressure, require explicit readiness before dropping,
apply discontinuity rebases only through the concrete clock, and preserve timeline position when
changing clock sources.
Keep export selection one-shot, route source and output identities separately, drain every decoder
and encoder through end of stream, retain exact input and packet evidence, validate interval unions
with rational time, and reset failure state through a fresh operation context without publishing
partial output. Add muxing or persistence only through an explicit owner after elementary streams
have passed the current completion checks. Keep logical export identity stable across attempts,
dependency edges immutable and older-only, recoverable failures unresolved until retry, terminal
history final, all controls nonblocking on EngineControl, and worker joining confined to explicit
blocking-safe shutdown after every logical job is final. Route every logical export action through the dispatcher, require fresh
admission for submit and recovery attempts, emit full revisioned replacement state for explicit and
automated transitions, and keep runtime executor bindings plus typed artifacts outside stable event
payloads. Keep the public API as a strict projection of those dispatcher commands and envelopes,
never a second queue, and keep host polling, submission, waiting, and typed result access out of the
transport catalog until their owning product workflows exist.
Keep resource classes complete and stable, preserve exact shared and class accounting, keep unused
floors borrowable without reclaiming protected capacity, invoke cooperative owners only in the
documented order, and measure progress from reservation release rather than callback claims. Bind
real values opaquely, keep audio arbitration outside its callback, preserve consumer-aware fallback,
and do not turn this shared envelope into a competing cache, GPU pool, decoder, audio queue, AI
runtime, export scheduler, or lifecycle authority.
Remove a placeholder label only after substantive behavior and consumer proof exist. Keep
`plugins.rs` bound to `superi_effects::ofx::IsolatedOfxAdapter`, the graph-owned registry and
missing-node resolver, canonical bundle paths, permission-free scanning, exact permission narrowing,
and classified retained failures. Concrete platform launchers must satisfy the declared process,
message, deadline, restart, and sandbox contract without moving native code into the engine process
or creating an engine-private editable plugin model.
Keep `audio_plugins.rs` bound to the audio-owned identity, state, graph-latency, and isolated bridge
contracts. Preserve deterministic candidate order, BackgroundJob control ownership, separate-process
validation, per-instance durable keys, compatible-component restore, exact captured-state rebinding,
retained checkpoints, one-fault restart, repeated-fault quarantine, and timing-matched dry fallback.
Concrete launchers may add IPC, shared memory, heartbeats, kill, sandbox, and Audio Unit enumeration,
but may not load vendor code in engine, persist operational readiness, merge two node instances, or
bypass the existing state and descriptor bounds.
