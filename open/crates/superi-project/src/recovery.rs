//! Crash recovery discovery, comparison, and durable dismissal.
//!
//! Recovery consumes the exact autosave namespace owned by [`crate::autosave`].
//! Paths and raw diagnostics remain inside the project layer. Callers select an
//! opaque generation identity, then this owner revalidates the exact regular
//! file before opening, comparing, restoring, or dismissing it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use superi_core::diagnostics::FailureDiagnostic;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{GraphId, ProjectId};

use crate::autosave::{artifact_file_name, parse_artifact_file_name, project_directory_name};
use crate::document::{ProjectGraph, ProjectSnapshot};
use crate::ProjectDatabase;

const COMPONENT: &str = "superi-project.recovery";
const TOMBSTONE_PREFIX: &str = ".autosave-dismissed-g";
const TOMBSTONE_SUFFIX: &str = ".superi";
const TOMBSTONE_DIGITS: usize = 20;

/// Opaque stable identity for one exact managed autosave generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectRecoveryCandidateId(u64);

impl ProjectRecoveryCandidateId {
    /// Creates an identity for one positive C009 generation.
    pub fn new(generation: u64) -> Result<Self> {
        if generation == 0 {
            return Err(recovery_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_candidate_identity",
                "project recovery generation must be greater than zero",
            ));
        }
        Ok(Self(generation))
    }

    /// Returns the managed generation represented by this identity.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ProjectRecoveryCandidateId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "recovery-g{:020}", self.0)
    }
}

/// One valid, fully loaded recovery point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecoveryCandidate {
    id: ProjectRecoveryCandidateId,
    project_revision: u64,
}

impl ProjectRecoveryCandidate {
    /// Returns the opaque managed candidate identity.
    #[must_use]
    pub const fn id(&self) -> ProjectRecoveryCandidateId {
        self.id
    }

    /// Returns the document revision captured in the autosave.
    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }
}

/// Stable next action derived from one classified recovery finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectRecoveryNextAction {
    /// Retry discovery because the same operation may now succeed.
    RetryDiscovery,
    /// Continue with valid candidates while this candidate remains unavailable.
    ContinueWithOtherCandidate,
    /// Correct permissions, project identity, or unsafe recovery-store state.
    CorrectRecoveryStore,
    /// Restart into a fresh application lifetime before continuing.
    RestartApplication,
}

impl ProjectRecoveryNextAction {
    /// Returns the permanent automation code for this action.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::RetryDiscovery => "retry_discovery",
            Self::ContinueWithOtherCandidate => "continue_with_other_candidate",
            Self::CorrectRecoveryStore => "correct_recovery_store",
            Self::RestartApplication => "restart_application",
        }
    }
}

/// One managed entry that could not become a valid recovery candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecoveryFinding {
    candidate_id: ProjectRecoveryCandidateId,
    diagnostic: FailureDiagnostic,
    next_action: ProjectRecoveryNextAction,
}

impl ProjectRecoveryFinding {
    /// Returns the opaque identity of the affected managed entry.
    #[must_use]
    pub const fn candidate_id(&self) -> ProjectRecoveryCandidateId {
        self.candidate_id
    }

    /// Returns complete internal diagnostic evidence.
    ///
    /// This value may include paths and source details. It must not cross a
    /// user-facing API without a separate safe projection.
    #[must_use]
    pub const fn diagnostic(&self) -> &FailureDiagnostic {
        &self.diagnostic
    }

    /// Returns the stable next action for this classification.
    #[must_use]
    pub const fn next_action(&self) -> ProjectRecoveryNextAction {
        self.next_action
    }
}

/// Complete replacement catalog for one project's managed recovery namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecoveryCatalog {
    revision: u64,
    project_id: ProjectId,
    candidates: Vec<ProjectRecoveryCandidate>,
    findings: Vec<ProjectRecoveryFinding>,
}

impl ProjectRecoveryCatalog {
    /// Returns the monotonic catalog revision for optimistic commands.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the project permanently bound to this catalog.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Returns valid candidates in ascending managed-generation order.
    #[must_use]
    pub fn candidates(&self) -> &[ProjectRecoveryCandidate] {
        &self.candidates
    }

