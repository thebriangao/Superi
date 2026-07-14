---
module_id: superi-cache
source_paths:
  - open/crates/superi-cache
source_hash: d57f9f3cbaba18120550bcdff8e628dc35e699ccae63d63c7b8bebea689b25b7
source_files: 8
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-cache` reserves ownership for final-frame and intermediate caches, proxy media, background render caching, prefetch, eviction, and persistent storage. It now owns the exact color-pipeline identity attached to a cached frame, while storage, lookup, eviction, persistence, and cache generation remain placeholders.

## Source inventory

- `open/crates/superi-cache/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`, `superi-image`, and `superi-graph`.
- `open/crates/superi-cache/src/disk.rs`: Placeholder for persistent on-disk caching.
- `open/crates/superi-cache/src/eviction.rs`: Placeholder for eviction and memory-budget policy.
- `open/crates/superi-cache/src/frame.rs`: Defines immutable `CachedFrameColorMetadata`, which clones a graph color pipeline, compares the complete pipeline for cache identity, and exposes the retained metadata.
- `open/crates/superi-cache/src/lib.rs`: Documents the intended cache subsystem and exports six placeholder modules.
- `open/crates/superi-cache/src/prefetch.rs`: Placeholder for playback prediction and prefetch.
- `open/crates/superi-cache/src/proxy.rs`: Placeholder for proxy or optimized-media generation and substitution.
- `open/crates/superi-cache/src/render.rs`: Placeholder for render and background-render caching.

## Public surface

The library publicly exports `disk`, `eviction`, `frame`, `prefetch`, `proxy`, and `render`. Only `frame` has substantive behavior: `CachedFrameColorMetadata::from_graph`, `matches`, and `pipeline` preserve exact source tags, ordered transforms, and output intent as part of cached-frame identity.

## Architecture and data flow

There is no stored value, lookup, population, invalidation, persistence, proxy substitution, or prefetch flow. The implemented metadata seam copies `GraphColorMetadata` into a cache-owned immutable value and requires complete `ColorPipelineMetadata` equality for a match. It does not allocate GPU resources or claim a usable cache.

## Dependencies and consumers

- Declared dependencies are `superi-core`, `superi-gpu`, `superi-image`, and `superi-graph`. The frame metadata contract directly consumes image and graph; core and GPU remain unused in source.
- `superi-engine::render` consumes `CachedFrameColorMetadata` when deriving independent viewport and export color branches.
- No other workspace crate declares or uses `superi-cache`.

## Invariants and operational boundaries

- Cargo keeps cache above graph, GPU, image, and core and below engine.
- Complete color-pipeline equality is required before cached color metadata matches a request. GPU residency, resource budgets, invalidation, persistence integrity, and proxy equivalence remain intended concerns only.
- There is no disk format, cache directory, locking policy, cleanup operation, or capacity configuration.

## Tests and verification

The crate owns no direct tests, examples, or benchmarks. The engine color propagation integration contract exercises the cache metadata seam, including a mismatch after one creative-stage identity changes. Compilation alone does not prove cache storage or reuse.

## Current status and risks

`frame.rs` is a narrow substantive metadata boundary; the other cache modules remain explicit placeholders. Engine viewport and export metadata consume it, but no frame pixels are stored or reused by playback, graph evaluation, export encoding, or GPU upload. Treating the crate as operational would hide missing memory, correctness, generation, and persistence behavior.

## Maintenance notes

When cache behavior is added, map the exact key and invalidation model, GPU ownership, memory accounting, disk lifecycle, proxy substitution rules, concurrency, and engine consumers. Add verification for eviction, corruption, stale entries, deterministic reuse, and pressure behavior before describing those as invariants.
