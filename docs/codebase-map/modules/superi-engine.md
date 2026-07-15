---
module_id: superi-engine
source_paths:
  - open/crates/superi-engine
source_hash: ce2793c589d38376e2122797d653d05391edee5f55726eb5c34817e36b15313d
source_files: 36
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-engine` is the open orchestration layer. Its substantive paths cover canonical editorial
command state, complete media backend registry assembly, transactional timeline graph plus source
and decoder preparation, capability introspection, CPU-decoded frame upload, exact viewport and
export color metadata branching, derived-media generation, transparent proxy resolution,
predictive cache population, foreground graph and display-color execution, bounded audio admission,
audio-master A/V coordination with bounded video correction and discontinuity recovery, monotonic
clock fallback, lossless viewport handoff, deterministic subsystem lifecycle, and atomic timeline
plus clip-mix edits. Full transport queue policy, native GPU presentation, export muxing, broad
transactions, plugins, nodes, and validation remain incomplete.

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
  validation, monotonic revisions, and full-state undo plus redo.
- `open/crates/superi-engine/src/derived_media.rs`: Canonicalizes complete video or audio encoder
  settings, validates cache request identity, selects one explicit primary encoder, drives its
  nonblocking lifecycle to end of stream, hashes complete packet semantics, and publishes only a
  complete proxy or optimized-media payload through `superi-cache`.
- `open/crates/superi-engine/src/error.rs`: Placeholder for cross-subsystem recovery.
- `open/crates/superi-engine/src/export_queue.rs`: Placeholder for render and export queues.
- `open/crates/superi-engine/src/frame_upload.rs`: Implements the media-I/O-to-GPU upload boundary
  for CPU-addressable decoded video and retains the frame's complete color pipeline beside the GPU owner.
- `open/crates/superi-engine/src/introspection.rs`: Implements deterministic API-neutral backend and
  codec capability snapshots.
- `open/crates/superi-engine/src/lib.rs`: Documents the implemented orchestration boundaries and
  exposes seventeen engine modules.
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
  seam, bounded audio output, audio-master and monotonic clock continuity, lossless viewport
  handoff, structured degradation, and weak worker-pool lifecycle boundaries.
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
- `open/crates/superi-engine/src/validation.rs`: Placeholder for real-condition validation.
- `open/crates/superi-engine/tests/av1_capability_contract.rs`: Default AV1 selection proof.
- `open/crates/superi-engine/tests/audio_editorial_mix_contract.rs`: Proves atomic timeline and
  clip-mix publication plus complete audio intent, source, record, identity, grouping, linking, and
  synchronization preservation through a real razor edit.
- `open/crates/superi-engine/tests/av_sync_coordination_contract.rs`: Proves the interactive
  policy, playback-domain ownership, bounded hold and correction, protected and eligible-drop
  behavior, discontinuity recovery, continuous clock fallback and restoration, exact timing, and
  deterministic statistics.
- `open/crates/superi-engine/tests/color_metadata_propagation_contract.rs`: Proves exact source interpretation, ordered transform history, cache identity, independent viewport and export intent, and invalid-order rejection across media, graph, timeline, cache, and engine boundaries.
- `open/crates/superi-engine/tests/derived_media_generation_contract.rs`: Proves complete and
  deterministic real AV1 generation, exact cache reuse, quality identity, settings mismatch
  rejection, cooperative cancellation, and original-source fallback.
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
policy, scheduler statistics, exact clock reads, monotonic fallback, audio-master restoration, and
one nonblocking coordination call that applies discontinuity rebases through the concrete clock.

`playback` exposes the `PlaybackPrefetchEvaluator` seam, concrete
`GraphPlaybackPrefetchEvaluator`, playback-owned `PlaybackPrefetcher`, and structured prediction
submission and completion reports. It also exposes `DecodedFrameMetadata`,
`PlaybackSceneFrame<V>`, `PlaybackCacheIdentity`, `PlaybackDisplayTransform<V>`,
`CpuPlaybackDisplayTransform`, `PlaybackViewportFrame<O>`, `PlaybackFrameEvaluator<O>`,
`GraphPlaybackFrameEvaluator<T, N, V, D>`, `PlaybackAudioOutput`, `PlaybackPoll`, and
`PlaybackOrchestrator<O>`. Foreground orchestration admits one exact frame at a time, executes it on
a playback-priority worker, validates cache identity and scene meaning, applies display color,
coordinates presentation from the shared audio-master clock, and retains both a saturated viewport
payload and its resolved synchronization decision. Poll outcomes expose exact timing, master
position, drift, live display interval, correction, reason, and recovery evidence. Submission,
polling, audio admission, coordination, and clock switching require the playback domain and never
block.

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

The five remaining placeholder modules contain documentation only.

## Architecture and data flow

### Canonical editorial command state

The scenario engine begins with stable project identity and empty state. Import requires exact
`slice/video-cfr` version 1 metadata, lowercase manifest and payload digests, 24 fps, 96 frames, and
96 by 54 extent. It reads at most 64 MiB and independently verifies the payload digest. Placement
accepts only timeline frame zero. Trim accepts only source `[24, 72)`. The mirror action constructs
the exact source, transform, and output graph, two image edges, binary64 matrix, nearest sampling,
transparent black edges, and derived timeline identity.

Every successful mutation stores a complete before and after content snapshot plus one stable typed
operation record. The active log uses `slice.op.import`, `slice.op.insert`, `slice.op.trim`, and
`slice.op.effect` with original resulting revisions. Undo and redo restore complete content without
filesystem side effects or reimport and advance the global revision monotonically. Rejected actions
leave state and both history stacks unchanged.

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
branch validates its terminal stage kind and leaves the cached scene state unchanged. No production
path yet executes those transforms, connects uploaded textures to graph evaluation, monitors a
viewport, or encodes an export.

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

Shutdown emits teardown actions only after every owned dependent has released its resource, which
produces the exact inverse of initialization. Initialization failure enters the same reverse path
for already owned dependencies. A failed teardown retains ownership and blocks its dependencies,
while an independent branch may continue stopping. Retry must release the retained resource before
the engine can acknowledge `Stopped`. Restart first completes this teardown, increments the engine
lifetime, clears resolved failure state, and begins the canonical startup again. Action tokens and
work permits carry lifetime and revision identity, so late completion and stale work are rejected.
EngineControl never executes subsystem acquisition or teardown inline and therefore remains
nonblocking.

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
for later playback, render, and export orchestration without implementing those consumers or
copying graph, time, pixel, metadata, alpha, source, or decoder meanings.

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
  retained lifecycle failure evidence.
- `sha2` supplies bounded fixture payload identity and complete packet-content fingerprinting.
- `superi-media-io`, `superi-gpu`, and `superi-codecs-rs` support source and codec registry assembly,
  content probing, source and decoder preparation, declaration, upload, codec-neutral derived
  generation, and the common proxy or original source interface.
- Platform and vendor codec crates are feature-gated.
- Cache now supplies color metadata, media-neutral derived publication, bounded playback prediction,
  budgeted scene retention, and an owned host evaluator adapter. Graph supplies the immutable
  evaluation snapshot. Timeline supplies the retained editable graph compilation and reachable
  editorial source relationships for resource preparation. Concurrency supplies proxy selection,
  playback ownership, worker priority, nonblocking completion, the playback clock, A/V drift
  measurement and scheduling policy, bounded handoffs, the shared lifecycle coordinator and
  signal, EngineControl ownership, and immutable snapshot publication.
  Media I/O supplies exact decoded frame semantics, image supplies color and scene artifacts, color
  supplies CPU display execution, and audio supplies the bounded producer and actual presentation
  clock. Timeline and audio are also jointly consumed by the clip-mix edit transaction. Effects
  supplies the safe
  `IsolatedOfxAdapter` contract, typed requests, graph projection, and plugin lifecycle state that a
  future engine worker supervisor can consume, but engine implements no adapter, native discovery,
  transport, or production command integration. AI and project remain declared dependencies without
  production command integration.
- `superi-api` consumes command snapshots and capability snapshots, preserving the public seam.
- `superi-cli` reaches this module only through `superi-api`.

## Invariants and operational boundaries

- Canonical arguments are exact; the reference engine is not a general editor model.
- Source validation is bounded, digest checked, and atomic.
- Four mutations have stable typed IDs and complete internal prior state.
- Export is not a project mutation and is not represented in engine history.
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
  viewport progress, and never invents transport rate, seek, direction, step, or frame-drop policy.
- Decoded timing, format, metadata, color history, and alpha meaning remain immutable provenance.
  Cache admission requires the exact requested timestamp and complete nonterminal scene pipeline.
- The device-owned sample clock is the normal master. Live A/V coordination occurs only on the
  playback domain and returns rather than sleeping for early video.
- Frame PTS and nominal duration remain immutable through hold, correction, drop, and rebase
  outcomes. A corrected display interval is live presentation metadata and cannot alter render or
  export timing.
- A late drop requires both caller-owned semantic eligibility and an immediately ready successor.
  The single-ready-frame foreground owner supplies neither and therefore preserves its frame.
- Discontinuity recovery applies one exact clock reanchor and retains the triggering signed drift.
  Mode changes preserve timeline position, and viewport saturation returns the exact frame plus its
  resolved presentation for retry without another scheduler observation.
- Audio admission is all-or-nothing and A/V coordination never changes audio samples or the
  callback-owned clock counter.
- GPU ownership and pool lifetime remain tied to the originating device.
- Timeline and clip-mix publication is all-or-nothing across both expected revisions and typed edit
  outcomes. It does not imply a general whole-engine transaction owner.
- Lifecycle mutation and publication require EngineControl, while subsystem acquisition and
  teardown occur outside the controller through exact immutable actions.
- Startup is dependency-first and teardown is dependent-first. A retained failed resource blocks
  teardown of its dependencies but not independent branches.
- Action completion must match subsystem, kind, lifetime, and action revision exactly. Every
  committed state publishes a new immutable generation, and prior snapshots never mutate.
- Playback, rendering, and export permits come from one snapshot and require their complete
  transitive subsystem set to be ready in the same running lifetime.
- Nonterminal failures degrade only dependent work. Terminal failures close all admission until
  orderly teardown and a fresh lifetime complete.
- Successful recovery clears the subsystem's active failure and restores admission without erasing
  the latest reported failure evidence from the current lifetime snapshot.
- Placeholder modules do not imply whole-engine transport or render/export execution behavior.

## Tests and verification

Canonical scenario contracts prove the exact fixture metadata, names, half-open ranges, complete
graph topology and ports, mirror matrix, four operation IDs and original revisions, two undo plus
two redo recovery at revision 8, rejected-action atomicity, payload digest failure, and the 64 MiB
bound.

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

Five A/V coordination contracts prove the engine policy remains inside the product scheduling
target, coordination rejects UI and unowned callers before statistics mutate, normal and early
video return nominal presentation or a bounded hold, moderate lateness shortens only the live video
interval, protected and last-ready frames remain visible, and explicit successor plus eligibility
evidence permits dropping. They also prove a ten-second discontinuity reanchors once and presents
the unchanged frame in the same call, and that monotonic fallback plus audio restoration preserves
one continuous exact timeline.

The foreground playback integration contract constructs a real high-precision decoded frame,
immutable graph snapshot, budgeted scene cache, binary16 working image, CPU display transform,
lock-free audio producer and consumer, audio master clock, bounded worker pool, and one-slot viewport
handoff. It proves exact semantics through binary32 display output, bounded clock waiting, nominal
presentation, thirty-millisecond lateness correction, unchanged PTS and duration, cache reuse,
audio saturation, lossless viewport retry without a duplicate scheduler observation, invalid scene
rejection without cache poisoning, a greater-than-ten-second discontinuity recovery, later frame
recovery, and continuity across audio-clock loss and restoration.

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

## Current status and risks

Canonical command state is substantive and test-backed, but it is a reference boundary whose
implementation identity is disclosed as such. It validates fixture bytes without opening their
container or decoding frames. Timeline and graph state are exact control models but do not use the
production timeline owner or the generic `superi-graph` DAG store.

Five orchestration files remain documentation-only placeholders. Source registration and timeline
media preparation, deterministic lifecycle, and foreground playback are coherent and test-backed,
but there is no transport controller, decoded-audio graph renderer, persistent cache lifecycle
owner, native GPU viewport submission, encoder-to-mux path, project persistence, native plugin
discovery, isolated OpenFX adapter implementation, worker transport, or real-condition validator.
The effects-side
OpenFX host contract is substantive, but `plugins.rs` remains the production supervisor placeholder.
Playback prediction and foreground orchestration are substantive, but they accept caller-prepared
graph, audio, cache, and viewport owners and are not a full transport, proxy selector, source
session binder, queue-based frame-drop owner, or native presentation path. Engine A/V coordination
now consumes the shared scheduler and actual audio clock, but physical A/V latency and drift remain
hardware-lane evidence. `TimelineResources` prepares the reachable sources, decoders, and graph but
does not schedule packets, evaluate frames, or write outputs. The derived-media driver and resolver
are synchronous and caller-owned, and no application, export queue, or API path invokes them yet.
Clip-mix reconciliation is substantive but currently entered by Rust callers rather than the public
API or playback controller. Lifecycle is a production control-plane contract, but later transport,
render, export, resource-arbitration, and error owners still must perform concrete subsystem actions
before acknowledging it.

## Maintenance notes

Keep fixed canonical state synchronized with `docs/vertical-slice.md`, the strict API projection,
CLI runner, and operation contracts. A new production owner should replace the corresponding stub
through its real crate rather than growing this reference model into a competing system. Registry
or upload changes require updating their actual consumers and tests independently. Keep source
registration synchronized with all four media-I/O adapters, and keep resource preparation bound to
the timeline compiler, exact reachable media set, persistent source identity, explicit decoder
streams, one-shot selection, operation checks, and all-or-nothing publication. Keep derived
request canonicalization synchronized with every media format field that can change encoder output,
and keep codec selection, cancellation, complete publication, proxy admission, scheduler-owned
quality choice, lazy source opening, and full identity verification explicit. Keep playback prefetch
domain-owned, nonblocking, cooperatively cancellable between exact frames, and bound to complete
cache identity. Keep lifecycle actions nonblocking on EngineControl, add new canonical subsystems
only in dependency order, preserve exact token checks, update every work requirement that consumes
them, and never release a dependency while an owned dependent remains. Keep foreground playback
single-flight and nonblocking, retain exact sink payloads under backpressure, validate scene time,
color, and alpha before retention, preserve immutable media timing through live corrections, cache
one resolved presentation across viewport backpressure, require explicit readiness before dropping,
apply discontinuity rebases only through the concrete clock, and preserve timeline position when
changing clock sources.
Remove a placeholder label only after substantive behavior and consumer proof exist.
When implementing `plugins.rs`, consume `superi_effects::ofx::IsolatedOfxAdapter` and preserve its
worker-process, bounded-message, deadline, permission, restart, and quarantine guarantees rather
than creating an engine-private editable plugin model.
