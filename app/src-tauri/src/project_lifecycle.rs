//! Desktop project lifecycle presentation above the durable public API host.
//!
//! This module owns only the active project reference, bounded recent-project presentation,
//! recovery presentation, and actionable failure state. Project contents, persistence, recovery
//! artifacts, settings, and media remain owned below this boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use superi_api::commands::ApiCommand;
use superi_api::commands::{
    ExecuteProjectSettingsTransaction, GetProjectRecovery, GetProjectSettings,
    RestoreProjectRecovery,
};
use superi_api::editor::{
    EditorImportedMedia, EditorImportedMediaKind, EditorMediaPath, EditorMediaPathPlatform,
    ExecuteProjectCommand, ExecuteProjectCommandResult, ProjectAction, ProjectActionEvidence,
    ProjectCommand, ProjectCommandEvidence,
};
use superi_api::events::{ApiEvent, ProjectStateChanged};
use superi_api::local::{
    LocalAutomationId, LocalAutomationRequest, LocalAutomationResult, LocalProjectCollision,
    LocalProjectCreateRequest, LocalProjectExecution, LocalProjectHost, LocalProjectSummary,
};
use superi_api::permissions::{
    ApiDestructiveOperation, ApiFilesystemAccess, ApiFilesystemPath, ApiFilesystemScope,
    ApiPermissionContext, ApiPermissionEffect, ApiPermissionRule,
};
use superi_api::project::{ProjectSettingMutation, ProjectSettingValue, ProjectSettingsSnapshot};
use superi_core::diagnostics::UserSafeError;
use superi_core::error::{Error, ErrorCategory, Recoverability};
use superi_core::ids::MediaId;
use superi_engine::editor::{ClipSource, EditorialProject, ProjectDatabase};
use tauri::State;

mod media_preview;
pub(crate) mod source_monitor;
mod source_monitoring;

pub use media_preview::{
    FilmstripArtifact, MediaPreviewBundle, MediaPreviewProduct, MediaPreviewRequest,
    PreviewImageArtifact, WaveformArtifact,
};
pub use source_monitor::{
    desktop_source_monitor_load, desktop_source_monitor_seek, desktop_source_monitor_snapshot,
    desktop_source_monitor_unload, desktop_source_monitor_update_marks, SourceMonitorEngineState,
    SourceMonitorLoadRequest, SourceMonitorMarkMutation, SourceMonitorMarkUpdate,
    SourceMonitorMarks, SourceMonitorSeekRequest, SourceMonitorSnapshot, SourceMonitorStream,
    SourceMonitorTime, SourceMonitorUnloadRequest, SourceMonitorUpdateResult,
};
pub use source_monitoring::{
    MediaRelinkIntent, MediaSourceFingerprint, MediaSourceMonitoring, MediaSourceMonitoringStatus,
    MediaSourcePathState, MediaSourcePathStatus, MediaSourceScanRequest, MediaSourceVolume,
    MediaVolumeKind, MediaVolumeStatus,
};

const MAX_RECENT_PROJECTS: usize = 32;
const MAX_IMPORT_SOURCES: usize = 4096;
const MAX_MEDIA_BATCH_OPERATIONS: usize = 4096;
const MAX_MEDIA_BINS: usize = 1024;
const MAX_MEDIA_DISPLAY_NAME_BYTES: usize = 512;
const MAX_SMART_COLLECTIONS: usize = 256;
const MAX_USER_METADATA_ENTRIES: usize = 128;
const MAX_USER_METADATA_KEY_BYTES: usize = 64;
const MAX_USER_METADATA_VALUE_BYTES: usize = 4096;
const MAX_ANNOTATION_NAME_BYTES: usize = 256;
const MAX_ANNOTATION_LABELS: usize = 32;
const MAX_ANNOTATION_LABEL_BYTES: usize = 64;
const MAX_ANNOTATION_KEYWORDS: usize = 64;
const MAX_ANNOTATION_KEYWORD_BYTES: usize = 64;
const MAX_ANNOTATION_COMMENT_BYTES: usize = 4096;
const MAX_ANNOTATION_PAYLOAD_BYTES: usize = 16 * 1024;
const MAX_MEDIA_SELECTIONS: usize = 64;
const MAX_SELECTION_NAME_BYTES: usize = 128;
const MAX_TRACKED_REGIONS: usize = 32;
const MAX_TRACKED_OBSERVATIONS: usize = 1024;
const MAX_IDENTITY_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_DERIVED_MEDIA_ATTACHMENTS: usize = 16;
const MAX_TRANSCRIPT_SEGMENTS: usize = 4096;
const MAX_TRANSCRIPT_TEXT_BYTES: usize = 16 * 1024;
const MAX_TRANSCRIPT_SPEAKER_BYTES: usize = 256;
const MAX_TIMELINE_RELATIONSHIPS: usize = 64;
const MAX_CONTENT_IDENTIFIER_BYTES: usize = 256;
const MAX_CONTENT_PROVENANCE_BYTES: usize = 512;
const MAX_LOCAL_AI_CONTENT: usize = 2048;
const MAX_LOCAL_AI_TERMS: usize = 64;
const MAX_LOCAL_AI_TERM_BYTES: usize = 256;
const MAX_LOCAL_AI_SEGMENT_LINKS: usize = 256;
const MAX_CONTENT_ANALYSIS_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONTENT_SEARCH_QUERY_BYTES: usize = 512;
const MAX_CONTENT_SEARCH_TOKENS: usize = 32;
const MAX_CONTENT_SEARCH_RESULTS: usize = 256;
const MAX_CONTENT_SEARCH_MATCHES: usize = 16;
const MAX_CONTENT_SEARCH_EVIDENCE_CHARS: usize = 512;
const NORMALIZED_REGION_SCALE: u32 = 1_000_000;

const TIMELINE_RATE_NUMERATOR_KEY: &str = "superi.project.timeline.default_rate_numerator";
const TIMELINE_RATE_DENOMINATOR_KEY: &str = "superi.project.timeline.default_rate_denominator";
const TIMELINE_TIMECODE_MODE_KEY: &str = "superi.project.timeline.timecode_mode";
const COLOR_MODE_KEY: &str = "superi.project.color.mode";
const COLOR_WORKING_SPACE_KEY: &str = "superi.project.color.working_space";
const COLOR_CONFIG_ID_KEY: &str = "superi.project.color.config_id";
const COLOR_CONFIG_FINGERPRINT_KEY: &str = "superi.project.color.config_fingerprint";
const AUDIO_SAMPLE_RATE_KEY: &str = "superi.project.audio.sample_rate_hz";
const AUDIO_OUTPUT_LAYOUT_KEY: &str = "superi.project.audio.output_layout";
const CACHE_MODE_KEY: &str = "superi.project.cache.mode";
const CACHE_MAX_BYTES_KEY: &str = "superi.project.cache.max_bytes";
const CACHE_MAX_FRAMES_KEY: &str = "superi.project.cache.max_frames";
const PROXY_MODE_KEY: &str = "superi.project.proxy.mode";
const PROXY_QUALITY_KEY: &str = "superi.project.proxy.quality";
const RENDER_EXTENT_MODE_KEY: &str = "superi.project.render.extent_mode";
const RENDER_WIDTH_KEY: &str = "superi.project.render.width";
const RENDER_HEIGHT_KEY: &str = "superi.project.render.height";
const WORKING_FOLDER_KEY: &str = "superi.project.working.folder";
const WORKING_CACHE_FOLDER_KEY: &str = "superi.project.working.cache_folder";
const WORKING_PROXY_FOLDER_KEY: &str = "superi.project.working.proxy_folder";

/// Closed actionable failure classification used by the desktop project lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopProjectFailureClass {
    Retryable,
    Degraded,
    UserCorrectable,
    Terminal,
}

/// Reviewed actionable failure data retained beside the last valid project state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectFailure {
    class: DesktopProjectFailureClass,
    code: String,
    title: String,
    action: String,
    context: BTreeMap<String, String>,
}

impl DesktopProjectFailure {
    #[must_use]
    pub fn new(
        class: DesktopProjectFailureClass,
        code: impl Into<String>,
        title: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            class,
            code: code.into(),
            title: title.into(),
            action: action.into(),
            context: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub const fn class(&self) -> DesktopProjectFailureClass {
        self.class
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    #[must_use]
    pub fn context(&self, key: &str) -> Option<&str> {
        self.context.get(key).map(String::as_str)
    }
}

impl fmt::Display for DesktopProjectFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.title)
    }
}

impl std::error::Error for DesktopProjectFailure {}

/// Durable identity returned only after a backend operation succeeds completely.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectIdentity {
    project_id: String,
    project_revision: u64,
    root_timeline_id: String,
}

impl DesktopProjectIdentity {
    pub fn new(
        project_id: impl Into<String>,
        project_revision: u64,
        root_timeline_id: impl Into<String>,
    ) -> Result<Self, DesktopProjectFailure> {
        let identity = Self {
            project_id: project_id.into(),
            project_revision,
            root_timeline_id: root_timeline_id.into(),
        };
        if identity.project_id.trim().is_empty() || identity.root_timeline_id.trim().is_empty() {
            return Err(user_correctable(
                "project_identity_invalid",
                "Project identity is invalid",
                "Choose a valid Superi project and try again.",
            ));
        }
        Ok(identity)
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub fn root_timeline_id(&self) -> &str {
        &self.root_timeline_id
    }
}

/// Strict desktop request for creating one minimal durable project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectCreateRequest {
    pub project_id: String,
    pub project_name: String,
    pub root_timeline_id: String,
    pub root_timeline_name: String,
    pub edit_rate_numerator: u32,
    pub edit_rate_denominator: u32,
}

/// Desktop interaction that requested one authoritative media import.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopMediaImportOrigin {
    Picker,
    DragDrop,
    FolderScan,
    Api,
    Automation,
}

/// Strict discovery request shared by every desktop import entry point.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopMediaImportRequest {
    pub expected_project_revision: u64,
    pub origin: DesktopMediaImportOrigin,
    pub paths: Vec<String>,
    pub recursive: bool,
    pub detect_image_sequences: bool,
}

/// Editor-visible source classification returned after import.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopImportedMediaKind {
    File,
    ImageSequence,
}

/// One discovered source retained by the project transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopImportedMedia {
    media_id: String,
    name: String,
    source_paths: Vec<String>,
    content_fingerprint: String,
    kind: DesktopImportedMediaKind,
    source_count: u64,
    first_frame: Option<i64>,
    last_frame: Option<i64>,
    frame_rate_numerator: Option<u32>,
    frame_rate_denominator: Option<u32>,
    #[serde(skip, default)]
    source_fingerprints: Vec<MediaSourceFingerprint>,
}

impl DesktopImportedMedia {
    #[must_use]
    pub fn media_id(&self) -> &str {
        &self.media_id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn source_paths(&self) -> &[String] {
        &self.source_paths
    }

    #[must_use]
    pub fn content_fingerprint(&self) -> &str {
        &self.content_fingerprint
    }

    #[must_use]
    pub const fn kind(&self) -> DesktopImportedMediaKind {
        self.kind
    }

    #[must_use]
    pub const fn source_count(&self) -> u64 {
        self.source_count
    }

    #[must_use]
    pub const fn frame_range(&self) -> Option<(i64, i64)> {
        match (self.first_frame, self.last_frame) {
            (Some(first), Some(last)) => Some((first, last)),
            _ => None,
        }
    }

    #[must_use]
    pub const fn frame_rate(&self) -> Option<(u32, u32)> {
        match (self.frame_rate_numerator, self.frame_rate_denominator) {
            (Some(numerator), Some(denominator)) => Some((numerator, denominator)),
            _ => None,
        }
    }
}

/// Stable command, event, and automation evidence for one import attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopMediaImportResult {
    project_revision: u64,
    imported: Vec<DesktopImportedMedia>,
    skipped: Vec<String>,
    command_method: String,
    event_name: String,
    event_sequence: Option<u64>,
    automation_method: Option<String>,
}

impl DesktopMediaImportResult {
    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub fn imported(&self) -> &[DesktopImportedMedia] {
        &self.imported
    }

    #[must_use]
    pub fn skipped(&self) -> &[String] {
        &self.skipped
    }

    #[must_use]
    pub fn command_method(&self) -> &str {
        &self.command_method
    }

    #[must_use]
    pub fn event_name(&self) -> &str {
        &self.event_name
    }

    #[must_use]
    pub const fn event_sequence(&self) -> Option<u64> {
        self.event_sequence
    }

    #[must_use]
    pub fn automation_method(&self) -> Option<&str> {
        self.automation_method.as_deref()
    }
}

/// One replaceable thumbnail presentation derived from source identity and freshness.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ThumbnailPresentation {
    Source {
        source_path: String,
        freshness: String,
    },
    ThumbnailFallback {
        thumbnail_fallback: String,
        freshness: String,
    },
}

/// Current read-only inspection state for source-derived metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceMetadataStatus {
    Ready,
    Missing,
    Unavailable,
}

/// Bounded source facts tied to one exact imported-media freshness value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMetadataInspection {
    status: SourceMetadataStatus,
    inspection_generation: u64,
    freshness: String,
    fields: BTreeMap<String, String>,
}

/// Typed user-authored editorial annotations kept separate from source and generic metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaEditorialAnnotations {
    clip_name: Option<String>,
    labels: Vec<String>,
    rating: Option<u8>,
    keywords: Vec<String>,
    comment: Option<String>,
    favorite: bool,
}

/// Read-only usage projected from current authoritative timeline clip references.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaUsageIndicator {
    clip_count: u64,
    timeline_count: u64,
    timeline_ids: Vec<String>,
}

/// Read-only exact-content identity projected from the current media library.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaIdentityTracking {
    canonical_media_id: String,
    content_fingerprint: String,
    duplicate_media_ids: Vec<String>,
}

/// One exact-time fixed-point observation authored for a tracked region.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackedRegionObservation {
    frame: i64,
    x_millionths: u32,
    y_millionths: u32,
    width_millionths: u32,
    height_millionths: u32,
}

/// One stable manually refinable tracked region within a reusable selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaTrackedRegion {
    region_id: String,
    observations: Vec<TrackedRegionObservation>,
}

/// Ordinary editable source-time selection retained beside C006 annotations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaSelection {
    selection_id: String,
    name: String,
    start_frame: i64,
    end_frame: i64,
    rate_numerator: u32,
    rate_denominator: u32,
    tracked_regions: Vec<MediaTrackedRegion>,
}

/// Stable relationship from one language segment to an ordinary timeline clip.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaTimelineRelationship {
    timeline_id: String,
    clip_id: Option<String>,
}

/// One editable exact-time language-analysis artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaTranscriptSegment {
    segment_id: String,
    text: String,
    start_frame: i64,
    end_frame: i64,
    rate_numerator: u32,
    rate_denominator: u32,
    speaker: Option<String>,
    timeline_relationships: Vec<MediaTimelineRelationship>,
}

/// One ordinary editable entry produced by a bounded local content-analysis tool.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaLocalAiContent {
    content_id: String,
    label: String,
    terms: Vec<String>,
    segment_ids: Vec<String>,
}

/// Persisted model-independent language and local content artifacts for one source identity.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaContentAnalysis {
    source_fingerprint: String,
    provenance: Option<String>,
    transcript_segments: Vec<MediaTranscriptSegment>,
    local_ai_content: Vec<MediaLocalAiContent>,
}

/// Replaceable derived-media role; the imported source remains authoritative.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivedMediaPurpose {
    Proxy,
    Optimized,
}

/// Explicit derived-media quality ordered from lowest to highest fidelity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivedMediaQuality {
    Eighth,
    Quarter,
    Half,
    Full,
}

/// Inspectable lifecycle state for one replaceable derived artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivedMediaStatus {
    Generating,
    Ready,
    Stale,
    Failed,
}

/// User intent for transparent media representation selection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaRepresentationChoice {
    #[default]
    Original,
    Proxy {
        quality: DerivedMediaQuality,
    },
    Optimized {
        quality: DerivedMediaQuality,
    },
}

/// One replaceable attachment bound to exact authoritative source freshness.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedMediaAttachment {
    artifact_id: String,
    purpose: DerivedMediaPurpose,
    quality: DerivedMediaQuality,
    status: DerivedMediaStatus,
    source_fingerprint: String,
    source_revision: u64,
    byte_len: u64,
}

/// Deterministic selection evidence exposed without changing source identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedMediaRepresentation {
    representation: String,
    quality: Option<DerivedMediaQuality>,
    artifact_id: Option<String>,
    fallback_to_original: bool,
}

/// Fresh local availability for the authoritative source paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflineMediaStatus {
    Online,
    Partial,
    Offline,
}

/// Derived local-only source availability; never replaces stable media identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfflineMediaState {
    status: OfflineMediaStatus,
    available_paths: Vec<String>,
    missing_paths: Vec<String>,
    derived_fallback_available: bool,
}

fn unavailable_offline_state() -> OfflineMediaState {
    OfflineMediaState {
        status: OfflineMediaStatus::Offline,
        available_paths: Vec::new(),
        missing_paths: Vec::new(),
        derived_fallback_available: false,
    }
}

fn original_representation() -> ResolvedMediaRepresentation {
    ResolvedMediaRepresentation {
        representation: "original".to_owned(),
        quality: None,
        artifact_id: None,
        fallback_to_original: false,
    }
}

fn untracked_media_identity() -> MediaIdentityTracking {
    MediaIdentityTracking::default()
}

fn unused_media() -> MediaUsageIndicator {
    MediaUsageIndicator::default()
}

fn unavailable_source_metadata() -> SourceMetadataInspection {
    SourceMetadataInspection {
        status: SourceMetadataStatus::Unavailable,
        inspection_generation: 0,
        freshness: "unavailable".to_owned(),
        fields: BTreeMap::new(),
    }
}

fn source_metadata_inspection(
    media: &DesktopImportedMedia,
    inspection_generation: u64,
) -> SourceMetadataInspection {
    let mut fields = BTreeMap::new();
    fields.insert(
        "content_fingerprint".to_owned(),
        media.content_fingerprint.clone(),
    );
    fields.insert("source_count".to_owned(), media.source_count.to_string());
    fields.insert(
        "media_kind".to_owned(),
        match media.kind {
            DesktopImportedMediaKind::File => "file",
            DesktopImportedMediaKind::ImageSequence => "image_sequence",
        }
        .to_owned(),
    );
    if let (Some(first), Some(last)) = (media.first_frame, media.last_frame) {
        fields.insert("frame_range".to_owned(), format!("{first}-{last}"));
    }
    if let (Some(numerator), Some(denominator)) =
        (media.frame_rate_numerator, media.frame_rate_denominator)
    {
        fields.insert(
            "frame_rate".to_owned(),
            format!("{numerator}/{denominator}"),
        );
    }

    let status = match media.source_paths.first() {
        None => SourceMetadataStatus::Unavailable,
        Some(path) => {
            if let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str()) {
                fields.insert("extension".to_owned(), extension.to_ascii_lowercase());
            }
            match std::fs::metadata(path) {
                Ok(metadata) => {
                    fields.insert("file_size_bytes".to_owned(), metadata.len().to_string());
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(age) = modified.duration_since(std::time::UNIX_EPOCH) {
                            fields.insert(
                                "modified_unix_seconds".to_owned(),
                                age.as_secs().to_string(),
                            );
                        }
                    }
                    SourceMetadataStatus::Ready
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    SourceMetadataStatus::Missing
                }
                Err(_) => SourceMetadataStatus::Unavailable,
            }
        }
    };

    SourceMetadataInspection {
        status,
        inspection_generation: inspection_generation.max(1),
        freshness: media.content_fingerprint.clone(),
        fields,
    }
}

