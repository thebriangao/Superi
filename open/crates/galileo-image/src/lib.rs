//! `galileo-image`, OIIO/EXR-equivalent image data model + CPU pixel ops + IO.
//!
//! § 5.4 in `docs/architecture.md`. Depends on: galileo-core. Status: skeleton.

pub mod channels;
pub mod io;
pub mod metadata;
pub mod model;
pub mod ops;
pub mod tiling;
