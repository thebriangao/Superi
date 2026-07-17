//! Stable transport-neutral complete editor-state replacement snapshot.
//!
//! The engine captures every owner once. This adapter preserves canonical domain documents and
//! exact runtime observations while keeping packets, frames, samples, textures, command buffers,
//! opaque extension payloads, and typed export artifacts outside the public response.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::SemanticVersion;
use superi_core::time::{RationalTime, TimeRange, Timebase};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineReportedFailure, EngineTransactionId,
};
use superi_engine::editor_state::{
    EditorAudioChannelTarget as EngineAudioChannelTarget,
    EditorAudioContinuity as EngineAudioContinuity,
    EditorAudioRecordContinuity as EngineAudioRecordContinuity,
    EditorAudioRouteDestination as EngineAudioRouteDestination,
    EditorAudioSourceContinuity as EngineAudioSourceContinuity,
    EditorAudioTrackState as EngineAudioTrackState,
    EditorCanonicalDocument as EngineCanonicalDocument,
    EditorExtensionRecord as EngineExtensionRecord, EditorGraphDocument as EngineGraphDocument,
    EditorGraphScope as EngineGraphScope, EditorStateAvailability as EngineAvailability,
    EditorStateSnapshot as EngineState,
};
use superi_engine::export_dispatch::{EngineExportJobSnapshot, EngineExportJobState};
use superi_engine::project_settings::{
    ColorSettings as EngineColorSettings, ProjectSettingsState as EngineProjectSettingsState,
    RenderColorTarget as EngineRenderColorTarget,
};
use superi_engine::transport::{
    PlaybackDirection as EnginePlaybackDirection, PlaybackTransportMode as EnginePlaybackMode,
    PlaybackTransportSnapshot as EnginePlaybackSnapshot,
    TransportAudioState as EngineTransportAudioState,
    TransportDegradationCode as EngineTransportDegradationCode,
};

use crate::audio_automation::{self, AudioAutomationSnapshot};
use crate::commands::{GetEditorState, GetEditorStateResult};
use crate::project::{self, ProjectSettingsSnapshot};
use crate::recovery::{self, ProjectRecoverySnapshot};
use crate::version::EDITOR_STATE_SCHEMA_VERSION;

const COMPONENT: &str = "superi-api.editor-state";
const TIMELINE_RESOURCE: &str = "superi.editor.state.timeline";
const CLIP_MIX_RESOURCE: &str = "superi.editor.state.audio.clip-mix";
const SETTINGS_RESOURCE: &str = "superi.project.settings";
const EFFECT_KIND: &str = "superi.extension.effect";
const AI_ARTIFACT_KIND: &str = "superi.extension.ai-artifact";

#[derive(Clone, Debug)]
struct CanonicalJson(Box<RawValue>);

impl PartialEq for CanonicalJson {
    fn eq(&self, other: &Self) -> bool {
        self.0.get() == other.0.get()
    }
}

impl Eq for CanonicalJson {}

impl Serialize for CanonicalJson {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CanonicalJson {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Box::<RawValue>::deserialize(deserializer).map(Self)
    }
}

/// One exact rational clock.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorTimebase {
    numerator: u32,
    denominator: u32,
}

impl EditorTimebase {
    fn from_engine(value: Timebase) -> Self {
        Self {
            numerator: value.numerator(),
            denominator: value.denominator(),
        }
    }
}

/// One exact signed coordinate and its explicit clock.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorRationalTime {
    value: i64,
    timebase: EditorTimebase,
}

impl EditorRationalTime {
    fn from_engine(value: RationalTime) -> Self {
        Self {
            value: value.value(),
            timebase: EditorTimebase::from_engine(value.timebase()),
        }
    }
}

/// One exact half-open range.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorTimeRange {
    start: EditorRationalTime,
    duration: u64,
}

impl EditorTimeRange {
    fn from_engine(value: TimeRange) -> Self {
        Self {
            start: EditorRationalTime::from_engine(value.start()),
            duration: value.duration().value(),
        }
    }
}

