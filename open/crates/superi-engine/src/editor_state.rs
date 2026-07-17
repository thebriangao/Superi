//! One coherent immutable editor-state capture owned by the engine dispatcher.
//!
//! Authored project state remains owned by project history. Runtime observations remain owned by
//! their audio, recovery, playback, and export subsystems. This module captures those owners once
//! without polling workers, discovering files, executing transport work, or publishing an event.

use sha2::{Digest, Sha256};
use superi_audio::serialize::{serialize_clip_mix_state, CLIP_MIX_FORMAT_REVISION};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::ChannelPosition;
use superi_graph::serialize::{serialize_graph, GRAPH_DOCUMENT_FORMAT_REVISION};
use superi_project::document::{ProjectGraph, ProjectSnapshot};
use superi_project::ProjectDiagnostics;
use superi_timeline::model::{
    AudioChannelTarget, AudioRecordContinuity, AudioRouteDestination, AudioSourceContinuity,
    AudioSpan, AudioTrackSemantics, TrackSemantics,
};
use superi_timeline::retime::RetimeMode;
use superi_timeline::serialize::{serialize_timeline_state, TIMELINE_STATE_FORMAT_REVISION};

use crate::dispatcher::{AudioAutomationSnapshot, EngineReportedFailure};
use crate::export_dispatch::EngineExportJobState;
use crate::history::ProjectHistoryState;
use crate::project_recovery::ProjectRecoveryState;
use crate::project_settings::ProjectSettingsState;
use crate::transport::PlaybackTransportSnapshot;

/// Explicit attachment state for one optional editor-state owner.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditorStateAvailability<T> {
    /// The engine dispatcher does not own this subsystem state.
    Detached,
    /// The engine dispatcher owns one complete immutable observation.
    Attached(T),
}

/// One complete canonical JSON state document owned by an existing domain codec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorCanonicalDocument {
    format: String,
    format_revision: u32,
    byte_length: u64,
    sha256: String,
    bytes: Vec<u8>,
}

impl EditorCanonicalDocument {
    fn new(format: &'static str, format_revision: u32, bytes: Vec<u8>) -> Self {
        Self {
            format: format.to_owned(),
            format_revision,
            byte_length: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            bytes,
        }
    }

    /// Returns the permanent domain document format.
    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
    }

    /// Returns the incompatible domain document revision.
    #[must_use]
    pub const fn format_revision(&self) -> u32 {
        self.format_revision
    }

    /// Returns the exact canonical document byte count.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }

    /// Returns the lowercase SHA-256 identity of the complete canonical document.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Returns complete canonical JSON bytes without media payloads or runtime artifacts.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Durable ownership meaning for one canonical graph document.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditorGraphScope {
    /// The graph is compiled from one editorial timeline root.
    Timeline { root_timeline_id: String },
    /// The graph is retained independently under an editor-facing name.
    Standalone { name: String },
}

/// One complete canonical graph document and its stable project ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorGraphDocument {
    graph_id: String,
    graph_revision: u64,
    scope: EditorGraphScope,
    document: EditorCanonicalDocument,
}

impl EditorGraphDocument {
    /// Returns the stable graph identity.
    #[must_use]
    pub fn graph_id(&self) -> &str {
        &self.graph_id
    }

    /// Returns the authored graph revision.
    #[must_use]
    pub const fn graph_revision(&self) -> u64 {
        self.graph_revision
    }

    /// Returns timeline or standalone ownership meaning.
    #[must_use]
    pub const fn scope(&self) -> &EditorGraphScope {
        &self.scope
    }

    /// Returns the complete canonical graph state document.
    #[must_use]
    pub const fn document(&self) -> &EditorCanonicalDocument {
        &self.document
    }
}

/// Complete canonical authored state documents captured from one project snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorAuthoredDocuments {
    timeline: EditorCanonicalDocument,
    graphs: Vec<EditorGraphDocument>,
    clip_mix: EditorCanonicalDocument,
}

