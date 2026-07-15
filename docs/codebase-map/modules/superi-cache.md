---
module_id: superi-cache
source_paths:
  - open/crates/superi-cache
source_hash: 211bb872d63c4b1ce82eb05e63a6b5aa6d046a5e78cce70ff90557fd6e8223ec
source_files: 20
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-cache` owns deterministic reusable-result identity, budgeted final-frame and
intermediate-node memory retention, exact global, project, and device cache accounting, and the
priority-aware least-recently-used eviction policy, precise graph edit invalidation, and versioned
persistent final-frame and intermediate-node value storage. It also owns the media-neutral identity
and complete-only publication contract for replaceable proxy and optimized media, plus layered
memory and persistent render reuse, bounded background population, and bounded exact-frame playback
prediction plus deterministic target planning for likely edit boundaries and observed timeline
scrubs. It also owns deterministic memory, persistent, and derived-media inspection, exact clearing
evidence, and safe persistent namespace relocation. Cross-quality substitution is
scheduler-owned and engine-integrated. Composite keys join identities
owned by media, graph, parameter, image-color, time, and render-setting boundaries without taking
ownership of those source models. One atomic memory state retains cloneable
evaluator values in separate final and intermediate LRU tiers only while exact byte and frame
reservations remain live, reclaims lower-priority work under pressure, removes precise graph edit
lineage, and fences late work from invalidated revisions. A caller-versioned disk adapter retains
encoded values across cache reconstruction. Exact cached color metadata remains implemented beside
both retained-value paths. A separate ordered derived-media catalog binds every
published payload to source identity, source revision, purpose, quality, and complete render
settings while leaving codec execution and cross-quality selection in the engine and scheduler.

## Source inventory

- `open/crates/superi-cache/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`,
  `superi-image`, `superi-graph`, `superi-concurrency`, and the pinned workspace `sha2` package.
- `open/crates/superi-cache/src/disk.rs`: Defines validated disk configuration and value schemas,
  caller-owned value codecs, complete outer identity scopes, separate final and intermediate entry
  namespaces, bounded versioned envelopes, synchronized atomic publication, corruption quarantine,
  classified diagnostics, deterministic no-follow namespace inspection, same-parent atomic
  clearing, rename-first relocation with synchronized destination-local copy fallback, and the
  graph retained-value adapter.
- `open/crates/superi-cache/src/eviction.rs`: Defines checked global, per-project, and per-device
  byte and frame limits, exact managed-payload costs and usage, serialized admission, immutable
  diagnostics, classified pressure scopes, host reservations, composite GPU reservations, RAII
  release, the public tier selector, and a generic logical-stamp LRU map with deterministic victim
  ordering and overflow normalization.
- `open/crates/superi-cache/src/frame.rs`: Defines budgeted `FrameMemoryCache<V>`, its scoped graph
  adapter, complete caller-owned cache and placement context, final-frame and intermediate-node LRU
  stores plus graph and node revision fences in one atomic state, deterministic key inventories,
  exact cost callback, host or device admission, pressure-scoped automatic reclamation, immutable
  inspection with budget evidence, precise invalidation and clear reports, explicit per-tier
  reclamation, poison recovery, caller-handle cloning or destruction outside locks, and an owned
  asynchronous host adapter that preserves the complete cache scope. It also defines immutable
  `CachedFrameColorMetadata` over exact graph color metadata.
- `open/crates/superi-cache/src/key.rs`: Defines cache-local media identity, domain-separated
  parameter and render-settings fingerprints, complete color-pipeline fingerprinting, physical-time
  canonicalization, and the composite frame or intermediate-output cache key.
- `open/crates/superi-cache/src/lib.rs`: Documents implemented composite identity, budgeted memory
  retention, hierarchical accounting, deterministic eviction, precise edit invalidation, versioned
  corruption-recovering disk persistence, media-neutral derived generation publication, bounded
  playback prediction, bounded edit and scrub warming, deterministic management, safe persistent
  relocation, and exports the eight owned modules.
- `open/crates/superi-cache/src/prefetch.rs`: Defines validated finite prediction configuration,
  exact signed playback observations, nearest-first critical and predictive requests, trailing
  work, deterministic plans, half-open timeline clipping, and a hard 512-frame plan ceiling.
- `open/crates/superi-cache/src/proxy.rs`: Defines proxy and optimized purpose and quality values,
  complete source and render-setting generation requests, deterministic derived and artifact
  identities, immutable complete payloads, exact lookup with original-source fallback, reuse, and
  replacement that publishes only after a fallible producer succeeds, plus payload-neutral
  inspection and exact clear reports that retain source, revision, purpose, quality, and content
  evidence.
- `open/crates/superi-cache/src/render.rs`: Defines owned host or device render context, exact
  graph-derived frame keys, memory-first and persistent-fallback reuse, disk-hit promotion,
  cancellation-safe staged cache publication, bounded background jobs, exact-frame single-flight,
  deterministic active-work inspection, and explicit task, frame, queue, and shutdown controls.
- `open/crates/superi-cache/src/warming.rs`: Defines checked half-open timeline frame bounds, hard
  edit radius, scrub horizon, stride, and target limits, immutable exact-frame targets, canonical
  nearest-first multi-edit plans, velocity-aware forward and backward scrub plans, stationary
  nearest-neighbor plans, and caller-owned execution boundaries.
- `open/crates/superi-cache/tests/cache_key_contract.rs`: Proves every required invalidation axis,
  canonical media sets and physical time, complete color meaning and order, proxy and precision
  separation, stable vectors, invalid media input, domain separation, and thread-safe key values.
- `open/crates/superi-cache/tests/eviction_contract.rs`: Proves exact final-frame hit promotion,
  least-recent victim order, oversized reclamation, tier isolation, intermediate removal, and
  identical graph recomputation after evicted derived values are requested again.
- `open/crates/superi-cache/tests/cache_invalidation_contract.rs`: Proves one editable parameter
  edit removes only affected final and intermediate lineage, retains unrelated and upstream work,
  recomputes the current snapshot precisely, and fences repeated old-snapshot insertion.
- `open/crates/superi-cache/tests/frame_disk_cache_contract.rs`: Proves restart persistence,
  concurrent reads, independent final and intermediate tiers, upstream pruning, schema namespace
  isolation, bounded malformed-entry recovery, digest and semantic corruption quarantine, unchanged
  fresh results, retryable, degraded, user-correctable, and terminal diagnostics, deterministic
  lifecycle classification, transparent clearing and recomputation, relocation and reopen reuse,
  conflict preservation, and symbolic-link rejection.
- `open/crates/superi-cache/tests/frame_memory_cache_contract.rs`: Proves concrete final-frame
  reuse, intermediate-node subtree pruning, graph-state and outer render-context freshness,
  deterministic composite key inventory, separate tier counts, bounded host retention, semantic
  equality after admission refusal, device retention through the shared GPU cache class, release,
  immutable inspection, exact clear reports, the `Send + Sync` public memory-cache boundary, and
  exact reuse through the owned host adapter.
- `open/crates/superi-cache/tests/memory_budget_contract.rs`: Proves checked configuration, exact
  byte and frame admission, global, project, and device isolation, deterministic classified
  refusal, usage and peak diagnostics, GPU rollback, RAII cleanup, and concurrent hard-limit safety.
- `open/crates/superi-cache/tests/prefetch_contract.rs`: Proves nearest-first forward prediction,
  exact reverse and faster signed steps, boundary clipping, finite configuration, observation
  validation, bounded request counts, and deterministic equal-input plans.
- `open/crates/superi-cache/tests/proxy_generation_contract.rs`: Proves every derived identity axis,
  exact reuse, explicit replaceability, source freshness, complete-only publication, failure
  preservation, payload-neutral source, revision, quality, and content inspection, exact clearing,
  and deterministic original-source fallback.
- `open/crates/superi-cache/tests/render_cache_contract.rs`: Proves persistent-to-memory promotion,
  quality and proxy identity separation, owned device accounting, bounded background population,
  exact foreground reuse, duplicate rejection, cooperative cancellation without new publication,
  deterministic active-key cleanup, and pre-queue rejection of noncacheable graph work.
- `open/crates/superi-cache/tests/cache_warming_contract.rs`: Proves checked policy and frame bounds,
  duplicate-independent bounded edit ordering, forward, backward, stationary, and clipped scrub
  ranking, real graph-backed memory warming, exact demand reuse, source-fingerprint freshness, and
  unchanged recomputation after ordinary budget pressure.

## Public surface

The library publicly exports `disk`, `eviction`, `frame`, `key`, `prefetch`, `proxy`, `render`, and
`warming`.
`eviction::MemoryCacheTier` selects final frames or intermediate nodes without importing graph-owned
retention kinds into budget and management interfaces.
The `key` module exposes `MediaCacheIdentity`, `ParameterStateFingerprint`,
`RenderSettingsFingerprint`, `ColorPipelineFingerprint`, `FrameCacheKeyInputs`, and
`FrameCacheKey`. Together they compose media content, graph lineage, evaluated parameters, exact
color meaning, physical time, and render settings into one versioned SHA-256 digest.

`proxy` exposes `DerivedMediaPurpose`, `DerivedMediaQuality`, `DerivedMediaRequest`,
`DerivedMediaKey`, `GeneratedMedia<T>`, `DerivedMediaArtifact<T>`, `DerivedMediaLookup<T>`, and
`DerivedMediaCatalog<T>`, plus payload-neutral inspection and clear-report values. The request
composes one authoritative `MediaCacheIdentity`, exact source revision, purpose, quality, and
caller-owned `RenderSettingsFingerprint`. The catalog reuses only the exact request key, publishes
one immutable `Arc` artifact after successful complete generation, permits explicit replacement,
reports ordered source, freshness, quality, content, and byte evidence, and returns the
authoritative original source when no exact result exists.

`frame::FrameMemoryCache<V>` creates empty final and intermediate tiers, reports each tier's entry
count, returns each tier's `FrameCacheKey` inventory in deterministic order, publishes one
consistent budget snapshot, captures immutable key and accounting inspections, clears both tiers
with exact removed-key evidence, and exposes
`evict_lru`, which removes up to a requested count from one tier and returns exact victim keys in
least-recent-first order. Construction requires a shared `CacheBudgetManager` and exact caller-owned
value-cost function. `FrameMemoryCacheContext` binds one project, host or device placement, media,
parameter, color, and render identity for one evaluation.
`FrameMemoryCache::scope` returns `FrameMemoryCacheScope`, which implements
`superi_graph::eval::EvaluationValueCache<V>` for cloneable caller values and composes each
graph-supplied lineage and exact work time into a `FrameCacheKey` before lookup or insertion.
`OwnedHostFrameMemoryCache` retains an `Arc<FrameMemoryCache<V>>` plus complete project, media,
parameter, color, and render identity. It implements the same adapter for asynchronous host work by
recreating the existing borrowed scope at each operation and never implies device residency.
`FrameMemoryCache::apply_invalidation` accepts a graph-owned `GraphEditInvalidation`, atomically
advances every affected graph and node revision fence, removes superseded or unversioned values
from both tiers, and returns a deterministic `CacheInvalidationReport` with exact removed keys.
`CachedFrameColorMetadata::from_graph`, `matches`, and `pipeline` preserve exact source tags,
ordered transforms, and output intent.

`render::RenderCacheContext` owns one complete outer result context and a host or device
`RenderCachePlacement`. `RenderCache<V>` derives the exact target `FrameCacheKey`, layers the
budgeted memory adapter over an optional persistent adapter, promotes persistent hits to memory,
and evaluates fresh work through the same graph snapshot on a complete miss.
`BackgroundRenderQueue<T, N, V>` binds one immutable `GraphEvaluationSnapshot` and render cache to
one bounded `BoundedWorkerPool`. `submit` returns a typed `BackgroundRenderTask`, rejects an active
duplicate exact frame, and publishes staged values only after the job's final cancellation,
deadline, and progress checks pass. Tasks expose exact frame identity, cooperative cancellation,
job identity, and typed completion with terminal progress; the queue exposes frame cancellation,
deterministic active keys, worker snapshots, and shutdown. Submission accepts caller-prepared
cooperative cancellation and deadline controls.

`prefetch` exposes `PlaybackPrefetchConfig`, `PlaybackPrefetchInput`, `PlaybackPrefetchPlan`,
`PlaybackPrefetchRequest`, `PlaybackPrefetchDirection`, `PlaybackPrefetchUrgency`, and
`MAX_PREFETCH_FRAMES`. Configuration fixes critical, farther predictive, and trailing counts.
Planning follows the exact nonzero signed transport step, keeps nearest critical work first, clips
to one caller-owned half-open range, and never repeats the current playhead.

`disk::DiskCacheConfig` validates a caller-selected root and per-entry byte bound.
`DiskCacheValueSchema` gives one caller-owned value family a stable identifier, revision, and digest,
while `DiskCacheCodec<V>` owns canonical tier-aware encoding and decoding. `FrameDiskCache<V>`
creates a format and schema namespace, exposes ordered diagnostics, returns deterministic namespace
inspection and clearing reports, relocates with exclusive ownership, and returns a
`FrameDiskCacheScope` for one `FrameDiskCacheContext`. Relocation reports unchanged, renamed, or
copied publication plus cleanup degradation. The scope implements the same graph
`EvaluationValueCache<V>` seam without requiring cloneable values or propagating storage failure
into evaluation.

`eviction` exposes `CacheBudgetLimit`, `CacheMemoryBudgets`, `CacheCost`, `CacheBudgetUsage`,
`CacheBudgetStats`, `CacheBudgetManager`, and non-cloneable `CacheBudgetReservation`. Host admission
charges total and project scopes. Device admission also charges one `DeviceId` and holds a real
`superi_gpu::pool::MemoryReservation` in `MemoryClass::Cache` for the same managed bytes.

`warming` exposes `CacheWarmBounds`, `CacheWarmPolicy`, `CacheWarmReason`, `CacheWarmTarget`,
`CacheWarmPlan`, and `CacheWarmPlanner`. Bounds identify one nonempty half-open timeline frame
interval. Policy owns exact edit radius, scrub-ahead and scrub-behind horizons, a capped observed
scrub stride, and one nonzero hard target limit. Edit planning canonicalizes and bounds distinct
input boundaries, then ranks the nearest valid frames across every boundary independently of input
order or duplicates. Scrub planning infers forward, backward, or stationary intent from two exact
in-bounds frame observations, ranks the current frame first, and clips predicted candidates to the
available interval. Targets contain only frame, reason, and deterministic rank. Callers map them to
their ordinary complete evaluation request and cache scope.

## Architecture and data flow

At orchestration, authoritative owners provide one project, host or device placement, media
identities, canonical parameter bytes, color pipeline state, and canonical render-setting bytes to
one `FrameMemoryCacheContext`. Graph cached evaluation supplies `EvaluationCacheIdentity` values
containing its deterministic lineage digest plus stable graph identity, optional editable revision,
and exact endpoint, physical time, and region. The scoped adapter first checks the graph and node
revision fence, derives one `FrameCacheKey` per graph work unit, checks the matching final or
intermediate tier, and returns an owned clone on a hit.

`LazyEvaluator::evaluate_with_cache` checks the final key before node execution and recursively
stops at retained intermediate values. On a miss it executes through the ordinary immutable graph
contract, then offers only successful work whose graph inspection produced an available identity.
`FrameMemoryCache<V>` stores both ordered LRU value maps and minimum valid revisions in one
`Mutex<FrameMemoryCacheState<V>>`. Every entry is partitioned by project and host or device
ownership, preserves evaluator identity for precise invalidation, and pairs `Arc<V>` with one
non-cloneable budget reservation. Lookup promotes the entry and clones the retained `Arc` while
holding the state lock, releases the guard, and only then clones the caller-defined value handle.
Insertion computes exact cost without a lock, releases any replaced entry, and performs
hierarchical and optional GPU admission without the state lock. A refusal queries the constrained
total, project, or device scope, removes one eligible LRU victim, releases its cache and optional GPU
reservation after unlock, and retries the same admission path. Intermediate entries are lower
priority than final frames, while victim choice inside each tier is oldest access then composite
key. After potentially slow pressure and admission work, insertion rechecks the revision fence and
publishes only an admitted current entry. If no eligible victim can make room, the fresh result
still returns and retention is skipped. Inspection snapshots both ordered tiers under their shared
lock, then obtains budget diagnostics without claiming a cross-lock transaction. Clearing extracts
both tiers under the state lock, preserves graph revision fences, releases values and reservations
after unlock, and reports exact removed keys. Because foreground render and engine prefetch share
the same cache owner, no second management state is required. Dropping the cache releases every
counter.

Applying an edit plan installs each affected node fence before releasing the shared state lock and
extracts only entries for the same graph and affected node whose revision is older than the new
revision or unknown. Reservations and values are released after that lock is dropped. Unaffected
semantic keys remain reusable across revisions, while an older or direct unversioned evaluation can
neither hit nor insert affected lineage after the fence advances.

The disk scope composes each graph identity with project, media, parameter, color, time, and render
identity into the same complete `FrameCacheKey`. Entries live under format, value-schema, project,
and final or intermediate namespaces. Reads validate a fixed-size envelope, expected tier, key,
schema digest and revision, bounded payload length, exact end of file, and SHA-256 payload digest
before caller decoding. Invalid envelopes and corrupt decoded values are renamed to unique
quarantine paths, then ordinary evaluation recomputes and republishes the same semantic value.
Writes reserve a unique same-directory file, write and synchronize the complete envelope, rename it
to the final path, and synchronize the parent directory on Unix. A cache-local mutex serializes
filesystem decisions within one cache instance, while immutable final paths and revalidation before
semantic quarantine protect concurrent readers and independently opened instances.

Persistent inspection walks the complete schema namespace in deterministic path order using
no-follow metadata. It classifies final, intermediate, quarantined, temporary, and unknown regular
files, counts exact managed and total bytes, and rejects symbolic links or special files. Clearing
first captures that inventory, renames the active namespace to a unique same-parent tombstone,
publishes and synchronizes an empty namespace, then removes detached bytes. A prepublication
failure restores the old namespace; a cleanup failure leaves the empty namespace authoritative and
records degraded diagnostics.

Relocation accepts a caller-selected destination root only through exclusive cache ownership. It
rejects nested or occupied destinations, tries one whole-directory rename, and otherwise copies a
deterministically ordered no-follow tree into a unique destination-local staging directory. Every
copied file is exclusively created and synchronized before the staged directory is renamed into
place. Only after destination publication does the cache switch roots and remove the old copied
namespace, with any cleanup failure reported while the new complete namespace remains live.

`CacheBudgetManager` serializes local admission through one state lock. It preflights total bytes,
total frames, project bytes, project frames, device bytes, and device frames in deterministic order
before counters change. Device admission releases that lock before the shared GPU pool is called;
a GPU refusal drops the provisional local reservation and rolls back all cache scopes. Immutable
stats copy current and independent peak usage plus active scope, reservation, and denial counts.
Explicit eviction snapshots stamps, orders victims oldest first with key tie breaking, and reports
exact keys. Stamp normalization preserves recency before integer overflow.

Media order and duplicates are canonicalized, physical time is reduced exactly, and all identity
categories use stable domain-separated digests. The existing metadata seam independently copies
`GraphColorMetadata` into a cache-owned immutable value and requires complete
`ColorPipelineMetadata` equality for a match.

Derived generation is separate from retained graph values. `DerivedMediaKey` hashes the complete
request with a versioned domain. A generated payload carries its producer-computed content digest
and nonzero byte length. The catalog invokes a fallible producer before touching its ordered map,
derives one stable `CacheId` from request and output content only after success, then replaces the
exact key in one insertion. Failure leaves a prior exact artifact untouched, and a miss returns the
request's source identity rather than choosing a different quality or revision. The engine is the
first real producer: it validates canonical encoder settings, drives a codec-neutral encoder to end
of stream, and returns complete packet media through this publication boundary.
Catalog inspection copies no payload. It reports each exact request, cache identity, generated
content digest, and byte length in deterministic key order. Clearing removes only replaceable
artifacts and reports the same evidence, so every later lookup follows the unchanged original-source
fallback.

Render reuse composes an owned outer context with the graph snapshot's inspected target identity
before dispatch. A foreground evaluation checks memory first, then persistent storage, promoting an
exact disk hit to memory, and finally runs the real graph evaluator through a layered adapter that
writes successful values to both tiers. Background work uses the same path through a staged adapter:
lookups remain real, but new final and intermediate inserts remain private until the worker reaches
its final cooperative checkpoint. Exact frame keys single-flight within one queue, and an RAII
active-work guard removes ownership after success, error, cancellation, timeout, or panic.

Prediction is separate from evaluation and retention. The cache policy converts an exact playhead,
timebase, bounds, and signed step into finite ordered frame requests. The engine remains responsible
for cancellation, scheduling, graph evaluation, and observing degradation. Its concrete playback
consumer passes each request through `GraphEvaluationSnapshot::evaluate_with_cache` and the owned
host adapter, so hits skip evaluator work while misses preserve the same graph result and composite
identity.

Warming planning is also separate from retained graph values and derived media. The planner keeps no
history or cache contents. For edits, it validates every boundary against one half-open frame range,
permits the exclusive end as an editorial boundary, canonicalizes duplicates, and walks distance
rings across sorted boundaries until the hard target limit. For a moving scrub, it caps the observed
frame delta, ranks predicted positions in that direction, and then ranks a smaller configured
opposite-direction tail. For a stationary scrub, it alternates the nearest earlier and later frames.
Arithmetic uses a wider checked domain before accepting only in-bounds `i64` frames. A caller warms
each target by using the same graph evaluator, complete cache context, budget, and revision fence as
ordinary demand. The planner never inserts a value, opens media, chooses quality, mutates a project,
or changes original-source fallback.

## Dependencies and consumers

- Declared internal dependencies are `superi-core`, `superi-gpu`, `superi-image`, `superi-graph`,
  and `superi-concurrency`; `sha2` is the only direct external dependency. Composite identity
  consumes core IDs and time, graph lineage and work identity, and image color metadata. Budget policy consumes
  core `ProjectId` and `DeviceId` plus the GPU pool's `MemoryClass::Cache`, pressure cooperators,
  and RAII reservation boundary.
- `superi-concurrency` supplies the bounded worker pool, `JobKind::Cache`,
  `JobPriority::Background`, typed job completion, cooperative cancellation and deadlines,
  progress, panic containment, and worker snapshots used by the background render queue.
- `superi-graph` owns evaluation, cacheability decisions, graph lineage, and the node-neutral
  retained-value adapter. It does not depend upward on concrete storage, memory policy, or outer
  render identity.
- `superi-engine::render` consumes `CachedFrameColorMetadata` when deriving independent viewport
  and export color branches.
- `superi-engine::derived_media` consumes the derived request and catalog, validates exact encoder
  settings, and publishes complete packet output from the selected codec-neutral backend.
- `superi-engine::proxy_substitution` filters exact fresh proxy artifacts, translates their quality
  values to scheduler-owned selection inputs, and retains a selected immutable artifact for the
  lifetime of its packet-backed media source.
- The render adapter is the first production cache surface to compose retained graph evaluation,
  layered persistent reuse, and bounded background jobs. Cache integration contracts remain the
  first executable consumers of that surface and of budgeted GPU cache memory. No production engine
  node catalog, playback, export, or GPU value path invokes the render queue.
- `superi-engine::playback` consumes prediction plans and `OwnedHostFrameMemoryCache`, then populates
  exact frames through the role-neutral graph evaluator on bounded playback-priority worker jobs.
- Engine generation and substitution are the cross-crate derived-media consumers, while playback
  prefetch is the first retained-value engine consumer.
- The warming integration contract is the first concrete consumer of edit and scrub plans. It maps
  exact target frames to ordinary `EvaluationRequest` values and uses the real `LazyEvaluator` plus
  `FrameMemoryCache`. No production editor, playback, engine, API, or UI owner invokes warming yet.

## Invariants and operational boundaries

- Cargo keeps cache above graph, GPU, image, and core and below engine.
- Equal result-affecting inputs produce equal composite keys. Changes to media identity or content,
  graph lineage, parameter state, color meaning or order, physical time, precision, quality, proxy
  policy, or other canonical render settings change identity.
- Only graph work with an available deterministic cache identity reaches retained storage.
  Policy-disabled, nondeterministic, and dependency-blocked work bypasses both tiers.
- Render dispatch derives one complete final-frame key before queue admission. One queue never
  accepts two simultaneously active tasks for the same key, and active ownership is removed on
  every terminal path, including panic containment.
- Background evaluation may reuse complete memory or persistent values immediately, but all newly
  evaluated final and intermediate inserts remain staged until the final cooperative job checks
  pass. Cancellation or timeout before that boundary publishes none of those new values.
- Final-frame and intermediate-node entries never share one tier. Both maps use ordered composite
  keys, and both maps and all revision fences change under one lock. No node evaluation,
  caller-defined clone, value-cost function, GPU cooperation, reservation release, or retained-value
  destruction runs while that lock is held.
- Successful hits, insertions, and replacements are exact LRU accesses. Victim order is the oldest
  serialized tier access first with the full partitioned cache key as the deterministic tie break,
  and an oversized request removes only the entries that exist.
- Automatic pressure is priority-aware across tiers: eligible intermediate entries are reclaimed
  before final frames, and total, project, or device pressure removes only entries that reduce the
  constrained scope. GPU-only refusal reclaims from the matching device.
- Eviction changes no graph state, identity, source media, quality choice, or fallback data. A later
  request is an ordinary miss and recomputes through the same evaluator contract.
- Equivalent rational coordinates share one physical-time identity. Media traversal order and exact
  duplicate references do not perturb a key; graph ordering remains represented by the graph key.
- Parameter and render settings are separate domains even when their canonical payload bytes are
  equal. Media content, color pipelines, and the composite key also use separate versioned domains.
- Cache lock poisoning is recovered for lookup, insertion, clearing, and atomic two-tier
  invalidation, and cache failure cannot change an otherwise valid freshly evaluated result.
- Memory inspection observes exact deterministic tier inventories and budget diagnostics without
  claiming a transaction across their separate locks. Clearing preserves revision fences, reports
  exact removed keys, and releases reservations outside the cache lock.
- Disk entries are partitioned by format revision, caller-owned value schema and revision, project,
  retention tier, and the complete composite key. Incompatible bytes cannot be reinterpreted as the
  current value family.
- Disk reads allocate only after the declared payload length fits the configured bound and platform.
  Integrity, identity, schema, tier, and exact-file-length checks precede caller decoding.
- A persistence read, decode, recovery, or write failure becomes a classified diagnostic and a
  graph miss or skipped insertion. Fresh evaluation meaning and final output remain authoritative.
- Invalid entries are isolated before replacement. A semantic decoder corruption is re-read before
  quarantine so a concurrently replaced entry is retained. Non-corruption decoder failures preserve
  their original category and recoverability and leave bytes in place for diagnosis.
- Persistent lifecycle traversal never follows a symbolic link and rejects special files. Clear
  publishes a synchronized empty namespace before deleting detached bytes. Relocation refuses an
  occupied or nested destination, publishes a complete destination before source cleanup, and
  preserves the source on every prepublication failure.
- Every live retained entry owns one exact nonzero byte and frame charge. Total, per-project, and
  per-device usage can never exceed configured hard limits, including under concurrent admission.
- Device values cannot enter a tier unless both the cache hierarchy and shared GPU pool admit the
  same managed payload bytes. No cache tier lock is held while GPU pressure cooperators run.
- Admission refusal is retryable resource exhaustion for explicit callers and an ordinary skipped
  insertion through graph's storage-neutral adapter. It never changes the freshly evaluated value,
  key identity, project meaning, or final output.
- Active project and device records are removed when their last reservation drops, so long sessions
  cannot accumulate historical scope state. Peak usage remains a bounded scalar diagnostic.
- An edit plan advances every affected graph and node fence under the same lock as both value tiers,
  removes only superseded affected lineage, and prevents late old or unversioned repopulation.
- Complete color-pipeline equality is required before cached color metadata matches a request.
- Derived-media reuse requires exact source `MediaId`, source-content fingerprint, source revision,
  purpose, quality, and render-settings identity. It never chooses a stale revision or another
  quality.
- Derived payloads have nonzero encoded length and remain private until producer success. Failed or
  cancelled replacement cannot remove a prior artifact, and absence deterministically exposes the
  authoritative original source.
- A derived artifact is replaceable and owns no project mutation. It cannot change source identity,
  graph meaning, or final-render settings. Cache does not choose among qualities, and explicit
  source-only delivery remains outside this crate rather than becoming implicit proxy reuse.
- Derived inspection and clearing copy no payload and preserve exact source, revision, purpose,
  quality, render settings, content identity, and byte evidence. After clearing, ordinary exact
  lookup returns the authoritative original source.
- Prediction configuration requires at least one critical frame and at most 512 total requests.
  Observation requires one exact timebase, a playhead inside caller-owned half-open bounds, and a
  nonzero signed step. Prediction changes no graph, project, source, cache identity, or final output.
- The owned asynchronous adapter is host-only. Device placement still requires a borrowed GPU pool
  and pressure-cooperator scope, so asynchronous ownership cannot silently change accounting.
- Warming targets are bounded exact timeline frame positions, not cache keys or stored values.
  Equal edit inputs produce equal priority order regardless of input order or duplicates. Scrub
  direction, stride capping, clipping, and tie order are deterministic. The hard target limit bounds
  every plan before any caller-owned evaluation begins.
- Warming never bypasses complete cache identity, memory admission, LRU eviction, disk schema,
  graph revision fences, source freshness, proxy quality selection, or original fallback. A skipped,
  evicted, stale, or failed speculative request remains an ordinary demand miss and cannot change
  project meaning or the freshly evaluated result.
- Driver allocation overhead and physical residency remain outside managed-payload accounting.
  Invalidation invocation remains a later concern. Cross-quality substitution is scheduler-owned
  and engine-integrated, while playback scheduling stays owned by the engine and concurrency
  layers.
- Persistent storage has explicit inspection, clearing, and relocation but no automatic capacity
  cleanup, cross-process single-flight lock, default root, or default capacity. Callers own policy,
  external directory coordination, and value schema evolution.

## Tests and verification

`cache_key_contract.rs` runs five integration contracts through real graph inspection and image
pipeline construction. It proves media ID and fingerprint invalidation, graph and parameter edits,
source interpretation, named space, ICC payload, stage order, display versus delivery intent,
physical time equality, time changes, precision and proxy-quality separation, media-set order and
deduplication, invalid fingerprint rejection, domain separation, a locked digest vector, and
`Send + Sync` key values.

`frame_memory_cache_contract.rs` runs nine contracts through the real graph cached-evaluation path.
It proves exact final reuse, final and intermediate tier separation, an intermediate hit that skips
upstream work, fresh results after graph state changes, fresh results after outer render identity
changes, deterministic composite key order, exact host bounds, automatic replacement and fresh
recomputation under pressure, independent ownership of equal results by project, device accounting
and automatic GPU-only pressure replacement in the shared GPU cache class, explicit release,
deterministic inspection, exact clear evidence and recomputation, and a thread-safe cache type with
exact `Send + Sync` owned host-adapter reuse.
`frame_disk_cache_contract.rs` runs eleven contracts through the same graph path. It proves reuse
after cache reconstruction and eight concurrent reads, separate final and intermediate persistence,
real upstream pruning, digest and semantic corruption quarantine followed by exact recomputation,
terminal decoder byte retention, truncated and oversized input bounds, future format isolation,
value-schema revision separation, all required failure classifications with actionable context,
recovery, temporary, and unknown-file inspection, atomic clearing and unchanged recomputation,
relocation and reopen reuse, occupied-destination preservation, and symbolic-link rejection.
`memory_budget_contract.rs` runs five contracts for configuration,
hierarchical host and device admission, deterministic errors and stats, GPU rollback, cleanup, and
eight-way concurrent admission. The graph package adds adapter, failure, disabled-policy, and
role-neutral snapshot proof. The engine color propagation contract continues to exercise the
metadata seam and independent viewport and export branches.

`eviction_contract.rs` runs four contracts through the same real graph cached-evaluation path. It
proves final-frame hit promotion and exact victim order, scoped project pressure, intermediate-first
automatic pressure, tier isolation, oversized reclamation, and unchanged graph results after both
classes of derived work are recomputed.

`cache_invalidation_contract.rs` runs one end-to-end contract through editable graph snapshots,
headless runtime compilation, real budgeted final and intermediate caches, semantic edit derivation,
and the concrete invalidation API. It proves exact removed-key reports, released reservations,
unaffected upstream and unrelated reuse, current-revision recomputation, and rejection of repeated
old-snapshot repopulation.

`proxy_generation_contract.rs` runs four contracts over the public media-neutral generation
surface. It proves changes to source ID, content, revision, purpose, quality, or render settings
change the key; exact complete artifacts reuse; regeneration replaces one exact key; freshness is
strict; failed generation retains prior complete media; empty payloads fail; payload-neutral
inspection retains source, revision, quality, content, and byte evidence; exact clearing reports
every artifact; and a missing, stale, or cleared request returns the original source.

`render_cache_contract.rs` runs five contracts through real editable graph snapshots, the graph
  evaluation cache seam, the memory and disk implementations, GPU cache accounting, and the bounded
  worker pool. It proves layered promotion, exact quality identity, owned device placement and
  release, bounded background-to-foreground reuse, active duplicate rejection, queued and running
  cancellation with terminal active-key cleanup and no new publication, and noncacheable rejection
  before dispatch.

`prefetch_contract.rs` runs five contracts over public prediction policy. It proves critical before
extended and trailing order, exact forward, reverse, and faster stepping, half-open clipping,
boundary-empty plans, invalid-input classification, the hard request ceiling, and deterministic
equal-input output. Engine playback contracts then prove nonblocking consumption and exact graph
cache reuse without semantic drift.

`cache_warming_contract.rs` runs four contracts over the public planner and real retained evaluator
path. It proves zero stride and target limits, empty or reversed bounds, out-of-range edit and scrub
observations, canonical duplicate-independent multi-edit distance rings, hard output bounds,
forward, backward, stationary, and clipped scrub order, exact-frame graph warming, subsequent demand
hits without node execution, changed media fingerprint recomputation, and exact fresh results after
budget pressure evicts speculative work.

Focused package tests, graph, concurrency, dependency, and engine contract closure,
warnings-denied Clippy, and rustdoc pass after the integrated budget, LRU, invalidation,
persistence, lifecycle management, generation, layered rendering, prediction, and warming
implementation. The 53 cache integration contracts across ten files prove bounded retained
evaluator values, shared managed GPU accounting, exact eviction, precise revision-safe cleanup,
process-restart reuse, corruption recovery, deterministic lifecycle inspection, transparent
clearing and relocation, complete-only derived publication and clearing, layered render reuse,
cancellation-safe background population, bounded predictive plans, asynchronous host retention,
and bounded exact-frame edit and scrub targets. Engine contracts add real AV1 packet generation,
packet-backed transparent proxy substitution with strict fallback, and playback prefetch. They do
not prove production editor warming, playback-clock integration, muxing, or physical GPU residency.

## Current status and risks

Deterministic composite key derivation, exact cached color metadata, hierarchical budgets, budgeted
two-tier retained evaluation, priority-aware strict per-tier LRU eviction, precise revision-safe
edit invalidation, versioned two-tier disk persistence, proxy or optimized-media identity and
atomic publication, layered render reuse, bounded background population, bounded playback
prediction, owned asynchronous host retention, deterministic management inspection and clearing,
safe persistent relocation, and bounded edit and scrub warming plans are substantive and
test-backed. Final
hits stop the complete graph pull, intermediate hits stop their prerequisite subtree, hard caps
remain exact, automatic or explicit victims recompute with unchanged result meaning, edit plans
remove only affected older lineage, and disk corruption falls back to exact fresh evaluation.
Device entries share the existing GPU cache class, and eviction or invalidation releases matching
reservations outside the cache state lock. Background jobs single-flight exact frame keys within
one immutable snapshot and outer render context, stage newly evaluated values until final job
checks pass, and expose deterministic active-work inspection. The engine generates real complete
AV1 packet artifacts through the proxy module and selects exact or lower-quality fresh proxies
through the scheduler. Engine playback also consumes prediction and retained graph evaluation.
Cache itself owns no quality-selection policy. Warming has a real graph-backed retained-value
consumer test but no production editor, engine, playback, API, or UI caller. No production frame
pixels are selected or reclaimed by a playback clock, export encoder, or GPU upload through this
cache path.

The main correctness risk is each caller's canonical parameter and render-settings encoding.
Omitting one output-affecting byte can cause false reuse even though composition and storage are
stable. Future schema changes must bump the affected domain rather than silently changing a key
contract. The main budget risk is inaccurate caller-owned managed-payload cost. Separate queues or
foreground callers may still perform duplicate deterministic work because single-flight ownership
is queue-local. Concurrent lock acquisition defines the observed access order,
which may change cache contents but cannot change semantic results. Memory inspection does not
claim a transaction across the cache and budget locks. Explicit victim selection sorts
the retained tier during pressure handling, so measured production scale may justify an equivalent
indexed implementation later. Derived-media inspection and clearing are implemented, but the
catalog remains externally synchronized and has no single-flight generation, eviction, or
persistence. Generated quality must match the caller-prepared frames and canonical engine
settings; substitution admits only exact request identity and revision before scheduler choice.
Persistent value callers must version codec meaning correctly, choose bounded entry sizes, and
provide cleanup and relocation policy. Lifecycle operations coordinate only one cache instance;
independently opened instances can duplicate work or race external directory ownership, but
immutable complete keys keep published value meaning equal. Cross-filesystem relocation can leave a
classified cleanup-pending source copy only after the complete destination is authoritative.
Warming callers must map timeline frame indexes to the authoritative timeline rate and ordinary
graph request, invalidate or abandon superseded plans at their orchestration boundary, and never
treat ranking as permission to weaken exact demand. The crate is not yet a complete cache
subsystem.

## Maintenance notes

Keep stored values keyed by `FrameCacheKey`; do not fall back to the narrower graph digest alone.
Define and test canonical parameter and render-settings encodings at their owning orchestration
boundaries. Cached values should be cloneable resource handles, such as `Arc`-backed frame or GPU
wrappers, rather than deep copies of texture contents. Preserve separate final and intermediate
ownership and the invariant that every retained value owns its exact reservation. Cost functions
must report managed payload bytes rather than driver or allocator estimates. Eviction must release
entries before retrying the same serialized admission path and must not hold the state lock while GPU
pressure cooperators execute. Preserve the single-source access stamp, shared invalidation lock,
and rule that no affected value may be admitted below its graph and node revision fence. Preserve
no-follow lifecycle traversal, destination-local staged copy, publish-before-cleanup ordering, and
exact management reports. Preserve the predictor's hard bound, exact timebase, nearest-first order, and
host-only asynchronous adapter. Never weaken the disk envelope bounds, complete identity
checks, schema
partition, synchronization sequence, corruption revalidation, or fallback-to-fresh behavior.
Derived-media producers must compute complete output identity before publication, never mutate
source state, and keep media or codec dependencies above this crate.
Keep warming plans stateless, exact-frame-only, input-order-independent, and bounded before caller
evaluation. Do not add source identity, cache-key derivation, direct insertion, proxy selection,
project mutation, worker dispatch, or caller-specific timeline state to the planner. Production
editor and playback owners should map targets through their ordinary revisioned graph and complete
cache scopes, then abandon obsolete plans without changing demand behavior.