fn thumbnail_fallback(kind: DesktopImportedMediaKind, freshness: &str) -> ThumbnailPresentation {
    let kind = match kind {
        DesktopImportedMediaKind::File => "file",
        DesktopImportedMediaKind::ImageSequence => "sequence",
    };
    ThumbnailPresentation::ThumbnailFallback {
        thumbnail_fallback: format!(
            "{kind}:{}",
            &freshness[freshness.len().saturating_sub(12)..]
        ),
        freshness: freshness.to_owned(),
    }
}

fn unavailable_thumbnail() -> ThumbnailPresentation {
    ThumbnailPresentation::ThumbnailFallback {
        thumbnail_fallback: "media:unavailable".to_owned(),
        freshness: "unavailable".to_owned(),
    }
}

fn thumbnail_presentation(media: &DesktopImportedMedia) -> ThumbnailPresentation {
    let source = media.source_paths.first();
    let is_still = source
        .and_then(|path| Path::new(path).extension())
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "tif" | "tiff"
            )
        })
        .unwrap_or(false);
    if media.kind == DesktopImportedMediaKind::File && is_still {
        ThumbnailPresentation::Source {
            source_path: source.expect("still path checked").clone(),
            freshness: media.content_fingerprint.clone(),
        }
    } else {
        thumbnail_fallback(media.kind, &media.content_fingerprint)
    }
}

/// One imported source projected into every media-browser view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaBrowserItem {
    media_id: String,
    name: String,
    source_paths: Vec<String>,
    content_fingerprint: String,
    kind: DesktopImportedMediaKind,
    source_count: u64,
    first_frame: Option<i64>,
    last_frame: Option<i64>,
    frame_rate_numerator: Option<u32>,
    frame_rate_denominator: Option<u32>,
    bin_id: Option<String>,
    metadata: BTreeMap<String, String>,
    #[serde(default = "unavailable_source_metadata")]
    source_metadata: SourceMetadataInspection,
    #[serde(default)]
    user_metadata: BTreeMap<String, String>,
    #[serde(default)]
    annotations: MediaEditorialAnnotations,
    #[serde(default)]
    content_analysis: MediaContentAnalysis,
    #[serde(default)]
    source_monitoring: MediaSourceMonitoring,
    #[serde(default = "unused_media")]
    usage: MediaUsageIndicator,
    #[serde(default = "untracked_media_identity")]
    identity_tracking: MediaIdentityTracking,
    #[serde(default)]
    selections: Vec<MediaSelection>,
    #[serde(default)]
    derived_media: Vec<DerivedMediaAttachment>,
    #[serde(default)]
    representation_choice: MediaRepresentationChoice,
    #[serde(default = "original_representation")]
    resolved_representation: ResolvedMediaRepresentation,
    #[serde(default = "unavailable_offline_state")]
    offline: OfflineMediaState,
    #[serde(default = "unavailable_thumbnail")]
    thumbnail: ThumbnailPresentation,
    #[serde(default)]
    source_monitor_marks: SourceMonitorMarks,
}

impl MediaBrowserItem {
    fn from_imported(media: &DesktopImportedMedia) -> Self {
        let mut metadata = BTreeMap::new();
        metadata.insert("media_id".to_owned(), media.media_id.clone());
        metadata.insert("source_count".to_owned(), media.source_count.to_string());
        metadata.insert("freshness".to_owned(), media.content_fingerprint.clone());
        if let Some(first) = media.first_frame {
            metadata.insert("first_frame".to_owned(), first.to_string());
        }
        if let Some(last) = media.last_frame {
            metadata.insert("last_frame".to_owned(), last.to_string());
        }
        if let (Some(numerator), Some(denominator)) =
            (media.frame_rate_numerator, media.frame_rate_denominator)
        {
            metadata.insert(
                "frame_rate".to_owned(),
                format!("{numerator}/{denominator}"),
            );
        }
        Self {
            media_id: media.media_id.clone(),
            name: media.name.clone(),
            source_paths: media.source_paths.clone(),
            content_fingerprint: media.content_fingerprint.clone(),
            kind: media.kind,
            source_count: media.source_count,
            first_frame: media.first_frame,
            last_frame: media.last_frame,
            frame_rate_numerator: media.frame_rate_numerator,
            frame_rate_denominator: media.frame_rate_denominator,
            bin_id: None,
            metadata,
            source_metadata: source_metadata_inspection(media, 1),
            user_metadata: BTreeMap::new(),
            annotations: MediaEditorialAnnotations::default(),
            content_analysis: MediaContentAnalysis::default(),
            source_monitoring: MediaSourceMonitoring::from_imported(media),
            usage: unused_media(),
            identity_tracking: untracked_media_identity(),
            selections: Vec::new(),
            derived_media: Vec::new(),
            representation_choice: MediaRepresentationChoice::Original,
            resolved_representation: original_representation(),
            offline: unavailable_offline_state(),
            thumbnail: thumbnail_presentation(media),
            source_monitor_marks: SourceMonitorMarks::default(),
        }
    }

    fn refresh_thumbnail(&mut self) {
        let media = DesktopImportedMedia {
            media_id: self.media_id.clone(),
            name: self.name.clone(),
            source_paths: self.source_paths.clone(),
            content_fingerprint: self.content_fingerprint.clone(),
            kind: self.kind,
            source_count: self.source_count,
            first_frame: self.first_frame,
            last_frame: self.last_frame,
            frame_rate_numerator: self.frame_rate_numerator,
            frame_rate_denominator: self.frame_rate_denominator,
            source_fingerprints: Vec::new(),
        };
        self.thumbnail = thumbnail_presentation(&media);
    }

    fn refresh_derived_media(&mut self) {
        self.source_monitoring
            .reconcile(&self.source_paths, &self.content_fingerprint);
        for attachment in &mut self.derived_media {
            if attachment.source_fingerprint != self.content_fingerprint
                && attachment.status == DerivedMediaStatus::Ready
            {
                attachment.status = DerivedMediaStatus::Stale;
            }
        }
        self.resolved_representation = resolve_media_representation(
            self.representation_choice,
            &self.content_fingerprint,
            &self.derived_media,
        );
        self.refresh_offline_state();
    }

    fn refresh_offline_state(&mut self) {
        let (available_paths, missing_paths): (Vec<_>, Vec<_>) = self
            .source_paths
            .iter()
            .cloned()
            .partition(|path| Path::new(path).is_file());
        let status = if missing_paths.is_empty() && !available_paths.is_empty() {
            OfflineMediaStatus::Online
        } else if available_paths.is_empty() {
            OfflineMediaStatus::Offline
        } else {
            OfflineMediaStatus::Partial
        };
        self.offline = OfflineMediaState {
            status,
            available_paths,
            missing_paths,
            derived_fallback_available: self.derived_media.iter().any(|attachment| {
                attachment.source_fingerprint == self.content_fingerprint
                    && attachment.status == DerivedMediaStatus::Ready
            }),
        };
    }
}

/// One stable manual bin or sub-bin.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaBinView {
    bin_id: String,
    name: String,
    parent_id: Option<String>,
}

/// One saved name predicate and its freshly derived stable membership.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmartCollectionView {
    collection_id: String,
    name: String,
    name_contains: String,
    #[serde(default)]
    media_ids: Vec<String>,
}

/// Complete authoritative media-browser replacement state for one active project identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaLibrarySnapshot {
    revision: u64,
    project_revision: u64,
    items: Vec<MediaBrowserItem>,
    bins: Vec<MediaBinView>,
    smart_collections: Vec<SmartCollectionView>,
}

impl MediaLibrarySnapshot {
    fn empty(project_revision: u64) -> Self {
        Self {
            revision: 0,
            project_revision,
            items: Vec::new(),
            bins: Vec::new(),
            smart_collections: Vec::new(),
        }
    }

    fn refresh_derived(&mut self) {
        for item in &mut self.items {
            item.refresh_thumbnail();
            item.refresh_derived_media();
        }
        for collection in &mut self.smart_collections {
            let needle = collection.name_contains.to_lowercase();
            collection.media_ids = self
                .items
                .iter()
                .filter(|item| item.name.to_lowercase().contains(&needle))
                .map(|item| item.media_id.clone())
                .collect();
        }
        self.refresh_identity_tracking();
    }

    fn refresh_identity_tracking(&mut self) {
        let mut groups = BTreeMap::<String, Vec<String>>::new();
        for item in &self.items {
            groups
                .entry(item.content_fingerprint.clone())
                .or_default()
                .push(item.media_id.clone());
        }
        for media_ids in groups.values_mut() {
            media_ids.sort();
            media_ids.dedup();
        }
        for item in &mut self.items {
            let media_ids = groups
                .get(&item.content_fingerprint)
                .cloned()
                .unwrap_or_else(|| vec![item.media_id.clone()]);
            item.identity_tracking = MediaIdentityTracking {
                canonical_media_id: media_ids
                    .first()
                    .cloned()
                    .unwrap_or_else(|| item.media_id.clone()),
                content_fingerprint: item.content_fingerprint.clone(),
                duplicate_media_ids: media_ids
                    .into_iter()
                    .filter(|media_id| media_id != &item.media_id)
                    .collect(),
            };
        }
    }

    fn refresh_usage(&mut self, usage: &BTreeMap<String, MediaUsageIndicator>) {
        for item in &mut self.items {
            item.usage = usage
                .get(&item.media_id)
                .cloned()
                .unwrap_or_else(unused_media);
        }
    }

    fn retain_import(&mut self, result: &DesktopMediaImportResult) {
        self.project_revision = result.project_revision;
        for imported in &result.imported {
            match self
                .items
                .binary_search_by(|item| item.media_id.cmp(&imported.media_id))
            {
                Ok(index) => {
                    let name = self.items[index].name.clone();
                    let bin_id = self.items[index].bin_id.clone();
                    let user_metadata = self.items[index].user_metadata.clone();
                    let annotations = self.items[index].annotations.clone();
                    let content_analysis = self.items[index].content_analysis.clone();
                    let selections = self.items[index].selections.clone();
                    let derived_media = self.items[index].derived_media.clone();
                    let representation_choice = self.items[index].representation_choice;
                    let source_monitor_marks = self.items[index].source_monitor_marks.clone();
                    let inspection_generation = self.items[index]
                        .source_metadata
                        .inspection_generation
                        .saturating_add(1);
                    self.items[index] = MediaBrowserItem::from_imported(imported);
                    self.items[index].name = name;
                    self.items[index].bin_id = bin_id;
                    self.items[index].user_metadata = user_metadata;
                    self.items[index].annotations = annotations;
                    self.items[index].content_analysis = content_analysis;
                    self.items[index].selections = selections;
                    self.items[index].derived_media = derived_media;
                    self.items[index].representation_choice = representation_choice;
                    self.items[index].source_monitor_marks = source_monitor_marks;
                    self.items[index].source_metadata =
                        source_metadata_inspection(imported, inspection_generation);
                }
                Err(index) => self
                    .items
                    .insert(index, MediaBrowserItem::from_imported(imported)),
            }
        }
        if !result.imported.is_empty() {
            self.revision = self.revision.saturating_add(1);
        }
        self.refresh_derived();
    }

    fn apply(&mut self, update: MediaLibraryUpdate) -> Result<(), DesktopProjectFailure> {
        if update.expected_project_revision != self.project_revision
            || update.expected_library_revision != self.revision
        {
            return Err(user_correctable(
                "media_library_revision_stale",
                "Media library changed",
                "Refresh the media library and try again.",
            ));
        }
        let mut candidate = self.clone();
        candidate.apply_mutation(update.mutation)?;
        candidate.validate()?;
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_revision_exhausted",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        candidate.refresh_derived();
        *self = candidate;
        Ok(())
    }

    fn inspect_source(
        &mut self,
        request: SourceMetadataInspectionRequest,
    ) -> Result<(), DesktopProjectFailure> {
        let item = self
            .items
            .iter_mut()
            .find(|item| item.media_id == request.media_id)
            .ok_or_else(|| media_library_invalid("Imported media was not found"))?;
        if item.content_fingerprint != request.expected_freshness {
            return Err(user_correctable(
                "source_metadata_stale",
                "Source changed before inspection",
                "Refresh the media library and inspect the source again.",
            ));
        }
        let generation = item
            .source_metadata
            .inspection_generation
            .checked_add(1)
            .ok_or_else(|| {
                DesktopProjectFailure::new(
                    DesktopProjectFailureClass::Terminal,
                    "source_metadata_generation_exhausted",
                    "Source metadata cannot continue",
                    "Restart Superi before inspecting this source again.",
                )
            })?;
        let media = DesktopImportedMedia {
            media_id: item.media_id.clone(),
            name: item.name.clone(),
            source_paths: item.source_paths.clone(),
            content_fingerprint: item.content_fingerprint.clone(),
            kind: item.kind,
            source_count: item.source_count,
            first_frame: item.first_frame,
            last_frame: item.last_frame,
            frame_rate_numerator: item.frame_rate_numerator,
            frame_rate_denominator: item.frame_rate_denominator,
            source_fingerprints: Vec::new(),
        };
        item.source_metadata = source_metadata_inspection(&media, generation);
        Ok(())
    }

    fn apply_user_metadata(
        &mut self,
        update: UserMetadataUpdate,
    ) -> Result<(), DesktopProjectFailure> {
        if update.expected_project_revision != self.project_revision
            || update.expected_library_revision != self.revision
        {
            return Err(user_correctable(
                "media_library_revision_stale",
                "Media library changed",
                "Refresh the media library and try again.",
            ));
        }
        let mut candidate = self.clone();
        let item = candidate
            .items
            .iter_mut()
            .find(|item| item.media_id == update.media_id)
            .ok_or_else(|| media_library_invalid("Imported media was not found"))?;
        normalize_user_metadata(item, update.mutation)?;
        candidate.validate()?;
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_revision_exhausted",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        candidate.refresh_derived();
        *self = candidate;
        Ok(())
    }

    fn apply_annotations(
        &mut self,
        update: MediaAnnotationUpdate,
    ) -> Result<(), DesktopProjectFailure> {
        if update.expected_project_revision != self.project_revision
            || update.expected_library_revision != self.revision
        {
            return Err(user_correctable(
                "media_library_revision_stale",
                "Media library changed",
                "Refresh the media library and try again.",
            ));
        }
        let annotations = normalize_annotations(update.annotations)?;
        let mut candidate = self.clone();
        let item = candidate
            .items
            .iter_mut()
            .find(|item| item.media_id == update.media_id)
            .ok_or_else(|| media_library_invalid("Imported media was not found"))?;
        item.annotations = annotations;
        candidate.validate()?;
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_revision_exhausted",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        candidate.refresh_derived();
        *self = candidate;
        Ok(())
    }

    fn apply_identity_update(
        &mut self,
        update: MediaIdentityUpdate,
    ) -> Result<(), DesktopProjectFailure> {
        if update.expected_project_revision != self.project_revision
            || update.expected_library_revision != self.revision
        {
            return Err(user_correctable(
                "media_library_revision_stale",
                "Media library changed",
                "Refresh the media library and try again.",
            ));
        }
        let selections = normalize_media_selections(update.selections)?;
        let mut candidate = self.clone();
        let item = candidate
            .items
            .iter_mut()
            .find(|item| item.media_id == update.media_id)
            .ok_or_else(|| media_library_invalid("Imported media was not found"))?;
        item.selections = selections;
        candidate.validate()?;
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_revision_exhausted",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        candidate.refresh_derived();
        *self = candidate;
        Ok(())
    }

    fn apply_content_analysis(
        &mut self,
        update: MediaContentAnalysisUpdate,
    ) -> Result<(), DesktopProjectFailure> {
        if update.expected_project_revision != self.project_revision
            || update.expected_library_revision != self.revision
        {
            return Err(user_correctable(
                "media_library_revision_stale",
                "Media library changed",
                "Refresh the media library and try again.",
            ));
        }
        let mut candidate = self.clone();
        let item = candidate
            .items
            .iter_mut()
            .find(|item| item.media_id == update.media_id)
            .ok_or_else(|| media_library_invalid("Imported media was not found"))?;
        let analysis = normalize_content_analysis(update.analysis)?;
        if update.expected_source_fingerprint != item.content_fingerprint
            || (!analysis.source_fingerprint.is_empty()
                && analysis.source_fingerprint != item.content_fingerprint)
        {
            return Err(user_correctable(
                "media_content_source_stale",
                "Source changed before content analysis was saved",
                "Refresh the media library and apply the analysis to the current source.",
            ));
        }
        item.content_analysis = analysis;
        candidate.validate()?;
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_revision_exhausted",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        candidate.refresh_derived();
        *self = candidate;
        Ok(())
    }

