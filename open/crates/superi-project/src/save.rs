//! Atomic whole-project save publication.

use std::error::Error as StdError;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::document::ProjectSnapshot;
use crate::persist::ProjectDatabase;

const COMPONENT: &str = "superi-project.save";
const CANDIDATE_MARKER: &str = ".superi-save-candidate-";
const MAX_CANDIDATE_ATTEMPTS: u64 = 128;
static NEXT_CANDIDATE: AtomicU64 = AtomicU64::new(0);

#[cfg(unix)]
#[path = "save/unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "save/windows.rs"]
mod platform;

/// Filesystem publication authority granted to the platform adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicationMode {
    /// Replaces one destination already validated as a current project.
    ReplaceExisting,
    /// Publishes only when no destination entry wins the name.
    NoClobber,
}

/// Exact native publication stage used for classified failure context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(windows, allow(dead_code))]
pub(super) enum PublicationStage {
    /// The atomic name operation itself.
    NativePublish,
    /// Parent-directory synchronization after the new name became visible.
    ParentSyncAfterPublish,
    /// Removal of the candidate name after no-clobber hard-link publication.
    CandidateUnlink,
    /// Parent-directory synchronization after candidate-name removal.
    ParentSyncAfterCleanup,
}

impl PublicationStage {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::NativePublish => "native_publish",
            Self::ParentSyncAfterPublish => "parent_sync_after_publish",
            Self::CandidateUnlink => "candidate_unlink",
            Self::ParentSyncAfterCleanup => "parent_sync_after_cleanup",
        }
    }
}

/// Builds one native publication error without losing its platform source.
#[allow(clippy::too_many_arguments)]
pub(super) fn platform_error<E>(
    source: E,
    stage: PublicationStage,
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    native_code: Option<String>,
    candidate: &Path,
    destination: &Path,
) -> Error
where
    E: StdError + Send + Sync + 'static,
{
    let mut context = ErrorContext::new(COMPONENT, "publish_project")
        .with_field("phase", stage.code())
        .with_field("candidate", candidate.display().to_string())
        .with_field("destination", destination.display().to_string());
    if let Some(native_code) = native_code {
        context.insert_field("native_code", native_code);
    }
    Error::with_source(category, recoverability, message, source).with_context(context)
}

/// One stable project-file command surface for UI, script, and headless callers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectFileSession {
    active_path: PathBuf,
    pending_publication: Option<PendingPublication>,
}

impl ProjectFileSession {
    /// Binds a session to one absolute project path without creating the file.
    pub fn new(active_path: impl Into<PathBuf>) -> Result<Self> {
        let active_path = normalize_project_path(active_path.into(), "bind_project_session")?;
        validate_entry_type(&active_path.path, "bind_project_session")?;
        Ok(Self {
            active_path: active_path.path,
            pending_publication: None,
        })
    }

    /// Returns the absolute active path currently owned by this session.
    #[must_use]
    pub fn active_path(&self) -> &Path {
        &self.active_path
    }

    /// Executes one save command through the same durable publication pipeline.
    pub fn execute(&mut self, command: ProjectSaveCommand) -> Result<ProjectSaveOutcome> {
        self.execute_inner(command, |_| Ok(()))
    }

    #[cfg(test)]
    fn execute_with_hook<F>(
        &mut self,
        command: ProjectSaveCommand,
        hook: F,
    ) -> Result<ProjectSaveOutcome>
    where
        F: FnMut(SavePhase) -> Result<()>,
    {
        self.execute_inner(command, hook)
    }

    fn execute_inner<F>(
        &mut self,
        command: ProjectSaveCommand,
        mut hook: F,
    ) -> Result<ProjectSaveOutcome>
    where
        F: FnMut(SavePhase) -> Result<()>,
    {
        if self.pending_publication.is_some() {
            return self.recover_pending(command);
        }

        let active = normalize_project_path(self.active_path.clone(), "validate_active_path")?;
        validate_entry_type(&active.path, "validate_active_path")?;
        self.active_path = active.path.clone();

        let prepare_context = command.prepare_context(&active.path);
        let prepared = self
            .prepare_command(command, &active)
            .map_err(|error| error.with_context(prepare_context))?;
        let kind = prepared.kind;
        let destination = prepared.destination.path.clone();
        let revision = prepared.snapshot.revision();
        let context = |phase: &'static str| {
            command_context(kind, phase, &self.active_path, &destination, revision)
        };

        let (candidate_file, candidate) = reserve_candidate(&prepared.destination)
            .map_err(|error| error.with_context(context("reserve_candidate")))?;
        let candidate_path = candidate.path.clone();
        let mut guard = CandidateCleanup::new(candidate.clone());
        if let Err(error) =
            build_candidate(candidate_file, &candidate, &prepared, &mut hook, &context)
        {
            return Err(guard.cleanup_failure(error));
        }

        guard.disarm();
        let publication =
            platform::publish(&candidate_path, &destination, prepared.publication_mode);
        if let Err(error) = publication {
            let evidence = inspect_publication(
                &candidate_path,
                &destination,
                prepared.publication_mode,
                &prepared.snapshot,
                prepared.previous_snapshot.as_ref(),
            );
            if evidence.state == PublicationState::NotPublished {
                let error = error
                    .with_context(evidence.context())
                    .with_context(context("publish_candidate"));
                return Err(cleanup_after_failure(error, &candidate));
            }
            if prepared.publication_mode == PublicationMode::NoClobber
                && evidence.state == PublicationState::PublishedNotDurable
                && publication_is_owned(&candidate, &destination, evidence)
            {
                self.pending_publication = Some(PendingPublication::from_prepared(
                    &prepared,
                    &active.path,
                    candidate,
                ));
            }
            return Err(error
                .with_context(evidence.context())
                .with_context(context("publish_candidate")));
        }

        let evidence = inspect_publication(
            &candidate_path,
            &destination,
            prepared.publication_mode,
            &prepared.snapshot,
            prepared.previous_snapshot.as_ref(),
        );
        if evidence.state != PublicationState::PublishedNotDurable {
            return Err(project_error(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "verify_publication",
                "project publication completed with unresolved artifact evidence",
            )
            .with_context(evidence.context())
            .with_context(context("verify_publication")));
        }
        if let Err(error) = invoke_hook(&mut hook, SavePhase::AfterPublication) {
            if prepared.publication_mode == PublicationMode::NoClobber
                && publication_is_owned(&candidate, &destination, evidence)
            {
                self.pending_publication = Some(PendingPublication::from_prepared(
                    &prepared,
                    &active.path,
                    candidate,
                ));
            }
            return Err(error
                .with_context(evidence.context())
                .with_context(context("after_publication")));
        }

