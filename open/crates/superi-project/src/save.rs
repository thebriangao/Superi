//! Atomic whole-project save publication.

use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

use rusqlite::TransactionBehavior;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::document::ProjectSnapshot;
use crate::persist::{
    close_connection, database_error, initialize_schema, load_connection, open_file_connection,
    project_error, validate_full_integrity, write_prepared_project, PreparedProject,
    ProjectDatabase,
};

const COMPONENT: &str = "superi-project.save";
const CANDIDATE_MARKER: &str = ".superi-save-candidate-";
const MAX_CANDIDATE_ATTEMPTS: u64 = 128;
static NEXT_CANDIDATE: AtomicU64 = AtomicU64::new(0);

/// The collision behavior for an explicit project destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectDestinationCollision {
    /// Publish only when no filesystem entry already owns the destination name.
    RequireAbsent,
    /// Atomically replace an existing validated project, or create the destination when absent.
    ReplaceExisting,
}

/// The semantic operation executed through the project save surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectSaveOperation {
    /// Replace the current active project.
    Save,
    /// Publish a destination and make it the active project.
    SaveAs,
    /// Publish a destination while preserving active project identity.
    SaveCopy,
    /// Publish a no-clobber backup while preserving active project identity.
    Backup,
}

impl ProjectSaveOperation {
    const fn code(self) -> &'static str {
        match self {
            Self::Save => "save",
            Self::SaveAs => "save_as",
            Self::SaveCopy => "save_copy",
            Self::Backup => "backup",
        }
    }
}

/// One typed project-file command shared by interactive, script, and headless callers.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectSaveCommand {
    /// Atomically replaces the active project path.
    Save,
    /// Publishes a destination and rebinds active project identity at the commit point.
    SaveAs {
        /// Destination project path.
        destination: PathBuf,
        /// Explicit destination collision behavior.
        collision: ProjectDestinationCollision,
    },
    /// Publishes a copy without changing active project identity.
    SaveCopy {
        /// Destination project path.
        destination: PathBuf,
        /// Explicit destination collision behavior.
        collision: ProjectDestinationCollision,
    },
    /// Publishes a no-clobber backup of the supplied live snapshot.
    Backup {
        /// Destination project path.
        destination: PathBuf,
    },
}

impl ProjectSaveCommand {
    /// Returns the stable semantic operation.
    #[must_use]
    pub const fn operation(&self) -> ProjectSaveOperation {
        match self {
            Self::Save => ProjectSaveOperation::Save,
            Self::SaveAs { .. } => ProjectSaveOperation::SaveAs,
            Self::SaveCopy { .. } => ProjectSaveOperation::SaveCopy,
            Self::Backup { .. } => ProjectSaveOperation::Backup,
        }
    }
}

/// Evidence returned only after publication, supported synchronization, and owned cleanup succeed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSaveOutcome {
    operation: ProjectSaveOperation,
    destination: PathBuf,
    active_path: Option<PathBuf>,
    replaced_existing: bool,
}

impl ProjectSaveOutcome {
    /// Returns the semantic operation that completed.
    #[must_use]
    pub const fn operation(&self) -> ProjectSaveOperation {
        self.operation
    }

    /// Returns the absolute published destination.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// Returns the active path after the operation.
    #[must_use]
    pub fn active_path(&self) -> Option<&Path> {
        self.active_path.as_deref()
    }

