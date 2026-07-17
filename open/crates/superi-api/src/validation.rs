//! Strict transport-neutral projection of coherent engine integration validation state.
//!
//! Canonical capability, lifecycle, subsystem, workflow readiness, resource, and user-safe
//! failure state comes from the existing engine introspection contract. Validation adds precise
//! action tokens, revision-scoped admission, scenario reversal, endpoint replacement state, and
//! deterministic coherence findings without creating a second state owner.

use serde::{Deserialize, Serialize};
use superi_core::diagnostics::UserSafeError;
use superi_core::error::Error;
use superi_core::settings::SemanticVersion;
use superi_engine::dispatcher::{EngineCommandDispatcher, EngineReportedFailure};
use superi_engine::error::EngineRecoveryAction;
use superi_engine::lifecycle::{EngineFailureSnapshot, EngineLifecycleAction};
use superi_engine::transport::{
    PlaybackTransportMode, PlaybackTransportSnapshot, TransportAudioState, TransportDegradationCode,
};
use superi_engine::validation::{
    EngineIntegrationCondition as EngineCondition, EngineIntegrationFinding as EngineFinding,
    EngineIntegrationValidationSnapshot as EngineSnapshot,
};

use crate::api::{
    public_engine_snapshot, public_recovery_request, public_subsystem, public_workflow,
    EngineIntrospectionSnapshot, EngineLifecyclePhase, EngineRecoveryRequest, EngineSubsystem,
    EngineWorkflow,
};
use crate::commands::{GetEngineIntegrationValidation, GetEngineIntegrationValidationResult};
use crate::version::ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION;

/// Stable public high-level condition of the integrated engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum IntegrationCondition {
    Starting,
    Normal,
    Degraded,
    Recovering,
    Pausing,
    Paused,
    Resuming,
    PreparingSleep,
    Sleeping,
    Waking,
    Stopping,
    Stopped,
    Failed,
    Unknown,
}

impl IntegrationCondition {
    /// Returns the stable condition code.
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
            Self::Unknown => "unknown",
        }
    }
}

/// Stable public lifecycle action kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum IntegrationLifecycleActionKind {
    Initialize,
    Recover,
    PrepareSleep,
    Wake,
    Teardown,
    Unknown,
}

impl IntegrationLifecycleActionKind {
    /// Returns the stable lifecycle action code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Initialize => "initialize",
            Self::Recover => "recover",
            Self::PrepareSleep => "prepare_sleep",
            Self::Wake => "wake",
            Self::Teardown => "teardown",
            Self::Unknown => "unknown",
        }
    }
}

/// Exact public lifecycle action token.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleActionState {
    subsystem: EngineSubsystem,
    kind: IntegrationLifecycleActionKind,
    lifetime: u64,
    revision: u64,
}

impl LifecycleActionState {
    #[must_use]
    pub const fn subsystem(&self) -> EngineSubsystem {
        self.subsystem
    }

    #[must_use]
    pub const fn kind(&self) -> IntegrationLifecycleActionKind {
        self.kind
    }

    #[must_use]
    pub const fn lifetime(&self) -> u64 {
        self.lifetime
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}

/// Stable classified failure evidence without raw messages or diagnostic context.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationFailureState {
    category: String,
    recoverability: String,
}

impl IntegrationFailureState {
    /// Returns the stable shared error category code.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    /// Returns the stable shared recoverability code.
    #[must_use]
    pub fn recoverability(&self) -> &str {
        &self.recoverability
    }
}

/// Public editable scenario summary and reversal capacity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioValidationState {
    revision: u64,
    phase: String,
    operation_count: usize,
    undo_depth: usize,
    redo_depth: usize,
}

impl ScenarioValidationState {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn phase(&self) -> &str {
        &self.phase
    }

    #[must_use]
    pub const fn operation_count(&self) -> usize {
        self.operation_count
    }

    #[must_use]
    pub const fn undo_depth(&self) -> usize {
        self.undo_depth
    }