        if prepared.adopt_destination {
            self.active_path = destination.clone();
        }
        Ok(ProjectSaveOutcome {
            kind,
            destination,
            active_path: self.active_path.clone(),
            revision,
            adopted: prepared.adopt_destination,
            durable: true,
        })
    }

    fn recover_pending(&mut self, command: ProjectSaveCommand) -> Result<ProjectSaveOutcome> {
        let pending = self
            .pending_publication
            .clone()
            .expect("pending recovery was checked before dispatch");
        if self.active_path != pending.active_path || !pending.matches(&command)? {
            return Err(project_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "recover_pending_publication",
                "a different save command cannot replace retained publication evidence",
            )
            .with_context(pending.context("command_mismatch"))
            .with_context(command.prepare_context(&self.active_path)));
        }

        verify_pending_publication(&pending)
            .map_err(|error| error.with_context(pending.context("verify_visible_state")))?;
        platform::recover_visible_no_clobber(&pending.candidate.path, &pending.destination)
            .map_err(|error| error.with_context(pending.context("finish_durability")))?;
        verify_recovered_publication(&pending)
            .map_err(|error| error.with_context(pending.context("verify_recovery")))?;

        self.pending_publication = None;
        if pending.adopt_destination {
            self.active_path = pending.destination.clone();
        }
        Ok(ProjectSaveOutcome {
            kind: pending.kind,
            destination: pending.destination,
            active_path: self.active_path.clone(),
            revision: pending.snapshot.revision(),
            adopted: pending.adopt_destination,
            durable: true,
        })
    }

    fn prepare_command(
        &self,
        command: ProjectSaveCommand,
        active: &NormalizedPath,
    ) -> Result<PreparedCommand> {
        match command {
            ProjectSaveCommand::Save { snapshot } => {
                let destination = normalize_project_path(active.path.clone(), "prepare_save")?;
                let existing = inspect_entry(&destination.path, "prepare_save")?;
                let (publication_mode, previous_snapshot, replacement_permissions) = match existing
                {
                    EntryState::Missing => (PublicationMode::NoClobber, None, None),
                    EntryState::Regular { permissions, .. } => {
                        let previous = load_current_snapshot(&destination.path, "prepare_save")?;
                        (
                            PublicationMode::ReplaceExisting,
                            Some(previous),
                            Some(permissions),
                        )
                    }
                };
                Ok(PreparedCommand {
                    kind: ProjectSaveKind::Save,
                    destination,
                    snapshot,
                    publication_mode,
                    previous_snapshot,
                    replacement_permissions,
                    adopt_destination: false,
                })
            }
            ProjectSaveCommand::SaveAs {
                destination,
                snapshot,
            } => self.prepare_no_clobber(
                ProjectSaveKind::SaveAs,
                destination,
                snapshot,
                active,
                true,
            ),
            ProjectSaveCommand::Copy {
                destination,
                snapshot,
            } => {
                self.prepare_no_clobber(ProjectSaveKind::Copy, destination, snapshot, active, false)
            }
            ProjectSaveCommand::Backup { destination } => {
                let snapshot = load_current_snapshot(&active.path, "prepare_backup")?;
                self.prepare_no_clobber(
                    ProjectSaveKind::Backup,
                    destination,
                    snapshot,
                    active,
                    false,
                )
            }
        }
    }

    fn prepare_no_clobber(
        &self,
        kind: ProjectSaveKind,
        destination: PathBuf,
        snapshot: ProjectSnapshot,
        active: &NormalizedPath,
        adopt_destination: bool,
    ) -> Result<PreparedCommand> {
        let operation = kind.operation();
        let destination = normalize_project_path(destination, operation)?;
        let active_entry = inspect_entry(&active.path, operation)?;
        let destination_entry = inspect_entry(&destination.path, operation)?;
        if paths_alias(
            &active.path,
            &active_entry,
            &destination.path,
            &destination_entry,
        ) {
            return Err(project_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                operation,
                "save destination aliases the active project path",
            )
            .with_context(
                ErrorContext::new(COMPONENT, operation)
                    .with_field("active_path", active.path.display().to_string())
                    .with_field("destination", destination.path.display().to_string()),
            ));
        }
        if destination_entry != EntryState::Missing {
            return Err(project_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                operation,
                "save destination already exists",
            )
            .with_context(
                ErrorContext::new(COMPONENT, operation)
                    .with_field("destination", destination.path.display().to_string()),
            ));
        }
        Ok(PreparedCommand {
            kind,
            destination,
            snapshot,
            publication_mode: PublicationMode::NoClobber,
            previous_snapshot: None,
            replacement_permissions: None,
            adopt_destination,
        })
    }
}

