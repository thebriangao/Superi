//! Strict transport-neutral API for canonical editorial slice state.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use superi_core::diagnostics::UserSafeError;
use superi_core::error::Error;
use superi_core::settings::SemanticVersion;
use superi_core::time::FrameRate;
use superi_engine::command::{
    GraphEffect as EngineGraphEffect, GraphNodeKind as EngineGraphNodeKind,
    OperationArguments as EngineOperationArguments, ScenarioAction as EngineAction,
    ScenarioPhase as EnginePhase, ScenarioSnapshot as EngineSnapshot,
    SliceImplementation as EngineImplementation,
};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineTransactionId, ScenarioTransaction as EngineScenarioTransaction,
};

use crate::commands::{ExecuteScenarioAction, ExecuteScenarioTransaction};
use crate::events::ScenarioStateChanged;
use crate::version::SLICE_SCENARIO_SCHEMA_VERSION;

/// Maximum actions accepted in one scenario document.
pub const MAX_SCENARIO_ACTIONS: usize = 64;

/// Exact rational frame rate carried by scenario JSON.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactFrameRate {
    numerator: u32,
    denominator: u32,
}

impl ExactFrameRate {
    /// Creates an exact rational frame rate.
    #[must_use]
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    /// Returns the frames-per-second numerator.
    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    /// Returns the frames-per-second denominator.
    #[must_use]
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    fn from_engine(value: FrameRate) -> Self {
        Self {
            numerator: value.numerator(),
            denominator: value.denominator(),
        }
    }

    fn into_engine(self) -> superi_core::error::Result<FrameRate> {
        FrameRate::new(self.numerator, self.denominator)
    }
}

/// Typed graph effect supported by canonical scenario schema 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SliceGraphEffect {
    /// Mirror the canonical image horizontally.
    HorizontalMirror,
    /// A newer engine effect not understood by this API version.
    Unknown,
}

impl SliceGraphEffect {
    /// Returns the stable effect code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::HorizontalMirror => "horizontal_mirror",
            Self::Unknown => "unknown",
        }
    }
}

/// Stable action discriminator for output and diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SliceActionKind {
    /// Import the canonical source clip.
    ImportClip,
    /// Place the clip on the canonical track.
    PlaceClip,
    /// Apply the canonical source trim.
    TrimClip,
    /// Attach the canonical graph effect.
    ApplyGraphEffect,
    /// Reverse the latest committed mutation.
    Undo,
    /// Reapply the latest reversed mutation.
    Redo,
    /// Read state without mutation.
    Inspect,
}

impl SliceActionKind {
    /// Returns the stable action code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::ImportClip => "import_clip",
            Self::PlaceClip => "place_clip",
            Self::TrimClip => "trim_clip",
            Self::ApplyGraphEffect => "apply_graph_effect",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Inspect => "inspect",
        }
    }
}

/// One strict canonical editorial action.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum SliceAction {
    /// Validate and identify the canonical fixture payload.
    ImportClip {
        /// Local payload path.
        path: String,
        /// Authoritative fixture identifier.
        fixture_id: String,
        /// Authoritative fixture revision.
        fixture_version: u32,
        /// Complete fixture manifest digest.
        manifest_sha256: String,
        /// Fixture payload digest.
        payload_sha256: String,
        /// Exact source frame rate.
        frame_rate: ExactFrameRate,
        /// Exact source frame count.
        frame_count: u64,
        /// Exact source width.
        width: u32,
        /// Exact source height.
        height: u32,
    },
    /// Place the imported clip on the canonical timeline.
    PlaceClip {
        /// Exact timeline frame.
        timeline_start_frame: i64,
    },
    /// Apply the canonical half-open source range.
    TrimClip {
        /// Inclusive source frame.
        source_start_frame: u64,
        /// Exclusive source frame.
        source_end_frame: u64,
    },
    /// Attach the canonical graph effect.
    ApplyGraphEffect {
        /// Effect type.
        effect: SliceGraphEffect,
    },
    /// Reverse the latest committed mutation.
    Undo {},
    /// Reapply the latest reversed mutation.
    Redo {},
    /// Return current state without changing revision or history.
    Inspect {},
}