    #[must_use]
    pub const fn redo_depth(&self) -> usize {
        self.redo_depth
    }

    #[must_use]
    pub const fn can_undo(&self) -> bool {
        self.undo_depth > 0
    }

    #[must_use]
    pub const fn can_redo(&self) -> bool {
        self.redo_depth > 0
    }
}

/// Revision-scoped public workflow permit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPermitState {
    lifetime: u64,
    state_revision: u64,
}

impl WorkflowPermitState {
    #[must_use]
    pub const fn lifetime(&self) -> u64 {
        self.lifetime
    }

    #[must_use]
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }
}

/// Exact public workflow denial evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowDenialState {
    phase: EngineLifecyclePhase,
    blocking_subsystem: Option<EngineSubsystem>,
    failure: Option<IntegrationFailureState>,
}

impl WorkflowDenialState {
    #[must_use]
    pub const fn phase(&self) -> EngineLifecyclePhase {
        self.phase
    }

    #[must_use]
    pub const fn blocking_subsystem(&self) -> Option<EngineSubsystem> {
        self.blocking_subsystem
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&IntegrationFailureState> {
        self.failure.as_ref()
    }
}

/// Current exact admission state for one coherent engine workflow.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowValidationState {
    workflow: EngineWorkflow,
    permit: Option<WorkflowPermitState>,
    denial: Option<WorkflowDenialState>,
}

impl WorkflowValidationState {
    #[must_use]
    pub const fn workflow(&self) -> EngineWorkflow {
        self.workflow
    }

    #[must_use]
    pub const fn is_available(&self) -> bool {
        self.permit.is_some()
    }

    #[must_use]
    pub const fn permit(&self) -> Option<&WorkflowPermitState> {
        self.permit.as_ref()
    }

    #[must_use]
    pub const fn denial(&self) -> Option<&WorkflowDenialState> {
        self.denial.as_ref()
    }
}

/// Exact public classified recovery token.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryActionState {
    sequence: u64,
    subsystem: EngineSubsystem,
    request: EngineRecoveryRequest,
    attempt: u32,
}

impl RecoveryActionState {
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn subsystem(&self) -> EngineSubsystem {
        self.subsystem
    }

    #[must_use]
    pub const fn request(&self) -> EngineRecoveryRequest {
        self.request
    }

    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }
}

/// Classified recovery state that supplements user-safe failures in engine introspection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryValidationState {
    revision: u64,
    diagnostic_event_count: usize,
    pending_recovery: Option<RecoveryActionState>,
}

impl RecoveryValidationState {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn diagnostic_event_count(&self) -> usize {
        self.diagnostic_event_count
    }

    #[must_use]
    pub const fn pending_recovery(&self) -> Option<&RecoveryActionState> {
        self.pending_recovery.as_ref()
    }
}

/// Exact rational coordinate for transport state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RationalCoordinateState {
    value: i64,
    timebase_numerator: u32,
    timebase_denominator: u32,
}

impl RationalCoordinateState {
    #[must_use]
    pub const fn value(&self) -> i64 {
        self.value
    }

    #[must_use]
    pub const fn timebase_numerator(&self) -> u32 {
        self.timebase_numerator
    }

    #[must_use]
    pub const fn timebase_denominator(&self) -> u32 {
        self.timebase_denominator
    }
}

/// Latest complete playback transport state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaybackTransportValidationState {
    mode: String,
    playhead: RationalCoordinateState,
    scheduled_frame: Option<RationalCoordinateState>,
    scheduled_due_clock: Option<RationalCoordinateState>,
    rate_numerator: i64,
    rate_denominator: u64,
    epoch: u64,
    audio_state: String,
    degradations: Vec<String>,
}

impl PlaybackTransportValidationState {
    #[must_use]
    pub fn mode(&self) -> &str {
        &self.mode
    }

    #[must_use]
    pub const fn playhead(&self) -> &RationalCoordinateState {
        &self.playhead
    }

