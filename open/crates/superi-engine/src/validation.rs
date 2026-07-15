//! Coherent nonblocking integration validation over dispatcher-owned engine state.
//!
//! The validator projects canonical scenario, lifecycle, recovery, playback, and export snapshots.
//! It never polls workers, acquires runtime locks, or becomes another mutable state authority.

use superi_concurrency::lifecycle::LifecyclePhase;

use crate::command::ScenarioSnapshot;
use crate::dispatcher::{EngineRecoveryState, EngineReportedFailure};
use crate::export_dispatch::EngineExportJobState;
use crate::introspection::EngineIntrospectionSnapshot;
use crate::lifecycle::{
    EngineHealth, EngineLifecycleActionKind, EngineLifecycleSnapshot, EngineSubsystemState,
    EngineWorkAdmission, EngineWorkKind,
};
use crate::transport::PlaybackTransportSnapshot;

/// Stable high-level condition of the integrated engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EngineIntegrationCondition {
    /// Initial subsystem resources are being acquired.
    Starting,
    /// All admitted workflows are healthy and no recovery is active.
    Normal,
    /// One or more failures limit only affected workflows.
    Degraded,
    /// An exact classified recovery action is active.
    Recovering,
    /// Work is reaching a resumable pause boundary.
    Pausing,
    /// Work is quiescent while resumable resources remain owned.
    Paused,
    /// Paused work is reacquiring its running state.
    Resuming,
    /// Work is quiescing and releasing resources that cannot survive sleep.
    PreparingSleep,
    /// The engine is prepared for system sleep.
    Sleeping,
    /// Resources are being revalidated after system wake.
    Waking,
    /// Subsystems are releasing resources in reverse dependency order.
    Stopping,
    /// Every subsystem completed teardown.
    Stopped,
    /// A lifecycle or terminal subsystem failure prevents coherent work.
    Failed,
}

impl EngineIntegrationCondition {
    /// Returns the stable diagnostic and API code for this condition.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Normal => "normal",
            Self::Degraded => "degraded",
            Self::Recovering => "recovering",
            Self::Pausing => "pausing",
            Self::Paused => "paused",
            Self::Resuming => "resuming",
            Self::PreparingSleep => "preparing_sleep",
            Self::Sleeping => "sleeping",
            Self::Waking => "waking",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }
}

/// Stable coherence failure detected while projecting canonical state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EngineIntegrationFindingCode {
    /// The canonical introspection revision or lifecycle state differs from validation state.
    IntrospectionLifecycleMismatch,
    /// Canonical introspection and exact workflow admission disagree.
    IntrospectionWorkflowMismatch,
    /// Canonical introspection and classified recovery state disagree.
    IntrospectionRecoveryMismatch,
    /// Recovery state contains a lifecycle snapshot different from the dispatcher snapshot.
    RecoveryLifecycleMismatch,
    /// An active recovery token does not match the lifecycle's exact recovery action.
    RecoveryActionMismatch,
    /// Playback state exists or is pending without an attached playback bridge.
    PlaybackAttachmentMismatch,
    /// Export state exists without an attached export owner.
    ExportAttachmentMismatch,
}

impl EngineIntegrationFindingCode {
    /// Returns the stable diagnostic and API code for this finding.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::IntrospectionLifecycleMismatch => "introspection_lifecycle_mismatch",
            Self::IntrospectionWorkflowMismatch => "introspection_workflow_mismatch",
            Self::IntrospectionRecoveryMismatch => "introspection_recovery_mismatch",
            Self::RecoveryLifecycleMismatch => "recovery_lifecycle_mismatch",
            Self::RecoveryActionMismatch => "recovery_action_mismatch",
            Self::PlaybackAttachmentMismatch => "playback_attachment_mismatch",
            Self::ExportAttachmentMismatch => "export_attachment_mismatch",
        }
    }
}

/// One deterministic integration coherence finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineIntegrationFinding {
    code: EngineIntegrationFindingCode,
    message: &'static str,
}

