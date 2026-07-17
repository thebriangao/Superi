//! Deterministic project autosave scheduling, publication, and retention.
//!
//! The controller is clockless and threadless. A host supplies monotonic elapsed
//! time and runs commands on its background I/O owner. Complete recovery points
//! are published through the same atomic project backup path as explicit saves.

use std::fs::{self, DirBuilder, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::ProjectId;

use crate::document::ProjectSnapshot;
use crate::{ProjectDatabase, ProjectSaveCommand};

const COMPONENT: &str = "superi-project.autosave";
const PROJECT_DIRECTORY_PREFIX: &str = "project-";
const ARTIFACT_PREFIX: &str = "autosave-g";
const ARTIFACT_SUFFIX: &str = ".superi";
const ARTIFACT_DIGITS: usize = 20;
const MAX_PUBLICATION_COLLISIONS: usize = 16;

/// Maximum completed recovery points accepted by one autosave policy.
pub const MAX_PROJECT_AUTOSAVE_RETENTION: usize = 4096;

/// Validated runtime policy for one project autosave controller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectAutosavePolicy {
    enabled: bool,
    interval: Duration,
    recovery_root: PathBuf,
    retention: usize,
}

impl ProjectAutosavePolicy {
    /// Creates a policy rooted in one explicit existing local directory.
    pub fn new(
        enabled: bool,
        interval: Duration,
        recovery_root: impl AsRef<Path>,
        retention: usize,
    ) -> Result<Self> {
        if interval.is_zero() {
            return Err(autosave_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_policy",
                "project autosave interval must be greater than zero",
            ));
        }
        if retention == 0 {
            return Err(autosave_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_policy",
                "project autosave retention must be greater than zero",
            ));
        }
        if retention > MAX_PROJECT_AUTOSAVE_RETENTION {
            return Err(autosave_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "validate_policy",
                "project autosave retention exceeds the supported bound",
            ));
        }
        let recovery_root = canonical_recovery_root(recovery_root.as_ref())?;
        Ok(Self {
            enabled,
            interval,
            recovery_root,
            retention,
        })
    }

    /// Reports whether periodic autosave ticks may publish.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the nonzero periodic interval.
    #[must_use]
    pub const fn interval(&self) -> Duration {
        self.interval
    }

    /// Returns the canonical existing recovery root.
    #[must_use]
    pub fn recovery_root(&self) -> &Path {
        &self.recovery_root
    }

    /// Returns the number of newest completed artifacts retained.
    #[must_use]
    pub const fn retention(&self) -> usize {
        self.retention
    }
}

/// One user or host command accepted by the autosave controller.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectAutosaveCommand {
    /// Replaces runtime policy and anchors a fresh periodic deadline.
    Configure {
        /// Fully validated policy.
        policy: ProjectAutosavePolicy,
        /// Monotonic elapsed time supplied by the host.
        elapsed: Duration,
    },
    /// Observes one periodic scheduling point.
    Tick {
        /// Monotonic elapsed time supplied by the host.
        elapsed: Duration,
        /// Immutable selected project state.
        snapshot: ProjectSnapshot,
    },
    /// Publishes a recovery point regardless of enabled state or revision equality.
    SaveNow {
        /// Monotonic elapsed time supplied by the host.
        elapsed: Duration,
        /// Immutable selected project state.
        snapshot: ProjectSnapshot,
    },
    /// Applies the active retention policy immediately.
    Prune,
    /// Returns current schedule and managed-artifact state without mutation.
    Inspect,
}

impl ProjectAutosaveCommand {
    /// Returns the stable semantic operation.
    #[must_use]
    pub const fn operation(&self) -> ProjectAutosaveOperation {
        match self {
            Self::Configure { .. } => ProjectAutosaveOperation::Configure,
            Self::Tick { .. } => ProjectAutosaveOperation::Tick,
            Self::SaveNow { .. } => ProjectAutosaveOperation::SaveNow,
            Self::Prune => ProjectAutosaveOperation::Prune,
            Self::Inspect => ProjectAutosaveOperation::Inspect,
        }
    }
}

