//! `superi-timeline`, Rust-native editorial state and staged interchange surfaces.
//!
//! Section 5.8 in `docs/architecture.md`. Foundational project and timeline objects, typed track
//! semantics, and atomic validated edits are implemented. OTIO interchange, advanced edit
//! operations, and compile-to-graph remain staged.

pub mod compile;
pub mod edit_ops;
pub mod ids;
pub mod markers;
pub mod model;
pub mod multicam;
pub mod nested;
pub mod otio;
