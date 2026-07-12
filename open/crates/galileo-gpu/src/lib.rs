//! `galileo-gpu`, wgpu pipeline, GPU-resident frames.
//!
//! § 5.2 in `docs/architecture.md`. Depends on: galileo-core, galileo-image. Status: skeleton.

pub mod buffer;
pub mod convert;
pub mod device;
pub mod pass;
pub mod pool;
pub mod readback;
pub mod shader;
pub mod texture;
pub mod upload;