impl EditorAuthoredDocuments {
    pub(crate) fn from_snapshot(snapshot: &ProjectSnapshot) -> Result<Self> {
        let timeline = EditorCanonicalDocument::new(
            "superi.timeline",
            TIMELINE_STATE_FORMAT_REVISION,
            serialize_timeline_state(snapshot.editorial_project())?,
        );
        let graphs = snapshot
            .graphs()
            .map(|graph| {
                let graph_snapshot = graph.snapshot();
                let scope = match graph {
                    ProjectGraph::Timeline(compilation) => EditorGraphScope::Timeline {
                        root_timeline_id: compilation.root_timeline_id().to_string(),
                    },
                    ProjectGraph::Standalone(standalone) => EditorGraphScope::Standalone {
                        name: standalone.name().to_owned(),
                    },
                    _ => {
                        return Err(Error::new(
                            ErrorCategory::Unsupported,
                            Recoverability::UserCorrectable,
                            "project graph kind is unsupported by editor state capture",
                        )
                        .with_context(ErrorContext::new(
                            "superi-engine.editor-state",
                            "capture_graph_document",
                        )));
                    }
                };
                Ok(EditorGraphDocument {
                    graph_id: graph_snapshot.graph_id().to_string(),
                    graph_revision: graph_snapshot.revision(),
                    scope,
                    document: EditorCanonicalDocument::new(
                        "superi.graph",
                        GRAPH_DOCUMENT_FORMAT_REVISION,
                        serialize_graph(&graph_snapshot)?,
                    ),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let clip_mix = EditorCanonicalDocument::new(
            "superi.clip-mix",
            CLIP_MIX_FORMAT_REVISION,
            serialize_clip_mix_state(snapshot.clip_mix_state())?,
        );
        Ok(Self {
            timeline,
            graphs,
            clip_mix,
        })
    }

    /// Returns complete timeline, media, track, clip, routing, and edit-state JSON.
    #[must_use]
    pub const fn timeline(&self) -> &EditorCanonicalDocument {
        &self.timeline
    }

    /// Returns every complete graph document in stable graph identity order.
    #[must_use]
    pub fn graphs(&self) -> &[EditorGraphDocument] {
        &self.graphs
    }

    /// Returns complete authored clip-mix controls and channel mapping JSON.
    #[must_use]
    pub const fn clip_mix(&self) -> &EditorCanonicalDocument {
        &self.clip_mix
    }
}

/// Record coverage at one public audio continuity seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditorAudioRecordContinuity {
    /// The right span begins at the exact sample after the left span.
    Seamless,
    /// Neither adjacent clip covers these sample frames.
    Gap { sample_count: u64 },
    /// Both adjacent clips cover these sample frames.
    Overlap { sample_count: u64 },
}

/// Source relationship at one public audio continuity seam.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditorAudioSourceContinuity {
    /// One clip continues at its next exact source sample.
    Continuous,
    /// One clip jumps or repeats between exact source samples.
    Discontinuous { expected: i64, actual: i64 },
    /// The seam changes to another linked clip.
    DifferentClip { left: String, right: String },
}

/// Typed destination for one public audio track route.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditorAudioRouteDestination {
    Main,
    Track(String),
}

/// Destination or explicit suppression for one source channel.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditorAudioChannelTarget {
    Channel(String),
    Muted,
}

/// One ordered source-channel routing decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorAudioChannelRoute {
    source: String,
    target: EditorAudioChannelTarget,
}

impl EditorAudioChannelRoute {
    /// Returns the semantic source channel code.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the destination channel or explicit mute decision.
    #[must_use]
    pub const fn target(&self) -> &EditorAudioChannelTarget {
        &self.target
    }
}

/// One adjacent sample-clock seam on an audio track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorAudioSeam {
    left_clip_id: String,
    right_clip_id: String,
    record: EditorAudioRecordContinuity,
    source: EditorAudioSourceContinuity,
}

impl EditorAudioSeam {
    /// Returns the clip before the seam.
    #[must_use]
    pub fn left_clip_id(&self) -> &str {
        &self.left_clip_id
    }

    /// Returns the clip after the seam.
    #[must_use]
    pub fn right_clip_id(&self) -> &str {
        &self.right_clip_id
    }

    /// Returns exact record coverage at the seam.
    #[must_use]
    pub const fn record(&self) -> EditorAudioRecordContinuity {
        self.record
    }

    /// Returns exact source continuity at the seam.
    #[must_use]
    pub const fn source(&self) -> &EditorAudioSourceContinuity {
        &self.source
    }
}

/// Sample-clock continuity evidence for one audio track.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditorAudioContinuity {
    /// Every clip was audited without rounding.
    Audited {
        uninterrupted_record_coverage: bool,
        seams: Vec<EditorAudioSeam>,
    },
    /// Canonical authored timing is retained, but a linear sample-span audit would lose meaning.
    Unsupported { reason: String },
}

/// One audio track's exact clock and continuity evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
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