impl SliceAction {
    /// Returns the stable action discriminator.
    #[must_use]
    pub const fn kind(&self) -> SliceActionKind {
        match self {
            Self::ImportClip { .. } => SliceActionKind::ImportClip,
            Self::PlaceClip { .. } => SliceActionKind::PlaceClip,
            Self::TrimClip { .. } => SliceActionKind::TrimClip,
            Self::ApplyGraphEffect { .. } => SliceActionKind::ApplyGraphEffect,
            Self::Undo {} => SliceActionKind::Undo,
            Self::Redo {} => SliceActionKind::Redo,
            Self::Inspect {} => SliceActionKind::Inspect,
        }
    }
}

/// Versioned ordered scenario input for public control clients.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDocument {
    schema_version: SemanticVersion,
    actions: Vec<SliceAction>,
}

impl ScenarioDocument {
    /// Returns the exact scenario schema.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns actions in execution order.
    #[must_use]
    pub fn actions(&self) -> &[SliceAction] {
        &self.actions
    }

    /// Validates schema support and bounded action count.
    ///
    /// # Errors
    ///
    /// Returns a structured public failure for unsupported schema or unbounded input.
    pub fn validate(&self) -> Result<(), ScenarioFailure> {
        if self.schema_version != SLICE_SCENARIO_SCHEMA_VERSION {
            return Err(ScenarioFailure::plain(
                "unsupported",
                "user_correctable",
                "scenario schema version is not supported",
                empty_state(),
            ));
        }
        if self.actions.is_empty() {
            return Err(ScenarioFailure::plain(
                "invalid_input",
                "user_correctable",
                "scenario must contain at least one action",
                empty_state(),
            ));
        }
        if self.actions.len() > MAX_SCENARIO_ACTIONS {
            return Err(ScenarioFailure::plain(
                "resource_exhausted",
                "user_correctable",
                "scenario exceeds the bounded action count",
                empty_state(),
            ));
        }
        Ok(())
    }
}

/// Public implementation ownership for canonical editorial state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SliceImplementation {
    /// Deterministic boundary behavior awaiting a production subsystem replacement.
    Reference,
    /// A newer engine owner not understood by this API version.
    Unknown,
}

/// Highest completed phase in public scenario state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ScenarioPhase {
    /// No source is imported.
    Empty,
    /// Canonical source identity and timing are known.
    Imported,
    /// The canonical clip is placed.
    Placed,
    /// The canonical source range is trimmed.
    Trimmed,
    /// The canonical graph effect is attached.
    Effected,
    /// A newer engine phase not understood by this API version.
    Unknown,
}

/// Public imported-media state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedMediaState {
    media_id: String,
    path: String,
    fixture_id: String,
    fixture_version: u32,
    manifest_sha256: String,
    payload_sha256: String,
    frame_rate: ExactFrameRate,
    frame_count: u64,
    width: u32,
    height: u32,
    implementation: SliceImplementation,
}

impl ImportedMediaState {
    /// Returns stable media identity.
    #[must_use]
    pub fn media_id(&self) -> &str {
        &self.media_id
    }
    /// Returns the source path supplied by the client.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
    /// Returns the canonical fixture identifier.
    #[must_use]
    pub fn fixture_id(&self) -> &str {
        &self.fixture_id
    }
    /// Returns the canonical fixture revision.
    #[must_use]
    pub const fn fixture_version(&self) -> u32 {
        self.fixture_version
    }
    /// Returns the complete manifest digest.
    #[must_use]
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    /// Returns the fixture payload digest.
    #[must_use]
    pub fn payload_sha256(&self) -> &str {
        &self.payload_sha256
    }
    /// Returns the exact source frame rate.
    #[must_use]
    pub const fn frame_rate(&self) -> ExactFrameRate {
        self.frame_rate
    }
    /// Returns the source frame count.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }
    /// Returns the source extent.
    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    /// Returns current import implementation ownership.
    #[must_use]
    pub const fn implementation(&self) -> SliceImplementation {
        self.implementation
    }
}

/// Public canonical timeline state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineState {
    track_id: String,
    clip_id: String,
    timeline_name: String,
    track_name: String,
    clip_name: String,
    edit_rate: ExactFrameRate,
    canvas_width: u32,
    canvas_height: u32,
    timeline_start_frame: i64,
    source_start_frame: u64,
    source_end_frame: u64,
    timeline_end_frame: i64,
    implementation: SliceImplementation,
}