    #[must_use]
    pub const fn scheduled_frame(&self) -> Option<&RationalCoordinateState> {
        self.scheduled_frame.as_ref()
    }

    #[must_use]
    pub const fn scheduled_due_clock(&self) -> Option<&RationalCoordinateState> {
        self.scheduled_due_clock.as_ref()
    }

    #[must_use]
    pub const fn rate_numerator(&self) -> i64 {
        self.rate_numerator
    }

    #[must_use]
    pub const fn rate_denominator(&self) -> u64 {
        self.rate_denominator
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    #[must_use]
    pub fn audio_state(&self) -> &str {
        &self.audio_state
    }

    #[must_use]
    pub fn degradations(&self) -> &[String] {
        &self.degradations
    }
}

/// Attached, pending, and latest observed playback state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaybackValidationState {
    attached: bool,
    command_pending: bool,
    latest: Option<PlaybackTransportValidationState>,
    latest_failure: Option<IntegrationFailureState>,
}

impl PlaybackValidationState {
    #[must_use]
    pub const fn is_attached(&self) -> bool {
        self.attached
    }

    #[must_use]
    pub const fn command_pending(&self) -> bool {
        self.command_pending
    }

    #[must_use]
    pub const fn latest(&self) -> Option<&PlaybackTransportValidationState> {
        self.latest.as_ref()
    }

    #[must_use]
    pub const fn latest_failure(&self) -> Option<&IntegrationFailureState> {
        self.latest_failure.as_ref()
    }
}

/// Public state for one retained logical export job.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportJobValidationState {
    job_id: String,
    status: String,
    attempt: u32,
    completed_units: u64,
    total_units: Option<u64>,
    progress_revision: u64,
    dependencies: Vec<String>,
    has_result: bool,
    retry_allowed: bool,
    is_final: bool,
    failure: Option<IntegrationFailureState>,
}

impl ExportJobValidationState {
    #[must_use]
    pub fn job_id(&self) -> &str {
        &self.job_id
    }

    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    #[must_use]
    pub const fn completed_units(&self) -> u64 {
        self.completed_units
    }

    #[must_use]
    pub const fn total_units(&self) -> Option<u64> {
        self.total_units
    }

    #[must_use]
    pub const fn progress_revision(&self) -> u64 {
        self.progress_revision
    }

    #[must_use]
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    #[must_use]
    pub const fn has_result(&self) -> bool {
        self.has_result
    }

    #[must_use]
    pub const fn retry_allowed(&self) -> bool {
        self.retry_allowed
    }

    #[must_use]
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&IntegrationFailureState> {
        self.failure.as_ref()
    }
}

/// Latest complete export queue replacement state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportQueueValidationState {
    revision: u64,
    jobs: Vec<ExportJobValidationState>,
}

impl ExportQueueValidationState {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn jobs(&self) -> &[ExportJobValidationState] {
        &self.jobs
    }
}

/// Attached and latest observed export state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportValidationState {
    attached: bool,
    latest: Option<ExportQueueValidationState>,
}

impl ExportValidationState {
    #[must_use]
    pub const fn is_attached(&self) -> bool {
        self.attached
    }

    #[must_use]
    pub const fn latest(&self) -> Option<&ExportQueueValidationState> {
        self.latest.as_ref()
    }
}

/// One public integration coherence finding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationFindingState {
    code: String,
    message: String,
}

impl IntegrationFindingState {
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Complete strict integration validation state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationValidationSnapshot {
    schema_version: SemanticVersion,
    condition: IntegrationCondition,
    coherent: bool,
    engine: EngineIntrospectionSnapshot,
    scenario: ScenarioValidationState,
    pending_action: Option<LifecycleActionState>,
    recovery: RecoveryValidationState,
    workflows: Vec<WorkflowValidationState>,
    playback: PlaybackValidationState,
    export: ExportValidationState,
    findings: Vec<IntegrationFindingState>,
}

impl IntegrationValidationSnapshot {
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    #[must_use]
    pub const fn condition(&self) -> IntegrationCondition {
        self.condition
    }