impl EditorAudioTrackState {
    pub(crate) fn from_snapshot(snapshot: &ProjectSnapshot) -> Result<Vec<Self>> {
        snapshot
            .editorial_project()
            .timelines()
            .flat_map(|timeline| {
                timeline.tracks().iter().filter_map(move |track| {
                    let TrackSemantics::Audio(semantics) = track.semantics() else {
                        return None;
                    };
                    Some(Self::from_track(
                        timeline.id().to_string(),
                        track.id().to_string(),
                        semantics,
                        track.items().iter().filter_map(|item| item.as_clip()),
                    ))
                })
            })
            .collect()
    }

    fn from_track<'a>(
        timeline_id: String,
        track_id: String,
        semantics: &AudioTrackSemantics,
        clips: impl Iterator<Item = &'a superi_timeline::model::Clip>,
    ) -> Result<Self> {
        let clips = clips.collect::<Vec<_>>();
        let continuity = Self::audit(semantics, &clips);
        let source_channels = semantics
            .channel_layout()
            .positions()
            .iter()
            .copied()
            .map(channel_position_code)
            .collect::<Result<Vec<_>>>()?;
        let routing = semantics.routing();
        let destination = match routing.destination() {
            AudioRouteDestination::Main => EditorAudioRouteDestination::Main,
            AudioRouteDestination::Track(track_id) => {
                EditorAudioRouteDestination::Track(track_id.to_string())
            }
            _ => return Err(unsupported_audio_state("audio route destination")),
        };
        let destination_channels = routing
            .destination_layout()
            .positions()
            .iter()
            .copied()
            .map(channel_position_code)
            .collect::<Result<Vec<_>>>()?;
        let routes = routing
            .routes()
            .iter()
            .map(|route| {
                let target = match route.target() {
                    AudioChannelTarget::Channel(position) => {
                        EditorAudioChannelTarget::Channel(channel_position_code(position)?)
                    }
                    AudioChannelTarget::Muted => EditorAudioChannelTarget::Muted,
                    _ => return Err(unsupported_audio_state("audio channel target")),
                };
                Ok(EditorAudioChannelRoute {
                    source: channel_position_code(route.source())?,
                    target,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            timeline_id,
            track_id,
            sample_rate: semantics.sample_rate(),
            source_channels,
            destination,
            destination_channels,
            routes,
            clip_count: clips.len() as u64,
            continuity,
        })
    }

    fn audit(
        semantics: &AudioTrackSemantics,
        clips: &[&superi_timeline::model::Clip],
    ) -> EditorAudioContinuity {
        let sample_timebase = semantics.timebase();
        let mut spans = Vec::with_capacity(clips.len());
        for clip in clips {
            if clip.time_map().mode() != RetimeMode::Identity {
                return EditorAudioContinuity::Unsupported {
                    reason: "retimed_clip".to_owned(),
                };
            }
            let source = match clip
                .source_time_at(
                    clip.record_range().start(),
                    superi_core::time::TimeRounding::Exact,
                )
                .and_then(|mapped| {
                    mapped
                        .time()
                        .checked_rescale(sample_timebase, superi_core::time::TimeRounding::Exact)
                })
                .and_then(|time| {
                    superi_core::time::SampleTime::new(time.value(), semantics.sample_rate())
                }) {
                Ok(source) => source,
                Err(_) => {
                    return EditorAudioContinuity::Unsupported {
                        reason: "non_integral_source_sample".to_owned(),
                    };
                }
            };
            let Ok(span) = AudioSpan::new(
                clip.id(),
                clip.record_range().start(),
                source,
                clip.record_range().duration().value(),
            ) else {
                return EditorAudioContinuity::Unsupported {
                    reason: "unrepresentable_sample_span".to_owned(),
                };
            };
            spans.push(span);
        }
        let Ok(report) = semantics.audit_continuity(&spans) else {
            return EditorAudioContinuity::Unsupported {
                reason: "invalid_sample_sequence".to_owned(),
            };
        };
        let mut seams = Vec::with_capacity(report.seams().len());
        for seam in report.seams() {
            let record = match seam.record() {
                AudioRecordContinuity::Seamless => EditorAudioRecordContinuity::Seamless,
                AudioRecordContinuity::Gap { sample_count } => {
                    EditorAudioRecordContinuity::Gap { sample_count }
                }
                AudioRecordContinuity::Overlap { sample_count } => {
                    EditorAudioRecordContinuity::Overlap { sample_count }
                }
                _ => {
                    return EditorAudioContinuity::Unsupported {
                        reason: "unknown_record_continuity".to_owned(),
                    };
                }
            };
            let source = match seam.source() {
                AudioSourceContinuity::Continuous => EditorAudioSourceContinuity::Continuous,
                AudioSourceContinuity::Discontinuous { expected, actual } => {
                    EditorAudioSourceContinuity::Discontinuous {
                        expected: expected.sample(),
                        actual: actual.sample(),
                    }
                }
                AudioSourceContinuity::DifferentClip { left, right } => {
                    EditorAudioSourceContinuity::DifferentClip {
                        left: left.to_string(),
                        right: right.to_string(),
                    }
                }
                _ => {
                    return EditorAudioContinuity::Unsupported {
                        reason: "unknown_source_continuity".to_owned(),
                    };
                }
            };
            seams.push(EditorAudioSeam {
                left_clip_id: seam.left_clip_id().to_string(),
                right_clip_id: seam.right_clip_id().to_string(),
                record,
                source,
            });
        }
        EditorAudioContinuity::Audited {
            uninterrupted_record_coverage: report.has_uninterrupted_record_coverage(),
            seams,
        }
    }

    /// Returns the owning timeline.
    #[must_use]
    pub fn timeline_id(&self) -> &str {
        &self.timeline_id
    }

    /// Returns the stable audio track identity.
    #[must_use]
    pub fn track_id(&self) -> &str {
        &self.track_id
    }

    /// Returns the integral sample clock.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns ordered semantic source channel codes.
    #[must_use]
    pub fn source_channels(&self) -> &[String] {
        &self.source_channels
    }

    /// Returns the typed track-level destination.
    #[must_use]
    pub const fn destination(&self) -> &EditorAudioRouteDestination {
        &self.destination
    }

    /// Returns ordered semantic destination channel codes.
    #[must_use]
    pub fn destination_channels(&self) -> &[String] {
        &self.destination_channels
    }

    /// Returns one explicit decision per source channel in stream order.
    #[must_use]
    pub fn routes(&self) -> &[EditorAudioChannelRoute] {
        &self.routes
    }

    /// Returns the number of clip spans considered by the audit.
    #[must_use]
    pub const fn clip_count(&self) -> u64 {
        self.clip_count
    }

    /// Returns audited continuity or an explicit exactness limitation.
    #[must_use]
    pub const fn continuity(&self) -> &EditorAudioContinuity {
        &self.continuity
    }
}

fn channel_position_code(position: ChannelPosition) -> Result<String> {
    let code = match position {
        ChannelPosition::FrontLeft => "front_left",
        ChannelPosition::FrontRight => "front_right",
        ChannelPosition::FrontCenter => "front_center",
        ChannelPosition::LowFrequency => "low_frequency",
        ChannelPosition::BackLeft => "back_left",
        ChannelPosition::BackRight => "back_right",
        ChannelPosition::FrontLeftOfCenter => "front_left_of_center",
        ChannelPosition::FrontRightOfCenter => "front_right_of_center",
        ChannelPosition::BackCenter => "back_center",
        ChannelPosition::SideLeft => "side_left",
        ChannelPosition::SideRight => "side_right",
        ChannelPosition::TopCenter => "top_center",
        ChannelPosition::TopFrontLeft => "top_front_left",
        ChannelPosition::TopFrontCenter => "top_front_center",
        ChannelPosition::TopFrontRight => "top_front_right",
        ChannelPosition::TopBackLeft => "top_back_left",
        ChannelPosition::TopBackCenter => "top_back_center",
        ChannelPosition::TopBackRight => "top_back_right",
        ChannelPosition::Discrete(index) => return Ok(format!("discrete:{index}")),
        _ => return Err(unsupported_audio_state("audio channel position")),
    };
    Ok(code.to_owned())
}

fn unsupported_audio_state(value: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        format!("editor state cannot project an unknown {value}"),
    )
    .with_context(ErrorContext::new(
        "superi-engine.editor-state",
        "project_audio_track",
    ))
}