impl EngineIntegrationFinding {
    /// Returns the stable machine-readable finding code.
    #[must_use]
    pub const fn code(&self) -> EngineIntegrationFindingCode {
        self.code
    }

    /// Returns the concise invariant failure.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

/// Current admission or exact denial for one integrated workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineWorkflowValidation {
    work: EngineWorkKind,
    admission: EngineWorkAdmission,
}

impl EngineWorkflowValidation {
    /// Returns playback, rendering, or export workflow identity.
    #[must_use]
    pub const fn work(&self) -> EngineWorkKind {
        self.work
    }

    /// Returns a revision-scoped permit or exact blocking evidence.
    #[must_use]
    pub const fn admission(&self) -> &EngineWorkAdmission {
        &self.admission
    }
}

/// Latest accepted playback replacement state retained by the dispatcher.
#[derive(Clone, Debug, PartialEq)]
pub struct EnginePlaybackValidationState {
    attached: bool,
    command_pending: bool,
    latest_snapshot: Option<PlaybackTransportSnapshot>,
    latest_failure: Option<EngineReportedFailure>,
}

impl EnginePlaybackValidationState {
    /// Returns whether the dispatcher has an attached playback-domain bridge.
    #[must_use]
    pub const fn is_attached(&self) -> bool {
        self.attached
    }

    /// Returns whether one accepted command still awaits its playback-domain completion.
    #[must_use]
    pub const fn command_pending(&self) -> bool {
        self.command_pending
    }

    /// Returns the latest complete transport state observed from the playback owner.
    #[must_use]
    pub const fn latest_snapshot(&self) -> Option<PlaybackTransportSnapshot> {
        self.latest_snapshot
    }

    /// Returns the failure paired with the latest transport replacement state.
    #[must_use]
    pub const fn latest_failure(&self) -> Option<&EngineReportedFailure> {
        self.latest_failure.as_ref()
    }
}

/// Latest full export queue state retained by the dispatcher.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineExportValidationState {
    attached: bool,
    latest_state: Option<EngineExportJobState>,
}

impl EngineExportValidationState {
    /// Returns whether the dispatcher owns an attached logical export queue.
    #[must_use]
    pub const fn is_attached(&self) -> bool {
        self.attached
    }

    /// Returns the latest complete export queue replacement state.
    #[must_use]
    pub const fn latest_state(&self) -> Option<&EngineExportJobState> {
        self.latest_state.as_ref()
    }
}

/// Complete immutable integration state used by CLI, UI, and contract tests.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineIntegrationValidationSnapshot {
    condition: EngineIntegrationCondition,
    introspection: EngineIntrospectionSnapshot,
    scenario: ScenarioSnapshot,
    lifecycle: EngineLifecycleSnapshot,
    recovery: EngineRecoveryState,
    workflows: [EngineWorkflowValidation; 3],
    playback: EnginePlaybackValidationState,
    export: EngineExportValidationState,
    findings: Vec<EngineIntegrationFinding>,
}

