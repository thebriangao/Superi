//! `superi-timeline`, Rust-native editorial state and staged interchange surfaces.
//!
//! Section 5.8 in `docs/architecture.md`. Foundational project and timeline objects, typed track
//! semantics, authoritative edit intent, atomic validated edits, and foundational editorial
//! operations are implemented. OTIO interchange, advanced trim operations, and compile-to-graph
//! remain staged.

pub mod compile;
pub mod edit_ops;
pub mod edit_state;
pub mod ids;
pub mod markers;
pub mod media;
pub mod model;
pub mod multicam;
pub mod nested;
pub mod otio;
pub mod retime;