/// Safe durable failure classification retained for one extension record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorExtensionFailure {
    category: String,
    recoverability: String,
    total_failures: u64,
    consecutive_failures: u32,
}

impl EditorExtensionFailure {
    /// Returns the permanent error category code.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    /// Returns the permanent recovery classification code.
    #[must_use]
    pub fn recoverability(&self) -> &str {
        &self.recoverability
    }

    /// Returns the retained lifetime failure count.
    #[must_use]
    pub const fn total_failures(&self) -> u64 {
        self.total_failures
    }

    /// Returns the retained consecutive failure count.
    #[must_use]
    pub const fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}

/// Bounded public-safe descriptor for one durable extension record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorExtensionRecord {
    extension_id: String,
    record_id: String,
    extension_version: String,
    kind: String,
    payload_schema: String,
    requested_capabilities: Vec<String>,
    granted_capabilities: Vec<String>,
    lifecycle: String,
    failure: Option<EditorExtensionFailure>,
    payload_byte_length: u64,
    payload_sha256: String,
}

impl EditorExtensionRecord {
    pub(crate) fn from_snapshot(snapshot: &ProjectSnapshot) -> Vec<Self> {
        snapshot
            .extension_records()
            .values()
            .map(|record| Self {
                extension_id: record.key().extension_id().as_str().to_owned(),
                record_id: record.key().record_id().as_str().to_owned(),
                extension_version: record.extension_version().to_string(),
                kind: record.kind().as_str().to_owned(),
                payload_schema: record.payload_schema().to_string(),
                requested_capabilities: record
                    .requested_capabilities()
                    .iter()
                    .map(|capability| capability.as_str().to_owned())
                    .collect(),
                granted_capabilities: record
                    .granted_capabilities()
                    .iter()
                    .map(|capability| capability.as_str().to_owned())
                    .collect(),
                lifecycle: record.lifecycle().code().to_owned(),
                failure: record.failure().map(|failure| EditorExtensionFailure {
                    category: failure.category().code().to_owned(),
                    recoverability: failure.recoverability().code().to_owned(),
                    total_failures: failure.total_failures(),
                    consecutive_failures: failure.consecutive_failures(),
                }),
                payload_byte_length: record.payload().len() as u64,
                payload_sha256: format!("{:x}", Sha256::digest(record.payload())),
            })
            .collect()
    }

