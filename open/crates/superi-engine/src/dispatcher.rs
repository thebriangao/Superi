//! Single-owner command, transaction, and state-event dispatch for the coherent engine.
//!
//! The full dispatcher owns the canonical lifecycle and requires the EngineControl execution
//! domain. A scenario-only constructor preserves the existing in-process reference scenario while
//! routing it through the same transaction and event machinery. A bounded in-process bridge routes
//! transport control to Playback, while wire envelopes, subscriptions, and process bridges remain
//! API and shell concerns.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError, TrySendError};
use std::time::Instant;

pub use superi_audio::automation::{
    AudioAutomationActivePassSnapshot, AudioAutomationKeyframe, AudioAutomationLaneSnapshot,
    AudioAutomationMode, AudioAutomationMutation, AudioAutomationSnapshot, AudioAutomationState,
    AudioAutomationTarget, AudioAutomationTransaction, MAX_AUDIO_AUTOMATION_KEYFRAMES,
    MAX_AUDIO_AUTOMATION_LANES, MAX_AUDIO_AUTOMATION_MUTATIONS,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::diagnostics::DiagnosticEvent;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_project::document::{ProjectDocument, ProjectSnapshot};
use superi_project::{ProjectDatabase, ProjectDiagnostics};

use crate::command::{
    ScenarioAction, ScenarioEngine, ScenarioSnapshot, MAX_SCENARIO_TRANSACTION_ACTIONS,
};
use crate::editor_state::{
    EditorExportState, EditorPlaybackObservation, EditorPlaybackState, EditorStateAvailability,
    EditorStateSnapshot,
};
use crate::error::{
    EngineErrorCoordinator, EngineFailureRecord, EngineFailureSequence, EngineRecoveryAction,
    EngineRecoveryRequest, MAX_FAILURE_OPERATION_BYTES,
};
use crate::export_dispatch::{
    create_export_runtime, EngineExportJobCommand, EngineExportJobRuntime, EngineExportJobState,
    ErasedExportJobController,
};
use crate::export_jobs::ExportJobQueueConfig;
use crate::history::{
    ProjectCommandHistory, ProjectHistoryCommand, ProjectHistoryOutcome, ProjectHistoryState,
};
use crate::introspection::{EngineIntrospectionSnapshot, MediaCapabilities};
use crate::lifecycle::{
    EngineLifecycle, EngineLifecycleAction, EngineLifecycleActionKind, EngineLifecycleSnapshot,
    EngineSubsystem, EngineWorkAdmission, EngineWorkKind, EngineWorkPermit,
};
use crate::project_recovery::{
    ProjectRecoveryCommand, ProjectRecoveryCoordinator, ProjectRecoveryOutcome,
    ProjectRecoveryState,
};
use crate::project_settings::{ProjectSettingsState, ProjectSettingsTransaction};
use crate::resource_arbitration::ResourceArbitrationSnapshot;
use crate::transport::{PlaybackTransport, PlaybackTransportCommand, PlaybackTransportSnapshot};
use crate::validation::EngineIntegrationValidationSnapshot;

const COMPONENT: &str = "superi-engine.dispatcher";

/// Maximum UTF-8 bytes accepted in a caller-owned transaction identifier.
pub const MAX_TRANSACTION_ID_BYTES: usize = 128;
/// Maximum state events retained until the owning consumer drains them.
pub const MAX_PENDING_ENGINE_EVENTS: usize = 64;
/// Maximum context frames retained on one subsystem-reported failure command.
pub const MAX_REPORTED_FAILURE_CONTEXTS: usize = 16;
/// Maximum UTF-8 bytes accepted in one subsystem-reported failure summary.
pub const MAX_REPORTED_FAILURE_MESSAGE_BYTES: usize = 1024;
/// Maximum playback commands that may wait between the serialized control and playback owners.
pub const MAX_PENDING_PLAYBACK_COMMANDS: usize = 1;

/// Caller-owned stable identity for one serialized engine command transaction.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EngineTransactionId(String);

impl EngineTransactionId {
    /// Creates a bounded nonempty transaction identifier.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the identifier is empty, only whitespace, or contains a control
    /// character. Returns resource exhaustion when it exceeds [`MAX_TRANSACTION_ID_BYTES`].
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(invalid(
                "validate_transaction_id",
                "engine transaction identifier must not be empty",
            ));
        }
        if value.len() > MAX_TRANSACTION_ID_BYTES {
            return Err(resource_exhausted(
                "validate_transaction_id",
                "engine transaction identifier exceeds the byte bound",
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(invalid(
                "validate_transaction_id",
                "engine transaction identifier must not contain control characters",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the exact caller-owned identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Bounded stable label for one subsystem operation that may report a classified failure.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EngineFailureOperation(String);

impl EngineFailureOperation {
    /// Creates a nonempty operation label accepted by the canonical error coordinator.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the label is empty or only whitespace. Returns resource
    /// exhaustion when it exceeds the coordinator's byte bound.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() {
            return Err(invalid(
                "validate_failure_operation",
                "engine failure operation must not be empty",
            ));
        }
        if value.len() > MAX_FAILURE_OPERATION_BYTES {
            return Err(resource_exhausted(
                "validate_failure_operation",
                "engine failure operation exceeds the byte bound",
            ));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the validated operation label.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn into_string(self) -> String {
        self.0
    }
}

/// One optimistic, ordered scenario transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioTransaction {
    expected_revision: u64,
    actions: Vec<ScenarioAction>,
}

impl ScenarioTransaction {
    /// Creates a transaction fenced to one exact public scenario revision.
    ///
    /// This constructor rejects invalid shapes early, and [`ScenarioEngine`] revalidates them as the
    /// authoritative mutation owner so direct and dispatched callers share one contract.
    pub fn new(expected_revision: u64, actions: Vec<ScenarioAction>) -> Result<Self> {
        if actions.is_empty() {
            return Err(invalid(
                "validate_scenario_transaction",
                "scenario transaction must contain at least one action",
            ));
        }
        if actions.len() > MAX_SCENARIO_TRANSACTION_ACTIONS {
            return Err(resource_exhausted(
                "validate_scenario_transaction",
                "scenario transaction exceeds the action bound",
            ));
        }
        if actions.len() > 1
            && actions
                .iter()
                .any(|action| matches!(action, ScenarioAction::Undo | ScenarioAction::Redo))
        {
            return Err(invalid(
                "validate_scenario_transaction",
                "history actions must be dispatched in a one-action transaction",
            ));
        }
        Ok(Self {
            expected_revision,
            actions,
        })
    }

    /// Returns the exact scenario revision required before execution.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Returns ordered actions without exposing mutable transaction state.
    #[must_use]
    pub fn actions(&self) -> &[ScenarioAction] {
        &self.actions
    }

    fn into_actions(self) -> Vec<ScenarioAction> {
        self.actions
    }
}

/// Cloneable failure evidence supplied by a subsystem owner through the dispatcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineReportedFailure {
    category: ErrorCategory,
    recoverability: Recoverability,
    message: String,
    contexts: Vec<ErrorContext>,
}

impl EngineReportedFailure {
    /// Creates bounded typed failure evidence.
    ///
    /// # Errors
    ///
    /// Returns invalid input for an empty summary and resource exhaustion for an oversized summary.
    pub fn new(
        category: ErrorCategory,
        recoverability: Recoverability,
        message: impl Into<String>,
    ) -> Result<Self> {
        let message = message.into();
        if message.trim().is_empty() {
            return Err(invalid(
                "validate_reported_failure",
                "reported failure summary must not be empty",
            ));
        }
        if message.len() > MAX_REPORTED_FAILURE_MESSAGE_BYTES {
            return Err(resource_exhausted(
                "validate_reported_failure",
                "reported failure summary exceeds the byte bound",
            ));
        }
        Ok(Self {
            category,
            recoverability,
            message,
            contexts: Vec::new(),
        })
    }

    /// Adds one deterministic context frame.
    ///
    /// # Errors
    ///
    /// Returns resource exhaustion when the context bound is already reached.
    pub fn with_context(mut self, context: ErrorContext) -> Result<Self> {
        if self.contexts.len() >= MAX_REPORTED_FAILURE_CONTEXTS {
            return Err(resource_exhausted(
                "validate_reported_failure",
                "reported failure exceeds the context-frame bound",
            ));
        }
        self.contexts.push(context);
        Ok(self)
    }

    /// Returns the stable error category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the explicit recovery classification.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    /// Returns the bounded failure summary.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns bounded context frames from the failing operation toward outer callers.
    #[must_use]
    pub fn contexts(&self) -> &[ErrorContext] {
        &self.contexts
    }

    pub(crate) fn from_error(error: &Error) -> Self {
        Self {
            category: error.category(),
            recoverability: error.recoverability(),
            message: bounded_utf8_prefix(error.message(), MAX_REPORTED_FAILURE_MESSAGE_BYTES),
            contexts: error
                .contexts()
                .iter()
                .take(MAX_REPORTED_FAILURE_CONTEXTS)
                .cloned()
                .collect(),
        }
    }

    fn into_error(self) -> Error {
        let mut error = Error::new(self.category, self.recoverability, self.message);
        for context in self.contexts {
            error.push_context(context);
        }
        error
    }
}

/// Complete replacement state for classified failure and recovery automation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineRecoveryState {
    revision: u64,
    lifecycle: EngineLifecycleSnapshot,
    active_failures: Vec<EngineFailureRecord>,
    diagnostic_history: Vec<DiagnosticEvent>,
    pending_recovery: Option<EngineRecoveryAction>,
}

impl EngineRecoveryState {
    /// Returns the dispatcher-owned failure and recovery state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the coherent lifecycle state that controls workflow admission.
    #[must_use]
    pub const fn lifecycle(&self) -> &EngineLifecycleSnapshot {
        &self.lifecycle
    }

    /// Returns active failures in canonical subsystem order.
    #[must_use]
    pub fn active_failures(&self) -> &[EngineFailureRecord] {
        &self.active_failures
    }

    /// Returns retained diagnostic events from oldest to newest.
    #[must_use]
    pub fn diagnostic_history(&self) -> &[DiagnosticEvent] {
        &self.diagnostic_history
    }

    /// Returns the exact recovery token currently owned by a subsystem worker.
    #[must_use]
    pub const fn pending_recovery(&self) -> Option<EngineRecoveryAction> {
        self.pending_recovery
    }
}

/// Every currently supported command routed through the coherent engine owner.
#[derive(Debug)]
#[non_exhaustive]
pub enum EngineCommand {
    /// Commit one optimistic scenario transaction.
    ExecuteScenario(ScenarioTransaction),
    /// Inspect current scenario state without mutation.
    InspectScenario,
    /// Execute one typed authored-project mutation, undo, or redo command.
    ExecuteProjectHistory(ProjectHistoryCommand),
    /// Inspect current authored-project state and history without mutation.
    InspectProjectHistory,
    /// Inspect deterministic semantic project identity and component evidence without mutation.
    InspectProjectDiagnostics,
    /// Capture every authored and attached runtime editor-state owner without mutation.
    InspectEditorState,
    /// Inspect the attached project's durable and resolved settings.
    InspectProjectSettings,
    /// Commit one optimistic project settings transaction.
    ExecuteProjectSettings(ProjectSettingsTransaction),
    /// Inspect complete authored audio automation state without mutation.
    InspectAudioAutomation,
    /// Commit one optimistic ordered audio automation transaction.
    ExecuteAudioAutomation(AudioAutomationTransaction),
    /// Execute one crash recovery discovery, comparison, restore, or dismissal command.
    ExecuteProjectRecovery(ProjectRecoveryCommand),
    /// Inspect current lifecycle state without mutation.
    InspectLifecycle,
    /// Inspect complete classified failure and recovery state without mutation.
    InspectRecoveryState,
    /// Complete the exact outstanding lifecycle action.
    CompleteLifecycleAction(EngineLifecycleAction),
    /// Fail the exact outstanding lifecycle action with structured evidence.
    FailLifecycleAction {
        /// Exact revision-scoped action token.
        action: EngineLifecycleAction,
        /// Subsystem-supplied failure evidence.
        failure: EngineReportedFailure,
    },
    /// Report a failure from a ready subsystem during normal work.
    ReportRuntimeFailure {
        /// Subsystem that observed the failure.
        subsystem: EngineSubsystem,
        /// Subsystem-supplied failure evidence.
        failure: EngineReportedFailure,
    },
    /// Report a classified failure with the exact subsystem operation label.
    ReportClassifiedFailure {
        /// Subsystem that observed the failure.
        subsystem: EngineSubsystem,
        /// Stable bounded operation label.
        operation: EngineFailureOperation,
        /// Subsystem-supplied failure evidence.
        failure: EngineReportedFailure,
    },
    /// Begin recovery for one degraded subsystem.
    BeginRecovery(EngineSubsystem),
    /// Begin recovery for one exact classified failure and recovery intent.
    BeginClassifiedRecovery {
        /// Subsystem that owns the recovery work.
        subsystem: EngineSubsystem,
        /// Exact active failure identity.
        sequence: EngineFailureSequence,
        /// Classification-specific recovery intent.
        request: EngineRecoveryRequest,
    },
    /// Complete one exact classified recovery action.
    CompleteRecovery(EngineRecoveryAction),
    /// Fail and reclassify one exact classified recovery action.
    FailRecovery {
        /// Exact classified recovery token.
        action: EngineRecoveryAction,
        /// Subsystem-supplied failure evidence from the recovery attempt.
        failure: EngineReportedFailure,
    },
    /// Begin acknowledged project quiescence and volatile resource release for system sleep.
    BeginSleep,
    /// Begin acknowledged project and device revalidation after system wake.
    BeginWake,
    /// Begin deterministic reverse shutdown.
    BeginShutdown,
    /// Begin orderly restart into a fresh engine lifetime.
    BeginRestart,
    /// Evaluate one work kind against the complete lifecycle snapshot.
    AdmitWork(EngineWorkKind),
    /// Check whether a prior permit still names the exact current healthy state.
    ValidateWorkPermit(EngineWorkPermit),
    /// Route one stable transport command to the playback-domain owner.
    ExecutePlayback(PlaybackTransportCommand),
    /// Route one stable logical export-job command through the canonical queue owner.
    ExecuteExportJob(EngineExportJobCommand),
}

impl EngineCommand {
    const fn emits_state_event(&self) -> bool {
        match self {
            Self::ExecuteExportJob(command) => command.may_change_state(),
            Self::ExecuteProjectSettings(_) | Self::ExecuteAudioAutomation(_) => true,
            Self::ExecuteProjectRecovery(ProjectRecoveryCommand::Compare { .. }) => false,
            _ => matches!(
                self,
                Self::ExecuteScenario(_)
                    | Self::ExecuteProjectHistory(_)
                    | Self::ExecuteProjectRecovery(_)
                    | Self::CompleteLifecycleAction(_)
                    | Self::FailLifecycleAction { .. }
                    | Self::ReportRuntimeFailure { .. }
                    | Self::ReportClassifiedFailure { .. }
                    | Self::BeginRecovery(_)
                    | Self::BeginClassifiedRecovery { .. }
                    | Self::CompleteRecovery(_)
                    | Self::FailRecovery { .. }
                    | Self::BeginSleep
                    | Self::BeginWake
                    | Self::BeginShutdown
                    | Self::BeginRestart
                    | Self::ExecutePlayback(_)
            ),
        }
    }
}

/// One typed command paired with its caller-owned transaction identity.
#[derive(Debug)]
pub struct EngineCommandRequest {
    transaction_id: EngineTransactionId,
    command: EngineCommand,
}

impl EngineCommandRequest {
    /// Creates one command request.
    #[must_use]
    pub const fn new(transaction_id: EngineTransactionId, command: EngineCommand) -> Self {
        Self {
            transaction_id,
            command,
        }
    }
}

/// Typed result from one successful engine command.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum EngineCommandResult {
    /// Complete scenario state.
    Scenario(Box<ScenarioSnapshot>),
    /// Complete authored-project state, history metadata, and semantic command evidence.
    ProjectHistory(Box<ProjectHistoryOutcome>),
    /// Deterministic semantic project identity and ordered component evidence.
    ProjectDiagnostics(Box<ProjectDiagnostics>),
    /// Complete immutable editor state captured at one authoritative project revision.
    EditorState(Box<EditorStateSnapshot>),
    /// Complete durable and resolved project settings state.
    ProjectSettings(Box<ProjectSettingsState>),
    /// Complete replacement state for every authored audio automation lane.
    AudioAutomation(Box<AudioAutomationSnapshot>),
    /// Complete project crash recovery command outcome.
    ProjectRecovery(Box<ProjectRecoveryOutcome>),
    /// Complete lifecycle state.
    Lifecycle(Box<EngineLifecycleSnapshot>),
    /// Complete classified failure, recovery, and coherent lifecycle state.
    Recovery(Box<EngineRecoveryState>),
    /// Coherent admission or denial for one work kind.
    WorkAdmission(EngineWorkAdmission),
    /// Whether a prior work permit remains current.
    WorkPermitCurrent(bool),
    /// Playback-domain execution was admitted and queued in global command order.
    PlaybackAccepted {
        /// Revision-scoped lifecycle permit, absent only for read-only transport inspection.
        permit: Option<EngineWorkPermit>,
    },
    /// Complete replacement state for every retained logical export job.
    ExportJobs(Box<EngineExportJobState>),
}

/// Successful serialized command outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineCommandOutcome {
    transaction_id: EngineTransactionId,
    command_sequence: u64,
    result: EngineCommandResult,
}

impl EngineCommandOutcome {
    /// Returns the caller-owned transaction identity.
    #[must_use]
    pub const fn transaction_id(&self) -> &EngineTransactionId {
        &self.transaction_id
    }

    /// Returns this dispatcher's monotonic successful-command sequence.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns the typed command result.
    #[must_use]
    pub const fn result(&self) -> &EngineCommandResult {
        &self.result
    }
}

/// Full replacement state emitted after a committed or accepted state command.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum EngineEvent {
    /// Scenario state changed atomically.
    ScenarioStateChanged(Box<ScenarioSnapshot>),
    /// Authored project state changed atomically.
    ProjectStateChanged(Box<ProjectHistoryState>),
    /// Authoritative project settings changed atomically.
    ProjectSettingsChanged(Box<ProjectSettingsState>),
    /// Authoritative audio automation state changed atomically.
    AudioAutomationStateChanged(Box<AudioAutomationSnapshot>),
    /// Project crash recovery state changed atomically.
    ProjectRecoveryStateChanged(Box<ProjectRecoveryState>),
    /// Lifecycle state changed atomically.
    LifecycleStateChanged(Box<EngineLifecycleSnapshot>),
    /// Classified failure, recovery, and lifecycle state changed atomically.
    RecoveryStateChanged(Box<EngineRecoveryState>),
    /// Complete playback transport state after one accepted command.
    PlaybackStateChanged {
        /// Full replacement transport state after success or failure.
        snapshot: Box<PlaybackTransportSnapshot>,
        /// Bounded structured failure evidence when execution did not complete successfully.
        failure: Option<EngineReportedFailure>,
    },
    /// Complete logical export queue state after an explicit or automated transition.
    ExportJobsStateChanged(Box<EngineExportJobState>),
}