    /// Returns whether a destination entry existed before publication.
    #[must_use]
    pub const fn replaced_existing(&self) -> bool {
        self.replaced_existing
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicationMode {
    ReplaceExisting,
    RequireAbsent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DestinationExpectation {
    Absent,
    Existing(DestinationIdentity),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DestinationIdentity {
    length: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    mode: u32,
}

impl DestinationIdentity {
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(unix)]
            mode: metadata.permissions().mode(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SaveStage {
    Prepared,
    CandidateReservation,
    CandidateReserved,
    DatabaseOpened,
    SchemaInitialized,
    RowsWritten,
    SemanticValidated,
    ClosingDatabase,
    DatabaseClosed,
    CandidateSyncing,
    CandidateSynced,
    Publishing,
    Published,
    PublishedSynced,
    ParentSynced,
    CandidateCleaned,
    Completed,
}

impl SaveStage {
    const fn code(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::CandidateReservation => "candidate_reservation",
            Self::CandidateReserved => "candidate_reserved",
            Self::DatabaseOpened => "database_opened",
            Self::SchemaInitialized => "schema_initialized",
            Self::RowsWritten => "rows_written",
            Self::SemanticValidated => "semantic_validated",
            Self::ClosingDatabase => "closing_database",
            Self::DatabaseClosed => "database_closed",
            Self::CandidateSyncing => "candidate_syncing",
            Self::CandidateSynced => "candidate_synced",
            Self::Publishing => "publishing",
            Self::Published => "published",
            Self::PublishedSynced => "published_synced",
            Self::ParentSynced => "parent_synced",
            Self::CandidateCleaned => "candidate_cleaned",
            Self::Completed => "completed",
        }
    }
}

struct SavePlan {
    operation: ProjectSaveOperation,
    destination: PathBuf,
    publication_mode: PublicationMode,
    rebind: bool,
    replaced_existing: bool,
    destination_expectation: DestinationExpectation,
}

impl ProjectDatabase {
    /// Executes save, save-as, copy, and backup through one atomic publication path.
    ///
    /// Every operation reconstructs the supplied snapshot into a complete current-schema
    /// candidate in the destination directory. Save-as changes active identity only after the
    /// filesystem publication commit point. Copy and backup never change active identity.
    pub fn execute_save_command(
        &mut self,
        command: ProjectSaveCommand,
        snapshot: &ProjectSnapshot,
    ) -> Result<ProjectSaveOutcome> {
        self.execute_save_command_inner(command, snapshot, &mut |_| Ok(()))
    }

    fn execute_save_command_inner<F>(
        &mut self,
        command: ProjectSaveCommand,
        snapshot: &ProjectSnapshot,
        hook: &mut F,
    ) -> Result<ProjectSaveOutcome>
    where
        F: FnMut(SaveStage) -> Result<()>,
    {
        let prepared = PreparedProject::from_snapshot(snapshot)?;
        invoke_hook(hook, SaveStage::Prepared)?;
        let plan = plan_command(self, &command)?;
        let parent = plan.destination.parent().ok_or_else(|| {
            invalid_destination(
                "resolve_destination",
                &plan.destination,
                "project destination must have a parent directory",
            )
        })?;

        invoke_hook(hook, SaveStage::CandidateReservation)?;
        let (reservation, candidate) = reserve_candidate(&plan.destination)?;
        let build_result = build_candidate(
            reservation,
            &candidate,
            parent,
            &plan,
            &prepared,
            snapshot,
            hook,
        );
        if let Err(error) = build_result {
            return Err(cleanup_precommit(error, &candidate));
        }

        if let Err(error) = invoke_hook(hook, SaveStage::Publishing) {
            return Err(cleanup_precommit(error, &candidate));
        }
        if let Err(error) = publish_candidate(&candidate, &plan) {
            return Err(cleanup_precommit(error, &candidate));
        }

        if plan.rebind {
            self.rebind_after_save_as(plan.destination.clone());
        }
        if let Err(error) = invoke_hook(hook, SaveStage::Published) {
            return Err(mark_published(error, &candidate, &plan, "publication_hook"));
        }

        if let Err(error) = finalize_publication(&candidate, parent, &plan, hook) {
            return Err(mark_published(
                error,
                &candidate,
                &plan,
                "finalize_publication",
            ));
        }

        Ok(ProjectSaveOutcome {
            operation: plan.operation,
            destination: plan.destination,
            active_path: self.active_path().map(Path::to_path_buf),
            replaced_existing: plan.replaced_existing,
        })
    }
}

fn plan_command(database: &ProjectDatabase, command: &ProjectSaveCommand) -> Result<SavePlan> {
    let active = database.active_path();
    let operation = command.operation();
    let (requested_destination, collision) = match command {
        ProjectSaveCommand::Save => {
            if !database.is_writable() {
                return Err(project_error(
                    ErrorCategory::PermissionDenied,
                    Recoverability::UserCorrectable,
                    "save_project",
                    "active project was opened without write authority",
                ));
            }
            let destination = active.ok_or_else(|| {
                project_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "save_project",
                    "in-memory project has no active path; use save-as",
                )
            })?;
            (
                destination.to_path_buf(),
                ProjectDestinationCollision::ReplaceExisting,
            )
        }
        ProjectSaveCommand::SaveAs {
            destination,
            collision,
        }
        | ProjectSaveCommand::SaveCopy {
            destination,
            collision,
        } => (destination.clone(), *collision),
        ProjectSaveCommand::Backup { destination } => (
            destination.clone(),
            ProjectDestinationCollision::RequireAbsent,
        ),
    };

    let destination = normalize_destination(&requested_destination)?;
    let aliases_active = active
        .map(|active_path| paths_alias(active_path, &destination))
        .transpose()?
        .unwrap_or(false);
    if matches!(
        operation,
        ProjectSaveOperation::SaveCopy | ProjectSaveOperation::Backup
    ) && aliases_active
    {
        return Err(conflict(
            "validate_destination_identity",
            &destination,
            "copy or backup destination aliases the active project",
        ));
    }
    if operation == ProjectSaveOperation::SaveAs && aliases_active && !database.is_writable() {
        return Err(project_error(
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
            "save_as_project",
            "save-as to the read-only active project is not permitted",
        ));
    }
    let collision = if operation == ProjectSaveOperation::SaveAs && aliases_active {
        ProjectDestinationCollision::ReplaceExisting
    } else {
        collision
    };

    let metadata = destination_metadata(&destination)?;
    if collision == ProjectDestinationCollision::RequireAbsent && metadata.is_some() {
        return Err(conflict(
            "validate_destination_collision",
            &destination,
            "project destination already exists",
        ));
    }
    if let Some(metadata) = metadata.as_ref() {
        validate_replaceable_entry(metadata, &destination)?;
        let existing = ProjectDatabase::open_read_only(&destination)?;
        if !aliases_active {
            existing.load()?;
        }
    }
    let destination_expectation = metadata
        .as_ref()
        .map(DestinationIdentity::from_metadata)
        .map(DestinationExpectation::Existing)
        .unwrap_or(DestinationExpectation::Absent);

    let publication_mode = match collision {
        ProjectDestinationCollision::RequireAbsent => PublicationMode::RequireAbsent,
        ProjectDestinationCollision::ReplaceExisting => PublicationMode::ReplaceExisting,
    };
    Ok(SavePlan {
        operation,
        destination,
        publication_mode,
        rebind: operation == ProjectSaveOperation::SaveAs && !aliases_active,
        replaced_existing: metadata.is_some(),
        destination_expectation,
    })
}

fn normalize_destination(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| io_error(source, "resolve_current_directory", path))?
            .join(path)
    };
    let file_name = absolute.file_name().ok_or_else(|| {
        invalid_destination(
            "resolve_destination",
            &absolute,
            "project destination must name a file",
        )
    })?;
    let parent = absolute.parent().ok_or_else(|| {
        invalid_destination(
            "resolve_destination",
            &absolute,
            "project destination must have a parent directory",
        )
    })?;
    let parent = fs::canonicalize(parent)
        .map_err(|source| io_error(source, "resolve_destination_parent", parent))?;
    if !fs::metadata(&parent)
        .map_err(|source| io_error(source, "inspect_destination_parent", &parent))?
        .is_dir()
    {
        return Err(invalid_destination(
            "inspect_destination_parent",
            &parent,
            "project destination parent is not a directory",
        ));
    }
    Ok(parent.join(file_name))
}

fn destination_metadata(path: &Path) -> Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error(source, "inspect_destination", path)),
    }
}