impl EngineIntegrationValidationSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        introspection: EngineIntrospectionSnapshot,
        scenario: ScenarioSnapshot,
        lifecycle: EngineLifecycleSnapshot,
        recovery: EngineRecoveryState,
        playback_attached: bool,
        playback_pending: bool,
        latest_playback: Option<(PlaybackTransportSnapshot, Option<EngineReportedFailure>)>,
        export_attached: bool,
        latest_export: Option<EngineExportJobState>,
    ) -> Self {
        let mut findings = Vec::new();
        if !introspection_matches_lifecycle(&introspection, &lifecycle) {
            findings.push(EngineIntegrationFinding {
                code: EngineIntegrationFindingCode::IntrospectionLifecycleMismatch,
                message: "engine introspection and validation do not name the same lifecycle state",
            });
        }
        if !introspection_matches_workflows(&introspection, &lifecycle) {
            findings.push(EngineIntegrationFinding {
                code: EngineIntegrationFindingCode::IntrospectionWorkflowMismatch,
                message: "engine introspection and exact workflow admission disagree",
            });
        }
        if !introspection_matches_recovery(&introspection, &recovery) {
            findings.push(EngineIntegrationFinding {
                code: EngineIntegrationFindingCode::IntrospectionRecoveryMismatch,
                message: "engine introspection and classified recovery state disagree",
            });
        }
        if recovery.lifecycle() != &lifecycle {
            findings.push(EngineIntegrationFinding {
                code: EngineIntegrationFindingCode::RecoveryLifecycleMismatch,
                message: "recovery and lifecycle projections do not name the same engine state",
            });
        }
        if let Some(recovery_action) = recovery.pending_recovery() {
            let matches_lifecycle = lifecycle.pending_action().is_some_and(|action| {
                action.subsystem() == recovery_action.subsystem()
                    && action.kind() == EngineLifecycleActionKind::Recover
            });
            if !matches_lifecycle {
                findings.push(EngineIntegrationFinding {
                    code: EngineIntegrationFindingCode::RecoveryActionMismatch,
                    message: "classified recovery does not match the pending lifecycle action",
                });
            }
        }
        if !playback_attached && (playback_pending || latest_playback.is_some()) {
            findings.push(EngineIntegrationFinding {
                code: EngineIntegrationFindingCode::PlaybackAttachmentMismatch,
                message: "playback state exists without an attached playback bridge",
            });
        }
        if !export_attached && latest_export.is_some() {
            findings.push(EngineIntegrationFinding {
                code: EngineIntegrationFindingCode::ExportAttachmentMismatch,
                message: "export state exists without an attached export owner",
            });
        }

        let condition = condition(&lifecycle, &recovery);
        let workflows = [
            workflow(&lifecycle, EngineWorkKind::Playback),
            workflow(&lifecycle, EngineWorkKind::Rendering),
            workflow(&lifecycle, EngineWorkKind::Export),
        ];
        let (latest_snapshot, latest_failure) = match latest_playback {
            Some((snapshot, failure)) => (Some(snapshot), failure),
            None => (None, None),
        };
        Self {
            condition,
            introspection,
            scenario,
            lifecycle,
            recovery,
            workflows,
            playback: EnginePlaybackValidationState {
                attached: playback_attached,
                command_pending: playback_pending,
                latest_snapshot,
                latest_failure,
            },
            export: EngineExportValidationState {
                attached: export_attached,
                latest_state: latest_export,
            },
            findings,
        }
    }

    /// Returns the high-level normal, degraded, recovery, or lifecycle condition.
    #[must_use]
    pub const fn condition(&self) -> EngineIntegrationCondition {
        self.condition
    }

    /// Reports whether every cross-subsystem ownership invariant is coherent.
    #[must_use]
    pub fn is_coherent(&self) -> bool {
        self.findings.is_empty()
    }

    /// Returns the canonical capability, health, subsystem, workflow, failure, and resource view.
    #[must_use]
    pub const fn introspection(&self) -> &EngineIntrospectionSnapshot {
        &self.introspection
    }

    /// Returns the authoritative editable scenario state.
    #[must_use]
    pub const fn scenario(&self) -> &ScenarioSnapshot {
        &self.scenario
    }

    /// Returns the authoritative lifecycle and subsystem state.
    #[must_use]
    pub const fn lifecycle(&self) -> &EngineLifecycleSnapshot {
        &self.lifecycle
    }

    /// Returns classified failure, recovery, and diagnostic state.
    #[must_use]
    pub const fn recovery(&self) -> &EngineRecoveryState {
        &self.recovery
    }

    /// Returns validation state for one integrated workflow.
    #[must_use]
    pub fn workflow(&self, work: EngineWorkKind) -> Option<&EngineWorkflowValidation> {
        self.workflows.iter().find(|state| state.work == work)
    }

    /// Returns workflow validation in playback, rendering, and export order.
    #[must_use]
    pub const fn workflows(&self) -> &[EngineWorkflowValidation; 3] {
        &self.workflows
    }

    /// Returns attached, pending, and latest observed playback state.
    #[must_use]
    pub const fn playback(&self) -> &EnginePlaybackValidationState {
        &self.playback
    }

    /// Returns attached and latest observed export queue state.
    #[must_use]
    pub const fn export(&self) -> &EngineExportValidationState {
        &self.export
    }

    /// Returns every deterministic coherence failure.
    #[must_use]
    pub fn findings(&self) -> &[EngineIntegrationFinding] {
        &self.findings
    }
}