/// Stable semantic autosave operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectAutosaveOperation {
    /// Replace runtime policy.
    Configure,
    /// Observe a periodic deadline.
    Tick,
    /// Force one recovery point.
    SaveNow,
    /// Apply retention immediately.
    Prune,
    /// Inspect state.
    Inspect,
}

/// Stable result classification for a successful autosave command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectAutosaveDisposition {
    /// Policy was configured and any required retention work completed.
    Configured,
    /// Periodic publication is disabled.
    Disabled,
    /// The supplied tick precedes the current deadline.
    NotDue,
    /// The due project revision already has a successful recovery point.
    Unchanged,
    /// A complete recovery point was published.
    Published,
    /// Retention was applied without publication.
    Pruned,
    /// State was inspected without mutation.
    Inspected,
}

/// Durable evidence for one completed project recovery point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectAutosaveArtifact {
    generation: u64,
    path: PathBuf,
    project_revision: u64,
}

impl ProjectAutosaveArtifact {
    /// Returns the monotonic managed-file generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the absolute completed artifact path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the exact project revision stored by the artifact.
    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }
}

/// Complete observable state after one successful command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectAutosaveState {
    project_id: ProjectId,
    policy: Option<ProjectAutosavePolicy>,
    last_elapsed: Option<Duration>,
    next_due: Option<Duration>,
    last_published: Option<ProjectAutosaveArtifact>,
    managed_count: usize,
}

impl ProjectAutosaveState {
    /// Returns the project permanently bound to the controller.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Returns active runtime policy when configured.
    #[must_use]
    pub const fn policy(&self) -> Option<&ProjectAutosavePolicy> {
        self.policy.as_ref()
    }

    /// Returns the latest accepted monotonic time.
    #[must_use]
    pub const fn last_elapsed(&self) -> Option<Duration> {
        self.last_elapsed
    }

    /// Returns the next periodic deadline when periodic autosave is enabled.
    #[must_use]
    pub const fn next_due(&self) -> Option<Duration> {
        self.next_due
    }

    /// Returns the newest successful publication in this controller lifetime.
    #[must_use]
    pub const fn last_published(&self) -> Option<&ProjectAutosaveArtifact> {
        self.last_published.as_ref()
    }

    /// Returns the number of strictly managed completed files currently present.
    #[must_use]
    pub const fn managed_count(&self) -> usize {
        self.managed_count
    }
}

/// Complete typed result of one successful autosave command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectAutosaveOutcome {
    operation: ProjectAutosaveOperation,
    disposition: ProjectAutosaveDisposition,
    state: ProjectAutosaveState,
    published: Option<ProjectAutosaveArtifact>,
    removed_count: usize,
}

impl ProjectAutosaveOutcome {
    /// Returns the semantic operation that completed.
    #[must_use]
    pub const fn operation(&self) -> ProjectAutosaveOperation {
        self.operation
    }

    /// Returns the semantic completion classification.
    #[must_use]
    pub const fn disposition(&self) -> ProjectAutosaveDisposition {
        self.disposition
    }

    /// Returns complete controller state observed after completion.
    #[must_use]
    pub const fn state(&self) -> &ProjectAutosaveState {
        &self.state
    }

    /// Returns the artifact created by this command, when applicable.
    #[must_use]
    pub const fn published(&self) -> Option<&ProjectAutosaveArtifact> {
        self.published.as_ref()
    }

    /// Returns the number of completed artifacts pruned by this command.
    #[must_use]
    pub const fn removed_count(&self) -> usize {
        self.removed_count
    }
}

/// Exclusive runtime owner of one project's autosave schedule and managed files.
#[derive(Debug)]
pub struct ProjectAutosaveController {
    project_id: ProjectId,
    publisher: ProjectDatabase,
    policy: Option<ProjectAutosavePolicy>,
    project_directory: Option<PathBuf>,
    last_elapsed: Option<Duration>,
    next_due: Option<Duration>,
    last_published: Option<ProjectAutosaveArtifact>,
    last_periodic: Option<ProjectAutosaveArtifact>,
}

impl ProjectAutosaveController {
    /// Creates an unconfigured controller bound permanently to one project.
    pub fn new(project_id: ProjectId) -> Result<Self> {
        Ok(Self {
            project_id,
            publisher: ProjectDatabase::memory()?,
            policy: None,
            project_directory: None,
            last_elapsed: None,
            next_due: None,
            last_published: None,
            last_periodic: None,
        })
    }