/// Ordered event envelope retained for the owning API or shell consumer.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineEventEnvelope {
    sequence: u64,
    command_sequence: u64,
    transaction_id: EngineTransactionId,
    project_revision: Option<u64>,
    lifecycle_state_revision: Option<u64>,
    playback_epoch: Option<u64>,
    export_state_revision: Option<u64>,
    recovery_state_revision: Option<u64>,
    audio_automation_revision: Option<u64>,
    event: EngineEvent,
}

impl EngineEventEnvelope {
    /// Returns this dispatcher's monotonic state-event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the successful command sequence that produced this state event.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns the transaction that produced this event.
    #[must_use]
    pub const fn transaction_id(&self) -> &EngineTransactionId {
        &self.transaction_id
    }

    /// Returns the resulting scenario revision for scenario events.
    #[must_use]
    pub const fn project_revision(&self) -> Option<u64> {
        self.project_revision
    }

    /// Returns the resulting lifecycle state revision for lifecycle events.
    #[must_use]
    pub const fn lifecycle_state_revision(&self) -> Option<u64> {
        self.lifecycle_state_revision
    }

    /// Returns the resulting playback discontinuity epoch for transport events.
    #[must_use]
    pub const fn playback_epoch(&self) -> Option<u64> {
        self.playback_epoch
    }

