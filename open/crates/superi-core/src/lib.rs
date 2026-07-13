//! `superi-core`, shared primitives, errors, and identifier/time/pixel types.
//!
//! § 5.12 in `docs/architecture.md`. Depends on: nothing (T0). Status: skeleton.

pub mod color_space;
pub mod diagnostics;
pub mod error;
pub mod geometry;
pub mod ids;
pub mod pixel;
pub mod prelude;
pub mod serialization;
pub mod settings;
pub mod time;
pub mod timecode;