fn build_candidate<F, C>(
    candidate_file: File,
    candidate: &OwnedCandidate,
    prepared: &PreparedCommand,
    hook: &mut F,
    context: &C,
) -> Result<()>
where
    F: FnMut(SavePhase) -> Result<()>,
    C: Fn(&'static str) -> ErrorContext,
{
    let candidate_path = &candidate.path;
    invoke_hook(hook, SavePhase::TempReserved)
        .map_err(|error| error.with_context(context("temp_reserved")))?;

    let mut database =
        ProjectDatabase::create_reserved(candidate_path, candidate_file, |reservation, path| {
            verify_reserved_candidate(reservation, path, candidate)
        })
        .map_err(|error| error.with_context(context("create_candidate")))?;
    database
        .configure_save_candidate()
        .map_err(|error| error.with_context(context("configure_candidate")))?;
    database
        .replace(&prepared.snapshot)
        .map_err(|error| error.with_context(context("write_candidate")))?;
    invoke_hook(hook, SavePhase::SemanticWritten)
        .map_err(|error| error.with_context(context("semantic_written")))?;

    require_exact_snapshot(
        &database
            .load()
            .map_err(|error| error.with_context(context("load_candidate_before_close")))?
            .snapshot(),
        &prepared.snapshot,
        "validate_candidate_before_close",
    )
    .map_err(|error| error.with_context(context("validate_candidate_before_close")))?;
    invoke_hook(hook, SavePhase::CandidateValidated)
        .map_err(|error| error.with_context(context("candidate_validated")))?;

    database
        .close_for_save()
        .map_err(|error| error.with_context(context("close_candidate")))?;
    invoke_hook(hook, SavePhase::DatabaseClosed)
        .map_err(|error| error.with_context(context("database_closed")))?;

    verify_owned_candidate_path(candidate, "reopen_candidate")
        .map_err(|error| error.with_context(context("reopen_candidate")))?;
    let reopened = ProjectDatabase::open_read_only(candidate_path)
        .map_err(|error| error.with_context(context("reopen_candidate")))?;
    verify_owned_candidate_path(candidate, "reopen_candidate")
        .map_err(|error| error.with_context(context("reopen_candidate")))?;
    require_exact_snapshot(
        &reopened
            .load()
            .map_err(|error| error.with_context(context("load_reopened_candidate")))?
            .snapshot(),
        &prepared.snapshot,
        "validate_reopened_candidate",
    )
    .map_err(|error| error.with_context(context("validate_reopened_candidate")))?;
    reopened
        .close_for_save()
        .map_err(|error| error.with_context(context("close_reopened_candidate")))?;
    reject_sidecars(candidate_path)
        .map_err(|error| error.with_context(context("reject_candidate_sidecars")))?;
    invoke_hook(hook, SavePhase::CandidateReopened)
        .map_err(|error| error.with_context(context("candidate_reopened")))?;

    verify_owned_candidate_path(candidate, "open_candidate_for_sync")
        .map_err(|error| error.with_context(context("open_candidate_for_sync")))?;
    let sync_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(candidate_path)
        .map_err(|source| {
            io_error(
                source,
                "open_candidate_for_sync",
                "save candidate could not be opened for synchronization",
                Some(candidate_path),
            )
            .with_context(context("open_candidate_for_sync"))
        })?;
    let sync_identity = file_identity_from_file(&sync_file, candidate_path)
        .map_err(|error| error.with_context(context("open_candidate_for_sync")))?;
    if sync_identity != candidate.identity {
        return Err(ownership_error(
            candidate_path,
            "open_candidate_for_sync",
            "save synchronization handle does not name the reserved candidate",
        )
        .with_context(context("open_candidate_for_sync")));
    }
    if let Some(permissions) = prepared.replacement_permissions.clone() {
        verify_owned_candidate_path(candidate, "copy_destination_permissions")
            .map_err(|error| error.with_context(context("copy_destination_permissions")))?;
        fs::set_permissions(candidate_path, permissions).map_err(|source| {
            io_error(
                source,
                "copy_destination_permissions",
                "save candidate permissions could not be applied",
                Some(candidate_path),
            )
            .with_context(context("copy_destination_permissions"))
        })?;
        verify_owned_candidate_path(candidate, "copy_destination_permissions")
            .map_err(|error| error.with_context(context("copy_destination_permissions")))?;
    }
    sync_file.sync_all().map_err(|source| {
        io_error(
            source,
            "sync_candidate",
            "save candidate could not be synchronized",
            Some(candidate_path),
        )
        .with_context(context("sync_candidate"))
    })?;
    invoke_hook(hook, SavePhase::CandidateSynced)
        .map_err(|error| error.with_context(context("candidate_synced")))?;
    invoke_hook(hook, SavePhase::BeforePublication)
        .map_err(|error| error.with_context(context("before_publication")))?;
    verify_owned_candidate_path(candidate, "before_publication")
        .map_err(|error| error.with_context(context("before_publication")))
}

/// One immutable request executed through [`ProjectFileSession`].
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectSaveCommand {
    /// Publishes one snapshot at the active project path.
    Save {
        /// Complete immutable project state to publish.
        snapshot: ProjectSnapshot,
    },
    /// Publishes at a new path and adopts it only after durable success.
    SaveAs {
        /// Absolute path that must not already exist.
        destination: PathBuf,
        /// Complete immutable project state to publish.
        snapshot: ProjectSnapshot,
    },
    /// Publishes one semantic copy without changing the active path.
    Copy {
        /// Absolute path that must not already exist.
        destination: PathBuf,
        /// Complete immutable project state to publish.
        snapshot: ProjectSnapshot,
    },
    /// Publishes the last durable active project without unsaved state.
    Backup {
        /// Absolute path that must not already exist.
        destination: PathBuf,
    },
}

impl ProjectSaveCommand {
    fn prepare_context(&self, active: &Path) -> ErrorContext {
        let (kind, destination, revision) = match self {
            Self::Save { snapshot } => (
                ProjectSaveKind::Save,
                active,
                snapshot.revision().to_string(),
            ),
            Self::SaveAs {
                destination,
                snapshot,
            } => (
                ProjectSaveKind::SaveAs,
                destination.as_path(),
                snapshot.revision().to_string(),
            ),
            Self::Copy {
                destination,
                snapshot,
            } => (
                ProjectSaveKind::Copy,
                destination.as_path(),
                snapshot.revision().to_string(),
            ),
            Self::Backup { destination } => (
                ProjectSaveKind::Backup,
                destination.as_path(),
                "durable_active".to_owned(),
            ),
        };
        ErrorContext::new(COMPONENT, kind.operation())
            .with_field("phase", "prepare_command")
            .with_field("active_path", active.display().to_string())
            .with_field("destination", destination.display().to_string())
            .with_field("revision", revision)
    }
}

/// Stable semantic kind of one project save command.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ProjectSaveKind {
    /// Save to the active path.
    Save,
    /// Save to and adopt a new path.
    SaveAs,
    /// Copy without adoption.
    Copy,
    /// Backup the last durable active state.
    Backup,
}

impl ProjectSaveKind {
    const fn operation(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::SaveAs => "save_as",
            Self::Copy => "copy",
            Self::Backup => "backup",
        }
    }
}

/// Durable result of one completed project save command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSaveOutcome {
    kind: ProjectSaveKind,
    destination: PathBuf,
    active_path: PathBuf,
    revision: u64,
    adopted: bool,
    durable: bool,
}

impl ProjectSaveOutcome {
    /// Returns the command kind.
    #[must_use]
    pub const fn kind(&self) -> ProjectSaveKind {
        self.kind
    }

    /// Returns the absolute published destination.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// Returns the active path after the command.
    #[must_use]
    pub fn active_path(&self) -> &Path {
        &self.active_path
    }

    /// Returns the saved document revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns whether this command adopted its destination.
    #[must_use]
    pub const fn adopted(&self) -> bool {
        self.adopted
    }

    /// Returns whether publication and required synchronization completed.
    #[must_use]
    pub const fn durable(&self) -> bool {
        self.durable
    }
}

struct PreparedCommand {
    kind: ProjectSaveKind,
    destination: NormalizedPath,
    snapshot: ProjectSnapshot,
    publication_mode: PublicationMode,
    previous_snapshot: Option<ProjectSnapshot>,
    replacement_permissions: Option<Permissions>,
    adopt_destination: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingPublication {
    kind: ProjectSaveKind,
    active_path: PathBuf,
    destination: PathBuf,
    snapshot: ProjectSnapshot,
    candidate: OwnedCandidate,
    adopt_destination: bool,
}

impl PendingPublication {
    fn from_prepared(
        prepared: &PreparedCommand,
        active_path: &Path,
        candidate: OwnedCandidate,
    ) -> Self {
        Self {
            kind: prepared.kind,
            active_path: active_path.to_path_buf(),
            destination: prepared.destination.path.clone(),
            snapshot: prepared.snapshot.clone(),
            candidate,
            adopt_destination: prepared.adopt_destination,
        }
    }

    fn matches(&self, command: &ProjectSaveCommand) -> Result<bool> {
        let (kind, destination, snapshot_matches) = match command {
            ProjectSaveCommand::Save { snapshot } => {
                return Ok(self.kind == ProjectSaveKind::Save && snapshot == &self.snapshot);
            }
            ProjectSaveCommand::SaveAs {
                destination,
                snapshot,
            } => (
                ProjectSaveKind::SaveAs,
                destination,
                snapshot == &self.snapshot,
            ),
            ProjectSaveCommand::Copy {
                destination,
                snapshot,
            } => (
                ProjectSaveKind::Copy,
                destination,
                snapshot == &self.snapshot,
            ),
            ProjectSaveCommand::Backup { destination } => {
                (ProjectSaveKind::Backup, destination, true)
            }
        };
        if kind != self.kind || !snapshot_matches {
            return Ok(false);
        }
        let destination = normalize_project_path(destination.clone(), kind.operation())?;
        Ok(destination.path == self.destination)
    }

