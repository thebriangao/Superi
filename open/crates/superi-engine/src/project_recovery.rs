//! Engine-owned crash recovery transaction coordination.
//!
//! The project layer owns discovery and exact recovery files. This module binds
//! that store to the authoritative command history and file-backed database so
//! a restore publishes durable state before the infallible in-memory swap.

use std::path::Path;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::ProjectId;
use superi_project::document::ProjectSnapshot;
pub use superi_project::recovery::{ProjectRecoveryCandidateId, ProjectRecoveryComparison};
use superi_project::recovery::{ProjectRecoveryCatalog, ProjectRecoveryController};
use superi_project::ProjectDatabase;

use crate::history::{ProjectCommandHistory, ProjectHistoryState};

const COMPONENT: &str = "superi-engine.project-recovery";

/// Stable semantic recovery operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectRecoveryOperation {
    /// Refresh the exact managed candidate catalog.
    Discover,
    /// Compare one candidate with current authored state.
    Compare,
    /// Restore one candidate through durable project publication.
    Restore,
    /// Durably dismiss one managed entry.
    Dismiss,
}

/// One typed recovery command accepted by the engine owner.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectRecoveryCommand {
    /// Refresh restart-discoverable recovery state.
    Discover,
    /// Compare one candidate under an exact catalog revision.
    Compare {
        expected_catalog_revision: u64,
        candidate_id: ProjectRecoveryCandidateId,
    },
    /// Restore one candidate under exact catalog and project revisions.
    Restore {
        expected_catalog_revision: u64,
        expected_project_revision: u64,
        candidate_id: ProjectRecoveryCandidateId,
    },
    /// Dismiss one candidate under an exact catalog revision.
    Dismiss {
        expected_catalog_revision: u64,
        candidate_id: ProjectRecoveryCandidateId,
    },
}

impl ProjectRecoveryCommand {
    #[must_use]
    pub const fn discover() -> Self {
        Self::Discover
    }

    #[must_use]
    pub const fn compare(
        expected_catalog_revision: u64,
        candidate_id: ProjectRecoveryCandidateId,
    ) -> Self {
        Self::Compare {
            expected_catalog_revision,
            candidate_id,
        }
    }

    #[must_use]
    pub const fn restore(
        expected_catalog_revision: u64,
        expected_project_revision: u64,
        candidate_id: ProjectRecoveryCandidateId,
    ) -> Self {
        Self::Restore {
            expected_catalog_revision,
            expected_project_revision,
            candidate_id,
        }
    }

    #[must_use]
    pub const fn dismiss(
        expected_catalog_revision: u64,
        candidate_id: ProjectRecoveryCandidateId,
    ) -> Self {
        Self::Dismiss {
            expected_catalog_revision,
            candidate_id,
        }
    }

    /// Returns the stable semantic operation.
    #[must_use]
    pub const fn operation(&self) -> ProjectRecoveryOperation {
        match self {
            Self::Discover => ProjectRecoveryOperation::Discover,
            Self::Compare { .. } => ProjectRecoveryOperation::Compare,
            Self::Restore { .. } => ProjectRecoveryOperation::Restore,
            Self::Dismiss { .. } => ProjectRecoveryOperation::Dismiss,
        }
    }
}

/// Complete replacement recovery state beside the authoritative project history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecoveryState {
    project_id: ProjectId,
    history: ProjectHistoryState,
    catalog: ProjectRecoveryCatalog,
}

impl ProjectRecoveryState {
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.history.snapshot().revision()
    }

    #[must_use]
    pub const fn snapshot(&self) -> &ProjectSnapshot {
        self.history.snapshot()
    }

    #[must_use]
    pub const fn history(&self) -> &ProjectHistoryState {
        &self.history
    }

    #[must_use]
    pub const fn catalog(&self) -> &ProjectRecoveryCatalog {
        &self.catalog
    }
}

/// Typed result after one successful project recovery command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecoveryOutcome {
    operation: ProjectRecoveryOperation,
    state: ProjectRecoveryState,
    comparison: Option<ProjectRecoveryComparison>,
    restored: bool,
    dismissed: bool,
}