/// One integrity-identified canonical JSON document owned by a domain codec.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorCanonicalDocument {
    resource: String,
    format: String,
    format_revision: u32,
    byte_length: u64,
    sha256: String,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::CanonicalJsonBinding)
    )]
    content: CanonicalJson,
}

impl EditorCanonicalDocument {
    fn from_engine(resource: impl Into<String>, value: &EngineCanonicalDocument) -> Result<Self> {
        let content = String::from_utf8(value.bytes().to_vec()).map_err(|source| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "canonical editor state document is not valid UTF-8 JSON",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "convert_canonical_document")
                    .with_field("format", value.format())
                    .with_field("source", source.to_string()),
            )
        })?;
        let content = RawValue::from_string(content)
            .map(CanonicalJson)
            .map_err(|source| {
                Error::new(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "canonical editor state document is not valid JSON",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "convert_canonical_document")
                        .with_field("format", value.format())
                        .with_field("source", source.to_string()),
                )
            })?;
        Ok(Self {
            resource: resource.into(),
            format: value.format().to_owned(),
            format_revision: value.format_revision(),
            byte_length: value.byte_length(),
            sha256: value.sha256().to_owned(),
            content,
        })
    }

    /// Returns the permanent resource identity.
    #[must_use]
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// Returns the source domain document format.
    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
    }

    /// Returns the incompatible source domain document revision.
    #[must_use]
    pub const fn format_revision(&self) -> u32 {
        self.format_revision
    }

    /// Returns the exact canonical JSON byte count.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }

    /// Returns the lowercase SHA-256 identity of the exact canonical JSON.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Returns the exact canonical JSON that is embedded as structured response content.
    #[must_use]
    pub fn content_json(&self) -> &str {
        self.content.0.get()
    }
}

/// Timeline or standalone ownership for one graph resource.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum EditorGraphScope {
    Timeline { root_timeline_id: String },
    Standalone { name: String },
}

/// One complete versioned graph state resource.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorGraphDocument {
    graph_id: String,
    graph_revision: u64,
    scope: EditorGraphScope,
    document: EditorCanonicalDocument,
}

impl EditorGraphDocument {
    fn from_engine(value: &EngineGraphDocument) -> Result<Self> {
        let scope = match value.scope() {
            EngineGraphScope::Timeline { root_timeline_id } => EditorGraphScope::Timeline {
                root_timeline_id: root_timeline_id.clone(),
            },
            EngineGraphScope::Standalone { name } => {
                EditorGraphScope::Standalone { name: name.clone() }
            }
            _ => return Err(unsupported_engine_value("convert_graph_scope")),
        };
        Ok(Self {
            graph_id: value.graph_id().to_owned(),
            graph_revision: value.graph_revision(),
            scope,
            document: EditorCanonicalDocument::from_engine(
                format!("superi.editor.state.graph.{}", value.graph_id()),
                value.document(),
            )?,
        })
    }
}

/// Safe failure classification without raw internal messages or contexts.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorFailureState {
    category: String,
    recoverability: String,
}

impl EditorFailureState {
    fn from_engine(value: &EngineReportedFailure) -> Self {
        Self {
            category: value.category().code().to_owned(),
            recoverability: value.recoverability().code().to_owned(),
        }
    }
}

/// Bounded descriptor for one extension record whose opaque payload remains project-owned.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[cfg_attr(
    feature = "typescript-bindings",
    specta(rename = "EditorStateExtensionRecord")
)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorExtensionRecord {
    extension_id: String,
    record_id: String,
    extension_version: String,
    kind: String,
    payload_schema: String,
    requested_capabilities: Vec<String>,
    granted_capabilities: Vec<String>,
    lifecycle: String,
    failure_category: Option<String>,
    failure_recoverability: Option<String>,
    total_failures: Option<u64>,
    consecutive_failures: Option<u32>,
    payload_byte_length: u64,
    payload_sha256: String,
}