    fn search_content(
        &self,
        request: MediaContentSearchRequest,
    ) -> Result<MediaContentSearchSnapshot, DesktopProjectFailure> {
        if request.expected_project_revision != self.project_revision
            || request.expected_library_revision != self.revision
        {
            return Err(user_correctable(
                "media_content_search_revision_stale",
                "Media library changed during search",
                "Refresh the media library and search again.",
            ));
        }
        let (query, tokens) = normalize_content_search_query(request.query)?;
        let max_results = usize::from(request.max_results.unwrap_or(100));
        if max_results == 0 || max_results > MAX_CONTENT_SEARCH_RESULTS {
            return Err(media_library_invalid(
                "Content search result limit is invalid",
            ));
        }
        let mut results = self
            .items
            .iter()
            .filter_map(|item| search_media_content(item, &query, &tokens))
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.media_id.cmp(&right.media_id))
        });
        results.truncate(max_results);
        Ok(MediaContentSearchSnapshot {
            project_revision: self.project_revision,
            library_revision: self.revision,
            query,
            results,
        })
    }

    fn apply_derived_media(
        &mut self,
        update: DerivedMediaUpdate,
    ) -> Result<(), DesktopProjectFailure> {
        if update.expected_project_revision != self.project_revision
            || update.expected_library_revision != self.revision
        {
            return Err(user_correctable(
                "media_library_revision_stale",
                "Media library changed",
                "Refresh the media library and try again.",
            ));
        }
        let mut candidate = self.clone();
        let item = candidate
            .items
            .iter_mut()
            .find(|item| item.media_id == update.media_id)
            .ok_or_else(|| media_library_invalid("Imported media was not found"))?;
        normalize_derived_media(item, update.mutation)?;
        candidate.validate()?;
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_revision_exhausted",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        candidate.refresh_derived();
        *self = candidate;
        Ok(())
    }

    fn apply_offline_media(
        &mut self,
        update: OfflineMediaUpdate,
    ) -> Result<(), DesktopProjectFailure> {
        if update.expected_project_revision != self.project_revision
            || update.expected_library_revision != self.revision
        {
            return Err(user_correctable(
                "media_library_revision_stale",
                "Media library changed",
                "Refresh the media library and try again.",
            ));
        }
        let mut candidate = self.clone();
        let item = candidate
            .items
            .iter_mut()
            .find(|item| item.media_id == update.media_id)
            .ok_or_else(|| media_library_invalid("Imported media was not found"))?;
        normalize_offline_media(item, update.mutation)?;
        candidate.validate()?;
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_revision_exhausted",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        candidate.refresh_derived();
        *self = candidate;
        Ok(())
    }

    fn scan_media_sources(
        &mut self,
        request: MediaSourceScanRequest,
    ) -> Result<(), DesktopProjectFailure> {
        if request.expected_project_revision != self.project_revision
            || request.expected_library_revision != self.revision
        {
            return Err(user_correctable(
                "media_library_revision_stale",
                "Media library changed",
                "Refresh the media library and scan sources again.",
            ));
        }
        if request.media_ids.len() > MAX_IMPORT_SOURCES {
            return Err(media_library_invalid(
                "Source scan contains too many media IDs",
            ));
        }
        let media_ids = if request.media_ids.is_empty() {
            self.items
                .iter()
                .map(|item| item.media_id.clone())
                .collect::<Vec<_>>()
        } else {
            let unique = request.media_ids.iter().collect::<BTreeSet<_>>();
            if unique.len() != request.media_ids.len() {
                return Err(media_library_invalid(
                    "Source scan media IDs are duplicated",
                ));
            }
            request.media_ids
        };
        if media_ids.is_empty() {
            return Err(media_library_invalid("Source scan has no media to inspect"));
        }

        let mut candidate = self.clone();
        for media_id in media_ids {
            require_media_text("media_id", &media_id)?;
            let item = candidate
                .items
                .iter_mut()
                .find(|item| item.media_id == media_id)
                .ok_or_else(|| media_library_invalid("Source scan media was not found"))?;
            source_monitoring::scan_item(item, request.verify_content)?;
        }
        candidate.validate()?;
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_revision_exhausted",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        candidate.refresh_derived();
        *self = candidate;
        Ok(())
    }

    fn apply_media_batch(
        &mut self,
        update: MediaBatchUpdate,
    ) -> Result<Vec<String>, DesktopProjectFailure> {
        if update.expected_project_revision != self.project_revision
            || update.expected_library_revision != self.revision
        {
            return Err(user_correctable(
                "media_library_revision_stale",
                "Media library changed",
                "Refresh the media library and try again.",
            ));
        }
        if update.operations.is_empty() || update.operations.len() > MAX_MEDIA_BATCH_OPERATIONS {
            return Err(media_library_invalid(
                "Media batch is empty or exceeds the operation limit",
            ));
        }

        let mut candidate = self.clone();
        let mut affected_media_ids = Vec::new();
        let mut affected = BTreeSet::new();
        for (index, operation) in update.operations.into_iter().enumerate() {
            let media_id = operation.media_id().to_owned();
            candidate
                .apply_batch_operation(operation)
                .map_err(|failure| {
                    failure.with_context("batch_operation_index", index.to_string())
                })?;
            if affected.insert(media_id.clone()) {
                affected_media_ids.push(media_id);
            }
        }
        candidate.validate()?;
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_revision_exhausted",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        candidate.refresh_derived();
        *self = candidate;
        Ok(affected_media_ids)
    }

    fn apply_batch_operation(
        &mut self,
        operation: MediaBatchOperation,
    ) -> Result<(), DesktopProjectFailure> {
        match operation {
            MediaBatchOperation::Rename { media_id, name } => {
                validate_media_display_name(&name)?;
                self.media_item_mut(&media_id)?.name = name;
            }
            MediaBatchOperation::Organize { media_id, bin_id } => {
                self.apply_mutation(MediaLibraryMutation::MoveMedia { media_id, bin_id })?;
            }
            MediaBatchOperation::Transcode {
                media_id,
                artifact_id,
                quality,
                status,
                source_fingerprint,
                source_revision,
                byte_len,
                select,
            } => self.apply_batch_derived_media(
                media_id,
                DerivedMediaMutation::CreateOrReplace {
                    artifact_id,
                    purpose: DerivedMediaPurpose::Optimized,
                    quality,
                    status,
                    source_fingerprint,
                    source_revision,
                    byte_len,
                },
                select.then_some(MediaRepresentationChoice::Optimized { quality }),
            )?,
            MediaBatchOperation::Proxy {
                media_id,
                artifact_id,
                quality,
                status,
                source_fingerprint,
                source_revision,
                byte_len,
                select,
            } => self.apply_batch_derived_media(
                media_id,
                DerivedMediaMutation::CreateOrReplace {
                    artifact_id,
                    purpose: DerivedMediaPurpose::Proxy,
                    quality,
                    status,
                    source_fingerprint,
                    source_revision,
                    byte_len,
                },
                select.then_some(MediaRepresentationChoice::Proxy { quality }),
            )?,
            MediaBatchOperation::Relink {
                media_id,
                source_paths,
                candidate_fingerprint,
            } => normalize_offline_media(
                self.media_item_mut(&media_id)?,
                OfflineMediaMutation::Relink {
                    source_paths,
                    candidate_fingerprint,
                },
            )?,
            MediaBatchOperation::MetadataUpsert {
                media_id,
                key,
                value,
            } => normalize_user_metadata(
                self.media_item_mut(&media_id)?,
                UserMetadataMutation::Upsert { key, value },
            )?,
            MediaBatchOperation::MetadataRemove { media_id, key } => normalize_user_metadata(
                self.media_item_mut(&media_id)?,
                UserMetadataMutation::Remove { key },
            )?,
        }
        Ok(())
    }

    fn apply_batch_derived_media(
        &mut self,
        media_id: String,
        mutation: DerivedMediaMutation,
        choice: Option<MediaRepresentationChoice>,
    ) -> Result<(), DesktopProjectFailure> {
        let item = self.media_item_mut(&media_id)?;
        normalize_derived_media(item, mutation)?;
        if let Some(choice) = choice {
            normalize_derived_media(item, DerivedMediaMutation::SetChoice { choice })?;
        }
        Ok(())
    }

    fn media_item_mut(
        &mut self,
        media_id: &str,
    ) -> Result<&mut MediaBrowserItem, DesktopProjectFailure> {
        self.items
            .iter_mut()
            .find(|item| item.media_id == media_id)
            .ok_or_else(|| media_library_invalid("Imported media was not found"))
    }

    fn apply_mutation(
        &mut self,
        mutation: MediaLibraryMutation,
    ) -> Result<(), DesktopProjectFailure> {
        match mutation {
            MediaLibraryMutation::CreateBin {
                bin_id,
                name,
                parent_id,
            } => {
                require_media_text("bin_id", &bin_id)?;
                require_media_text("bin_name", &name)?;
                if self.bins.len() >= MAX_MEDIA_BINS {
                    return Err(media_library_invalid("Media bin limit reached"));
                }
                if self.bins.iter().any(|bin| bin.bin_id == bin_id) {
                    return Err(media_library_invalid("Media bin identity already exists"));
                }
                if let Some(parent) = &parent_id {
                    if !self.bins.iter().any(|bin| &bin.bin_id == parent) {
                        return Err(media_library_invalid("Parent media bin was not found"));
                    }
                }
                self.bins.push(MediaBinView {
                    bin_id,
                    name,
                    parent_id,
                });
                self.bins
                    .sort_by(|left, right| left.bin_id.cmp(&right.bin_id));
            }
            MediaLibraryMutation::MoveMedia { media_id, bin_id } => {
                if let Some(target) = &bin_id {
                    if !self.bins.iter().any(|bin| &bin.bin_id == target) {
                        return Err(media_library_invalid("Target media bin was not found"));
                    }
                }
                let item = self
                    .items
                    .iter_mut()
                    .find(|item| item.media_id == media_id)
                    .ok_or_else(|| media_library_invalid("Imported media was not found"))?;
                item.bin_id = bin_id;
            }
            MediaLibraryMutation::RemoveBin { bin_id } => {
                if self
                    .bins
                    .iter()
                    .any(|bin| bin.parent_id.as_deref() == Some(&bin_id))
                {
                    return Err(media_library_invalid("Remove child bins first"));
                }
                let before = self.bins.len();
                self.bins.retain(|bin| bin.bin_id != bin_id);
                if self.bins.len() == before {
                    return Err(media_library_invalid("Media bin was not found"));
                }
                for item in &mut self.items {
                    if item.bin_id.as_deref() == Some(&bin_id) {
                        item.bin_id = None;
                    }
                }
            }
            MediaLibraryMutation::UpsertSmartCollection {
                collection_id,
                name,
                name_contains,
            } => {
                require_media_text("collection_id", &collection_id)?;
                require_media_text("collection_name", &name)?;
                require_media_text("name_contains", &name_contains)?;
                if let Some(collection) = self
                    .smart_collections
                    .iter_mut()
                    .find(|collection| collection.collection_id == collection_id)
                {
                    collection.name = name;
                    collection.name_contains = name_contains;
                } else {
                    if self.smart_collections.len() >= MAX_SMART_COLLECTIONS {
                        return Err(media_library_invalid("Smart collection limit reached"));
                    }
                    self.smart_collections.push(SmartCollectionView {
                        collection_id,
                        name,
                        name_contains,
                        media_ids: Vec::new(),
                    });
                    self.smart_collections
                        .sort_by(|left, right| left.collection_id.cmp(&right.collection_id));
                }
            }
            MediaLibraryMutation::RemoveSmartCollection { collection_id } => {
                let before = self.smart_collections.len();
                self.smart_collections
                    .retain(|collection| collection.collection_id != collection_id);
                if self.smart_collections.len() == before {
                    return Err(media_library_invalid("Smart collection was not found"));
                }
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), DesktopProjectFailure> {
        for item in &self.items {
            if item.user_metadata.len() > MAX_USER_METADATA_ENTRIES {
                return Err(media_library_invalid("User metadata limit exceeded"));
            }
            for (key, value) in &item.user_metadata {
                validate_user_metadata_key(key)?;
                validate_user_metadata_value(value)?;
            }
            if normalize_annotations(item.annotations.clone())? != item.annotations {
                return Err(media_library_invalid(
                    "Editorial annotations are not canonical",
                ));
            }
            if normalize_media_selections(item.selections.clone())? != item.selections {
                return Err(media_library_invalid(
                    "Media selections and tracked regions are not canonical",
                ));
            }
            if normalize_content_analysis(item.content_analysis.clone())? != item.content_analysis {
                return Err(media_library_invalid(
                    "Media content analysis is not canonical",
                ));
            }
            source_monitoring::validate(
                &item.source_monitoring,
                &item.source_paths,
                &item.content_fingerprint,
            )?;
            source_monitor::validate_marks(&item.source_monitor_marks)?;
        }
        for bin in &self.bins {
            let mut current = bin.parent_id.as_deref();
            let mut seen = BTreeSet::from([bin.bin_id.as_str()]);
            while let Some(parent) = current {
                if !seen.insert(parent) {
                    return Err(media_library_invalid(
                        "Media bin hierarchy contains a cycle",
                    ));
                }
                current = self
                    .bins
                    .iter()
                    .find(|candidate| candidate.bin_id == parent)
                    .and_then(|candidate| candidate.parent_id.as_deref());
            }
        }
        Ok(())
    }
}

/// Every C004 media-library mutation. View mode and selection remain client presentation state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaLibraryMutation {
    CreateBin {
        bin_id: String,
        name: String,
        parent_id: Option<String>,
    },
    MoveMedia {
        media_id: String,
        bin_id: Option<String>,
    },
    RemoveBin {
        bin_id: String,
    },
    UpsertSmartCollection {
        collection_id: String,
        name: String,
        name_contains: String,
    },
    RemoveSmartCollection {
        collection_id: String,
    },
}

/// Revision-fenced update for the active project's authoritative media-browser snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaLibraryUpdate {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub mutation: MediaLibraryMutation,
}

/// Exact freshness-fenced request for one read-only source inspection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMetadataInspectionRequest {
    pub media_id: String,
    pub expected_freshness: String,
}

/// Generic user-authored metadata edits owned by C005.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UserMetadataMutation {
    Upsert { key: String, value: String },
    Remove { key: String },
}

/// Revision-fenced user metadata update for one stable imported-media identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserMetadataUpdate {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub media_id: String,
    pub mutation: UserMetadataMutation,
}

/// Atomic replacement of C006 annotations for one stable imported-media identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaAnnotationUpdate {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub media_id: String,
    pub annotations: MediaEditorialAnnotations,
}

/// Atomic C007 replacement for editable selections on one stable media identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaIdentityUpdate {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub media_id: String,
    pub selections: Vec<MediaSelection>,
}

/// Atomic replacement of ordinary editable content-analysis artifacts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaContentAnalysisUpdate {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub media_id: String,
    pub expected_source_fingerprint: String,
    pub analysis: MediaContentAnalysis,
}

/// Explicit signal group responsible for one content-search match.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaContentSearchSignal {
    Metadata,
    Transcript,
    LocalAi,
}

/// One concise piece of explainable evidence for a content-search result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaContentSearchMatch {
    signal: MediaContentSearchSignal,
    evidence: String,
    segment_id: Option<String>,
    start_frame: Option<i64>,
    end_frame: Option<i64>,
    rate_numerator: Option<u32>,
    rate_denominator: Option<u32>,
    speaker: Option<String>,
    timeline_relationships: Vec<MediaTimelineRelationship>,
}

/// One deterministically ranked media match.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaContentSearchResult {
    media_id: String,
    name: String,
    score: u64,
    analysis_fresh: bool,
    matches: Vec<MediaContentSearchMatch>,
}

/// Revision-fenced read-only content-search request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaContentSearchRequest {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub query: String,
    pub max_results: Option<u16>,
}

/// Complete native query response tied to one authoritative media-library revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaContentSearchSnapshot {
    project_revision: u64,
    library_revision: u64,
    query: String,
    results: Vec<MediaContentSearchResult>,
}

/// One revision-fenced derived-media lifecycle mutation for a stable C007 identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DerivedMediaMutation {
    CreateOrReplace {
        artifact_id: String,
        purpose: DerivedMediaPurpose,
        quality: DerivedMediaQuality,
        status: DerivedMediaStatus,
        source_fingerprint: String,
        source_revision: u64,
        byte_len: u64,
    },
    SetChoice {
        choice: MediaRepresentationChoice,
    },
    Remove {
        purpose: DerivedMediaPurpose,
        quality: DerivedMediaQuality,
    },
}

/// Atomic C008 update that never mutates source, annotations, or tracked selections.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedMediaUpdate {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub media_id: String,
    pub mutation: DerivedMediaMutation,
}

/// Local-only recovery operations for one stable imported-media reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OfflineMediaMutation {
    Relink {
        source_paths: Vec<String>,
        candidate_fingerprint: String,
    },
    Replace {
        source_paths: Vec<String>,
        replacement_fingerprint: String,
    },
    Conform {
        frame_rate_numerator: u32,
        frame_rate_denominator: u32,
    },
}

/// Revision-fenced offline-media mutation for one media identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfflineMediaUpdate {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub media_id: String,
    pub mutation: OfflineMediaMutation,
}

/// One ordered C012 operation against a stable imported-media identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MediaBatchOperation {
    Rename {
        media_id: String,
        name: String,
    },
    Organize {
        media_id: String,
        bin_id: Option<String>,
    },
    Transcode {
        media_id: String,
        artifact_id: String,
        quality: DerivedMediaQuality,
        status: DerivedMediaStatus,
        source_fingerprint: String,
        source_revision: u64,
        byte_len: u64,
        select: bool,
    },
    Proxy {
        media_id: String,
        artifact_id: String,
        quality: DerivedMediaQuality,
        status: DerivedMediaStatus,
        source_fingerprint: String,
        source_revision: u64,
        byte_len: u64,
        select: bool,
    },
    Relink {
        media_id: String,
        source_paths: Vec<String>,
        candidate_fingerprint: String,
    },
    MetadataUpsert {
        media_id: String,
        key: String,
        value: String,
    },
    MetadataRemove {
        media_id: String,
        key: String,
    },
}

impl MediaBatchOperation {
    fn media_id(&self) -> &str {
        match self {
            Self::Rename { media_id, .. }
            | Self::Organize { media_id, .. }
            | Self::Transcode { media_id, .. }
            | Self::Proxy { media_id, .. }
            | Self::Relink { media_id, .. }
            | Self::MetadataUpsert { media_id, .. }
            | Self::MetadataRemove { media_id, .. } => media_id,
        }
    }
}

/// One bounded revision-fenced C012 batch executed as a single replacement transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaBatchUpdate {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub operations: Vec<MediaBatchOperation>,
}

/// Deterministic evidence and replacement state from one committed media batch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaBatchResult {
    operation_count: usize,
    affected_media_ids: Vec<String>,
    snapshot: MediaLibrarySnapshot,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaLibraryStore {
    projects: BTreeMap<String, MediaLibrarySnapshot>,
}

fn require_media_text(field: &'static str, value: &str) -> Result<(), DesktopProjectFailure> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(
            media_library_invalid("Media library text is invalid").with_context("field", field)
        );
    }
    Ok(())
}

fn validate_media_display_name(name: &str) -> Result<(), DesktopProjectFailure> {
    require_media_text("media_name", name)?;
    if name.trim() != name || name.len() > MAX_MEDIA_DISPLAY_NAME_BYTES {
        return Err(media_library_invalid("Media name is invalid"));
    }
    Ok(())
}

fn media_library_invalid(title: &'static str) -> DesktopProjectFailure {
    user_correctable(
        "media_library_invalid",
        title,
        "Refresh the media library and try again.",
    )
}

fn validate_user_metadata_key(key: &str) -> Result<(), DesktopProjectFailure> {
    let valid = !key.is_empty()
        && key.len() <= MAX_USER_METADATA_KEY_BYTES
        && key.trim() == key
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'));
    if !valid {
        return Err(media_library_invalid("User metadata key is invalid"));
    }
    let reserved = matches!(
        key.to_ascii_lowercase().as_str(),
        "name"
            | "label"
            | "labels"
            | "rating"
            | "ratings"
            | "keyword"
            | "keywords"
            | "comment"
            | "comments"
            | "favorite"
            | "favorites"
            | "usage"
    );
    if reserved {
        return Err(media_library_invalid(
            "This metadata key is owned by a later editorial annotation checkpoint",
        ));
    }
    Ok(())
}

fn validate_user_metadata_value(value: &str) -> Result<(), DesktopProjectFailure> {
    if value.len() > MAX_USER_METADATA_VALUE_BYTES || value.chars().any(char::is_control) {
        return Err(media_library_invalid("User metadata value is invalid"));
    }
    Ok(())
}

fn normalize_user_metadata(
    item: &mut MediaBrowserItem,
    mutation: UserMetadataMutation,
) -> Result<(), DesktopProjectFailure> {
    match mutation {
        UserMetadataMutation::Upsert { key, value } => {
            validate_user_metadata_key(&key)?;
            validate_user_metadata_value(&value)?;
            if !item.user_metadata.contains_key(&key)
                && item.user_metadata.len() >= MAX_USER_METADATA_ENTRIES
            {
                return Err(media_library_invalid("User metadata limit reached"));
            }
            item.user_metadata.insert(key, value);
        }
        UserMetadataMutation::Remove { key } => {
            validate_user_metadata_key(&key)?;
            if item.user_metadata.remove(&key).is_none() {
                return Err(media_library_invalid("User metadata key was not found"));
            }
        }
    }
    Ok(())
}

fn normalize_annotations(
    annotations: MediaEditorialAnnotations,
) -> Result<MediaEditorialAnnotations, DesktopProjectFailure> {
    let normalized = MediaEditorialAnnotations {
        clip_name: normalize_annotation_text(
            annotations.clip_name,
            MAX_ANNOTATION_NAME_BYTES,
            "Clip name",
        )?,
        labels: normalize_annotation_terms(
            annotations.labels,
            MAX_ANNOTATION_LABELS,
            MAX_ANNOTATION_LABEL_BYTES,
            "Label",
        )?,
        rating: match annotations.rating {
            Some(rating @ 1..=5) => Some(rating),
            None => None,
            Some(_) => return Err(media_library_invalid("Rating must be between 1 and 5")),
        },
        keywords: normalize_annotation_terms(
            annotations.keywords,
            MAX_ANNOTATION_KEYWORDS,
            MAX_ANNOTATION_KEYWORD_BYTES,
            "Keyword",
        )?,
        comment: normalize_annotation_text(
            annotations.comment,
            MAX_ANNOTATION_COMMENT_BYTES,
            "Comment",
        )?,
        favorite: annotations.favorite,
    };
    let bytes = serde_json::to_vec(&normalized)
        .map_err(|_| media_library_invalid("Editorial annotations could not be encoded"))?;
    if bytes.len() > MAX_ANNOTATION_PAYLOAD_BYTES {
        return Err(media_library_invalid(
            "Editorial annotation payload is too large",
        ));
    }
    Ok(normalized)
}