    /// Executes one typed user or host command.
    pub fn execute(&mut self, command: ProjectAutosaveCommand) -> Result<ProjectAutosaveOutcome> {
        match command {
            ProjectAutosaveCommand::Configure { policy, elapsed } => {
                self.configure(policy, elapsed)
            }
            ProjectAutosaveCommand::Tick { elapsed, snapshot } => self.tick(elapsed, &snapshot),
            ProjectAutosaveCommand::SaveNow { elapsed, snapshot } => {
                self.save_now(elapsed, &snapshot)
            }
            ProjectAutosaveCommand::Prune => self.prune(),
            ProjectAutosaveCommand::Inspect => self.inspect(),
        }
    }

    fn configure(
        &mut self,
        policy: ProjectAutosavePolicy,
        elapsed: Duration,
    ) -> Result<ProjectAutosaveOutcome> {
        self.validate_elapsed(elapsed)?;
        let next_due = scheduled_deadline(&policy, elapsed)?;
        let project_directory = ensure_project_directory(&policy, self.project_id)?;
        let pruned = prune_managed(&project_directory, policy.retention())?;
        let root_changed = self.project_directory.as_deref() != Some(project_directory.as_path());

        self.policy = Some(policy);
        self.project_directory = Some(project_directory);
        self.last_elapsed = Some(elapsed);
        self.next_due = next_due;
        if root_changed {
            self.last_published = None;
            self.last_periodic = None;
        }

        Ok(self.outcome(
            ProjectAutosaveOperation::Configure,
            ProjectAutosaveDisposition::Configured,
            pruned.remaining_count,
            None,
            pruned.removed_count,
        ))
    }

    fn tick(
        &mut self,
        elapsed: Duration,
        snapshot: &ProjectSnapshot,
    ) -> Result<ProjectAutosaveOutcome> {
        self.validate_snapshot(snapshot)?;
        self.validate_elapsed(elapsed)?;
        let (policy, project_directory) = self.configured()?;

        if !policy.enabled() {
            let managed_count = scan_managed(&project_directory)?.len();
            self.last_elapsed = Some(elapsed);
            return Ok(self.outcome(
                ProjectAutosaveOperation::Tick,
                ProjectAutosaveDisposition::Disabled,
                managed_count,
                None,
                0,
            ));
        }

        let deadline = self.next_due.ok_or_else(|| {
            internal(
                "execute_tick",
                "enabled autosave policy has no periodic deadline",
            )
        })?;
        if elapsed < deadline {
            let managed_count = scan_managed(&project_directory)?.len();
            self.last_elapsed = Some(elapsed);
            return Ok(self.outcome(
                ProjectAutosaveOperation::Tick,
                ProjectAutosaveDisposition::NotDue,
                managed_count,
                None,
                0,
            ));
        }

        let next_due = checked_deadline(elapsed, policy.interval())?;
        let managed = scan_managed(&project_directory)?;
        let periodic_recovery_exists = self.last_periodic.as_ref().is_some_and(|artifact| {
            artifact.project_revision() == snapshot.revision()
                && managed.iter().any(|managed_artifact| {
                    managed_artifact.generation == artifact.generation()
                        && managed_artifact.path == artifact.path()
                })
        });
        if periodic_recovery_exists {
            self.last_elapsed = Some(elapsed);
            self.next_due = Some(next_due);
            return Ok(self.outcome(
                ProjectAutosaveOperation::Tick,
                ProjectAutosaveDisposition::Unchanged,
                managed.len(),
                None,
                0,
            ));
        }

        let published = self.publish(&project_directory, snapshot)?;
        self.last_elapsed = Some(elapsed);
        self.last_published = Some(published.clone());
        self.last_periodic = Some(published.clone());
        self.next_due = Some(next_due);
        let pruned = prune_managed(&project_directory, policy.retention())
            .map_err(|error| mark_published(error, &published, "prune_after_tick"))?;
        Ok(self.outcome(
            ProjectAutosaveOperation::Tick,
            ProjectAutosaveDisposition::Published,
            pruned.remaining_count,
            Some(published),
            pruned.removed_count,
        ))
    }

