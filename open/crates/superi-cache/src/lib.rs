//! `superi-cache`, frame + intermediate cache, proxy/optimized media.
//!
//! § 5.6 in `docs/architecture.md`. Depends on: superi-core, superi-gpu, superi-image, superi-graph.
//! Status: composite result identity plus final-frame and intermediate-node memory retention are
//! implemented; budgets, eviction, persistence, proxies, render caching, and prefetch remain.

pub mod disk;
pub mod eviction;
pub mod frame;
pub mod key;
pub mod prefetch;
pub mod proxy;
pub mod render;
