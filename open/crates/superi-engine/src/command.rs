//! Engine-owned state for the canonical command-line slice contract.
//!
//! This module owns deterministic editorial state and reversal. It deliberately stops before
//! media decoding, graph evaluation, color delivery, and muxing, whose current contract stubs are
//! coordinated by the CLI runner and disclosed in its report.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ClipId, MediaId, NodeId, ProjectId, TrackId};
use superi_core::time::FrameRate;

const COMPONENT: &str = "superi-engine.command";
const MAX_HISTORY_ENTRIES: usize = 4;

/// Maximum ordered mutations accepted in one atomic scenario transaction.
pub const MAX_SCENARIO_TRANSACTION_ACTIONS: usize = 64;

/// Stable canonical fixture identifier.
pub const CANONICAL_FIXTURE_ID: &str = "slice/video-cfr";
/// Stable canonical fixture revision.
pub const CANONICAL_FIXTURE_VERSION: u32 = 1;
/// Canonical source and output width in pixels.
pub const CANONICAL_WIDTH: u32 = 96;
/// Canonical source and output height in pixels.
pub const CANONICAL_HEIGHT: u32 = 54;
/// Canonical source duration in frames.
pub const CANONICAL_SOURCE_FRAMES: u64 = 96;
/// Canonical trimmed duration in frames.
pub const CANONICAL_OUTPUT_FRAMES: u64 = 48;
/// Maximum bytes read from one canonical fixture payload.
pub const MAX_SCENARIO_SOURCE_BYTES: u64 = 64 * 1024 * 1024;

/// Current implementation ownership for deterministic editorial state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum SliceImplementation {
    /// A deterministic boundary implementation awaiting its production subsystem owner.
    Reference,
}

impl SliceImplementation {
    /// Returns the stable implementation code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Reference => "reference",
        }
    }
}

/// Typed graph effect required by the canonical slice.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum GraphEffect {
    /// Mirror the image horizontally with the canonical transform matrix.
    HorizontalMirror,
}

impl GraphEffect {
    /// Returns the stable effect code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::HorizontalMirror => "horizontal_mirror",
        }
    }
}

/// Highest completed mutation phase in the canonical slice.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ScenarioPhase {
    /// No source is imported.
    Empty,
    /// Canonical source identity and timing are known.
    Imported,
    /// The canonical clip is placed on the canonical track.
    Placed,
    /// The exact source trim is applied.
    Trimmed,
    /// The canonical mirror graph is attached.
    Effected,
}

impl ScenarioPhase {
    /// Returns the stable phase code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Imported => "imported",
            Self::Placed => "placed",
            Self::Trimmed => "trimmed",
            Self::Effected => "effected",
        }
    }
}

/// One precise mutation or history action accepted by the canonical engine boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ScenarioAction {
    /// Validate and identify the canonical fixture payload.
    ImportClip {
        /// Local payload path.
        path: PathBuf,
        /// Authoritative fixture identifier.
        fixture_id: String,
        /// Authoritative fixture revision.
        fixture_version: u32,
        /// Lowercase SHA-256 digest of the complete fixture manifest.
        manifest_sha256: String,
        /// Lowercase SHA-256 digest of the payload.
        payload_sha256: String,
        /// Exact source frame rate.
        frame_rate: FrameRate,
        /// Exact source frame count.
        frame_count: u64,
        /// Exact source width.
        width: u32,
        /// Exact source height.
        height: u32,
    },
    /// Place the imported source on the canonical timeline.
    PlaceClip {
        /// Exact timeline start frame.
        timeline_start_frame: i64,
    },
    /// Apply the canonical half-open source trim.
    TrimClip {
        /// Inclusive source frame.
        source_start_frame: u64,
        /// Exclusive source frame.
        source_end_frame: u64,
    },
    /// Attach the canonical graph effect.
    ApplyGraphEffect {
        /// Supported effect type.
        effect: GraphEffect,
    },
    /// Reverse the latest committed mutation.
    Undo,
    /// Reapply the latest reversed mutation.
    Redo,
}

