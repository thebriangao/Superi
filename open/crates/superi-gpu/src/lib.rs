//! `superi-gpu`, the wgpu pipeline and GPU-resident resource substrate.
//!
//! Adapter discovery, capability selection, logical-device ownership, native
//! viewport surfaces, and device-scoped resource management share one wgpu
//! path. Managed handles retain dependency ownership and reject resources from
//! obsolete device lifetimes after recovery.

pub mod binding;
pub mod buffer;
pub mod convert;
pub mod device;
pub mod pass;
pub mod pipeline;
pub mod pool;
pub mod readback;
pub mod resource;
pub mod shader;
pub mod surface;
pub mod texture;
pub mod upload;

pub use wgpu;

#[cfg(test)]
mod resource_contract;
