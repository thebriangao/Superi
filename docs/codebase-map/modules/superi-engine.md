---
module_id: superi-engine
source_paths:
  - open/crates/superi-engine
source_hash: 892f86a014f8dceae20c675102dcbf358c93710c98fa2ecbea565a3e4a1ec304
source_files: 47
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-engine` is the open orchestration layer. Its substantive paths cover canonical editorial
command state, complete media backend registry assembly, transactional timeline graph plus source
and decoder preparation, capability introspection, CPU-decoded frame upload, exact viewport and
export color metadata branching, derived-media generation, transparent proxy resolution,
predictive cache population, foreground graph and display-color execution, bounded audio admission,
audio-master A/V coordination with bounded video correction and discontinuity recovery, monotonic
clock fallback, lossless viewport handoff, exact interactive transport control, coherent decode,
graph, delivery-color, audio, and elementary-stream export execution, deterministic subsystem
lifecycle, bounded logical export job orchestration, classified cross-subsystem failure propagation
and recovery, and atomic timeline plus clip-mix edits. An
engine-wide typed command dispatcher
coordinates canonical scenario transactions, lifecycle state changes, failure and recovery control,
coherent work admission, bounded cross-domain playback control, canonical logical export-job
control, and ordered replacement events.
Native GPU presentation, timeline-owned decoded-audio rendering, export muxing and publication,
general project transactions, plugins, nodes, and validation remain incomplete.

The command path is a bounded reference owner for contract conformance. It does not claim to replace
the production project, timeline, graph, media, color, render, or muxing owners.

## Source inventory

- `open/crates/superi-engine/Cargo.toml`: Declares subsystem dependencies, optional codec features,
  production `sha2`, and test-only `pollster`.
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
  scenario, lifecycle, classified recovery, playback, and logical export events with
  originating-command correlation, canonical error-coordinator ownership, exact recovery tokens,
  lifecycle action routing, shutdown, restart, coherent work admission, plus a one-command
  nonblocking bridge from EngineControl to the playback owner.
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
- `open/crates/superi-engine/src/introspection.rs`: Implements deterministic API-neutral backend and
  codec capability snapshots.
- `open/crates/superi-engine/src/lib.rs`: Documents the implemented orchestration boundaries and
  exposes twenty-one engine modules.
- `open/crates/superi-engine/src/lifecycle.rs`: Implements the EngineControl-owned lifecycle state
  machine, canonical subsystem dependency plan, exact action tokens, immutable generated snapshots,
  coherent playback/render/export admission, recoverable degradation, rollback, reverse teardown,
  teardown retry, and fresh-lifetime restart.
- `open/crates/superi-engine/src/media.rs`: Builds default and feature-gated media registries,
  including atomically preflighted primary registrations for all four in-tree container sources.
- `open/crates/superi-engine/src/nodes.rs`: Placeholder for media and graph nodes.
- `open/crates/superi-engine/src/playback.rs`: Defines playback-domain nonblocking prediction and
  foreground submission, complete decoded and scene semantic envelopes, shared-graph retained
  evaluation, validated cache admission, concrete CPU display conversion behind a generic output
  seam, cancellable distinct-deadline foreground work, bounded audio output and discontinuity
  status, audio-master and monotonic clock continuity, lossless viewport handoff, structured
  degradation, and weak worker-pool lifecycle boundaries.
- `open/crates/superi-engine/src/plugins.rs`: Placeholder for plugins and extensions.
- `open/crates/superi-engine/src/proxy_substitution.rs`: Validates exact proxy purpose, source
  fingerprint, source revision, packet integrity, and stream metadata; translates cache quality to
  scheduler quality; consumes deterministic derived selection; lazily opens verified original
  media; and adapts complete generated packets to the codec-neutral `MediaSource` contract.
- `open/crates/superi-engine/src/render.rs`: Defines independent viewport and export color metadata branches from cached scene state, requiring correctly classified terminal display or output stages.
- `open/crates/superi-engine/src/resources.rs`: Compiles one reachable timeline graph, validates the
  exact caller-declared media and stream request set, binds and verifies project fingerprints,
  probes and opens each source, selects and creates each decoder once, retains policy evidence, and
  publishes one all-or-nothing owner bundle.
- `open/crates/superi-engine/src/transport.rs`: Implements playback-domain pause, play, seek,
  superseding scrub, exact frame step, reduced signed rate, direction, half-open loop, rational
  clock cadence, prediction replanning, protected discontinuities, bounded ordinary late-frame
  dropping, explicit audio degradation, immutable transport observations, and the stable typed
  command vocabulary consumed by the engine bridge.
- `open/crates/superi-engine/src/validation.rs`: Placeholder for real-condition validation.
- `open/crates/superi-engine/tests/av1_capability_contract.rs`: Default AV1 selection proof.
- `open/crates/superi-engine/tests/audio_editorial_mix_contract.rs`: Proves atomic timeline and
  clip-mix publication plus complete audio intent, source, record, identity, grouping, linking, and
  synchronization preservation through a real razor edit.
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
  execution-domain enforcement, and blocking-safe shutdown.
- `open/crates/superi-engine/tests/export_dispatcher_contract.rs`: Proves dispatcher-owned prepared
  submit, ordered complete replacement state, automated progress and completion polling, typed
  result retention, inspection, pause, fresh resume, dependency release, one-job and all-job cancel,
  degradation denial, recovery retry with a fresh permit, removal, and final-state-only blocking-safe
  shutdown.
- `open/crates/superi-engine/tests/dispatcher_contract.rs`: Proves atomic multi-action commit and
  rollback, revision fences, one-unit undo, bounded ordered replacement events, EngineControl
  enforcement, coherent normal and degraded work admission, recovery, stale permits, retained
  failure context, restart, shutdown, and inspection.
- `open/crates/superi-engine/tests/frame_upload_contract.rs`: Upload, ownership, storage, and budget
  proof.
- `open/crates/superi-engine/tests/lifecycle_contract.rs`: Proves deterministic startup and reverse
  teardown, generated shared snapshots, coherent workflow admission, isolated degradation and
  recovery, initialization rollback, stale action rejection, terminal closure, dependency-safe
  teardown retry, direct and stopped restart, and EngineControl ownership.
- `open/crates/superi-engine/tests/media_resource_acquisition_contract.rs`: Proves complete source
  registration, real WebM and AV1 preparation, compiled graph retention, exact timing, precision,
  metadata, color and alpha semantics, strict request validation, explicit fallback evidence,
  cancellation, no exception retry, and fresh-context recovery.
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
- `open/crates/superi-engine/tests/render_export_orchestration_contract.rs`: Proves coherent paired
  video and audio export, deterministic fallback evidence, no exception retry, lifecycle degradation,
  partial-source recovery, encoder and terminal-delivery color agreement, exact WAVE PCM completion,
  rejection of real VP9 timing drift after complete WebM AV1 decode and shared graph evaluation, and
  completion through dispatcher-owned logical export commands over the real queue with exact
  semantic progress and retained typed artifact access.
- `open/crates/superi-engine/tests/playback_transport_contract.rs`: Proves exact seek and scrub
  supersession, pause and resume, frame stepping, fractional cadence, reverse looping, stale-work
  exclusion, bounded drops, protected intent, audio discontinuity, foreground and prediction
  degradation, viewport backpressure, recovery through the real playback owners, and bounded
  EngineControl-to-Playback command and event dispatch.
- `open/crates/superi-engine/tests/scenario_contract.rs`: Exact canonical state, atomicity, bounds,
  operation log, and reversal proof.
- `open/crates/superi-engine/tests/vendor_codec_registry_contract.rs`: Explicit vendor registry
  proof.
- `open/crates/superi-engine/tests/vorbis_capability_contract.rs`: Default Vorbis selection proof.

## Public surface

`command` exposes the fixed canonical values, `ScenarioEngine`, `ScenarioAction`,
`ScenarioSnapshot`, phases, fixture state, timeline state, graph nodes and edges, mirror parameters,
implementation identity, typed operation arguments, and operation records. Supported mutations are
import, insert, trim, and horizontal mirror. Undo and redo are history actions. Export is
intentionally absent from engine mutations.

`dispatcher` exposes `EngineTransactionId`, `ScenarioTransaction`, `EngineCommandRequest`, the
non-exhaustive `EngineCommand` and `EngineCommandResult` vocabularies, classified
`EngineReportedFailure`, `EngineEventEnvelope`, `PlaybackCommandExecution`,
`PlaybackCommandExecutor`, and `EngineCommandDispatcher`. The coherent constructor binds scenario
and lifecycle control behind EngineControl, `new_with_playback_bridge` adds a bounded nonblocking
executor for the real playback owner, `attach_export_jobs` installs the canonical logical export
queue behind a type-erased controller, and `scenario_only` is the explicit compatibility owner used
by the narrow public reference API. Successful command and event sequences are independently
monotonic, each event identifies its originating command sequence, and event draining returns
complete ordered replacement state with scenario, lifecycle, playback, or export correlation.

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
stage without transforming pixels. `introspection` exposes engine-owned media backend, operation, codec,
constraint, and hardware records through `MediaCapabilities::from_registry`. `media` exposes the
default registry with four primary source adapters and the feature-gated explicitly configured
vendor constructor.

`resources` exposes explicit `MediaResourceRequest`, `DecoderResourceRequest`, and
`ResourceAcquisitionPolicy` inputs; stable source and decoder selection evidence; stateful acquired
source and decoder owners; and `TimelineResources`. `acquire_timeline_resources` compiles one root
and its reachable nested timelines, requires exactly one request for each reachable linked media
identity, and publishes the compilation and live media owners only after every source and decoder
has succeeded.

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

`lifecycle` exposes `EngineLifecycle` as the single EngineControl owner. Canonical
`EngineSubsystem` values describe shared state, playback, rendering, and export. Generated
`EngineLifecycleSnapshot` values expose lifecycle phase and revision, engine state revision,
lifetime, health, ordered subsystem state, retained classified failure evidence, and one exact
pending action. Subsystem owners complete or fail the full `EngineLifecycleAction` token after work
on their legal domain. `EngineWorkKind` admission returns revision-scoped permits only when every
required subsystem is ready in the same running lifetime. The retained `LifecycleSignal` is the
lock-free observation path for latency-sensitive consumers.

`error` exposes `EngineErrorCoordinator`, monotonic `EngineFailureSequence`, stable
`EngineFailureDisposition` and `EngineRecoveryRequest` codes, immutable `EngineFailureRecord` and
failure reports, exact `EngineRecoveryAction` tokens, and recovery start and completion reports.
Every returned report carries the canonical lifecycle snapshot that determined workflow admission.
Mutable active-failure and bounded-history bookkeeping remains EngineControl-owned and deliberately
non-`Sync`; immutable records and snapshots remain `Send + Sync`.

The three remaining placeholder modules contain documentation only.

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

### Engine command dispatch and event publication

One `EngineCommandRequest` combines a validated caller transaction identity with one typed command.
Scenario execution requires an exact expected revision and one to 64 ordered actions. Lifecycle
inspection and mutation, operation-labeled classified failure, exact recovery start, completion,
failure and inspection, shutdown, restart, work admission, permit validation, stable interactive
transport control, and every logical export queue action all route through the same dispatcher
rather than creating parallel control paths. The lifecycle-attached owner checks EngineControl
before observing or mutating authoritative state.

Before a state-changing command executes, the dispatcher reserves bounded event capacity and
checks command and event sequence exhaustion. Success advances the command sequence once and emits
one full replacement scenario, lifecycle, or recovery event when state changed. Recovery state has
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

### Registry, introspection, and upload

Default registry construction atomically registers permissive Rust codecs plus primary priority-100
Matroska or WebM, MP4 or MOV, MXF, and WAV or AIFF sources. Source implementations and stable IDs
are constructed and preflighted before source mutation. `os-codecs` may add host-discovered codec
operations, and `vendor-codecs` requires explicit worker configuration.
Introspection reads declarations only, orders stable records, separates primary and fallback tiers,
and never constructs a source or codec.

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
the shared-state owner to initialize. Exact action completion advances playback, rendering, and
export one at a time in dependency order. After all four resources are owned, the engine participant
acknowledges the shared `LifecycleCoordinator` startup revision and the common phase becomes
`Running`.

Every committed state change publishes a new generation-tagged immutable snapshot through
`SnapshotPublisher`. Playback requires shared state and playback; rendering requires shared state
and rendering; export requires shared state, rendering, and export. One nonterminal subsystem
failure therefore denies only its transitive work while unrelated work remains admitted. Recovery
uses another exact action token. Successful recovery clears the subsystem blocker while the latest
reported failure remains inspectable for the rest of that lifetime. A terminal failure advances the
shared lifecycle phase to `Failed` and denies every workflow.

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

Shutdown emits teardown actions only after every owned dependent has released its resource, which
produces the exact inverse of initialization. Initialization failure enters the same reverse path
for already owned dependencies. A failed teardown retains ownership and blocks its dependencies,
while an independent branch may continue stopping. Retry must release the retained resource before
the engine can acknowledge `Stopped`. Restart first completes this teardown, increments the engine
lifetime, clears resolved failure state, and begins the canonical startup again. Action tokens and
work permits carry lifetime and revision identity, so late completion and stale work are rejected.
EngineControl never executes subsystem acquisition or teardown inline and therefore remains
nonblocking. When the logical export queue is attached, the dispatcher rejects completion of its
export teardown action until all retained jobs are final. Stable all-job cancellation plus polling
settles that state before the blocking-safe runtime handle joins workers.

### Timeline graph and media preparation

The caller supplies one validated `EditorialProject`, root timeline, immutable backend registry,
explicit request for each reachable linked media identity, fallback policy, and operation context.
The engine first consumes `superi-timeline::compile_timeline`, then recursively identifies every
media clip in the same reachable nested-timeline closure. Missing, duplicate, or extra media
requests and empty or duplicate decoder sets fail before any bundle is published.

For each media identity, the engine binds the project's persistent fingerprint when the request
omits it and rejects a conflicting caller identity. `BackendRegistry::probe_source` performs the
bounded content selection, the chosen adapter opens once, and the engine verifies the returned
media ID and fingerprint again. Each requested `StreamId` resolves to its complete opened
`StreamInfo`; optional audio representation is applied through the ordinary `DecoderConfig`; and
registry ranking selects exactly one decoder factory. Source and decoder selections retain stable
backend IDs, fallback candidates, fallback-use state, container confidence, and probe bounds.

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

### Editorial audio intent

The engine applies a timeline edit batch to a cloned project, reads its typed fragment, inserted,
and removed identities, and translates only clip identities that already own explicit audio
intent. Right fragments inherit complete controls, replacements transfer them, and whole removals
delete them. The same mutation batch is revision checked by `superi-audio`; only then does the
engine replace both caller-owned states. Stable-identity trims, moves, slips, and rolls require no
audio mutation, so their user intent remains attached without synthesis.

## Dependencies and consumers

- `superi-core` supplies errors, identifiers, geometry, and exact time used directly by canonical
  commands, introspection, upload, playback prediction, foreground pixel and alpha meaning, and
  retained lifecycle failure evidence. Error propagation additionally consumes core-owned
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
  bounded producer, callback-owned discard acknowledgement, and actual presentation clock. Export
  reuses the graph snapshot and playback scene envelope, invokes explicit delivery and audio
  processing seams, and validates all returned semantics before encoding. Timeline and audio are
  also jointly consumed by the clip-mix edit transaction. Effects
  supplies the safe
  `IsolatedOfxAdapter` contract, typed requests, graph projection, and plugin lifecycle state that a
  future engine worker supervisor can consume, but engine implements no adapter, native discovery,
  transport, or production command integration. AI and project remain declared dependencies without
  production command integration.
- `superi-api` consumes dispatcher transactions and events plus command and capability snapshots,
  preserving the public scenario seam without exposing engine-private owners. The broader Rust
  engine vocabulary includes playback and logical export dispatch, while their wire projection
  remains with the future unified protocol host.
- `superi-cli` reaches this module only through `superi-api`.

## Invariants and operational boundaries

- Canonical arguments are exact; the reference engine is not a general editor model.
- Source validation is bounded, digest checked, and atomic.
- Four mutations have stable typed IDs and complete internal prior state.
- Scenario transactions contain one to 64 ordered actions, use an exact revision fence, publish one
  revision and one undo unit, and leave state, history, sequences, and events unchanged on failure.
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
- Undo and redo restore complete semantic state without reimport or filesystem effects.
- Default registry construction is vendor free; host and vendor behavior remains opt-in.
- Default registry construction includes all four in-tree source backends as primary priority-100
  registrations, with stable identifiers preflighted before source registry mutation.
- Introspection is declaration-only and has deterministic ordering.
- Timeline resource preparation requires the exact reachable media request set and at least one
  unique explicit decoder stream for each source-bearing media request.
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
  teardown occur outside the controller through exact immutable actions.
- An attached logical export queue must report every job final before the dispatcher accepts the
  exact export teardown completion token.
- Startup is dependency-first and teardown is dependent-first. A retained failed resource blocks
  teardown of its dependencies but not independent branches.
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
- Remaining placeholder modules do not imply whole-engine transport, node registration,
  plugin supervision, or validation behavior.

## Tests and verification

Canonical scenario contracts prove the exact fixture metadata, names, half-open ranges, complete
graph topology and ports, mirror matrix, four operation IDs and original revisions, two undo plus
two redo recovery at revision 8, rejected-action atomicity, payload digest failure, and the 64 MiB
bound.

Four dispatcher contracts prove atomic multi-action rollback and commit, one revision and one undo
unit, optimistic conflict rejection, matching complete replacement state events, sequence ordering,
full-queue rejection before mutation, off-domain rejection before mutation, coherent playback,
rendering, and export admission through healthy and degraded states, recovery, stale permit
rejection, retained failure context, restart, shutdown, and both scenario and lifecycle inspection.

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
alpha, and metadata. It also proves exact request-set validation, fallback-tier policy evidence,
selected-factory failure without retry, pre-cancelled atomic failure, and later success through a
fresh operation context.

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
blocking-safe shutdown.

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

Four lifecycle contracts drive the public EngineControl owner through exact action tokens. They
prove shared-state, playback, rendering, and export initialization order; healthy common admission;
playback-only and rendering-plus-export degradation; recovery; immutable prior snapshots and stale
permits; cross-thread immutable inspection; reverse teardown; direct and stopped restart with fresh
lifetimes; initialization rollback; stale completion rejection; terminal engine failure;
dependency-safe teardown failure and retry; and unchanged owner state after off-domain construction,
inspection, and mutation rejection.

Four error-recovery contracts drive the coordinator beside that lifecycle. They prove all four
recoverability dispositions and stable request codes, source-chain and ordered-context retention,
separate safe projection, playback-only and rendering-plus-export denial, unrelated-work
continuation, exact retry and correction completion, failed-recovery reclassification, stale
sequence rejection without lifecycle mutation, terminal all-work denial, bounded oldest-event
eviction, cross-thread immutable record safety, and off-domain coordinator rejection.

## Current status and risks

Canonical command state is substantive and test-backed, but it is a reference boundary whose
implementation identity is disclosed as such. It validates fixture bytes without opening their
container or decoding frames. Timeline and graph state are exact control models but do not use the
production timeline owner or the generic `superi-graph` DAG store.

The typed dispatcher is the substantive engine-wide control seam for current scenario, lifecycle,
classified failure and recovery, work admission, interactive transport, and logical export
operations. Playback crosses
one bounded nonblocking bridge to its real domain owner. Logical export state remains in the
canonical queue behind a type-erased dispatcher controller, while prepared executors receive fresh
revision-scoped permits and render-export revalidates one before publication. The public JSON
scenario projection remains intentionally narrower than the full Rust engine vocabulary.

Three orchestration files remain documentation-only placeholders. Source registration, timeline
media preparation, deterministic lifecycle, classified failure propagation and recovery,
foreground playback, interactive transport, render-export execution, and bounded logical export
scheduling are coherent and test-backed, but there is no timeline-owned decoded-audio graph
renderer, persistent cache lifecycle owner, native GPU viewport or export submission, packet muxer,
output publisher, project persistence, native plugin discovery, isolated OpenFX adapter
implementation, worker transport, or real-condition validator.
The effects-side
OpenFX host contract is substantive, but `plugins.rs` remains the production supervisor placeholder.
Playback prediction, foreground orchestration, and transport control are substantive, but they
accept caller-prepared graph, audio, cache, and viewport owners and are not a source session binder,
decoded-audio rate processor, or native presentation path. Engine A/V coordination consumes the
shared scheduler and actual audio clock, but physical A/V latency and drift remain hardware-lane
evidence. `TimelineResources` prepares the reachable sources, decoders, and graph, and its acquired
media owner now has a real export consumer.
Export still requires caller-supplied graph, delivery, and audio stage owners and returns elementary
streams without muxing or persistence. The export queue retains results in process memory, uses
cooperative checkpoints, and requires an explicit blocking-safe shutdown; it is not a persistent or
crash-recoverable scheduler. Its generic executor preparation and typed results remain process-local
runtime bindings, and no application or wire API path invokes them yet. The derived-media driver
and resolver are synchronous and caller-owned, and no application or API path invokes them yet.
Clip-mix reconciliation is substantive but currently entered by Rust callers rather than the public
API or playback controller. Lifecycle is a production control-plane contract, but later native
render submission, export publication, resource arbitration, and subsystem owners still must
perform concrete actions before acknowledging it.

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
registration synchronized with all four media-I/O adapters, and keep resource preparation bound to
the timeline compiler, exact reachable media set, persistent source identity, explicit decoder
streams, one-shot selection, operation checks, and all-or-nothing publication. Keep derived
request canonicalization synchronized with every media format field that can change encoder output,
and keep codec selection, cancellation, complete publication, proxy admission, scheduler-owned
quality choice, lazy source opening, and full identity verification explicit. Keep playback prefetch
domain-owned, nonblocking, cooperatively cancellable between exact frames, and bound to complete
cache identity. Keep transport frame and clock anchors separate, protect explicit and loop-boundary
frames, make dropping scheduling-only, and keep audio discard callback-owned. Keep lifecycle actions
nonblocking on EngineControl, add new canonical subsystems
only in dependency order, preserve exact token checks, update every work requirement that consumes
them, and never release a dependency while an owned dependent remains. Keep foreground playback
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
payloads.
Remove a placeholder label only after substantive behavior and consumer proof exist.
When implementing `plugins.rs`, consume `superi_effects::ofx::IsolatedOfxAdapter` and preserve its
worker-process, bounded-message, deadline, permission, restart, and quarantine guarantees rather
than creating an engine-private editable plugin model.