impl ScenarioAction {
    /// Returns the stable action code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ImportClip { .. } => "import_clip",
            Self::PlaceClip { .. } => "place_clip",
            Self::TrimClip { .. } => "trim_clip",
            Self::ApplyGraphEffect { .. } => "apply_graph_effect",
            Self::Undo => "undo",
            Self::Redo => "redo",
        }
    }
}

/// Imported source identity and exact canonical metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportedMediaState {
    id: MediaId,
    path: PathBuf,
    fixture_id: String,
    fixture_version: u32,
    manifest_sha256: String,
    payload_sha256: String,
    frame_rate: FrameRate,
    frame_count: u64,
    width: u32,
    height: u32,
    implementation: SliceImplementation,
}

impl ImportedMediaState {
    /// Returns stable media identity within the scenario.
    #[must_use]
    pub const fn id(&self) -> MediaId {
        self.id
    }
    /// Returns the local fixture payload path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Returns the fixture identifier.
    #[must_use]
    pub fn fixture_id(&self) -> &str {
        &self.fixture_id
    }
    /// Returns the fixture revision.
    #[must_use]
    pub const fn fixture_version(&self) -> u32 {
        self.fixture_version
    }
    /// Returns the manifest digest.
    #[must_use]
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    /// Returns the payload digest.
    #[must_use]
    pub fn payload_sha256(&self) -> &str {
        &self.payload_sha256
    }
    /// Returns the payload digest as the stable source fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.payload_sha256
    }
    /// Returns the exact source frame rate.
    #[must_use]
    pub const fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }
    /// Returns the exact source frame count.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }
    /// Returns the source width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }
    /// Returns the source height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }
    /// Returns the current implementation owner.
    #[must_use]
    pub const fn implementation(&self) -> SliceImplementation {
        self.implementation
    }
}

/// Canonical single-track placement and source trim state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineState {
    track_id: TrackId,
    clip_id: ClipId,
    timeline_start_frame: i64,
    source_start_frame: u64,
    source_end_frame: u64,
    implementation: SliceImplementation,
}

impl TimelineState {
    /// Returns the typed track identity.
    #[must_use]
    pub const fn track_id(&self) -> TrackId {
        self.track_id
    }
    /// Returns the typed clip identity.
    #[must_use]
    pub const fn clip_id(&self) -> ClipId {
        self.clip_id
    }
    /// Returns the canonical timeline name.
    #[must_use]
    pub const fn timeline_name(&self) -> &'static str {
        "canonical"
    }
    /// Returns the canonical track name.
    #[must_use]
    pub const fn track_name(&self) -> &'static str {
        "V1"
    }
    /// Returns the canonical clip name.
    #[must_use]
    pub const fn clip_name(&self) -> &'static str {
        "clip-1"
    }
    /// Returns the canonical edit rate.
    #[must_use]
    pub const fn edit_rate(&self) -> FrameRate {
        FrameRate::FPS_24
    }
    /// Returns the canonical canvas dimensions.
    #[must_use]
    pub const fn canvas(&self) -> (u32, u32) {
        (CANONICAL_WIDTH, CANONICAL_HEIGHT)
    }
    /// Returns the timeline start frame.
    #[must_use]
    pub const fn timeline_start_frame(&self) -> i64 {
        self.timeline_start_frame
    }
    /// Returns the inclusive source trim frame.
    #[must_use]
    pub const fn source_start_frame(&self) -> u64 {
        self.source_start_frame
    }
    /// Returns the exclusive source trim frame.
    #[must_use]
    pub const fn source_end_frame(&self) -> u64 {
        self.source_end_frame
    }
    /// Returns the half-open source range.
    #[must_use]
    pub const fn source_range(&self) -> (u64, u64) {
        (self.source_start_frame, self.source_end_frame)
    }
    /// Returns the half-open timeline range.
    #[must_use]
    pub const fn timeline_range(&self) -> (i64, i64) {
        (
            self.timeline_start_frame,
            self.timeline_start_frame + (self.source_end_frame - self.source_start_frame) as i64,
        )
    }
    /// Returns the current implementation owner.
    #[must_use]
    pub const fn implementation(&self) -> SliceImplementation {
        self.implementation
    }
}