fn validate_replaceable_entry(metadata: &Metadata, path: &Path) -> Result<()> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid_destination(
            "validate_destination_type",
            path,
            "replace-existing destination must be a regular project file",
        ));
    }
    Ok(())
}

fn paths_alias(active: &Path, destination: &Path) -> Result<bool> {
    if active == destination {
        return Ok(true);
    }
    let destination_exists = destination_metadata(destination)?.is_some();
    if !destination_exists {
        return Ok(false);
    }
    let active_canonical = fs::canonicalize(active)
        .map_err(|source| io_error(source, "resolve_active_identity", active))?;
    let destination_canonical = fs::canonicalize(destination)
        .map_err(|source| io_error(source, "resolve_destination_identity", destination))?;
    if active_canonical == destination_canonical {
        return Ok(true);
    }
    #[cfg(unix)]
    {
        let active_metadata = fs::metadata(active)
            .map_err(|source| io_error(source, "inspect_active_identity", active))?;
        let destination_metadata = fs::metadata(destination)
            .map_err(|source| io_error(source, "inspect_destination_identity", destination))?;
        Ok(active_metadata.dev() == destination_metadata.dev()
            && active_metadata.ino() == destination_metadata.ino())
    }
    #[cfg(not(unix))]
    {
        Ok(false)
    }
}

