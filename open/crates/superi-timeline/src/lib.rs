//! `superi-timeline`, Rust-native timeline + OTIO-compat serde + edit-ops + compile-to-graph.
//!
//! § 5.8 in `docs/architecture.md`. Depends on: superi-core, superi-graph. Status: skeleton.

pub mod compile;
pub mod edit_ops;
pub mod markers;
pub mod model;
pub mod multicam;
pub mod nested;
pub mod otio;