/// Stable graph node role.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum GraphNodeKind {
    Source,
    Effect,
    Output,
}

/// One canonical graph node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphNodeState {
    id: NodeId,
    instance_id: &'static str,
    kind: GraphNodeKind,
    node_type: &'static str,
    schema_version: u32,
}

impl GraphNodeState {
    /// Returns the typed node identity.
    #[must_use]
    pub const fn id(&self) -> NodeId {
        self.id
    }
    /// Returns the stable instance identifier.
    #[must_use]
    pub const fn instance_id(&self) -> &'static str {
        self.instance_id
    }
    /// Returns the node role.
    #[must_use]
    pub const fn kind(&self) -> GraphNodeKind {
        self.kind
    }
    /// Returns the stable node type.
    #[must_use]
    pub const fn node_type(&self) -> &'static str {
        self.node_type
    }
    /// Returns the node schema revision.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
    /// Returns the ordered input port names.
    #[must_use]
    pub const fn input_ports(&self) -> &'static [&'static str] {
        match self.kind {
            GraphNodeKind::Source => &[],
            GraphNodeKind::Effect | GraphNodeKind::Output => &["image"],
        }
    }
    /// Returns the ordered output port names.
    #[must_use]
    pub const fn output_ports(&self) -> &'static [&'static str] {
        match self.kind {
            GraphNodeKind::Source | GraphNodeKind::Effect => &["image"],
            GraphNodeKind::Output => &[],
        }
    }
}

/// One canonical image edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphEdgeState {
    from_node: &'static str,
    to_node: &'static str,
}

impl GraphEdgeState {
    /// Returns the source node instance.
    #[must_use]
    pub const fn from_node(&self) -> &'static str {
        self.from_node
    }
    /// Returns the destination node instance.
    #[must_use]
    pub const fn to_node(&self) -> &'static str {
        self.to_node
    }
    /// Returns the source image port.
    #[must_use]
    pub const fn from_port(&self) -> &'static str {
        "image"
    }
    /// Returns the destination image port.
    #[must_use]
    pub const fn to_port(&self) -> &'static str {
        "image"
    }
}

/// Canonical sampling policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum GraphSampling {
    Nearest,
}

/// Canonical edge behavior.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum GraphEdgeMode {
    TransparentBlack,
}

/// Complete public graph state for the canonical mirror effect.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphState {
    nodes: Vec<GraphNodeState>,
    edges: Vec<GraphEdgeState>,
    effect: GraphEffect,
    matrix: [f64; 9],
    implementation: SliceImplementation,
}

impl GraphState {
    /// Returns the ordered node table.
    #[must_use]
    pub fn nodes(&self) -> &[GraphNodeState] {
        &self.nodes
    }
    /// Returns the ordered edge table.
    #[must_use]
    pub fn edges(&self) -> &[GraphEdgeState] {
        &self.edges
    }
    /// Returns the typed effect.
    #[must_use]
    pub const fn effect(&self) -> GraphEffect {
        self.effect
    }
    /// Returns the row-major binary64 transform matrix.
    #[must_use]
    pub const fn matrix(&self) -> [f64; 9] {
        self.matrix
    }
    /// Returns the sampling policy.
    #[must_use]
    pub const fn sampling(&self) -> GraphSampling {
        GraphSampling::Nearest
    }
    /// Returns the out-of-bounds edge behavior.
    #[must_use]
    pub const fn edge_mode(&self) -> GraphEdgeMode {
        GraphEdgeMode::TransparentBlack
    }
    /// Returns the graph output extent.
    #[must_use]
    pub const fn output_extent(&self) -> (u32, u32) {
        (CANONICAL_WIDTH, CANONICAL_HEIGHT)
    }
    /// Returns the timeline identity from which this graph is derived.
    #[must_use]
    pub const fn derived_timeline_identity(&self) -> &'static str {
        "canonical/V1/clip-1"
    }
    /// Returns the current implementation owner.
    #[must_use]
    pub const fn implementation(&self) -> SliceImplementation {
        self.implementation
    }
}