    /// Returns classified entry findings in ascending generation order.
    #[must_use]
    pub fn findings(&self) -> &[ProjectRecoveryFinding] {
        &self.findings
    }
}

/// Typed semantic comparison between current project state and one candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecoveryComparison {
    candidate_id: ProjectRecoveryCandidateId,
    current_revision: u64,
    candidate_revision: u64,
    editorial_changed: bool,
    settings_changed: bool,
    clip_mix_changed: bool,
    root_timeline_changed: bool,
    changed_graph_count: usize,
    current_graph_count: usize,
    candidate_graph_count: usize,
}

impl ProjectRecoveryComparison {
    #[must_use]
    pub const fn candidate_id(&self) -> ProjectRecoveryCandidateId {
        self.candidate_id
    }

    #[must_use]
    pub const fn current_revision(&self) -> u64 {
        self.current_revision
    }

    #[must_use]
    pub const fn candidate_revision(&self) -> u64 {
        self.candidate_revision
    }

    #[must_use]
    pub const fn editorial_changed(&self) -> bool {
        self.editorial_changed
    }

    #[must_use]
    pub const fn settings_changed(&self) -> bool {
        self.settings_changed
    }

    #[must_use]
    pub const fn clip_mix_changed(&self) -> bool {
        self.clip_mix_changed
    }

    #[must_use]
    pub const fn root_timeline_changed(&self) -> bool {
        self.root_timeline_changed
    }

    #[must_use]
    pub const fn changed_graph_count(&self) -> usize {
        self.changed_graph_count
    }

    #[must_use]
    pub const fn current_graph_count(&self) -> usize {
        self.current_graph_count
    }

    #[must_use]
    pub const fn candidate_graph_count(&self) -> usize {
        self.candidate_graph_count
    }