    #[must_use]
    pub const fn is_coherent(&self) -> bool {
        self.coherent
    }

    #[must_use]
    pub const fn engine(&self) -> &EngineIntrospectionSnapshot {
        &self.engine
    }

    #[must_use]
    pub const fn scenario(&self) -> &ScenarioValidationState {
        &self.scenario
    }

    #[must_use]
    pub const fn pending_action(&self) -> Option<&LifecycleActionState> {
        self.pending_action.as_ref()
    }

    #[must_use]
    pub const fn recovery(&self) -> &RecoveryValidationState {
        &self.recovery
    }

    #[must_use]
    pub fn workflows(&self) -> &[WorkflowValidationState] {
        &self.workflows
    }

    #[must_use]
    pub fn workflow(&self, workflow: EngineWorkflow) -> Option<&WorkflowValidationState> {
        self.workflows
            .iter()
            .find(|state| state.workflow == workflow)
    }

    #[must_use]
    pub const fn playback(&self) -> &PlaybackValidationState {
        &self.playback
    }

    #[must_use]
    pub const fn export(&self) -> &ExportValidationState {
        &self.export
    }

    #[must_use]
    pub fn findings(&self) -> &[IntegrationFindingState] {
        &self.findings
    }
}

/// Structured public failure while constructing a standalone validation host.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationValidationFailure {
    category: String,
    recoverability: String,
    message: String,
}