impl EditorExtensionRecord {
    fn from_engine(value: &EngineExtensionRecord) -> Self {
        let failure = value.failure();
        Self {
            extension_id: value.extension_id().to_owned(),
            record_id: value.record_id().to_owned(),
            extension_version: value.extension_version().to_owned(),
            kind: value.kind().to_owned(),
            payload_schema: value.payload_schema().to_owned(),
            requested_capabilities: value.requested_capabilities().to_vec(),
            granted_capabilities: value.granted_capabilities().to_vec(),
            lifecycle: value.lifecycle().to_owned(),
            failure_category: failure.map(|failure| failure.category().to_owned()),
            failure_recoverability: failure.map(|failure| failure.recoverability().to_owned()),
            total_failures: failure.map(|failure| failure.total_failures()),
            consecutive_failures: failure.map(|failure| failure.consecutive_failures()),
            payload_byte_length: value.payload_byte_length(),
            payload_sha256: value.payload_sha256().to_owned(),
        }
    }
}

/// Explicit optional owner state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "state", rename_all = "snake_case")]
#[non_exhaustive]
pub enum EditorAvailability<T> {
    Detached,
    Attached(T),
}

/// Project identity, history, durability evidence, settings, recovery, and extensions.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorProjectState {
    project_id: String,
    root_timeline_id: String,
    project_revision: u64,
    history_capacity: u64,
    undo_depth: u64,
    redo_depth: u64,
    next_undo: Option<String>,
    next_redo: Option<String>,
    semantic_hash_algorithm: String,
    semantic_hash_format_revision: u32,
    semantic_hash: String,
    settings: ProjectSettingsSnapshot,
    recovery: EditorAvailability<ProjectRecoverySnapshot>,
    extensions: Vec<EditorExtensionRecord>,
    timeline_resource: String,
    graph_resources: Vec<String>,
    clip_mix_resource: String,
}

/// Complete canonical timeline and editorial state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorTimelineState {
    timeline_count: u64,
    document: EditorCanonicalDocument,
}

impl EditorTimelineState {
    /// Returns the number of editable timelines in the project.
    #[must_use]
    pub const fn timeline_count(&self) -> u64 {
        self.timeline_count
    }

    /// Returns the complete canonical timeline document.
    #[must_use]
    pub const fn document(&self) -> &EditorCanonicalDocument {
        &self.document
    }
}

/// Every complete editable graph resource.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorGraphState {
    documents: Vec<EditorGraphDocument>,
}

/// Media state is retained in the canonical timeline resource without copying media payloads.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorMediaState {
    media_count: u64,
    timeline_resource: String,
    timeline_sha256: String,
}

/// Record coverage at one adjacent audio seam.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum EditorAudioRecordContinuity {
    Seamless,
    Gap { sample_count: u64 },
    Overlap { sample_count: u64 },
}

/// Source relationship at one adjacent audio seam.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum EditorAudioSourceContinuity {
    Continuous,
    Discontinuous { expected: i64, actual: i64 },
    DifferentClip { left: String, right: String },
}

/// Typed destination for one public audio track route.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum EditorAudioRouteDestination {
    Main,
    Track { track_id: String },
}

/// Destination or explicit suppression for one source channel.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[cfg_attr(
    feature = "typescript-bindings",
    specta(rename = "EditorStateAudioChannelTarget")
)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum EditorAudioChannelTarget {
    Channel { channel: String },
    Muted,
}

/// One ordered source-channel routing decision.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[cfg_attr(
    feature = "typescript-bindings",
    specta(rename = "EditorStateAudioChannelRoute")
)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorAudioChannelRoute {
    source: String,
    target: EditorAudioChannelTarget,
}

/// One exact sample-clock seam between adjacent authored clips.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorAudioSeam {
    left_clip_id: String,
    right_clip_id: String,
    record: EditorAudioRecordContinuity,
    source: EditorAudioSourceContinuity,
}

/// Audited sample timing, or an explicit exactness limitation for retimed material.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum EditorAudioContinuity {
    Audited {
        uninterrupted_record_coverage: bool,
        seams: Vec<EditorAudioSeam>,
    },
    Unsupported {
        reason: String,
    },
}

/// One audio track's exact sample clock and structural continuity evidence.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorAudioTrackState {
    timeline_id: String,
    track_id: String,
    sample_rate: u32,
    source_channels: Vec<String>,
    destination: EditorAudioRouteDestination,
    destination_channels: Vec<String>,
    routes: Vec<EditorAudioChannelRoute>,
    clip_count: u64,
    continuity: EditorAudioContinuity,
}

