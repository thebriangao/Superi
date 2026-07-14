---
module_id: superi-cache
source_paths:
  - open/crates/superi-cache
source_hash: 07097a7e5edb45a0ecfe6d2f4a5afa11a4ae9062909bbb7b86cee123f9fbb9ed
source_files: 8
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-cache` reserves ownership for final-frame and intermediate caches, proxy media, background render caching, prefetch, eviction, and persistent storage. It currently contains only the planned module boundaries and no cache implementation.

## Source inventory

- `open/crates/superi-cache/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`, `superi-image`, and `superi-graph`.
- `open/crates/superi-cache/src/disk.rs`: Placeholder for persistent on-disk caching.
- `open/crates/superi-cache/src/eviction.rs`: Placeholder for eviction and memory-budget policy.
- `open/crates/superi-cache/src/frame.rs`: Placeholder for final-frame and intermediate-node output caching.
- `open/crates/superi-cache/src/lib.rs`: Documents the intended cache subsystem and exports six placeholder modules.
- `open/crates/superi-cache/src/prefetch.rs`: Placeholder for playback prediction and prefetch.
- `open/crates/superi-cache/src/proxy.rs`: Placeholder for proxy or optimized-media generation and substitution.
- `open/crates/superi-cache/src/render.rs`: Placeholder for render and background-render caching.

## Public surface

The library publicly exports `disk`, `eviction`, `frame`, `prefetch`, `proxy`, and `render`. These modules contain no public items or behavior.

## Architecture and data flow

There is no cache key, stored value, lookup, population, invalidation, persistence, proxy substitution, or prefetch flow. The manifest establishes only the intended ability to use core identities, GPU and image resources, and graph outputs. No source imports those dependencies.

## Dependencies and consumers

- Declared dependencies are `superi-core`, `superi-gpu`, `superi-image`, and `superi-graph`. They are unused in source.
- `superi-engine` declares `superi-cache` as a dependency, but its implemented paths do not reference a `superi_cache` item.
- No other workspace crate declares or uses `superi-cache`.

## Invariants and operational boundaries

- Cargo keeps cache above graph, GPU, image, and core and below engine.
- GPU residency, cache determinism, resource budgets, invalidation, persistence integrity, and proxy equivalence are intended concerns only.
- There is no disk format, cache directory, locking policy, cleanup operation, or capacity configuration.

## Tests and verification

The crate owns no tests, examples, or benchmarks. Compilation proves only that its empty public modules and declared dependencies are accepted.

## Current status and risks

All Rust files are explicit placeholders. No cache is consumed by playback, graph evaluation, export, or the implemented GPU upload path. Treating the crate as operational would hide missing memory, correctness, and persistence behavior.

## Maintenance notes

When cache behavior is added, map the exact key and invalidation model, GPU ownership, memory accounting, disk lifecycle, proxy substitution rules, concurrency, and engine consumers. Add verification for eviction, corruption, stale entries, deterministic reuse, and pressure behavior before describing those as invariants.
