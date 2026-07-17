//! Bounded engine-owned command history for authored project state.
//!
//! History is session-local. Each entry retains immutable before and after snapshots, while the
//! selected snapshot remains the only state persisted by the project layer.

use std::collections::VecDeque;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_project::document::{ProjectDocument, ProjectSnapshot};
use superi_project::media::{ProjectMediaCommand, ProjectMediaCommandResult};

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
#[derive(Debug)]
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
    /// Replace the active referenced-media path.
    SetMediaPath,
    /// Mark one referenced-media path unavailable.
    MarkMediaMissing,
    /// Consider a fingerprint-checked relink candidate.
    ConsiderMediaRelink,
}

impl ProjectMutationKind {
    /// Returns the stable mutation code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::SetMediaPath => "set_media_path",
            Self::MarkMediaMissing => "mark_media_missing",
            Self::ConsiderMediaRelink => "consider_media_relink",
        }
    }
}

/// One typed authored-state mutation accepted by project command history.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectMutation {
    /// Execute a project media command through its checked document boundary.
    Media(ProjectMediaCommand),
}

impl ProjectMutation {
    /// Creates an authored media mutation.
    #[must_use]
    pub const fn media(command: ProjectMediaCommand) -> Self {
        Self::Media(command)
    }

    /// Returns the stable mutation classification.
    #[must_use]
    pub const fn kind(&self) -> ProjectMutationKind {
        match self {
            Self::Media(ProjectMediaCommand::SetPath { .. }) => ProjectMutationKind::SetMediaPath,
            Self::Media(ProjectMediaCommand::MarkMissing { .. }) => {
                ProjectMutationKind::MarkMediaMissing
            }
            Self::Media(ProjectMediaCommand::ConsiderRelink { .. }) => {
                ProjectMutationKind::ConsiderMediaRelink
            }
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
    /// A typed mutation was accepted by its project owner.
    Applied {
        /// Stable mutation classification.
        mutation: ProjectMutationKind,
        /// Result returned by the project media command.
        media_result: ProjectMediaCommandResult,
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
}

/// Exclusive mutable owner of one project document and its session history.
#[derive(Debug)]
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
        }
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

    fn apply(
        &mut self,
        expected_revision: u64,
        mutation: ProjectMutation,
    ) -> Result<ProjectHistoryOutcome> {
        let kind = mutation.kind();
        let before = self.document.snapshot();
        let media_result = match mutation {
            ProjectMutation::Media(command) => self
                .document
                .execute_media_command(expected_revision, command)?
                .result(),
        };
        let after = self.document.snapshot();
        let authored_state_changed = after.revision() != before.revision();
        if authored_state_changed {
            self.history.record(before, after, kind);
        }
        Ok(ProjectHistoryOutcome {
            state: self.state(),
            action_result: Some(ProjectHistoryActionResult::Applied {
                mutation: kind,
                media_result,
            }),
            authored_state_changed,
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
