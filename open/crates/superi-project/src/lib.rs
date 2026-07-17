//! `superi-project`, coherent editable whole-project state.
//!
//! Section 5.12 in `docs/architecture.md`. The in-memory document aggregate,
//! immutable snapshots, atomic revision-fenced edits, timeline compilations,
//! named standalone graphs, durable project settings, stable schema-2 project
//! serialization, checked schema migration, atomic save publication, portable
//! referenced-media paths, MediaId-keyed relink commands, and deterministic
//! project autosave scheduling and retention are implemented. Recovery remains
//! staged in its dedicated module.

pub mod autosave;
pub mod document;
pub mod media;
mod migrate;
pub mod persist;
pub mod recovery;
mod save;
pub mod settings;

pub use autosave::{
    ProjectAutosaveArtifact, ProjectAutosaveCommand, ProjectAutosaveController,
    ProjectAutosaveDisposition, ProjectAutosaveOperation, ProjectAutosaveOutcome,
    ProjectAutosavePolicy, ProjectAutosaveState, MAX_PROJECT_AUTOSAVE_RETENTION,
};
pub use media::MEDIA_PATH_TARGET_FORMAT;
pub use persist::{
    ProjectDatabase, PROJECT_APPLICATION_ID, PROJECT_FORMAT, PROJECT_FORMAT_VERSION,
    PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION, PROJECT_SCHEMA_REVISION,
};
pub use save::{
    ProjectDestinationCollision, ProjectSaveCommand, ProjectSaveOperation, ProjectSaveOutcome,
};