fn normalize_media_selections(
    mut selections: Vec<MediaSelection>,
) -> Result<Vec<MediaSelection>, DesktopProjectFailure> {
    if selections.len() > MAX_MEDIA_SELECTIONS {
        return Err(media_library_invalid("Media selection limit reached"));
    }
    selections.sort_by(|left, right| left.selection_id.cmp(&right.selection_id));
    let mut selection_ids = BTreeSet::new();
    for selection in &mut selections {
        require_media_text("selection_id", &selection.selection_id)?;
        require_media_text("selection_name", &selection.name)?;
        if selection.name.len() > MAX_SELECTION_NAME_BYTES
            || !selection_ids.insert(selection.selection_id.as_str())
            || selection.start_frame >= selection.end_frame
            || selection.rate_numerator == 0
            || selection.rate_denominator == 0
            || selection.tracked_regions.len() > MAX_TRACKED_REGIONS
        {
            return Err(media_library_invalid("Media selection is invalid"));
        }
        selection
            .tracked_regions
            .sort_by(|left, right| left.region_id.cmp(&right.region_id));
        let mut region_ids = BTreeSet::new();
        for region in &mut selection.tracked_regions {
            require_media_text("region_id", &region.region_id)?;
            if !region_ids.insert(region.region_id.as_str())
                || region.observations.is_empty()
                || region.observations.len() > MAX_TRACKED_OBSERVATIONS
            {
                return Err(media_library_invalid("Tracked region is invalid"));
            }
            region
                .observations
                .sort_by_key(|observation| observation.frame);
            let mut prior_frame = None;
            for observation in &region.observations {
                let right = observation
                    .x_millionths
                    .checked_add(observation.width_millionths);
                let bottom = observation
                    .y_millionths
                    .checked_add(observation.height_millionths);
                if observation.frame < selection.start_frame
                    || observation.frame >= selection.end_frame
                    || prior_frame == Some(observation.frame)
                    || observation.width_millionths == 0
                    || observation.height_millionths == 0
                    || right.is_none_or(|value| value > NORMALIZED_REGION_SCALE)
                    || bottom.is_none_or(|value| value > NORMALIZED_REGION_SCALE)
                {
                    return Err(media_library_invalid("Tracked observation is invalid"));
                }
                prior_frame = Some(observation.frame);
            }
        }
    }
    let bytes = serde_json::to_vec(&selections)
        .map_err(|_| media_library_invalid("Media identity state could not be encoded"))?;
    if bytes.len() > MAX_IDENTITY_PAYLOAD_BYTES {
        return Err(media_library_invalid("Media identity state is too large"));
    }
    Ok(selections)
}

fn normalize_content_analysis(
    mut analysis: MediaContentAnalysis,
) -> Result<MediaContentAnalysis, DesktopProjectFailure> {
    if analysis.transcript_segments.len() > MAX_TRANSCRIPT_SEGMENTS {
        return Err(media_library_invalid("Transcript segment limit reached"));
    }
    if analysis.local_ai_content.len() > MAX_LOCAL_AI_CONTENT {
        return Err(media_library_invalid("Local AI content limit reached"));
    }
    analysis.provenance = normalize_annotation_text(
        analysis.provenance,
        MAX_CONTENT_PROVENANCE_BYTES,
        "Content analysis provenance is invalid",
    )?;
    if analysis.provenance.is_none()
        && analysis.transcript_segments.is_empty()
        && analysis.local_ai_content.is_empty()
    {
        return Ok(MediaContentAnalysis::default());
    }
    if analysis.source_fingerprint.is_empty() {
        return Err(media_library_invalid(
            "Content analysis must identify its source",
        ));
    }
    analysis.source_fingerprint =
        normalize_content_identifier(analysis.source_fingerprint, "content_source_fingerprint")?;

    analysis.transcript_segments.sort_by(|left, right| {
        left.start_frame
            .cmp(&right.start_frame)
            .then_with(|| left.end_frame.cmp(&right.end_frame))
            .then_with(|| left.segment_id.cmp(&right.segment_id))
    });
    let mut segment_ids = BTreeSet::new();
    for segment in &mut analysis.transcript_segments {
        segment.segment_id = normalize_content_identifier(
            std::mem::take(&mut segment.segment_id),
            "transcript_segment_id",
        )?;
        if !segment_ids.insert(segment.segment_id.clone())
            || segment.start_frame >= segment.end_frame
            || segment.rate_numerator == 0
            || segment.rate_denominator == 0
            || segment.timeline_relationships.len() > MAX_TIMELINE_RELATIONSHIPS
        {
            return Err(media_library_invalid("Transcript segment is invalid"));
        }
        segment.text = normalize_required_content_text(
            std::mem::take(&mut segment.text),
            MAX_TRANSCRIPT_TEXT_BYTES,
            "Transcript text is invalid",
        )?;
        segment.speaker = normalize_annotation_text(
            segment.speaker.take(),
            MAX_TRANSCRIPT_SPEAKER_BYTES,
            "Transcript speaker is invalid",
        )?;
        for relationship in &mut segment.timeline_relationships {
            relationship.timeline_id = normalize_content_identifier(
                std::mem::take(&mut relationship.timeline_id),
                "timeline_relationship_id",
            )?;
            relationship.clip_id = relationship
                .clip_id
                .take()
                .map(|clip_id| normalize_content_identifier(clip_id, "timeline_clip_id"))
                .transpose()?;
        }
        segment.timeline_relationships.sort();
        if segment
            .timeline_relationships
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(media_library_invalid(
                "Transcript timeline relationships must be unique",
            ));
        }
    }
    analysis.transcript_segments.sort_by(|left, right| {
        left.start_frame
            .cmp(&right.start_frame)
            .then_with(|| left.end_frame.cmp(&right.end_frame))
            .then_with(|| left.segment_id.cmp(&right.segment_id))
    });

    analysis
        .local_ai_content
        .sort_by(|left, right| left.content_id.cmp(&right.content_id));
    let mut content_ids = BTreeSet::new();
    for content in &mut analysis.local_ai_content {
        content.content_id = normalize_content_identifier(
            std::mem::take(&mut content.content_id),
            "local_ai_content_id",
        )?;
        if !content_ids.insert(content.content_id.clone())
            || content.segment_ids.len() > MAX_LOCAL_AI_SEGMENT_LINKS
        {
            return Err(media_library_invalid("Local AI content is invalid"));
        }
        content.label = normalize_required_content_text(
            std::mem::take(&mut content.label),
            MAX_LOCAL_AI_TERM_BYTES,
            "Local AI content label is invalid",
        )?;
        content.terms = normalize_annotation_terms(
            std::mem::take(&mut content.terms),
            MAX_LOCAL_AI_TERMS,
            MAX_LOCAL_AI_TERM_BYTES,
            "Local AI content term is invalid",
        )?;
        content.terms.sort_by_key(|term| term.to_lowercase());
        for segment_id in &mut content.segment_ids {
            *segment_id =
                normalize_content_identifier(std::mem::take(segment_id), "local_ai_segment_id")?;
        }
        content.segment_ids.sort();
        if content
            .segment_ids
            .windows(2)
            .any(|pair| pair[0] == pair[1])
            || content
                .segment_ids
                .iter()
                .any(|segment_id| !segment_ids.contains(segment_id))
        {
            return Err(media_library_invalid(
                "Local AI content references an invalid transcript segment",
            ));
        }
    }
    analysis
        .local_ai_content
        .sort_by(|left, right| left.content_id.cmp(&right.content_id));

    let bytes = serde_json::to_vec(&analysis)
        .map_err(|_| media_library_invalid("Content analysis could not be encoded"))?;
    if bytes.len() > MAX_CONTENT_ANALYSIS_PAYLOAD_BYTES {
        return Err(media_library_invalid("Content analysis is too large"));
    }
    Ok(analysis)
}

fn normalize_content_identifier(
    value: String,
    field: &'static str,
) -> Result<String, DesktopProjectFailure> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_CONTENT_IDENTIFIER_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(media_library_invalid(field));
    }
    Ok(value.to_owned())
}

fn normalize_required_content_text(
    value: String,
    max_bytes: usize,
    field: &'static str,
) -> Result<String, DesktopProjectFailure> {
    normalize_annotation_text(Some(value), max_bytes, field)?
        .ok_or_else(|| media_library_invalid(field))
}

#[derive(Clone)]
struct ContentSearchField {
    searchable: String,
    weight: u64,
    evidence: MediaContentSearchMatch,
}

fn normalize_content_search_query(
    query: String,
) -> Result<(String, Vec<String>), DesktopProjectFailure> {
    let query = query.trim();
    if query.is_empty()
        || query.len() > MAX_CONTENT_SEARCH_QUERY_BYTES
        || query.chars().any(char::is_control)
    {
        return Err(media_library_invalid("Content search query is invalid"));
    }
    let mut tokens = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    tokens.sort();
    tokens.dedup();
    if tokens.is_empty() || tokens.len() > MAX_CONTENT_SEARCH_TOKENS {
        return Err(media_library_invalid("Content search query is invalid"));
    }
    Ok((query.to_owned(), tokens))
}

fn search_media_content(
    item: &MediaBrowserItem,
    query: &str,
    tokens: &[String],
) -> Option<MediaContentSearchResult> {
    let mut fields = Vec::new();
    push_content_search_field(
        &mut fields,
        MediaContentSearchSignal::Metadata,
        20,
        format!("Name: {}", item.name),
        None,
    );
    push_content_search_field(
        &mut fields,
        MediaContentSearchSignal::Metadata,
        18,
        format!("Media identity: {}", item.media_id),
        None,
    );
    push_content_search_field(
        &mut fields,
        MediaContentSearchSignal::Metadata,
        14,
        format!("Content fingerprint: {}", item.content_fingerprint),
        None,
    );
    if let Some(bin_id) = &item.bin_id {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            12,
            format!("Bin: {bin_id}"),
            None,
        );
    }
    for source_path in &item.source_paths {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            14,
            format!("Source: {source_path}"),
            None,
        );
    }
    let offline_status = match item.offline.status {
        OfflineMediaStatus::Online => "online",
        OfflineMediaStatus::Partial => "partial",
        OfflineMediaStatus::Offline => "offline",
    };
    push_content_search_field(
        &mut fields,
        MediaContentSearchSignal::Metadata,
        10,
        format!("Availability: {offline_status}"),
        None,
    );
    for (key, value) in &item.metadata {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            16,
            format!("Metadata {key}: {value}"),
            None,
        );
    }
    for (key, value) in &item.source_metadata.fields {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            16,
            format!("Source metadata {key}: {value}"),
            None,
        );
    }
    for (key, value) in &item.user_metadata {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            22,
            format!("User metadata {key}: {value}"),
            None,
        );
    }
    if let Some(clip_name) = &item.annotations.clip_name {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            24,
            format!("Clip name: {clip_name}"),
            None,
        );
    }
    for label in &item.annotations.labels {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            24,
            format!("Label: {label}"),
            None,
        );
    }
    for keyword in &item.annotations.keywords {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            24,
            format!("Keyword: {keyword}"),
            None,
        );
    }
    if let Some(comment) = &item.annotations.comment {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            18,
            format!("Comment: {comment}"),
            None,
        );
    }
    if let Some(rating) = item.annotations.rating {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            10,
            format!("Rating: {rating}"),
            None,
        );
    }
    if item.annotations.favorite {
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::Metadata,
            10,
            "Favorite: true".to_owned(),
            None,
        );
    }

    for segment in &item.content_analysis.transcript_segments {
        let relationships = segment
            .timeline_relationships
            .iter()
            .map(|relationship| match &relationship.clip_id {
                Some(clip_id) => format!("{} {clip_id}", relationship.timeline_id),
                None => relationship.timeline_id.clone(),
            })
            .collect::<Vec<_>>()
            .join(" ");
        let speaker = segment.speaker.as_deref().unwrap_or("unassigned");
        let searchable = format!(
            "Transcript {} speaker {speaker} {} {relationships}",
            segment.segment_id, segment.text
        );
        let evidence = MediaContentSearchMatch {
            signal: MediaContentSearchSignal::Transcript,
            evidence: truncate_content_search_evidence(&format!("{speaker}: {}", segment.text)),
            segment_id: Some(segment.segment_id.clone()),
            start_frame: Some(segment.start_frame),
            end_frame: Some(segment.end_frame),
            rate_numerator: Some(segment.rate_numerator),
            rate_denominator: Some(segment.rate_denominator),
            speaker: segment.speaker.clone(),
            timeline_relationships: segment.timeline_relationships.clone(),
        };
        fields.push(ContentSearchField {
            searchable: searchable.to_lowercase(),
            weight: 40,
            evidence,
        });
    }

    for content in &item.content_analysis.local_ai_content {
        let linked_segment = content.segment_ids.iter().find_map(|segment_id| {
            item.content_analysis
                .transcript_segments
                .iter()
                .find(|segment| &segment.segment_id == segment_id)
        });
        push_content_search_field(
            &mut fields,
            MediaContentSearchSignal::LocalAi,
            32,
            format!(
                "Local AI {}: {} terms {} segments {}",
                content.content_id,
                content.label,
                content.terms.join(" "),
                content.segment_ids.join(" ")
            ),
            linked_segment,
        );
    }

    if tokens
        .iter()
        .any(|token| !fields.iter().any(|field| field.searchable.contains(token)))
    {
        return None;
    }
    let query = query.to_lowercase();
    let mut score = 0_u64;
    let mut matches = fields
        .into_iter()
        .filter_map(|field| {
            let token_match = tokens.iter().any(|token| field.searchable.contains(token));
            if !token_match {
                return None;
            }
            let phrase_bonus = if field.searchable.contains(&query) {
                field.weight
            } else {
                0
            };
            score = score.saturating_add(field.weight.saturating_add(phrase_bonus));
            Some((field.weight.saturating_add(phrase_bonus), field.evidence))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.evidence.cmp(&right.1.evidence))
    });
    let matches = matches
        .into_iter()
        .take(MAX_CONTENT_SEARCH_MATCHES)
        .map(|(_, evidence)| evidence)
        .collect();
    Some(MediaContentSearchResult {
        media_id: item.media_id.clone(),
        name: item.name.clone(),
        score,
        analysis_fresh: !item.content_analysis.source_fingerprint.is_empty()
            && item.content_analysis.source_fingerprint == item.content_fingerprint,
        matches,
    })
}

fn push_content_search_field(
    fields: &mut Vec<ContentSearchField>,
    signal: MediaContentSearchSignal,
    weight: u64,
    evidence: String,
    segment: Option<&MediaTranscriptSegment>,
) {
    if evidence.trim().is_empty() {
        return;
    }
    fields.push(ContentSearchField {
        searchable: evidence.to_lowercase(),
        weight,
        evidence: MediaContentSearchMatch {
            signal,
            evidence: truncate_content_search_evidence(&evidence),
            segment_id: segment.map(|segment| segment.segment_id.clone()),
            start_frame: segment.map(|segment| segment.start_frame),
            end_frame: segment.map(|segment| segment.end_frame),
            rate_numerator: segment.map(|segment| segment.rate_numerator),
            rate_denominator: segment.map(|segment| segment.rate_denominator),
            speaker: segment.and_then(|segment| segment.speaker.clone()),
            timeline_relationships: segment
                .map(|segment| segment.timeline_relationships.clone())
                .unwrap_or_default(),
        },
    });
}

fn truncate_content_search_evidence(value: &str) -> String {
    value
        .chars()
        .take(MAX_CONTENT_SEARCH_EVIDENCE_CHARS)
        .collect()
}

fn normalize_derived_media(
    item: &mut MediaBrowserItem,
    mutation: DerivedMediaMutation,
) -> Result<(), DesktopProjectFailure> {
    match mutation {
        DerivedMediaMutation::CreateOrReplace {
            artifact_id,
            purpose,
            quality,
            status,
            source_fingerprint,
            source_revision,
            byte_len,
        } => {
            require_media_text("artifact_id", &artifact_id)?;
            if source_fingerprint != item.content_fingerprint {
                return Err(user_correctable(
                    "derived_media_source_stale",
                    "Source changed before derived media was attached",
                    "Regenerate the proxy or optimized media from the current source.",
                ));
            }
            if byte_len == 0 && status == DerivedMediaStatus::Ready {
                return Err(media_library_invalid("Ready derived media cannot be empty"));
            }
            let attachment = DerivedMediaAttachment {
                artifact_id,
                purpose,
                quality,
                status,
                source_fingerprint,
                source_revision,
                byte_len,
            };
            if let Some(existing) = item
                .derived_media
                .iter_mut()
                .find(|candidate| candidate.purpose == purpose && candidate.quality == quality)
            {
                *existing = attachment;
            } else {
                if item.derived_media.len() >= MAX_DERIVED_MEDIA_ATTACHMENTS {
                    return Err(media_library_invalid(
                        "Derived media attachment limit reached",
                    ));
                }
                item.derived_media.push(attachment);
            }
            item.derived_media
                .sort_by_key(|attachment| (attachment.purpose as u8, attachment.quality));
        }
        DerivedMediaMutation::SetChoice { choice } => item.representation_choice = choice,
        DerivedMediaMutation::Remove { purpose, quality } => item
            .derived_media
            .retain(|attachment| attachment.purpose != purpose || attachment.quality != quality),
    }
    item.refresh_derived_media();
    Ok(())
}

fn normalize_offline_media(
    item: &mut MediaBrowserItem,
    mutation: OfflineMediaMutation,
) -> Result<(), DesktopProjectFailure> {
    let reset_monitoring;
    match mutation {
        OfflineMediaMutation::Relink {
            source_paths,
            candidate_fingerprint,
        } => {
            validate_source_paths(&source_paths)?;
            if candidate_fingerprint != item.content_fingerprint {
                return Err(user_correctable(
                    "relink_fingerprint_mismatch",
                    "Relink candidate is different media",
                    "Use Replace source to intentionally change the source identity.",
                ));
            }
            item.source_paths = source_paths;
            item.source_count = item.source_paths.len() as u64;
            reset_monitoring = true;
        }
        OfflineMediaMutation::Replace {
            source_paths,
            replacement_fingerprint,
        } => {
            validate_source_paths(&source_paths)?;
            require_media_text("replacement_fingerprint", &replacement_fingerprint)?;
            item.source_paths = source_paths;
            item.source_count = item.source_paths.len() as u64;
            item.content_fingerprint = replacement_fingerprint;
            item.metadata
                .insert("freshness".to_owned(), item.content_fingerprint.clone());
            reset_monitoring = true;
        }
        OfflineMediaMutation::Conform {
            frame_rate_numerator,
            frame_rate_denominator,
        } => {
            if frame_rate_numerator == 0 || frame_rate_denominator == 0 {
                return Err(media_library_invalid("Conform frame rate must be positive"));
            }
            item.frame_rate_numerator = Some(frame_rate_numerator);
            item.frame_rate_denominator = Some(frame_rate_denominator);
            item.metadata.insert(
                "frame_rate".to_owned(),
                format!("{frame_rate_numerator}/{frame_rate_denominator}"),
            );
            reset_monitoring = false;
        }
    }
    if reset_monitoring {
        item.source_monitoring =
            MediaSourceMonitoring::unscanned(&item.source_paths, &item.content_fingerprint);
    }
    let imported = DesktopImportedMedia {
        media_id: item.media_id.clone(),
        name: item.name.clone(),
        source_paths: item.source_paths.clone(),
        content_fingerprint: item.content_fingerprint.clone(),
        kind: item.kind,
        source_count: item.source_count,
        first_frame: item.first_frame,
        last_frame: item.last_frame,
        frame_rate_numerator: item.frame_rate_numerator,
        frame_rate_denominator: item.frame_rate_denominator,
        source_fingerprints: Vec::new(),
    };
    item.source_metadata = source_metadata_inspection(
        &imported,
        item.source_metadata.inspection_generation.saturating_add(1),
    );
    item.refresh_thumbnail();
    item.refresh_derived_media();
    Ok(())
}

fn validate_source_paths(paths: &[String]) -> Result<(), DesktopProjectFailure> {
    if paths.is_empty() || paths.len() > MAX_IMPORT_SOURCES {
        return Err(media_library_invalid(
            "Source path list is empty or too large",
        ));
    }
    for path in paths {
        require_media_text("source_path", path)?;
    }
    Ok(())
}