/// Typed arguments retained for reversible operation evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
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
    /// Canonical graph arguments.
    Effect { effect: GraphEffect },
}

/// One active canonical mutation record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationRecord {
    operation_id: &'static str,
    resulting_revision: u64,
    arguments: OperationArguments,
    prior_phase: ScenarioPhase,
    result_phase: ScenarioPhase,
}

impl OperationRecord {
    /// Returns the canonical operation identifier.
    #[must_use]
    pub const fn operation_id(&self) -> &'static str {
        self.operation_id
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

#[derive(Clone, Debug, PartialEq)]
struct ScenarioContent {
    project_id: ProjectId,
    phase: ScenarioPhase,
    media: Option<ImportedMediaState>,
    timeline: Option<TimelineState>,
    graph: Option<GraphState>,
    operation_log: Vec<OperationRecord>,
}

impl Default for ScenarioContent {
    fn default() -> Self {
        Self {
            project_id: ProjectId::from_raw(1),
            phase: ScenarioPhase::Empty,
            media: None,
            timeline: None,
            graph: None,
            operation_log: Vec::new(),
        }
    }
}

/// Complete immutable engine state exposed after every action.
#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioSnapshot {
    revision: u64,
    content: ScenarioContent,
    undo_depth: usize,
    redo_depth: usize,
}

impl ScenarioSnapshot {
    /// Returns the monotonic successful-action revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Returns stable project identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.content.project_id
    }
    /// Returns the highest completed mutation phase.
    #[must_use]
    pub const fn phase(&self) -> ScenarioPhase {
        self.content.phase
    }
    /// Returns imported source state when present.
    #[must_use]
    pub const fn media(&self) -> Option<&ImportedMediaState> {
        self.content.media.as_ref()
    }
    /// Returns timeline state when present.
    #[must_use]
    pub const fn timeline(&self) -> Option<&TimelineState> {
        self.content.timeline.as_ref()
    }
    /// Returns graph state when present.
    #[must_use]
    pub const fn graph(&self) -> Option<&GraphState> {
        self.content.graph.as_ref()
    }
    /// Returns the active mutation log.
    #[must_use]
    pub fn operation_log(&self) -> &[OperationRecord] {
        &self.content.operation_log
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
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    before: ScenarioContent,
    after: ScenarioContent,
}