    fn save_now(
        &mut self,
        elapsed: Duration,
        snapshot: &ProjectSnapshot,
    ) -> Result<ProjectAutosaveOutcome> {
        self.validate_snapshot(snapshot)?;
        self.validate_elapsed(elapsed)?;
        let (policy, project_directory) = self.configured()?;
        let next_due = scheduled_deadline(&policy, elapsed)?;

        let published = self.publish(&project_directory, snapshot)?;
        self.last_elapsed = Some(elapsed);
        self.last_published = Some(published.clone());
        self.next_due = next_due;
        let pruned = prune_managed(&project_directory, policy.retention())
            .map_err(|error| mark_published(error, &published, "prune_after_save_now"))?;
        Ok(self.outcome(
            ProjectAutosaveOperation::SaveNow,
            ProjectAutosaveDisposition::Published,
            pruned.remaining_count,
            Some(published),
            pruned.removed_count,
        ))
    }

    fn prune(&mut self) -> Result<ProjectAutosaveOutcome> {
        let (policy, project_directory) = self.configured()?;
        let pruned = prune_managed(&project_directory, policy.retention())?;
        Ok(self.outcome(
            ProjectAutosaveOperation::Prune,
            ProjectAutosaveDisposition::Pruned,
            pruned.remaining_count,
            None,
            pruned.removed_count,
        ))
    }

    fn inspect(&self) -> Result<ProjectAutosaveOutcome> {
        let managed_count = match self.project_directory.as_deref() {
            Some(project_directory) => scan_managed(project_directory)?.len(),
            None => 0,
        };
        Ok(self.outcome(
            ProjectAutosaveOperation::Inspect,
            ProjectAutosaveDisposition::Inspected,
            managed_count,
            None,
            0,
        ))
    }

    fn publish(
        &mut self,
        project_directory: &Path,
        snapshot: &ProjectSnapshot,
    ) -> Result<ProjectAutosaveArtifact> {
        for collision in 0..MAX_PUBLICATION_COLLISIONS {
            let managed = scan_managed(project_directory)?;
            let generation = next_generation(&managed)?;
            let path = project_directory.join(artifact_file_name(generation));
            match self.publisher.execute_save_command(
                ProjectSaveCommand::Backup {
                    destination: path.clone(),
                },
                snapshot,
            ) {
                Ok(_) => {
                    return Ok(ProjectAutosaveArtifact {
                        generation,
                        path,
                        project_revision: snapshot.revision(),
                    });
                }
                Err(error) if error.category() == ErrorCategory::Conflict => {
                    if collision + 1 == MAX_PUBLICATION_COLLISIONS {
                        break;
                    }
                }
                Err(mut error) => {
                    error.push_context(
                        ErrorContext::new(COMPONENT, "publish_recovery_point")
                            .with_field("project_id", self.project_id.to_string())
                            .with_field("project_revision", snapshot.revision().to_string())
                            .with_field("generation", generation.to_string())
                            .with_field("path", path.display().to_string()),
                    );
                    return Err(error);
                }
            }
        }
        Err(autosave_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::Retryable,
            "publish_recovery_point",
            "project autosave publication collision bound was exhausted",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "publish_recovery_point")
                .with_field("project_id", self.project_id.to_string())
                .with_field("collision_attempts", MAX_PUBLICATION_COLLISIONS.to_string())
                .with_field("project_directory", project_directory.display().to_string()),
        ))
    }

    fn validate_snapshot(&self, snapshot: &ProjectSnapshot) -> Result<()> {
        if snapshot.project_id() == self.project_id {
            return Ok(());
        }
        Err(autosave_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "validate_project_identity",
            "autosave snapshot belongs to another project",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_project_identity")
                .with_field("project_id", self.project_id.to_string())
                .with_field("snapshot_project_id", snapshot.project_id().to_string()),
        ))
    }

    fn validate_elapsed(&self, elapsed: Duration) -> Result<()> {
        if self
            .last_elapsed
            .map_or(true, |last_elapsed| elapsed >= last_elapsed)
        {
            return Ok(());
        }
        let last_elapsed = self.last_elapsed.ok_or_else(|| {
            internal(
                "validate_monotonic_time",
                "backward autosave time has no prior observation",
            )
        })?;
        Err(autosave_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "validate_monotonic_time",
            "autosave elapsed time moved backward",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_monotonic_time")
                .with_field("elapsed_nanos", elapsed.as_nanos().to_string())
                .with_field("last_elapsed_nanos", last_elapsed.as_nanos().to_string()),
        ))
    }

    fn configured(&self) -> Result<(ProjectAutosavePolicy, PathBuf)> {
        let policy = self.policy.clone().ok_or_else(|| {
            autosave_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "require_policy",
                "project autosave controller is not configured",
            )
        })?;
        let project_directory = self.project_directory.clone().ok_or_else(|| {
            internal(
                "require_project_directory",
                "configured autosave policy has no managed project directory",
            )
        })?;
        Ok((policy, project_directory))
    }

    fn outcome(
        &self,
        operation: ProjectAutosaveOperation,
        disposition: ProjectAutosaveDisposition,
        managed_count: usize,
        published: Option<ProjectAutosaveArtifact>,
        removed_count: usize,
    ) -> ProjectAutosaveOutcome {
        ProjectAutosaveOutcome {
            operation,
            disposition,
            state: ProjectAutosaveState {
                project_id: self.project_id,
                policy: self.policy.clone(),
                last_elapsed: self.last_elapsed,
                next_due: self.next_due,
                last_published: self.last_published.clone(),
                managed_count,
            },
            published,
            removed_count,
        }
    }
}

