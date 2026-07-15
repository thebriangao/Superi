//! `superi-cache`, frame + intermediate cache, proxy/optimized media.
//!
//! § 5.6 in `docs/architecture.md`. Depends on: superi-core, superi-gpu, superi-image, superi-graph,
//! and superi-concurrency.
//! Status: composite result identity, budgeted final-frame and intermediate-node memory retention,
//! exact global, project, device, and GPU accounting, deterministic priority-aware LRU eviction,
//! precise graph edit invalidation, versioned corruption-recovering disk persistence, media-neutral
//! proxy or optimized-media generation identity and publication, layered render caching, bounded
//! background population, bounded playback prediction, an owned asynchronous host-cache adapter,
//! bounded edit and scrub cache-warming plans, deterministic inspection and clearing, and safe
//! persistent-cache relocation are implemented; broader quality substitution integration remains.

pub mod disk;
pub mod eviction;
pub mod frame;
pub mod key;
pub mod prefetch;
pub mod proxy;
pub mod render;
pub mod warming;
