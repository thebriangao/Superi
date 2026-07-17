//! `superi-project`, coherent editable whole-project state.
//!
//! Section 5.12 in `docs/architecture.md`. The in-memory document aggregate,
//! immutable snapshots, atomic revision-fenced edits, timeline compilations,
//! named standalone graphs, stable schema-1 project serialization, checked
//! schema migration, atomic save publication, portable referenced-media paths,
//! and MediaId-keyed relink commands are implemented. Autosave and recovery
//! remain staged in their dedicated modules.

pub mod autosave;
pub mod document;
pub mod media;
mod migrate;
pub mod persist;
pub mod recovery;
mod save;

pub use media::MEDIA_PATH_TARGET_FORMAT;
pub use persist::{
    ProjectDatabase, PROJECT_APPLICATION_ID, PROJECT_FORMAT, PROJECT_FORMAT_VERSION,
    PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION, PROJECT_SCHEMA_REVISION,
};
pub use save::{
    ProjectDestinationCollision, ProjectSaveCommand, ProjectSaveOperation, ProjectSaveOutcome,
};
