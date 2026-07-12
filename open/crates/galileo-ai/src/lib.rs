//! `galileo-ai`, local offline inference runtime + scattered-AI pipelines; outputs are editable graph artifacts.
//!
//! § 5.11 in `docs/architecture.md`. Depends on: galileo-core, galileo-image, galileo-graph. Status: skeleton.

pub mod artifacts;
pub mod audit;
pub mod pipelines;
pub mod runtime;
