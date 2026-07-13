//! `superi-gpu`, wgpu pipeline and GPU-resident frames.
//!
//! Adapter discovery, capability selection, and logical-device ownership are
//! implemented. Resource, pass, conversion, pooling, and readback modules are
//! still staged behind their focused checkpoints.

pub mod buffer;
pub mod convert;
pub mod device;
pub mod pass;
pub mod pool;
pub mod readback;
pub mod shader;
pub mod texture;
pub mod upload;