impl TimelineState {
    /// Returns stable typed track identity.
    #[must_use]
    pub fn track_id(&self) -> &str {
        &self.track_id
    }
    /// Returns stable typed clip identity.
    #[must_use]
    pub fn clip_id(&self) -> &str {
        &self.clip_id
    }
    /// Returns the canonical timeline name.
    #[must_use]
    pub fn timeline_name(&self) -> &str {
        &self.timeline_name
    }
    /// Returns the canonical track name.
    #[must_use]
    pub fn track_name(&self) -> &str {
        &self.track_name
    }
    /// Returns the canonical clip name.
    #[must_use]
    pub fn clip_name(&self) -> &str {
        &self.clip_name
    }
    /// Returns the exact edit rate.
    #[must_use]
    pub const fn edit_rate(&self) -> ExactFrameRate {
        self.edit_rate
    }
    /// Returns the canonical canvas extent.
    #[must_use]
    pub const fn canvas(&self) -> (u32, u32) {
        (self.canvas_width, self.canvas_height)
    }
    /// Returns the half-open source range.
    #[must_use]
    pub const fn source_range(&self) -> (u64, u64) {
        (self.source_start_frame, self.source_end_frame)
    }
    /// Returns the half-open timeline range.
    #[must_use]
    pub const fn timeline_range(&self) -> (i64, i64) {
        (self.timeline_start_frame, self.timeline_end_frame)
    }
    /// Returns current timeline implementation ownership.
    #[must_use]
    pub const fn implementation(&self) -> SliceImplementation {
        self.implementation
    }
}

/// Public graph node role.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GraphNodeKind {
    Source,
    Effect,
    Output,
    Unknown,
}

/// Public canonical graph node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphNodeState {
    node_id: String,
    instance_id: String,
    kind: GraphNodeKind,
    node_type: String,
    schema_version: u32,
    input_ports: Vec<String>,
    output_ports: Vec<String>,
}

impl GraphNodeState {
    /// Returns the typed node identity.
    #[must_use]
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
    /// Returns the stable graph instance identifier.
    #[must_use]
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    /// Returns the node role.
    #[must_use]
    pub const fn kind(&self) -> GraphNodeKind {
        self.kind
    }
    /// Returns the stable node type.
    #[must_use]
    pub fn node_type(&self) -> &str {
        &self.node_type
    }
    /// Returns the node schema revision.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
    /// Returns ordered input ports.
    #[must_use]
    pub fn input_ports(&self) -> &[String] {
        &self.input_ports
    }
    /// Returns ordered output ports.
    #[must_use]
    pub fn output_ports(&self) -> &[String] {
        &self.output_ports
    }
}

/// Public canonical graph edge.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphEdgeState {
    from_node: String,
    from_port: String,
    to_node: String,
    to_port: String,
}

impl GraphEdgeState {
    /// Returns the source node.
    #[must_use]
    pub fn from_node(&self) -> &str {
        &self.from_node
    }
    /// Returns the source port.
    #[must_use]
    pub fn from_port(&self) -> &str {
        &self.from_port
    }
    /// Returns the destination node.
    #[must_use]
    pub fn to_node(&self) -> &str {
        &self.to_node
    }
    /// Returns the destination port.
    #[must_use]
    pub fn to_port(&self) -> &str {
        &self.to_port
    }
}

/// Public complete graph state for the canonical mirror.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphState {
    nodes: Vec<GraphNodeState>,
    edges: Vec<GraphEdgeState>,
    effect: SliceGraphEffect,
    matrix: [f64; 9],
    sampling: String,
    edge_mode: String,
    output_width: u32,
    output_height: u32,
    derived_timeline_identity: String,
    implementation: SliceImplementation,
}