impl ProjectRecoveryOutcome {
    #[must_use]
    pub const fn operation(&self) -> ProjectRecoveryOperation {
        self.operation
    }

    #[must_use]
    pub const fn state(&self) -> &ProjectRecoveryState {
        &self.state
    }

    #[must_use]
    pub const fn comparison(&self) -> Option<&ProjectRecoveryComparison> {
        self.comparison.as_ref()
    }

    #[must_use]
    pub const fn restored(&self) -> bool {
        self.restored
    }

    #[must_use]
    pub const fn dismissed(&self) -> bool {
        self.dismissed
    }
}

/// Durable project recovery authority attached to one engine dispatcher.
#[derive(Debug)]
pub(crate) struct ProjectRecoveryCoordinator {
    database: ProjectDatabase,
    recovery: ProjectRecoveryController,
}

impl ProjectRecoveryCoordinator {
    pub(crate) fn new(
        database: ProjectDatabase,
        recovery_root: impl AsRef<Path>,
        current: &ProjectSnapshot,
    ) -> Result<Self> {
        if database.active_path().is_none() {
            return Err(recovery_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "attach_recovery",
                "project recovery requires a file-backed active project database",
            ));
        }
        let persisted = database.load()?.snapshot();
        if persisted != *current {
            return Err(recovery_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "attach_recovery",
                "active project database does not match the attached project state",
            ));
        }
        Ok(Self {
            database,
            recovery: ProjectRecoveryController::new(current.project_id(), recovery_root)?,
        })
    }

    pub(crate) fn execute(
        &mut self,
        history: &mut ProjectCommandHistory,
        command: ProjectRecoveryCommand,
    ) -> Result<ProjectRecoveryOutcome> {
        let operation = command.operation();
        match command {
            ProjectRecoveryCommand::Discover => {
                self.recovery.discover()?;
                Ok(ProjectRecoveryOutcome {
                    operation,
                    state: self.state(history),
                    comparison: None,
                    restored: false,
                    dismissed: false,
                })
            }
            ProjectRecoveryCommand::Compare {
                expected_catalog_revision,
                candidate_id,
            } => {
                let current = history.project_snapshot();
                let comparison =
                    self.recovery
                        .compare(expected_catalog_revision, candidate_id, &current)?;
                Ok(ProjectRecoveryOutcome {
                    operation,
                    state: self.state(history),
                    comparison: Some(comparison),
                    restored: false,
                    dismissed: false,
                })
            }
            ProjectRecoveryCommand::Restore {
                expected_catalog_revision,
                expected_project_revision,
                candidate_id,
            } => {
                let current = history.project_snapshot();
                let target = self.recovery.load_candidate_for_restore(
                    expected_catalog_revision,
                    candidate_id,
                    &current,
                )?;
                let comparison =
                    self.recovery
                        .compare(expected_catalog_revision, candidate_id, &current)?;
                let candidate =
                    history.prepare_recovery_restore(expected_project_revision, &target)?;
                let restored = candidate.revision() != current.revision();
                if restored {
                    self.database.replace(&candidate.snapshot())?;
                    history.commit_recovery_restore(candidate);
                }
                Ok(ProjectRecoveryOutcome {
                    operation,
                    state: self.state(history),
                    comparison: Some(comparison),
                    restored,
                    dismissed: false,
                })
            }
            ProjectRecoveryCommand::Dismiss {
                expected_catalog_revision,
                candidate_id,
            } => {
                self.recovery
                    .dismiss(expected_catalog_revision, candidate_id)?;
                Ok(ProjectRecoveryOutcome {
                    operation,
                    state: self.state(history),
                    comparison: None,
                    restored: false,
                    dismissed: true,
                })
            }
        }
    }

    fn state(&self, history: &ProjectCommandHistory) -> ProjectRecoveryState {
        let history = history.state();
        ProjectRecoveryState {
            project_id: history.snapshot().project_id(),
            history,
            catalog: self.recovery.catalog().clone(),
        }
    }
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
