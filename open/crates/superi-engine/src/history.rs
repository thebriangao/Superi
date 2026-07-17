//! Bounded engine-owned command history for authored project state.
//!
//! History is session-local. Each entry retains immutable before and after snapshots, while the
//! selected snapshot remains the only state persisted by the project layer.

use std::collections::VecDeque;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_project::command_log::{
    ProjectCommandRecord, ProjectCommandRecordDraft, ProjectCommandRecordKind,
};
use superi_project::diagnostics::ProjectDiagnostics;
use superi_project::document::{ProjectDocument, ProjectSnapshot};
use superi_project::extensions::{
    ProjectExtensionCommand, ProjectExtensionCommandResult, ProjectExtensionOperation,
};
use superi_project::media::{ProjectMediaCommand, ProjectMediaCommandResult};
use superi_project::settings::ProjectSettingsTransaction;

use crate::project_transaction::{
    execute_compound_project_transaction, CompoundProjectActionResult, CompoundProjectTransaction,
};

const COMPONENT: &str = "superi-engine.history";

/// Default number of authored project mutations retained for undo and redo.
pub const DEFAULT_PROJECT_HISTORY_CAPACITY: usize = 64;
/// Maximum project history capacity accepted from a caller.
pub const MAX_PROJECT_HISTORY_CAPACITY: usize = 4096;

#[derive(Clone, Debug)]
struct HistoryEntry<S, M> {
    before: S,
    after: S,
    metadata: M,
}

/// Shared bounded storage for immutable before and after states.
#[derive(Clone, Debug)]
pub(crate) struct SnapshotHistory<S, M> {
    capacity: usize,
    undo: VecDeque<HistoryEntry<S, M>>,
    redo: VecDeque<HistoryEntry<S, M>>,
}

impl<S, M> SnapshotHistory<S, M> {
    pub(crate) fn new(capacity: usize) -> Self {
        debug_assert!(capacity > 0);
        Self {
            capacity,
            undo: VecDeque::with_capacity(capacity),
            redo: VecDeque::with_capacity(capacity),
        }
    }

    pub(crate) const fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub(crate) fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    pub(crate) fn next_undo(&self) -> Option<&M> {
        self.undo.back().map(|entry| &entry.metadata)
    }

    pub(crate) fn next_redo(&self) -> Option<&M> {
        self.redo.back().map(|entry| &entry.metadata)
    }

    pub(crate) fn undo_target(&self) -> Option<&S> {
        self.undo.back().map(|entry| &entry.before)
    }

    pub(crate) fn redo_target(&self) -> Option<&S> {
        self.redo.back().map(|entry| &entry.after)
    }

    pub(crate) fn record(&mut self, before: S, after: S, metadata: M) {
        self.redo.clear();
        if self.undo.len() == self.capacity {
            self.undo.pop_front();
        }
        self.undo.push_back(HistoryEntry {
            before,
            after,
            metadata,
        });
    }

    pub(crate) fn commit_undo(&mut self) {
        let entry = self
            .undo
            .pop_back()
            .expect("an inspected undo entry remains available until commit");
        self.redo.push_back(entry);
    }

    pub(crate) fn commit_redo(&mut self) {
        let entry = self
            .redo
            .pop_back()
            .expect("an inspected redo entry remains available until commit");
        self.undo.push_back(entry);
    }
}

/// Stable classification for one authored project mutation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ProjectMutationKind {
    /// Change one or more durable project settings atomically.
    ProjectSettings,
    /// Execute one ordered whole-project compound transaction.
    Compound,
    /// Replace the active referenced-media path.
    SetMediaPath,
    /// Mark one referenced-media path unavailable.
    MarkMediaMissing,
    /// Consider a fingerprint-checked relink candidate.
    ConsiderMediaRelink,
    /// Create or replace one durable extension record.
    UpsertExtension,
    /// Remove one durable extension record.
    RemoveExtension,
    /// Change durable extension lifecycle state.
    SetExtensionLifecycle,
    /// Change durable extension capability grants.
    SetExtensionCapabilities,
    /// Retain structured extension failure evidence.
    RecordExtensionFailure,
    /// Clear retained extension failure evidence.
    ClearExtensionFailure,
    /// Execute an extension operation introduced by a newer project crate.
    ExtensionState,
}