impl IntegrationValidationFailure {
    fn from_error(error: &Error) -> Self {
        let safe = UserSafeError::from_error(error);
        Self {
            category: error.category().code().to_owned(),
            recoverability: error.recoverability().code().to_owned(),
            message: safe.title().to_owned(),
        }
    }

    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    #[must_use]
    pub fn recoverability(&self) -> &str {
        &self.recoverability
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Public validation query owner over one immutable engine observation.
#[derive(Clone, Debug)]
pub struct IntegrationValidationApi {
    snapshot: IntegrationValidationSnapshot,
}

impl IntegrationValidationApi {
    /// Creates a UI or test query owner from an existing canonical engine observation.
    #[must_use]
    pub fn new(engine: &EngineSnapshot) -> Self {
        Self {
            snapshot: public_snapshot(engine),
        }
    }

    /// Creates the standalone starting-engine observation consumed by the CLI.
    pub fn from_fresh_engine() -> Result<Self, IntegrationValidationFailure> {
        let engine = EngineCommandDispatcher::standalone_integration_validation_snapshot()
            .map_err(|error| IntegrationValidationFailure::from_error(&error))?;
        Ok(Self::new(&engine))
    }

    /// Executes one strict read-only validation query.
    #[must_use]
    pub fn execute(
        &self,
        _command: GetEngineIntegrationValidation,
    ) -> GetEngineIntegrationValidationResult {
        GetEngineIntegrationValidationResult::new(self.snapshot.clone())
    }
}

fn public_snapshot(engine: &EngineSnapshot) -> IntegrationValidationSnapshot {
    IntegrationValidationSnapshot {
        schema_version: ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION.clone(),
        condition: public_condition(engine.condition()),
        coherent: engine.is_coherent(),
        engine: public_engine_snapshot(engine.introspection(), 0, 0),
        scenario: ScenarioValidationState {
            revision: engine.scenario().revision(),
            phase: engine.scenario().phase().code().to_owned(),
            operation_count: engine.scenario().operation_log().len(),
            undo_depth: engine.scenario().undo_depth(),
            redo_depth: engine.scenario().redo_depth(),
        },
        pending_action: engine
            .lifecycle()
            .pending_action()
            .map(public_lifecycle_action),
        recovery: RecoveryValidationState {
            revision: engine.recovery().revision(),
            diagnostic_event_count: engine.recovery().diagnostic_history().len(),
            pending_recovery: engine
                .recovery()
                .pending_recovery()
                .map(public_recovery_action),
        },
        workflows: engine
            .workflows()
            .iter()
            .map(|state| WorkflowValidationState {
                workflow: public_workflow(state.work()),
                permit: state
                    .admission()
                    .permit()
                    .map(|permit| WorkflowPermitState {
                        lifetime: permit.lifetime(),
                        state_revision: permit.state_revision(),
                    }),
                denial: state
                    .admission()
                    .denial()
                    .map(|denial| WorkflowDenialState {
                        phase: phase_from_code(denial.phase().code()),
                        blocking_subsystem: denial.blocking_subsystem().map(public_subsystem),
                        failure: denial.failure().map(public_lifecycle_failure),
                    }),
            })
            .collect(),
        playback: PlaybackValidationState {
            attached: engine.playback().is_attached(),
            command_pending: engine.playback().command_pending(),
            latest: engine
                .playback()
                .latest_snapshot()
                .map(public_playback_snapshot),
            latest_failure: engine
                .playback()
                .latest_failure()
                .map(public_reported_failure),
        },
        export: ExportValidationState {
            attached: engine.export().is_attached(),
            latest: engine
                .export()
                .latest_state()
                .map(|state| ExportQueueValidationState {
                    revision: state.revision(),
                    jobs: state
                        .jobs()
                        .iter()
                        .map(|job| {
                            let progress = job.progress();
                            ExportJobValidationState {
                                job_id: job.job_id().to_string(),
                                status: job.status().code().to_owned(),
                                attempt: job.attempt(),
                                completed_units: progress.completed_units(),
                                total_units: progress.total_units(),
                                progress_revision: progress.revision(),
                                dependencies: job
                                    .dependencies()
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect(),
                                has_result: job.has_result(),
                                retry_allowed: job.retry_allowed(),
                                is_final: job.is_final(),
                                failure: job.failure().map(public_reported_failure),
                            }
                        })
                        .collect(),
                }),
        },
        findings: engine.findings().iter().map(public_finding).collect(),
    }
}

fn public_lifecycle_action(engine: EngineLifecycleAction) -> LifecycleActionState {
    LifecycleActionState {
        subsystem: public_subsystem(engine.subsystem()),
        kind: public_action_kind(engine.kind().code()),
        lifetime: engine.lifetime(),
        revision: engine.revision(),
    }
}

fn public_lifecycle_failure(engine: &EngineFailureSnapshot) -> IntegrationFailureState {
    IntegrationFailureState {
        category: engine.category().code().to_owned(),
        recoverability: engine.recoverability().code().to_owned(),
    }
}

fn public_reported_failure(engine: &EngineReportedFailure) -> IntegrationFailureState {
    IntegrationFailureState {
        category: engine.category().code().to_owned(),
        recoverability: engine.recoverability().code().to_owned(),
    }
}

fn public_recovery_action(engine: EngineRecoveryAction) -> RecoveryActionState {
    RecoveryActionState {
        sequence: engine.sequence().get(),
        subsystem: public_subsystem(engine.subsystem()),
        request: public_recovery_request(engine.request()),
        attempt: engine.attempt(),
    }
}

fn public_playback_snapshot(engine: PlaybackTransportSnapshot) -> PlaybackTransportValidationState {
    let degradation = engine.degradation();
    let mut degradations = Vec::new();
    for (code, name) in [
        (TransportDegradationCode::FrameFailure, "frame_failure"),
        (
            TransportDegradationCode::ViewportBackpressure,
            "viewport_backpressure",
        ),
        (
            TransportDegradationCode::PrefetchFailure,
            "prefetch_failure",
        ),
        (
            TransportDegradationCode::AudioDiscardPending,
            "audio_discard_pending",
        ),
        (
            TransportDegradationCode::AudioRateUnsupported,
            "audio_rate_unsupported",
        ),
    ] {
        if degradation.contains(code) {
            degradations.push(name.to_owned());
        }
    }
    PlaybackTransportValidationState {
        mode: playback_mode(engine.mode()).to_owned(),
        playhead: public_coordinate(engine.playhead()),
        scheduled_frame: engine.scheduled_frame().map(public_coordinate),
        scheduled_due_clock: engine.scheduled_due_clock().map(public_coordinate),
        rate_numerator: engine.rate().numerator(),
        rate_denominator: engine.rate().denominator(),
        epoch: engine.epoch(),
        audio_state: audio_state(engine.audio_state()).to_owned(),
        degradations,
    }
}

fn public_coordinate(engine: superi_core::time::RationalTime) -> RationalCoordinateState {
    RationalCoordinateState {
        value: engine.value(),
        timebase_numerator: engine.timebase().numerator(),
        timebase_denominator: engine.timebase().denominator(),
    }
}

fn public_finding(engine: &EngineFinding) -> IntegrationFindingState {
    IntegrationFindingState {
        code: engine.code().code().to_owned(),
        message: engine.message().to_owned(),
    }
}

fn public_condition(engine: EngineCondition) -> IntegrationCondition {
    match engine {
        EngineCondition::Starting => IntegrationCondition::Starting,
        EngineCondition::Normal => IntegrationCondition::Normal,
        EngineCondition::Degraded => IntegrationCondition::Degraded,
        EngineCondition::Recovering => IntegrationCondition::Recovering,
        EngineCondition::Pausing => IntegrationCondition::Pausing,
        EngineCondition::Paused => IntegrationCondition::Paused,
        EngineCondition::Resuming => IntegrationCondition::Resuming,
        EngineCondition::PreparingSleep => IntegrationCondition::PreparingSleep,
        EngineCondition::Sleeping => IntegrationCondition::Sleeping,
        EngineCondition::Waking => IntegrationCondition::Waking,
        EngineCondition::Stopping => IntegrationCondition::Stopping,
        EngineCondition::Stopped => IntegrationCondition::Stopped,
        EngineCondition::Failed => IntegrationCondition::Failed,
        _ => IntegrationCondition::Unknown,
    }
}

fn phase_from_code(code: &str) -> EngineLifecyclePhase {
    match code {
        "starting" => EngineLifecyclePhase::Starting,
        "running" => EngineLifecyclePhase::Running,
        "pausing" => EngineLifecyclePhase::Pausing,
        "paused" => EngineLifecyclePhase::Paused,
        "resuming" => EngineLifecyclePhase::Resuming,
        "preparing_sleep" => EngineLifecyclePhase::PreparingSleep,
        "sleeping" => EngineLifecyclePhase::Sleeping,
        "waking" => EngineLifecyclePhase::Waking,
        "stopping" => EngineLifecyclePhase::Stopping,
        "stopped" => EngineLifecyclePhase::Stopped,
        "failed" => EngineLifecyclePhase::Failed,
        _ => EngineLifecyclePhase::Unknown,
    }
}

fn public_action_kind(code: &str) -> IntegrationLifecycleActionKind {
    match code {
        "initialize" => IntegrationLifecycleActionKind::Initialize,
        "recover" => IntegrationLifecycleActionKind::Recover,
        "prepare_sleep" => IntegrationLifecycleActionKind::PrepareSleep,
        "wake" => IntegrationLifecycleActionKind::Wake,
        "teardown" => IntegrationLifecycleActionKind::Teardown,
        _ => IntegrationLifecycleActionKind::Unknown,
    }
}

fn playback_mode(mode: PlaybackTransportMode) -> &'static str {
    match mode {
        PlaybackTransportMode::Paused => "paused",
        PlaybackTransportMode::Playing => "playing",
        PlaybackTransportMode::Scrubbing => "scrubbing",
        PlaybackTransportMode::Ended => "ended",
        _ => "unknown",
    }
}

fn audio_state(state: TransportAudioState) -> &'static str {
    match state {
        TransportAudioState::Synchronized => "synchronized",
        TransportAudioState::DiscardPending(_) => "discard_pending",
        TransportAudioState::MutedInactive(_) => "muted_inactive",
        TransportAudioState::MutedUnsupportedRate(_) => "muted_unsupported_rate",
        _ => "unknown",
    }
}