fn resolve_media_representation(
    choice: MediaRepresentationChoice,
    source_fingerprint: &str,
    attachments: &[DerivedMediaAttachment],
) -> ResolvedMediaRepresentation {
    let (purpose, quality, representation) = match choice {
        MediaRepresentationChoice::Original => return original_representation(),
        MediaRepresentationChoice::Proxy { quality } => {
            (DerivedMediaPurpose::Proxy, quality, "proxy")
        }
        MediaRepresentationChoice::Optimized { quality } => {
            (DerivedMediaPurpose::Optimized, quality, "optimized")
        }
    };
    let selected = attachments.iter().find(|attachment| {
        attachment.purpose == purpose
            && attachment.quality == quality
            && attachment.status == DerivedMediaStatus::Ready
            && attachment.source_fingerprint == source_fingerprint
    });
    match selected {
        Some(attachment) => ResolvedMediaRepresentation {
            representation: representation.to_owned(),
            quality: Some(quality),
            artifact_id: Some(attachment.artifact_id.clone()),
            fallback_to_original: false,
        },
        None => ResolvedMediaRepresentation {
            representation: "original".to_owned(),
            quality: None,
            artifact_id: None,
            fallback_to_original: true,
        },
    }
}

fn normalize_annotation_text(
    value: Option<String>,
    max_bytes: usize,
    field: &'static str,
) -> Result<Option<String>, DesktopProjectFailure> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > max_bytes
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        return Err(media_library_invalid(field));
    }
    Ok(Some(value.to_owned()))
}

fn normalize_annotation_terms(
    values: Vec<String>,
    max_count: usize,
    max_bytes: usize,
    field: &'static str,
) -> Result<Vec<String>, DesktopProjectFailure> {
    if values.len() > max_count {
        return Err(media_library_invalid(field));
    }
    let mut canonical = BTreeSet::new();
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let value = normalize_annotation_text(Some(value), max_bytes, field)?
            .ok_or_else(|| media_library_invalid(field))?;
        if !canonical.insert(value.to_lowercase()) {
            return Err(media_library_invalid(field));
        }
        normalized.push(value);
    }
    Ok(normalized)
}

fn derive_media_usage(project: &EditorialProject) -> BTreeMap<String, MediaUsageIndicator> {
    let mut usage = BTreeMap::<String, MediaUsageIndicator>::new();
    for timeline in project.timelines() {
        let timeline_id = timeline.id().to_string();
        let mut timeline_counts = BTreeMap::<String, u64>::new();
        for clip in timeline
            .tracks()
            .iter()
            .flat_map(|track| track.items())
            .filter_map(|item| item.as_clip())
        {
            if let ClipSource::Media(media_id) = clip.source() {
                let count = timeline_counts.entry(media_id.to_string()).or_default();
                *count = count.saturating_add(1);
            }
        }
        for (media_id, clip_count) in timeline_counts {
            let indicator = usage.entry(media_id).or_default();
            indicator.clip_count = indicator.clip_count.saturating_add(clip_count);
            indicator.timeline_ids.push(timeline_id.clone());
            indicator.timeline_count = indicator.timeline_ids.len() as u64;
        }
    }
    usage
}

fn inspect_media_usage(
    project_path: &Path,
) -> Result<BTreeMap<String, MediaUsageIndicator>, DesktopProjectFailure> {
    let database = ProjectDatabase::open_read_only(project_path)
        .map_err(|error| safe_failure("inspect_media_usage", error))?;
    let document = database
        .load()
        .map_err(|error| safe_failure("inspect_media_usage", error))?;
    Ok(derive_media_usage(document.snapshot().editorial_project()))
}

/// One opaque recovery candidate projected for user selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopRecoveryCandidate {
    candidate_id: String,
    project_revision: u64,
    action: String,
}

impl DesktopRecoveryCandidate {
    pub fn new(
        candidate_id: impl Into<String>,
        project_revision: u64,
        action: impl Into<String>,
    ) -> Result<Self, DesktopProjectFailure> {
        let candidate = Self {
            candidate_id: candidate_id.into(),
            project_revision,
            action: action.into(),
        };
        if candidate.candidate_id.trim().is_empty() || candidate.action.trim().is_empty() {
            return Err(user_correctable(
                "recovery_candidate_invalid",
                "Recovery candidate is invalid",
                "Refresh recovery options and choose another candidate.",
            ));
        }
        Ok(candidate)
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }
}

/// Complete revision-fenced recovery presentation for the active project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopRecoveryCatalog {
    catalog_revision: u64,
    candidates: Vec<DesktopRecoveryCandidate>,
}

impl DesktopRecoveryCatalog {
    pub fn new(
        catalog_revision: u64,
        candidates: Vec<DesktopRecoveryCandidate>,
    ) -> Result<Self, DesktopProjectFailure> {
        if catalog_revision == 0 {
            return Err(user_correctable(
                "recovery_catalog_invalid",
                "Recovery catalog is invalid",
                "Refresh recovery options before continuing.",
            ));
        }
        let mut identities = BTreeSet::new();
        if candidates
            .iter()
            .any(|candidate| !identities.insert(candidate.candidate_id.clone()))
        {
            return Err(user_correctable(
                "recovery_catalog_duplicate",
                "Recovery catalog is ambiguous",
                "Refresh recovery options before continuing.",
            ));
        }
        Ok(Self {
            catalog_revision,
            candidates,
        })
    }

    #[must_use]
    pub const fn catalog_revision(&self) -> u64 {
        self.catalog_revision
    }

    #[must_use]
    pub fn candidates(&self) -> &[DesktopRecoveryCandidate] {
        &self.candidates
    }
}

/// Active or recent project presentation. The path is explicit user-selected display state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectRecord {
    path: String,
    identity: DesktopProjectIdentity,
}

impl DesktopProjectRecord {
    fn new(path: String, identity: DesktopProjectIdentity) -> Self {
        Self { path, identity }
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        self.identity.project_id()
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.identity.project_revision()
    }

    #[must_use]
    pub fn root_timeline_id(&self) -> &str {
        self.identity.root_timeline_id()
    }
}

/// Complete replacement state for desktop project lifecycle presentation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectSnapshot {
    revision: u64,
    active: Option<DesktopProjectRecord>,
    recent: Vec<DesktopProjectRecord>,
    recovery: Option<DesktopRecoveryCatalog>,
    failure: Option<DesktopProjectFailure>,
}

impl DesktopProjectSnapshot {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn active(&self) -> Option<&DesktopProjectRecord> {
        self.active.as_ref()
    }

    #[must_use]
    pub fn recent(&self) -> &[DesktopProjectRecord] {
        &self.recent
    }

    #[must_use]
    pub const fn recovery(&self) -> Option<&DesktopRecoveryCatalog> {
        self.recovery.as_ref()
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&DesktopProjectFailure> {
        self.failure.as_ref()
    }
}

/// Complete editable project settings presented by the desktop lifecycle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectSettings {
    project_revision: u64,
    frame_rate_numerator: u32,
    frame_rate_denominator: u32,
    timecode_mode: String,
    resolution_width: Option<u32>,
    resolution_height: Option<u32>,
    color_mode: String,
    color_working_space: String,
    color_config_id: Option<String>,
    color_config_fingerprint: Option<String>,
    audio_sample_rate_hz: u32,
    audio_output_layout: String,
    cache_mode: String,
    cache_max_bytes: Option<u64>,
    cache_max_frames: Option<u64>,
    proxy_mode: String,
    proxy_quality: String,
    working_folder: Option<String>,
    cache_folder: Option<String>,
    proxy_folder: Option<String>,
}

impl DesktopProjectSettings {
    fn from_snapshot(snapshot: &ProjectSettingsSnapshot) -> Result<Self, DesktopProjectFailure> {
        let resolution = match setting_text(snapshot, RENDER_EXTENT_MODE_KEY)?.as_str() {
            "source" => None,
            "explicit" => Some((
                setting_u32(snapshot, RENDER_WIDTH_KEY)?,
                setting_u32(snapshot, RENDER_HEIGHT_KEY)?,
            )),
            _ => return Err(settings_snapshot_invalid(RENDER_EXTENT_MODE_KEY)),
        };
        let (resolution_width, resolution_height) = resolution
            .map(|(width, height)| (Some(width), Some(height)))
            .unwrap_or((None, None));
        let cache_mode = setting_text(snapshot, CACHE_MODE_KEY)?;
        let (cache_max_bytes, cache_max_frames) = if cache_mode == "bounded" {
            (
                Some(setting_u64(snapshot, CACHE_MAX_BYTES_KEY)?),
                Some(setting_u64(snapshot, CACHE_MAX_FRAMES_KEY)?),
            )
        } else {
            (None, None)
        };
        Ok(Self {
            project_revision: snapshot.project_revision(),
            frame_rate_numerator: setting_u32(snapshot, TIMELINE_RATE_NUMERATOR_KEY)?,
            frame_rate_denominator: setting_u32(snapshot, TIMELINE_RATE_DENOMINATOR_KEY)?,
            timecode_mode: setting_text(snapshot, TIMELINE_TIMECODE_MODE_KEY)?,
            resolution_width,
            resolution_height,
            color_mode: setting_text(snapshot, COLOR_MODE_KEY)?,
            color_working_space: setting_text(snapshot, COLOR_WORKING_SPACE_KEY)?,
            color_config_id: optional_setting_text(snapshot, COLOR_CONFIG_ID_KEY)?,
            color_config_fingerprint: optional_setting_text(
                snapshot,
                COLOR_CONFIG_FINGERPRINT_KEY,
            )?,
            audio_sample_rate_hz: setting_u32(snapshot, AUDIO_SAMPLE_RATE_KEY)?,
            audio_output_layout: setting_text(snapshot, AUDIO_OUTPUT_LAYOUT_KEY)?,
            cache_mode,
            cache_max_bytes,
            cache_max_frames,
            proxy_mode: setting_text(snapshot, PROXY_MODE_KEY)?,
            proxy_quality: setting_text(snapshot, PROXY_QUALITY_KEY)?,
            working_folder: optional_setting_text(snapshot, WORKING_FOLDER_KEY)?,
            cache_folder: optional_setting_text(snapshot, WORKING_CACHE_FOLDER_KEY)?,
            proxy_folder: optional_setting_text(snapshot, WORKING_PROXY_FOLDER_KEY)?,
        })
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub const fn frame_rate(&self) -> (u32, u32) {
        (self.frame_rate_numerator, self.frame_rate_denominator)
    }

    #[must_use]
    pub const fn resolution(&self) -> Option<(u32, u32)> {
        match (self.resolution_width, self.resolution_height) {
            (Some(width), Some(height)) => Some((width, height)),
            _ => None,
        }
    }

    #[must_use]
    pub fn color_mode(&self) -> &str {
        &self.color_mode
    }

    #[must_use]
    pub fn color_working_space(&self) -> &str {
        &self.color_working_space
    }

    #[must_use]
    pub const fn audio_sample_rate_hz(&self) -> u32 {
        self.audio_sample_rate_hz
    }

    #[must_use]
    pub fn audio_output_layout(&self) -> &str {
        &self.audio_output_layout
    }

    #[must_use]
    pub fn cache_mode(&self) -> &str {
        &self.cache_mode
    }

    #[must_use]
    pub const fn cache_budget(&self) -> Option<(u64, u64)> {
        match (self.cache_max_bytes, self.cache_max_frames) {
            (Some(bytes), Some(frames)) => Some((bytes, frames)),
            _ => None,
        }
    }

    #[must_use]
    pub fn proxy_mode(&self) -> &str {
        &self.proxy_mode
    }

    #[must_use]
    pub fn proxy_quality(&self) -> &str {
        &self.proxy_quality
    }

    #[must_use]
    pub fn working_folder(&self) -> Option<&str> {
        self.working_folder.as_deref()
    }

    #[must_use]
    pub fn cache_folder(&self) -> Option<&str> {
        self.cache_folder.as_deref()
    }

    #[must_use]
    pub fn proxy_folder(&self) -> Option<&str> {
        self.proxy_folder.as_deref()
    }
}

/// One optimistic full replacement of the desktop-owned project settings surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectSettingsUpdate {
    pub expected_project_revision: u64,
    pub frame_rate_numerator: u32,
    pub frame_rate_denominator: u32,
    pub timecode_mode: String,
    pub resolution_width: Option<u32>,
    pub resolution_height: Option<u32>,
    pub color_mode: String,
    pub color_working_space: String,
    pub color_config_id: Option<String>,
    pub color_config_fingerprint: Option<String>,
    pub audio_sample_rate_hz: u32,
    pub audio_output_layout: String,
    pub cache_mode: String,
    pub cache_max_bytes: Option<u64>,
    pub cache_max_frames: Option<u64>,
    pub proxy_mode: String,
    pub proxy_quality: String,
    pub working_folder: Option<String>,
    pub cache_folder: Option<String>,
    pub proxy_folder: Option<String>,
}

impl DesktopProjectSettingsUpdate {
    fn into_transaction(self) -> Result<ExecuteProjectSettingsTransaction, DesktopProjectFailure> {
        let mut mutations = vec![
            set_integer(
                TIMELINE_RATE_NUMERATOR_KEY,
                i64::from(self.frame_rate_numerator),
            ),
            set_integer(
                TIMELINE_RATE_DENOMINATOR_KEY,
                i64::from(self.frame_rate_denominator),
            ),
            set_text(TIMELINE_TIMECODE_MODE_KEY, self.timecode_mode),
            set_text(COLOR_MODE_KEY, self.color_mode),
            set_text(COLOR_WORKING_SPACE_KEY, self.color_working_space),
            set_integer(AUDIO_SAMPLE_RATE_KEY, i64::from(self.audio_sample_rate_hz)),
            set_text(AUDIO_OUTPUT_LAYOUT_KEY, self.audio_output_layout),
            set_text(CACHE_MODE_KEY, self.cache_mode),
            set_text(PROXY_MODE_KEY, self.proxy_mode),
            set_text(PROXY_QUALITY_KEY, self.proxy_quality),
        ];
        match (self.resolution_width, self.resolution_height) {
            (Some(width), Some(height)) => {
                mutations.push(set_text(RENDER_EXTENT_MODE_KEY, "explicit"));
                mutations.push(set_integer(RENDER_WIDTH_KEY, i64::from(width)));
                mutations.push(set_integer(RENDER_HEIGHT_KEY, i64::from(height)));
            }
            (None, None) => {
                mutations.push(set_text(RENDER_EXTENT_MODE_KEY, "source"));
                mutations.push(remove_setting(RENDER_WIDTH_KEY));
                mutations.push(remove_setting(RENDER_HEIGHT_KEY));
            }
            _ => {
                return Err(user_correctable(
                    "project_settings_resolution_incomplete",
                    "Project resolution is incomplete",
                    "Provide both resolution dimensions or clear both values.",
                ));
            }
        }
        replace_optional_text(&mut mutations, COLOR_CONFIG_ID_KEY, self.color_config_id);
        replace_optional_text(
            &mut mutations,
            COLOR_CONFIG_FINGERPRINT_KEY,
            self.color_config_fingerprint,
        );
        replace_optional_integer(&mut mutations, CACHE_MAX_BYTES_KEY, self.cache_max_bytes)?;
        replace_optional_integer(&mut mutations, CACHE_MAX_FRAMES_KEY, self.cache_max_frames)?;
        replace_optional_text(&mut mutations, WORKING_FOLDER_KEY, self.working_folder);
        replace_optional_text(&mut mutations, WORKING_CACHE_FOLDER_KEY, self.cache_folder);
        replace_optional_text(&mut mutations, WORKING_PROXY_FOLDER_KEY, self.proxy_folder);
        Ok(ExecuteProjectSettingsTransaction::new(
            format!(
                "superi-desktop-project-settings-{}",
                self.expected_project_revision
            ),
            self.expected_project_revision,
            mutations,
        ))
    }
}

fn setting_text(
    snapshot: &ProjectSettingsSnapshot,
    key: &'static str,
) -> Result<String, DesktopProjectFailure> {
    match snapshot.value(key) {
        Some(ProjectSettingValue::Text(value)) => Ok(value.clone()),
        _ => Err(settings_snapshot_invalid(key)),
    }
}

fn optional_setting_text(
    snapshot: &ProjectSettingsSnapshot,
    key: &'static str,
) -> Result<Option<String>, DesktopProjectFailure> {
    match snapshot.value(key) {
        None => Ok(None),
        Some(ProjectSettingValue::Text(value)) => Ok(Some(value.clone())),
        _ => Err(settings_snapshot_invalid(key)),
    }
}

fn setting_u64(
    snapshot: &ProjectSettingsSnapshot,
    key: &'static str,
) -> Result<u64, DesktopProjectFailure> {
    match snapshot.value(key) {
        Some(ProjectSettingValue::Integer(value)) => {
            u64::try_from(*value).map_err(|_| settings_snapshot_invalid(key))
        }
        _ => Err(settings_snapshot_invalid(key)),
    }
}

fn setting_u32(
    snapshot: &ProjectSettingsSnapshot,
    key: &'static str,
) -> Result<u32, DesktopProjectFailure> {
    setting_u64(snapshot, key)
        .and_then(|value| u32::try_from(value).map_err(|_| settings_snapshot_invalid(key)))
}

fn settings_snapshot_invalid(key: &'static str) -> DesktopProjectFailure {
    DesktopProjectFailure::new(
        DesktopProjectFailureClass::Degraded,
        "project_settings_snapshot_invalid",
        "Project settings could not be displayed",
        "Close and reopen the project before editing its settings.",
    )
    .with_context("key", key)
}

fn set_text(key: &'static str, value: impl Into<String>) -> ProjectSettingMutation {
    ProjectSettingMutation::Set {
        key: key.to_owned(),
        value: ProjectSettingValue::Text(value.into()),
    }
}

fn set_integer(key: &'static str, value: i64) -> ProjectSettingMutation {
    ProjectSettingMutation::Set {
        key: key.to_owned(),
        value: ProjectSettingValue::Integer(value),
    }
}

fn remove_setting(key: &'static str) -> ProjectSettingMutation {
    ProjectSettingMutation::Remove {
        key: key.to_owned(),
    }
}

fn replace_optional_text(
    mutations: &mut Vec<ProjectSettingMutation>,
    key: &'static str,
    value: Option<String>,
) {
    mutations.push(match value {
        Some(value) => set_text(key, value),
        None => remove_setting(key),
    });
}

fn replace_optional_integer(
    mutations: &mut Vec<ProjectSettingMutation>,
    key: &'static str,
    value: Option<u64>,
) -> Result<(), DesktopProjectFailure> {
    mutations.push(match value {
        Some(value) => set_integer(
            key,
            i64::try_from(value).map_err(|_| {
                user_correctable(
                    "project_settings_integer_too_large",
                    "Project setting is too large",
                    "Choose a smaller cache budget.",
                )
                .with_context("key", key)
            })?,
        ),
        None => remove_setting(key),
    });
    Ok(())
}

