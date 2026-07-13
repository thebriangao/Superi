//! `superi-image`, the high-bit-depth image representation and processing substrate.
//!
//! Dense image values preserve exact integer and IEEE floating-point samples,
//! ordered channel and nested-layer identity, typed metadata, alpha and color
//! interpretation, and separate signed data and display windows. Storage
//! layouts, semantic image values, and metadata remain separate composable
//! contracts; explicit alpha association and dense CPU operations build on
//! those contracts. Deterministic CPU reference validation makes their output
//! available as a bounded oracle for GPU implementations. Tiled access and
//! still-image I/O build on this foundation.

pub mod alpha;
pub mod channels;
pub mod io;
pub mod metadata;
pub mod model;
pub mod ops;
pub mod reference;
pub mod tiling;
pub mod value;