fn reserve_candidate(destination: &Path) -> Result<(File, PathBuf)> {
    let parent = destination.parent().ok_or_else(|| {
        invalid_destination(
            "reserve_candidate",
            destination,
            "project destination must have a parent directory",
        )
    })?;
    let file_name = destination.file_name().ok_or_else(|| {
        invalid_destination(
            "reserve_candidate",
            destination,
            "project destination must name a file",
        )
    })?;

    for _ in 0..MAX_CANDIDATE_ATTEMPTS {
        let mut candidate_name = OsString::from(".");
        candidate_name.push(file_name);
        candidate_name.push(CANDIDATE_MARKER);
        candidate_name.push(std::process::id().to_string());
        candidate_name.push("-");
        candidate_name.push(NEXT_CANDIDATE.fetch_add(1, Ordering::Relaxed).to_string());
        let candidate = parent.join(candidate_name);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&candidate) {
            Ok(file) => return Ok((file, candidate)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(io_error(source, "reserve_candidate", &candidate)),
        }
    }
    Err(project_error(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "reserve_candidate",
        "could not reserve a unique project save candidate",
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_candidate<F>(
    reservation: File,
    candidate: &Path,
    parent: &Path,
    plan: &SavePlan,
    prepared: &PreparedProject,
    snapshot: &ProjectSnapshot,
    hook: &mut F,
) -> Result<()>
where
    F: FnMut(SaveStage) -> Result<()>,
{
    drop(reservation);
    invoke_hook(hook, SaveStage::CandidateReserved)?;

    let mut connection = open_file_connection(candidate, true)?;
    invoke_hook(hook, SaveStage::DatabaseOpened)?;
    connection
        .pragma_update(None, "journal_mode", "DELETE")
        .map_err(|source| database_error(source, "select_candidate_journal_mode"))?;
    let journal_mode: String = connection
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .map_err(|source| database_error(source, "verify_candidate_journal_mode"))?;
    if !journal_mode.eq_ignore_ascii_case("delete") {
        return Err(project_error(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "verify_candidate_journal_mode",
            "filesystem did not provide single-file SQLite rollback journal mode",
        ));
    }
    connection
        .pragma_update(None, "synchronous", "EXTRA")
        .map_err(|source| database_error(source, "select_candidate_synchronous_mode"))?;
    let synchronous: i64 = connection
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .map_err(|source| database_error(source, "verify_candidate_synchronous_mode"))?;
    if synchronous != 3 {
        return Err(project_error(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "verify_candidate_synchronous_mode",
            "SQLite did not apply EXTRA synchronization to the save candidate",
        ));
    }

    initialize_schema(&connection)?;
    invoke_hook(hook, SaveStage::SchemaInitialized)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|source| database_error(source, "begin_candidate_write"))?;
    write_prepared_project(&transaction, prepared)?;
    invoke_hook(hook, SaveStage::RowsWritten)?;
    let reconstructed = load_connection(&transaction)?;
    if reconstructed.snapshot() != *snapshot {
        return Err(project_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "verify_candidate_snapshot",
            "project save candidate did not reproduce the supplied snapshot",
        ));
    }
    invoke_hook(hook, SaveStage::SemanticValidated)?;
    transaction
        .commit()
        .map_err(|source| database_error(source, "commit_candidate_write"))?;

    validate_full_integrity(&connection)?;
    let reconstructed = load_connection(&connection)?;
    if reconstructed.snapshot() != *snapshot {
        return Err(project_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "verify_committed_candidate",
            "committed project save candidate changed semantic state",
        ));
    }
    invoke_hook(hook, SaveStage::ClosingDatabase)?;
    close_connection(connection, "close_candidate_database")?;
    invoke_hook(hook, SaveStage::DatabaseClosed)?;
    ensure_no_sidecars(candidate)?;
    preserve_destination_permissions(candidate, plan)?;

    invoke_hook(hook, SaveStage::CandidateSyncing)?;
    sync_file(candidate, "sync_candidate")?;
    sync_parent(parent, "sync_candidate_parent")?;
    invoke_hook(hook, SaveStage::CandidateSynced)
}

