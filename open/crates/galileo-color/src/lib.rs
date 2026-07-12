//! `galileo-color`, color ops as graph nodes + OCIO-equivalent config; linear 16f working space.
//!
//! § 5.3 in `docs/architecture.md`. Depends on: galileo-core, galileo-gpu, galileo-image, galileo-graph. Status: skeleton.

pub mod config;
pub mod hdr;
pub mod icc;
pub mod lut;
pub mod transform_in;
pub mod transform_out;
pub mod view;
pub mod working_space;
