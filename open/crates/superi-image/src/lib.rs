//! `superi-image`, OIIO/EXR-equivalent image data model, CPU pixel ops, and I/O.
//!
//! Section 5.4 in `docs/architecture.md`. Channel and nested layer naming is
//! implemented; pixel storage, operations, metadata, tiling, and I/O remain
//! separate modules.

pub mod channels;
pub mod io;
pub mod metadata;
pub mod model;
pub mod ops;
pub mod tiling;