/// Authoritative transaction owner for canonical editorial state.
#[derive(Debug, Default)]
pub struct ScenarioEngine {
    revision: u64,
    content: ScenarioContent,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl ScenarioEngine {
    /// Creates an empty canonical scenario.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a complete point-in-time snapshot.
    #[must_use]
    pub fn snapshot(&self) -> ScenarioSnapshot {
        ScenarioSnapshot {
            revision: self.revision,
            content: self.content.clone(),
            undo_depth: self.undo.len(),
            redo_depth: self.redo.len(),
        }
    }

    /// Applies, reverses, or reapplies one atomic action.
    ///
    /// # Errors
    ///
    /// Returns a classified error without changing state or history when validation, bounded I/O,
    /// or revision requirements fail.
    pub fn execute(&mut self, action: ScenarioAction) -> Result<ScenarioSnapshot> {
        match action {
            ScenarioAction::Undo => self.undo(),
            ScenarioAction::Redo => self.redo(),
            action => self.commit_transaction(vec![action]),
        }
    }

    /// Applies one bounded ordered transaction as a single revision and undo unit.
    ///
    /// Every mutation is staged against private content. The public state, revision, history, and
    /// redo stack change only after every action succeeds. Undo and redo are accepted only as a
    /// one-action transaction because mixing history navigation with new mutations would make the
    /// transaction boundary ambiguous.
    ///
    /// # Errors
    ///
    /// Returns a classified error without changing state or history when the transaction is empty,
    /// exceeds the action bound, mixes history navigation with mutations, or any action fails.
    pub fn execute_transaction(
        &mut self,
        actions: Vec<ScenarioAction>,
    ) -> Result<ScenarioSnapshot> {
        if actions.is_empty() {
            return Err(invalid(
                "execute_transaction",
                "scenario transaction must contain at least one action",
            ));
        }
        if actions.len() > MAX_SCENARIO_TRANSACTION_ACTIONS {
            return Err(resource_exhausted(
                "execute_transaction",
                "scenario transaction exceeds the action bound",
            ));
        }
        if actions.len() == 1 {
            let action = actions
                .into_iter()
                .next()
                .expect("one-action transaction contains one action");
            return self.execute(action);
        }
        if actions
            .iter()
            .any(|action| matches!(action, ScenarioAction::Undo | ScenarioAction::Redo))
        {
            return Err(invalid(
                "execute_transaction",
                "history actions must be dispatched in a one-action transaction",
            ));
        }
        self.commit_transaction(actions)
    }

    fn commit_transaction(&mut self, actions: Vec<ScenarioAction>) -> Result<ScenarioSnapshot> {
        if self.undo.len() >= MAX_HISTORY_ENTRIES {
            return Err(resource_exhausted(
                "commit_transaction",
                "canonical history reached its four-transaction capacity",
            ));
        }
        let next_revision = next_revision(self.revision)?;
        let before = self.content.clone();
        let mut after = before.clone();
        for action in actions {
            after = apply(&after, action, next_revision)?;
        }
        self.content = after.clone();
        self.revision = next_revision;
        self.undo.push(HistoryEntry { before, after });
        self.redo.clear();
        Ok(self.snapshot())
    }

    fn undo(&mut self) -> Result<ScenarioSnapshot> {
        let next_revision = next_revision(self.revision)?;
        let entry = self
            .undo
            .pop()
            .ok_or_else(|| conflict("undo", "no committed scenario action is available to undo"))?;
        self.content = entry.before.clone();
        self.revision = next_revision;
        self.redo.push(entry);
        Ok(self.snapshot())
    }

    fn redo(&mut self) -> Result<ScenarioSnapshot> {
        let next_revision = next_revision(self.revision)?;
        let entry = self
            .redo
            .pop()
            .ok_or_else(|| conflict("redo", "no reversed scenario action is available to redo"))?;
        self.content = entry.after.clone();
        self.revision = next_revision;
        self.undo.push(entry);
        Ok(self.snapshot())
    }
}

fn apply(
    current: &ScenarioContent,
    action: ScenarioAction,
    revision: u64,
) -> Result<ScenarioContent> {
    let prior_phase = current.phase;
    let mut next = current.clone();
    let (operation_id, arguments) = match action {
        ScenarioAction::ImportClip {
            path,
            fixture_id,
            fixture_version,
            manifest_sha256,
            payload_sha256,
            frame_rate,
            frame_count,
            width,
            height,
        } => {
            require_phase(current, ScenarioPhase::Empty, "import_clip")?;
            require_canonical_import(&CanonicalImportContract {
                fixture_id: &fixture_id,
                fixture_version,
                manifest_sha256: &manifest_sha256,
                payload_sha256: &payload_sha256,
                frame_rate,
                frame_count,
                width,
                height,
            })?;
            let bytes = read_bounded_source(&path)?;
            if bytes.is_empty() {
                return Err(corrupt(
                    "import_clip",
                    "canonical fixture payload must not be empty",
                ));
            }
            if sha256_hex(&bytes) != payload_sha256 {
                return Err(corrupt(
                    "import_clip",
                    "canonical fixture payload digest does not match the manifest identity",
                ));
            }
            next.media = Some(ImportedMediaState {
                id: MediaId::from_raw(1),
                path,
                fixture_id: fixture_id.clone(),
                fixture_version,
                manifest_sha256: manifest_sha256.clone(),
                payload_sha256: payload_sha256.clone(),
                frame_rate,
                frame_count,
                width,
                height,
                implementation: SliceImplementation::Reference,
            });
            next.phase = ScenarioPhase::Imported;
            (
                "slice.op.import",
                OperationArguments::Import {
                    fixture_id,
                    fixture_version,
                    manifest_sha256,
                    payload_sha256,
                },
            )
        }
        ScenarioAction::PlaceClip {
            timeline_start_frame,
        } => {
            require_phase(current, ScenarioPhase::Imported, "place_clip")?;
            if timeline_start_frame != 0 {
                return Err(invalid(
                    "place_clip",
                    "canonical clip must start at timeline frame zero",
                ));
            }
            next.timeline = Some(TimelineState {
                track_id: TrackId::from_raw(1),
                clip_id: ClipId::from_raw(1),
                timeline_start_frame,
                source_start_frame: 0,
                source_end_frame: CANONICAL_SOURCE_FRAMES,
                implementation: SliceImplementation::Reference,
            });
            next.phase = ScenarioPhase::Placed;
            (
                "slice.op.insert",
                OperationArguments::Insert {
                    timeline_start_frame,
                },
            )
        }
        ScenarioAction::TrimClip {
            source_start_frame,
            source_end_frame,
        } => {
            require_phase(current, ScenarioPhase::Placed, "trim_clip")?;
            if (source_start_frame, source_end_frame) != (24, 72) {
                return Err(invalid(
                    "trim_clip",
                    "canonical source trim must be the half-open frame range 24 through 72",
                ));
            }
            let timeline = next
                .timeline
                .as_mut()
                .expect("placed phase retains timeline state");
            timeline.source_start_frame = source_start_frame;
            timeline.source_end_frame = source_end_frame;
            next.phase = ScenarioPhase::Trimmed;
            (
                "slice.op.trim",
                OperationArguments::Trim {
                    source_start_frame,
                    source_end_frame,
                },
            )
        }
        ScenarioAction::ApplyGraphEffect { effect } => {
            require_phase(current, ScenarioPhase::Trimmed, "apply_graph_effect")?;
            next.graph = Some(canonical_graph(effect));
            next.phase = ScenarioPhase::Effected;
            ("slice.op.effect", OperationArguments::Effect { effect })
        }
        ScenarioAction::Undo | ScenarioAction::Redo => {
            unreachable!("history actions are routed before state application")
        }
    };
    next.operation_log.push(OperationRecord {
        operation_id,
        resulting_revision: revision,
        arguments,
        prior_phase,
        result_phase: next.phase,
    });
    Ok(next)
}

struct CanonicalImportContract<'a> {
    fixture_id: &'a str,
    fixture_version: u32,
    manifest_sha256: &'a str,
    payload_sha256: &'a str,
    frame_rate: FrameRate,
    frame_count: u64,
    width: u32,
    height: u32,
}