fn workflow(lifecycle: &EngineLifecycleSnapshot, work: EngineWorkKind) -> EngineWorkflowValidation {
    EngineWorkflowValidation {
        work,
        admission: lifecycle.admit(work),
    }
}

fn introspection_matches_lifecycle(
    introspection: &EngineIntrospectionSnapshot,
    lifecycle: &EngineLifecycleSnapshot,
) -> bool {
    introspection.lifecycle_revision() == lifecycle.lifecycle_revision()
        && introspection.state_revision() == lifecycle.state_revision()
        && introspection.lifetime() == lifecycle.lifetime()
        && introspection.health() == lifecycle.health()
        && introspection.subsystems().len() == lifecycle.subsystems().len()
        && introspection
            .subsystems()
            .iter()
            .zip(lifecycle.subsystems())
            .all(|(observed, canonical)| {
                observed.subsystem() == canonical.subsystem()
                    && observed.state() == canonical.state()
            })
}

fn introspection_matches_workflows(
    introspection: &EngineIntrospectionSnapshot,
    lifecycle: &EngineLifecycleSnapshot,
) -> bool {
    introspection.workflows().iter().all(|observed| {
        let admission = lifecycle.admit(observed.work());
        observed.available() == admission.permit().is_some()
            && observed.blocking_subsystem()
                == admission
                    .denial()
                    .and_then(|denial| denial.blocking_subsystem())
    })
}

fn introspection_matches_recovery(
    introspection: &EngineIntrospectionSnapshot,
    recovery: &EngineRecoveryState,
) -> bool {
    introspection.recovery_revision() == recovery.revision()
        && introspection.active_failures().len() == recovery.active_failures().len()
        && introspection
            .active_failures()
            .iter()
            .zip(recovery.active_failures())
            .all(|(observed, canonical)| observed.subsystem() == canonical.subsystem())
}

fn condition(
    lifecycle: &EngineLifecycleSnapshot,
    recovery: &EngineRecoveryState,
) -> EngineIntegrationCondition {
    let recovering = recovery.pending_recovery().is_some()
        || lifecycle
            .subsystems()
            .iter()
            .any(|subsystem| subsystem.state() == EngineSubsystemState::Recovering);
    if recovering {
        return EngineIntegrationCondition::Recovering;
    }
    match lifecycle.phase() {
        LifecyclePhase::Starting => EngineIntegrationCondition::Starting,
        LifecyclePhase::Running => match lifecycle.health() {
            EngineHealth::Healthy => EngineIntegrationCondition::Normal,
            EngineHealth::Degraded => EngineIntegrationCondition::Degraded,
            EngineHealth::Failed => EngineIntegrationCondition::Failed,
        },
        LifecyclePhase::Pausing => EngineIntegrationCondition::Pausing,
        LifecyclePhase::Paused => EngineIntegrationCondition::Paused,
        LifecyclePhase::Resuming => EngineIntegrationCondition::Resuming,
        LifecyclePhase::PreparingSleep => EngineIntegrationCondition::PreparingSleep,
        LifecyclePhase::Sleeping => EngineIntegrationCondition::Sleeping,
        LifecyclePhase::Waking => EngineIntegrationCondition::Waking,
        LifecyclePhase::Stopping => EngineIntegrationCondition::Stopping,
        LifecyclePhase::Stopped => EngineIntegrationCondition::Stopped,
        LifecyclePhase::Failed => EngineIntegrationCondition::Failed,
        _ => EngineIntegrationCondition::Failed,
    }
}