    /// Returns the resulting export state revision for logical export queue events.
    #[must_use]
    pub const fn export_state_revision(&self) -> Option<u64> {
        self.export_state_revision
    }

    /// Returns the resulting classified failure and recovery state revision.
    #[must_use]
    pub const fn recovery_state_revision(&self) -> Option<u64> {
        self.recovery_state_revision
    }

    /// Returns the resulting authored audio automation revision.
    #[must_use]
    pub const fn audio_automation_revision(&self) -> Option<u64> {
        self.audio_automation_revision
    }

    /// Returns the typed full replacement state.
    #[must_use]
    pub const fn event(&self) -> &EngineEvent {
        &self.event
    }
}

#[derive(Debug)]
struct QueuedPlaybackCommand {
    transaction_id: EngineTransactionId,
    command_sequence: u64,
    event_sequence: u64,
    permit: Option<EngineWorkPermit>,
    command: PlaybackTransportCommand,
}

/// Complete playback-domain execution returned to the serialized engine owner.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackCommandExecution {
    transaction_id: EngineTransactionId,
    command_sequence: u64,
    event_sequence: u64,
    permit: Option<EngineWorkPermit>,
    snapshot: PlaybackTransportSnapshot,
    failure: Option<EngineReportedFailure>,
}

impl PlaybackCommandExecution {
    /// Returns the transaction identity accepted by the engine dispatcher.
    #[must_use]
    pub const fn transaction_id(&self) -> &EngineTransactionId {
        &self.transaction_id
    }

    /// Returns the successful engine command sequence reserved before cross-domain execution.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns the lifecycle permit used for an admitted state-changing command.
    #[must_use]
    pub const fn permit(&self) -> Option<EngineWorkPermit> {
        self.permit
    }

    /// Returns complete transport state after success or failure.
    #[must_use]
    pub const fn snapshot(&self) -> PlaybackTransportSnapshot {
        self.snapshot
    }

    /// Returns bounded failure evidence when the real transport command failed.
    #[must_use]
    pub const fn failure(&self) -> Option<&EngineReportedFailure> {
        self.failure.as_ref()
    }
}

struct PlaybackCommandBridge {
    requests: SyncSender<QueuedPlaybackCommand>,
    completions: Receiver<PlaybackCommandExecution>,
}

/// Playback-domain half of the bounded engine command bridge.
///
/// The executor never blocks. At most one accepted command may be pending, and the engine owner
/// reserves its ordered completion event before publishing the request.
pub struct PlaybackCommandExecutor {
    requests: Receiver<QueuedPlaybackCommand>,
    completions: SyncSender<PlaybackCommandExecution>,
}