    /// Reports whether restoring the candidate would change semantic project state.
    #[must_use]
    pub const fn has_changes(&self) -> bool {
        self.editorial_changed
            || self.settings_changed
            || self.clip_mix_changed
            || self.root_timeline_changed
            || self.changed_graph_count > 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileIdentity {
    length: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl FileIdentity {
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ManagedRecoveryEntry {
    path: PathBuf,
    identity: FileIdentity,
}

/// Exclusive recovery owner for one project and one C009 recovery root.
#[derive(Debug)]
pub struct ProjectRecoveryController {
    project_id: ProjectId,
    project_directory: PathBuf,
    catalog: ProjectRecoveryCatalog,
    entries: BTreeMap<ProjectRecoveryCandidateId, ManagedRecoveryEntry>,
}

impl ProjectRecoveryController {
    /// Creates a controller rooted in one explicit existing local directory.
    pub fn new(project_id: ProjectId, recovery_root: impl AsRef<Path>) -> Result<Self> {
        let recovery_root = fs::canonicalize(recovery_root.as_ref()).map_err(|source| {
            filesystem_error(source, "canonicalize_recovery_root", recovery_root.as_ref())
        })?;
        validate_real_directory(&recovery_root, "validate_recovery_root")?;
        let project_directory = recovery_root.join(project_directory_name(project_id));
        Ok(Self {
            project_id,
            project_directory,
            catalog: ProjectRecoveryCatalog {
                revision: 0,
                project_id,
                candidates: Vec::new(),
                findings: Vec::new(),
            },
            entries: BTreeMap::new(),
        })
    }

    /// Returns the latest published catalog without touching the filesystem.
    #[must_use]
    pub const fn catalog(&self) -> &ProjectRecoveryCatalog {
        &self.catalog
    }

    /// Discovers exact published autosaves and safely classifies bad entries.
    pub fn discover(&mut self) -> Result<&ProjectRecoveryCatalog> {
        let metadata = match fs::symlink_metadata(&self.project_directory) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                self.publish_catalog(Vec::new(), Vec::new(), BTreeMap::new())?;
                return Ok(&self.catalog);
            }
            Err(source) => {
                return Err(filesystem_error(
                    source,
                    "inspect_project_directory",
                    &self.project_directory,
                ));
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(recovery_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "validate_project_directory",
                "project recovery namespace must be a real directory",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "validate_project_directory")
                    .with_field("path", self.project_directory.display().to_string()),
            ));
        }

        let mut directory_entries = fs::read_dir(&self.project_directory)
            .map_err(|source| {
                filesystem_error(source, "scan_project_directory", &self.project_directory)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| {
                filesystem_error(
                    source,
                    "read_project_directory_entry",
                    &self.project_directory,
                )
            })?;
        directory_entries.sort_by_key(|entry| entry.file_name());

        let mut candidates = Vec::new();
        let mut findings = Vec::new();
        let mut entries = BTreeMap::new();
        for entry in directory_entries {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let path = entry.path();
            if let Some(generation) = parse_tombstone_file_name(name) {
                let candidate_id = ProjectRecoveryCandidateId(generation);
                let metadata = fs::symlink_metadata(&path).map_err(|source| {
                    filesystem_error(source, "inspect_dismissal_tombstone", &path)
                })?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    findings.push(finding_from_error(
                        candidate_id,
                        &path,
                        recovery_error(
                            ErrorCategory::Conflict,
                            Recoverability::UserCorrectable,
                            "inspect_dismissal_tombstone",
                            "project recovery dismissal tombstone is not a regular file",
                        ),
                    ));
                } else if let Err(source) = fs::remove_file(&path) {
                    findings.push(dismissal_cleanup_finding(
                        candidate_id,
                        &path,
                        filesystem_error(source, "cleanup_dismissal_tombstone", &path),
                    ));
                } else if let Err(error) =
                    sync_directory(&self.project_directory, "sync_tombstone_cleanup")
                {
                    findings.push(dismissal_cleanup_finding(candidate_id, &path, error));
                }
                continue;
            }
            let Some(generation) = parse_artifact_file_name(name) else {
                continue;
            };
            let candidate_id = ProjectRecoveryCandidateId(generation);
            let metadata = fs::symlink_metadata(&path)
                .map_err(|source| filesystem_error(source, "inspect_recovery_candidate", &path))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                findings.push(finding_from_error(
                    candidate_id,
                    &path,
                    recovery_error(
                        ErrorCategory::Conflict,
                        Recoverability::UserCorrectable,
                        "inspect_recovery_candidate",
                        "managed recovery candidate is not a regular file",
                    ),
                ));
                continue;
            }
            entries.insert(
                candidate_id,
                ManagedRecoveryEntry {
                    path: path.clone(),
                    identity: FileIdentity::from_metadata(&metadata),
                },
            );
            match load_project_candidate(&path) {
                Ok(snapshot) if snapshot.project_id() == self.project_id => {
                    candidates.push(ProjectRecoveryCandidate {
                        id: candidate_id,
                        project_revision: snapshot.revision(),
                    });
                }
                Ok(snapshot) => findings.push(finding_from_error(
                    candidate_id,
                    &path,
                    recovery_error(
                        ErrorCategory::Conflict,
                        Recoverability::UserCorrectable,
                        "validate_candidate_project",
                        "recovery candidate belongs to another project",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "validate_candidate_project")
                            .with_field("project_id", self.project_id.to_string())
                            .with_field("candidate_project_id", snapshot.project_id().to_string()),
                    ),
                )),
                Err(error) => findings.push(finding_from_error(candidate_id, &path, error)),
            }
        }
        candidates.sort_by_key(ProjectRecoveryCandidate::id);
        findings.sort_by_key(ProjectRecoveryFinding::candidate_id);
        self.publish_catalog(candidates, findings, entries)?;
        Ok(&self.catalog)
    }

    /// Compares one exact candidate with the caller's current project snapshot.
    pub fn compare(
        &self,
        expected_catalog_revision: u64,
        candidate_id: ProjectRecoveryCandidateId,
        current: &ProjectSnapshot,
    ) -> Result<ProjectRecoveryComparison> {
        self.require_catalog_revision(expected_catalog_revision)?;
        self.require_project(current)?;
        let candidate = self.load_candidate(candidate_id)?;

        let current_graphs = current
            .graphs()
            .map(|graph| (graph.graph_id(), graph))
            .collect::<BTreeMap<GraphId, &ProjectGraph>>();
        let candidate_graphs = candidate
            .graphs()
            .map(|graph| (graph.graph_id(), graph))
            .collect::<BTreeMap<GraphId, &ProjectGraph>>();
        let graph_ids = current_graphs
            .keys()
            .chain(candidate_graphs.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        let changed_graph_count = graph_ids
            .iter()
            .filter(|graph_id| current_graphs.get(graph_id) != candidate_graphs.get(graph_id))
            .count();

        Ok(ProjectRecoveryComparison {
            candidate_id,
            current_revision: current.revision(),
            candidate_revision: candidate.revision(),
            editorial_changed: current.editorial_project() != candidate.editorial_project(),
            settings_changed: current.settings() != candidate.settings(),
            clip_mix_changed: current.clip_mix_state() != candidate.clip_mix_state(),
            root_timeline_changed: current.root_timeline_id() != candidate.root_timeline_id(),
            changed_graph_count,
            current_graph_count: current_graphs.len(),
            candidate_graph_count: candidate_graphs.len(),
        })
    }

    /// Loads one exact candidate for the engine-owned restore transaction.
    pub fn load_candidate_for_restore(
        &self,
        expected_catalog_revision: u64,
        candidate_id: ProjectRecoveryCandidateId,
        current: &ProjectSnapshot,
    ) -> Result<ProjectSnapshot> {
        self.require_catalog_revision(expected_catalog_revision)?;
        self.require_project(current)?;
        self.load_candidate(candidate_id)
    }

    /// Durably dismisses one exact regular managed entry through a tombstone transition.
    pub fn dismiss(
        &mut self,
        expected_catalog_revision: u64,
        candidate_id: ProjectRecoveryCandidateId,
    ) -> Result<&ProjectRecoveryCatalog> {
        self.require_catalog_revision(expected_catalog_revision)?;
        let next_revision = self.catalog.revision.checked_add(1).ok_or_else(|| {
            recovery_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "advance_catalog_revision",
                "project recovery catalog revision is exhausted",
            )
        })?;
        let entry = self.revalidated_entry(candidate_id)?.clone();
        let tombstone = self
            .project_directory
            .join(tombstone_file_name(candidate_id.generation()));
        match fs::symlink_metadata(&tombstone) {
            Ok(_) => {
                return Err(recovery_error(
                    ErrorCategory::Conflict,
                    Recoverability::Retryable,
                    "reserve_dismissal_tombstone",
                    "project recovery dismissal tombstone already exists; discover again",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "reserve_dismissal_tombstone")
                        .with_field("candidate_id", candidate_id.to_string())
                        .with_field("path", tombstone.display().to_string()),
                ));
            }
            Err(source) if source.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(filesystem_error(
                    source,
                    "reserve_dismissal_tombstone",
                    &tombstone,
                ));
            }
        }

        fs::rename(&entry.path, &tombstone).map_err(|source| {
            filesystem_error(source, "publish_dismissal_tombstone", &entry.path)
        })?;
        sync_directory(&self.project_directory, "sync_dismissal_transition").map_err(
            |mut error| {
                error.push_context(
                    ErrorContext::new(COMPONENT, "dismiss_candidate")
                        .with_field("candidate_id", candidate_id.to_string())
                        .with_field("dismissal_state", "tombstoned"),
                );
                error
            },
        )?;
        let cleanup_finding = match fs::remove_file(&tombstone) {
            Ok(()) => sync_directory(&self.project_directory, "sync_dismissal_cleanup")
                .err()
                .map(|error| dismissal_cleanup_finding(candidate_id, &tombstone, error)),
            Err(source) => Some(dismissal_cleanup_finding(
                candidate_id,
                &tombstone,
                filesystem_error(source, "cleanup_dismissal_tombstone", &tombstone),
            )),
        };

        self.entries.remove(&candidate_id);
        self.catalog
            .candidates
            .retain(|candidate| candidate.id() != candidate_id);
        self.catalog
            .findings
            .retain(|finding| finding.candidate_id() != candidate_id);
        if let Some(finding) = cleanup_finding {
            self.catalog.findings.push(finding);
            self.catalog
                .findings
                .sort_by_key(ProjectRecoveryFinding::candidate_id);
        }
        self.catalog.revision = next_revision;
        Ok(&self.catalog)
    }

    fn publish_catalog(
        &mut self,
        candidates: Vec<ProjectRecoveryCandidate>,
        findings: Vec<ProjectRecoveryFinding>,
        entries: BTreeMap<ProjectRecoveryCandidateId, ManagedRecoveryEntry>,
    ) -> Result<()> {
        let changed = self.catalog.candidates != candidates
            || self.catalog.findings != findings
            || self.entries != entries;
        let revision = if changed {
            self.catalog.revision.checked_add(1).ok_or_else(|| {
                recovery_error(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::Terminal,
                    "advance_catalog_revision",
                    "project recovery catalog revision is exhausted",
                )
            })?
        } else {
            self.catalog.revision
        };
        self.catalog = ProjectRecoveryCatalog {
            revision,
            project_id: self.project_id,
            candidates,
            findings,
        };
        self.entries = entries;
        Ok(())
    }

    fn require_catalog_revision(&self, expected_revision: u64) -> Result<()> {
        if expected_revision == self.catalog.revision {
            return Ok(());
        }
        Err(recovery_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "validate_catalog_revision",
            "project recovery catalog revision is stale",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_catalog_revision")
                .with_field("expected_revision", expected_revision.to_string())
                .with_field("actual_revision", self.catalog.revision.to_string()),
        ))
    }

    fn require_project(&self, snapshot: &ProjectSnapshot) -> Result<()> {
        if snapshot.project_id() == self.project_id {
            return Ok(());
        }
        Err(recovery_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "validate_current_project",
            "current project does not match the recovery controller",
        ))
    }

    fn load_candidate(&self, candidate_id: ProjectRecoveryCandidateId) -> Result<ProjectSnapshot> {
        let entry = self.revalidated_entry(candidate_id)?;
        let snapshot = load_project_candidate(&entry.path)?;
        if snapshot.project_id() != self.project_id {
            return Err(recovery_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "validate_candidate_project",
                "recovery candidate belongs to another project",
            ));
        }
        Ok(snapshot)
    }

    fn revalidated_entry(
        &self,
        candidate_id: ProjectRecoveryCandidateId,
    ) -> Result<&ManagedRecoveryEntry> {
        let entry = self.entries.get(&candidate_id).ok_or_else(|| {
            recovery_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "resolve_candidate",
                "project recovery candidate was not found in the current catalog",
            )
        })?;
        let expected_path = self
            .project_directory
            .join(artifact_file_name(candidate_id.generation()));
        if entry.path != expected_path {
            return Err(recovery_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "resolve_candidate",
                "project recovery catalog path invariant failed",
            ));
        }
        let metadata = fs::symlink_metadata(&entry.path)
            .map_err(|source| filesystem_error(source, "reinspect_candidate", &entry.path))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(recovery_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "reinspect_candidate",
                "project recovery candidate is no longer a regular file",
            ));
        }
        if FileIdentity::from_metadata(&metadata) != entry.identity {
            return Err(recovery_error(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "reinspect_candidate",
                "project recovery candidate changed; discover again before continuing",
            ));
        }
        Ok(entry)
    }
}