impl GraphState {
    /// Returns ordered graph nodes.
    #[must_use]
    pub fn nodes(&self) -> &[GraphNodeState] {
        &self.nodes
    }
    /// Returns ordered graph edges.
    #[must_use]
    pub fn edges(&self) -> &[GraphEdgeState] {
        &self.edges
    }
    /// Returns the typed graph effect.
    #[must_use]
    pub const fn effect(&self) -> SliceGraphEffect {
        self.effect
    }
    /// Returns the row-major binary64 transform matrix.
    #[must_use]
    pub const fn matrix(&self) -> [f64; 9] {
        self.matrix
    }
    /// Returns the sampling policy.
    #[must_use]
    pub fn sampling(&self) -> &str {
        &self.sampling
    }
    /// Returns the out-of-bounds edge policy.
    #[must_use]
    pub fn edge_mode(&self) -> &str {
        &self.edge_mode
    }
    /// Returns the graph output extent.
    #[must_use]
    pub const fn output_extent(&self) -> (u32, u32) {
        (self.output_width, self.output_height)
    }
    /// Returns the derived timeline identity.
    #[must_use]
    pub fn derived_timeline_identity(&self) -> &str {
        &self.derived_timeline_identity
    }
    /// Returns current graph implementation ownership.
    #[must_use]
    pub const fn implementation(&self) -> SliceImplementation {
        self.implementation
    }
}

/// Public typed arguments retained for operation evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationArguments {
    /// Canonical import arguments.
    Import {
        fixture_id: String,
        fixture_version: u32,
        manifest_sha256: String,
        payload_sha256: String,
    },
    /// Canonical placement arguments.
    Insert { timeline_start_frame: i64 },
    /// Canonical trim arguments.
    Trim {
        source_start_frame: u64,
        source_end_frame: u64,
    },
    /// Canonical effect arguments.
    Effect { effect: SliceGraphEffect },
    /// A newer engine argument shape not understood by this API version.
    Unknown,
}

/// Public reversible operation evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRecord {
    operation_id: String,
    resulting_revision: u64,
    arguments: OperationArguments,
    prior_phase: ScenarioPhase,
    result_phase: ScenarioPhase,
}

impl OperationRecord {
    /// Returns the canonical operation identifier.
    #[must_use]
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }
    /// Returns the revision produced by the original mutation.
    #[must_use]
    pub const fn resulting_revision(&self) -> u64 {
        self.resulting_revision
    }
    /// Returns the typed original arguments.
    #[must_use]
    pub const fn arguments(&self) -> &OperationArguments {
        &self.arguments
    }
    /// Returns the phase before the mutation.
    #[must_use]
    pub const fn prior_phase(&self) -> ScenarioPhase {
        self.prior_phase
    }
    /// Returns the phase after the mutation.
    #[must_use]
    pub const fn result_phase(&self) -> ScenarioPhase {
        self.result_phase
    }
}

/// Complete strict public scenario state after one action.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioStateSnapshot {
    schema_version: SemanticVersion,
    revision: u64,
    phase: ScenarioPhase,
    project_id: String,
    media: Option<ImportedMediaState>,
    timeline: Option<TimelineState>,
    graph: Option<GraphState>,
    operation_log: Vec<OperationRecord>,
    undo_depth: usize,
    redo_depth: usize,
}

impl ScenarioStateSnapshot {
    /// Returns the state schema.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }
    /// Returns the successful-action revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Returns the highest completed slice phase.
    #[must_use]
    pub const fn phase(&self) -> ScenarioPhase {
        self.phase
    }
    /// Returns stable project identity.
    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }
    /// Returns imported source state when present.
    #[must_use]
    pub const fn media(&self) -> Option<&ImportedMediaState> {
        self.media.as_ref()
    }
    /// Returns timeline state when present.
    #[must_use]
    pub const fn timeline(&self) -> Option<&TimelineState> {
        self.timeline.as_ref()
    }
    /// Returns graph state when present.
    #[must_use]
    pub const fn graph(&self) -> Option<&GraphState> {
        self.graph.as_ref()
    }
    /// Returns the active operation log.
    #[must_use]
    pub fn operation_log(&self) -> &[OperationRecord] {
        &self.operation_log
    }
    /// Returns available undo count.
    #[must_use]
    pub const fn undo_depth(&self) -> usize {
        self.undo_depth
    }
    /// Returns available redo count.
    #[must_use]
    pub const fn redo_depth(&self) -> usize {
        self.redo_depth
    }
}

/// Successful result for one typed action command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioActionResult {
    action: SliceActionKind,
    state: ScenarioStateSnapshot,
}