fn require_canonical_import(contract: &CanonicalImportContract<'_>) -> Result<()> {
    if contract.fixture_id != CANONICAL_FIXTURE_ID
        || contract.fixture_version != CANONICAL_FIXTURE_VERSION
        || contract.frame_rate != FrameRate::FPS_24
        || contract.frame_count != CANONICAL_SOURCE_FRAMES
        || contract.width != CANONICAL_WIDTH
        || contract.height != CANONICAL_HEIGHT
    {
        return Err(invalid(
            "import_clip",
            "import arguments do not match the canonical fixture contract",
        ));
    }
    if !is_lower_sha256(contract.manifest_sha256) || !is_lower_sha256(contract.payload_sha256) {
        return Err(invalid(
            "import_clip",
            "canonical manifest and payload identities must be lowercase SHA-256 digests",
        ));
    }
    Ok(())
}

fn canonical_graph(effect: GraphEffect) -> GraphState {
    GraphState {
        nodes: vec![
            GraphNodeState {
                id: NodeId::from_raw(1),
                instance_id: "slice.node.source",
                kind: GraphNodeKind::Source,
                node_type: "superi.source.fixture",
                schema_version: 1,
            },
            GraphNodeState {
                id: NodeId::from_raw(2),
                instance_id: "slice.node.effect",
                kind: GraphNodeKind::Effect,
                node_type: "superi.effect.transform",
                schema_version: 1,
            },
            GraphNodeState {
                id: NodeId::from_raw(3),
                instance_id: "slice.node.output",
                kind: GraphNodeKind::Output,
                node_type: "superi.output.image",
                schema_version: 1,
            },
        ],
        edges: vec![
            GraphEdgeState {
                from_node: "slice.node.source",
                to_node: "slice.node.effect",
            },
            GraphEdgeState {
                from_node: "slice.node.effect",
                to_node: "slice.node.output",
            },
        ],
        effect,
        matrix: [-1.0, 0.0, 95.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        implementation: SliceImplementation::Reference,
    }
}

fn require_phase(
    current: &ScenarioContent,
    required: ScenarioPhase,
    operation: &'static str,
) -> Result<()> {
    if current.phase != required {
        return Err(conflict(
            operation,
            "scenario action is not valid in the current phase",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "inspect_phase")
                .with_field("current", current.phase.code())
                .with_field("required", required.code()),
        ));
    }
    Ok(())
}