/// Exact authored audio state and optional sample-clock automation.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorAudioState {
    audio_track_count: u64,
    tracks: Vec<EditorAudioTrackState>,
    timeline_resource: String,
    clip_mix: EditorCanonicalDocument,
    automation: EditorAvailability<AudioAutomationSnapshot>,
}

/// Durable project color state plus references to graph-authored color operations.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum EditorColorManagement {
    BuiltInAcesCg {
        working_space: String,
    },
    PinnedConfig {
        config_id: String,
        config_fingerprint: String,
        working_space: String,
    },
}

/// Render output interpretation selected by the authoritative project settings.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EditorRenderColorTarget {
    ProjectWorking,
    Display,
    Delivery,
}

/// Durable project color state plus references to graph-authored color operations.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorColorState {
    settings_resource: String,
    project_revision: u64,
    management: EditorColorManagement,
    render_target: EditorRenderColorTarget,
    render_settings_fingerprint: String,
    graph_resources: Vec<String>,
}

/// Effect state references canonical graphs and auxiliary effect extension records.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorEffectState {
    graph_resources: Vec<String>,
    extension_records: Vec<EditorExtensionRecord>,
}

/// AI artifacts remain ordinary editable resources while the runtime is honestly unavailable.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorAiState {
    runtime_availability: String,
    graph_resources: Vec<String>,
    artifact_records: Vec<EditorExtensionRecord>,
}

/// Exact playback scheduling and audio synchronization state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorPlaybackSnapshot {
    mode: String,
    playhead: EditorRationalTime,
    scheduled_frame: Option<EditorRationalTime>,
    scheduled_due_clock: Option<EditorRationalTime>,
    rate_numerator: i64,
    rate_denominator: u64,
    direction: String,
    loop_range: Option<EditorTimeRange>,
    epoch: u64,
    total_dropped: u64,
    consecutive_dropped: u32,
    forced_presentations: u64,
    audio_state: String,
    discard_requested_generation: u64,
    discard_applied_generation: u64,
    degradation: Vec<String>,
    failure: Option<EditorFailureState>,
}

/// Playback bridge attachment with pending and latest observation status.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum EditorPlaybackState {
    Detached,
    Attached {
        pending_command: bool,
        latest: Option<Box<EditorPlaybackSnapshot>>,
    },
}

/// Exact elapsed wall duration for one observed export attempt.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorElapsedDuration {
    seconds: u64,
    nanoseconds: u32,
}

/// Complete public-safe state for one retained logical export job.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorExportJobSnapshot {
    job_id: String,
    status: String,
    attempt: u32,
    progress_revision: u64,
    completed_units: u64,
    total_units: Option<u64>,
    elapsed: Option<EditorElapsedDuration>,
    dependencies: Vec<String>,
    failure: Option<EditorFailureState>,
    dependency_failures: Vec<EditorExportDependencyFailure>,
    has_result: bool,
    retry_allowed: bool,
    is_final: bool,
}

/// One prerequisite that finalized without success.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorExportDependencyFailure {
    job_id: String,
    state: String,
}

/// Latest explicitly observed export queue replacement state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorExportQueueState {
    revision: u64,
    jobs: Vec<EditorExportJobSnapshot>,
}

/// Export queue attachment and latest explicit observation.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum EditorExportState {
    Detached,
    Attached {
        latest: Option<EditorExportQueueState>,
    },
}

/// Complete replacement state for every editor-owned authored and runtime domain.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorStateSnapshot {
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    schema_version: SemanticVersion,
    project: EditorProjectState,
    timeline: EditorTimelineState,
    graph: EditorGraphState,
    media: EditorMediaState,
    audio: EditorAudioState,
    color: EditorColorState,
    effect: EditorEffectState,
    ai: EditorAiState,
    playback: EditorPlaybackState,
    export: EditorExportState,
}

