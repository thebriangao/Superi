//! `galileo-project`, project/document model + persistence + autosave/recovery.
//!
//! § 5.12 in `docs/architecture.md`. Depends on: galileo-core, galileo-graph, galileo-timeline. Status: skeleton.

pub mod autosave;
pub mod document;
pub mod persist;
pub mod recovery;