fn read_bounded_source(path: &Path) -> Result<Vec<u8>> {
    let metadata =
        fs::metadata(path).map_err(|source| source_io_error(path, "inspect_source", source))?;
    if !metadata.is_file() {
        return Err(
            invalid("import_clip", "import source must be a regular file")
                .with_context(path_context("inspect_source", path)),
        );
    }
    if metadata.len() > MAX_SCENARIO_SOURCE_BYTES {
        return Err(resource_exhausted(
            "import_clip",
            "import source exceeds the canonical scenario byte bound",
        )
        .with_context(path_context("inspect_source", path)));
    }
    let file =
        fs::File::open(path).map_err(|source| source_io_error(path, "open_source", source))?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(MAX_SCENARIO_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| source_io_error(path, "read_source", source))?;
    if bytes.len() as u64 > MAX_SCENARIO_SOURCE_BYTES {
        return Err(resource_exhausted(
            "import_clip",
            "import source grew beyond the canonical scenario byte bound while reading",
        )
        .with_context(path_context("read_source", path)));
    }
    Ok(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn next_revision(current: u64) -> Result<u64> {
    current.checked_add(1).ok_or_else(|| {
        Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "scenario revision is exhausted",
        )
        .with_context(ErrorContext::new(COMPONENT, "advance_revision"))
    })
}

fn source_io_error(path: &Path, operation: &'static str, source: std::io::Error) -> Error {
    let (category, message) = if source.kind() == std::io::ErrorKind::NotFound {
        (ErrorCategory::NotFound, "import source does not exist")
    } else {
        (
            ErrorCategory::Unavailable,
            "import source could not be read",
        )
    };
    Error::with_source(category, Recoverability::UserCorrectable, message, source)
        .with_context(path_context(operation, path))
}

fn path_context(operation: &'static str, path: &Path) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string())
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
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
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

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