/// Successful result for one optimistic ordered scenario transaction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTransactionResult {
    transaction_id: String,
    command_sequence: u64,
    actions: Vec<SliceActionKind>,
    state: ScenarioStateSnapshot,
}

impl ScenarioTransactionResult {
    /// Returns the exact caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the monotonic successful-command sequence.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns action kinds in committed order.
    #[must_use]
    pub fn actions(&self) -> &[SliceActionKind] {
        &self.actions
    }

    /// Returns complete state after the transaction.
    #[must_use]
    pub const fn state(&self) -> &ScenarioStateSnapshot {
        &self.state
    }
}

impl ScenarioActionResult {
    /// Returns the executed action kind.
    #[must_use]
    pub const fn action(&self) -> SliceActionKind {
        self.action
    }
    /// Returns complete state after the action.
    #[must_use]
    pub const fn state(&self) -> &ScenarioStateSnapshot {
        &self.state
    }
}

/// One public error context frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFailureContext {
    component: String,
    operation: String,
    fields: BTreeMap<String, String>,
}

impl ScenarioFailureContext {
    /// Returns the failing component.
    #[must_use]
    pub fn component(&self) -> &str {
        &self.component
    }
    /// Returns the failing operation.
    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }
    /// Returns deterministic context fields.
    #[must_use]
    pub const fn fields(&self) -> &BTreeMap<String, String> {
        &self.fields
    }
}

/// Structured public failure that retains the last valid state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFailure {
    category: String,
    recoverability: String,
    message: String,
    contexts: Vec<ScenarioFailureContext>,
    state: Box<ScenarioStateSnapshot>,
}

impl ScenarioFailure {
    fn plain(
        category: &str,
        recoverability: &str,
        message: &str,
        state: ScenarioStateSnapshot,
    ) -> Self {
        Self {
            category: category.to_owned(),
            recoverability: recoverability.to_owned(),
            message: message.to_owned(),
            contexts: Vec::new(),
            state: Box::new(state),
        }
    }

    fn from_error(error: &Error, state: ScenarioStateSnapshot) -> Self {
        let safe = UserSafeError::from_error(error);
        Self {
            category: error.category().code().to_owned(),
            recoverability: error.recoverability().code().to_owned(),
            message: safe.title().to_owned(),
            contexts: error
                .contexts()
                .iter()
                .map(|context| ScenarioFailureContext {
                    component: context.component().to_owned(),
                    operation: context.operation().to_owned(),
                    fields: BTreeMap::new(),
                })
                .collect(),
            state: Box::new(state),
        }
    }

    /// Returns the permanent shared error category code.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }
    /// Returns the permanent recovery code.
    #[must_use]
    pub fn recoverability(&self) -> &str {
        &self.recoverability
    }
    /// Returns the concise failure summary.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Returns context frames in engine-to-client order.
    #[must_use]
    pub fn contexts(&self) -> &[ScenarioFailureContext] {
        &self.contexts
    }
    /// Returns the complete last valid engine state.
    #[must_use]
    pub fn state(&self) -> &ScenarioStateSnapshot {
        self.state.as_ref()
    }
}

/// Mutable public API facade around one engine-owned canonical scenario dispatcher.
pub struct ScenarioApi {
    dispatcher: EngineCommandDispatcher,
    legacy_transaction_sequence: u64,
}

impl ScenarioApi {
    /// Creates an empty scenario API.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the complete current public state.
    #[must_use]
    pub fn snapshot(&self) -> ScenarioStateSnapshot {
        let snapshot = self
            .dispatcher
            .scenario_snapshot()
            .expect("the scenario-only dispatcher has no execution-domain failure");
        public_snapshot(&snapshot)
    }

    /// Executes the same typed command used by CLI, UI, and automation clients.
    ///
    /// # Errors
    ///
    /// Returns a structured failure containing the complete last valid state.
    pub fn execute(
        &mut self,
        command: ExecuteScenarioAction,
    ) -> Result<ScenarioActionResult, ScenarioFailure> {
        let action = command.into_action();
        let kind = action.kind();
        let sequence = self
            .legacy_transaction_sequence
            .checked_add(1)
            .ok_or_else(|| {
                ScenarioFailure::from_error(
                    &Error::new(
                        superi_core::error::ErrorCategory::ResourceExhausted,
                        superi_core::error::Recoverability::Terminal,
                        "legacy scenario transaction sequence is exhausted",
                    ),
                    self.snapshot(),
                )
            })?;
        let transaction = ExecuteScenarioTransaction::new(
            format!("legacy-scenario-{sequence}"),
            self.snapshot().revision(),
            vec![action],
        );
        let result = self.execute_transaction(transaction)?;
        self.legacy_transaction_sequence = sequence;
        Ok(ScenarioActionResult {
            action: kind,
            state: result.state,
        })
    }

