//! `superi-cache`, frame + intermediate cache, proxy/optimized media.
//!
//! § 5.6 in `docs/architecture.md`. Depends on: superi-core, superi-gpu, superi-image, superi-graph,
//! and superi-concurrency.
//! Status: composite result identity, budgeted final-frame and intermediate-node memory retention,
//! exact global, project, device, and GPU accounting, deterministic priority-aware LRU eviction,
//! precise graph edit invalidation, versioned corruption-recovering disk persistence, and
//! media-neutral proxy or optimized-media generation identity and publication, layered render
//! caching, and bounded background population are implemented; quality substitution and prefetch
//! remain.

pub mod disk;
pub mod eviction;
pub mod frame;
pub mod key;
pub mod prefetch;
pub mod proxy;
pub mod render;
