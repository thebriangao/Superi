//! `superi-project`, coherent editable whole-project state.
//!
//! Section 5.12 in `docs/architecture.md`. The in-memory document aggregate,
//! immutable snapshots, atomic revision-fenced edits, timeline compilations,
//! named standalone graphs, stable schema-1 project serialization, and checked
//! schema migration are implemented. Autosave and recovery remain staged in
//! their dedicated modules.

pub mod autosave;
pub mod document;
mod migrate;
pub mod persist;
pub mod recovery;

pub use persist::{
    ProjectDatabase, PROJECT_APPLICATION_ID, PROJECT_FORMAT, PROJECT_FORMAT_VERSION,
    PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION, PROJECT_SCHEMA_REVISION,
};
