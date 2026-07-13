//! `superi-gpu`, the wgpu pipeline and GPU-resident resource substrate.
//!
//! Adapter discovery, capability selection, logical-device ownership, native
//! viewport surfaces, device-scoped resource management, canonical WGSL
//! validation and reflection, bounded shader caching, aligned texture reuse,
//! portable memory budgeting and pressure cooperation, decoded-frame upload,
//! exact pixel conversion, ordered compute and render passes, explicit
//! submission, and export or thumbnail readback share one wgpu path. Managed
//! handles retain dependency ownership, in-flight submissions retain reusable
//! allocations through fence completion, and obsolete device lifetimes are
//! rejected after recovery.

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
pub mod submission;
pub mod surface;
pub mod texture;
pub mod texture_pool;
pub mod upload;

pub use wgpu;

#[cfg(test)]
mod conversion_contract;
#[cfg(test)]
mod pass_contract;
#[cfg(test)]
mod resource_contract;
#[cfg(test)]
mod upload_contract;
