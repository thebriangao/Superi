//! `superi-project`, project/document model + persistence + autosave/recovery.
//!
//! § 5.12 in `docs/architecture.md`. Depends on: superi-core, superi-graph, superi-timeline. Status: skeleton.

pub mod autosave;
pub mod document;
pub mod persist;
pub mod recovery;