    fn context(&self, phase: &'static str) -> ErrorContext {
        command_context(
            self.kind,
            phase,
            &self.active_path,
            &self.destination,
            self.snapshot.revision(),
        )
        .with_field("candidate", self.candidate.path.display().to_string())
    }
}

struct NormalizedPath {
    path: PathBuf,
    parent: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EntryState {
    Missing,
    Regular {
        identity: FileIdentity,
        permissions: Permissions,
    },
}

use platform::FileIdentity;

fn normalize_project_path(path: PathBuf, operation: &'static str) -> Result<NormalizedPath> {
    if !path.is_absolute() {
        return Err(project_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "project file path must be absolute",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
        ));
    }
    let file_name = path.file_name().ok_or_else(|| {
        project_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "project file path must name a file",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
        )
    })?;
    let parent = path.parent().ok_or_else(|| {
        project_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "project file path must have a parent directory",
        )
    })?;
    let parent_metadata = fs::metadata(parent).map_err(|source| {
        io_error(
            source,
            operation,
            "project parent directory is unavailable",
            Some(parent),
        )
    })?;
    if !parent_metadata.is_dir() {
        return Err(project_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "project parent path is not a directory",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("parent", parent.display().to_string()),
        ));
    }
    let parent = fs::canonicalize(parent).map_err(|source| {
        io_error(
            source,
            operation,
            "project parent directory could not be normalized",
            Some(parent),
        )
    })?;
    Ok(NormalizedPath {
        path: parent.join(file_name),
        parent,
    })
}

fn validate_entry_type(path: &Path, operation: &'static str) -> Result<()> {
    inspect_entry(path, operation).map(|_| ())
}

fn inspect_entry(path: &Path, operation: &'static str) -> Result<EntryState> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(EntryState::Missing),
        Err(source) => {
            return Err(io_error(
                source,
                operation,
                "project path metadata could not be inspected",
                Some(path),
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(project_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            operation,
            "project path is a symbolic link",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
        ));
    }
    if !metadata.is_file() {
        return Err(project_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            operation,
            "project path is not a regular file",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
        ));
    }
    let identity = file_identity_from_path(path)?;
    Ok(EntryState::Regular {
        identity,
        permissions: metadata.permissions(),
    })
}

fn file_identity_from_path(path: &Path) -> Result<FileIdentity> {
    platform::identity_from_path(path).map_err(|source| {
        io_error(
            source,
            "inspect_file_identity",
            "project file identity could not be inspected",
            Some(path),
        )
    })
}

fn file_identity_from_file(file: &File, path: &Path) -> Result<FileIdentity> {
    platform::identity_from_file(file).map_err(|source| {
        io_error(
            source,
            "retain_file_identity",
            "reserved project file identity could not be retained",
            Some(path),
        )
    })
}

fn paths_alias(
    first_path: &Path,
    first: &EntryState,
    second_path: &Path,
    second: &EntryState,
) -> bool {
    if first_path == second_path {
        return true;
    }
    match (first, second) {
        (
            EntryState::Regular {
                identity: first, ..
            },
            EntryState::Regular {
                identity: second, ..
            },
        ) => first == second,
        _ => false,
    }
}

fn reserve_candidate(destination: &NormalizedPath) -> Result<(File, OwnedCandidate)> {
    let file_name = destination
        .path
        .file_name()
        .expect("normalized project path has a file name");
    for _ in 0..MAX_CANDIDATE_ATTEMPTS {
        let sequence = NEXT_CANDIDATE.fetch_add(1, Ordering::Relaxed);
        let mut candidate_name = OsString::from(".");
        candidate_name.push(file_name);
        candidate_name.push(format!(
            "{CANDIDATE_MARKER}{}-{sequence}",
            std::process::id()
        ));
        let candidate = destination.parent.join(candidate_name);
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                let identity = match file_identity_from_file(&file, &candidate) {
                    Ok(identity) => identity,
                    Err(error) => {
                        return Err(error.with_context(
                            ErrorContext::new(COMPONENT, "cleanup_candidate")
                                .with_field("candidate", candidate.display().to_string())
                                .with_field("cleanup_state", "retained")
                                .with_field("cleanup_error", "reservation identity unavailable"),
                        ));
                    }
                };
                return Ok((
                    file,
                    OwnedCandidate {
                        path: candidate,
                        identity,
                    },
                ));
            }
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(io_error(
                    source,
                    "reserve_candidate",
                    "save candidate path could not be reserved",
                    Some(&candidate),
                ));
            }
        }
    }
    Err(project_error(
        ErrorCategory::ResourceExhausted,
        Recoverability::Retryable,
        "reserve_candidate",
        "save candidate name attempts were exhausted",
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OwnedCandidate {
    path: PathBuf,
    identity: FileIdentity,
}

struct CandidateCleanup {
    candidate: OwnedCandidate,
    armed: bool,
}

impl CandidateCleanup {
    fn new(candidate: OwnedCandidate) -> Self {
        Self {
            candidate,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    fn cleanup_failure(&mut self, error: Error) -> Error {
        self.armed = false;
        cleanup_after_failure(error, &self.candidate)
    }
}

impl Drop for CandidateCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = cleanup_owned_candidate(&self.candidate);
        }
    }
}

fn cleanup_after_failure(error: Error, candidate: &OwnedCandidate) -> Error {
    let mut cleanup_context = ErrorContext::new(COMPONENT, "cleanup_candidate")
        .with_field("candidate", candidate.path.display().to_string());
    match cleanup_owned_candidate(candidate) {
        Ok(()) => {
            cleanup_context.insert_field("cleanup_state", "removed");
        }
        Err(cleanup_error) => {
            cleanup_context.insert_field("cleanup_state", "retained");
            cleanup_context.insert_field(
                "cleanup_error_category",
                format!("{:?}", cleanup_error.category()),
            );
            cleanup_context.insert_field("cleanup_error", cleanup_error.to_string());
        }
    }
    error.with_context(cleanup_context)
}

fn cleanup_owned_candidate(candidate: &OwnedCandidate) -> Result<()> {
    verify_owned_candidate_path(candidate, "cleanup_candidate")?;
    for sidecar in sidecar_paths(&candidate.path) {
        match fs::symlink_metadata(&sidecar) {
            Err(source) if source.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(project_error(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "cleanup_candidate",
                    "save candidate cleanup preserved an unproven SQLite sidecar",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "cleanup_candidate")
                        .with_field("sidecar", sidecar.display().to_string()),
                ));
            }
            Err(source) => {
                return Err(io_error(
                    source,
                    "cleanup_candidate",
                    "save candidate sidecar state could not be inspected",
                    Some(&sidecar),
                ));
            }
        }
    }
    verify_owned_candidate_path(candidate, "cleanup_candidate")?;
    fs::remove_file(&candidate.path).map_err(|source| {
        io_error(
            source,
            "cleanup_candidate",
            "identity-proven save candidate could not be removed",
            Some(&candidate.path),
        )
    })
}