impl ProjectMutationKind {
    /// Returns the stable mutation code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::ProjectSettings => "project_settings",
            Self::Compound => "compound",
            Self::SetMediaPath => "set_media_path",
            Self::MarkMediaMissing => "mark_media_missing",
            Self::ConsiderMediaRelink => "consider_media_relink",
            Self::UpsertExtension => "upsert_extension",
            Self::RemoveExtension => "remove_extension",
            Self::SetExtensionLifecycle => "set_extension_lifecycle",
            Self::SetExtensionCapabilities => "set_extension_capabilities",
            Self::RecordExtensionFailure => "record_extension_failure",
            Self::ClearExtensionFailure => "clear_extension_failure",
            Self::ExtensionState => "extension_state",
        }
    }
}

/// One typed authored-state mutation accepted by project command history.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectMutation {
    /// Execute one ordered transaction across authored project subsystems.
    Compound(CompoundProjectTransaction),
    /// Execute a project media command through its checked document boundary.
    Media(ProjectMediaCommand),
    /// Execute a project extension-state command through its checked document boundary.
    Extension(ProjectExtensionCommand),
}

impl ProjectMutation {
    /// Creates an authored compound mutation.
    #[must_use]
    pub const fn compound(transaction: CompoundProjectTransaction) -> Self {
        Self::Compound(transaction)
    }

    /// Creates an authored media mutation.
    #[must_use]
    pub const fn media(command: ProjectMediaCommand) -> Self {
        Self::Media(command)
    }

    /// Creates an authored extension-state mutation.
    #[must_use]
    pub const fn extension(command: ProjectExtensionCommand) -> Self {
        Self::Extension(command)
    }

    /// Returns the stable mutation classification.
    #[must_use]
    pub const fn kind(&self) -> ProjectMutationKind {
        match self {
            Self::Compound(_) => ProjectMutationKind::Compound,
            Self::Media(ProjectMediaCommand::SetPath { .. }) => ProjectMutationKind::SetMediaPath,
            Self::Media(ProjectMediaCommand::MarkMissing { .. }) => {
                ProjectMutationKind::MarkMediaMissing
            }
            Self::Media(ProjectMediaCommand::ConsiderRelink { .. }) => {
                ProjectMutationKind::ConsiderMediaRelink
            }
            Self::Extension(command) => match command.operation() {
                ProjectExtensionOperation::Upsert => ProjectMutationKind::UpsertExtension,
                ProjectExtensionOperation::Remove => ProjectMutationKind::RemoveExtension,
                ProjectExtensionOperation::SetLifecycle => {
                    ProjectMutationKind::SetExtensionLifecycle
                }
                ProjectExtensionOperation::SetGrantedCapabilities => {
                    ProjectMutationKind::SetExtensionCapabilities
                }
                ProjectExtensionOperation::RecordFailure => {
                    ProjectMutationKind::RecordExtensionFailure
                }
                ProjectExtensionOperation::ClearFailure => {
                    ProjectMutationKind::ClearExtensionFailure
                }
                _ => ProjectMutationKind::ExtensionState,
            },
        }
    }
}

/// One history action under an exact project revision fence.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectHistoryAction {
    /// Apply a new authored mutation and clear redo only when state changes.
    Apply(ProjectMutation),
    /// Restore the latest retained before-state.
    Undo,
    /// Restore the latest retained after-state.
    Redo,
}

/// One typed project history command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectHistoryCommand {
    expected_revision: u64,
    action: ProjectHistoryAction,
}

/// One public-surface history or inspection command paired with durable request evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedProjectCommand {
    command: Option<ProjectHistoryCommand>,
    record: ProjectCommandRecordDraft,
}

impl RecordedProjectCommand {
    /// Pairs one mutation, undo, or redo command with its exact public request evidence.
    pub fn history(
        command: ProjectHistoryCommand,
        record: ProjectCommandRecordDraft,
    ) -> Result<Self> {
        if command.expected_revision() != record.expected_project_revision()
            || !matches!(
                (command.action(), record.command_kind()),
                (
                    ProjectHistoryAction::Apply(_),
                    ProjectCommandRecordKind::Apply
                ) | (ProjectHistoryAction::Undo, ProjectCommandRecordKind::Undo)
                    | (ProjectHistoryAction::Redo, ProjectCommandRecordKind::Redo)
            )
        {
            return Err(history_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "create_recorded_project_command",
                "recorded project command evidence does not match its history operation",
            ));
        }
        Ok(Self {
            command: Some(command),
            record,
        })
    }

    /// Pairs a revision-fenced inspection with exact public request evidence.
    pub fn inspect(expected_revision: u64, record: ProjectCommandRecordDraft) -> Result<Self> {
        if expected_revision != record.expected_project_revision()
            || record.command_kind() != ProjectCommandRecordKind::Inspect
        {
            return Err(history_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "create_recorded_project_inspection",
                "recorded project command evidence does not match its inspection fence",
            ));
        }
        Ok(Self {
            command: None,
            record,
        })
    }
}