    /// Returns the stable extension component identity.
    #[must_use]
    pub fn extension_id(&self) -> &str {
        &self.extension_id
    }

    /// Returns the stable extension-owned record identity.
    #[must_use]
    pub fn record_id(&self) -> &str {
        &self.record_id
    }

    /// Returns the extension version that owns this record.
    #[must_use]
    pub fn extension_version(&self) -> &str {
        &self.extension_version
    }

    /// Returns the open namespaced record kind.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns the schema identity that interprets the omitted payload.
    #[must_use]
    pub fn payload_schema(&self) -> &str {
        &self.payload_schema
    }

    /// Returns requested capabilities in canonical identity order.
    #[must_use]
    pub fn requested_capabilities(&self) -> &[String] {
        &self.requested_capabilities
    }

    /// Returns user-granted capabilities in canonical identity order.
    #[must_use]
    pub fn granted_capabilities(&self) -> &[String] {
        &self.granted_capabilities
    }

    /// Returns the durable lifecycle code.
    #[must_use]
    pub fn lifecycle(&self) -> &str {
        &self.lifecycle
    }

    /// Returns safe failure classification without raw messages or contexts.
    #[must_use]
    pub const fn failure(&self) -> Option<&EditorExtensionFailure> {
        self.failure.as_ref()
    }

    /// Returns the omitted payload byte count.
    #[must_use]
    pub const fn payload_byte_length(&self) -> u64 {
        self.payload_byte_length
    }

    /// Returns the lowercase SHA-256 identity of the omitted payload bytes.
    #[must_use]
    pub fn payload_sha256(&self) -> &str {
        &self.payload_sha256
    }
}

/// Latest complete playback observation retained by the dispatcher.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorPlaybackObservation {
    snapshot: PlaybackTransportSnapshot,
    failure: Option<EngineReportedFailure>,
}

impl EditorPlaybackObservation {
    pub(crate) const fn new(
        snapshot: PlaybackTransportSnapshot,
        failure: Option<EngineReportedFailure>,
    ) -> Self {
        Self { snapshot, failure }
    }

    /// Returns the complete replacement transport state.
    #[must_use]
    pub const fn snapshot(&self) -> PlaybackTransportSnapshot {
        self.snapshot
    }

    /// Returns bounded failure evidence from the command that produced this observation.
    #[must_use]
    pub const fn failure(&self) -> Option<&EngineReportedFailure> {
        self.failure.as_ref()
    }
}

/// Playback attachment, in-flight command status, and latest retained observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorPlaybackState {
    attached: bool,
    pending_command: bool,
    latest: Option<EditorPlaybackObservation>,
}