fn preserve_destination_permissions(candidate: &Path, plan: &SavePlan) -> Result<()> {
    if !plan.replaced_existing {
        return Ok(());
    }
    #[cfg(unix)]
    {
        let DestinationExpectation::Existing(identity) = &plan.destination_expectation else {
            return Ok(());
        };
        let mode = identity.mode;
        fs::set_permissions(candidate, fs::Permissions::from_mode(mode))
            .map_err(|source| io_error(source, "preserve_destination_permissions", candidate))?;
    }
    Ok(())
}

fn publish_candidate(candidate: &Path, plan: &SavePlan) -> Result<()> {
    if plan.publication_mode == PublicationMode::ReplaceExisting {
        validate_destination_expectation(plan)?;
    }
    let result = match plan.publication_mode {
        PublicationMode::ReplaceExisting => fs::rename(candidate, &plan.destination),
        PublicationMode::RequireAbsent => fs::hard_link(candidate, &plan.destination),
    };
    result.map_err(|source| {
        let operation = match plan.publication_mode {
            PublicationMode::ReplaceExisting => "replace_destination",
            PublicationMode::RequireAbsent => "publish_no_clobber_destination",
        };
        let mut error = io_error(source, operation, &plan.destination);
        error.push_context(
            ErrorContext::new(COMPONENT, "publish_candidate")
                .with_field("candidate", candidate.display().to_string())
                .with_field("destination", plan.destination.display().to_string())
                .with_field("publication_state", "not_published"),
        );
        error
    })
}

fn validate_destination_expectation(plan: &SavePlan) -> Result<()> {
    let observed = destination_metadata(&plan.destination)?;
    let unchanged = match (&plan.destination_expectation, observed.as_ref()) {
        (DestinationExpectation::Absent, None) => true,
        (DestinationExpectation::Existing(expected), Some(metadata)) => {
            metadata.is_file() && DestinationIdentity::from_metadata(metadata) == *expected
        }
        _ => false,
    };
    if unchanged {
        Ok(())
    } else {
        Err(conflict(
            "validate_destination_commit_identity",
            &plan.destination,
            "project destination changed while the save candidate was prepared",
        ))
    }
}

fn finalize_publication<F>(
    candidate: &Path,
    parent: &Path,
    plan: &SavePlan,
    hook: &mut F,
) -> Result<()>
where
    F: FnMut(SaveStage) -> Result<()>,
{
    sync_file(&plan.destination, "sync_published_project")?;
    invoke_hook(hook, SaveStage::PublishedSynced)?;
    sync_parent(parent, "sync_published_parent")?;
    invoke_hook(hook, SaveStage::ParentSynced)?;

    if plan.publication_mode == PublicationMode::RequireAbsent {
        fs::remove_file(candidate)
            .map_err(|source| io_error(source, "remove_published_candidate", candidate))?;
        invoke_hook(hook, SaveStage::CandidateCleaned)?;
        sync_parent(parent, "sync_candidate_cleanup")?;
    }
    invoke_hook(hook, SaveStage::Completed)
}

fn ensure_no_sidecars(candidate: &Path) -> Result<()> {
    for suffix in ["-journal", "-wal", "-shm"] {
        let sidecar = path_with_suffix(candidate, suffix);
        if fs::symlink_metadata(&sidecar).is_ok() {
            return Err(project_error(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "verify_candidate_sidecars",
                "SQLite candidate retained a sidecar after close",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "verify_candidate_sidecars")
                    .with_field("sidecar", sidecar.display().to_string()),
            ));
        }
    }
    Ok(())
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn cleanup_precommit(mut error: Error, candidate: &Path) -> Error {
    let mut failures = Vec::new();
    for path in [
        candidate.to_path_buf(),
        path_with_suffix(candidate, "-journal"),
        path_with_suffix(candidate, "-wal"),
        path_with_suffix(candidate, "-shm"),
    ] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(source) if source.kind() == io::ErrorKind::NotFound => {}
            Err(source) => failures.push(format!("{}: {source}", path.display())),
        }
    }
    let mut context = ErrorContext::new(COMPONENT, "cleanup_precommit_candidate")
        .with_field("candidate", candidate.display().to_string())
        .with_field("publication_state", "not_published");
    if !failures.is_empty() {
        context.insert_field("cleanup_failures", failures.join(" | "));
    }
    error.push_context(context);
    error
}