#[derive(Debug)]
struct ManagedArtifact {
    generation: u64,
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PruneResult {
    removed_count: usize,
    remaining_count: usize,
}

fn canonical_recovery_root(root: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(root)
        .map_err(|source| filesystem_error(source, "canonicalize_recovery_root", root))?;
    validate_real_directory(&canonical, "validate_recovery_root")?;
    Ok(canonical)
}

fn ensure_project_directory(
    policy: &ProjectAutosavePolicy,
    project_id: ProjectId,
) -> Result<PathBuf> {
    validate_real_directory(policy.recovery_root(), "revalidate_recovery_root")?;
    let project_directory = policy
        .recovery_root()
        .join(project_directory_name(project_id));
    let mut builder = DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    let created = match builder.create(&project_directory) {
        Ok(()) => true,
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => false,
        Err(source) => {
            return Err(filesystem_error(
                source,
                "create_project_directory",
                &project_directory,
            ));
        }
    };
    validate_real_directory(&project_directory, "validate_project_directory")?;
    if created {
        sync_directory(policy.recovery_root(), "sync_recovery_root")?;
    }
    Ok(project_directory)
}

fn validate_real_directory(path: &Path, operation: &'static str) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).map_err(|source| filesystem_error(source, operation, path))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(autosave_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "project autosave path must be a real directory",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
        ));
    }
    Ok(())
}

fn scan_managed(project_directory: &Path) -> Result<Vec<ManagedArtifact>> {
    validate_real_directory(project_directory, "revalidate_project_directory")?;
    let entries = fs::read_dir(project_directory)
        .map_err(|source| filesystem_error(source, "scan_project_directory", project_directory))?;
    let mut managed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| {
            filesystem_error(source, "read_project_directory_entry", project_directory)
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(generation) = parse_artifact_file_name(name) else {
            continue;
        };
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|source| filesystem_error(source, "inspect_managed_artifact", &path))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(autosave_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "inspect_managed_artifact",
                "managed autosave name is not a regular file",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "inspect_managed_artifact")
                    .with_field("path", path.display().to_string())
                    .with_field("generation", generation.to_string()),
            ));
        }
        managed.push(ManagedArtifact { generation, path });
    }
    managed.sort_by_key(|artifact| artifact.generation);
    Ok(managed)
}

