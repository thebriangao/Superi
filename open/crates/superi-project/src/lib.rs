//! `superi-project`, coherent editable whole-project state.
//!
//! Section 5.12 in `docs/architecture.md`. The in-memory document aggregate,
//! immutable snapshots, atomic revision-fenced edits, timeline compilations,
//! named standalone graphs, durable project settings and authored clip-mix state,
//! opaque extension-state envelopes, stable project serialization, checked schema migration, atomic save
//! publication, portable referenced-media paths, MediaId-keyed relink commands, and
//! deterministic project autosave scheduling and retention, read-only whole-project
//! integrity validation and repair reporting, deterministic semantic project hashing with
//! ordered component diagnostics, crash recovery discovery, complete semantic comparison,
//! restoration, and durable dismissal are implemented against that same authoritative store.

pub mod autosave;
pub mod compatibility;
pub mod diagnostics;
pub mod document;
pub mod extensions;
pub mod integrity;
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
pub use compatibility::{
    negotiate_project_format, project_format_support, ProjectFormatIdentity, ProjectFormatRelease,
    ProjectFormatSupport, ProjectVersionDisposition, ProjectVersionNegotiation,
    ProjectVersionReason, PROJECT_APPLICATION_ID, PROJECT_FORMAT, PROJECT_FORMAT_VERSION,
    PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION, PROJECT_SCHEMA_REVISION,
};
pub use diagnostics::{
    ProjectComponentEvidence, ProjectDiagnosticComponent, ProjectDiagnostics, ProjectDigest,
    ProjectGraphScope, PROJECT_HASH_ALGORITHM, PROJECT_HASH_FORMAT_REVISION,
};
pub use integrity::{
    execute_project_integrity_command, ProjectIntegrityCommand, ProjectIntegrityFinding,
    ProjectIntegrityFindingCode, ProjectIntegrityIdentity, ProjectIntegrityReport,
    ProjectIntegrityStage, ProjectIntegrityStatus, ProjectRepairDisposition,
    MAX_PROJECT_INTEGRITY_EVIDENCE_VALUE_BYTES, MAX_PROJECT_INTEGRITY_FINDINGS,
};
pub use media::MEDIA_PATH_TARGET_FORMAT;
pub use persist::ProjectDatabase;
pub use recovery::{
    ProjectRecoveryCandidate, ProjectRecoveryCandidateId, ProjectRecoveryCatalog,
    ProjectRecoveryComparison, ProjectRecoveryController, ProjectRecoveryFinding,
    ProjectRecoveryNextAction,
};
pub use save::{
    ProjectDestinationCollision, ProjectSaveCommand, ProjectSaveOperation, ProjectSaveOutcome,
};
