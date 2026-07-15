//! `superi-project`, coherent editable whole-project state.
//!
//! Section 5.12 in `docs/architecture.md`. The in-memory document aggregate,
//! immutable snapshots, atomic revision-fenced edits, timeline compilations,
//! and named standalone graphs are implemented. Persistence, autosave, and
//! recovery remain staged in their dedicated modules.

pub mod autosave;
pub mod document;
pub mod persist;
pub mod recovery;