fn mark_published(
    mut error: Error,
    candidate: &Path,
    plan: &SavePlan,
    operation: &'static str,
) -> Error {
    error.push_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("operation", plan.operation.code())
            .with_field("candidate", candidate.display().to_string())
            .with_field("destination", plan.destination.display().to_string())
            .with_field("publication_state", "published")
            .with_field("durability", "unconfirmed")
            .with_field(
                "recovery_action",
                "reopen and validate the published destination before retrying",
            ),
    );
    error
}

fn invoke_hook<F>(hook: &mut F, stage: SaveStage) -> Result<()>
where
    F: FnMut(SaveStage) -> Result<()>,
{
    hook(stage).map_err(|mut error| {
        error.push_context(
            ErrorContext::new(COMPONENT, "save_stage_hook").with_field("stage", stage.code()),
        );
        error
    })
}

fn sync_file(path: &Path, operation: &'static str) -> Result<()> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    options.write(true);
    options
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|source| io_error(source, operation, path))
}

#[cfg(unix)]
fn sync_parent(parent: &Path, operation: &'static str) -> Result<()> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| io_error(source, operation, parent))
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path, _operation: &'static str) -> Result<()> {
    Ok(())
}

fn conflict(operation: &'static str, path: &Path, message: &'static str) -> Error {
    project_error(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
    )
}

fn invalid_destination(operation: &'static str, path: &Path, message: &'static str) -> Error {
    project_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
    )
}

fn io_error(source: io::Error, operation: &'static str, path: &Path) -> Error {
    let (category, recoverability, message) = if storage_exhausted(&source) {
        (
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            "project save storage limit was reached",
        )
    } else {
        match source.kind() {
            io::ErrorKind::AlreadyExists => (
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "project destination already exists",
            ),
            io::ErrorKind::NotFound => (
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "project save path does not exist",
            ),
            io::ErrorKind::PermissionDenied => (
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "project save path is not accessible",
            ),
            io::ErrorKind::Unsupported => (
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "filesystem does not support the required atomic publication primitive",
            ),
            _ => (
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "project save filesystem operation failed",
            ),
        }
    };
    Error::with_source(category, recoverability, message, source).with_context(
        ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
    )
}