fn load_project_candidate(path: &Path) -> Result<ProjectSnapshot> {
    let database = ProjectDatabase::open_read_only(path)?;
    database.load().map(|document| document.snapshot())
}

fn finding_from_error(
    candidate_id: ProjectRecoveryCandidateId,
    path: &Path,
    error: Error,
) -> ProjectRecoveryFinding {
    let recoverability = match (error.category(), error.recoverability()) {
        (_, Recoverability::Terminal) => Recoverability::Terminal,
        (_, Recoverability::Retryable) => Recoverability::Retryable,
        (_, Recoverability::Degraded) | (ErrorCategory::CorruptData, _) => Recoverability::Degraded,
        _ => Recoverability::UserCorrectable,
    };
    let next_action = match recoverability {
        Recoverability::Retryable => ProjectRecoveryNextAction::RetryDiscovery,
        Recoverability::Degraded => ProjectRecoveryNextAction::ContinueWithOtherCandidate,
        Recoverability::UserCorrectable => ProjectRecoveryNextAction::CorrectRecoveryStore,
        Recoverability::Terminal => ProjectRecoveryNextAction::RestartApplication,
        _ => ProjectRecoveryNextAction::RestartApplication,
    };
    let category = error.category();
    let wrapped = Error::with_source(
        category,
        recoverability,
        "project recovery candidate could not be used safely",
        error,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "inspect_candidate")
            .with_field("candidate_id", candidate_id.to_string())
            .with_field("generation", candidate_id.generation().to_string())
            .with_field("path", path.display().to_string()),
    );
    ProjectRecoveryFinding {
        candidate_id,
        diagnostic: FailureDiagnostic::from_error(&wrapped),
        next_action,
    }
}