impl EditorPlaybackState {
    pub(crate) const fn new(
        attached: bool,
        pending_command: bool,
        latest: Option<EditorPlaybackObservation>,
    ) -> Self {
        Self {
            attached,
            pending_command,
            latest,
        }
    }

    /// Reports whether a playback command bridge is attached.
    #[must_use]
    pub const fn attached(&self) -> bool {
        self.attached
    }

    /// Reports whether one accepted playback command still awaits execution or collection.
    #[must_use]
    pub const fn pending_command(&self) -> bool {
        self.pending_command
    }

    /// Returns the latest dispatcher-observed transport replacement state.
    #[must_use]
    pub const fn latest(&self) -> Option<&EditorPlaybackObservation> {
        self.latest.as_ref()
    }
}

/// Export attachment and latest dispatcher-observed queue replacement state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorExportState {
    attached: bool,
    latest: Option<EngineExportJobState>,
}

impl EditorExportState {
    pub(crate) const fn new(attached: bool, latest: Option<EngineExportJobState>) -> Self {
        Self { attached, latest }
    }

    /// Reports whether a logical export queue is attached.
    #[must_use]
    pub const fn attached(&self) -> bool {
        self.attached
    }

    /// Returns the latest queue state observed by an explicit dispatcher command.
    #[must_use]
    pub const fn latest(&self) -> Option<&EngineExportJobState> {
        self.latest.as_ref()
    }
}

/// Complete immutable editor state captured at one authoritative project revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorStateSnapshot {
    project: ProjectHistoryState,
    diagnostics: ProjectDiagnostics,
    settings: ProjectSettingsState,
    authored_documents: EditorAuthoredDocuments,
    audio_tracks: Vec<EditorAudioTrackState>,
    extensions: Vec<EditorExtensionRecord>,
    audio_automation: EditorStateAvailability<AudioAutomationSnapshot>,
    project_recovery: EditorStateAvailability<ProjectRecoveryState>,
    playback: EditorPlaybackState,
    export: EditorExportState,
}

impl EditorStateSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        project: ProjectHistoryState,
        diagnostics: ProjectDiagnostics,
        settings: ProjectSettingsState,
        authored_documents: EditorAuthoredDocuments,
        audio_tracks: Vec<EditorAudioTrackState>,
        extensions: Vec<EditorExtensionRecord>,
        audio_automation: EditorStateAvailability<AudioAutomationSnapshot>,
        project_recovery: EditorStateAvailability<ProjectRecoveryState>,
        playback: EditorPlaybackState,
        export: EditorExportState,
    ) -> Self {
        Self {
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
        }
    }

    /// Returns the authoritative authored project snapshot and bounded history metadata.
    #[must_use]
    pub const fn project(&self) -> &ProjectHistoryState {
        &self.project
    }

    /// Returns deterministic evidence derived from the same authored project snapshot.
    #[must_use]
    pub const fn diagnostics(&self) -> &ProjectDiagnostics {
        &self.diagnostics
    }

    /// Returns durable and resolved settings derived from the same project snapshot.
    #[must_use]
    pub const fn settings(&self) -> &ProjectSettingsState {
        &self.settings
    }

    /// Returns canonical versioned state documents from the same project revision.
    #[must_use]
    pub const fn authored_documents(&self) -> &EditorAuthoredDocuments {
        &self.authored_documents
    }

    /// Returns exact sample-clock continuity evidence for every audio track.
    #[must_use]
    pub fn audio_tracks(&self) -> &[EditorAudioTrackState] {
        &self.audio_tracks
    }

    /// Returns bounded extension descriptors in stable compound-identity order.
    #[must_use]
    pub fn extensions(&self) -> &[EditorExtensionRecord] {
        &self.extensions
    }

    /// Returns authored automation state or explicit detached availability.
    #[must_use]
    pub const fn audio_automation(&self) -> &EditorStateAvailability<AudioAutomationSnapshot> {
        &self.audio_automation
    }

    /// Returns the cached recovery catalog or explicit detached availability.
    #[must_use]
    pub const fn project_recovery(&self) -> &EditorStateAvailability<ProjectRecoveryState> {
        &self.project_recovery
    }

    /// Returns playback attachment, pending status, and latest observation.
    #[must_use]
    pub const fn playback(&self) -> &EditorPlaybackState {
        &self.playback
    }

    /// Returns export attachment and latest explicit observation.
    #[must_use]
    pub const fn export(&self) -> &EditorExportState {
        &self.export
    }
}
