//! `galileo-cache`, frame + intermediate cache, proxy/optimized media.
//!
//! § 5.6 in `docs/architecture.md`. Depends on: galileo-core, galileo-gpu, galileo-image, galileo-graph. Status: skeleton.

pub mod disk;
pub mod eviction;
pub mod frame;
pub mod prefetch;
pub mod proxy;
pub mod render;
