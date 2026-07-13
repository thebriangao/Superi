//! `superi-image`, the high-bit-depth image representation and processing substrate.
//!
//! Dense image values preserve exact integer and IEEE floating-point samples,
//! ordered channel and nested-layer identity, typed metadata, alpha and color
//! interpretation, and separate signed data and display windows. Storage
//! layouts, semantic image values, and metadata remain separate composable
//! contracts; explicit alpha association and dense CPU operations build on
//! those contracts. Deterministic CPU reference validation makes their output
//! available as a bounded oracle for GPU implementations. Tiled access,
//! still-image I/O, and isolated thumbnail and waveform previews build on this
//! foundation. [`limits::ImageLimits`] gives decoders and image-producing
//! operations one finite policy for dimensions, allocation bytes, channels,
//! layers, metadata, and tiles. Existing operation entry points apply its
//! finite default, while constrained callers can select lower ceilings.

pub mod alpha;
pub mod channels;
pub mod io;
pub mod limits;
pub mod metadata;
pub mod model;
pub mod ops;
pub mod preview;
pub mod reference;
pub mod sequence;
pub mod tiling;
pub mod value;
