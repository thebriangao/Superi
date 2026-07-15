---
module_id: superi-cache
source_paths:
  - open/crates/superi-cache
source_hash: bda3eaf4bd399e06d76862d2f6f064528edee6e78fda01fd6326e13c76d8348f
source_files: 17
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-cache` owns deterministic reusable-result identity, budgeted final-frame and
intermediate-node memory retention, exact global, project, and device cache accounting, and the
priority-aware least-recently-used eviction policy, precise graph edit invalidation, and versioned
persistent final-frame and intermediate-node value storage. It also owns the media-neutral identity
and complete-only publication contract for replaceable proxy and optimized media, plus layered
memory and persistent render reuse and bounded background population. Prefetch and lifecycle
systems remain planned; cross-quality substitution is scheduler-owned and engine-integrated.
Composite keys join identities
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
  classified diagnostics, and the graph retained-value adapter.
- `open/crates/superi-cache/src/eviction.rs`: Defines checked global, per-project, and per-device
  byte and frame limits, exact managed-payload costs and usage, serialized admission, immutable
  diagnostics, classified pressure scopes, host reservations, composite GPU reservations, RAII
  release, the public tier selector, and a generic logical-stamp LRU map with deterministic victim
  ordering and overflow normalization.
- `open/crates/superi-cache/src/frame.rs`: Defines budgeted `FrameMemoryCache<V>`, its scoped graph
  adapter, complete caller-owned cache and placement context, final-frame and intermediate-node LRU
  stores plus graph and node revision fences in one atomic state, deterministic key inventories,
  exact cost callback, host or device admission, pressure-scoped automatic reclamation, precise
  invalidation reports, explicit per-tier reclamation and clearing, poison recovery, and
  caller-handle cloning or destruction outside locks. It also defines immutable
  `CachedFrameColorMetadata` over exact graph color metadata.
- `open/crates/superi-cache/src/key.rs`: Defines cache-local media identity, domain-separated
  parameter and render-settings fingerprints, complete color-pipeline fingerprinting, physical-time
  canonicalization, and the composite frame or intermediate-output cache key.
- `open/crates/superi-cache/src/lib.rs`: Documents implemented composite identity, budgeted memory
  retention, hierarchical accounting, deterministic eviction, precise edit invalidation, and
  versioned corruption-recovering disk persistence, media-neutral derived generation publication,
  and exports the seven owned modules.
- `open/crates/superi-cache/src/prefetch.rs`: Placeholder for playback prediction and prefetch.
- `open/crates/superi-cache/src/proxy.rs`: Defines proxy and optimized purpose and quality values,
  complete source and render-setting generation requests, deterministic derived and artifact
  identities, immutable complete payloads, exact lookup with original-source fallback, reuse, and
  replacement that publishes only after a fallible producer succeeds.
- `open/crates/superi-cache/src/render.rs`: Defines owned host or device render context, exact
  graph-derived frame keys, memory-first and persistent-fallback reuse, disk-hit promotion,
  cancellation-safe staged cache publication, bounded background jobs, exact-frame single-flight,
  deterministic active-work inspection, and explicit task, frame, queue, and shutdown controls.
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
  fresh results, and retryable, degraded, user-correctable, and terminal diagnostics.
- `open/crates/superi-cache/tests/frame_memory_cache_contract.rs`: Proves concrete final-frame
  reuse, intermediate-node subtree pruning, graph-state and outer render-context freshness,
  deterministic composite key inventory, separate tier counts, bounded host retention, semantic
  equality after admission refusal, device retention through the shared GPU cache class, release,
  and the `Send + Sync` public memory-cache boundary.
- `open/crates/superi-cache/tests/memory_budget_contract.rs`: Proves checked configuration, exact
  byte and frame admission, global, project, and device isolation, deterministic classified
  refusal, usage and peak diagnostics, GPU rollback, RAII cleanup, and concurrent hard-limit safety.
- `open/crates/superi-cache/tests/proxy_generation_contract.rs`: Proves every derived identity axis,
  exact reuse, explicit replaceability, source freshness, complete-only publication, failure
  preservation, and deterministic original-source fallback.
- `open/crates/superi-cache/tests/render_cache_contract.rs`: Proves persistent-to-memory promotion,
  quality and proxy identity separation, owned device accounting, bounded background population,
  exact foreground reuse, duplicate rejection, cooperative cancellation without new publication,
  deterministic active-key cleanup, and pre-queue rejection of noncacheable graph work.

## Public surface

The library publicly exports `disk`, `eviction`, `frame`, `key`, `prefetch`, `proxy`, and `render`.
`eviction::MemoryCacheTier` selects final frames or intermediate nodes without importing graph-owned
retention kinds into budget and management interfaces.
The `key` module exposes `MediaCacheIdentity`, `ParameterStateFingerprint`,
`RenderSettingsFingerprint`, `ColorPipelineFingerprint`, `FrameCacheKeyInputs`, and
`FrameCacheKey`. Together they compose media content, graph lineage, evaluated parameters, exact
color meaning, physical time, and render settings into one versioned SHA-256 digest.

`proxy` exposes `DerivedMediaPurpose`, `DerivedMediaQuality`, `DerivedMediaRequest`,
`DerivedMediaKey`, `GeneratedMedia<T>`, `DerivedMediaArtifact<T>`, `DerivedMediaLookup<T>`, and
`DerivedMediaCatalog<T>`. The request composes one authoritative `MediaCacheIdentity`, exact source
revision, purpose, quality, and caller-owned `RenderSettingsFingerprint`. The catalog reuses only
the exact request key, publishes one immutable `Arc` artifact after successful complete generation,
permits explicit replacement, and returns the authoritative original source when no exact result
exists.

`frame::FrameMemoryCache<V>` creates empty final and intermediate tiers, reports each tier's entry
count, returns each tier's `FrameCacheKey` inventory in deterministic order, publishes one
consistent budget snapshot, clears both tiers explicitly, and exposes
`evict_lru`, which removes up to a requested count from one tier and returns exact victim keys in
least-recent-first order. Construction requires a shared `CacheBudgetManager` and exact caller-owned
value-cost function. `FrameMemoryCacheContext` binds one project, host or device placement, media,
parameter, color, and render identity for one evaluation.
`FrameMemoryCache::scope` returns `FrameMemoryCacheScope`, which implements
`superi_graph::eval::EvaluationValueCache<V>` for cloneable caller values and composes each
graph-supplied lineage and exact work time into a `FrameCacheKey` before lookup or insertion.
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

`disk::DiskCacheConfig` validates a caller-selected root and per-entry byte bound.
`DiskCacheValueSchema` gives one caller-owned value family a stable identifier, revision, and digest,
while `DiskCacheCodec<V>` owns canonical tier-aware encoding and decoding. `FrameDiskCache<V>`
creates a format and schema namespace, exposes ordered diagnostics, and returns a
`FrameDiskCacheScope` for one `FrameDiskCacheContext`. That scope implements the same graph
`EvaluationValueCache<V>` seam without requiring cloneable values or propagating storage failure
into evaluation.

`eviction` exposes `CacheBudgetLimit`, `CacheMemoryBudgets`, `CacheCost`, `CacheBudgetUsage`,
`CacheBudgetStats`, `CacheBudgetManager`, and non-cloneable `CacheBudgetReservation`. Host admission
charges total and project scopes. Device admission also charges one `DeviceId` and holds a real
`superi_gpu::pool::MemoryReservation` in `MemoryClass::Cache` for the same managed bytes.

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
still returns and retention is skipped. Clearing or dropping the cache releases every counter.

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

Render reuse composes an owned outer context with the graph snapshot's inspected target identity
before dispatch. A foreground evaluation checks memory first, then persistent storage, promoting an
exact disk hit to memory, and finally runs the real graph evaluator through a layered adapter that
writes successful values to both tiers. Background work uses the same path through a staged adapter:
lookups remain real, but new final and intermediate inserts remain private until the worker reaches
its final cooperative checkpoint. Exact frame keys single-flight within one queue, and an RAII
active-work guard removes ownership after success, error, cancellation, timeout, or panic.

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
  node catalog, playback, export, or GPU value path invokes the render queue. Engine generation and
  substitution are the cross-crate derived-media consumers.

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
- Driver allocation overhead and physical residency remain outside managed-payload accounting.
  Invalidation invocation, generation scheduling, cache management, and prefetch remain later
  concerns. Cross-quality substitution is scheduler-owned and engine-integrated.
- Persistent storage has no automatic cleanup, relocation, cross-process single-flight lock, or
  default root or capacity. Callers own the directory lifecycle and value schema evolution.

## Tests and verification

`cache_key_contract.rs` runs five integration contracts through real graph inspection and image
pipeline construction. It proves media ID and fingerprint invalidation, graph and parameter edits,
source interpretation, named space, ICC payload, stage order, display versus delivery intent,
physical time equality, time changes, precision and proxy-quality separation, media-set order and
deduplication, invalid fingerprint rejection, domain separation, a locked digest vector, and
`Send + Sync` key values.

`frame_memory_cache_contract.rs` runs seven contracts through the real graph cached-evaluation path.
It proves exact final reuse, final and intermediate tier separation, an intermediate hit that skips
upstream work, fresh results after graph state changes, fresh results after outer render identity
changes, deterministic composite key order, exact host bounds, automatic replacement and fresh
recomputation under pressure, independent ownership of equal results by project, device accounting
and automatic GPU-only pressure replacement in the shared GPU cache class, explicit release, and a
thread-safe cache type.
`frame_disk_cache_contract.rs` runs seven contracts through the same graph path. It proves reuse
after cache reconstruction and eight concurrent reads, separate final and intermediate persistence,
real upstream pruning, digest and semantic corruption quarantine followed by exact recomputation,
terminal decoder byte retention, truncated and oversized input bounds, future format isolation,
value-schema revision separation, and all required failure classifications with actionable context.
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

`proxy_generation_contract.rs` runs three contracts over the public media-neutral generation
surface. It proves changes to source ID, content, revision, purpose, quality, or render settings
change the key; exact complete artifacts reuse; regeneration replaces one exact key; freshness is
strict; failed generation retains prior complete media; empty payloads fail; and a missing or stale
request returns the original source.

`render_cache_contract.rs` runs five contracts through real editable graph snapshots, the graph
  evaluation cache seam, the memory and disk implementations, GPU cache accounting, and the bounded
  worker pool. It proves layered promotion, exact quality identity, owned device placement and
  release, bounded background-to-foreground reuse, active duplicate rejection, queued and running
  cancellation with terminal active-key cleanup and no new publication, and noncacheable rejection
  before dispatch.

Focused package tests, graph, concurrency, dependency, and engine contract closure,
warnings-denied Clippy, and rustdoc pass
after the integrated budget, LRU, invalidation, persistence, and generation implementation. The 32
cache integration tests plus five render contracts across eight files prove bounded retained
evaluator values, shared managed
GPU accounting, exact eviction, precise revision-safe cleanup, process-restart reuse, corruption
recovery, and complete-only derived publication through public adapters. The engine contract adds
real AV1 packet generation and packet-backed transparent substitution with strict fallback. These
tests do not prove playback-clock integration, muxing, or physical GPU residency.

## Current status and risks

Deterministic composite key derivation, exact cached color metadata, hierarchical budgets, budgeted
two-tier retained evaluation, priority-aware strict per-tier LRU eviction, precise revision-safe
edit invalidation, versioned two-tier disk persistence, proxy or optimized-media identity and
atomic publication, layered render reuse, and bounded background population are substantive and
test-backed. Final
hits stop the complete graph pull, intermediate hits stop their prerequisite subtree, hard caps
remain exact, automatic or explicit victims recompute with unchanged result meaning, edit plans
remove only affected older lineage, and disk corruption falls back to exact fresh evaluation.
Device entries share the existing GPU cache class, and eviction or invalidation releases matching
reservations outside the cache state lock. Background jobs single-flight exact frame keys within
one immutable snapshot and outer render context, stage newly evaluated values until final job
checks pass, and expose deterministic active-work inspection. The engine generates real complete
AV1 packet artifacts through the proxy module and selects exact or lower-quality fresh proxies
through the scheduler. Prefetch remains a placeholder, and cache itself owns no quality-selection
policy. No production frame pixels are selected or reclaimed by playback, export encoding, or GPU
upload.

The main correctness risk is each caller's canonical parameter and render-settings encoding.
Omitting one output-affecting byte can cause false reuse even though composition and storage are
stable. Future schema changes must bump the affected domain rather than silently changing a key
contract. The main budget risk is inaccurate caller-owned managed-payload cost. Separate queues or
foreground callers may still perform duplicate deterministic work because single-flight ownership
is queue-local. Concurrent lock acquisition defines the observed access order,
which may change cache contents but cannot change semantic results. Explicit victim selection sorts
the retained tier during pressure handling, so measured production scale may justify an equivalent
indexed implementation later. Derived-media cleanup, persistence, and management are absent, so the
catalog remains externally synchronized and has no single-flight generation, eviction, persistence,
or management. Generated quality must match the caller-prepared frames and canonical engine
settings; substitution admits only exact request identity and revision before scheduler choice.
Persistent value callers must version codec meaning correctly, choose bounded entry sizes, and
provide cleanup and relocation policy. Independently opened cache instances can duplicate
deterministic work and last-writer publication, but immutable complete keys keep the published
meaning equal. The crate is not yet a complete cache subsystem.

## Maintenance notes

Keep stored values keyed by `FrameCacheKey`; do not fall back to the narrower graph digest alone.
Define and test canonical parameter and render-settings encodings at their owning orchestration
boundaries. Cached values should be cloneable resource handles, such as `Arc`-backed frame or GPU
wrappers, rather than deep copies of texture contents. Preserve separate final and intermediate
ownership and the invariant that every retained value owns its exact reservation. Cost functions
must report managed payload bytes rather than driver or allocator estimates. Eviction must release
entries before retrying the same serialized admission path and must not hold the state lock while GPU
pressure cooperators execute. Preserve the single-source access stamp, shared invalidation lock,
and rule that no affected value may be admitted below its graph and node revision fence. Keep these
boundaries when adding disk lifecycle policy, production playback consumers, and broader
diagnostics in their assigned checkpoints. Never weaken the disk envelope bounds, complete identity
checks, schema
partition, synchronization sequence, corruption revalidation, or fallback-to-fresh behavior.
Derived-media producers must compute complete output identity before publication, never mutate
source state, and keep media or codec dependencies above this crate.
