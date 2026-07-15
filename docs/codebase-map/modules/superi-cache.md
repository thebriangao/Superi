---
module_id: superi-cache
source_paths:
  - open/crates/superi-cache
source_hash: 3c4af6a5b133bfbae7d7487cee129ccbe07b32696796c51f9ddad148e07fd140
source_files: 12
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-cache` owns deterministic reusable-result identity, budgeted final-frame and
intermediate-node memory retention, exact global, project, and device cache accounting, and the
planned proxy, background-render, prefetch, eviction-order, and persistent-cache systems. Composite
keys join identities owned by media, graph, parameter, image-color, time, and render-setting
boundaries without taking ownership of those source models. Two independent thread-safe memory
tiers retain cloneable evaluator values only while exact byte and frame reservations remain live,
and exact cached color metadata remains implemented beside the retained-value path.

## Source inventory

- `open/crates/superi-cache/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`,
  `superi-image`, `superi-graph`, and the pinned workspace `sha2` package.
- `open/crates/superi-cache/src/disk.rs`: Placeholder for persistent on-disk caching.
- `open/crates/superi-cache/src/eviction.rs`: Defines checked global, per-project, and per-device
  byte and frame limits, exact managed-payload costs and usage, serialized admission, immutable
  diagnostics, classified refusal, host reservations, composite GPU reservations, and RAII release.
- `open/crates/superi-cache/src/frame.rs`: Defines budgeted `FrameMemoryCache<V>`, its scoped graph
  adapter, complete caller-owned cache and placement context, independent final-frame and
  intermediate-node stores, deterministic key inventories, exact cost callback, host or device
  admission, explicit clearing, scoped locking, poison recovery, and caller-handle cloning outside
  locks. It also defines immutable `CachedFrameColorMetadata` over exact graph color metadata.
- `open/crates/superi-cache/src/key.rs`: Defines cache-local media identity, domain-separated
  parameter and render-settings fingerprints, complete color-pipeline fingerprinting, physical-time
  canonicalization, and the composite frame or intermediate-output cache key.
- `open/crates/superi-cache/src/lib.rs`: Documents implemented composite identity, budgeted memory
  retention, and hierarchical accounting and exports the seven owned modules.
- `open/crates/superi-cache/src/prefetch.rs`: Placeholder for playback prediction and prefetch.
- `open/crates/superi-cache/src/proxy.rs`: Placeholder for proxy or optimized-media generation and
  substitution.
- `open/crates/superi-cache/src/render.rs`: Placeholder for render and background-render caching.
- `open/crates/superi-cache/tests/cache_key_contract.rs`: Proves every required invalidation axis,
  canonical media sets and physical time, complete color meaning and order, proxy and precision
  separation, stable vectors, invalid media input, domain separation, and thread-safe key values.
- `open/crates/superi-cache/tests/frame_memory_cache_contract.rs`: Proves concrete final-frame
  reuse, intermediate-node subtree pruning, graph-state and outer render-context freshness,
  deterministic composite key inventory, separate tier counts, bounded host retention, semantic
  equality after admission refusal, device retention through the shared GPU cache class, release,
  and the `Send + Sync` public memory-cache boundary.
- `open/crates/superi-cache/tests/memory_budget_contract.rs`: Proves checked configuration, exact
  byte and frame admission, global, project, and device isolation, deterministic classified
  refusal, usage and peak diagnostics, GPU rollback, RAII cleanup, and concurrent hard-limit safety.

## Public surface

The library publicly exports `disk`, `eviction`, `frame`, `key`, `prefetch`, `proxy`, and `render`.
The `key` module exposes `MediaCacheIdentity`, `ParameterStateFingerprint`,
`RenderSettingsFingerprint`, `ColorPipelineFingerprint`, `FrameCacheKeyInputs`, and
`FrameCacheKey`. Together they compose media content, graph lineage, evaluated parameters, exact
color meaning, physical time, and render settings into one versioned SHA-256 digest.

`frame::FrameMemoryCache<V>` creates empty final and intermediate tiers, reports each tier's entry
count, returns each tier's `FrameCacheKey` inventory in deterministic order, publishes one
consistent budget snapshot, and clears both tiers explicitly. Construction requires a shared
`CacheBudgetManager` and exact caller-owned value-cost function. `FrameMemoryCacheContext` binds one
project, host or device placement, media, parameter, color, and render identity for one evaluation.
`FrameMemoryCache::scope` returns `FrameMemoryCacheScope`, which implements
`superi_graph::eval::EvaluationValueCache<V>` for cloneable caller values and composes each
graph-supplied lineage and exact work time into a `FrameCacheKey` before lookup or insertion.
`CachedFrameColorMetadata::from_graph`, `matches`, and `pipeline` preserve exact source tags,
ordered transforms, and output intent.

`eviction` exposes `CacheBudgetLimit`, `CacheMemoryBudgets`, `CacheCost`, `CacheBudgetUsage`,
`CacheBudgetStats`, `CacheBudgetManager`, and non-cloneable `CacheBudgetReservation`. Host admission
charges total and project scopes. Device admission also charges one `DeviceId` and holds a real
`superi_gpu::pool::MemoryReservation` in `MemoryClass::Cache` for the same managed bytes.

## Architecture and data flow

At orchestration, authoritative owners provide one project, host or device placement, media
identities, canonical parameter bytes, color pipeline state, and canonical render-setting bytes to
one `FrameMemoryCacheContext`. Graph cached evaluation supplies `EvaluationCacheIdentity` values
containing its deterministic lineage digest and exact endpoint, physical time, and region. The
scoped adapter derives one `FrameCacheKey` per graph work unit, checks the matching final or
intermediate tier, and returns an owned clone on a hit.

`LazyEvaluator::evaluate_with_cache` checks the final key before node execution and recursively
stops at retained intermediate values. On a miss it executes through the ordinary immutable graph
contract, then offers only successful work whose graph inspection produced an available identity.
`FrameMemoryCache<V>` stores final-frame and intermediate-node values in separate ordered tiers.
Every entry is partitioned by project and host or device ownership, then pairs `Arc<V>` with one
non-cloneable budget reservation. Lookup clones the retained `Arc` while holding the tier lock,
releases the guard, and only then clones the caller-defined value handle. Insertion computes exact
cost without a lock, releases any replaced entry, performs hierarchical and optional GPU admission
without a tier lock, and publishes only an admitted entry.
A refusal leaves the fresh evaluator result unchanged and stores nothing. Clearing or dropping the
cache drops entries and releases every cache and GPU counter.

`CacheBudgetManager` serializes local admission through one state lock. It preflights total bytes,
total frames, project bytes, project frames, device bytes, and device frames in deterministic order
before counters change. Device admission releases that lock before the shared GPU pool is called;
a GPU refusal drops the provisional local reservation and rolls back all cache scopes. Immutable
stats copy current and independent peak usage plus active scope, reservation, and denial counts.

Media order and duplicates are canonicalized, physical time is reduced exactly, and all identity
categories use stable domain-separated digests. The existing metadata seam independently copies
`GraphColorMetadata` into a cache-owned immutable value and requires complete
`ColorPipelineMetadata` equality for a match.

## Dependencies and consumers

- Declared internal dependencies are `superi-core`, `superi-gpu`, `superi-image`, and
  `superi-graph`; `sha2` is the only direct external dependency. Composite identity consumes core
  IDs and time, graph lineage and work identity, and image color metadata. Budget policy consumes
  core `ProjectId` and `DeviceId` plus the GPU pool's `MemoryClass::Cache`, pressure cooperators,
  and RAII reservation boundary.
- `superi-graph` owns evaluation, cacheability decisions, graph lineage, and the node-neutral
  retained-value adapter. It does not depend upward on concrete storage, memory policy, or outer
  render identity.
- `superi-engine::render` consumes `CachedFrameColorMetadata` when deriving independent viewport
  and export color branches.
- Cache integration contracts are the first concrete consumers of retained graph evaluation and
  budgeted GPU cache memory. No production engine node catalog, playback, render, export, or GPU
  value path uses `FrameMemoryCache` yet.

## Invariants and operational boundaries

- Cargo keeps cache above graph, GPU, image, and core and below engine.
- Equal result-affecting inputs produce equal composite keys. Changes to media identity or content,
  graph lineage, parameter state, color meaning or order, physical time, precision, quality, proxy
  policy, or other canonical render settings change identity.
- Only graph work with an available deterministic cache identity reaches memory storage.
  Policy-disabled, nondeterministic, and dependency-blocked work bypasses both tiers.
- Final-frame and intermediate-node entries never share one tier. Both maps use ordered composite
  keys, and no node evaluation, caller-defined clone, value-cost function, or GPU cooperation runs
  while a tier lock is held.
- Equivalent rational coordinates share one physical-time identity. Media traversal order and exact
  duplicate references do not perturb a key; graph ordering remains represented by the graph key.
- Parameter and render settings are separate domains even when their canonical payload bytes are
  equal. Media content, color pipelines, and the composite key also use separate versioned domains.
- Cache lock poisoning is recovered because each locked operation is an internally atomic ordered
  map lookup or insertion and cache failure cannot change an otherwise valid evaluation result.
- Every live retained entry owns one exact nonzero byte and frame charge. Total, per-project, and
  per-device usage can never exceed configured hard limits, including under concurrent admission.
- Device values cannot enter a tier unless both the cache hierarchy and shared GPU pool admit the
  same managed payload bytes. No cache tier lock is held while GPU pressure cooperators run.
- Admission refusal is retryable resource exhaustion for explicit callers and an ordinary skipped
  insertion through graph's storage-neutral adapter. It never changes the freshly evaluated value,
  key identity, project meaning, or final output.
- Active project and device records are removed when their last reservation drops, so long sessions
  cannot accumulate historical scope state. Peak usage remains a bounded scalar diagnostic.
- Complete color-pipeline equality is required before cached color metadata matches a request.
- Driver allocation overhead and physical residency remain outside managed-payload accounting.
  Explicit invalidation cleanup, generation ownership, victim selection, persistence integrity,
  cache management, proxy substitution, and prefetch remain later concerns.
- There is no disk format, cache directory, automatic cleanup operation, or default capacity.

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
changes, deterministic composite key order, exact host bounds, repeated fresh evaluation after
denied retention, independent ownership of equal results by project, device accounting in the
shared GPU cache class, explicit release, and a thread-safe cache type.
`memory_budget_contract.rs` runs five contracts for configuration,
hierarchical host and device admission, deterministic errors and stats, GPU rollback, cleanup, and
eight-way concurrent admission. The graph package adds adapter, failure, disabled-policy, and
role-neutral snapshot proof. The engine color propagation contract continues to exercise the
metadata seam and independent viewport and export branches.

Focused package tests and warnings-denied Clippy pass after the budgeted store integration. These
tests prove bounded retained evaluator values and shared managed GPU accounting through public
adapters, not production pixels, playback acceleration, physical residency, eviction order, or
persistence.

## Current status and risks

Deterministic composite key derivation, exact cached color metadata, exact hierarchical memory
budgets, and budgeted two-tier retained evaluation are substantive and test-backed. Final hits stop
the complete graph pull and intermediate hits stop their prerequisite subtree while preserving
ordinary result meaning. Hard caps remain exact under concurrent load, and device entries share the
existing GPU cache class. Disk, prefetch, proxy, and render modules remain explicit placeholders;
`eviction.rs` implements admission and release but not victim order. No production frame pixels are
reused by playback, export encoding, or GPU upload.

The main identity risk is each caller's canonical parameter and render-settings encoding. Omitting
one output-affecting byte can cause false reuse even though composition and storage are stable.
Future schema changes must bump the affected domain rather than silently changing a key contract.
The main budget risk is inaccurate caller-owned managed-payload cost. Concurrent identical misses
may perform duplicate deterministic work and reservations because single-flight coordination is not
implemented, although every transient reservation remains within hard caps. A full cache refuses
new retention until entries are replaced, cleared, invalidated, or later eviction policy selects
victims. Eviction order, generation cleanup, persistence, and management are absent, so the crate is
not yet a complete cache subsystem.

## Maintenance notes

Keep stored values keyed by `FrameCacheKey`; do not fall back to the narrower graph digest alone.
Define and test canonical parameter and render-settings encodings at their owning orchestration
boundaries. Cached values should be cloneable resource handles, such as `Arc`-backed frame or GPU
wrappers, rather than deep copies of texture contents. Preserve separate final and intermediate
ownership and the invariant that every retained value owns its exact reservation. Cost functions
must report managed payload bytes rather than driver or allocator estimates. Later eviction must
release entries before retrying the same serialized admission path and must not hold a tier lock
while GPU pressure cooperators execute. Preserve this boundary when adding explicit invalidation,
disk lifecycle, proxy substitution, engine consumers, and broader diagnostics in their assigned
checkpoints.