    /// Executes one stable typed automation transaction through the engine dispatcher.
    ///
    /// Successful mutations commit as one revision and one undo unit and enqueue one ordered full
    /// replacement event. Failed conversion, validation, revision, or engine work leaves the last
    /// valid state and event stream unchanged.
    ///
    /// # Errors
    ///
    /// Returns a structured failure containing the complete last valid public state.
    pub fn execute_transaction(
        &mut self,
        command: ExecuteScenarioTransaction,
    ) -> Result<ScenarioTransactionResult, ScenarioFailure> {
        let (transaction_id, expected_revision, actions) = command.into_parts();
        let action_kinds = actions.iter().map(SliceAction::kind).collect::<Vec<_>>();
        let inspect_count = actions
            .iter()
            .filter(|action| matches!(action, SliceAction::Inspect {}))
            .count();
        if inspect_count > 0 && (inspect_count != 1 || actions.len() != 1) {
            return Err(ScenarioFailure::plain(
                "invalid_input",
                "user_correctable",
                "inspect must be the only action in a scenario transaction",
                self.snapshot(),
            ));
        }

        let id = EngineTransactionId::new(transaction_id)
            .map_err(|error| ScenarioFailure::from_error(&error, self.snapshot()))?;
        let request = if inspect_count == 1 {
            let actual_revision = self.snapshot().revision();
            if actual_revision != expected_revision {
                return Err(ScenarioFailure::plain(
                    "conflict",
                    "user_correctable",
                    "scenario revision does not match the transaction fence",
                    self.snapshot(),
                ));
            }
            EngineCommandRequest::new(id, EngineCommand::InspectScenario)
        } else {
            let engine_actions = actions
                .into_iter()
                .map(to_engine_action)
                .collect::<superi_core::error::Result<Vec<_>>>()
                .map_err(|error| ScenarioFailure::from_error(&error, self.snapshot()))?;
            let transaction = EngineScenarioTransaction::new(expected_revision, engine_actions)
                .map_err(|error| ScenarioFailure::from_error(&error, self.snapshot()))?;
            EngineCommandRequest::new(id, EngineCommand::ExecuteScenario(transaction))
        };

        let outcome = self
            .dispatcher
            .dispatch(request)
            .map_err(|error| ScenarioFailure::from_error(&error, self.snapshot()))?;
        let snapshot = match outcome.result() {
            EngineCommandResult::Scenario(snapshot) => snapshot,
            _ => {
                return Err(ScenarioFailure::plain(
                    "internal",
                    "terminal",
                    "scenario dispatcher returned a non-scenario result",
                    self.snapshot(),
                ));
            }
        };
        Ok(ScenarioTransactionResult {
            transaction_id: outcome.transaction_id().as_str().to_owned(),
            command_sequence: outcome.command_sequence(),
            actions: action_kinds,
            state: public_snapshot(snapshot),
        })
    }

    /// Drains ordered scenario state events for the current in-process consumer.
    #[must_use]
    pub fn drain_events(&mut self) -> Vec<ScenarioStateChanged> {
        self.dispatcher
            .drain_events()
            .expect("the scenario-only dispatcher has no execution-domain failure")
            .into_iter()
            .map(|envelope| match envelope.event() {
                EngineEvent::ScenarioStateChanged(snapshot) => ScenarioStateChanged::new(
                    envelope.sequence(),
                    envelope.command_sequence(),
                    envelope.transaction_id().as_str().to_owned(),
                    envelope
                        .project_revision()
                        .expect("scenario event carries a scenario revision"),
                    public_snapshot(snapshot),
                ),
                EngineEvent::LifecycleStateChanged(_) => {
                    unreachable!("the scenario-only dispatcher cannot emit lifecycle state")
                }
                _ => unreachable!("the scenario-only dispatcher emitted an unknown event kind"),
            })
            .collect()
    }
}

