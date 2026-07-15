---
module_id: superi-cache
source_paths:
  - open/crates/superi-cache
source_hash: ecbbe01f8190145e246cc3ecb75acdc1311bc2639af0acc60c56a0ec35f119b8
source_files: 11
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-cache` owns deterministic reusable-result identity, final-frame and intermediate-node
memory retention, and the planned proxy, background-render, prefetch, eviction, and persistent-cache
systems. Composite keys join identities owned by media, graph, parameter, image-color, time, and
render-setting boundaries without taking ownership of those source models. Two independent
thread-safe memory tiers retain cloneable evaluator values by those composite identities, and exact
cached color metadata remains implemented beside the retained-value path.

## Source inventory

- `open/crates/superi-cache/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`,
  `superi-image`, `superi-graph`, and the pinned workspace `sha2` package.
- `open/crates/superi-cache/src/disk.rs`: Placeholder for persistent on-disk caching.
- `open/crates/superi-cache/src/eviction.rs`: Placeholder for eviction and memory-budget policy.
- `open/crates/superi-cache/src/frame.rs`: Defines `FrameMemoryCache<V>`, its scoped graph adapter,
  complete caller-owned cache context, independent final-frame and intermediate-node stores,
  deterministic key inventories, scoped locking, poison recovery, and caller-handle cloning outside
  locks. It also defines immutable `CachedFrameColorMetadata` over exact graph color metadata.
- `open/crates/superi-cache/src/key.rs`: Defines cache-local media identity, domain-separated
  parameter and render-settings fingerprints, complete color-pipeline fingerprinting, physical-time
  canonicalization, and the composite frame or intermediate-output cache key.
- `open/crates/superi-cache/src/lib.rs`: Documents implemented composite identity and memory
  retention and exports the seven owned modules.
- `open/crates/superi-cache/src/prefetch.rs`: Placeholder for playback prediction and prefetch.
- `open/crates/superi-cache/src/proxy.rs`: Placeholder for proxy or optimized-media generation and
  substitution.
- `open/crates/superi-cache/src/render.rs`: Placeholder for render and background-render caching.
- `open/crates/superi-cache/tests/cache_key_contract.rs`: Proves every required invalidation axis,
  canonical media sets and physical time, complete color meaning and order, proxy and precision
  separation, stable vectors, invalid media input, domain separation, and thread-safe key values.
- `open/crates/superi-cache/tests/frame_memory_cache_contract.rs`: Proves concrete final-frame
  reuse, intermediate-node subtree pruning, graph-state and outer render-context freshness,
  deterministic composite key inventory, separate tier counts, exact result equality, and the
  `Send + Sync` public memory-cache boundary.

## Public surface

The library publicly exports `disk`, `eviction`, `frame`, `key`, `prefetch`, `proxy`, and `render`.
The `key` module exposes `MediaCacheIdentity`, `ParameterStateFingerprint`,
`RenderSettingsFingerprint`, `ColorPipelineFingerprint`, `FrameCacheKeyInputs`, and
`FrameCacheKey`. Together they compose media content, graph lineage, evaluated parameters, exact
color meaning, physical time, and render settings into one versioned SHA-256 digest.

`frame::FrameMemoryCache<V>` creates empty final and intermediate tiers, reports each tier's entry
count, and returns each tier's `FrameCacheKey` inventory in deterministic order.
`FrameMemoryCacheContext` binds media, parameter, color, and render identity for one evaluation.
`FrameMemoryCache::scope` returns `FrameMemoryCacheScope`, which implements
`superi_graph::eval::EvaluationValueCache<V>` for cloneable caller values and composes each
graph-supplied lineage and exact work time into a `FrameCacheKey` before lookup or insertion.
`CachedFrameColorMetadata::from_graph`, `matches`, and `pipeline` preserve exact source tags,
ordered transforms, and output intent.

## Architecture and data flow

At orchestration, authoritative owners provide media identities, canonical parameter bytes, color
pipeline state, and canonical render-setting bytes to one `FrameMemoryCacheContext`. Graph cached
evaluation supplies `EvaluationCacheIdentity` values containing its deterministic lineage digest
and exact endpoint, physical time, and region. The scoped adapter derives one `FrameCacheKey` per
graph work unit, checks the matching final or intermediate tier, and returns an owned clone on a hit.

`LazyEvaluator::evaluate_with_cache` checks the final key before node execution and recursively
stops at retained intermediate values. On a miss it executes through the ordinary immutable graph
contract, then inserts only successful work whose graph inspection produced an available identity.
`FrameMemoryCache<V>` stores final-frame and intermediate-node values in separate
`Mutex<BTreeMap<FrameCacheKey, Arc<V>>>` tiers. Lookup clones the retained `Arc` while holding the
lock, releases the guard, and only then clones the caller-defined value handle. The cache never
holds a lock while graph evaluation or caller-defined cloning runs.

Media order and duplicates are canonicalized, physical time is reduced exactly, and all identity
categories use stable domain-separated digests. The existing metadata seam independently copies
`GraphColorMetadata` into a cache-owned immutable value and requires complete
`ColorPipelineMetadata` equality for a match.

## Dependencies and consumers

- Declared internal dependencies are `superi-core`, `superi-gpu`, `superi-image`, and
  `superi-graph`; `sha2` is the only direct external dependency. Composite identity consumes core
  IDs and time, graph lineage and work identity, and image color metadata. GPU remains reserved but
  unused in source.
- `superi-graph` owns evaluation, cacheability decisions, graph lineage, and the node-neutral
  retained-value adapter. It does not depend upward on concrete storage or outer render identity.
- `superi-engine::render` consumes `CachedFrameColorMetadata` when deriving independent viewport
  and export color branches.
- Cache integration contracts are the first concrete consumers of retained graph evaluation. No
  production engine node catalog, playback, render, export, or GPU path uses `FrameMemoryCache` yet.

## Invariants and operational boundaries

- Cargo keeps cache above graph, GPU, image, and core and below engine.
- Equal result-affecting inputs produce equal composite keys. Changes to media identity or content,
  graph lineage, parameter state, color meaning or order, physical time, precision, quality, proxy
  policy, or other canonical render settings change identity.
- Only graph work with an available deterministic cache identity reaches memory storage.
  Policy-disabled, nondeterministic, and dependency-blocked work bypasses both tiers.
- Final-frame and intermediate-node entries never share one tier. Both maps use ordered composite
  keys, and no node evaluation or caller-defined clone runs while a cache lock is held.
- Equivalent rational coordinates share one physical-time identity. Media traversal order and exact
  duplicate references do not perturb a key; graph ordering remains represented by the graph key.
- Parameter and render settings are separate domains even when their canonical payload bytes are
  equal. Media content, color pipelines, and the composite key also use separate versioned domains.
- Cache lock poisoning is recovered because each locked operation is an internally atomic ordered
  map lookup or insertion and cache failure cannot change an otherwise valid evaluation result.
- Complete color-pipeline equality is required before cached color metadata matches a request.
- GPU residency, budgets, explicit invalidation cleanup, generation ownership, eviction,
  persistence integrity, cache management, proxy substitution, and prefetch remain later concerns.
- There is no disk format, cache directory, cleanup operation, or capacity configuration.

## Tests and verification

`cache_key_contract.rs` runs five integration contracts through real graph inspection and image
pipeline construction. It proves media ID and fingerprint invalidation, graph and parameter edits,
source interpretation, named space, ICC payload, stage order, display versus delivery intent,
physical time equality, time changes, precision and proxy-quality separation, media-set order and
deduplication, invalid fingerprint rejection, domain separation, a locked digest vector, and
`Send + Sync` key values.

`frame_memory_cache_contract.rs` runs four contracts through the real graph cached-evaluation path.
It proves exact final reuse, final and intermediate tier separation, an intermediate hit that skips
upstream work, fresh results after graph state changes, fresh results after outer render identity
changes, deterministic composite key order, and a thread-safe cache type. The graph package adds
adapter, failure, disabled-policy, and role-neutral snapshot proof. The engine color propagation
contract continues to exercise the metadata seam and independent viewport and export branches.

Focused package tests and warnings-denied Clippy pass for cache and graph after the integrated key
and storage change. These tests prove retained evaluator values through the public adapter, not
production pixels, playback acceleration, eviction, persistence, or GPU residency.

## Current status and risks

Deterministic composite key derivation, exact cached color metadata, and two-tier in-memory retained
evaluation are substantive and test-backed. Final hits stop the complete graph pull and intermediate
hits stop their prerequisite subtree while preserving ordinary result meaning. Disk, eviction,
prefetch, proxy, and render modules remain explicit placeholders. No production frame pixels are
reused by playback, export encoding, or GPU upload.

The main correctness risk is each caller's canonical parameter and render-settings encoding.
Omitting one output-affecting byte can cause false reuse even though composition and storage are
stable. Future schema changes must bump the affected domain rather than silently changing a key
contract. Concurrent identical misses may perform duplicate deterministic work because single-flight
coordination is not implemented. Budgets, eviction, cleanup, persistence, and management are absent,
so the crate is not yet a complete cache subsystem.

## Maintenance notes

Keep stored values keyed by `FrameCacheKey`; do not fall back to the narrower graph digest alone.
Define and test canonical parameter and render-settings encodings at their owning orchestration
boundaries. Cached values should be cloneable resource handles, such as `Arc`-backed frame or GPU
wrappers, rather than deep copies of texture contents. Preserve separate final and intermediate
ownership when adding budgets, eviction, explicit invalidation, disk lifecycle, proxy substitution,
diagnostics, clearing, GPU ownership, and engine consumers in their assigned checkpoints.