impl EditorStateSnapshot {
    /// Returns the complete resource schema version.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns project identity, history, settings, recovery, and durability evidence.
    #[must_use]
    pub const fn project(&self) -> &EditorProjectState {
        &self.project
    }

    /// Returns the canonical timeline resource.
    #[must_use]
    pub const fn timeline(&self) -> &EditorTimelineState {
        &self.timeline
    }

    /// Returns every canonical graph resource.
    #[must_use]
    pub const fn graph(&self) -> &EditorGraphState {
        &self.graph
    }

    /// Returns explicit media resource state.
    #[must_use]
    pub const fn media(&self) -> &EditorMediaState {
        &self.media
    }

    /// Returns exact audio authoring and automation state.
    #[must_use]
    pub const fn audio(&self) -> &EditorAudioState {
        &self.audio
    }

    /// Returns project color and graph references.
    #[must_use]
    pub const fn color(&self) -> &EditorColorState {
        &self.color
    }

    /// Returns graph-native effect state references.
    #[must_use]
    pub const fn effect(&self) -> &EditorEffectState {
        &self.effect
    }

    /// Returns ordinary editable AI artifact references and runtime availability.
    #[must_use]
    pub const fn ai(&self) -> &EditorAiState {
        &self.ai
    }

    /// Returns playback state.
    #[must_use]
    pub const fn playback(&self) -> &EditorPlaybackState {
        &self.playback
    }

    /// Returns export state.
    #[must_use]
    pub const fn export(&self) -> &EditorExportState {
        &self.export
    }
}

pub(crate) fn execute_editor_state(
    dispatcher: &mut EngineCommandDispatcher,
    command: GetEditorState,
) -> Result<GetEditorStateResult> {
    let transaction_id = EngineTransactionId::new(command.into_transaction_id())?;
    let outcome = dispatcher.dispatch(EngineCommandRequest::new(
        transaction_id,
        EngineCommand::InspectEditorState,
    ))?;
    let EngineCommandResult::EditorState(state) = outcome.result() else {
        return Err(unsupported_engine_value("get_editor_state"));
    };
    Ok(GetEditorStateResult::new(
        outcome.transaction_id().as_str().to_owned(),
        outcome.command_sequence(),
        public_snapshot(state)?,
    ))
}