impl PlaybackCommandExecutor {
    /// Executes at most one accepted command against the real playback transport owner.
    ///
    /// Command failures are returned as successful bridge executions with complete replacement
    /// state and structured failure evidence. Errors from this method describe domain or bridge
    /// failure, not a transport command rejection.
    pub fn execute_next<O: Send + 'static>(
        &mut self,
        transport: &mut PlaybackTransport<O>,
        now: Instant,
    ) -> Result<Option<PlaybackCommandExecution>> {
        ExecutionDomain::Playback.require_current()?;
        let queued = match self.requests.try_recv() {
            Ok(queued) => queued,
            Err(TryRecvError::Empty) => return Ok(None),
            Err(TryRecvError::Disconnected) => {
                return Err(unavailable(
                    "receive_playback_command",
                    "engine playback command bridge is disconnected",
                ));
            }
        };

        let failure = transport
            .execute_command(queued.command, now)
            .err()
            .map(|error| EngineReportedFailure::from_error(&error));
        let execution = PlaybackCommandExecution {
            transaction_id: queued.transaction_id,
            command_sequence: queued.command_sequence,
            event_sequence: queued.event_sequence,
            permit: queued.permit,
            snapshot: transport.snapshot(),
            failure,
        };
        match self.completions.try_send(execution.clone()) {
            Ok(()) => Ok(Some(execution)),
            Err(TrySendError::Full(_)) => Err(internal(
                "complete_playback_command",
                "reserved playback completion capacity was unavailable",
            )),
            Err(TrySendError::Disconnected(_)) => Err(unavailable(
                "complete_playback_command",
                "engine playback completion bridge is disconnected",
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingPlaybackCommand {
    transaction_id: EngineTransactionId,
    command_sequence: u64,
    event_sequence: u64,
}

/// Single serialized command and transaction owner.
pub struct EngineCommandDispatcher {
    scenario: ScenarioEngine,
    project_history: Option<ProjectCommandHistory>,
    audio_automation: Option<AudioAutomationState>,
    project_recovery: Option<ProjectRecoveryCoordinator>,
    lifecycle: Option<EngineLifecycle>,
    errors: Option<EngineErrorCoordinator>,
    recovery_state_revision: u64,
    pending_recovery: Option<EngineRecoveryAction>,
    command_sequence: u64,
    event_sequence: u64,
    events: VecDeque<EngineEventEnvelope>,
    playback_bridge: Option<PlaybackCommandBridge>,
    pending_playback: Option<PendingPlaybackCommand>,
    latest_playback: Option<(PlaybackTransportSnapshot, Option<EngineReportedFailure>)>,
    export_jobs: Option<Box<dyn ErasedExportJobController>>,
    export_state_revision: u64,
    latest_export: Option<EngineExportJobState>,
}

impl EngineCommandDispatcher {
    /// Creates a coherent lifecycle-attached dispatcher on EngineControl.
    ///
    /// # Errors
    ///
    /// Returns a classified error when the caller does not own EngineControl or lifecycle
    /// construction fails.
    pub fn new() -> Result<Self> {
        ExecutionDomain::EngineControl.require_current()?;
        let lifecycle = EngineLifecycle::new()?;
        let errors = EngineErrorCoordinator::new()?;
        Ok(Self {
            scenario: ScenarioEngine::new(),
            project_history: None,
            audio_automation: None,
            project_recovery: None,
            lifecycle: Some(lifecycle),
            errors: Some(errors),
            recovery_state_revision: 0,
            pending_recovery: None,
            command_sequence: 0,
            event_sequence: 0,
            events: VecDeque::with_capacity(MAX_PENDING_ENGINE_EVENTS),
            playback_bridge: None,
            pending_playback: None,
            latest_playback: None,
            export_jobs: None,
            export_state_revision: 0,
            latest_export: None,
        })
    }

    /// Creates the coherent engine owner and a bounded playback-domain command executor.
    ///
    /// Construction requires EngineControl. The returned executor is moved to the playback owner,
    /// where it applies accepted commands to the real [`PlaybackTransport`].
    pub fn new_with_playback_bridge() -> Result<(Self, PlaybackCommandExecutor)> {
        let mut dispatcher = Self::new()?;
        let (request_sender, request_receiver) = sync_channel(MAX_PENDING_PLAYBACK_COMMANDS);
        let (completion_sender, completion_receiver) = sync_channel(MAX_PENDING_PLAYBACK_COMMANDS);
        dispatcher.playback_bridge = Some(PlaybackCommandBridge {
            requests: request_sender,
            completions: completion_receiver,
        });
        Ok((
            dispatcher,
            PlaybackCommandExecutor {
                requests: request_receiver,
                completions: completion_sender,
            },
        ))
    }

    /// Creates the compatibility dispatcher for the existing reference scenario API.
    ///
    /// Lifecycle and work commands are unavailable on this instance. Scenario transactions still
    /// use the same revision fences, atomic publication, command sequence, and state events.
    #[must_use]
    pub fn scenario_only() -> Self {
        Self {
            scenario: ScenarioEngine::new(),
            project_history: None,
            audio_automation: None,
            project_recovery: None,
            lifecycle: None,
            errors: None,
            recovery_state_revision: 0,
            pending_recovery: None,
            command_sequence: 0,
            event_sequence: 0,
            events: VecDeque::with_capacity(MAX_PENDING_ENGINE_EVENTS),
            playback_bridge: None,
            pending_playback: None,
            latest_playback: None,
            export_jobs: None,
            export_state_revision: 0,
            latest_export: None,
        }
    }

    /// Attaches the authoritative project document to the full engine owner.
    pub fn attach_project(&mut self, project: ProjectDocument) -> Result<()> {
        self.require_owner()?;
        if self.lifecycle.is_none() {
            return Err(unavailable(
                "attach_project",
                "project settings require the full lifecycle-attached dispatcher",
            ));
        }
        if self.project_history.is_some() {
            return Err(conflict(
                "attach_project",
                "a project document is already attached to this dispatcher",
            ));
        }
        ProjectSettingsState::from_snapshot(&project.snapshot())?;
        self.project_history = Some(ProjectCommandHistory::new(project));
        Ok(())
    }

    /// Attaches the exclusive authored audio automation owner to the full engine dispatcher.
    ///
    /// The state is consumed so later changes cannot bypass serialized engine commands.
    pub fn attach_audio_automation(&mut self, state: AudioAutomationState) -> Result<()> {
        self.require_owner()?;
        if self.lifecycle.is_none() {
            return Err(unavailable(
                "attach_audio_automation",
                "audio automation requires the full lifecycle-attached dispatcher",
            ));
        }
        if self.audio_automation.is_some() {
            return Err(conflict(
                "attach_audio_automation",
                "audio automation state is already attached to this dispatcher",
            ));
        }
        self.audio_automation = Some(state);
        Ok(())
    }

    /// Returns complete authored audio automation state without mutation.
    pub fn audio_automation_snapshot(&self) -> Result<AudioAutomationSnapshot> {
        self.require_owner()?;
        Ok(self.audio_automation_ref()?.snapshot())
    }

    /// Returns the attached authoritative project snapshot without mutation.
    pub fn project_snapshot(&self) -> Result<ProjectSnapshot> {
        self.require_owner()?;
        self.require_project_settings_owner()?;
        Ok(self.project_history_ref()?.project_snapshot())
    }

    /// Attaches the canonical bounded logical export queue and returns its typed runtime handle.
    ///
    /// Construction and later command dispatch require EngineControl. Executor preparation and
    /// typed result access remain on the handle because generic closures and artifacts are not
    /// stable command or event payloads.
    pub fn attach_export_jobs<R>(
        &mut self,
        config: ExportJobQueueConfig,
    ) -> Result<EngineExportJobRuntime<R>>
    where
        R: Send + Sync + 'static,
    {
        self.require_owner()?;
        if self.export_jobs.is_some() {
            return Err(conflict(
                "attach_export_jobs",
                "logical export queue is already attached to this dispatcher",
            ));
        }
        let (controller, runtime) = create_export_runtime(config)?;
        self.export_jobs = Some(controller);
        Ok(runtime)
    }

    /// Attaches the exclusive project document owner with one bounded history capacity.
    ///
    /// The document is consumed so authored mutations cannot bypass serialized engine commands.
    ///
    /// # Errors
    ///
    /// Returns a conflict when a project owner is already attached, or a classified capacity or
    /// ownership error when the requested owner cannot be created here.
    pub fn attach_project_history(
        &mut self,
        document: ProjectDocument,
        capacity: usize,
    ) -> Result<()> {
        self.require_owner()?;
        if self.project_history.is_some() {
            return Err(conflict(
                "attach_project_history",
                "project command history is already attached to this dispatcher",
            ));
        }
        ProjectSettingsState::from_snapshot(&document.snapshot())?;
        self.project_history = Some(ProjectCommandHistory::with_capacity(document, capacity)?);
        Ok(())
    }

    /// Attaches file-backed crash recovery to the authoritative project owner.
    ///
    /// The supplied database must reproduce the attached project exactly. Recovery
    /// then owns that database so restore publication cannot bypass serialized
    /// engine state.
    pub fn attach_project_recovery(
        &mut self,
        database: ProjectDatabase,
        recovery_root: impl AsRef<Path>,
    ) -> Result<()> {
        self.require_owner()?;
        self.require_project_settings_owner()?;
        if self.project_recovery.is_some() {
            return Err(conflict(
                "attach_project_recovery",
                "project recovery is already attached to this dispatcher",
            ));
        }
        let current = self.project_history_ref()?.project_snapshot();
        self.project_recovery = Some(ProjectRecoveryCoordinator::new(
            database,
            recovery_root,
            &current,
        )?);
        Ok(())
    }

    /// Returns current scenario state after enforcing full-dispatcher ownership when applicable.
    ///
    /// # Errors
    ///
    /// Returns a conflict when a lifecycle-attached dispatcher is inspected outside EngineControl.
    pub fn scenario_snapshot(&self) -> Result<ScenarioSnapshot> {
        self.require_owner()?;
        Ok(self.scenario.snapshot())
    }

    /// Returns immutable authored-project state after enforcing dispatcher ownership.
    ///
    /// # Errors
    ///
    /// Returns an ownership error or unavailable when no project command owner is attached.
    pub fn project_history_state(&self) -> Result<ProjectHistoryState> {
        self.require_owner()?;
        Ok(self.project_history_ref()?.state())
    }

    /// Returns one read-only capability and health snapshot from existing engine owners.
    ///
    /// The caller supplies declaration-only media capabilities and may supply one exact resource
    /// accounting snapshot. This method does not dispatch a command, advance a sequence, emit an
    /// event, reserve resources, or mutate scenario or lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns a conflict outside EngineControl or when this scenario-only dispatcher has no
    /// lifecycle owner. Returns an internal error if an active failure lacks its required safe
    /// diagnostic projection.
    pub fn introspection_snapshot(
        &self,
        media_capabilities: &MediaCapabilities,
        resources: Option<&ResourceArbitrationSnapshot>,
    ) -> Result<EngineIntrospectionSnapshot> {
        self.require_owner()?;
        EngineIntrospectionSnapshot::from_state(
            media_capabilities,
            resources,
            &self.recovery_snapshot()?,
        )
    }

    /// Returns one read-only integration validation snapshot over the same introspection state.
    ///
    /// The caller supplies the capability and optional resource observations used by canonical
    /// engine introspection. This method adds exact lifecycle and recovery actions, scenario
    /// reversal state, and retained playback and export observations without dispatching a command,
    /// advancing a sequence, emitting an event, polling a worker, or mutating engine state.
    pub fn integration_validation_snapshot(
        &self,
        media_capabilities: &MediaCapabilities,
        resources: Option<&ResourceArbitrationSnapshot>,
    ) -> Result<EngineIntegrationValidationSnapshot> {
        self.require_owner()?;
        let recovery = self.recovery_snapshot()?;
        let lifecycle = recovery.lifecycle().clone();
        let introspection =
            EngineIntrospectionSnapshot::from_state(media_capabilities, resources, &recovery)?;
        Ok(EngineIntegrationValidationSnapshot::new(
            introspection,
            self.scenario.snapshot(),
            lifecycle,
            recovery,
            self.playback_bridge.is_some(),
            self.pending_playback.is_some(),
            self.latest_playback.clone(),
            self.export_jobs.is_some(),
            self.latest_export.clone(),
        ))
    }

    /// Builds one fresh default engine validation snapshot for standalone public clients.
    ///
    /// The helper owns its temporary EngineControl domain and default declaration-only media
    /// registry. Live application and test hosts should call [`Self::integration_validation_snapshot`]
    /// on their existing dispatcher instead.
    pub fn standalone_integration_validation_snapshot(
    ) -> Result<EngineIntegrationValidationSnapshot> {
        let _domain = ExecutionDomain::EngineControl.enter_current()?;
        let dispatcher = Self::new()?;
        let capabilities =
            MediaCapabilities::from_registry(&crate::media::media_backend_registry()?)?;
        dispatcher.integration_validation_snapshot(&capabilities, None)
    }

    /// Executes one command and publishes at most one state event.
    ///
    /// Synchronous failures do not advance command or event sequence. An accepted playback command
    /// advances command order immediately and always completes with one reserved replacement-state
    /// event, including structured failure evidence when the playback owner rejects execution.
    /// Event capacity and sequence exhaustion are checked before state mutation or queueing.
    ///
    /// # Errors
    ///
    /// Returns classified ownership, validation, revision, lifecycle, or bounded-resource errors
    /// without partial dispatcher publication.
    pub fn dispatch(&mut self, request: EngineCommandRequest) -> Result<EngineCommandOutcome> {
        self.require_owner()?;
        self.collect_playback_completion()?;
        let EngineCommandRequest {
            transaction_id,
            command,
        } = request;
        let emits_state_event = match &command {
            EngineCommand::ExecuteProjectSettings(transaction) => {
                self.project_settings_will_change(transaction)?
            }
            EngineCommand::ExecuteAudioAutomation(transaction) => {
                self.audio_automation_will_change(transaction)?
            }
            _ => command.emits_state_event(),
        };
        if emits_state_event && self.pending_playback.is_some() {
            return Err(conflict(
                "serialize_state_command",
                "a playback state event must complete before another state-changing command",
            ));
        }
        let next_command_sequence =
            next_sequence(self.command_sequence, "advance_command_sequence")?;
        let next_event_sequence = emits_state_event
            .then(|| next_sequence(self.event_sequence, "advance_event_sequence"))
            .transpose()?;
        if emits_state_event && self.events.len() >= MAX_PENDING_ENGINE_EVENTS {
            return Err(resource_exhausted(
                "enqueue_event",
                "engine state-event queue reached its capacity",
            ));
        }

        if let EngineCommand::ExecutePlayback(command) = command {
            return self.enqueue_playback_command(
                transaction_id,
                next_command_sequence,
                next_event_sequence.expect("playback command reserved an event"),
                command,
            );
        }

        let execution = self.execute_command(command)?;
        self.command_sequence = next_command_sequence;
        if let Some(event) = execution.event {
            let sequence = next_event_sequence.expect("state-changing command reserved an event");
            let (
                project_revision,
                lifecycle_state_revision,
                playback_epoch,
                export_state_revision,
                recovery_state_revision,
                audio_automation_revision,
            ) = match &event {
                EngineEvent::ScenarioStateChanged(snapshot) => {
                    (Some(snapshot.revision()), None, None, None, None, None)
                }
                EngineEvent::ProjectStateChanged(state) => (
                    Some(state.snapshot().revision()),
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
                EngineEvent::ProjectSettingsChanged(state) => {
                    (Some(state.project_revision()), None, None, None, None, None)
                }
                EngineEvent::AudioAutomationStateChanged(snapshot) => {
                    (None, None, None, None, None, Some(snapshot.revision()))
                }
                EngineEvent::ProjectRecoveryStateChanged(state) => {
                    (Some(state.project_revision()), None, None, None, None, None)
                }
                EngineEvent::LifecycleStateChanged(snapshot) => (
                    None,
                    Some(snapshot.state_revision()),
                    None,
                    None,
                    None,
                    None,
                ),
                EngineEvent::RecoveryStateChanged(state) => (
                    None,
                    Some(state.lifecycle().state_revision()),
                    None,
                    None,
                    Some(state.revision()),
                    None,
                ),
                EngineEvent::PlaybackStateChanged { snapshot, .. } => {
                    (None, None, Some(snapshot.epoch()), None, None, None)
                }
                EngineEvent::ExportJobsStateChanged(state) => {
                    (None, None, None, Some(state.revision()), None, None)
                }
            };
            self.events.push_back(EngineEventEnvelope {
                sequence,
                command_sequence: next_command_sequence,
                transaction_id: transaction_id.clone(),
                project_revision,
                lifecycle_state_revision,
                playback_epoch,
                export_state_revision,
                recovery_state_revision,
                audio_automation_revision,
                event,
            });
            self.event_sequence = sequence;
        }

        Ok(EngineCommandOutcome {
            transaction_id,
            command_sequence: next_command_sequence,
            result: execution.result,
        })
    }

    /// Drains every pending state event in exact sequence order.
    ///
    /// # Errors
    ///
    /// Returns a conflict when a lifecycle-attached dispatcher is drained outside EngineControl.
    pub fn drain_events(&mut self) -> Result<Vec<EngineEventEnvelope>> {
        self.require_owner()?;
        self.collect_playback_completion()?;
        Ok(self.events.drain(..).collect())
    }

    fn enqueue_playback_command(
        &mut self,
        transaction_id: EngineTransactionId,
        command_sequence: u64,
        event_sequence: u64,
        command: PlaybackTransportCommand,
    ) -> Result<EngineCommandOutcome> {
        let permit = if matches!(command, PlaybackTransportCommand::Inspect) {
            None
        } else {
            Some(
                self.lifecycle_snapshot()?
                    .admit(EngineWorkKind::Playback)
                    .permit()
                    .ok_or_else(|| {
                        conflict(
                            "admit_playback_command",
                            "current engine state does not admit playback transport commands",
                        )
                    })?,
            )
        };
        let queued = QueuedPlaybackCommand {
            transaction_id: transaction_id.clone(),
            command_sequence,
            event_sequence,
            permit,
            command,
        };
        let bridge = self.playback_bridge.as_ref().ok_or_else(|| {
            unavailable(
                "enqueue_playback_command",
                "playback command bridge is not attached to this dispatcher",
            )
        })?;
        match bridge.requests.try_send(queued) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                return Err(conflict(
                    "enqueue_playback_command",
                    "playback command bridge already owns one pending command",
                ));
            }
            Err(TrySendError::Disconnected(_)) => {
                return Err(unavailable(
                    "enqueue_playback_command",
                    "playback command executor is disconnected",
                ));
            }
        }
        self.pending_playback = Some(PendingPlaybackCommand {
            transaction_id: transaction_id.clone(),
            command_sequence,
            event_sequence,
        });
        self.command_sequence = command_sequence;
        Ok(EngineCommandOutcome {
            transaction_id,
            command_sequence,
            result: EngineCommandResult::PlaybackAccepted { permit },
        })
    }

    fn collect_playback_completion(&mut self) -> Result<()> {
        let Some(pending) = self.pending_playback.clone() else {
            return Ok(());
        };
        let bridge = self.playback_bridge.as_ref().ok_or_else(|| {
            internal(
                "collect_playback_completion",
                "pending playback command has no attached bridge",
            )
        })?;
        let completion = match bridge.completions.try_recv() {
            Ok(completion) => completion,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                return Err(unavailable(
                    "collect_playback_completion",
                    "playback command executor disconnected before completion",
                ));
            }
        };
        if completion.transaction_id != pending.transaction_id
            || completion.command_sequence != pending.command_sequence
            || completion.event_sequence != pending.event_sequence
        {
            return Err(internal(
                "collect_playback_completion",
                "playback completion did not match the reserved engine command",
            ));
        }
        if self.events.len() >= MAX_PENDING_ENGINE_EVENTS {
            return Err(internal(
                "collect_playback_completion",
                "reserved playback event capacity was unavailable",
            ));
        }
        let snapshot = completion.snapshot;
        self.latest_playback = Some((snapshot, completion.failure.clone()));
        self.events.push_back(EngineEventEnvelope {
            sequence: completion.event_sequence,
            command_sequence: completion.command_sequence,
            transaction_id: completion.transaction_id,
            project_revision: None,
            lifecycle_state_revision: None,
            playback_epoch: Some(snapshot.epoch()),
            export_state_revision: None,
            recovery_state_revision: None,
            audio_automation_revision: None,
            event: EngineEvent::PlaybackStateChanged {
                snapshot: Box::new(snapshot),
                failure: completion.failure,
            },
        });
        self.event_sequence = completion.event_sequence;
        self.pending_playback = None;
        Ok(())
    }

    fn require_owner(&self) -> Result<()> {
        if self.lifecycle.is_some() {
            ExecutionDomain::EngineControl.require_current()?;
        }
        Ok(())
    }

    fn execute_command(&mut self, command: EngineCommand) -> Result<CommandExecution> {
        match command {
            EngineCommand::ExecuteScenario(transaction) => {
                let actual_revision = self.scenario.snapshot().revision();
                if actual_revision != transaction.expected_revision() {
                    return Err(Error::new(
                        ErrorCategory::Conflict,
                        Recoverability::UserCorrectable,
                        "scenario revision does not match the transaction fence",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "execute_scenario_transaction")
                            .with_field(
                                "expected_revision",
                                transaction.expected_revision().to_string(),
                            )
                            .with_field("actual_revision", actual_revision.to_string()),
                    ));
                }
                let snapshot = self
                    .scenario
                    .execute_transaction(transaction.into_actions())?;
                Ok(CommandExecution {
                    result: EngineCommandResult::Scenario(Box::new(snapshot.clone())),
                    event: Some(EngineEvent::ScenarioStateChanged(Box::new(snapshot))),
                })
            }
            EngineCommand::InspectScenario => Ok(CommandExecution {
                result: EngineCommandResult::Scenario(Box::new(self.scenario.snapshot())),
                event: None,
            }),
            EngineCommand::ExecuteProjectHistory(command) => {
                let outcome = self.project_history_mut()?.execute(command)?;
                let event = outcome
                    .authored_state_changed()
                    .then(|| EngineEvent::ProjectStateChanged(Box::new(outcome.state().clone())));
                Ok(CommandExecution {
                    result: EngineCommandResult::ProjectHistory(Box::new(outcome)),
                    event,
                })
            }
            EngineCommand::InspectProjectHistory => Ok(CommandExecution {
                result: EngineCommandResult::ProjectHistory(Box::new(
                    self.project_history_ref()?.inspect(),
                )),
                event: None,
            }),
            EngineCommand::InspectProjectDiagnostics => {
                let snapshot = self.project_history_ref()?.project_snapshot();
                Ok(CommandExecution {
                    result: EngineCommandResult::ProjectDiagnostics(Box::new(
                        ProjectDiagnostics::from_snapshot(&snapshot)?,
                    )),
                    event: None,
                })
            }
            EngineCommand::InspectEditorState => Ok(CommandExecution {
                result: EngineCommandResult::EditorState(Box::new(self.editor_state_snapshot()?)),
                event: None,
            }),
            EngineCommand::InspectProjectSettings => {
                let state = ProjectSettingsState::from_snapshot(&self.project_snapshot()?)?;
                Ok(CommandExecution {
                    result: EngineCommandResult::ProjectSettings(Box::new(state)),
                    event: None,
                })
            }
            EngineCommand::ExecuteProjectSettings(transaction) => {
                self.require_project_settings_owner()?;
                let prior_revision = self.project_history_ref()?.project_snapshot().revision();
                let snapshot = self
                    .project_history_mut()?
                    .execute_settings_transaction(transaction)?;
                let state = ProjectSettingsState::from_snapshot(&snapshot)?;
                let event = (snapshot.revision() != prior_revision)
                    .then(|| EngineEvent::ProjectSettingsChanged(Box::new(state.clone())));
                Ok(CommandExecution {
                    result: EngineCommandResult::ProjectSettings(Box::new(state)),
                    event,
                })
            }
            EngineCommand::ExecuteProjectRecovery(command) => {
                let operation = command.operation();
                let (recovery, history) = self.project_recovery_parts()?;
                let outcome = recovery.execute(history, command)?;
                let event = (operation
                    != crate::project_recovery::ProjectRecoveryOperation::Compare)
                    .then(|| {
                        EngineEvent::ProjectRecoveryStateChanged(Box::new(outcome.state().clone()))
                    });
                Ok(CommandExecution {
                    result: EngineCommandResult::ProjectRecovery(Box::new(outcome)),
                    event,
                })
            }
            EngineCommand::InspectAudioAutomation => Ok(CommandExecution {
                result: EngineCommandResult::AudioAutomation(Box::new(
                    self.audio_automation_snapshot()?,
                )),
                event: None,
            }),
            EngineCommand::ExecuteAudioAutomation(transaction) => {
                let prior_revision = self.audio_automation_ref()?.revision();
                let snapshot = self.audio_automation_mut()?.apply(transaction)?;
                let event = (snapshot.revision() != prior_revision)
                    .then(|| EngineEvent::AudioAutomationStateChanged(Box::new(snapshot.clone())));
                Ok(CommandExecution {
                    result: EngineCommandResult::AudioAutomation(Box::new(snapshot)),
                    event,
                })
            }
            EngineCommand::InspectLifecycle => {
                let snapshot = self.lifecycle_snapshot()?;
                Ok(CommandExecution {
                    result: EngineCommandResult::Lifecycle(Box::new(snapshot)),
                    event: None,
                })
            }
            EngineCommand::InspectRecoveryState => Ok(CommandExecution {
                result: EngineCommandResult::Recovery(Box::new(self.recovery_snapshot()?)),
                event: None,
            }),
            EngineCommand::CompleteLifecycleAction(action) => {
                self.require_export_teardown_ready(action)?;
                if self.lifecycle_snapshot()?.pending_action() == Some(action) {
                    if let Some(recovery) = self.pending_recovery {
                        let state = self.complete_classified_recovery(recovery)?;
                        return Ok(legacy_recovery_change(state));
                    }
                }
                let snapshot = self
                    .lifecycle_mut()?
                    .complete_action(action)?
                    .value()
                    .clone();
                Ok(lifecycle_change(snapshot))
            }
            EngineCommand::FailLifecycleAction { action, failure } => {
                if self.lifecycle_snapshot()?.pending_action() == Some(action) {
                    if let Some(recovery) = self.pending_recovery {
                        let state = self.fail_classified_recovery(recovery, failure)?;
                        return Ok(legacy_recovery_change(state));
                    }
                }
                let snapshot = self
                    .lifecycle_mut()?
                    .fail_action(action, failure.into_error())?
                    .value()
                    .clone();
                Ok(lifecycle_change(snapshot))
            }
            EngineCommand::ReportRuntimeFailure { subsystem, failure } => {
                let state = self.report_classified_failure(
                    subsystem,
                    "runtime_failure".to_owned(),
                    failure,
                )?;
                Ok(legacy_recovery_change(state))
            }
            EngineCommand::ReportClassifiedFailure {
                subsystem,
                operation,
                failure,
            } => {
                let state =
                    self.report_classified_failure(subsystem, operation.into_string(), failure)?;
                Ok(recovery_change(state))
            }
            EngineCommand::BeginRecovery(subsystem) => {
                let state = self.begin_legacy_recovery(subsystem)?;
                Ok(legacy_recovery_change(state))
            }
            EngineCommand::BeginClassifiedRecovery {
                subsystem,
                sequence,
                request,
            } => {
                let state = self.begin_classified_recovery(subsystem, sequence, request)?;
                Ok(recovery_change(state))
            }
            EngineCommand::CompleteRecovery(action) => {
                let state = self.complete_classified_recovery(action)?;
                Ok(recovery_change(state))
            }
            EngineCommand::FailRecovery { action, failure } => {
                let state = self.fail_classified_recovery(action, failure)?;
                Ok(recovery_change(state))
            }
            EngineCommand::BeginSleep => {
                let snapshot = self.lifecycle_mut()?.begin_sleep()?.value().clone();
                Ok(lifecycle_change(snapshot))
            }
            EngineCommand::BeginWake => {
                let snapshot = self.lifecycle_mut()?.begin_wake()?.value().clone();
                Ok(lifecycle_change(snapshot))
            }
            EngineCommand::BeginShutdown => {
                let snapshot = self.lifecycle_mut()?.begin_shutdown()?.value().clone();
                Ok(lifecycle_change(snapshot))
            }
            EngineCommand::BeginRestart => {
                let snapshot = self.lifecycle_mut()?.begin_restart()?.value().clone();
                Ok(lifecycle_change(snapshot))
            }
            EngineCommand::AdmitWork(work) => {
                let admission = self.lifecycle_snapshot()?.admit(work);
                Ok(CommandExecution {
                    result: EngineCommandResult::WorkAdmission(admission),
                    event: None,
                })
            }
            EngineCommand::ValidateWorkPermit(permit) => {
                let current = self.lifecycle_snapshot()?.is_permit_current(permit);
                Ok(CommandExecution {
                    result: EngineCommandResult::WorkPermitCurrent(current),
                    event: None,
                })
            }
            EngineCommand::ExecutePlayback(_) => {
                unreachable!("playback commands are queued before synchronous command execution")
            }
            EngineCommand::ExecuteExportJob(command) => self.execute_export_command(command),
        }
    }

    fn execute_export_command(
        &mut self,
        command: EngineExportJobCommand,
    ) -> Result<CommandExecution> {
        let may_change_state = command.may_change_state();
        let next_state_revision = may_change_state
            .then(|| next_sequence(self.export_state_revision, "advance_export_state_revision"))
            .transpose()?;
        let permit = if command.requires_fresh_admission() {
            Some(
                self.lifecycle_snapshot()?
                    .admit(EngineWorkKind::Export)
                    .permit()
                    .ok_or_else(|| {
                        conflict(
                            "admit_export_job_command",
                            "current engine state does not admit fresh export execution",
                        )
                    })?,
            )
        } else {
            None
        };
        let execution = self
            .export_jobs
            .as_mut()
            .ok_or_else(missing_export_jobs)?
            .execute(command, permit)?;
        let revision = if execution.changed {
            let revision = next_state_revision
                .expect("state-changing export command reserved a state revision");
            self.export_state_revision = revision;
            revision
        } else {
            self.export_state_revision
        };
        let state = EngineExportJobState::new(revision, execution.jobs);
        self.latest_export = Some(state.clone());
        Ok(CommandExecution {
            result: EngineCommandResult::ExportJobs(Box::new(state.clone())),
            event: execution
                .changed
                .then(|| EngineEvent::ExportJobsStateChanged(Box::new(state))),
        })
    }

    fn report_classified_failure(
        &mut self,
        subsystem: EngineSubsystem,
        operation: String,
        failure: EngineReportedFailure,
    ) -> Result<EngineRecoveryState> {
        let revision = next_sequence(
            self.recovery_state_revision,
            "advance_recovery_state_revision",
        )?;
        {
            let (errors, lifecycle) = self.error_coordinator_parts()?;
            errors.report_failure(lifecycle, subsystem, operation, failure.into_error())?;
        }
        self.recovery_state_revision = revision;
        self.recovery_snapshot()
    }

    fn begin_legacy_recovery(&mut self, subsystem: EngineSubsystem) -> Result<EngineRecoveryState> {
        let active = self
            .error_coordinator_ref()?
            .active_failure(subsystem)?
            .ok_or_else(|| missing_active_failure(subsystem))?;
        let request = active
            .disposition()
            .recovery_request()
            .ok_or_else(|| terminal_recovery_error(subsystem, active.sequence()))?;
        self.begin_classified_recovery(subsystem, active.sequence(), request)
    }

    fn begin_classified_recovery(
        &mut self,
        subsystem: EngineSubsystem,
        sequence: EngineFailureSequence,
        request: EngineRecoveryRequest,
    ) -> Result<EngineRecoveryState> {
        let revision = next_sequence(
            self.recovery_state_revision,
            "advance_recovery_state_revision",
        )?;
        let action = {
            let (errors, lifecycle) = self.error_coordinator_parts()?;
            errors
                .begin_recovery(lifecycle, subsystem, sequence, request)?
                .action()
        };
        self.pending_recovery = Some(action);
        self.recovery_state_revision = revision;
        self.recovery_snapshot()
    }

    fn complete_classified_recovery(
        &mut self,
        action: EngineRecoveryAction,
    ) -> Result<EngineRecoveryState> {
        let revision = next_sequence(
            self.recovery_state_revision,
            "advance_recovery_state_revision",
        )?;
        {
            let (errors, lifecycle) = self.error_coordinator_parts()?;
            errors.complete_recovery(lifecycle, action)?;
        }
        self.pending_recovery = None;
        self.recovery_state_revision = revision;
        self.recovery_snapshot()
    }

    fn fail_classified_recovery(
        &mut self,
        action: EngineRecoveryAction,
        failure: EngineReportedFailure,
    ) -> Result<EngineRecoveryState> {
        let revision = next_sequence(
            self.recovery_state_revision,
            "advance_recovery_state_revision",
        )?;
        {
            let (errors, lifecycle) = self.error_coordinator_parts()?;
            errors.fail_recovery(lifecycle, action, failure.into_error())?;
        }
        self.pending_recovery = None;
        self.recovery_state_revision = revision;
        self.recovery_snapshot()
    }

    fn recovery_snapshot(&self) -> Result<EngineRecoveryState> {
        let lifecycle = self.lifecycle_snapshot()?;
        let errors = self.error_coordinator_ref()?;
        let mut active_failures = Vec::with_capacity(EngineSubsystem::ALL.len());
        for subsystem in EngineSubsystem::ALL {
            if let Some(failure) = errors.active_failure(subsystem)? {
                active_failures.push(failure);
            }
        }
        Ok(EngineRecoveryState {
            revision: self.recovery_state_revision,
            lifecycle,
            active_failures,
            diagnostic_history: errors.diagnostic_history()?,
            pending_recovery: self.pending_recovery,
        })
    }

    fn editor_state_snapshot(&self) -> Result<EditorStateSnapshot> {
        let project = self.project_history_ref()?.state();
        let diagnostics = ProjectDiagnostics::from_snapshot(project.snapshot())?;
        let settings = ProjectSettingsState::from_snapshot(project.snapshot())?;
        let authored_documents =
            crate::editor_state::EditorAuthoredDocuments::from_snapshot(project.snapshot())?;
        let audio_tracks =
            crate::editor_state::EditorAudioTrackState::from_snapshot(project.snapshot())?;
        let extensions =
            crate::editor_state::EditorExtensionRecord::from_snapshot(project.snapshot());
        let audio_automation = match self.audio_automation.as_ref() {
            Some(state) => EditorStateAvailability::Attached(state.snapshot()),
            None => EditorStateAvailability::Detached,
        };
        let project_recovery = match self.project_recovery.as_ref() {
            Some(recovery) => {
                EditorStateAvailability::Attached(recovery.state(self.project_history_ref()?))
            }
            None => EditorStateAvailability::Detached,
        };
        let playback = EditorPlaybackState::new(
            self.playback_bridge.is_some(),
            self.pending_playback.is_some(),
            self.latest_playback.as_ref().map(|(snapshot, failure)| {
                EditorPlaybackObservation::new(*snapshot, failure.clone())
            }),
        );
        let export = EditorExportState::new(self.export_jobs.is_some(), self.latest_export.clone());
        Ok(EditorStateSnapshot::new(
            project,
            diagnostics,
            settings,
            authored_documents,
            audio_tracks,
            extensions,
            audio_automation,
            project_recovery,
            playback,
            export,
        ))
    }

    fn lifecycle_snapshot(&self) -> Result<EngineLifecycleSnapshot> {
        self.lifecycle_ref()?
            .snapshot()
            .map(|snapshot| snapshot.value().clone())
    }

    fn require_export_teardown_ready(&self, action: EngineLifecycleAction) -> Result<()> {
        if action.subsystem() != EngineSubsystem::Export
            || action.kind() != EngineLifecycleActionKind::Teardown
        {
            return Ok(());
        }
        if let Some(controller) = self.export_jobs.as_ref() {
            if !controller.logical_jobs_final()? {
                return Err(conflict(
                    "complete_export_teardown",
                    "logical export jobs must reach final state before export teardown completes",
                ));
            }
        }
        Ok(())
    }

    fn project_settings_will_change(
        &self,
        transaction: &ProjectSettingsTransaction,
    ) -> Result<bool> {
        self.require_project_settings_owner()?;
        self.project_history_ref()?
            .settings_will_change(transaction)
    }

    fn audio_automation_will_change(
        &self,
        transaction: &AudioAutomationTransaction,
    ) -> Result<bool> {
        self.audio_automation_ref()?.would_change(transaction)
    }

    fn require_project_settings_owner(&self) -> Result<()> {
        if self.lifecycle.is_none() {
            return Err(unavailable(
                "require_project_settings",
                "project settings require the full lifecycle-attached dispatcher",
            ));
        }
        Ok(())
    }

    fn lifecycle_ref(&self) -> Result<&EngineLifecycle> {
        self.lifecycle.as_ref().ok_or_else(missing_lifecycle)
    }

    fn project_history_ref(&self) -> Result<&ProjectCommandHistory> {
        self.project_history
            .as_ref()
            .ok_or_else(missing_project_history)
    }

    fn project_history_mut(&mut self) -> Result<&mut ProjectCommandHistory> {
        self.project_history
            .as_mut()
            .ok_or_else(missing_project_history)
    }

    fn audio_automation_ref(&self) -> Result<&AudioAutomationState> {
        self.audio_automation
            .as_ref()
            .ok_or_else(missing_audio_automation)
    }

    fn audio_automation_mut(&mut self) -> Result<&mut AudioAutomationState> {
        self.audio_automation
            .as_mut()
            .ok_or_else(missing_audio_automation)
    }

    fn project_recovery_parts(
        &mut self,
    ) -> Result<(&mut ProjectRecoveryCoordinator, &mut ProjectCommandHistory)> {
        match (
            self.project_recovery.as_mut(),
            self.project_history.as_mut(),
        ) {
            (Some(recovery), Some(history)) => Ok((recovery, history)),
            _ => Err(missing_project_recovery()),
        }
    }

    fn lifecycle_mut(&mut self) -> Result<&mut EngineLifecycle> {
        self.lifecycle.as_mut().ok_or_else(missing_lifecycle)
    }

    fn error_coordinator_ref(&self) -> Result<&EngineErrorCoordinator> {
        self.errors.as_ref().ok_or_else(missing_error_coordinator)
    }

    fn error_coordinator_parts(
        &mut self,
    ) -> Result<(&mut EngineErrorCoordinator, &mut EngineLifecycle)> {
        match (self.errors.as_mut(), self.lifecycle.as_mut()) {
            (Some(errors), Some(lifecycle)) => Ok((errors, lifecycle)),
            _ => Err(missing_error_coordinator()),
        }
    }
}

impl Default for EngineCommandDispatcher {
    fn default() -> Self {
        Self::scenario_only()
    }
}

struct CommandExecution {
    result: EngineCommandResult,
    event: Option<EngineEvent>,
}

fn lifecycle_change(snapshot: EngineLifecycleSnapshot) -> CommandExecution {
    CommandExecution {
        result: EngineCommandResult::Lifecycle(Box::new(snapshot.clone())),
        event: Some(EngineEvent::LifecycleStateChanged(Box::new(snapshot))),
    }
}

fn recovery_change(state: EngineRecoveryState) -> CommandExecution {
    CommandExecution {
        result: EngineCommandResult::Recovery(Box::new(state.clone())),
        event: Some(EngineEvent::RecoveryStateChanged(Box::new(state))),
    }
}

fn legacy_recovery_change(state: EngineRecoveryState) -> CommandExecution {
    CommandExecution {
        result: EngineCommandResult::Lifecycle(Box::new(state.lifecycle().clone())),
        event: Some(EngineEvent::RecoveryStateChanged(Box::new(state))),
    }
}

fn next_sequence(current: u64, operation: &'static str) -> Result<u64> {
    current.checked_add(1).ok_or_else(|| {
        Error::new(
            ErrorCategory::ResourceExhausted,
            Recoverability::Terminal,
            "engine dispatcher sequence is exhausted",
        )
        .with_context(ErrorContext::new(COMPONENT, operation))
    })
}

fn missing_lifecycle() -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "lifecycle commands are unavailable on the scenario-only dispatcher",
    )
    .with_context(ErrorContext::new(COMPONENT, "require_lifecycle"))
}