impl ProjectHistoryCommand {
    /// Creates an authored mutation command.
    #[must_use]
    pub const fn apply(expected_revision: u64, mutation: ProjectMutation) -> Self {
        Self {
            expected_revision,
            action: ProjectHistoryAction::Apply(mutation),
        }
    }

    /// Creates an undo command.
    #[must_use]
    pub const fn undo(expected_revision: u64) -> Self {
        Self {
            expected_revision,
            action: ProjectHistoryAction::Undo,
        }
    }

    /// Creates a redo command.
    #[must_use]
    pub const fn redo(expected_revision: u64) -> Self {
        Self {
            expected_revision,
            action: ProjectHistoryAction::Redo,
        }
    }

    /// Returns the required current project revision.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Returns the requested history action.
    #[must_use]
    pub const fn action(&self) -> &ProjectHistoryAction {
        &self.action
    }
}

/// Semantic result of a successful history action.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectHistoryActionResult {
    /// One compound mutation was accepted as a single history entry.
    AppliedCompound {
        /// Semantic subsystem results in command order.
        actions: Vec<CompoundProjectActionResult>,
    },
    /// A typed mutation was accepted by its project owner.
    Applied {
        /// Stable mutation classification.
        mutation: ProjectMutationKind,
        /// Result returned by the project media command.
        media_result: ProjectMediaCommandResult,
    },
    /// A typed extension mutation was accepted by the project owner.
    AppliedExtension {
        /// Stable mutation classification.
        mutation: ProjectMutationKind,
        /// Result returned by the project extension command.
        extension_result: ProjectExtensionCommandResult,
    },
    /// A retained mutation was reversed.
    Undone(ProjectMutationKind),
    /// A retained mutation was reapplied.
    Redone(ProjectMutationKind),
}

/// Complete immutable public project history state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectHistoryState {
    snapshot: ProjectSnapshot,
    capacity: usize,
    undo_depth: usize,
    redo_depth: usize,
    next_undo: Option<ProjectMutationKind>,
    next_redo: Option<ProjectMutationKind>,
}

impl ProjectHistoryState {
    /// Returns the selected authored project snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &ProjectSnapshot {
        &self.snapshot
    }

    /// Returns the configured maximum retained entries per branch.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the number of actions available to undo.
    #[must_use]
    pub const fn undo_depth(&self) -> usize {
        self.undo_depth
    }

    /// Returns the number of actions available to redo.
    #[must_use]
    pub const fn redo_depth(&self) -> usize {
        self.redo_depth
    }

    /// Returns the next mutation available to undo.
    #[must_use]
    pub const fn next_undo(&self) -> Option<ProjectMutationKind> {
        self.next_undo
    }

    /// Returns the next mutation available to redo.
    #[must_use]
    pub const fn next_redo(&self) -> Option<ProjectMutationKind> {
        self.next_redo
    }
}

/// Project state and semantic evidence returned after one typed history command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectHistoryOutcome {
    state: ProjectHistoryState,
    action_result: Option<ProjectHistoryActionResult>,
    authored_state_changed: bool,
    command_record: Option<ProjectCommandRecord>,
}

impl ProjectHistoryOutcome {
    /// Returns the complete selected state and history metadata.
    #[must_use]
    pub const fn state(&self) -> &ProjectHistoryState {
        &self.state
    }

    /// Returns semantic action evidence, or `None` for an inspection.
    #[must_use]
    pub const fn action_result(&self) -> Option<&ProjectHistoryActionResult> {
        self.action_result.as_ref()
    }

    /// Reports whether the authored project state changed.
    #[must_use]
    pub const fn authored_state_changed(&self) -> bool {
        self.authored_state_changed
    }

    /// Returns the durable command record for stable-surface execution.
    #[must_use]
    pub const fn command_record(&self) -> Option<&ProjectCommandRecord> {
        self.command_record.as_ref()
    }
}