pub(crate) fn public_snapshot(state: &EngineState) -> Result<EditorStateSnapshot> {
    let project = state.project();
    let snapshot = project.snapshot();
    let documents = state.authored_documents();
    let timeline = EditorCanonicalDocument::from_engine(TIMELINE_RESOURCE, documents.timeline())?;
    let graphs = documents
        .graphs()
        .iter()
        .map(EditorGraphDocument::from_engine)
        .collect::<Result<Vec<_>>>()?;
    let graph_resources = graphs
        .iter()
        .map(|graph| graph.document.resource.clone())
        .collect::<Vec<_>>();
    let extensions = state
        .extensions()
        .iter()
        .map(EditorExtensionRecord::from_engine)
        .collect::<Vec<_>>();
    let recovery = match state.project_recovery() {
        EngineAvailability::Detached => EditorAvailability::Detached,
        EngineAvailability::Attached(value) => {
            EditorAvailability::Attached(recovery::public_snapshot(value))
        }
        _ => return Err(unsupported_engine_value("convert_recovery_availability")),
    };
    let automation = match state.audio_automation() {
        EngineAvailability::Detached => EditorAvailability::Detached,
        EngineAvailability::Attached(value) => {
            EditorAvailability::Attached(audio_automation::public_snapshot(value)?)
        }
        _ => return Err(unsupported_engine_value("convert_automation_availability")),
    };
    let effect_records = extensions
        .iter()
        .filter(|record| record.kind == EFFECT_KIND)
        .cloned()
        .collect();
    let ai_records = extensions
        .iter()
        .filter(|record| record.kind == AI_ARTIFACT_KIND)
        .cloned()
        .collect();
    let editorial = snapshot.editorial_project();
    let audio_tracks = state
        .audio_tracks()
        .iter()
        .map(public_audio_track)
        .collect::<Result<Vec<_>>>()?;
    let clip_mix = EditorCanonicalDocument::from_engine(CLIP_MIX_RESOURCE, documents.clip_mix())?;
    let playback = public_playback(state)?;
    let export = public_export(state.export())?;
    Ok(EditorStateSnapshot {
        schema_version: EDITOR_STATE_SCHEMA_VERSION.clone(),
        project: EditorProjectState {
            project_id: snapshot.project_id().to_string(),
            root_timeline_id: snapshot.root_timeline_id().to_string(),
            project_revision: snapshot.revision(),
            history_capacity: project.capacity() as u64,
            undo_depth: project.undo_depth() as u64,
            redo_depth: project.redo_depth() as u64,
            next_undo: project.next_undo().map(|kind| kind.code().to_owned()),
            next_redo: project.next_redo().map(|kind| kind.code().to_owned()),
            semantic_hash_algorithm: state.diagnostics().hash_algorithm().to_owned(),
            semantic_hash_format_revision: state.diagnostics().hash_format_revision(),
            semantic_hash: state.diagnostics().content_hash().to_string(),
            settings: project::public_snapshot(state.settings()),
            recovery,
            extensions: extensions.clone(),
            timeline_resource: TIMELINE_RESOURCE.to_owned(),
            graph_resources: graph_resources.clone(),
            clip_mix_resource: CLIP_MIX_RESOURCE.to_owned(),
        },
        timeline: EditorTimelineState {
            timeline_count: editorial.timelines().len() as u64,
            document: timeline,
        },
        graph: EditorGraphState { documents: graphs },
        media: EditorMediaState {
            media_count: editorial.media_references().len() as u64,
            timeline_resource: TIMELINE_RESOURCE.to_owned(),
            timeline_sha256: documents.timeline().sha256().to_owned(),
        },
        audio: EditorAudioState {
            audio_track_count: audio_tracks.len() as u64,
            tracks: audio_tracks,
            timeline_resource: TIMELINE_RESOURCE.to_owned(),
            clip_mix,
            automation,
        },
        color: public_color(state.settings(), graph_resources.clone())?,
        effect: EditorEffectState {
            graph_resources: graph_resources.clone(),
            extension_records: effect_records,
        },
        ai: EditorAiState {
            runtime_availability: "not_implemented".to_owned(),
            graph_resources,
            artifact_records: ai_records,
        },
        playback,
        export,
    })
}

fn public_color(
    value: &EngineProjectSettingsState,
    graph_resources: Vec<String>,
) -> Result<EditorColorState> {
    let management = match value.resolved().color() {
        EngineColorSettings::BuiltInAcesCg { working_space } => {
            EditorColorManagement::BuiltInAcesCg {
                working_space: working_space.clone(),
            }
        }
        EngineColorSettings::PinnedConfig {
            config_id,
            config_fingerprint,
            working_space,
        } => EditorColorManagement::PinnedConfig {
            config_id: config_id.clone(),
            config_fingerprint: config_fingerprint.clone(),
            working_space: working_space.clone(),
        },
        _ => return Err(unsupported_engine_value("convert_color_management")),
    };
    let render = *value.resolved().render();
    let render_target = match render.color_target() {
        EngineRenderColorTarget::ProjectWorking => EditorRenderColorTarget::ProjectWorking,
        EngineRenderColorTarget::Display => EditorRenderColorTarget::Display,
        EngineRenderColorTarget::Delivery => EditorRenderColorTarget::Delivery,
        _ => return Err(unsupported_engine_value("convert_render_color_target")),
    };
    Ok(EditorColorState {
        settings_resource: SETTINGS_RESOURCE.to_owned(),
        project_revision: value.project_revision(),
        management,
        render_target,
        render_settings_fingerprint: render.fingerprint().to_string(),
        graph_resources,
    })
}