fn dismissal_cleanup_finding(
    candidate_id: ProjectRecoveryCandidateId,
    path: &Path,
    error: Error,
) -> ProjectRecoveryFinding {
    let category = error.category();
    let wrapped = Error::with_source(
        category,
        Recoverability::Degraded,
        "project recovery candidate is dismissed but storage cleanup remains pending",
        error,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "cleanup_dismissed_candidate")
            .with_field("candidate_id", candidate_id.to_string())
            .with_field("generation", candidate_id.generation().to_string())
            .with_field("path", path.display().to_string()),
    );
    ProjectRecoveryFinding {
        candidate_id,
        diagnostic: FailureDiagnostic::from_error(&wrapped),
        next_action: ProjectRecoveryNextAction::ContinueWithOtherCandidate,
    }
}

fn validate_real_directory(path: &Path, operation: &'static str) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).map_err(|source| filesystem_error(source, operation, path))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(recovery_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "project recovery path must be a real directory",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
        ));
    }
    Ok(())
}

fn tombstone_file_name(generation: u64) -> String {
    format!("{TOMBSTONE_PREFIX}{generation:020}{TOMBSTONE_SUFFIX}")
}

fn parse_tombstone_file_name(name: &str) -> Option<u64> {
    let digits = name
        .strip_prefix(TOMBSTONE_PREFIX)?
        .strip_suffix(TOMBSTONE_SUFFIX)?;
    if digits.len() != TOMBSTONE_DIGITS || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let generation = digits.parse::<u64>().ok()?;
    (generation > 0 && tombstone_file_name(generation) == name).then_some(generation)
}