fn storage_exhausted(source: &io::Error) -> bool {
    if source.kind() == io::ErrorKind::OutOfMemory {
        return true;
    }
    #[cfg(unix)]
    {
        matches!(source.raw_os_error(), Some(27 | 28 | 69 | 122))
    }
    #[cfg(windows)]
    {
        matches!(source.raw_os_error(), Some(39 | 112 | 223))
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};

    use superi_core::ids::{ProjectId, TimelineId};
    use superi_core::time::{RationalTime, Timebase};
    use superi_timeline::model::{EditorialProject, Timeline};

    use super::*;
    use crate::document::ProjectDocument;

    const ABORT_CHILD_PATH: &str = "SUPERI_SAVE_ABORT_CHILD_PATH";
    const ABORT_CHILD_DESTINATION: &str = "SUPERI_SAVE_ABORT_CHILD_DESTINATION";
    const ABORT_CHILD_STAGE: &str = "SUPERI_SAVE_ABORT_CHILD_STAGE";
    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "superi-save-unit-{label}-{}-{}",
                std::process::id(),
                NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self { path }
        }

        fn project(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }

        fn candidate_count(&self) -> usize {
            fs::read_dir(&self.path)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .filter(|name| name.to_string_lossy().contains(CANDIDATE_MARKER))
                .count()
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_snapshot(label: &str, identity: u128) -> ProjectSnapshot {
        let root = TimelineId::from_raw(identity + 1);
        let timebase = Timebase::integer(24).unwrap();
        let timeline = Timeline::new(
            root,
            format!("{label} timeline"),
            timebase,
            RationalTime::zero(timebase),
            vec![],
        );
        let project = EditorialProject::new(
            ProjectId::from_raw(identity),
            format!("{label} project"),
            [],
            [timeline],
        )
        .unwrap();
        ProjectDocument::new(project, root).unwrap().snapshot()
    }

    fn create_project(path: &Path, snapshot: &ProjectSnapshot) {
        let mut database = ProjectDatabase::create(path).unwrap();
        database.replace(snapshot).unwrap();
    }

    fn assert_valid_project(path: &Path, expected: &ProjectSnapshot) {
        let database = ProjectDatabase::open_read_only(path).unwrap();
        assert_eq!(database.load().unwrap().snapshot(), *expected);
        let connection = open_file_connection(path, false).unwrap();
        validate_full_integrity(&connection).unwrap();
        close_connection(connection, "close_test_integrity_connection").unwrap();
    }

    fn injected_failure(stage: SaveStage) -> Error {
        Error::new(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            format!("injected failure at {}", stage.code()),
        )
    }

    fn has_context_field(error: &Error, key: &str, expected: &str) -> bool {
        error
            .contexts()
            .iter()
            .any(|context| context.field(key) == Some(expected))
    }

    #[test]
    fn deterministic_prepublication_stage_faults_preserve_the_old_authority() {
        let stages = [
            SaveStage::Prepared,
            SaveStage::CandidateReservation,
            SaveStage::CandidateReserved,
            SaveStage::DatabaseOpened,
            SaveStage::SchemaInitialized,
            SaveStage::RowsWritten,
            SaveStage::SemanticValidated,
            SaveStage::ClosingDatabase,
            SaveStage::DatabaseClosed,
            SaveStage::CandidateSyncing,
            SaveStage::CandidateSynced,
            SaveStage::Publishing,
        ];

        for stage in stages {
            let directory = TestDirectory::new(stage.code());
            let active = directory.project("active.superi");
            let old = test_snapshot("old stage authority", 0x5000);
            let new = test_snapshot("new stage candidate", 0x5010);
            create_project(&active, &old);
            let old_bytes = fs::read(&active).unwrap();
            let canonical_active = fs::canonicalize(&active).unwrap();
            let mut database = ProjectDatabase::open(&active).unwrap();
            let mut hook = |observed| {
                if observed == stage {
                    Err(injected_failure(stage))
                } else {
                    Ok(())
                }
            };

            let error = database
                .execute_save_command_inner(ProjectSaveCommand::Save, &new, &mut hook)
                .unwrap_err();

            assert_eq!(error.category(), ErrorCategory::Unavailable, "{stage:?}");
            assert_eq!(
                database.active_path(),
                Some(canonical_active.as_path()),
                "{stage:?}"
            );
            assert_eq!(fs::read(&active).unwrap(), old_bytes, "{stage:?}");
            assert_valid_project(&active, &old);
            assert_eq!(directory.candidate_count(), 0, "{stage:?}");
        }
    }

    #[test]
    fn deterministic_postpublication_stage_faults_retain_rebound_published_state() {
        let stages = [
            SaveStage::Published,
            SaveStage::PublishedSynced,
            SaveStage::ParentSynced,
            SaveStage::CandidateCleaned,
            SaveStage::Completed,
        ];

        for stage in stages {
            let directory = TestDirectory::new(stage.code());
            let destination = directory.project("rebound.superi");
            let expected = test_snapshot("published state", 0x5020);
            let mut database = ProjectDatabase::memory().unwrap();
            database.replace(&expected).unwrap();
            let mut hook = |observed| {
                if observed == stage {
                    Err(injected_failure(stage))
                } else {
                    Ok(())
                }
            };

            let error = database
                .execute_save_command_inner(
                    ProjectSaveCommand::SaveAs {
                        destination: destination.clone(),
                        collision: ProjectDestinationCollision::RequireAbsent,
                    },
                    &expected,
                    &mut hook,
                )
                .unwrap_err();

            let canonical_destination = fs::canonicalize(&destination).unwrap();
            assert_eq!(
                database.active_path(),
                Some(canonical_destination.as_path()),
                "{stage:?}"
            );
            assert_valid_project(&destination, &expected);
            assert!(
                has_context_field(&error, "publication_state", "published"),
                "{stage:?}: {error}"
            );
            assert!(
                has_context_field(&error, "durability", "unconfirmed"),
                "{stage:?}: {error}"
            );
            if matches!(stage, SaveStage::CandidateCleaned | SaveStage::Completed) {
                assert_eq!(directory.candidate_count(), 0, "{stage:?}");
            }
        }
    }

    #[test]
    fn destination_races_fail_without_clobber_for_both_collision_modes() {
        for collision in [
            ProjectDestinationCollision::RequireAbsent,
            ProjectDestinationCollision::ReplaceExisting,
        ] {
            let directory = TestDirectory::new("destination-race");
            let destination = directory.project("raced.superi");
            let expected = test_snapshot("race candidate", 0x5030);
            let mut database = ProjectDatabase::memory().unwrap();
            database.replace(&expected).unwrap();
            let mut hook = |stage| {
                if stage == SaveStage::CandidateSynced {
                    fs::write(&destination, b"competing destination").unwrap();
                }
                Ok(())
            };

            let error = database
                .execute_save_command_inner(
                    ProjectSaveCommand::SaveCopy {
                        destination: destination.clone(),
                        collision,
                    },
                    &expected,
                    &mut hook,
                )
                .unwrap_err();

            assert_eq!(error.category(), ErrorCategory::Conflict);
            assert_eq!(fs::read(&destination).unwrap(), b"competing destination");
            assert_eq!(database.active_path(), None);
            assert_eq!(directory.candidate_count(), 0);
        }
    }

    #[test]
    fn process_abort_child() {
        let Some(path) = std::env::var_os(ABORT_CHILD_PATH).map(PathBuf::from) else {
            return;
        };
        let target_stage = std::env::var(ABORT_CHILD_STAGE).unwrap();
        let expected = test_snapshot("abort new", 0x5050);
        let mut database = ProjectDatabase::open(&path).unwrap();
        let command = std::env::var_os(ABORT_CHILD_DESTINATION)
            .map(PathBuf::from)
            .map(|destination| ProjectSaveCommand::SaveCopy {
                destination,
                collision: ProjectDestinationCollision::RequireAbsent,
            })
            .unwrap_or(ProjectSaveCommand::Save);
        let mut hook = |stage: SaveStage| {
            if stage.code() == target_stage {
                std::process::abort();
            }
            Ok(())
        };
        database
            .execute_save_command_inner(command, &expected, &mut hook)
            .unwrap();
        panic!("abort checkpoint was not reached: {target_stage}");
    }

    #[test]
    fn process_abort_interruption_leaves_exact_old_or_new_project_authority() {
        let executable = std::env::current_exe().unwrap();
        let old = test_snapshot("abort old", 0x5040);
        let new = test_snapshot("abort new", 0x5050);
        for (stage, expected, expected_candidates) in [
            (SaveStage::DatabaseClosed, &old, 1),
            (SaveStage::CandidateSynced, &old, 1),
            (SaveStage::Published, &new, 0),
            (SaveStage::Completed, &new, 0),
        ] {
            let directory = TestDirectory::new(stage.code());
            let active = directory.project("active.superi");
            create_project(&active, &old);
            let old_bytes = fs::read(&active).unwrap();

            let status = Command::new(&executable)
                .arg("--exact")
                .arg("save::tests::process_abort_child")
                .arg("--nocapture")
                .env(ABORT_CHILD_PATH, &active)
                .env(ABORT_CHILD_STAGE, stage.code())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();

            assert!(!status.success(), "child did not abort at {stage:?}");
            assert_valid_project(&active, expected);
            if *expected == old {
                assert_eq!(fs::read(&active).unwrap(), old_bytes, "{stage:?}");
            }
            assert_eq!(
                directory.candidate_count(),
                expected_candidates,
                "{stage:?}"
            );
        }

        for (stage, published, expected_candidates) in [
            (SaveStage::DatabaseClosed, false, 1),
            (SaveStage::CandidateSynced, false, 1),
            (SaveStage::Published, true, 1),
            (SaveStage::Completed, true, 0),
        ] {
            let directory = TestDirectory::new(&format!("link-{}", stage.code()));
            let active = directory.project("active.superi");
            let destination = directory.project("copy.superi");
            create_project(&active, &old);
            let old_bytes = fs::read(&active).unwrap();

            let status = Command::new(&executable)
                .arg("--exact")
                .arg("save::tests::process_abort_child")
                .arg("--nocapture")
                .env(ABORT_CHILD_PATH, &active)
                .env(ABORT_CHILD_DESTINATION, &destination)
                .env(ABORT_CHILD_STAGE, stage.code())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();

            assert!(!status.success(), "child did not abort at {stage:?}");
            assert_eq!(fs::read(&active).unwrap(), old_bytes, "{stage:?}");
            assert_valid_project(&active, &old);
            if published {
                assert_valid_project(&destination, &new);
            } else {
                assert!(!destination.exists(), "{stage:?}");
            }
            assert_eq!(
                directory.candidate_count(),
                expected_candidates,
                "{stage:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_storage_limit_errors_use_the_resource_exhausted_classification() {
        for raw_error in [27, 28, 69, 122] {
            assert!(storage_exhausted(&io::Error::from_raw_os_error(raw_error)));
        }
    }
}