fn public_audio_track(value: &EngineAudioTrackState) -> Result<EditorAudioTrackState> {
    let destination = match value.destination() {
        EngineAudioRouteDestination::Main => EditorAudioRouteDestination::Main,
        EngineAudioRouteDestination::Track(track_id) => EditorAudioRouteDestination::Track {
            track_id: track_id.clone(),
        },
        _ => return Err(unsupported_engine_value("convert_audio_route_destination")),
    };
    let routes = value
        .routes()
        .iter()
        .map(|route| {
            let target = match route.target() {
                EngineAudioChannelTarget::Channel(channel) => EditorAudioChannelTarget::Channel {
                    channel: channel.clone(),
                },
                EngineAudioChannelTarget::Muted => EditorAudioChannelTarget::Muted,
                _ => return Err(unsupported_engine_value("convert_audio_channel_target")),
            };
            Ok(EditorAudioChannelRoute {
                source: route.source().to_owned(),
                target,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let continuity = match value.continuity() {
        EngineAudioContinuity::Audited {
            uninterrupted_record_coverage,
            seams,
        } => EditorAudioContinuity::Audited {
            uninterrupted_record_coverage: *uninterrupted_record_coverage,
            seams: seams
                .iter()
                .map(|seam| {
                    let record = match seam.record() {
                        EngineAudioRecordContinuity::Seamless => {
                            EditorAudioRecordContinuity::Seamless
                        }
                        EngineAudioRecordContinuity::Gap { sample_count } => {
                            EditorAudioRecordContinuity::Gap { sample_count }
                        }
                        EngineAudioRecordContinuity::Overlap { sample_count } => {
                            EditorAudioRecordContinuity::Overlap { sample_count }
                        }
                        _ => return Err(unsupported_engine_value("convert_audio_record_seam")),
                    };
                    let source = match seam.source() {
                        EngineAudioSourceContinuity::Continuous => {
                            EditorAudioSourceContinuity::Continuous
                        }
                        EngineAudioSourceContinuity::Discontinuous { expected, actual } => {
                            EditorAudioSourceContinuity::Discontinuous {
                                expected: *expected,
                                actual: *actual,
                            }
                        }
                        EngineAudioSourceContinuity::DifferentClip { left, right } => {
                            EditorAudioSourceContinuity::DifferentClip {
                                left: left.clone(),
                                right: right.clone(),
                            }
                        }
                        _ => return Err(unsupported_engine_value("convert_audio_source_seam")),
                    };
                    Ok(EditorAudioSeam {
                        left_clip_id: seam.left_clip_id().to_owned(),
                        right_clip_id: seam.right_clip_id().to_owned(),
                        record,
                        source,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        },
        EngineAudioContinuity::Unsupported { reason } => EditorAudioContinuity::Unsupported {
            reason: reason.clone(),
        },
        _ => return Err(unsupported_engine_value("convert_audio_continuity")),
    };
    Ok(EditorAudioTrackState {
        timeline_id: value.timeline_id().to_owned(),
        track_id: value.track_id().to_owned(),
        sample_rate: value.sample_rate(),
        source_channels: value.source_channels().to_vec(),
        destination,
        destination_channels: value.destination_channels().to_vec(),
        routes,
        clip_count: value.clip_count(),
        continuity,
    })
}

fn public_playback(state: &EngineState) -> Result<EditorPlaybackState> {
    let playback = state.playback();
    if !playback.attached() {
        return Ok(EditorPlaybackState::Detached);
    }
    let latest = playback
        .latest()
        .map(|observation| -> Result<EditorPlaybackSnapshot> {
            let mut snapshot = public_playback_snapshot(observation.snapshot())?;
            snapshot.failure = observation.failure().map(EditorFailureState::from_engine);
            Ok(snapshot)
        })
        .transpose()?;
    Ok(EditorPlaybackState::Attached {
        pending_command: playback.pending_command(),
        latest: latest.map(Box::new),
    })
}

fn public_playback_snapshot(value: EnginePlaybackSnapshot) -> Result<EditorPlaybackSnapshot> {
    let mode = match value.mode() {
        EnginePlaybackMode::Paused => "paused",
        EnginePlaybackMode::Playing => "playing",
        EnginePlaybackMode::Scrubbing => "scrubbing",
        EnginePlaybackMode::Ended => "ended",
        _ => return Err(unsupported_engine_value("convert_playback_mode")),
    };
    let direction = match value.direction() {
        EnginePlaybackDirection::Forward => "forward",
        EnginePlaybackDirection::Reverse => "reverse",
    };
    let audio_state = match value.audio_state() {
        EngineTransportAudioState::Synchronized => "synchronized",
        EngineTransportAudioState::DiscardPending(_) => "discard_pending",
        EngineTransportAudioState::MutedInactive(_) => "muted_inactive",
        EngineTransportAudioState::MutedUnsupportedRate(_) => "muted_unsupported_rate",
        _ => return Err(unsupported_engine_value("convert_playback_audio_state")),
    };
    let degradation_value = value.degradation();
    let degradation = [
        (
            EngineTransportDegradationCode::FrameFailure,
            "frame_failure",
        ),
        (
            EngineTransportDegradationCode::ViewportBackpressure,
            "viewport_backpressure",
        ),
        (
            EngineTransportDegradationCode::PrefetchFailure,
            "prefetch_failure",
        ),
        (
            EngineTransportDegradationCode::AudioDiscardPending,
            "audio_discard_pending",
        ),
        (
            EngineTransportDegradationCode::AudioRateUnsupported,
            "audio_rate_unsupported",
        ),
    ]
    .into_iter()
    .filter(|(code, _)| degradation_value.contains(*code))
    .map(|(_, name)| name.to_owned())
    .collect();
    let discard = value.audio_discard_status();
    let rate = value.rate();
    let drops = value.drop_statistics();
    Ok(EditorPlaybackSnapshot {
        mode: mode.to_owned(),
        playhead: EditorRationalTime::from_engine(value.playhead()),
        scheduled_frame: value.scheduled_frame().map(EditorRationalTime::from_engine),
        scheduled_due_clock: value
            .scheduled_due_clock()
            .map(EditorRationalTime::from_engine),
        rate_numerator: rate.numerator(),
        rate_denominator: rate.denominator(),
        direction: direction.to_owned(),
        loop_range: value.loop_range().map(EditorTimeRange::from_engine),
        epoch: value.epoch(),
        total_dropped: drops.total_dropped(),
        consecutive_dropped: drops.consecutive_dropped(),
        forced_presentations: drops.forced_presentations(),
        audio_state: audio_state.to_owned(),
        discard_requested_generation: discard.requested_generation,
        discard_applied_generation: discard.applied_generation,
        degradation,
        failure: None,
    })
}

fn public_export(
    value: &superi_engine::editor_state::EditorExportState,
) -> Result<EditorExportState> {
    if !value.attached() {
        return Ok(EditorExportState::Detached);
    }
    Ok(EditorExportState::Attached {
        latest: value.latest().map(public_export_queue).transpose()?,
    })
}

fn public_export_queue(value: &EngineExportJobState) -> Result<EditorExportQueueState> {
    Ok(EditorExportQueueState {
        revision: value.revision(),
        jobs: value
            .jobs()
            .iter()
            .map(public_export_job)
            .collect::<Result<Vec<_>>>()?,
    })
}

fn public_export_job(value: &EngineExportJobSnapshot) -> Result<EditorExportJobSnapshot> {
    let progress = value.progress();
    let elapsed = value.elapsed().map(|elapsed| EditorElapsedDuration {
        seconds: elapsed.as_secs(),
        nanoseconds: elapsed.subsec_nanos(),
    });
    Ok(EditorExportJobSnapshot {
        job_id: value.job_id().to_string(),
        status: value.status().code().to_owned(),
        attempt: value.attempt(),
        progress_revision: progress.revision(),
        completed_units: progress.completed_units(),
        total_units: progress.total_units(),
        elapsed,
        dependencies: value
            .dependencies()
            .iter()
            .map(ToString::to_string)
            .collect(),
        failure: value.failure().map(EditorFailureState::from_engine),
        dependency_failures: value
            .dependency_failures()
            .iter()
            .map(|failure| EditorExportDependencyFailure {
                job_id: failure.job_id().to_string(),
                state: failure.state().code().to_owned(),
            })
            .collect(),
        has_result: value.has_result(),
        retry_allowed: value.retry_allowed(),
        is_final: value.is_final(),
    })
}

fn unsupported_engine_value(operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "engine editor state value is unsupported by this public schema",
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