impl Default for ScenarioApi {
    fn default() -> Self {
        Self {
            dispatcher: EngineCommandDispatcher::scenario_only(),
            legacy_transaction_sequence: 0,
        }
    }
}

impl std::fmt::Debug for ScenarioApi {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScenarioApi")
            .field("revision", &self.snapshot().revision())
            .field(
                "legacy_transaction_sequence",
                &self.legacy_transaction_sequence,
            )
            .finish_non_exhaustive()
    }
}

fn to_engine_action(action: SliceAction) -> superi_core::error::Result<EngineAction> {
    match action {
        SliceAction::ImportClip {
            path,
            fixture_id,
            fixture_version,
            manifest_sha256,
            payload_sha256,
            frame_rate,
            frame_count,
            width,
            height,
        } => Ok(EngineAction::ImportClip {
            path: PathBuf::from(path),
            fixture_id,
            fixture_version,
            manifest_sha256,
            payload_sha256,
            frame_rate: frame_rate.into_engine()?,
            frame_count,
            width,
            height,
        }),
        SliceAction::PlaceClip {
            timeline_start_frame,
        } => Ok(EngineAction::PlaceClip {
            timeline_start_frame,
        }),
        SliceAction::TrimClip {
            source_start_frame,
            source_end_frame,
        } => Ok(EngineAction::TrimClip {
            source_start_frame,
            source_end_frame,
        }),
        SliceAction::ApplyGraphEffect { effect } => Ok(EngineAction::ApplyGraphEffect {
            effect: match effect {
                SliceGraphEffect::HorizontalMirror => EngineGraphEffect::HorizontalMirror,
                SliceGraphEffect::Unknown => {
                    return Err(Error::new(
                        superi_core::error::ErrorCategory::Unsupported,
                        superi_core::error::Recoverability::UserCorrectable,
                        "graph effect is not supported by this API version",
                    ));
                }
            },
        }),
        SliceAction::Undo {} => Ok(EngineAction::Undo),
        SliceAction::Redo {} => Ok(EngineAction::Redo),
        SliceAction::Inspect {} => unreachable!("inspect is handled without an engine mutation"),
    }
}

fn public_snapshot(snapshot: &EngineSnapshot) -> ScenarioStateSnapshot {
    ScenarioStateSnapshot {
        schema_version: SLICE_SCENARIO_SCHEMA_VERSION.clone(),
        revision: snapshot.revision(),
        phase: public_phase(snapshot.phase()),
        project_id: snapshot.project_id().to_string(),
        media: snapshot.media().map(|media| ImportedMediaState {
            media_id: media.id().to_string(),
            path: media.path().display().to_string(),
            fixture_id: media.fixture_id().to_owned(),
            fixture_version: media.fixture_version(),
            manifest_sha256: media.manifest_sha256().to_owned(),
            payload_sha256: media.payload_sha256().to_owned(),
            frame_rate: ExactFrameRate::from_engine(media.frame_rate()),
            frame_count: media.frame_count(),
            width: media.width(),
            height: media.height(),
            implementation: public_implementation(media.implementation()),
        }),
        timeline: snapshot.timeline().map(|timeline| {
            let (_, timeline_end_frame) = timeline.timeline_range();
            TimelineState {
                track_id: timeline.track_id().to_string(),
                clip_id: timeline.clip_id().to_string(),
                timeline_name: timeline.timeline_name().to_owned(),
                track_name: timeline.track_name().to_owned(),
                clip_name: timeline.clip_name().to_owned(),
                edit_rate: ExactFrameRate::from_engine(timeline.edit_rate()),
                canvas_width: timeline.canvas().0,
                canvas_height: timeline.canvas().1,
                timeline_start_frame: timeline.timeline_start_frame(),
                source_start_frame: timeline.source_start_frame(),
                source_end_frame: timeline.source_end_frame(),
                timeline_end_frame,
                implementation: public_implementation(timeline.implementation()),
            }
        }),
        graph: snapshot.graph().map(|graph| GraphState {
            nodes: graph
                .nodes()
                .iter()
                .map(|node| GraphNodeState {
                    node_id: node.id().to_string(),
                    instance_id: node.instance_id().to_owned(),
                    kind: public_node_kind(node.kind()),
                    node_type: node.node_type().to_owned(),
                    schema_version: node.schema_version(),
                    input_ports: node
                        .input_ports()
                        .iter()
                        .map(|port| (*port).to_owned())
                        .collect(),
                    output_ports: node
                        .output_ports()
                        .iter()
                        .map(|port| (*port).to_owned())
                        .collect(),
                })
                .collect(),
            edges: graph
                .edges()
                .iter()
                .map(|edge| GraphEdgeState {
                    from_node: edge.from_node().to_owned(),
                    from_port: edge.from_port().to_owned(),
                    to_node: edge.to_node().to_owned(),
                    to_port: edge.to_port().to_owned(),
                })
                .collect(),
            effect: public_effect(graph.effect()),
            matrix: graph.matrix(),
            sampling: "nearest".to_owned(),
            edge_mode: "transparent_black".to_owned(),
            output_width: graph.output_extent().0,
            output_height: graph.output_extent().1,
            derived_timeline_identity: graph.derived_timeline_identity().to_owned(),
            implementation: public_implementation(graph.implementation()),
        }),
        operation_log: snapshot
            .operation_log()
            .iter()
            .map(|operation| OperationRecord {
                operation_id: operation.operation_id().to_owned(),
                resulting_revision: operation.resulting_revision(),
                arguments: public_operation_arguments(operation.arguments()),
                prior_phase: public_phase(operation.prior_phase()),
                result_phase: public_phase(operation.result_phase()),
            })
            .collect(),
        undo_depth: snapshot.undo_depth(),
        redo_depth: snapshot.redo_depth(),
    }
}

