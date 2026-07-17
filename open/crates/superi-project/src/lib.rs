//! `superi-project`, coherent editable whole-project state.
//!
//! Section 5.12 in `docs/architecture.md`. The in-memory document aggregate,
//! immutable snapshots, atomic revision-fenced edits, timeline compilations,
//! named standalone graphs, durable project settings and authored clip-mix state,
//! opaque extension-state envelopes, stable project serialization, checked schema migration, atomic save
//! publication, portable referenced-media paths, MediaId-keyed relink commands, and
//! deterministic project autosave scheduling and retention are implemented. Crash
//! recovery discovery, complete semantic comparison, restoration, and durable
//! dismissal consume that same authoritative store.

pub mod autosave;
pub mod document;
pub mod extensions;
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
pub use recovery::{
    ProjectRecoveryCandidate, ProjectRecoveryCandidateId, ProjectRecoveryCatalog,
    ProjectRecoveryComparison, ProjectRecoveryController, ProjectRecoveryFinding,
    ProjectRecoveryNextAction,
};
pub use save::{
    ProjectDestinationCollision, ProjectSaveCommand, ProjectSaveOperation, ProjectSaveOutcome,
};
