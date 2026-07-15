---
module_id: superi-engine
source_paths:
  - open/crates/superi-engine
source_hash: a3a059e00137405fdef40c021a7906c5bda2e7530436a1489c94e6b5ecfacf32
source_files: 33
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-engine` is the open orchestration layer. Its substantive paths cover canonical editorial
command state, media backend registry assembly, declaration-based capability
introspection, CPU-decoded frame upload into GPU allocations, and exact viewport and export color
metadata branching, plus codec-neutral proxy and optimized-media packet generation and transparent
proxy or original-source resolution, predictive playback cache population, and atomic timeline and
clip-mix edit publication. It now also owns canonical subsystem initialization, dependency ordering,
immutable lifecycle state, coherent playback/render/export admission, degraded recovery, reverse
teardown, and restart. Full transport, rendered color execution, export muxing, broad transactions,
resources, plugins, nodes, A/V sync, and validation remain incomplete.

The command path is a bounded reference owner for contract conformance. It does not claim to replace
the production project, timeline, graph, media, color, render, or muxing owners.

## Source inventory

- `open/crates/superi-engine/Cargo.toml`: Declares subsystem dependencies, optional codec features,
  production `sha2`, and test-only `pollster`.
- `open/crates/superi-engine/src/audio_mix.rs`: Applies timeline edit batches and audio clip-mix
  identity reconciliation against cloned subsystem states, publishing both only after timeline and
  mix revisions, fragment inheritance, replacement transfer, and removal all validate.
- `open/crates/superi-engine/src/av_sync.rs`: Placeholder for A/V synchronization.
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
- `open/crates/superi-engine/src/media.rs`: Builds default and feature-gated media registries.
- `open/crates/superi-engine/src/nodes.rs`: Placeholder for media and graph nodes.
- `open/crates/superi-engine/src/playback.rs`: Defines playback-domain nonblocking prediction
  submission and polling, cooperative generation supersession, structured degraded completion, a
  weak worker-pool lifecycle boundary, and the shared-graph owned-host-cache evaluator.
- `open/crates/superi-engine/src/plugins.rs`: Placeholder for plugins and extensions.
- `open/crates/superi-engine/src/proxy_substitution.rs`: Validates exact proxy purpose, source
  fingerprint, source revision, packet integrity, and stream metadata; translates cache quality to
  scheduler quality; consumes deterministic derived selection; lazily opens verified original
  media; and adapts complete generated packets to the codec-neutral `MediaSource` contract.
- `open/crates/superi-engine/src/render.rs`: Defines independent viewport and export color metadata branches from cached scene state, requiring correctly classified terminal display or output stages.
- `open/crates/superi-engine/src/resources.rs`: Placeholder for resource arbitration.
- `open/crates/superi-engine/src/validation.rs`: Placeholder for real-condition validation.
- `open/crates/superi-engine/tests/av1_capability_contract.rs`: Default AV1 selection proof.
- `open/crates/superi-engine/tests/audio_editorial_mix_contract.rs`: Proves atomic timeline and
  clip-mix publication plus complete audio intent, source, record, identity, grouping, linking, and
  synchronization preservation through a real razor edit.
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
default registry and the feature-gated explicitly configured vendor constructor.

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

`playback` exposes the `PlaybackPrefetchEvaluator` seam, concrete
`GraphPlaybackPrefetchEvaluator`, playback-owned `PlaybackPrefetcher`, and structured submission
and completion reports. Submission accepts one caller-owned `JobId` and bounded cache prediction,
cancels the previous generation, queues playback-priority cache work without waiting, and requires
the playback execution domain. Polling is also playback-owned and never blocks. An empty clipped
plan cancels older work and publishes immediate successful no-work completion.

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

The seven remaining placeholder modules contain documentation only.

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

Default registry construction atomically registers permissive Rust codecs. `os-codecs` may add
host-discovered operations, and `vendor-codecs` requires explicit worker configuration.
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

### Editorial audio intent

The engine applies a timeline edit batch to a cloned project, reads its typed fragment, inserted,
and removed identities, and translates only clip identities that already own explicit audio
intent. Right fragments inherit complete controls, replacements transfer them, and whole removals
delete them. The same mutation batch is revision checked by `superi-audio`; only then does the
engine replace both caller-owned states. Stable-identity trims, moves, slips, and rolls require no
audio mutation, so their user intent remains attached without synthesis.

## Dependencies and consumers

- `superi-core` supplies errors, identifiers, geometry, and exact time used directly by canonical
  commands, introspection, upload, playback prefetch, and retained lifecycle failure evidence.
- `sha2` supplies bounded fixture payload identity and complete packet-content fingerprinting.
- `superi-media-io`, `superi-gpu`, and `superi-codecs-rs` support registry, declaration, upload,
  codec-neutral derived generation, and the common proxy or original source interface.
- Platform and vendor codec crates are feature-gated.
- Cache now supplies color metadata, media-neutral derived publication, bounded playback prediction,
  and an owned host evaluator adapter. Graph supplies the immutable evaluation snapshot.
  Concurrency supplies the production quality and fallback selector for proxy resolution plus
  playback ownership, worker priority, cancellation, progress, and nonblocking completion. It also
  supplies the shared lifecycle coordinator and lock-free signal, EngineControl domain enforcement,
  single-owner mutable state, and immutable generated snapshot publication used by engine lifecycle.
  Image and timeline support the color metadata integration contract. Timeline and audio are jointly
  consumed by the clip-mix edit transaction. Effects now supplies the safe
  `IsolatedOfxAdapter` contract, typed requests, graph projection, and plugin lifecycle state that a
  future engine worker supervisor can consume, but engine implements no adapter, native discovery,
  transport, or production command integration. Color execution, AI, and project likewise remain
  declared dependencies without production command integration.
- `superi-api` consumes command snapshots and capability snapshots, preserving the public seam.
- `superi-cli` reaches this module only through `superi-api`.

## Invariants and operational boundaries

- Canonical arguments are exact; the reference engine is not a general editor model.
- Source validation is bounded, digest checked, and atomic.
- Four mutations have stable typed IDs and complete internal prior state.
- Export is not a project mutation and is not represented in engine history.
- Undo and redo restore complete semantic state without reimport or filesystem effects.
- Default registry construction is vendor free; host and vendor behavior remains opt-in.
- Introspection is declaration-only and has deterministic ordering.
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

The two generation contracts and three substitution contracts run through the default registry and
real Rust AV1 encoder. They prove complete packet timing and metadata, deterministic content and
artifact identity across
independent catalogs, exact reuse that skips different input, explicit quality identity, settings
mismatch rejection, and cancellation before publication. Substitution then proves real packet reads
through `MediaSource`, exact and lower-quality choice, order-independent cache-ID ties, replacement,
exact seek preroll, stale and mismatched rejection, missing and higher-only fallback, original
identity verification, and source-only final delivery. This is a real encoder and source-interface
consumer but not a render, resize, mux, persistence, playback clock, or container-delivery proof.

Four playback orchestration contracts prove playback-domain enforcement, immediate nonblocking
submission and polling, exact nearest-first execution, cooperative supersession between frames,
structured degraded failure, recovery on later work, and boundary-empty cancellation without a
replacement job. The graph integration contract uses the real immutable snapshot and budgeted host
cache to prove three exact predicted frames execute once, reuse without node calls, and return the
same foreground evaluator value.

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

Seven orchestration files remain documentation-only placeholders. There is no coherent source
registry integration, playback clock, audio flow, persistent cache lifecycle owner, rendered color
execution, encoder-to-mux path, project persistence, native plugin discovery, isolated
OpenFX adapter implementation, worker transport, or real-condition validator. The effects-side
OpenFX host contract is substantive, but `plugins.rs` remains the production supervisor placeholder.
Playback prefetch is substantive but is not a full transport or proxy selector. The
derived-media driver and resolver are synchronous and caller-owned, and no production playback,
export queue, or API path invokes them yet. Clip-mix reconciliation is substantive but currently
entered by Rust callers rather than the public API or a playback controller. Lifecycle is a
production control-plane contract, but the later source, transport, render, export, resource, and
error checkpoints still must perform their concrete subsystem actions before acknowledging it.

## Maintenance notes

Keep fixed canonical state synchronized with `docs/vertical-slice.md`, the strict API projection,
CLI runner, and operation contracts. A new production owner should replace the corresponding stub
through its real crate rather than growing this reference model into a competing system. Registry
or upload changes require updating their actual consumers and tests independently. Keep derived
request canonicalization synchronized with every media format field that can change encoder output,
and keep codec selection, cancellation, complete publication, proxy admission, scheduler-owned
quality choice, lazy source opening, and full identity verification explicit. Keep playback prefetch
domain-owned, nonblocking, cooperatively cancellable between exact frames, and bound to complete
cache identity. Keep lifecycle actions nonblocking on EngineControl, add new canonical subsystems
only in dependency order, preserve exact token checks, update every work requirement that consumes
them, and never release a dependency while an owned dependent remains. Remove a placeholder label
only after substantive behavior and consumer proof exist.
When implementing `plugins.rs`, consume `superi_effects::ofx::IsolatedOfxAdapter` and preserve its
worker-process, bounded-message, deadline, permission, restart, and quarantine guarantees rather
than creating an engine-private editable plugin model.