fn verify_reserved_candidate(
    reservation: &File,
    path: &Path,
    candidate: &OwnedCandidate,
) -> Result<()> {
    let reservation_identity = file_identity_from_file(reservation, path)?;
    if reservation_identity != candidate.identity {
        return Err(ownership_error(
            path,
            "verify_reserved_candidate",
            "reserved project file handle identity changed",
        ));
    }
    verify_owned_candidate_path(candidate, "verify_reserved_candidate")
}

fn verify_owned_candidate_path(candidate: &OwnedCandidate, operation: &'static str) -> Result<()> {
    verify_identity_at_path(&candidate.identity, &candidate.path, operation)
}

fn verify_identity_at_path(
    expected: &FileIdentity,
    path: &Path,
    operation: &'static str,
) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|source| {
        io_error(
            source,
            operation,
            "owned project file path could not be inspected",
            Some(path),
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ownership_error(
            path,
            operation,
            "owned project file path is not a regular file",
        ));
    }
    let actual = file_identity_from_path(path)?;
    if actual != *expected {
        return Err(ownership_error(
            path,
            operation,
            "owned project file path no longer names the reserved file",
        ));
    }
    Ok(())
}

fn ownership_error(path: &Path, operation: &'static str, message: &'static str) -> Error {
    project_error(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        operation,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
    )
}

fn sidecar_paths(path: &Path) -> [PathBuf; 3] {
    ["-journal", "-wal", "-shm"].map(|suffix| {
        let mut value = path.as_os_str().to_os_string();
        value.push(suffix);
        PathBuf::from(value)
    })
}

fn reject_sidecars(path: &Path) -> Result<()> {
    for sidecar in sidecar_paths(path) {
        match fs::symlink_metadata(&sidecar) {
            Err(source) if source.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(project_error(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "reject_candidate_sidecars",
                    "closed save candidate retained a SQLite sidecar",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "reject_candidate_sidecars")
                        .with_field("sidecar", sidecar.display().to_string()),
                ));
            }
            Err(source) => {
                return Err(io_error(
                    source,
                    "reject_candidate_sidecars",
                    "save candidate sidecar state could not be inspected",
                    Some(&sidecar),
                ));
            }
        }
    }
    Ok(())
}

fn load_current_snapshot(path: &Path, operation: &'static str) -> Result<ProjectSnapshot> {
    let context =
        || ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string());
    let database =
        ProjectDatabase::open_read_only(path).map_err(|error| error.with_context(context()))?;
    let snapshot = database
        .load()
        .map_err(|error| error.with_context(context()))?
        .snapshot();
    database
        .close_for_save()
        .map_err(|error| error.with_context(context()))?;
    Ok(snapshot)
}