/// Exclusive mutable owner of one project document and its session history.
#[derive(Clone, Debug)]
pub struct ProjectCommandHistory {
    document: ProjectDocument,
    history: SnapshotHistory<ProjectSnapshot, ProjectMutationKind>,
}

impl ProjectCommandHistory {
    /// Creates history with the default bounded capacity.
    #[must_use]
    pub fn new(document: ProjectDocument) -> Self {
        Self {
            document,
            history: SnapshotHistory::new(DEFAULT_PROJECT_HISTORY_CAPACITY),
        }
    }

    /// Creates history with an explicitly bounded capacity.
    pub fn with_capacity(document: ProjectDocument, capacity: usize) -> Result<Self> {
        validate_capacity(capacity)?;
        Ok(Self {
            document,
            history: SnapshotHistory::new(capacity),
        })
    }

    /// Captures the selected project and all public history metadata.
    #[must_use]
    pub fn state(&self) -> ProjectHistoryState {
        ProjectHistoryState {
            snapshot: self.document.snapshot(),
            capacity: self.history.capacity(),
            undo_depth: self.history.undo_depth(),
            redo_depth: self.history.redo_depth(),
            next_undo: self.history.next_undo().copied(),
            next_redo: self.history.next_redo().copied(),
        }
    }

    /// Returns an immutable typed inspection outcome.
    #[must_use]
    pub fn inspect(&self) -> ProjectHistoryOutcome {
        ProjectHistoryOutcome {
            state: self.state(),
            action_result: None,
            authored_state_changed: false,
            command_record: None,
        }
    }

    /// Captures the selected authoritative project snapshot.
    #[must_use]
    pub(crate) fn project_snapshot(&self) -> ProjectSnapshot {
        self.document.snapshot()
    }

    /// Reports whether one valid settings transaction would publish a new project revision.
    pub(crate) fn settings_will_change(
        &self,
        transaction: &ProjectSettingsTransaction,
    ) -> Result<bool> {
        let prior_revision = self.document.revision();
        let mut candidate = self.document.clone();
        let snapshot = candidate.execute_settings_transaction(transaction.clone())?;
        Ok(snapshot.revision() != prior_revision)
    }

    /// Executes one settings transaction through the same document and history owner.
    pub(crate) fn execute_settings_transaction(
        &mut self,
        transaction: ProjectSettingsTransaction,
    ) -> Result<ProjectSnapshot> {
        let before = self.document.snapshot();
        let after = self.document.execute_settings_transaction(transaction)?;
        if after.revision() != before.revision() {
            self.history
                .record(before, after.clone(), ProjectMutationKind::ProjectSettings);
        }
        Ok(after)
    }

    /// Prepares one recovery replacement without changing live history state.
    pub(crate) fn prepare_recovery_restore(
        &self,
        expected_revision: u64,
        target: &ProjectSnapshot,
    ) -> Result<ProjectDocument> {
        require_revision(expected_revision, self.document.revision())?;
        let mut candidate = self.document.clone();
        candidate.restore_snapshot(expected_revision, target)?;
        Ok(candidate)
    }

    /// Commits an already validated and durably published recovery replacement.
    pub(crate) fn commit_recovery_restore(&mut self, document: ProjectDocument) {
        debug_assert_eq!(document.project_id(), self.document.project_id());
        let capacity = self.history.capacity();
        self.document = document;
        self.history = SnapshotHistory::new(capacity);
    }

    /// Executes one revision-fenced mutation, undo, or redo command atomically.
    pub fn execute(&mut self, command: ProjectHistoryCommand) -> Result<ProjectHistoryOutcome> {
        require_revision(command.expected_revision, self.document.revision())?;
        match command.action {
            ProjectHistoryAction::Apply(mutation) => {
                self.apply(command.expected_revision, mutation)
            }
            ProjectHistoryAction::Undo => self.undo(command.expected_revision),
            ProjectHistoryAction::Redo => self.redo(command.expected_revision),
        }
    }