fn prune_managed(project_directory: &Path, retention: usize) -> Result<PruneResult> {
    let managed = scan_managed(project_directory)?;
    let remove_count = managed.len().saturating_sub(retention);
    let mut removed_count = 0;
    for artifact in managed.iter().take(remove_count) {
        let metadata = fs::symlink_metadata(&artifact.path).map_err(|source| {
            prune_error(
                filesystem_error(source, "reinspect_prune_artifact", &artifact.path),
                removed_count,
                remove_count,
                artifact,
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(prune_error(
                autosave_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "reinspect_prune_artifact",
                    "managed autosave changed type before pruning",
                ),
                removed_count,
                remove_count,
                artifact,
            ));
        }
        fs::remove_file(&artifact.path).map_err(|source| {
            prune_error(
                filesystem_error(source, "remove_managed_artifact", &artifact.path),
                removed_count,
                remove_count,
                artifact,
            )
        })?;
        removed_count += 1;
    }
    if removed_count > 0 {
        sync_directory(project_directory, "sync_pruned_project_directory").map_err(
            |mut error| {
                error.push_context(
                    ErrorContext::new(COMPONENT, "sync_pruned_project_directory")
                        .with_field("removed_count", removed_count.to_string())
                        .with_field("requested_remove_count", remove_count.to_string()),
                );
                error
            },
        )?;
    }
    Ok(PruneResult {
        removed_count,
        remaining_count: managed.len() - removed_count,
    })
}

fn prune_error(
    mut error: Error,
    removed_count: usize,
    requested_remove_count: usize,
    artifact: &ManagedArtifact,
) -> Error {
    error.push_context(
        ErrorContext::new(COMPONENT, "prune_managed_artifacts")
            .with_field("removed_count", removed_count.to_string())
            .with_field("requested_remove_count", requested_remove_count.to_string())
            .with_field("generation", artifact.generation.to_string())
            .with_field("path", artifact.path.display().to_string()),
    );
    error
}

fn next_generation(managed: &[ManagedArtifact]) -> Result<u64> {
    match managed.last() {
        Some(artifact) => artifact.generation.checked_add(1).ok_or_else(|| {
            autosave_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "select_generation",
                "project autosave generation is exhausted",
            )
        }),
        None => Ok(1),
    }
}

fn scheduled_deadline(
    policy: &ProjectAutosavePolicy,
    elapsed: Duration,
) -> Result<Option<Duration>> {
    if policy.enabled() {
        checked_deadline(elapsed, policy.interval()).map(Some)
    } else {
        Ok(None)
    }
}

fn checked_deadline(elapsed: Duration, interval: Duration) -> Result<Duration> {
    elapsed.checked_add(interval).ok_or_else(|| {
        autosave_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            "schedule_deadline",
            "project autosave deadline exceeds the supported duration range",
        )
    })
}

pub(crate) fn project_directory_name(project_id: ProjectId) -> String {
    format!("{PROJECT_DIRECTORY_PREFIX}{:032x}", project_id.raw())
}

pub(crate) fn artifact_file_name(generation: u64) -> String {
    format!("{ARTIFACT_PREFIX}{generation:020}{ARTIFACT_SUFFIX}")
}

pub(crate) fn parse_artifact_file_name(name: &str) -> Option<u64> {
    let digits = name
        .strip_prefix(ARTIFACT_PREFIX)?
        .strip_suffix(ARTIFACT_SUFFIX)?;
    if digits.len() != ARTIFACT_DIGITS || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let generation = digits.parse::<u64>().ok()?;
    (generation > 0 && artifact_file_name(generation) == name).then_some(generation)
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

fn mark_published(
    mut error: Error,
    artifact: &ProjectAutosaveArtifact,
    operation: &'static str,
) -> Error {
    error.push_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("publication_state", "published")
            .with_field("generation", artifact.generation().to_string())
            .with_field("path", artifact.path().display().to_string())
            .with_field("project_revision", artifact.project_revision().to_string()),
    );
    error
}

fn filesystem_error(source: io::Error, operation: &'static str, path: &Path) -> Error {
    let (category, recoverability, message) = if storage_exhausted(&source) {
        (
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            "project autosave storage limit was reached",
        )
    } else {
        match source.kind() {
            io::ErrorKind::AlreadyExists => (
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "project autosave path already exists",
            ),
            io::ErrorKind::NotFound => (
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "project autosave path does not exist",
            ),
            io::ErrorKind::PermissionDenied => (
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "project autosave path is not accessible",
            ),
            io::ErrorKind::Unsupported => (
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "filesystem does not support required autosave durability behavior",
            ),
            _ => (
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "project autosave filesystem operation failed",
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

fn autosave_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    autosave_error(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        operation,
        message,
    )
}
