//! `superi-color`, color operations and color-management contracts.
//!
//! The implemented working-space boundary provides canonical premultiplied
//! RGBA binary16 storage in an explicit scene-linear space, with ACEScg as the
//! default and distinct binary32 promotion for numerically sensitive work.
//! Input, output, configuration, HDR, LUT, display, and ICC modules build on
//! that representation without changing its storage contract.

pub mod config;
pub mod hdr;
pub mod icc;
pub mod lut;
pub mod transform_in;
pub mod transform_out;
pub mod view;
pub mod working_space;
