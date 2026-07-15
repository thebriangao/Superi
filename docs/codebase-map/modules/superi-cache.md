---
module_id: superi-cache
source_paths:
  - open/crates/superi-cache
source_hash: bc71c47165b800ca6c5227b865019f08929197b1b0366f8ece0bf9640daf018f
source_files: 10
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-cache` owns deterministic reusable-result identity plus the planned final-frame,
intermediate, proxy, background-render, prefetch, eviction, and persistent-cache systems. The
implemented key layer composes identities owned by media, graph, parameter, image-color, time, and
render-setting boundaries without taking ownership of those source models. Exact cached color
metadata is also implemented, while value storage, lookup, invalidation, eviction, persistence,
proxy generation, and prefetch remain placeholders.

## Source inventory

- `open/crates/superi-cache/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`,
  `superi-image`, `superi-graph`, and the pinned workspace `sha2` package.
- `open/crates/superi-cache/src/disk.rs`: Placeholder for persistent on-disk caching.
- `open/crates/superi-cache/src/eviction.rs`: Placeholder for eviction and memory-budget policy.
- `open/crates/superi-cache/src/frame.rs`: Defines immutable `CachedFrameColorMetadata`, which
  clones a graph color pipeline, compares the complete pipeline for cache identity, and exposes the
  retained metadata.
- `open/crates/superi-cache/src/key.rs`: Defines cache-local media identity, domain-separated
  parameter and render-settings fingerprints, complete color-pipeline fingerprinting, physical-time
  canonicalization, and the composite frame or intermediate-output cache key.
- `open/crates/superi-cache/src/lib.rs`: Documents the cache subsystem and exports the seven owned
  modules.
- `open/crates/superi-cache/src/prefetch.rs`: Placeholder for playback prediction and prefetch.
- `open/crates/superi-cache/src/proxy.rs`: Placeholder for proxy or optimized-media generation and
  substitution.
- `open/crates/superi-cache/src/render.rs`: Placeholder for render and background-render caching.
- `open/crates/superi-cache/tests/cache_key_contract.rs`: Proves every required invalidation axis,
  canonical media sets and physical time, complete color meaning and order, proxy and precision
  separation, stable vectors, invalid media input, domain separation, and thread-safe public values.

## Public surface

The library publicly exports `disk`, `eviction`, `frame`, `key`, `prefetch`, `proxy`, and `render`.
`CachedFrameColorMetadata::from_graph`, `matches`, and `pipeline` preserve the exact image-owned
pipeline beside cached values.

The `key` module exposes:

- `MediaCacheIdentity`, which combines a core `MediaId` with a domain-separated digest of the exact
  nonempty backend content fingerprint.
- `ParameterStateFingerprint` and `RenderSettingsFingerprint`, distinct SHA-256 identities over
  caller-owned canonical bytes. Both can also wrap an existing digest from the same contract.
- `ColorPipelineFingerprint`, which encodes source interpretation, named-space text, ICC bytes,
  current and working spaces, every transform in order, and display or delivery intent.
- `FrameCacheKeyInputs` and `FrameCacheKey`, which compose the media set, authoritative graph
  `EvaluationCacheKey`, parameter state, color state, exact physical time, and render settings into
  one versioned digest with stable byte inspection and display.

## Architecture and data flow

At orchestration, an authoritative media source supplies `MediaId` and content fingerprint without
creating a forbidden cache-to-media-I/O dependency. Graph inspection supplies its existing
evaluation key, the parameter and render owners supply complete canonical byte encodings, image
metadata supplies the full color pipeline, and core supplies rational time. `FrameCacheKey::derive`
sorts and deduplicates exact media identities, hashes every typed category in fixed order, reduces
time to its exact physical numerator and denominator, and emits one domain-separated SHA-256 key.

Graph topology, behavior, endpoint, state, request scope, and upstream lineage remain owned by
`superi-graph::diagnostics::EvaluationCacheKey`. Parameter and render fingerprint constructors do
not claim that arbitrary caller bytes are canonical. The color path uses image-owned permanent
codes and exact payload bytes rather than Rust `Hash`, enum discriminants, or debug formatting.

There is still no stored value, lookup, population, invalidation transaction, persistence, proxy
substitution, eviction, or prefetch flow. Key derivation allocates only a small temporary media
identity vector and does not allocate GPU resources or claim operational cache reuse.

## Dependencies and consumers

- Declared internal dependencies remain `superi-core`, `superi-gpu`, `superi-image`, and
  `superi-graph`; `sha2` is the only direct external dependency. Key derivation consumes core IDs
  and time, graph evaluation identity, and image color metadata. GPU remains reserved but unused in
  source.
- `superi-engine::render` consumes `CachedFrameColorMetadata` when deriving independent viewport
  and export color branches.
- Future engine cache and proxy coordinators are the intended consumers of `FrameCacheKey`; current
  source proves the public contract through the cache integration test rather than claiming a
  storage consumer that does not exist.

## Invariants and operational boundaries

- Cargo keeps cache above graph, GPU, image, and core and below engine.
- Equal result-affecting inputs produce equal keys, while changes to media identity or content,
  graph lineage, parameter state, color meaning or order, physical time, precision, quality, proxy
  policy, or other canonical render settings change identity.
- Equivalent rational coordinates share one physical-time identity. Media traversal order and exact
  duplicate references do not perturb a key; graph ordering remains represented by the graph key.
- Parameter and render settings are separate domains even when their canonical payload bytes are
  equal. Media content, color pipelines, and the composite key also use separate versioned domains.
- Complete color-pipeline equality is required before cached color metadata matches a request.
  Color fingerprints retain exact source tags, ICC bytes, ordered transforms, current and working
  spaces, and terminal intent.
- GPU residency, resource budgets, invalidation, persistence integrity, and proxy equivalence remain
  intended concerns only.
- There is no disk format, cache directory, locking policy, cleanup operation, or capacity
  configuration.

## Tests and verification

`cache_key_contract.rs` runs five integration contracts through real graph inspection and image
pipeline construction. It proves media ID and fingerprint invalidation, graph and parameter edits,
source interpretation, named space, ICC payload, stage order, display versus delivery intent,
physical time equality, time changes, precision and proxy-quality separation, media-set order and
deduplication, invalid fingerprint rejection, domain separation, a locked digest vector, and
`Send + Sync` values.

The existing engine color-propagation integration contract exercises the cached metadata seam and
independent viewport and export branches. Focused package tests and warnings-denied Clippy passed
for both cache and image after this implementation. These tests prove identity derivation, not
cache hits, stored pixels, eviction, persistence, or playback acceleration.

## Current status and risks

Deterministic composite key derivation and exact cached color metadata are substantive. Disk,
eviction, prefetch, proxy, and render modules remain explicit placeholders, and no frame pixels or
intermediate values are stored or reused by playback, graph evaluation, export encoding, or GPU
upload.

The main correctness risk moves to each caller's canonical parameter and render-settings encoding.
Omitting one output-affecting byte can cause false reuse even though this composition is stable.
Future schema changes must bump the affected domain rather than silently changing a persisted key
contract. SHA-256 collision resistance is assumed; keys are identities for replaceable derived data,
not authenticity or security proofs. Treating the crate as an operational cache would still hide
missing storage, invalidation, memory, generation, and persistence behavior.

## Maintenance notes

When cache storage is added, consume `FrameCacheKey` rather than building another semantic identity.
Define and test the exact canonical parameter and render-settings encodings at their owning
orchestration boundaries. Map invalidation generations, GPU ownership, memory accounting, disk
lifecycle, proxy substitution rules, concurrency, and engine consumers. Add verification for real
hits, false-hit prevention, eviction, corruption, stale entries, deterministic reuse, and pressure
behavior before describing those as implemented invariants.