fn require_exact_snapshot(
    actual: &ProjectSnapshot,
    expected: &ProjectSnapshot,
    operation: &'static str,
) -> Result<()> {
    if actual != expected {
        return Err(project_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            operation,
            "save candidate did not reproduce the complete supplied snapshot",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArtifactEvidence {
    Missing,
    ExpectedSnapshot,
    PreviousSnapshot,
    OtherValidProject,
    InvalidEntry,
    Unavailable,
}

impl ArtifactEvidence {
    const fn code(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::ExpectedSnapshot => "expected_snapshot",
            Self::PreviousSnapshot => "previous_snapshot",
            Self::OtherValidProject => "other_valid_project",
            Self::InvalidEntry => "invalid_entry",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicationState {
    NotPublished,
    PublishedNotDurable,
    Ambiguous,
}

impl PublicationState {
    const fn code(self) -> &'static str {
        match self {
            Self::NotPublished => "not_published",
            Self::PublishedNotDurable => "published_not_durable",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublicationEvidence {
    candidate: ArtifactEvidence,
    destination: ArtifactEvidence,
    same_file: Option<bool>,
    state: PublicationState,
}

impl PublicationEvidence {
    fn context(self) -> ErrorContext {
        let same_file = self
            .same_file
            .map_or("unknown", |same| if same { "true" } else { "false" });
        ErrorContext::new(COMPONENT, "publication_evidence")
            .with_field("candidate", self.candidate.code())
            .with_field("destination", self.destination.code())
            .with_field("same_file", same_file)
            .with_field("publication_state", self.state.code())
    }
}

fn inspect_publication(
    candidate: &Path,
    destination: &Path,
    mode: PublicationMode,
    expected: &ProjectSnapshot,
    previous: Option<&ProjectSnapshot>,
) -> PublicationEvidence {
    let candidate_evidence = inspect_artifact(candidate, expected, previous);
    let destination_evidence = inspect_artifact(destination, expected, previous);
    let same_file = existing_same_file(candidate, destination);
    let published = destination_evidence == ArtifactEvidence::ExpectedSnapshot
        && (candidate_evidence == ArtifactEvidence::Missing
            || (mode == PublicationMode::NoClobber && same_file == Some(true)));
    let not_published = candidate_evidence == ArtifactEvidence::ExpectedSnapshot
        && match mode {
            PublicationMode::ReplaceExisting => {
                destination_evidence == ArtifactEvidence::PreviousSnapshot
            }
            PublicationMode::NoClobber => {
                destination_evidence == ArtifactEvidence::Missing || same_file == Some(false)
            }
        };
    let state = if published {
        PublicationState::PublishedNotDurable
    } else if not_published {
        PublicationState::NotPublished
    } else {
        PublicationState::Ambiguous
    };
    PublicationEvidence {
        candidate: candidate_evidence,
        destination: destination_evidence,
        same_file,
        state,
    }
}

fn publication_is_owned(
    candidate: &OwnedCandidate,
    destination: &Path,
    evidence: PublicationEvidence,
) -> bool {
    if evidence.destination != ArtifactEvidence::ExpectedSnapshot
        || verify_identity_at_path(
            &candidate.identity,
            destination,
            "verify_publication_identity",
        )
        .is_err()
    {
        return false;
    }
    match evidence.candidate {
        ArtifactEvidence::Missing => true,
        ArtifactEvidence::ExpectedSnapshot if evidence.same_file == Some(true) => {
            verify_owned_candidate_path(candidate, "verify_publication_identity").is_ok()
        }
        _ => false,
    }
}

fn verify_pending_publication(pending: &PendingPublication) -> Result<PublicationEvidence> {
    let evidence = inspect_publication(
        &pending.candidate.path,
        &pending.destination,
        PublicationMode::NoClobber,
        &pending.snapshot,
        None,
    );
    if evidence.state != PublicationState::PublishedNotDurable
        || !publication_is_owned(&pending.candidate, &pending.destination, evidence)
    {
        return Err(project_error(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "verify_pending_publication",
            "retained publication evidence no longer proves the owned visible snapshot",
        )
        .with_context(evidence.context()));
    }
    reject_sidecars(&pending.candidate.path)?;
    Ok(evidence)
}

fn verify_recovered_publication(pending: &PendingPublication) -> Result<()> {
    let evidence = inspect_publication(
        &pending.candidate.path,
        &pending.destination,
        PublicationMode::NoClobber,
        &pending.snapshot,
        None,
    );
    if evidence.state != PublicationState::PublishedNotDurable
        || evidence.candidate != ArtifactEvidence::Missing
        || !publication_is_owned(&pending.candidate, &pending.destination, evidence)
    {
        return Err(project_error(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "verify_recovered_publication",
            "publication recovery did not produce one owned durable destination name",
        )
        .with_context(evidence.context()));
    }
    reject_sidecars(&pending.candidate.path)
}

fn inspect_artifact(
    path: &Path,
    expected: &ProjectSnapshot,
    previous: Option<&ProjectSnapshot>,
) -> ArtifactEvidence {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return ArtifactEvidence::Missing;
        }
        Err(_) => return ArtifactEvidence::Unavailable,
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return ArtifactEvidence::InvalidEntry;
    }
    let database = match ProjectDatabase::open_read_only(path) {
        Ok(database) => database,
        Err(_) => return ArtifactEvidence::InvalidEntry,
    };
    let loaded = match database.load() {
        Ok(document) => document.snapshot(),
        Err(_) => {
            let _ = database.close_for_save();
            return ArtifactEvidence::InvalidEntry;
        }
    };
    if database.close_for_save().is_err() {
        return ArtifactEvidence::Unavailable;
    }
    if loaded == *expected {
        ArtifactEvidence::ExpectedSnapshot
    } else if previous.is_some_and(|snapshot| loaded == *snapshot) {
        ArtifactEvidence::PreviousSnapshot
    } else {
        ArtifactEvidence::OtherValidProject
    }
}

fn existing_same_file(first: &Path, second: &Path) -> Option<bool> {
    let first_metadata = fs::symlink_metadata(first).ok()?;
    let second_metadata = fs::symlink_metadata(second).ok()?;
    if !first_metadata.is_file() || !second_metadata.is_file() {
        return None;
    }
    let first = file_identity_from_path(first).ok()?;
    let second = file_identity_from_path(second).ok()?;
    Some(first == second)
}

fn command_context(
    kind: ProjectSaveKind,
    phase: &'static str,
    active: &Path,
    destination: &Path,
    revision: u64,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, kind.operation())
        .with_field("phase", phase)
        .with_field("active_path", active.display().to_string())
        .with_field("destination", destination.display().to_string())
        .with_field("revision", revision.to_string())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SavePhase {
    TempReserved,
    SemanticWritten,
    CandidateValidated,
    DatabaseClosed,
    CandidateReopened,
    CandidateSynced,
    BeforePublication,
    AfterPublication,
}

impl SavePhase {
    const fn code(self) -> &'static str {
        match self {
            Self::TempReserved => "temp_reserved",
            Self::SemanticWritten => "semantic_written",
            Self::CandidateValidated => "candidate_validated",
            Self::DatabaseClosed => "database_closed",
            Self::CandidateReopened => "candidate_reopened",
            Self::CandidateSynced => "candidate_synced",
            Self::BeforePublication => "before_publication",
            Self::AfterPublication => "after_publication",
        }
    }
}

fn invoke_hook<F>(hook: &mut F, phase: SavePhase) -> Result<()>
where
    F: FnMut(SavePhase) -> Result<()>,
{
    let _ = phase.code();
    hook(phase)
}

fn io_error(
    source: io::Error,
    operation: &'static str,
    message: &'static str,
    path: Option<&Path>,
) -> Error {
    let (category, recoverability) = classify_io_error(&source);
    let mut context = ErrorContext::new(COMPONENT, operation);
    if let Some(path) = path {
        context.insert_field("path", path.display().to_string());
    }
    if let Some(code) = source.raw_os_error() {
        context.insert_field("native_code", code.to_string());
    }
    Error::with_source(category, recoverability, message, source).with_context(context)
}

fn classify_io_error(source: &io::Error) -> (ErrorCategory, Recoverability) {
    #[cfg(unix)]
    if let Some(code) = source.raw_os_error() {
        if code == libc::ENOSPC || code == libc::EDQUOT {
            return (
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
            );
        }
        if code == libc::EXDEV || code == libc::ENOTSUP || code == libc::EOPNOTSUPP {
            return (ErrorCategory::Unsupported, Recoverability::UserCorrectable);
        }
        if code == libc::EROFS || code == libc::EACCES || code == libc::EPERM {
            return (
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
            );
        }
    }
    #[cfg(windows)]
    if let Some(code) = source.raw_os_error() {
        if matches!(code, 8 | 14 | 39 | 112 | 1295 | 1816) {
            return (
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
            );
        }
    }
    match source.kind() {
        io::ErrorKind::InvalidInput => {
            (ErrorCategory::InvalidInput, Recoverability::UserCorrectable)
        }
        io::ErrorKind::NotFound => (ErrorCategory::NotFound, Recoverability::UserCorrectable),
        io::ErrorKind::AlreadyExists => (ErrorCategory::Conflict, Recoverability::UserCorrectable),
        io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
        ),
        io::ErrorKind::Unsupported => (ErrorCategory::Unsupported, Recoverability::UserCorrectable),
        _ => (ErrorCategory::Unavailable, Recoverability::Retryable),
    }
}

fn project_error(
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
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use superi_core::error::{Error, ErrorCategory, Recoverability};
    use superi_core::ids::{ProjectId, TimelineId};
    use superi_core::time::{RationalTime, Timebase};
    use superi_timeline::model::{EditorialProject, Timeline};

    use crate::document::{ProjectDocument, ProjectSnapshot};
    use crate::persist::ProjectDatabase;

    use super::*;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "superi-save-private-{label}-{}-{}",
                std::process::id(),
                NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self {
                path: fs::canonicalize(path).unwrap(),
            }
        }

        fn project(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn document() -> ProjectDocument {
        let project_id = ProjectId::from_raw(0x5b00);
        let root = TimelineId::from_raw(0x5b01);
        let timebase = Timebase::integer(24).unwrap();
        let timeline = Timeline::new(
            root,
            "save root",
            timebase,
            RationalTime::zero(timebase),
            vec![],
        );
        let editorial = EditorialProject::new(project_id, "save project", [], [timeline]).unwrap();
        ProjectDocument::new(editorial, root).unwrap()
    }

    fn renamed_snapshot(document: &mut ProjectDocument) -> ProjectSnapshot {
        let revision = document.revision();
        let root = document.root_timeline_id();
        document
            .edit(revision, |draft| {
                let editorial_revision = draft.editorial_project().revision();
                draft
                    .editorial_project_mut()
                    .edit(editorial_revision, |editorial| {
                        editorial.set_name("new durable project");
                        Ok(())
                    })?;
                draft.recompile_timeline(root)
            })
            .unwrap()
    }

    fn persist(path: &Path, snapshot: &ProjectSnapshot) {
        let mut database = ProjectDatabase::create(path).unwrap();
        database.replace(snapshot).unwrap();
    }

    fn load(path: &Path) -> ProjectSnapshot {
        ProjectDatabase::open_read_only(path)
            .unwrap()
            .load()
            .unwrap()
            .snapshot()
    }

    fn injected_failure(phase: SavePhase) -> Error {
        Error::new(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "injected save interruption",
        )
        .with_context(
            superi_core::error::ErrorContext::new("superi-project.save.test", "interrupt")
                .with_field("phase", phase.code()),
        )
    }

    fn assert_no_owned_candidates(root: &Path) {
        let entries = fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            entries
                .iter()
                .all(|name| !name.contains(".superi-save-candidate-")),
            "owned candidates remain: {entries:?}"
        );
    }

    fn owned_candidate(root: &Path) -> PathBuf {
        let candidates = fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().contains(CANDIDATE_MARKER))
            })
            .collect::<Vec<_>>();
        assert_eq!(candidates.len(), 1, "expected one owned candidate");
        candidates.into_iter().next().unwrap()
    }

    #[test]
    fn every_prepublication_interruption_preserves_old_or_absent_destination_and_cleans_candidate()
    {
        for phase in [
            SavePhase::TempReserved,
            SavePhase::SemanticWritten,
            SavePhase::CandidateValidated,
            SavePhase::DatabaseClosed,
            SavePhase::CandidateReopened,
            SavePhase::CandidateSynced,
            SavePhase::BeforePublication,
        ] {
            let root = TempRoot::new(phase.code());
            let active = root.project("active.superi");
            let copy = root.project("copy.superi");
            let mut document = document();
            let old = document.snapshot();
            let new = renamed_snapshot(&mut document);
            persist(&active, &old);
            let old_bytes = fs::read(&active).unwrap();

            let mut session = ProjectFileSession::new(active.clone()).unwrap();
            let error = session
                .execute_with_hook(
                    ProjectSaveCommand::Save {
                        snapshot: new.clone(),
                    },
                    |observed| {
                        if observed == phase {
                            Err(injected_failure(observed))
                        } else {
                            Ok(())
                        }
                    },
                )
                .unwrap_err();
            assert_eq!(error.category(), ErrorCategory::Unavailable);
            assert_eq!(fs::read(&active).unwrap(), old_bytes);
            assert_eq!(load(&active), old);
            assert_no_owned_candidates(&root.path);

            let mut copy_session = ProjectFileSession::new(active.clone()).unwrap();
            let error = copy_session
                .execute_with_hook(
                    ProjectSaveCommand::Copy {
                        destination: copy.clone(),
                        snapshot: new,
                    },
                    |observed| {
                        if observed == phase {
                            Err(injected_failure(observed))
                        } else {
                            Ok(())
                        }
                    },
                )
                .unwrap_err();
            assert_eq!(error.category(), ErrorCategory::Unavailable);
            assert!(!copy.exists());
            assert_no_owned_candidates(&root.path);
        }
    }

    #[test]
    fn interruption_after_publication_preserves_a_complete_project_and_retry_is_idempotent() {
        let root = TempRoot::new("postpublication");
        let active = root.project("active.superi");
        let mut document = document();
        let old = document.snapshot();
        let new = renamed_snapshot(&mut document);
        persist(&active, &old);
        let mut session = ProjectFileSession::new(active.clone()).unwrap();

        let error = session
            .execute_with_hook(
                ProjectSaveCommand::Save {
                    snapshot: new.clone(),
                },
                |phase| {
                    if phase == SavePhase::AfterPublication {
                        Err(injected_failure(phase))
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(load(&active), new);

        let outcome = session
            .execute(ProjectSaveCommand::Save {
                snapshot: new.clone(),
            })
            .unwrap();
        assert!(outcome.durable());
        assert_eq!(load(&active), new);
        assert_no_owned_candidates(&root.path);
    }

    #[test]
    fn no_clobber_postpublication_retry_finishes_all_commands_for_both_candidate_states() {
        #[cfg(unix)]
        let candidate_states = [true, false];
        #[cfg(windows)]
        let candidate_states = [false];

        for kind in [
            ProjectSaveKind::SaveAs,
            ProjectSaveKind::Copy,
            ProjectSaveKind::Backup,
        ] {
            for candidate_present in candidate_states {
                let root = TempRoot::new(&format!(
                    "retry-{}-{}",
                    kind.operation(),
                    if candidate_present {
                        "candidate-present"
                    } else {
                        "candidate-missing"
                    }
                ));
                let active = root.project("active.superi");
                let destination = root.project("destination.superi");
                let other = root.project("other.superi");
                let mut document = document();
                let durable = document.snapshot();
                let supplied = renamed_snapshot(&mut document);
                persist(&active, &durable);
                let expected = if kind == ProjectSaveKind::Backup {
                    durable.clone()
                } else {
                    supplied.clone()
                };
                let command = match kind {
                    ProjectSaveKind::SaveAs => ProjectSaveCommand::SaveAs {
                        destination: destination.clone(),
                        snapshot: supplied.clone(),
                    },
                    ProjectSaveKind::Copy => ProjectSaveCommand::Copy {
                        destination: destination.clone(),
                        snapshot: supplied.clone(),
                    },
                    ProjectSaveKind::Backup => ProjectSaveCommand::Backup {
                        destination: destination.clone(),
                    },
                    ProjectSaveKind::Save => unreachable!(),
                };
                let mut candidate = None;
                let mut session = ProjectFileSession::new(active.clone()).unwrap();

                let error = session
                    .execute_with_hook(command.clone(), |phase| {
                        if phase == SavePhase::TempReserved {
                            candidate = Some(owned_candidate(&root.path));
                        }
                        if phase == SavePhase::AfterPublication {
                            if candidate_present {
                                fs::hard_link(&destination, candidate.as_ref().unwrap()).unwrap();
                            }
                            return Err(injected_failure(phase));
                        }
                        Ok(())
                    })
                    .unwrap_err();
                assert_eq!(error.category(), ErrorCategory::Unavailable);
                assert_eq!(load(&destination), expected);
                assert_eq!(session.active_path(), active);
                assert_eq!(candidate.as_ref().unwrap().exists(), candidate_present);

                let mut unrelated_session = ProjectFileSession::new(active.clone()).unwrap();
                assert_eq!(
                    unrelated_session
                        .execute(command.clone())
                        .unwrap_err()
                        .category(),
                    ErrorCategory::Conflict,
                    "an unrelated session must not adopt identical visible state"
                );
                let different = session
                    .execute(ProjectSaveCommand::Copy {
                        destination: other,
                        snapshot: supplied.clone(),
                    })
                    .unwrap_err();
                assert_eq!(different.category(), ErrorCategory::Conflict);

                let outcome = session.execute(command).unwrap();
                assert_eq!(outcome.kind(), kind);
                assert!(outcome.durable());
                assert_eq!(outcome.adopted(), kind == ProjectSaveKind::SaveAs);
                assert_eq!(load(&destination), expected);
                assert_eq!(
                    session.active_path(),
                    if kind == ProjectSaveKind::SaveAs {
                        destination.as_path()
                    } else {
                        active.as_path()
                    }
                );
                assert_no_owned_candidates(&root.path);
            }
        }
    }

    #[test]
    fn reserved_candidate_replacement_preserves_victim_and_reports_retained_cleanup() {
        let root = TempRoot::new("reserved-replacement");
        let active = root.project("active.superi");
        let destination = root.project("copy.superi");
        let escaped = root.project("escaped-reservation");
        let victim = b"unrelated replacement victim";
        let snapshot = document().snapshot();
        persist(&active, &snapshot);
        let mut replaced_path = None;
        let mut session = ProjectFileSession::new(active).unwrap();

        let error = session
            .execute_with_hook(
                ProjectSaveCommand::Copy {
                    destination: destination.clone(),
                    snapshot,
                },
                |phase| {
                    if phase == SavePhase::TempReserved {
                        let candidate = owned_candidate(&root.path);
                        fs::rename(&candidate, &escaped).unwrap();
                        fs::write(&candidate, victim).unwrap();
                        replaced_path = Some(candidate);
                    }
                    Ok(())
                },
            )
            .unwrap_err();

        let replaced_path = replaced_path.unwrap();
        assert_eq!(fs::read(&replaced_path).unwrap(), victim);
        assert!(
            escaped.exists(),
            "the displaced owned reservation is preserved"
        );
        assert!(!destination.exists());
        let cleanup = error
            .contexts()
            .iter()
            .find(|context| {
                context.component() == COMPONENT
                    && context.operation() == "cleanup_candidate"
                    && context.field("cleanup_state") == Some("retained")
            })
            .expect("explicit retained cleanup evidence");
        assert_eq!(
            cleanup.field("candidate"),
            Some(replaced_path.display().to_string().as_str())
        );
    }

    #[test]
    fn native_no_clobber_conflict_reports_publication_phase_and_confirmed_cleanup() {
        let root = TempRoot::new("native-conflict");
        let active = root.project("active.superi");
        let destination = root.project("copy.superi");
        let winner = b"concurrent destination winner";
        let snapshot = document().snapshot();
        persist(&active, &snapshot);
        let mut session = ProjectFileSession::new(active).unwrap();

        let error = session
            .execute_with_hook(
                ProjectSaveCommand::Copy {
                    destination: destination.clone(),
                    snapshot,
                },
                |phase| {
                    if phase == SavePhase::BeforePublication {
                        fs::write(&destination, winner).unwrap();
                    }
                    Ok(())
                },
            )
            .unwrap_err();

        assert_eq!(error.category(), ErrorCategory::Conflict);
        assert_eq!(fs::read(&destination).unwrap(), winner);
        assert!(error.contexts().iter().any(|context| {
            context.component() == COMPONENT
                && context.operation() == "publish_project"
                && context.field("phase") == Some("native_publish")
        }));
        assert!(error.contexts().iter().any(|context| {
            context.component() == COMPONENT
                && context.operation() == "cleanup_candidate"
                && context.field("cleanup_state") == Some("removed")
        }));
        assert_no_owned_candidates(&root.path);
    }

    #[cfg(windows)]
    #[test]
    fn windows_prepublication_disk_limits_are_resource_exhausted() {
        for code in [8, 14, 39, 112, 1295, 1816] {
            let (category, recoverability) = classify_io_error(&io::Error::from_raw_os_error(code));
            assert_eq!(category, ErrorCategory::ResourceExhausted);
            assert_eq!(recoverability, Recoverability::UserCorrectable);
        }
    }

    #[test]
    fn publication_evidence_separates_safe_cleanup_from_visible_or_ambiguous_state() {
        let root = TempRoot::new("evidence");
        let mut document = document();
        let old = document.snapshot();
        let new = renamed_snapshot(&mut document);

        let replace_candidate = root.project("replace-candidate.superi");
        let replace_destination = root.project("replace-destination.superi");
        persist(&replace_candidate, &new);
        persist(&replace_destination, &old);
        let evidence = inspect_publication(
            &replace_candidate,
            &replace_destination,
            PublicationMode::ReplaceExisting,
            &new,
            Some(&old),
        );
        assert_eq!(evidence.state, PublicationState::NotPublished);
        assert_eq!(evidence.candidate, ArtifactEvidence::ExpectedSnapshot);
        assert_eq!(evidence.destination, ArtifactEvidence::PreviousSnapshot);

        fs::rename(&replace_candidate, &replace_destination).unwrap();
        let evidence = inspect_publication(
            &replace_candidate,
            &replace_destination,
            PublicationMode::ReplaceExisting,
            &new,
            Some(&old),
        );
        assert_eq!(evidence.state, PublicationState::PublishedNotDurable);
        assert_eq!(evidence.candidate, ArtifactEvidence::Missing);
        assert_eq!(evidence.destination, ArtifactEvidence::ExpectedSnapshot);

        let link_candidate = root.project("link-candidate.superi");
        let link_destination = root.project("link-destination.superi");
        persist(&link_candidate, &new);
        let evidence = inspect_publication(
            &link_candidate,
            &link_destination,
            PublicationMode::NoClobber,
            &new,
            None,
        );
        assert_eq!(evidence.state, PublicationState::NotPublished);
        fs::hard_link(&link_candidate, &link_destination).unwrap();
        let evidence = inspect_publication(
            &link_candidate,
            &link_destination,
            PublicationMode::NoClobber,
            &new,
            None,
        );
        assert_eq!(evidence.state, PublicationState::PublishedNotDurable);
        assert_eq!(evidence.same_file, Some(true));

        let race_candidate = root.project("race-candidate.superi");
        let race_destination = root.project("race-destination.superi");
        persist(&race_candidate, &new);
        persist(&race_destination, &old);
        let evidence = inspect_publication(
            &race_candidate,
            &race_destination,
            PublicationMode::NoClobber,
            &new,
            None,
        );
        assert_eq!(evidence.state, PublicationState::NotPublished);
        assert_eq!(evidence.same_file, Some(false));

        let ambiguous_candidate = root.project("ambiguous-candidate.superi");
        let ambiguous_destination = root.project("ambiguous-destination.superi");
        fs::write(&ambiguous_destination, b"unresolved publication evidence").unwrap();
        let evidence = inspect_publication(
            &ambiguous_candidate,
            &ambiguous_destination,
            PublicationMode::ReplaceExisting,
            &new,
            Some(&old),
        );
        assert_eq!(evidence.state, PublicationState::Ambiguous);
        assert_eq!(evidence.destination, ArtifactEvidence::InvalidEntry);
    }
}