fn missing_error_coordinator() -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "classified failure commands are unavailable on the scenario-only dispatcher",
    )
    .with_context(ErrorContext::new(COMPONENT, "require_error_coordinator"))
}

fn missing_active_failure(subsystem: EngineSubsystem) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::Retryable,
        "subsystem has no active classified failure",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "begin_legacy_recovery")
            .with_field("subsystem", subsystem.code()),
    )
}

fn terminal_recovery_error(subsystem: EngineSubsystem, sequence: EngineFailureSequence) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::Terminal,
        "terminal engine failure requires a new engine lifetime",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "begin_legacy_recovery")
            .with_field("subsystem", subsystem.code())
            .with_field("failure_sequence", sequence.to_string()),
    )
}

fn missing_export_jobs() -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "logical export commands require an attached export queue",
    )
    .with_context(ErrorContext::new(COMPONENT, "require_export_jobs"))
}

fn missing_project_history() -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "project commands require an attached project command history",
    )
    .with_context(ErrorContext::new(COMPONENT, "require_project_history"))
}

fn missing_audio_automation() -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "audio automation commands require attached authored state",
    )
    .with_context(ErrorContext::new(COMPONENT, "require_audio_automation"))
}

fn missing_project_recovery() -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "project recovery commands require an attached recovery owner",
    )
    .with_context(ErrorContext::new(COMPONENT, "require_project_recovery"))
}

fn bounded_utf8_prefix(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_owned();
    }
    let mut end = maximum_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Conflict, Recoverability::Retryable, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unavailable(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn resource_exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