fn public_operation_arguments(value: &EngineOperationArguments) -> OperationArguments {
    match value {
        EngineOperationArguments::Import {
            fixture_id,
            fixture_version,
            manifest_sha256,
            payload_sha256,
        } => OperationArguments::Import {
            fixture_id: fixture_id.clone(),
            fixture_version: *fixture_version,
            manifest_sha256: manifest_sha256.clone(),
            payload_sha256: payload_sha256.clone(),
        },
        EngineOperationArguments::Insert {
            timeline_start_frame,
        } => OperationArguments::Insert {
            timeline_start_frame: *timeline_start_frame,
        },
        EngineOperationArguments::Trim {
            source_start_frame,
            source_end_frame,
        } => OperationArguments::Trim {
            source_start_frame: *source_start_frame,
            source_end_frame: *source_end_frame,
        },
        EngineOperationArguments::Effect { effect } => OperationArguments::Effect {
            effect: public_effect(*effect),
        },
        _ => OperationArguments::Unknown,
    }
}

fn empty_state() -> ScenarioStateSnapshot {
    let dispatcher = EngineCommandDispatcher::scenario_only();
    let snapshot = dispatcher
        .scenario_snapshot()
        .expect("the scenario-only dispatcher has no execution-domain failure");
    public_snapshot(&snapshot)
}

fn public_phase(value: EnginePhase) -> ScenarioPhase {
    match value {
        EnginePhase::Empty => ScenarioPhase::Empty,
        EnginePhase::Imported => ScenarioPhase::Imported,
        EnginePhase::Placed => ScenarioPhase::Placed,
        EnginePhase::Trimmed => ScenarioPhase::Trimmed,
        EnginePhase::Effected => ScenarioPhase::Effected,
        _ => ScenarioPhase::Unknown,
    }
}

fn public_effect(value: EngineGraphEffect) -> SliceGraphEffect {
    match value {
        EngineGraphEffect::HorizontalMirror => SliceGraphEffect::HorizontalMirror,
        _ => SliceGraphEffect::Unknown,
    }
}

fn public_node_kind(value: EngineGraphNodeKind) -> GraphNodeKind {
    match value {
        EngineGraphNodeKind::Source => GraphNodeKind::Source,
        EngineGraphNodeKind::Effect => GraphNodeKind::Effect,
        EngineGraphNodeKind::Output => GraphNodeKind::Output,
        _ => GraphNodeKind::Unknown,
    }
}

fn public_implementation(value: EngineImplementation) -> SliceImplementation {
    match value {
        EngineImplementation::Reference => SliceImplementation::Reference,
        _ => SliceImplementation::Unknown,
    }
}
