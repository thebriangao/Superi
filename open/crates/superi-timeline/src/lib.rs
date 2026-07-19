//! `superi-timeline`, Rust-native editorial state and staged interchange surfaces.
//!
//! Section 5.8 in `docs/architecture.md`. Foundational project and timeline objects, typed track
//! semantics, authoritative edit intent, atomic validated edits, and foundational editorial
//! operations, nesting, retiming, media organization, multicam state, deterministic
//! compile-to-graph state, and OTIO 0.18.1 interchange are implemented. Graph evaluation remains
//! staged.

pub mod compile;
pub mod edit_ops;
pub mod edit_state;
pub mod ids;
pub mod marker_ops;
pub mod markers;
pub mod media;
pub mod model;
pub mod multicam;
pub mod nested;
pub mod otio;
pub mod retime;
pub mod serialize;
pub mod track_ops;
