//! `superi-timeline`, Rust-native timeline + OTIO-compat serde + edit-ops + compile-to-graph.
//!
//! Section 5.8 in `docs/architecture.md`. Depends on: superi-core, superi-graph.
//! Status: typed track semantics and canonical interchange fixtures are implemented;
//! editorial containers, native OTIO serde, edit operations, and compilation remain pending.

pub mod compile;
pub mod edit_ops;
pub mod markers;
pub mod model;
pub mod multicam;
pub mod nested;
pub mod otio;