    /// Atomically executes and records one successful stable-surface command.
    pub fn execute_recorded(
        &mut self,
        recorded: RecordedProjectCommand,
        command_sequence: u64,
    ) -> Result<ProjectHistoryOutcome> {
        let mut candidate = self.clone();
        let before = candidate.document.snapshot();
        let before_hash = ProjectDiagnostics::from_snapshot(&before)?
            .content_hash()
            .into_bytes();
        let mut outcome = match recorded.command {
            Some(command) => candidate.execute(command)?,
            None => {
                require_revision(
                    recorded.record.expected_project_revision(),
                    candidate.document.revision(),
                )?;
                candidate.inspect()
            }
        };
        let after = candidate.document.snapshot();
        let after_hash = ProjectDiagnostics::from_snapshot(&after)?
            .content_hash()
            .into_bytes();
        let command_record = candidate.document.append_command_record(
            recorded.record,
            command_sequence,
            before.revision(),
            after.revision(),
            before_hash,
            after_hash,
            outcome.authored_state_changed,
        )?;
        outcome.state = candidate.state();
        outcome.command_record = Some(command_record);
        *self = candidate;
        Ok(outcome)
    }

    fn apply(
        &mut self,
        expected_revision: u64,
        mutation: ProjectMutation,
    ) -> Result<ProjectHistoryOutcome> {
        let kind = mutation.kind();
        let before = self.document.snapshot();
        let action_result = match mutation {
            ProjectMutation::Compound(transaction) => {
                let outcome = execute_compound_project_transaction(
                    &mut self.document,
                    expected_revision,
                    &transaction,
                )?;
                ProjectHistoryActionResult::AppliedCompound {
                    actions: outcome.actions().to_vec(),
                }
            }
            ProjectMutation::Media(command) => {
                let media_result = self
                    .document
                    .execute_media_command(expected_revision, command)?
                    .result();
                ProjectHistoryActionResult::Applied {
                    mutation: kind,
                    media_result,
                }
            }
            ProjectMutation::Extension(command) => {
                let extension_result = self
                    .document
                    .execute_extension_command(expected_revision, command)?
                    .result()
                    .clone();
                ProjectHistoryActionResult::AppliedExtension {
                    mutation: kind,
                    extension_result,
                }
            }
        };
        let after = self.document.snapshot();
        let authored_state_changed = after.revision() != before.revision();
        if authored_state_changed {
            self.history.record(before, after, kind);
        }
        Ok(ProjectHistoryOutcome {
            state: self.state(),
            action_result: Some(action_result),
            authored_state_changed,
            command_record: None,
        })
    }

    fn undo(&mut self, expected_revision: u64) -> Result<ProjectHistoryOutcome> {
        let target = self.history.undo_target().cloned().ok_or_else(|| {
            history_conflict(
                "undo_project_history",
                "no project mutation is available to undo",
            )
        })?;
        let kind = self
            .history
            .next_undo()
            .copied()
            .expect("an available undo target retains mutation metadata");
        self.document.restore_snapshot(expected_revision, &target)?;
        self.history.commit_undo();
        Ok(ProjectHistoryOutcome {
            state: self.state(),
            action_result: Some(ProjectHistoryActionResult::Undone(kind)),
            authored_state_changed: true,
            command_record: None,
        })
    }

    fn redo(&mut self, expected_revision: u64) -> Result<ProjectHistoryOutcome> {
        let target = self.history.redo_target().cloned().ok_or_else(|| {
            history_conflict(
                "redo_project_history",
                "no project mutation is available to redo",
            )
        })?;
        let kind = self
            .history
            .next_redo()
            .copied()
            .expect("an available redo target retains mutation metadata");
        self.document.restore_snapshot(expected_revision, &target)?;
        self.history.commit_redo();
        Ok(ProjectHistoryOutcome {
            state: self.state(),
            action_result: Some(ProjectHistoryActionResult::Redone(kind)),
            authored_state_changed: true,
            command_record: None,
        })
    }
}

fn validate_capacity(capacity: usize) -> Result<()> {
    if capacity == 0 {
        return Err(history_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "create_project_history",
            "project history capacity must be greater than zero",
        ));
    }
    if capacity > MAX_PROJECT_HISTORY_CAPACITY {
        return Err(history_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            "create_project_history",
            "project history capacity exceeds the supported bound",
        ));
    }
    Ok(())
}

fn require_revision(expected_revision: u64, actual_revision: u64) -> Result<()> {
    if expected_revision == actual_revision {
        return Ok(());
    }
    Err(history_conflict(
        "execute_project_history",
        "project history command revision is stale",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "execute_project_history")
            .with_field("expected_revision", expected_revision.to_string())
            .with_field("actual_revision", actual_revision.to_string()),
    ))
}

fn history_conflict(operation: &'static str, message: &'static str) -> Error {
    history_error(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}

fn history_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