/// Durable operation boundary implemented by the existing API-owned local project host.
pub trait DesktopProjectBackend {
    fn create(
        &mut self,
        path: &Path,
        request: &DesktopProjectCreateRequest,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure>;

    fn open(&mut self, path: &Path) -> Result<DesktopProjectIdentity, DesktopProjectFailure>;

    fn save(&mut self, path: &Path) -> Result<DesktopProjectIdentity, DesktopProjectFailure>;

    fn save_as(
        &mut self,
        path: &Path,
        destination: &Path,
        replace_existing: bool,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure>;

    fn discover_recovery(
        &mut self,
        path: &Path,
    ) -> Result<DesktopRecoveryCatalog, DesktopProjectFailure>;

    fn restore_recovery(
        &mut self,
        path: &Path,
        catalog_revision: u64,
        candidate_id: &str,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure>;
}

/// Concrete durable backend over the existing API-owned local project host.
pub struct LocalProjectBackend {
    recovery_root: PathBuf,
}

impl LocalProjectBackend {
    #[must_use]
    pub fn new(recovery_root: PathBuf) -> Self {
        Self { recovery_root }
    }

    fn identity(
        summary: &LocalProjectSummary,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        DesktopProjectIdentity::new(
            summary.project_id(),
            summary.project_revision(),
            summary.root_timeline_id(),
        )
    }

    fn validate(
        path: &Path,
        operation: &'static str,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        let validation =
            LocalProjectHost::validate(path).map_err(|error| safe_failure(operation, error))?;
        Self::identity(validation.project())
    }

    fn inspect_settings(
        &self,
        path: &Path,
    ) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
        let response = LocalProjectHost::inspect_settings(path, GetProjectSettings::new())
            .map_err(|error| safe_failure("inspect_settings", error))?;
        DesktopProjectSettings::from_snapshot(response.snapshot())
    }

    fn update_settings(
        &mut self,
        path: &Path,
        update: DesktopProjectSettingsUpdate,
    ) -> Result<(DesktopProjectSettings, DesktopProjectIdentity), DesktopProjectFailure> {
        let response = LocalProjectHost::execute_settings(
            path,
            update.into_transaction()?,
            Arc::new(ApiPermissionContext::default()),
        )
        .map_err(|error| safe_failure("update_settings", error))?;
        let settings = DesktopProjectSettings::from_snapshot(response.result().snapshot())?;
        let identity = Self::validate(path, "update_settings")?;
        Ok((settings, identity))
    }

    fn import_media(
        &mut self,
        path: &Path,
        request: DesktopMediaImportRequest,
    ) -> Result<(DesktopMediaImportResult, DesktopProjectIdentity), DesktopProjectFailure> {
        let identity = Self::validate(path, "import_media")?;
        if identity.project_revision() != request.expected_project_revision {
            return Err(user_correctable(
                "project_revision_stale",
                "Project changed before media import",
                "Refresh the project and try the import again.",
            ));
        }
        let frame_rate = self.inspect_settings(path)?.frame_rate();
        let origin = request.origin;
        let discovered = discover_import_sources(&request, frame_rate)?;
        if discovered.is_empty() {
            return Ok((
                DesktopMediaImportResult {
                    project_revision: identity.project_revision(),
                    imported: Vec::new(),
                    skipped: Vec::new(),
                    command_method: ExecuteProjectCommand::METHOD.to_owned(),
                    event_name: ProjectStateChanged::NAME.to_owned(),
                    event_sequence: None,
                    automation_method: (origin == DesktopMediaImportOrigin::Automation)
                        .then(|| ExecuteProjectCommand::METHOD.to_owned()),
                },
                identity,
            ));
        }

        let permissions = import_permissions(&discovered)?;
        let command = ExecuteProjectCommand::new(
            format!(
                "superi-desktop-import-{}-{}",
                request.expected_project_revision,
                discovered[0].desktop.media_id()
            ),
            request.expected_project_revision,
            ProjectCommand::Apply {
                actions: vec![ProjectAction::ImportMedia {
                    media: discovered
                        .iter()
                        .map(|source| source.editor.clone())
                        .collect(),
                }],
            },
        );
        let result = if origin == DesktopMediaImportOrigin::Automation {
            let response = LocalProjectHost::execute_automation(
                path,
                LocalAutomationRequest::ProjectCommand {
                    jsonrpc: "2.0".to_owned(),
                    id: LocalAutomationId::String(format!(
                        "desktop-import-{}",
                        request.expected_project_revision
                    )),
                    params: command,
                },
                permissions,
            )
            .map_err(|error| safe_failure("import_media_automation", error))?;
            match response.result() {
                LocalAutomationResult::ProjectCommand(execution) => {
                    present_import(execution, &discovered, true)?
                }
                _ => {
                    return Err(DesktopProjectFailure::new(
                        DesktopProjectFailureClass::Terminal,
                        "media_import_automation_invalid",
                        "Media import returned an invalid response",
                        "Restart Superi before importing again.",
                    ))
                }
            }
        } else {
            let execution = LocalProjectHost::execute_media(path, command, permissions)
                .map_err(|error| safe_failure("import_media", error))?;
            present_import(&execution, &discovered, false)?
        };
        let identity = Self::validate(path, "import_media")?;
        Ok((result, identity))
    }

    fn restore_permissions() -> Result<Arc<ApiPermissionContext>, DesktopProjectFailure> {
        ApiPermissionContext::new(
            "superi.desktop.project",
            [ApiPermissionRule::destructive(
                ApiPermissionEffect::Allow,
                ApiDestructiveOperation::RestoreProjectRecovery,
            )],
        )
        .map(Arc::new)
        .map_err(|error| safe_failure("restore_recovery", error))
    }
}

impl DesktopProjectBackend for LocalProjectBackend {
    fn create(
        &mut self,
        path: &Path,
        request: &DesktopProjectCreateRequest,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        let summary = LocalProjectHost::create(
            path,
            LocalProjectCreateRequest {
                project_id: request.project_id.clone(),
                project_name: request.project_name.clone(),
                root_timeline_id: request.root_timeline_id.clone(),
                root_timeline_name: request.root_timeline_name.clone(),
                edit_rate_numerator: request.edit_rate_numerator,
                edit_rate_denominator: request.edit_rate_denominator,
            },
        )
        .map_err(|error| safe_failure("create", error))?;
        Self::identity(&summary)
    }

    fn open(&mut self, path: &Path) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        Self::validate(path, "open")
    }

    fn save(&mut self, path: &Path) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        LocalProjectHost::save(path).map_err(|error| safe_failure("save", error))?;
        Self::validate(path, "save")
    }

    fn save_as(
        &mut self,
        path: &Path,
        destination: &Path,
        replace_existing: bool,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        let collision = if replace_existing {
            LocalProjectCollision::ReplaceExisting
        } else {
            LocalProjectCollision::RequireAbsent
        };
        LocalProjectHost::save_as(path, destination, collision)
            .map_err(|error| safe_failure("save_as", error))?;
        Self::validate(destination, "save_as")
    }

    fn discover_recovery(
        &mut self,
        path: &Path,
    ) -> Result<DesktopRecoveryCatalog, DesktopProjectFailure> {
        let response = LocalProjectHost::recovery_get(
            path,
            &self.recovery_root,
            GetProjectRecovery::new("superi-desktop-recovery-get"),
            Arc::new(ApiPermissionContext::default()),
        )
        .map_err(|error| safe_failure("discover_recovery", error))?;
        let snapshot = response.result().snapshot();
        let candidates = snapshot
            .candidates()
            .iter()
            .map(|candidate| {
                DesktopRecoveryCandidate::new(
                    candidate.candidate_id(),
                    candidate.captured_project_revision(),
                    format!(
                        "Recover autosaved project revision {}.",
                        candidate.captured_project_revision()
                    ),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        DesktopRecoveryCatalog::new(snapshot.catalog_revision(), candidates)
    }

    fn restore_recovery(
        &mut self,
        path: &Path,
        catalog_revision: u64,
        candidate_id: &str,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        let current = Self::validate(path, "restore_recovery")?;
        let response = LocalProjectHost::recovery_restore(
            path,
            &self.recovery_root,
            RestoreProjectRecovery::new(
                "superi-desktop-recovery-restore",
                catalog_revision,
                current.project_revision(),
                candidate_id,
            ),
            Self::restore_permissions()?,
        )
        .map_err(|error| safe_failure("restore_recovery", error))?;
        if !response.result().restored() {
            return Err(DesktopProjectFailure::new(
                DesktopProjectFailureClass::Degraded,
                "recovery_not_restored",
                "Project recovery did not complete",
                "Refresh recovery options and try another recovery point.",
            )
            .with_context("operation", "restore_recovery"));
        }
        Self::validate(path, "restore_recovery")
    }
}

/// Tauri-managed concrete project lifecycle state.
#[derive(Clone)]
pub struct DesktopProjectState {
    lifecycle: Arc<Mutex<Option<DesktopProjectLifecycle<LocalProjectBackend>>>>,
    media_libraries: Arc<Mutex<MediaLibraryStore>>,
    media_library_path: Arc<Mutex<Option<PathBuf>>>,
    source_monitor: Arc<Mutex<source_monitor::SourceMonitorRuntime>>,
}

impl Default for DesktopProjectState {
    fn default() -> Self {
        Self {
            lifecycle: Arc::new(Mutex::new(None)),
            media_libraries: Arc::new(Mutex::new(MediaLibraryStore::default())),
            media_library_path: Arc::new(Mutex::new(None)),
            source_monitor: Arc::new(Mutex::new(source_monitor::SourceMonitorRuntime::default())),
        }
    }
}

impl DesktopProjectState {
    pub fn initialize(&self, recovery_root: PathBuf) -> Result<(), DesktopProjectFailure> {
        std::fs::create_dir_all(&recovery_root).map_err(|source| {
            safe_failure(
                "initialize_recovery",
                Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "desktop recovery storage could not be initialized",
                    source,
                ),
            )
        })?;
        let media_library_path = recovery_root.join("media-library-views.json");
        let mut store = if media_library_path.exists() {
            let bytes = std::fs::read(&media_library_path)
                .map_err(|source| media_library_store_failure("read", source))?;
            serde_json::from_slice::<MediaLibraryStore>(&bytes).map_err(|_| {
                DesktopProjectFailure::new(
                    DesktopProjectFailureClass::Degraded,
                    "media_library_store_invalid",
                    "Media library presentation could not be restored",
                    "Reopen the project and rebuild its media organization.",
                )
            })?
        } else {
            MediaLibraryStore::default()
        };
        for library in store.projects.values_mut() {
            library.refresh_derived();
            library.validate()?;
        }
        let lifecycle = DesktopProjectLifecycle::new(LocalProjectBackend::new(recovery_root), 12)?;
        *self.lock("initialize")? = Some(lifecycle);
        *self.media_library_lock("initialize")? = store;
        *self.media_library_path_lock("initialize")? = Some(media_library_path);
        self.reset_source_monitor("initialize")?;
        Ok(())
    }

    pub fn snapshot(&self) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
        self.lock("snapshot")?
            .as_ref()
            .map(|lifecycle| lifecycle.snapshot().clone())
            .ok_or_else(not_initialized)
    }

    pub fn execute(
        &self,
        command: DesktopProjectCommand,
    ) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
        let previous_active = {
            let lifecycle = self.lock("execute")?;
            lifecycle
                .as_ref()
                .ok_or_else(not_initialized)?
                .snapshot()
                .active()
                .map(|active| (active.project_id().to_owned(), active.path().to_owned()))
        };
        let snapshot = self
            .lock("execute")?
            .as_mut()
            .ok_or_else(not_initialized)?
            .execute(command)?;
        let current_active = snapshot
            .active()
            .map(|active| (active.project_id().to_owned(), active.path().to_owned()));
        if previous_active != current_active {
            self.reset_source_monitor("execute_project_transition")?;
        }
        self.activate_media_library(&snapshot)?;
        Ok(snapshot)
    }

    pub fn settings(&self) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
        self.lock("settings")?
            .as_ref()
            .ok_or_else(not_initialized)?
            .inspect_settings()
    }

    pub fn update_settings(
        &self,
        update: DesktopProjectSettingsUpdate,
    ) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
        let settings = self
            .lock("update_settings")?
            .as_mut()
            .ok_or_else(not_initialized)?
            .update_settings(update)?;
        self.sync_active_project_revision(settings.project_revision())?;
        Ok(settings)
    }

    pub fn import_media(
        &self,
        request: DesktopMediaImportRequest,
    ) -> Result<DesktopMediaImportResult, DesktopProjectFailure> {
        let (result, project_id) = {
            let mut lifecycle = self.lock("import_media")?;
            let lifecycle = lifecycle.as_mut().ok_or_else(not_initialized)?;
            let result = lifecycle.import_media(request)?;
            let project_id = lifecycle
                .snapshot()
                .active()
                .ok_or_else(not_initialized)?
                .project_id()
                .to_owned();
            (result, project_id)
        };
        {
            let mut store = self.media_library_lock("import_media")?;
            store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(result.project_revision()))
                .retain_import(&result);
        }
        self.persist_media_libraries()?;
        Ok(result)
    }

    pub fn media_library(&self) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("media_library")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let mut store = self.media_library_lock("media_library")?;
        let library = store
            .projects
            .entry(project_id)
            .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
        library.project_revision = project_revision;
        library.refresh_derived();
        library.refresh_usage(&usage);
        Ok(library.clone())
    }

    pub fn generate_media_preview(
        &self,
        request: MediaPreviewRequest,
    ) -> Result<MediaPreviewBundle, DesktopProjectFailure> {
        let (project_id, project_revision) =
            self.active_project_identity("generate_media_preview")?;
        let item = {
            let store = self.media_library_lock("generate_media_preview")?;
            let library = store.projects.get(&project_id).ok_or_else(|| {
                media_library_invalid("Media library preview state is unavailable")
            })?;
            if request.expected_project_revision != project_revision
                || request.expected_project_revision != library.project_revision
                || request.expected_library_revision != library.revision
            {
                return Err(user_correctable(
                    "media_preview_revision_stale",
                    "Media preview request is stale",
                    "Refresh the media library and request the preview again.",
                ));
            }
            library
                .items
                .iter()
                .find(|item| item.media_id == request.media_id)
                .cloned()
                .ok_or_else(|| media_library_invalid("Imported media was not found"))?
        };
        if item.content_fingerprint != request.expected_freshness {
            return Err(user_correctable(
                "media_preview_freshness_stale",
                "Media preview source changed",
                "Refresh the media library and request the preview again.",
            ));
        }
        if item.source_monitoring.has_changed_source() {
            return Err(user_correctable(
                "media_preview_source_changed",
                "Media preview source has changed",
                "Review the changed source, then Relink or Replace it before generating a preview.",
            ));
        }
        Ok(media_preview::generate(&item))
    }

    pub fn mutate_media_library(
        &self,
        update: MediaLibraryUpdate,
    ) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("mutate_media_library")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let snapshot = {
            let mut store = self.media_library_lock("mutate_media_library")?;
            let library = store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
            library.project_revision = project_revision;
            library.apply(update)?;
            library.refresh_usage(&usage);
            library.clone()
        };
        self.persist_media_libraries()?;
        Ok(snapshot)
    }

    pub fn inspect_media_source(
        &self,
        request: SourceMetadataInspectionRequest,
    ) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("inspect_media_source")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let snapshot = {
            let mut store = self.media_library_lock("inspect_media_source")?;
            let library = store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
            library.project_revision = project_revision;
            library.inspect_source(request)?;
            library.refresh_usage(&usage);
            library.clone()
        };
        self.persist_media_libraries()?;
        Ok(snapshot)
    }

    pub fn mutate_user_metadata(
        &self,
        update: UserMetadataUpdate,
    ) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("mutate_user_metadata")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let snapshot = {
            let mut store = self.media_library_lock("mutate_user_metadata")?;
            let library = store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
            library.project_revision = project_revision;
            library.apply_user_metadata(update)?;
            library.refresh_usage(&usage);
            library.clone()
        };
        self.persist_media_libraries()?;
        Ok(snapshot)
    }

    pub fn mutate_annotations(
        &self,
        update: MediaAnnotationUpdate,
    ) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("mutate_media_annotations")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let snapshot = {
            let mut store = self.media_library_lock("mutate_media_annotations")?;
            let library = store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
            library.project_revision = project_revision;
            library.apply_annotations(update)?;
            library.refresh_usage(&usage);
            library.clone()
        };
        self.persist_media_libraries()?;
        Ok(snapshot)
    }

    pub fn mutate_media_identity(
        &self,
        update: MediaIdentityUpdate,
    ) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("mutate_media_identity")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let snapshot = {
            let mut store = self.media_library_lock("mutate_media_identity")?;
            let library = store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
            library.project_revision = project_revision;
            library.apply_identity_update(update)?;
            library.refresh_usage(&usage);
            library.clone()
        };
        self.persist_media_libraries()?;
        Ok(snapshot)
    }

    pub fn mutate_content_analysis(
        &self,
        update: MediaContentAnalysisUpdate,
    ) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("mutate_content_analysis")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let snapshot = {
            let mut store = self.media_library_lock("mutate_content_analysis")?;
            let library = store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
            library.project_revision = project_revision;
            library.apply_content_analysis(update)?;
            library.refresh_usage(&usage);
            library.clone()
        };
        self.persist_media_libraries()?;
        Ok(snapshot)
    }

    pub fn search_media_content(
        &self,
        request: MediaContentSearchRequest,
    ) -> Result<MediaContentSearchSnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("search_media_content")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let mut store = self.media_library_lock("search_media_content")?;
        let library = store
            .projects
            .entry(project_id)
            .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
        library.project_revision = project_revision;
        library.refresh_derived();
        library.refresh_usage(&usage);
        library.search_content(request)
    }

    pub fn mutate_derived_media(
        &self,
        update: DerivedMediaUpdate,
    ) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("mutate_derived_media")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let snapshot = {
            let mut store = self.media_library_lock("mutate_derived_media")?;
            let library = store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
            library.project_revision = project_revision;
            library.apply_derived_media(update)?;
            library.refresh_usage(&usage);
            library.clone()
        };
        self.persist_media_libraries()?;
        Ok(snapshot)
    }

    pub fn mutate_offline_media(
        &self,
        update: OfflineMediaUpdate,
    ) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("mutate_offline_media")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let snapshot = {
            let mut store = self.media_library_lock("mutate_offline_media")?;
            let library = store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
            library.project_revision = project_revision;
            library.apply_offline_media(update)?;
            library.refresh_usage(&usage);
            library.clone()
        };
        self.persist_media_libraries()?;
        Ok(snapshot)
    }

    pub fn scan_media_sources(
        &self,
        request: MediaSourceScanRequest,
    ) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
        let (project_id, project_revision, project_path) =
            self.active_project_context("scan_media_sources")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let snapshot = {
            let mut store = self.media_library_lock("scan_media_sources")?;
            let library = store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
            library.project_revision = project_revision;
            library.scan_media_sources(request)?;
            library.refresh_usage(&usage);
            library.clone()
        };
        self.persist_media_libraries()?;
        Ok(snapshot)
    }

    pub fn mutate_media_batch(
        &self,
        update: MediaBatchUpdate,
    ) -> Result<MediaBatchResult, DesktopProjectFailure> {
        let operation_count = update.operations.len();
        let (project_id, project_revision, project_path) =
            self.active_project_context("mutate_media_batch")?;
        let usage = inspect_media_usage(Path::new(&project_path))?;
        let path = self
            .media_library_path_lock("mutate_media_batch")?
            .clone()
            .ok_or_else(not_initialized)?;
        let (snapshot, affected_media_ids) = {
            let mut store = self.media_library_lock("mutate_media_batch")?;
            let mut candidate_store = store.clone();
            let library = candidate_store
                .projects
                .entry(project_id)
                .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision));
            library.project_revision = project_revision;
            let affected_media_ids = library.apply_media_batch(update)?;
            library.refresh_usage(&usage);
            let snapshot = library.clone();
            publish_media_library_store(&path, &candidate_store)?;
            *store = candidate_store;
            (snapshot, affected_media_ids)
        };
        Ok(MediaBatchResult {
            operation_count,
            affected_media_ids,
            snapshot,
        })
    }

    fn activate_media_library(
        &self,
        snapshot: &DesktopProjectSnapshot,
    ) -> Result<(), DesktopProjectFailure> {
        if let Some(active) = snapshot.active() {
            let mut store = self.media_library_lock("activate_media_library")?;
            let library = store
                .projects
                .entry(active.project_id().to_owned())
                .or_insert_with(|| MediaLibrarySnapshot::empty(active.project_revision()));
            library.project_revision = active.project_revision();
            library.refresh_derived();
            drop(store);
            self.persist_media_libraries()?;
        }
        Ok(())
    }

    fn sync_active_project_revision(
        &self,
        project_revision: u64,
    ) -> Result<(), DesktopProjectFailure> {
        let (project_id, _) = self.active_project_identity("sync_media_library")?;
        let mut store = self.media_library_lock("sync_media_library")?;
        store
            .projects
            .entry(project_id)
            .or_insert_with(|| MediaLibrarySnapshot::empty(project_revision))
            .project_revision = project_revision;
        drop(store);
        self.persist_media_libraries()
    }

    fn active_project_identity(
        &self,
        operation: &'static str,
    ) -> Result<(String, u64), DesktopProjectFailure> {
        let lifecycle = self.lock(operation)?;
        let active = lifecycle
            .as_ref()
            .ok_or_else(not_initialized)?
            .snapshot()
            .active()
            .ok_or_else(|| {
                user_correctable(
                    "project_not_open",
                    "No project is open",
                    "Create or open a project before continuing.",
                )
            })?;
        Ok((active.project_id().to_owned(), active.project_revision()))
    }

    fn active_project_context(
        &self,
        operation: &'static str,
    ) -> Result<(String, u64, String), DesktopProjectFailure> {
        let lifecycle = self.lock(operation)?;
        let active = lifecycle
            .as_ref()
            .ok_or_else(not_initialized)?
            .snapshot()
            .active()
            .ok_or_else(|| {
                user_correctable(
                    "project_not_open",
                    "No project is open",
                    "Create or open a project before continuing.",
                )
            })?;
        Ok((
            active.project_id().to_owned(),
            active.project_revision(),
            active.path().to_owned(),
        ))
    }

    fn persist_media_libraries(&self) -> Result<(), DesktopProjectFailure> {
        let path = self
            .media_library_path_lock("persist_media_library")?
            .clone()
            .ok_or_else(not_initialized)?;
        let bytes = {
            let store = self.media_library_lock("persist_media_library")?;
            serialize_media_library_store(&store)?
        };
        publish_media_library_bytes(&path, bytes)
    }

    fn lock(
        &self,
        operation: &'static str,
    ) -> Result<
        std::sync::MutexGuard<'_, Option<DesktopProjectLifecycle<LocalProjectBackend>>>,
        DesktopProjectFailure,
    > {
        self.lifecycle.lock().map_err(|_| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "project_lifecycle_poisoned",
                "Project lifecycle cannot continue",
                "Restart Superi before continuing.",
            )
            .with_context("operation", operation)
        })
    }

    fn media_library_lock(
        &self,
        operation: &'static str,
    ) -> Result<std::sync::MutexGuard<'_, MediaLibraryStore>, DesktopProjectFailure> {
        self.media_libraries.lock().map_err(|_| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_poisoned",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
            .with_context("operation", operation)
        })
    }

    fn media_library_path_lock(
        &self,
        operation: &'static str,
    ) -> Result<std::sync::MutexGuard<'_, Option<PathBuf>>, DesktopProjectFailure> {
        self.media_library_path.lock().map_err(|_| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_path_poisoned",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
            .with_context("operation", operation)
        })
    }
}

