//! `superi-gpu`, the wgpu pipeline and GPU-resident resource substrate.
//!
//! Adapter discovery, capability selection, logical-device ownership, native
//! viewport surfaces, device-scoped resource management, canonical WGSL
//! validation and reflection, bounded shader caching, aligned texture reuse,
//! decoded-frame upload, and exact pixel conversion share one wgpu path.
//! Managed handles retain dependency ownership and reject resources from
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
pub mod texture_pool;
pub mod upload;

pub use wgpu;

#[cfg(test)]
mod conversion_contract;
#[cfg(test)]
mod resource_contract;
#[cfg(test)]
mod upload_contract;
