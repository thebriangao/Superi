//! `superi-image`, the high-bit-depth image representation and processing substrate.
//!
//! Dense image values preserve exact integer and IEEE floating-point samples,
//! ordered channel and nested-layer identity, alpha and color interpretation,
//! typed metadata, and separate signed data and display windows. Storage,
//! semantic image values, and metadata remain separate composable contracts;
//! CPU operations, tiled access, and still-image I/O build on this foundation.

pub mod channels;
pub mod io;
pub mod metadata;
pub mod model;
pub mod ops;
pub mod tiling;
pub mod value;