fn serialize_media_library_store(
    store: &MediaLibraryStore,
) -> Result<Vec<u8>, DesktopProjectFailure> {
    let mut value = serde_json::to_value(store).map_err(|_| media_library_serialize_failure())?;
    let projects = value
        .get_mut("projects")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(media_library_serialize_failure)?;
    for library in projects.values_mut() {
        let items = library
            .get_mut("items")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(media_library_serialize_failure)?;
        for item in items {
            let item = item
                .as_object_mut()
                .ok_or_else(media_library_serialize_failure)?;
            for derived in [
                "usage",
                "identity_tracking",
                "resolved_representation",
                "offline",
                "thumbnail",
            ] {
                item.remove(derived);
            }
        }
        let smart_collections = library
            .get_mut("smart_collections")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(media_library_serialize_failure)?;
        for collection in smart_collections {
            collection
                .as_object_mut()
                .ok_or_else(media_library_serialize_failure)?
                .remove("media_ids");
        }
    }
    serde_json::to_vec_pretty(&value).map_err(|_| media_library_serialize_failure())
}

fn publish_media_library_store(
    path: &Path,
    store: &MediaLibraryStore,
) -> Result<(), DesktopProjectFailure> {
    let bytes = serialize_media_library_store(store)?;
    publish_media_library_bytes(path, bytes)
}

fn publish_media_library_bytes(path: &Path, bytes: Vec<u8>) -> Result<(), DesktopProjectFailure> {
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, bytes)
        .map_err(|source| media_library_store_failure("write", source))?;
    std::fs::rename(&temporary, path)
        .map_err(|source| media_library_store_failure("publish", source))
}

fn media_library_serialize_failure() -> DesktopProjectFailure {
    DesktopProjectFailure::new(
        DesktopProjectFailureClass::Terminal,
        "media_library_store_serialize_failed",
        "Media library could not be saved",
        "Restart Superi before continuing.",
    )
}

fn media_library_store_failure(
    operation: &'static str,
    source: std::io::Error,
) -> DesktopProjectFailure {
    safe_failure(
        operation,
        Error::with_source(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "media library presentation could not be saved",
            source,
        ),
    )
}

#[tauri::command]
pub fn desktop_project_snapshot(
    state: State<'_, DesktopProjectState>,
) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
    state.snapshot()
}

#[tauri::command]
pub async fn desktop_project_execute(
    command: DesktopProjectCommand,
    state: State<'_, DesktopProjectState>,
) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.execute(command))
        .await
        .map_err(|_| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "project_task_failed",
                "Project operation could not finish",
                "Restart Superi before continuing.",
            )
            .with_context("operation", "join")
        })?
}

#[tauri::command]
pub async fn desktop_project_settings(
    state: State<'_, DesktopProjectState>,
) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.settings())
        .await
        .map_err(|_| project_task_failed("inspect_settings"))?
}

#[tauri::command]
pub async fn desktop_project_settings_update(
    update: DesktopProjectSettingsUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.update_settings(update))
        .await
        .map_err(|_| project_task_failed("update_settings"))?
}

#[tauri::command]
pub async fn desktop_project_media_import(
    request: DesktopMediaImportRequest,
    state: State<'_, DesktopProjectState>,
) -> Result<DesktopMediaImportResult, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.import_media(request))
        .await
        .map_err(|_| project_task_failed("import_media"))?
}

#[tauri::command]
pub async fn project_media_library(
    state: State<'_, DesktopProjectState>,
) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.media_library())
        .await
        .map_err(|_| project_task_failed("media_library"))?
}

#[tauri::command]
pub async fn desktop_generate_media_preview(
    request: MediaPreviewRequest,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaPreviewBundle, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.generate_media_preview(request))
        .await
        .map_err(|_| project_task_failed("generate_media_preview"))?
}

#[tauri::command]
pub async fn mutate_project_media_library(
    update: MediaLibraryUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.mutate_media_library(update))
        .await
        .map_err(|_| project_task_failed("mutate_media_library"))?
}

#[tauri::command]
pub async fn inspect_project_media_source(
    request: SourceMetadataInspectionRequest,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.inspect_media_source(request))
        .await
        .map_err(|_| project_task_failed("inspect_media_source"))?
}

#[tauri::command]
pub async fn mutate_project_media_metadata(
    update: UserMetadataUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.mutate_user_metadata(update))
        .await
        .map_err(|_| project_task_failed("mutate_user_metadata"))?
}

#[tauri::command]
pub async fn mutate_project_media_annotations(
    update: MediaAnnotationUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.mutate_annotations(update))
        .await
        .map_err(|_| project_task_failed("mutate_media_annotations"))?
}

#[tauri::command]
pub async fn mutate_project_media_identity(
    update: MediaIdentityUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.mutate_media_identity(update))
        .await
        .map_err(|_| project_task_failed("mutate_media_identity"))?
}

#[tauri::command]
pub async fn mutate_project_media_content_analysis(
    update: MediaContentAnalysisUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.mutate_content_analysis(update))
        .await
        .map_err(|_| project_task_failed("mutate_content_analysis"))?
}

#[tauri::command]
pub async fn search_project_media_content(
    request: MediaContentSearchRequest,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaContentSearchSnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.search_media_content(request))
        .await
        .map_err(|_| project_task_failed("search_media_content"))?
}

#[tauri::command]
pub async fn mutate_project_derived_media(
    update: DerivedMediaUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.mutate_derived_media(update))
        .await
        .map_err(|_| project_task_failed("mutate_derived_media"))?
}

#[tauri::command]
pub async fn mutate_project_offline_media(
    update: OfflineMediaUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.mutate_offline_media(update))
        .await
        .map_err(|_| project_task_failed("mutate_offline_media"))?
}

#[tauri::command]
pub async fn scan_project_media_sources(
    request: MediaSourceScanRequest,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaLibrarySnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.scan_media_sources(request))
        .await
        .map_err(|_| project_task_failed("scan_media_sources"))?
}

#[tauri::command]
pub async fn mutate_project_media_batch(
    update: MediaBatchUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<MediaBatchResult, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.mutate_media_batch(update))
        .await
        .map_err(|_| project_task_failed("mutate_media_batch"))?
}

/// Closed desktop command set for project lifecycle behavior only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DesktopProjectCommand {
    Create {
        path: String,
        project: DesktopProjectCreateRequest,
    },
    Open {
        path: String,
    },
    OpenRecent {
        path: String,
    },
    Save,
    SaveAs {
        destination: String,
        replace_existing: bool,
    },
    Close,
    DiscoverRecovery,
    RestoreRecovery {
        catalog_revision: u64,
        candidate_id: String,
    },
}

/// Sole mutable desktop project lifecycle owner.
pub struct DesktopProjectLifecycle<B> {
    backend: B,
    recent_capacity: usize,
    snapshot: DesktopProjectSnapshot,
}

impl<B: DesktopProjectBackend> DesktopProjectLifecycle<B> {
    pub fn new(backend: B, recent_capacity: usize) -> Result<Self, DesktopProjectFailure> {
        if recent_capacity == 0 || recent_capacity > MAX_RECENT_PROJECTS {
            return Err(user_correctable(
                "recent_project_capacity_invalid",
                "Recent-project capacity is invalid",
                "Use a recent-project capacity between 1 and 32.",
            ));
        }
        Ok(Self {
            backend,
            recent_capacity,
            snapshot: DesktopProjectSnapshot {
                revision: 0,
                active: None,
                recent: Vec::new(),
                recovery: None,
                failure: None,
            },
        })
    }

    #[must_use]
    pub const fn snapshot(&self) -> &DesktopProjectSnapshot {
        &self.snapshot
    }

    pub fn execute(
        &mut self,
        command: DesktopProjectCommand,
    ) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
        let result = match command {
            DesktopProjectCommand::Create { path, project } => {
                let path = validate_path(path, "create")?;
                self.backend
                    .create(Path::new(&path), &project)
                    .map(|identity| ProjectTransition::Activate { path, identity })
            }
            DesktopProjectCommand::Open { path } => {
                let path = validate_path(path, "open")?;
                self.backend
                    .open(Path::new(&path))
                    .map(|identity| ProjectTransition::Activate { path, identity })
            }
            DesktopProjectCommand::OpenRecent { path } => {
                let path = validate_path(path, "open_recent")?;
                if !self.snapshot.recent.iter().any(|entry| entry.path == path) {
                    Err(user_correctable(
                        "recent_project_missing",
                        "Recent project is unavailable",
                        "Choose a project that is still present in the recent-project list.",
                    ))
                } else {
                    self.backend
                        .open(Path::new(&path))
                        .map(|identity| ProjectTransition::Activate { path, identity })
                }
            }
            DesktopProjectCommand::Save => {
                let active = self.active_record("save")?.clone();
                self.backend
                    .save(Path::new(&active.path))
                    .map(|identity| ProjectTransition::RefreshActive { identity })
            }
            DesktopProjectCommand::SaveAs {
                destination,
                replace_existing,
            } => {
                let active = self.active_record("save_as")?.clone();
                let destination = validate_path(destination, "save_as")?;
                self.backend
                    .save_as(
                        Path::new(&active.path),
                        Path::new(&destination),
                        replace_existing,
                    )
                    .map(|identity| ProjectTransition::Activate {
                        path: destination,
                        identity,
                    })
            }
            DesktopProjectCommand::Close => Ok(ProjectTransition::Close),
            DesktopProjectCommand::DiscoverRecovery => {
                let active = self.active_record("discover_recovery")?.clone();
                self.backend
                    .discover_recovery(Path::new(&active.path))
                    .map(ProjectTransition::Recovery)
            }
            DesktopProjectCommand::RestoreRecovery {
                catalog_revision,
                candidate_id,
            } => {
                let active = self.active_record("restore_recovery")?.clone();
                let catalog = self.snapshot.recovery.as_ref().ok_or_else(|| {
                    user_correctable(
                        "recovery_catalog_missing",
                        "Recovery options are unavailable",
                        "Refresh recovery options before restoring a project.",
                    )
                })?;
                if catalog.catalog_revision != catalog_revision
                    || !catalog
                        .candidates
                        .iter()
                        .any(|candidate| candidate.candidate_id == candidate_id)
                {
                    Err(user_correctable(
                        "recovery_selection_stale",
                        "Recovery selection is stale",
                        "Refresh recovery options and choose the candidate again.",
                    ))
                } else {
                    self.backend
                        .restore_recovery(Path::new(&active.path), catalog_revision, &candidate_id)
                        .map(|identity| ProjectTransition::Restored { identity })
                }
            }
        };

        match result {
            Ok(transition) => {
                self.commit(transition)?;
                Ok(self.snapshot.clone())
            }
            Err(failure) => {
                self.record_failure(failure.clone());
                Err(failure)
            }
        }
    }

    fn active_record(
        &self,
        operation: &'static str,
    ) -> Result<&DesktopProjectRecord, DesktopProjectFailure> {
        self.snapshot.active.as_ref().ok_or_else(|| {
            user_correctable(
                "project_not_open",
                "No project is open",
                "Create or open a project before continuing.",
            )
            .with_context("operation", operation)
        })
    }

    fn commit(&mut self, transition: ProjectTransition) -> Result<(), DesktopProjectFailure> {
        match transition {
            ProjectTransition::Activate { path, identity } => {
                let record = DesktopProjectRecord::new(path, identity);
                self.snapshot.active = Some(record.clone());
                self.snapshot.recovery = None;
                self.push_recent(record);
            }
            ProjectTransition::RefreshActive { identity } => {
                let path = self.active_record("save")?.path.clone();
                let record = DesktopProjectRecord::new(path, identity);
                self.snapshot.active = Some(record.clone());
                if let Some(recent) = self
                    .snapshot
                    .recent
                    .iter_mut()
                    .find(|recent| recent.path == record.path)
                {
                    *recent = record;
                }
            }
            ProjectTransition::Close => {
                self.snapshot.active = None;
                self.snapshot.recovery = None;
            }
            ProjectTransition::Recovery(catalog) => {
                self.snapshot.recovery = Some(catalog);
            }
            ProjectTransition::Restored { identity } => {
                let path = self.active_record("restore_recovery")?.path.clone();
                let record = DesktopProjectRecord::new(path, identity);
                self.snapshot.active = Some(record.clone());
                self.snapshot.recovery = None;
                if let Some(recent) = self
                    .snapshot
                    .recent
                    .iter_mut()
                    .find(|recent| recent.path == record.path)
                {
                    *recent = record;
                }
            }
        }
        self.snapshot.failure = None;
        self.advance_revision()
    }

    fn push_recent(&mut self, record: DesktopProjectRecord) {
        self.snapshot
            .recent
            .retain(|recent| recent.path != record.path);
        self.snapshot.recent.insert(0, record);
        self.snapshot.recent.truncate(self.recent_capacity);
    }

    fn record_failure(&mut self, failure: DesktopProjectFailure) {
        self.snapshot.failure = Some(failure);
        if self.advance_revision().is_err() {
            self.snapshot.failure = Some(DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "project_lifecycle_revision_exhausted",
                "Project lifecycle cannot continue",
                "Restart Superi before continuing.",
            ));
        }
    }

    fn advance_revision(&mut self) -> Result<(), DesktopProjectFailure> {
        self.snapshot.revision = self.snapshot.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "project_lifecycle_revision_exhausted",
                "Project lifecycle cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        Ok(())
    }
}

impl DesktopProjectLifecycle<LocalProjectBackend> {
    /// Inspects settings for the currently active durable project.
    pub fn inspect_settings(&self) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
        let active = self.active_record("inspect_settings")?;
        self.backend.inspect_settings(Path::new(active.path()))
    }

    /// Applies one optimistic settings replacement and refreshes lifecycle revision identity.
    pub fn update_settings(
        &mut self,
        update: DesktopProjectSettingsUpdate,
    ) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
        let path = self.active_record("update_settings")?.path().to_owned();
        match self.backend.update_settings(Path::new(&path), update) {
            Ok((settings, identity)) => {
                self.commit(ProjectTransition::RefreshActive { identity })?;
                Ok(settings)
            }
            Err(failure) => {
                self.record_failure(failure.clone());
                Err(failure)
            }
        }
    }

    /// Discovers and imports media through the durable public project transaction.
    pub fn import_media(
        &mut self,
        request: DesktopMediaImportRequest,
    ) -> Result<DesktopMediaImportResult, DesktopProjectFailure> {
        let path = self.active_record("import_media")?.path().to_owned();
        match self.backend.import_media(Path::new(&path), request) {
            Ok((result, identity)) => {
                self.commit(ProjectTransition::RefreshActive { identity })?;
                Ok(result)
            }
            Err(failure) => {
                self.record_failure(failure.clone());
                Err(failure)
            }
        }
    }
}

#[derive(Clone)]
struct DiscoveredImportSource {
    desktop: DesktopImportedMedia,
    editor: EditorImportedMedia,
}

type ImageSequenceKey = (PathBuf, String, String, usize);
type ImageSequenceMembers = Vec<(i64, PathBuf)>;

fn discover_import_sources(
    request: &DesktopMediaImportRequest,
    frame_rate: (u32, u32),
) -> Result<Vec<DiscoveredImportSource>, DesktopProjectFailure> {
    let mut files = BTreeSet::new();
    for raw in &request.paths {
        if raw.trim().is_empty() || raw.contains('\0') {
            return Err(user_correctable(
                "media_import_path_invalid",
                "Media import path is invalid",
                "Choose an existing media file or folder.",
            ));
        }
        let path = std::fs::canonicalize(raw)
            .map_err(|source| import_io_failure("resolve_import_path", source))?;
        if path.is_dir() {
            collect_import_directory(&path, request.recursive, &mut files)?;
        } else if path.is_file() && supported_media_path(&path) {
            files.insert(path);
        }
        if files.len() > MAX_IMPORT_SOURCES {
            return Err(user_correctable(
                "media_import_too_large",
                "Media import contains too many sources",
                "Import no more than 4096 sources at once.",
            ));
        }
    }

    let mut sequence_groups: BTreeMap<ImageSequenceKey, ImageSequenceMembers> = BTreeMap::new();
    if request.detect_image_sequences {
        for path in &files {
            if let Some((prefix, width, frame)) = image_sequence_parts(path) {
                sequence_groups
                    .entry((
                        path.parent().unwrap_or(Path::new("")).to_path_buf(),
                        prefix,
                        path.extension()
                            .and_then(|value| value.to_str())
                            .unwrap_or_default()
                            .to_ascii_lowercase(),
                        width,
                    ))
                    .or_default()
                    .push((frame, path.clone()));
            }
        }
    }

    let mut consumed = BTreeSet::new();
    let mut sources = Vec::new();
    for ((_, prefix, extension, width), mut members) in sequence_groups {
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        let paths: Vec<_> = members.iter().map(|(_, path)| path.clone()).collect();
        consumed.extend(paths.iter().cloned());
        let first = members.first().expect("nonempty sequence").0;
        let last = members.last().expect("nonempty sequence").0;
        let name = format!("{prefix}[{first:0width$}-{last:0width$}].{extension}");
        sources.push(build_discovered_source(
            name,
            paths,
            DesktopImportedMediaKind::ImageSequence,
            Some((first, last)),
            Some(frame_rate),
        )?);
    }
    for path in files {
        if consumed.contains(&path) {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("media")
            .to_owned();
        sources.push(build_discovered_source(
            name,
            vec![path],
            DesktopImportedMediaKind::File,
            None,
            None,
        )?);
    }
    sources.sort_by(|left, right| left.desktop.media_id.cmp(&right.desktop.media_id));
    Ok(sources)
}

fn collect_import_directory(
    root: &Path,
    recursive: bool,
    files: &mut BTreeSet<PathBuf>,
) -> Result<(), DesktopProjectFailure> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries = std::fs::read_dir(&directory)
            .map_err(|source| import_io_failure("scan_import_folder", source))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| import_io_failure("scan_import_folder", source))?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let file_type = entry
                .file_type()
                .map_err(|source| import_io_failure("scan_import_folder", source))?;
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() && recursive {
                pending.push(path);
            } else if file_type.is_file() && supported_media_path(&path) {
                files.insert(
                    std::fs::canonicalize(path)
                        .map_err(|source| import_io_failure("scan_import_folder", source))?,
                );
            }
        }
    }
    Ok(())
}