#[cfg(unix)]
fn sync_directory(path: &Path, operation: &'static str) -> Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| filesystem_error(source, operation, path))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path, _operation: &'static str) -> Result<()> {
    Ok(())
}

fn filesystem_error(source: io::Error, operation: &'static str, path: &Path) -> Error {
    let (category, recoverability, message) = match source.kind() {
        io::ErrorKind::NotFound => (
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "project recovery path does not exist",
        ),
        io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
            "project recovery path is not accessible",
        ),
        io::ErrorKind::AlreadyExists => (
            ErrorCategory::Conflict,
            Recoverability::Retryable,
            "project recovery path changed concurrently",
        ),
        io::ErrorKind::OutOfMemory => (
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            "project recovery filesystem resource limit was reached",
        ),
        io::ErrorKind::Unsupported => (
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "filesystem does not support required recovery durability behavior",
        ),
        _ => (
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "project recovery filesystem operation failed",
        ),
    };
    Error::with_source(category, recoverability, message, source).with_context(
        ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
    )
}

fn recovery_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn findings_preserve_retryable_and_terminal_classifications() {
        let id = ProjectRecoveryCandidateId::new(1).unwrap();
        for recoverability in [Recoverability::Retryable, Recoverability::Terminal] {
            let finding = finding_from_error(
                id,
                Path::new("/internal/recovery/path"),
                Error::new(
                    ErrorCategory::Unavailable,
                    recoverability,
                    "classified source",
                ),
            );
            assert_eq!(finding.diagnostic().recoverability(), recoverability);
            assert!(!finding.diagnostic().source_summaries().is_empty());
        }
    }
}