fn supported_media_path(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "mov"
            | "mp4"
            | "mkv"
            | "webm"
            | "wav"
            | "flac"
            | "mp3"
            | "aac"
            | "png"
            | "jpg"
            | "jpeg"
            | "exr"
            | "tif"
            | "tiff"
            | "dpx"
    )
}

fn image_sequence_parts(path: &Path) -> Option<(String, usize, i64)> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "exr" | "tif" | "tiff" | "dpx"
    ) {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let digit_start = stem
        .char_indices()
        .rev()
        .take_while(|(_, character)| character.is_ascii_digit())
        .map(|(index, _)| index)
        .last()?;
    let digits = &stem[digit_start..];
    Some((
        stem[..digit_start].to_owned(),
        digits.len(),
        digits.parse().ok()?,
    ))
}

fn build_discovered_source(
    name: String,
    paths: Vec<PathBuf>,
    kind: DesktopImportedMediaKind,
    frame_range: Option<(i64, i64)>,
    frame_rate: Option<(u32, u32)>,
) -> Result<DiscoveredImportSource, DesktopProjectFailure> {
    let (fingerprint, source_fingerprints) = fingerprint_sources(&paths)?;
    let source_paths: Vec<_> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let media_id = stable_media_id(kind, &source_paths, &fingerprint).to_string();
    let editor_paths = source_paths
        .iter()
        .cloned()
        .map(native_editor_path)
        .collect();
    let source = match (kind, frame_range, frame_rate) {
        (DesktopImportedMediaKind::File, _, _) => EditorImportedMediaKind::File {},
        (DesktopImportedMediaKind::ImageSequence, Some((first, last)), Some((num, den))) => {
            EditorImportedMediaKind::ImageSequence {
                source_count: paths.len() as u64,
                first_frame: first,
                last_frame: last,
                frame_rate_numerator: num,
                frame_rate_denominator: den,
            }
        }
        _ => {
            return Err(DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_import_discovery_invalid",
                "Media discovery returned invalid state",
                "Restart Superi before importing again.",
            ))
        }
    };
    let desktop = DesktopImportedMedia {
        media_id: media_id.clone(),
        name: name.clone(),
        source_paths,
        content_fingerprint: fingerprint.clone(),
        kind,
        source_count: paths.len() as u64,
        first_frame: frame_range.map(|value| value.0),
        last_frame: frame_range.map(|value| value.1),
        frame_rate_numerator: frame_rate.map(|value| value.0),
        frame_rate_denominator: frame_rate.map(|value| value.1),
        source_fingerprints,
    };
    Ok(DiscoveredImportSource {
        desktop,
        editor: EditorImportedMedia {
            media_id,
            name,
            paths: editor_paths,
            content_fingerprint: fingerprint,
            source,
        },
    })
}

fn native_editor_path(path: String) -> EditorMediaPath {
    EditorMediaPath::Absolute {
        platform: if cfg!(windows) {
            EditorMediaPathPlatform::Windows
        } else {
            EditorMediaPathPlatform::Unix
        },
        path,
    }
}

fn fingerprint_sources(
    paths: &[PathBuf],
) -> Result<(String, Vec<MediaSourceFingerprint>), DesktopProjectFailure> {
    source_monitoring::fingerprint_sources(paths)
        .map_err(|source| import_io_failure("fingerprint_import_source", source))
}

fn stable_media_id(kind: DesktopImportedMediaKind, paths: &[String], fingerprint: &str) -> MediaId {
    let mut hasher = Sha256::new();
    hasher.update(match kind {
        DesktopImportedMediaKind::File => b"file".as_slice(),
        DesktopImportedMediaKind::ImageSequence => b"image_sequence".as_slice(),
    });
    for path in paths {
        hasher.update(path.as_bytes());
        hasher.update([0]);
    }
    hasher.update(fingerprint.as_bytes());
    let digest = hasher.finalize();
    let mut raw = [0_u8; 16];
    raw.copy_from_slice(&digest[..16]);
    let raw = u128::from_be_bytes(raw);
    MediaId::from_raw(if raw == 0 { 1 } else { raw })
}

fn import_permissions(
    sources: &[DiscoveredImportSource],
) -> Result<Arc<ApiPermissionContext>, DesktopProjectFailure> {
    let mut rules = Vec::new();
    for source in sources {
        for path in source.desktop.source_paths() {
            let target = ApiFilesystemPath::native(path.clone())
                .map_err(|error| safe_failure("authorize_media_import", error))?;
            rules.push(ApiPermissionRule::filesystem(
                ApiPermissionEffect::Allow,
                ApiFilesystemAccess::Read,
                ApiFilesystemScope::exact(target),
            ));
        }
    }
    ApiPermissionContext::new("superi.desktop.project", rules)
        .map(Arc::new)
        .map_err(|error| safe_failure("authorize_media_import", error))
}

fn present_import(
    execution: &LocalProjectExecution<ExecuteProjectCommandResult, ProjectStateChanged>,
    discovered: &[DiscoveredImportSource],
    automation: bool,
) -> Result<DesktopMediaImportResult, DesktopProjectFailure> {
    let (imported_ids, skipped_ids) = match execution.result().evidence() {
        ProjectCommandEvidence::Applied { actions } => actions
            .iter()
            .find_map(|action| match action {
                ProjectActionEvidence::MediaImported {
                    media_ids,
                    skipped_media_ids,
                } => Some((media_ids, skipped_media_ids)),
                _ => None,
            })
            .ok_or_else(|| {
                DesktopProjectFailure::new(
                    DesktopProjectFailureClass::Terminal,
                    "media_import_evidence_missing",
                    "Media import evidence is unavailable",
                    "Restart Superi before importing again.",
                )
            })?,
        _ => {
            return Err(DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_import_evidence_invalid",
                "Media import evidence is invalid",
                "Restart Superi before importing again.",
            ))
        }
    };
    let imported_set: BTreeSet<_> = imported_ids.iter().map(String::as_str).collect();
    Ok(DesktopMediaImportResult {
        project_revision: execution.result().state().project_revision(),
        imported: discovered
            .iter()
            .filter(|source| imported_set.contains(source.desktop.media_id()))
            .map(|source| source.desktop.clone())
            .collect(),
        skipped: skipped_ids.clone(),
        command_method: ExecuteProjectCommand::METHOD.to_owned(),
        event_name: ProjectStateChanged::NAME.to_owned(),
        event_sequence: execution
            .result()
            .authored_state_changed()
            .then(|| execution.events().last().map(ProjectStateChanged::sequence))
            .flatten(),
        automation_method: automation.then(|| ExecuteProjectCommand::METHOD.to_owned()),
    })
}

fn import_io_failure(operation: &'static str, source: std::io::Error) -> DesktopProjectFailure {
    safe_failure(
        operation,
        Error::with_source(
            ErrorCategory::Unavailable,
            Recoverability::UserCorrectable,
            "media import source could not be read",
            source,
        ),
    )
}

enum ProjectTransition {
    Activate {
        path: String,
        identity: DesktopProjectIdentity,
    },
    RefreshActive {
        identity: DesktopProjectIdentity,
    },
    Close,
    Recovery(DesktopRecoveryCatalog),
    Restored {
        identity: DesktopProjectIdentity,
    },
}

fn validate_path(path: String, operation: &'static str) -> Result<String, DesktopProjectFailure> {
    if path.trim().is_empty() || path.contains('\0') {
        return Err(user_correctable(
            "project_path_invalid",
            "Project path is invalid",
            "Choose a valid Superi project file.",
        )
        .with_context("operation", operation));
    }
    Ok(path)
}

fn user_correctable(
    code: &'static str,
    title: &'static str,
    action: &'static str,
) -> DesktopProjectFailure {
    DesktopProjectFailure::new(
        DesktopProjectFailureClass::UserCorrectable,
        code,
        title,
        action,
    )
}

fn safe_failure(operation: &'static str, error: Error) -> DesktopProjectFailure {
    let safe = UserSafeError::from_error(&error);
    let class = match safe.recoverability() {
        Recoverability::Retryable => DesktopProjectFailureClass::Retryable,
        Recoverability::Degraded => DesktopProjectFailureClass::Degraded,
        Recoverability::UserCorrectable => DesktopProjectFailureClass::UserCorrectable,
        Recoverability::Terminal => DesktopProjectFailureClass::Terminal,
        _ => DesktopProjectFailureClass::Terminal,
    };
    DesktopProjectFailure::new(class, safe.code(), safe.title(), safe.action())
        .with_context("operation", operation)
}

fn not_initialized() -> DesktopProjectFailure {
    DesktopProjectFailure::new(
        DesktopProjectFailureClass::Terminal,
        "project_lifecycle_not_initialized",
        "Project lifecycle is not ready",
        "Restart Superi before continuing.",
    )
}

fn project_task_failed(operation: &'static str) -> DesktopProjectFailure {
    DesktopProjectFailure::new(
        DesktopProjectFailureClass::Terminal,
        "project_task_failed",
        "Project operation could not finish",
        "Restart Superi before continuing.",
    )
    .with_context("operation", operation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn missing_imported_media() -> DesktopImportedMedia {
        DesktopImportedMedia {
            media_id: "media-c005".to_owned(),
            name: "offline.mov".to_owned(),
            source_paths: vec![std::env::temp_dir()
                .join(format!("superi-c005-missing-{}.mov", std::process::id()))
                .to_string_lossy()
                .into_owned()],
            content_fingerprint: "sha256:c005".to_owned(),
            kind: DesktopImportedMediaKind::File,
            source_count: 1,
            first_frame: None,
            last_frame: None,
            frame_rate_numerator: None,
            frame_rate_denominator: None,
            source_fingerprints: Vec::new(),
        }
    }

    #[test]
    fn source_inspection_and_user_metadata_preserve_media_identity_and_bin_intent() {
        let imported = missing_imported_media();
        let mut library = MediaLibrarySnapshot::empty(7);
        let mut item = MediaBrowserItem::from_imported(&imported);
        item.bin_id = Some("bin-a".to_owned());
        library.items.push(item);

        library
            .apply_user_metadata(UserMetadataUpdate {
                expected_project_revision: 7,
                expected_library_revision: 0,
                media_id: imported.media_id.clone(),
                mutation: UserMetadataMutation::Upsert {
                    key: "production.note".to_owned(),
                    value: "requires conform".to_owned(),
                },
            })
            .expect("generic user metadata should be editable");

        let identity = (
            library.items[0].media_id.clone(),
            library.items[0].content_fingerprint.clone(),
            library.items[0].source_paths.clone(),
            library.items[0].bin_id.clone(),
        );
        let revision = library.revision;
        let generation = library.items[0].source_metadata.inspection_generation;

        library
            .inspect_source(SourceMetadataInspectionRequest {
                media_id: imported.media_id.clone(),
                expected_freshness: imported.content_fingerprint.clone(),
            })
            .expect("fresh source identity should be inspectable");

        assert_eq!(library.revision, revision);
        assert_eq!(
            library.items[0].source_metadata.status,
            SourceMetadataStatus::Missing
        );
        assert_eq!(
            library.items[0].source_metadata.inspection_generation,
            generation + 1
        );
        assert_eq!(
            library.items[0].user_metadata.get("production.note"),
            Some(&"requires conform".to_owned())
        );
        assert_eq!(
            (
                library.items[0].media_id.clone(),
                library.items[0].content_fingerprint.clone(),
                library.items[0].source_paths.clone(),
                library.items[0].bin_id.clone(),
            ),
            identity
        );

        let encoded = serde_json::to_string(&library).expect("media library must serialize");
        let restored: MediaLibrarySnapshot =
            serde_json::from_str(&encoded).expect("media library must restore");
        assert_eq!(
            restored.items[0].source_metadata,
            library.items[0].source_metadata
        );
        assert_eq!(
            restored.items[0].user_metadata,
            library.items[0].user_metadata
        );
    }

    #[test]
    fn stale_inspection_and_later_annotation_keys_leave_metadata_unchanged() {
        let imported = missing_imported_media();
        let mut library = MediaLibrarySnapshot::empty(3);
        library
            .items
            .push(MediaBrowserItem::from_imported(&imported));
        let original = library.clone();

        assert!(library
            .inspect_source(SourceMetadataInspectionRequest {
                media_id: imported.media_id.clone(),
                expected_freshness: "sha256:stale".to_owned(),
            })
            .is_err());
        assert_eq!(library, original);

        assert!(library
            .apply_user_metadata(UserMetadataUpdate {
                expected_project_revision: 3,
                expected_library_revision: 0,
                media_id: imported.media_id,
                mutation: UserMetadataMutation::Upsert {
                    key: "rating".to_owned(),
                    value: "5".to_owned(),
                },
            })
            .is_err());
        assert_eq!(library, original);
    }

    #[test]
    fn content_analysis_is_editable_searchable_persistent_and_source_aware() {
        let imported = missing_imported_media();
        let mut library = MediaLibrarySnapshot::empty(11);
        let mut item = MediaBrowserItem::from_imported(&imported);
        item.user_metadata.insert(
            "production.location".to_owned(),
            "river overlook".to_owned(),
        );
        library.items.push(item);
        library.refresh_derived();

        let analysis = MediaContentAnalysis {
            source_fingerprint: imported.content_fingerprint.clone(),
            provenance: Some("local language pass".to_owned()),
            transcript_segments: vec![MediaTranscriptSegment {
                segment_id: "segment-b".to_owned(),
                text: "The planning meeting starts after sunset.".to_owned(),
                start_frame: 120,
                end_frame: 240,
                rate_numerator: 24,
                rate_denominator: 1,
                speaker: Some("Alice".to_owned()),
                timeline_relationships: vec![MediaTimelineRelationship {
                    timeline_id: "timeline-main".to_owned(),
                    clip_id: Some("clip-dialogue".to_owned()),
                }],
            }],
            local_ai_content: vec![MediaLocalAiContent {
                content_id: "topic-1".to_owned(),
                label: "Outdoor production".to_owned(),
                terms: vec!["Sunset".to_owned(), "river".to_owned()],
                segment_ids: vec!["segment-b".to_owned()],
            }],
        };
        library
            .apply_content_analysis(MediaContentAnalysisUpdate {
                expected_project_revision: 11,
                expected_library_revision: 0,
                media_id: imported.media_id.clone(),
                expected_source_fingerprint: imported.content_fingerprint.clone(),
                analysis,
            })
            .expect("ordinary content analysis should be editable");

        assert_eq!(library.revision, 1);
        assert_eq!(
            library.items[0].content_analysis.local_ai_content[0].terms,
            vec!["river".to_owned(), "Sunset".to_owned()]
        );

        let mixed = library
            .search_content(MediaContentSearchRequest {
                expected_project_revision: 11,
                expected_library_revision: 1,
                query: "river meeting alice".to_owned(),
                max_results: Some(20),
            })
            .expect("metadata, transcript, and local content should compose");
        assert_eq!(mixed.results.len(), 1);
        assert!(mixed.results[0].analysis_fresh);
        assert!(mixed.results[0]
            .matches
            .iter()
            .any(|evidence| evidence.signal == MediaContentSearchSignal::Metadata));
        let transcript = mixed.results[0]
            .matches
            .iter()
            .find(|evidence| evidence.signal == MediaContentSearchSignal::Transcript)
            .expect("search must return exact language evidence");
        assert_eq!(transcript.start_frame, Some(120));
        assert_eq!(transcript.end_frame, Some(240));
        assert_eq!(transcript.speaker.as_deref(), Some("Alice"));
        assert_eq!(transcript.timeline_relationships.len(), 1);
        assert!(mixed.results[0]
            .matches
            .iter()
            .any(|evidence| evidence.signal == MediaContentSearchSignal::LocalAi));

        let encoded = serde_json::to_string(&library).expect("analysis must serialize");
        let mut restored: MediaLibrarySnapshot =
            serde_json::from_str(&encoded).expect("analysis must restore without a model");
        restored.refresh_derived();
        assert_eq!(
            restored.items[0].content_analysis,
            library.items[0].content_analysis
        );
        assert_eq!(
            restored
                .search_content(MediaContentSearchRequest {
                    expected_project_revision: 11,
                    expected_library_revision: 1,
                    query: "sunset".to_owned(),
                    max_results: None,
                })
                .expect("restored analysis should remain searchable")
                .results
                .len(),
            1
        );

        let mut cleared = library.clone();
        cleared
            .apply_content_analysis(MediaContentAnalysisUpdate {
                expected_project_revision: 11,
                expected_library_revision: 1,
                media_id: imported.media_id.clone(),
                expected_source_fingerprint: imported.content_fingerprint.clone(),
                analysis: MediaContentAnalysis::default(),
            })
            .expect("editors should be able to clear ordinary analysis state");
        assert_eq!(
            cleared.items[0].content_analysis,
            MediaContentAnalysis::default()
        );

        let original = library.clone();
        let mut stale_source = original.items[0].content_analysis.clone();
        stale_source.source_fingerprint = "sha256:other".to_owned();
        assert!(library
            .apply_content_analysis(MediaContentAnalysisUpdate {
                expected_project_revision: 11,
                expected_library_revision: 1,
                media_id: imported.media_id.clone(),
                expected_source_fingerprint: imported.content_fingerprint.clone(),
                analysis: stale_source,
            })
            .is_err());
        assert_eq!(library, original);

        let mut invalid = original.items[0].content_analysis.clone();
        invalid.transcript_segments[0].rate_denominator = 0;
        assert!(library
            .apply_content_analysis(MediaContentAnalysisUpdate {
                expected_project_revision: 11,
                expected_library_revision: 1,
                media_id: imported.media_id.clone(),
                expected_source_fingerprint: imported.content_fingerprint.clone(),
                analysis: invalid,
            })
            .is_err());
        assert_eq!(library, original);

        library
            .apply_offline_media(OfflineMediaUpdate {
                expected_project_revision: 11,
                expected_library_revision: 1,
                media_id: imported.media_id,
                mutation: OfflineMediaMutation::Replace {
                    source_paths: vec!["/missing/replacement.mov".to_owned()],
                    replacement_fingerprint: "sha256:replacement".to_owned(),
                },
            })
            .expect("source replacement should retain inspectable analysis");
        let stale = library
            .search_content(MediaContentSearchRequest {
                expected_project_revision: 11,
                expected_library_revision: 2,
                query: "meeting".to_owned(),
                max_results: None,
            })
            .expect("stale analysis should remain inspectable without a model");
        assert_eq!(stale.results.len(), 1);
        assert!(!stale.results[0].analysis_fresh);
    }
}
