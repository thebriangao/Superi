//! Versioned deterministic timeline state documents and checked migration.
//!
//! This module owns the portable editorial state component used by a future
//! project container. It performs no file I/O. Complete editable state is
//! encoded in a strict JSON envelope, protected by a SHA-256 payload digest,
//! migrated in memory, and reconstructed through checked timeline APIs.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use superi_core::diagnostics::FiniteF64;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{
    BinId, CaptionId, ClipId, GapId, GeneratorId, MarkerId, MediaId, MulticamAngleId, ProjectId,
    SmartCollectionId, TimelineId, TrackId, TransitionId,
};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};

use crate::edit_state::{SelectionExpansion, SelectionUpdate};
use crate::markers::{
    Marker, MarkerFlag, MarkerLabel, MarkerNote, MarkerOwner, MetadataKey, MetadataOwner,
    MetadataValue, TimelineMetadata,
};
use crate::media::{
    LinkedMediaReference, MediaBin, MediaLibrary, MediaPredicate, RelinkStatus, SmartCollection,
    SmartCollectionMatch,
};
use crate::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting,
    AudioTrackSemantics, Caption, CaptionPurpose, CaptionTrackSemantics, Clip, ClipSource,
    DataSchema, DataTrackSemantics, EditorialObjectId, EditorialProject, Gap, Generator,
    LanguageTag, Timeline, Track, TrackItem, TrackSemantics, Transition, VideoCompositing,
    VideoTrackSemantics,
};
use crate::multicam::{
    MulticamAngle, MulticamAudioPolicy, MulticamClip, MulticamSource, MulticamSyncMethod,
};
use crate::retime::{ClipTimeMap, PlaybackRate, RetimeSegment};

const COMPONENT: &str = "superi-timeline.serialization";
const TIMELINE_STATE_FORMAT: &str = "superi.timeline";
const LEGACY_TIMELINE_STATE_FORMAT_REVISION: u32 = 0;
const MAX_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;

/// The current incompatible revision of the timeline state document contract.
pub const TIMELINE_STATE_FORMAT_REVISION: u32 = 1;

/// One fully checked timeline state load and its canonical current document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineStateLoad {
    project: EditorialProject,
    source_format_revision: u32,
    canonical_document: Vec<u8>,
}

impl TimelineStateLoad {
    /// Returns the reconstructed editable project.
    #[must_use]
    pub const fn project(&self) -> &EditorialProject {
        &self.project
    }

    /// Consumes the load result and returns the reconstructed editable project.
    #[must_use]
    pub fn into_project(self) -> EditorialProject {
        self.project
    }

    /// Returns the format revision found in the supplied document.
    #[must_use]
    pub const fn source_format_revision(&self) -> u32 {
        self.source_format_revision
    }

    /// Returns whether a supported older document was migrated during loading.
    #[must_use]
    pub const fn was_migrated(&self) -> bool {
        self.source_format_revision != TIMELINE_STATE_FORMAT_REVISION
    }

    /// Returns deterministic current-format bytes produced after validation.
    #[must_use]
    pub fn canonical_document(&self) -> &[u8] {
        &self.canonical_document
    }
}

/// Serializes one complete editable project into canonical timeline JSON bytes.
pub fn serialize_timeline_state(project: &EditorialProject) -> Result<Vec<u8>> {
    let mut payload = TimelineStatePayloadWire::from_project(project);
    payload.canonicalize();
    encode_current_document(&payload)
}

/// Loads, migrates, validates, and reconstructs one timeline state document.
pub fn deserialize_timeline_state(document: &[u8]) -> Result<TimelineStateLoad> {
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err(document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "decode_document",
            "timeline state document exceeds the supported size limit",
            [("bytes", document.len().to_string())],
        ));
    }
    let raw = serde_json::from_slice::<Value>(document).map_err(|source| {
        document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "decode_document",
            "timeline state document is not valid JSON",
            [("source", source.to_string())],
        )
    })?;
    let source_format_revision = inspect_header(&raw)?;
    let mut payload = match source_format_revision {
        TIMELINE_STATE_FORMAT_REVISION => {
            let envelope =
                serde_json::from_value::<CurrentEnvelopeWire>(raw).map_err(|source| {
                    document_error(
                        ErrorCategory::CorruptData,
                        Recoverability::UserCorrectable,
                        "decode_document",
                        "current timeline state document does not match its strict schema",
                        [("source", source.to_string())],
                    )
                })?;
            validate_current_envelope(&envelope)?;
            envelope.payload
        }
        LEGACY_TIMELINE_STATE_FORMAT_REVISION => {
            let envelope = serde_json::from_value::<LegacyEnvelopeWire>(raw).map_err(|source| {
                document_error(
                    ErrorCategory::CorruptData,
                    Recoverability::UserCorrectable,
                    "migrate_document",
                    "legacy timeline state document does not match its strict schema",
                    [("source", source.to_string())],
                )
            })?;
            if envelope.format != TIMELINE_STATE_FORMAT
                || envelope.format_revision != LEGACY_TIMELINE_STATE_FORMAT_REVISION
            {
                return Err(unsupported_format_error(
                    &envelope.format,
                    envelope.format_revision,
                ));
            }
            envelope.timeline_state
        }
        revision => return Err(unsupported_format_error(TIMELINE_STATE_FORMAT, revision)),
    };

    payload.canonicalize();
    let project = payload.into_project()?;
    let mut canonical_payload = TimelineStatePayloadWire::from_project(&project);
    canonical_payload.canonicalize();
    let canonical_document = encode_current_document(&canonical_payload)?;
    Ok(TimelineStateLoad {
        project,
        source_format_revision,
        canonical_document,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentEnvelopeWire {
    format: String,
    format_revision: u32,
    primitive_schema_revision: u32,
    payload_sha256: String,
    payload: TimelineStatePayloadWire,
}

#[derive(Serialize)]
struct CurrentEnvelopeRef<'a> {
    format: &'static str,
    format_revision: u32,
    primitive_schema_revision: u32,
    payload_sha256: String,
    payload: &'a TimelineStatePayloadWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyEnvelopeWire {
    format: String,
    format_revision: u32,
    timeline_state: TimelineStatePayloadWire,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimelineStatePayloadWire {
    project_id: ProjectId,
    name: String,
    revision: String,
    media: Vec<MediaWire>,
    media_library: MediaLibraryWire,
    timelines: Vec<TimelineWire>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaWire {
    id: MediaId,
    name: String,
    target: String,
    available_range: Option<TimeRange>,
    metadata: Vec<MetadataEntryWire>,
    relink_state: RelinkStateWire,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RelinkStateWire {
    status: RelinkStatusWire,
    expected_fingerprint: Option<String>,
    observed_fingerprint: Option<String>,
    rejected_target: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RelinkStatusWire {
    Online,
    Missing,
    Unverified,
    FingerprintMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaLibraryWire {
    bins: Vec<MediaBinWire>,
    smart_collections: Vec<SmartCollectionWire>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaBinWire {
    id: BinId,
    name: String,
    parent: Option<BinId>,
    media_ids: Vec<MediaId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SmartCollectionWire {
    id: SmartCollectionId,
    name: String,
    match_mode: SmartCollectionMatchWire,
    predicates: Vec<MediaPredicateWire>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SmartCollectionMatchWire {
    All,
    Any,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum MediaPredicateWire {
    NameContains {
        value: String,
    },
    TargetContains {
        value: String,
    },
    MetadataExists {
        key: String,
    },
    MetadataEquals {
        key: String,
        value: MetadataValueWire,
    },
    RelinkStatus {
        status: RelinkStatusWire,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimelineWire {
    id: TimelineId,
    name: String,
    edit_rate: Timebase,
    global_start: RationalTime,
    tracks: Vec<TrackWire>,
    edit_state: EditStateWire,
    snapping_enabled: bool,
    markers: Vec<MarkerWire>,
    metadata: Vec<OwnedMetadataWire>,
    multicam_source: Option<MulticamSourceWire>,
    multicam_clips: Vec<MulticamClipWire>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackWire {
    id: TrackId,
    name: String,
    semantics: TrackSemanticsWire,
    items: Vec<TrackItemWire>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TrackSemanticsWire {
    Video {
        frame_rate: FrameRate,
        compositing: VideoCompositingWire,
    },
    Audio {
        sample_rate: u32,
        channel_layout: ChannelLayout,
        routing: AudioRoutingWire,
    },
    Caption {
        timebase: Timebase,
        language: String,
        purpose: CaptionPurposeWire,
    },
    Data {
        timebase: Timebase,
        scheme_id_uri: String,
        value: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum VideoCompositingWire {
    Over,
    Replace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CaptionPurposeWire {
    Captions,
    Subtitles,
    Descriptions,
    Chapters,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioRoutingWire {
    destination: AudioRouteDestinationWire,
    destination_layout: ChannelLayout,
    routes: Vec<AudioChannelRouteWire>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "track_id", rename_all = "snake_case")]
enum AudioRouteDestinationWire {
    Main,
    Track(TrackId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioChannelRouteWire {
    source: ChannelPosition,
    target: AudioChannelTargetWire,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "channel", rename_all = "snake_case")]
enum AudioChannelTargetWire {
    Channel(ChannelPosition),
    Muted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TrackItemWire {
    Clip {
        id: ClipId,
        name: String,
        source: ClipSourceWire,
        source_range: TimeRange,
        record_range: TimeRange,
        time_map: ClipTimeMapWire,
    },
    Gap {
        id: GapId,
        name: String,
        record_range: TimeRange,
    },
    Transition {
        id: TransitionId,
        name: String,
        from: EditorialObjectIdWire,
        to: EditorialObjectIdWire,
        from_offset: Duration,
        to_offset: Duration,
    },
    Generator {
        id: GeneratorId,
        name: String,
        generator_kind: String,
        parameters: BTreeMap<String, String>,
        record_range: TimeRange,
    },
    Caption {
        id: CaptionId,
        name: String,
        text: String,
        language: Option<String>,
        record_range: TimeRange,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
enum EditorialObjectIdWire {
    Clip(ClipId),
    Gap(GapId),
    Transition(TransitionId),
    Generator(GeneratorId),
    Caption(CaptionId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
enum ClipSourceWire {
    Media(MediaId),
    Timeline(TimelineId),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipTimeMapWire {
    record_duration: Duration,
    source_timebase: Timebase,
    segments: Vec<RetimeSegmentWire>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetimeSegmentWire {
    record_range: TimeRange,
    source_start: RationalTime,
    rate_numerator: String,
    rate_denominator: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditStateWire {
    selected_objects: Vec<EditorialObjectIdWire>,
    track_states: Vec<TrackEditStateWire>,
    linked_selection_enabled: bool,
    links: Vec<Vec<ClipId>>,
    groups: Vec<Vec<ClipId>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackEditStateWire {
    track_id: TrackId,
    targeted: bool,
    sync_locked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkerWire {
    id: MarkerId,
    owner: MarkerOwnerWire,
    marked_range: TimeRange,
    label: Option<String>,
    flag: Option<MarkerFlagWire>,
    note: Option<String>,
    metadata: Vec<MetadataEntryWire>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
enum MarkerOwnerWire {
    Timeline,
    Track(TrackId),
    Object(EditorialObjectIdWire),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MarkerFlagWire {
    Red,
    Pink,
    Orange,
    Yellow,
    Green,
    Cyan,
    Blue,
    Purple,
    Magenta,
    Black,
    White,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedMetadataWire {
    owner: MetadataOwnerWire,
    entries: Vec<MetadataEntryWire>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
enum MetadataOwnerWire {
    Timeline,
    Track(TrackId),
    Object(EditorialObjectIdWire),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataEntryWire {
    key: String,
    value: MetadataValueWire,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum MetadataValueWire {
    Null,
    Boolean { value: bool },
    Signed { value: String },
    Unsigned { value: String },
    Float { value: FiniteF64 },
    Text { value: String },
    Time { value: RationalTime },
    Range { value: TimeRange },
    List { values: Vec<MetadataValueWire> },
    Map { entries: Vec<MetadataEntryWire> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MulticamSourceWire {
    sync_method: MulticamSyncMethodWire,
    angles: Vec<MulticamAngleWire>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum MulticamSyncMethodWire {
    Manual,
    Timecode,
    InPoints,
    OutPoints,
    ClipMarker { name: String },
    Audio,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MulticamAngleWire {
    id: MulticamAngleId,
    name: String,
    camera_label: String,
    enabled: bool,
    metadata: Vec<MetadataEntryWire>,
    source_clips: Vec<ClipId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MulticamClipWire {
    clip_id: ClipId,
    switches: Vec<MulticamSwitchWire>,
    audio_policy: MulticamAudioPolicyWire,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MulticamSwitchWire {
    source_range: TimeRange,
    angle_id: MulticamAngleId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "angle_id", rename_all = "snake_case")]
enum MulticamAudioPolicyWire {
    FollowVideo,
    Fixed(MulticamAngleId),
    AllAngles,
}

impl TimelineStatePayloadWire {
    fn from_project(project: &EditorialProject) -> Self {
        Self {
            project_id: project.id(),
            name: project.name().to_owned(),
            revision: project.revision().to_string(),
            media: project
                .media_references()
                .map(MediaWire::from_media)
                .collect(),
            media_library: MediaLibraryWire::from_library(project.media_library()),
            timelines: project
                .timelines()
                .map(TimelineWire::from_timeline)
                .collect(),
        }
    }

    fn canonicalize(&mut self) {
        self.media.sort_by_key(|media| media.id);
        for media in &mut self.media {
            canonicalize_metadata(&mut media.metadata);
        }
        self.media_library.canonicalize();
        self.timelines.sort_by_key(|timeline| timeline.id);
        for timeline in &mut self.timelines {
            timeline.canonicalize();
        }
    }

    fn into_project(self) -> Result<EditorialProject> {
        let revision = parse_u64(&self.revision, "decode_project_revision")?;
        let media = self
            .media
            .into_iter()
            .map(MediaWire::into_media)
            .collect::<Result<Vec<_>>>()?;
        let library = self.media_library.into_library()?;
        let timelines = self
            .timelines
            .into_iter()
            .map(TimelineWire::into_timeline)
            .collect::<Result<Vec<_>>>()?;
        let mut project = EditorialProject::new(self.project_id, self.name, media, timelines)?;
        project.restore_persisted_state(revision, library)?;
        Ok(project)
    }
}

impl MediaWire {
    fn from_media(media: &LinkedMediaReference) -> Self {
        Self {
            id: media.id(),
            name: media.name().to_owned(),
            target: media.target().to_owned(),
            available_range: media.available_range(),
            metadata: metadata_to_wire(media.metadata()),
            relink_state: RelinkStateWire {
                status: RelinkStatusWire::from(media.relink_state().status()),
                expected_fingerprint: media
                    .relink_state()
                    .expected_fingerprint()
                    .map(str::to_owned),
                observed_fingerprint: media
                    .relink_state()
                    .observed_fingerprint()
                    .map(str::to_owned),
                rejected_target: media.relink_state().rejected_target().map(str::to_owned),
            },
        }
    }

    fn into_media(self) -> Result<LinkedMediaReference> {
        let mut media =
            LinkedMediaReference::new(self.id, self.name, self.target, self.available_range);
        *media.metadata_mut() = metadata_from_wire(self.metadata)?;
        media.restore_relink_state(
            self.relink_state.status.into(),
            self.relink_state.expected_fingerprint,
            self.relink_state.observed_fingerprint,
            self.relink_state.rejected_target,
        )?;
        media.validate()?;
        Ok(media)
    }
}

impl From<RelinkStatus> for RelinkStatusWire {
    fn from(value: RelinkStatus) -> Self {
        match value {
            RelinkStatus::Online => Self::Online,
            RelinkStatus::Missing => Self::Missing,
            RelinkStatus::Unverified => Self::Unverified,
            RelinkStatus::FingerprintMismatch => Self::FingerprintMismatch,
        }
    }
}

impl From<RelinkStatusWire> for RelinkStatus {
    fn from(value: RelinkStatusWire) -> Self {
        match value {
            RelinkStatusWire::Online => Self::Online,
            RelinkStatusWire::Missing => Self::Missing,
            RelinkStatusWire::Unverified => Self::Unverified,
            RelinkStatusWire::FingerprintMismatch => Self::FingerprintMismatch,
        }
    }
}

impl MediaLibraryWire {
    fn from_library(library: &MediaLibrary) -> Self {
        Self {
            bins: library
                .bins()
                .map(|bin| MediaBinWire {
                    id: bin.id(),
                    name: bin.name().to_owned(),
                    parent: bin.parent(),
                    media_ids: bin.media_ids().to_vec(),
                })
                .collect(),
            smart_collections: library
                .smart_collections()
                .map(SmartCollectionWire::from_collection)
                .collect(),
        }
    }

    fn canonicalize(&mut self) {
        self.bins.sort_by_key(|bin| bin.id);
        for bin in &mut self.bins {
            bin.media_ids.sort_unstable();
        }
        self.smart_collections
            .sort_by_key(|collection| collection.id);
        for collection in &mut self.smart_collections {
            for predicate in &mut collection.predicates {
                predicate.canonicalize();
            }
        }
    }

    fn into_library(self) -> Result<MediaLibrary> {
        let mut library = MediaLibrary::new();
        for wire in self.bins {
            let mut bin = MediaBin::new(wire.id, wire.name, wire.parent)?;
            for media_id in wire.media_ids {
                if !bin.add_media(media_id) {
                    return Err(document_error(
                        ErrorCategory::CorruptData,
                        Recoverability::UserCorrectable,
                        "decode_media_library",
                        "media bin contains a duplicate media identity",
                        [("media", media_id.to_string())],
                    ));
                }
            }
            if library.upsert_bin(bin).is_some() {
                return Err(duplicate_document_identity("media bin", wire.id));
            }
        }
        for wire in self.smart_collections {
            let collection = wire.into_collection()?;
            let id = collection.id();
            if library.upsert_smart_collection(collection).is_some() {
                return Err(duplicate_document_identity("smart collection", id));
            }
        }
        Ok(library)
    }
}

impl SmartCollectionWire {
    fn from_collection(collection: &SmartCollection) -> Self {
        Self {
            id: collection.id(),
            name: collection.name().to_owned(),
            match_mode: match collection.match_mode() {
                SmartCollectionMatch::All => SmartCollectionMatchWire::All,
                SmartCollectionMatch::Any => SmartCollectionMatchWire::Any,
            },
            predicates: collection
                .predicates()
                .iter()
                .map(MediaPredicateWire::from_predicate)
                .collect(),
        }
    }

    fn into_collection(self) -> Result<SmartCollection> {
        SmartCollection::new(
            self.id,
            self.name,
            match self.match_mode {
                SmartCollectionMatchWire::All => SmartCollectionMatch::All,
                SmartCollectionMatchWire::Any => SmartCollectionMatch::Any,
            },
            self.predicates
                .into_iter()
                .map(MediaPredicateWire::into_predicate)
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

impl MediaPredicateWire {
    fn from_predicate(predicate: &MediaPredicate) -> Self {
        match predicate {
            MediaPredicate::NameContains(value) => Self::NameContains {
                value: value.clone(),
            },
            MediaPredicate::TargetContains(value) => Self::TargetContains {
                value: value.clone(),
            },
            MediaPredicate::MetadataExists(key) => Self::MetadataExists {
                key: key.as_str().to_owned(),
            },
            MediaPredicate::MetadataEquals { key, value } => Self::MetadataEquals {
                key: key.as_str().to_owned(),
                value: MetadataValueWire::from_value(value),
            },
            MediaPredicate::RelinkStatus(status) => Self::RelinkStatus {
                status: (*status).into(),
            },
        }
    }

    fn canonicalize(&mut self) {
        if let Self::MetadataEquals { value, .. } = self {
            value.canonicalize();
        }
    }

    fn into_predicate(self) -> Result<MediaPredicate> {
        Ok(match self {
            Self::NameContains { value } => MediaPredicate::NameContains(value),
            Self::TargetContains { value } => MediaPredicate::TargetContains(value),
            Self::MetadataExists { key } => MediaPredicate::MetadataExists(MetadataKey::new(key)?),
            Self::MetadataEquals { key, value } => MediaPredicate::MetadataEquals {
                key: MetadataKey::new(key)?,
                value: value.into_value()?,
            },
            Self::RelinkStatus { status } => MediaPredicate::RelinkStatus(status.into()),
        })
    }
}

impl TimelineWire {
    fn from_timeline(timeline: &Timeline) -> Self {
        let edit_state = timeline.edit_state();
        let track_states = timeline
            .tracks()
            .iter()
            .map(|track| {
                let state = edit_state
                    .track_state(track.id())
                    .expect("validated timeline has track edit state");
                TrackEditStateWire {
                    track_id: state.track_id(),
                    targeted: state.targeted(),
                    sync_locked: state.sync_locked(),
                }
            })
            .collect();
        let mut metadata = Vec::new();
        if let Some(entries) = timeline.metadata(MetadataOwner::Timeline) {
            metadata.push(OwnedMetadataWire {
                owner: MetadataOwnerWire::Timeline,
                entries: metadata_to_wire(entries),
            });
        }
        for track in timeline.tracks() {
            if let Some(entries) = timeline.metadata(MetadataOwner::Track(track.id())) {
                metadata.push(OwnedMetadataWire {
                    owner: MetadataOwnerWire::Track(track.id()),
                    entries: metadata_to_wire(entries),
                });
            }
            for item in track.items() {
                if let Some(entries) = timeline.metadata(MetadataOwner::Object(item.id())) {
                    metadata.push(OwnedMetadataWire {
                        owner: MetadataOwnerWire::Object(item.id().into()),
                        entries: metadata_to_wire(entries),
                    });
                }
            }
        }
        Self {
            id: timeline.id(),
            name: timeline.name().to_owned(),
            edit_rate: timeline.edit_rate(),
            global_start: timeline.global_start(),
            tracks: timeline
                .tracks()
                .iter()
                .map(TrackWire::from_track)
                .collect(),
            edit_state: EditStateWire {
                selected_objects: edit_state.selected_objects().map(Into::into).collect(),
                track_states,
                linked_selection_enabled: edit_state.linked_selection_enabled(),
                links: edit_state
                    .links()
                    .map(|relation| relation.members().collect())
                    .collect(),
                groups: edit_state
                    .groups()
                    .map(|relation| relation.members().collect())
                    .collect(),
            },
            snapping_enabled: timeline.snapping_enabled(),
            markers: timeline.markers().map(MarkerWire::from_marker).collect(),
            metadata,
            multicam_source: timeline
                .multicam_source()
                .map(MulticamSourceWire::from_source),
            multicam_clips: timeline
                .multicam_clips()
                .map(MulticamClipWire::from_clip)
                .collect(),
        }
    }

    fn canonicalize(&mut self) {
        self.edit_state.selected_objects.sort_unstable();
        self.edit_state
            .track_states
            .sort_by_key(|state| state.track_id);
        canonicalize_relations(&mut self.edit_state.links);
        canonicalize_relations(&mut self.edit_state.groups);
        self.markers.sort_by_key(|marker| marker.id);
        for marker in &mut self.markers {
            canonicalize_metadata(&mut marker.metadata);
        }
        self.metadata.sort_by_key(|entry| entry.owner);
        for entry in &mut self.metadata {
            canonicalize_metadata(&mut entry.entries);
        }
        if let Some(source) = &mut self.multicam_source {
            for angle in &mut source.angles {
                canonicalize_metadata(&mut angle.metadata);
            }
        }
        self.multicam_clips.sort_by_key(|clip| clip.clip_id);
    }

    fn into_timeline(self) -> Result<Timeline> {
        let tracks = self
            .tracks
            .into_iter()
            .map(TrackWire::into_track)
            .collect::<Result<Vec<_>>>()?;
        let mut timeline = Timeline::new(
            self.id,
            self.name,
            self.edit_rate,
            self.global_start,
            tracks,
        );
        timeline.set_snapping_enabled(self.snapping_enabled);

        let mut marker_ids = BTreeSet::new();
        for marker in self.markers {
            let id = marker.id;
            if !marker_ids.insert(id) {
                return Err(duplicate_document_identity("marker", id));
            }
            timeline.upsert_marker(marker.into_marker()?)?;
        }
        let mut metadata_owners = BTreeSet::new();
        for metadata in self.metadata {
            let owner = metadata.owner.into();
            if !metadata_owners.insert(owner) {
                return Err(duplicate_document_identity(
                    "timeline metadata owner",
                    owner,
                ));
            }
            timeline.set_metadata(owner, metadata_from_wire(metadata.entries)?)?;
        }

        timeline.set_linked_selection_enabled(self.edit_state.linked_selection_enabled);
        let mut track_state_ids = BTreeSet::new();
        for state in self.edit_state.track_states {
            if !track_state_ids.insert(state.track_id) {
                return Err(duplicate_document_identity(
                    "track edit state",
                    state.track_id,
                ));
            }
            timeline.set_track_targeted(state.track_id, state.targeted)?;
            timeline.set_track_sync_locked(state.track_id, state.sync_locked)?;
        }
        if track_state_ids.len() != timeline.tracks().len() {
            return Err(document_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "decode_edit_state",
                "timeline document must retain one edit state for every track",
                std::iter::empty::<(&str, String)>(),
            ));
        }
        validate_relations(&self.edit_state.links, "clip link")?;
        validate_relations(&self.edit_state.groups, "clip group")?;
        for relation in self.edit_state.links {
            timeline.link_clips(relation)?;
        }
        for relation in self.edit_state.groups {
            timeline.group_clips(relation)?;
        }
        ensure_unique(
            &self.edit_state.selected_objects,
            "selected editorial object",
        )?;
        timeline.update_selection(
            self.edit_state.selected_objects.into_iter().map(Into::into),
            SelectionUpdate::Replace,
            SelectionExpansion::Direct,
        )?;

        if let Some(source) = self.multicam_source {
            timeline.set_multicam_source(source.into_source()?)?;
        }
        let mut multicam_clip_ids = BTreeSet::new();
        for clip in self.multicam_clips {
            let id = clip.clip_id;
            if !multicam_clip_ids.insert(id) {
                return Err(duplicate_document_identity("multicam clip", id));
            }
            timeline.upsert_multicam_clip(clip.into_clip()?)?;
        }
        Ok(timeline)
    }
}

impl TrackWire {
    fn from_track(track: &Track) -> Self {
        Self {
            id: track.id(),
            name: track.name().to_owned(),
            semantics: TrackSemanticsWire::from_semantics(track.semantics()),
            items: track.items().iter().map(TrackItemWire::from_item).collect(),
        }
    }

    fn into_track(self) -> Result<Track> {
        Ok(Track::new(
            self.id,
            self.name,
            self.semantics.into_semantics()?,
            self.items
                .into_iter()
                .map(TrackItemWire::into_item)
                .collect::<Result<Vec<_>>>()?,
        ))
    }
}

impl TrackSemanticsWire {
    fn from_semantics(semantics: &TrackSemantics) -> Self {
        match semantics {
            TrackSemantics::Video(value) => Self::Video {
                frame_rate: value.frame_rate(),
                compositing: value.compositing().into(),
            },
            TrackSemantics::Audio(value) => Self::Audio {
                sample_rate: value.sample_rate(),
                channel_layout: value.channel_layout().clone(),
                routing: AudioRoutingWire::from_routing(value.routing()),
            },
            TrackSemantics::Caption(value) => Self::Caption {
                timebase: value.timebase(),
                language: value.language().as_str().to_owned(),
                purpose: value.purpose().into(),
            },
            TrackSemantics::Data(value) => Self::Data {
                timebase: value.timebase(),
                scheme_id_uri: value.schema().scheme_id_uri().to_owned(),
                value: value.schema().value().map(str::to_owned),
            },
        }
    }

    fn into_semantics(self) -> Result<TrackSemantics> {
        Ok(match self {
            Self::Video {
                frame_rate,
                compositing,
            } => TrackSemantics::Video(VideoTrackSemantics::new(frame_rate, compositing.into())),
            Self::Audio {
                sample_rate,
                channel_layout,
                routing,
            } => TrackSemantics::Audio(AudioTrackSemantics::new(
                sample_rate,
                channel_layout,
                routing.into_routing()?,
            )?),
            Self::Caption {
                timebase,
                language,
                purpose,
            } => TrackSemantics::Caption(CaptionTrackSemantics::new(
                timebase,
                LanguageTag::new(language)?,
                purpose.into(),
            )),
            Self::Data {
                timebase,
                scheme_id_uri,
                value,
            } => TrackSemantics::Data(DataTrackSemantics::new(
                timebase,
                DataSchema::new(scheme_id_uri, value.as_deref())?,
            )),
        })
    }
}

impl From<VideoCompositing> for VideoCompositingWire {
    fn from(value: VideoCompositing) -> Self {
        match value {
            VideoCompositing::Over => Self::Over,
            VideoCompositing::Replace => Self::Replace,
        }
    }
}

impl From<VideoCompositingWire> for VideoCompositing {
    fn from(value: VideoCompositingWire) -> Self {
        match value {
            VideoCompositingWire::Over => Self::Over,
            VideoCompositingWire::Replace => Self::Replace,
        }
    }
}

impl From<CaptionPurpose> for CaptionPurposeWire {
    fn from(value: CaptionPurpose) -> Self {
        match value {
            CaptionPurpose::Captions => Self::Captions,
            CaptionPurpose::Subtitles => Self::Subtitles,
            CaptionPurpose::Descriptions => Self::Descriptions,
            CaptionPurpose::Chapters => Self::Chapters,
        }
    }
}

impl From<CaptionPurposeWire> for CaptionPurpose {
    fn from(value: CaptionPurposeWire) -> Self {
        match value {
            CaptionPurposeWire::Captions => Self::Captions,
            CaptionPurposeWire::Subtitles => Self::Subtitles,
            CaptionPurposeWire::Descriptions => Self::Descriptions,
            CaptionPurposeWire::Chapters => Self::Chapters,
        }
    }
}

impl AudioRoutingWire {
    fn from_routing(routing: &AudioRouting) -> Self {
        Self {
            destination: routing.destination().into(),
            destination_layout: routing.destination_layout().clone(),
            routes: routing
                .routes()
                .iter()
                .copied()
                .map(|route| AudioChannelRouteWire {
                    source: route.source(),
                    target: route.target().into(),
                })
                .collect(),
        }
    }

    fn into_routing(self) -> Result<AudioRouting> {
        AudioRouting::new(
            self.destination.into(),
            self.destination_layout,
            self.routes
                .into_iter()
                .map(|route| AudioChannelRoute::new(route.source, route.target.into())),
        )
    }
}

impl From<AudioRouteDestination> for AudioRouteDestinationWire {
    fn from(value: AudioRouteDestination) -> Self {
        match value {
            AudioRouteDestination::Main => Self::Main,
            AudioRouteDestination::Track(id) => Self::Track(id),
        }
    }
}

impl From<AudioRouteDestinationWire> for AudioRouteDestination {
    fn from(value: AudioRouteDestinationWire) -> Self {
        match value {
            AudioRouteDestinationWire::Main => Self::Main,
            AudioRouteDestinationWire::Track(id) => Self::Track(id),
        }
    }
}

impl From<AudioChannelTarget> for AudioChannelTargetWire {
    fn from(value: AudioChannelTarget) -> Self {
        match value {
            AudioChannelTarget::Channel(channel) => Self::Channel(channel),
            AudioChannelTarget::Muted => Self::Muted,
        }
    }
}

impl From<AudioChannelTargetWire> for AudioChannelTarget {
    fn from(value: AudioChannelTargetWire) -> Self {
        match value {
            AudioChannelTargetWire::Channel(channel) => Self::Channel(channel),
            AudioChannelTargetWire::Muted => Self::Muted,
        }
    }
}

impl TrackItemWire {
    fn from_item(item: &TrackItem) -> Self {
        match item {
            TrackItem::Clip(clip) => Self::Clip {
                id: clip.id(),
                name: clip.name().to_owned(),
                source: clip.source().into(),
                source_range: clip.source_range(),
                record_range: clip.record_range(),
                time_map: ClipTimeMapWire::from_time_map(clip.time_map()),
            },
            TrackItem::Gap(gap) => Self::Gap {
                id: gap.id(),
                name: gap.name().to_owned(),
                record_range: gap.record_range(),
            },
            TrackItem::Transition(transition) => Self::Transition {
                id: transition.id(),
                name: transition.name().to_owned(),
                from: transition.from().into(),
                to: transition.to().into(),
                from_offset: transition.from_offset(),
                to_offset: transition.to_offset(),
            },
            TrackItem::Generator(generator) => Self::Generator {
                id: generator.id(),
                name: generator.name().to_owned(),
                generator_kind: generator.kind().to_owned(),
                parameters: generator.parameters().clone(),
                record_range: generator.record_range(),
            },
            TrackItem::Caption(caption) => Self::Caption {
                id: caption.id(),
                name: caption.name().to_owned(),
                text: caption.text().to_owned(),
                language: caption.language().map(str::to_owned),
                record_range: caption.record_range(),
            },
        }
    }

    fn into_item(self) -> Result<TrackItem> {
        Ok(match self {
            Self::Clip {
                id,
                name,
                source,
                source_range,
                record_range,
                time_map,
            } => {
                let mut clip = Clip::new(id, name, source.into(), source_range, record_range)?;
                clip.set_time_map(time_map.into_time_map()?)?;
                TrackItem::Clip(clip)
            }
            Self::Gap {
                id,
                name,
                record_range,
            } => TrackItem::Gap(Gap::new(id, name, record_range)),
            Self::Transition {
                id,
                name,
                from,
                to,
                from_offset,
                to_offset,
            } => TrackItem::Transition(Transition::new(
                id,
                name,
                from.into(),
                to.into(),
                from_offset,
                to_offset,
            )),
            Self::Generator {
                id,
                name,
                generator_kind,
                parameters,
                record_range,
            } => TrackItem::Generator(Generator::new(
                id,
                name,
                generator_kind,
                parameters,
                record_range,
            )),
            Self::Caption {
                id,
                name,
                text,
                language,
                record_range,
            } => TrackItem::Caption(Caption::new(id, name, text, language, record_range)),
        })
    }
}

impl From<EditorialObjectId> for EditorialObjectIdWire {
    fn from(value: EditorialObjectId) -> Self {
        match value {
            EditorialObjectId::Clip(id) => Self::Clip(id),
            EditorialObjectId::Gap(id) => Self::Gap(id),
            EditorialObjectId::Transition(id) => Self::Transition(id),
            EditorialObjectId::Generator(id) => Self::Generator(id),
            EditorialObjectId::Caption(id) => Self::Caption(id),
        }
    }
}

impl From<EditorialObjectIdWire> for EditorialObjectId {
    fn from(value: EditorialObjectIdWire) -> Self {
        match value {
            EditorialObjectIdWire::Clip(id) => Self::Clip(id),
            EditorialObjectIdWire::Gap(id) => Self::Gap(id),
            EditorialObjectIdWire::Transition(id) => Self::Transition(id),
            EditorialObjectIdWire::Generator(id) => Self::Generator(id),
            EditorialObjectIdWire::Caption(id) => Self::Caption(id),
        }
    }
}

impl From<ClipSource> for ClipSourceWire {
    fn from(value: ClipSource) -> Self {
        match value {
            ClipSource::Media(id) => Self::Media(id),
            ClipSource::Timeline(id) => Self::Timeline(id),
        }
    }
}

impl From<ClipSourceWire> for ClipSource {
    fn from(value: ClipSourceWire) -> Self {
        match value {
            ClipSourceWire::Media(id) => Self::Media(id),
            ClipSourceWire::Timeline(id) => Self::Timeline(id),
        }
    }
}

impl ClipTimeMapWire {
    fn from_time_map(time_map: &ClipTimeMap) -> Self {
        Self {
            record_duration: time_map.record_duration(),
            source_timebase: time_map.source_timebase(),
            segments: time_map
                .segments()
                .iter()
                .map(|segment| RetimeSegmentWire {
                    record_range: segment.record_range(),
                    source_start: segment.source_start(),
                    rate_numerator: segment.rate().numerator().to_string(),
                    rate_denominator: segment.rate().denominator().to_string(),
                })
                .collect(),
        }
    }

    fn into_time_map(self) -> Result<ClipTimeMap> {
        let segments = self
            .segments
            .into_iter()
            .map(|segment| {
                RetimeSegment::new(
                    segment.record_range,
                    segment.source_start,
                    PlaybackRate::new(
                        parse_i64(&segment.rate_numerator, "decode_playback_rate")?,
                        parse_u64(&segment.rate_denominator, "decode_playback_rate")?,
                    )?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        ClipTimeMap::new(self.record_duration, self.source_timebase, segments)
    }
}

impl MarkerWire {
    fn from_marker(marker: &Marker) -> Self {
        Self {
            id: marker.id(),
            owner: marker.owner().into(),
            marked_range: marker.marked_range(),
            label: marker.label().map(|value| value.as_str().to_owned()),
            flag: marker.flag().map(Into::into),
            note: marker.note().map(|value| value.as_str().to_owned()),
            metadata: metadata_to_wire(marker.metadata()),
        }
    }

    fn into_marker(self) -> Result<Marker> {
        let mut marker = Marker::new(self.id, self.owner.into(), self.marked_range)?;
        marker.set_label(self.label.map(MarkerLabel::new).transpose()?);
        marker.set_flag(self.flag.map(Into::into));
        marker.set_note(self.note.map(MarkerNote::new).transpose()?);
        marker.set_metadata(metadata_from_wire(self.metadata)?);
        Ok(marker)
    }
}

impl From<MarkerOwner> for MarkerOwnerWire {
    fn from(value: MarkerOwner) -> Self {
        match value {
            MarkerOwner::Timeline => Self::Timeline,
            MarkerOwner::Track(id) => Self::Track(id),
            MarkerOwner::Object(id) => Self::Object(id.into()),
        }
    }
}

impl From<MarkerOwnerWire> for MarkerOwner {
    fn from(value: MarkerOwnerWire) -> Self {
        match value {
            MarkerOwnerWire::Timeline => Self::Timeline,
            MarkerOwnerWire::Track(id) => Self::Track(id),
            MarkerOwnerWire::Object(id) => Self::Object(id.into()),
        }
    }
}

impl From<MetadataOwnerWire> for MetadataOwner {
    fn from(value: MetadataOwnerWire) -> Self {
        match value {
            MetadataOwnerWire::Timeline => Self::Timeline,
            MetadataOwnerWire::Track(id) => Self::Track(id),
            MetadataOwnerWire::Object(id) => Self::Object(id.into()),
        }
    }
}

impl From<MarkerFlag> for MarkerFlagWire {
    fn from(value: MarkerFlag) -> Self {
        match value {
            MarkerFlag::Red => Self::Red,
            MarkerFlag::Pink => Self::Pink,
            MarkerFlag::Orange => Self::Orange,
            MarkerFlag::Yellow => Self::Yellow,
            MarkerFlag::Green => Self::Green,
            MarkerFlag::Cyan => Self::Cyan,
            MarkerFlag::Blue => Self::Blue,
            MarkerFlag::Purple => Self::Purple,
            MarkerFlag::Magenta => Self::Magenta,
            MarkerFlag::Black => Self::Black,
            MarkerFlag::White => Self::White,
        }
    }
}

impl From<MarkerFlagWire> for MarkerFlag {
    fn from(value: MarkerFlagWire) -> Self {
        match value {
            MarkerFlagWire::Red => Self::Red,
            MarkerFlagWire::Pink => Self::Pink,
            MarkerFlagWire::Orange => Self::Orange,
            MarkerFlagWire::Yellow => Self::Yellow,
            MarkerFlagWire::Green => Self::Green,
            MarkerFlagWire::Cyan => Self::Cyan,
            MarkerFlagWire::Blue => Self::Blue,
            MarkerFlagWire::Purple => Self::Purple,
            MarkerFlagWire::Magenta => Self::Magenta,
            MarkerFlagWire::Black => Self::Black,
            MarkerFlagWire::White => Self::White,
        }
    }
}

impl MetadataValueWire {
    fn from_value(value: &MetadataValue) -> Self {
        match value {
            MetadataValue::Null => Self::Null,
            MetadataValue::Boolean(value) => Self::Boolean { value: *value },
            MetadataValue::Signed(value) => Self::Signed {
                value: value.to_string(),
            },
            MetadataValue::Unsigned(value) => Self::Unsigned {
                value: value.to_string(),
            },
            MetadataValue::Float(value) => Self::Float { value: *value },
            MetadataValue::Text(value) => Self::Text {
                value: value.clone(),
            },
            MetadataValue::Time(value) => Self::Time { value: *value },
            MetadataValue::Range(value) => Self::Range { value: *value },
            MetadataValue::List(values) => Self::List {
                values: values.iter().map(Self::from_value).collect(),
            },
            MetadataValue::Map(entries) => Self::Map {
                entries: metadata_to_wire(entries),
            },
        }
    }

    fn canonicalize(&mut self) {
        match self {
            Self::List { values } => {
                for value in values {
                    value.canonicalize();
                }
            }
            Self::Map { entries } => canonicalize_metadata(entries),
            _ => {}
        }
    }

    fn into_value(self) -> Result<MetadataValue> {
        Ok(match self {
            Self::Null => MetadataValue::Null,
            Self::Boolean { value } => MetadataValue::Boolean(value),
            Self::Signed { value } => MetadataValue::Signed(parse_i64(&value, "decode_metadata")?),
            Self::Unsigned { value } => {
                MetadataValue::Unsigned(parse_u64(&value, "decode_metadata")?)
            }
            Self::Float { value } => MetadataValue::Float(value),
            Self::Text { value } => MetadataValue::Text(value),
            Self::Time { value } => MetadataValue::Time(value),
            Self::Range { value } => MetadataValue::Range(value),
            Self::List { values } => MetadataValue::List(
                values
                    .into_iter()
                    .map(Self::into_value)
                    .collect::<Result<Vec<_>>>()?,
            ),
            Self::Map { entries } => MetadataValue::Map(metadata_from_wire(entries)?),
        })
    }
}

fn metadata_to_wire(metadata: &TimelineMetadata) -> Vec<MetadataEntryWire> {
    metadata
        .iter()
        .map(|(key, value)| MetadataEntryWire {
            key: key.as_str().to_owned(),
            value: MetadataValueWire::from_value(value),
        })
        .collect()
}

fn metadata_from_wire(entries: Vec<MetadataEntryWire>) -> Result<TimelineMetadata> {
    let mut metadata = TimelineMetadata::new();
    for entry in entries {
        let key = MetadataKey::new(entry.key)?;
        if metadata
            .insert(key.clone(), entry.value.into_value()?)
            .is_some()
        {
            return Err(duplicate_document_identity("metadata key", key.as_str()));
        }
    }
    Ok(metadata)
}

fn canonicalize_metadata(entries: &mut [MetadataEntryWire]) {
    for entry in entries.iter_mut() {
        entry.value.canonicalize();
    }
    entries.sort_by(|left, right| left.key.cmp(&right.key));
}

impl MulticamSourceWire {
    fn from_source(source: &MulticamSource) -> Self {
        Self {
            sync_method: MulticamSyncMethodWire::from_method(source.sync_method()),
            angles: source
                .angles()
                .iter()
                .map(|angle| MulticamAngleWire {
                    id: angle.id(),
                    name: angle.name().to_owned(),
                    camera_label: angle.camera_label().to_owned(),
                    enabled: angle.enabled(),
                    metadata: metadata_to_wire(angle.metadata()),
                    source_clips: angle.source_clips().to_vec(),
                })
                .collect(),
        }
    }

    fn into_source(self) -> Result<MulticamSource> {
        MulticamSource::new(
            self.sync_method.into_method(),
            self.angles
                .into_iter()
                .map(|wire| {
                    let mut angle = MulticamAngle::new(
                        wire.id,
                        wire.name,
                        wire.camera_label,
                        wire.source_clips,
                    )?;
                    angle.set_enabled(wire.enabled);
                    angle.set_metadata(metadata_from_wire(wire.metadata)?);
                    Ok(angle)
                })
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

impl MulticamSyncMethodWire {
    fn from_method(method: &MulticamSyncMethod) -> Self {
        match method {
            MulticamSyncMethod::Manual => Self::Manual,
            MulticamSyncMethod::Timecode => Self::Timecode,
            MulticamSyncMethod::InPoints => Self::InPoints,
            MulticamSyncMethod::OutPoints => Self::OutPoints,
            MulticamSyncMethod::ClipMarker(name) => Self::ClipMarker { name: name.clone() },
            MulticamSyncMethod::Audio => Self::Audio,
        }
    }

    fn into_method(self) -> MulticamSyncMethod {
        match self {
            Self::Manual => MulticamSyncMethod::Manual,
            Self::Timecode => MulticamSyncMethod::Timecode,
            Self::InPoints => MulticamSyncMethod::InPoints,
            Self::OutPoints => MulticamSyncMethod::OutPoints,
            Self::ClipMarker { name } => MulticamSyncMethod::ClipMarker(name),
            Self::Audio => MulticamSyncMethod::Audio,
        }
    }
}

impl MulticamClipWire {
    fn from_clip(clip: &MulticamClip) -> Self {
        Self {
            clip_id: clip.clip_id(),
            switches: clip
                .switches()
                .iter()
                .map(|switch| MulticamSwitchWire {
                    source_range: switch.source_range(),
                    angle_id: switch.angle_id(),
                })
                .collect(),
            audio_policy: clip.audio_policy().into(),
        }
    }

    fn into_clip(self) -> Result<MulticamClip> {
        let first = self.switches.first().copied().ok_or_else(|| {
            document_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "decode_multicam_clip",
                "multicam clip must retain at least one switch interval",
                [("clip", self.clip_id.to_string())],
            )
        })?;
        let mut expected = first.source_range.start();
        for switch in &self.switches {
            if switch.source_range.is_empty()
                || switch.source_range.start() != expected
                || switch.source_range.timebase() != expected.timebase()
            {
                return Err(document_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "decode_multicam_clip",
                    "multicam switch intervals must be nonempty, gapless, and use one source clock",
                    [("clip", self.clip_id.to_string())],
                ));
            }
            expected = switch.source_range.end_exclusive()?;
        }
        let last = self.switches.last().copied().expect("first switch exists");
        let coverage = TimeRange::from_start_end(
            first.source_range.start(),
            last.source_range.end_exclusive()?,
        )?;
        let mut clip = MulticamClip::new(
            self.clip_id,
            coverage,
            first.angle_id,
            self.audio_policy.into(),
        )?;
        for switch in self.switches {
            clip.switch_range(switch.source_range, switch.angle_id)?;
        }
        Ok(clip)
    }
}

impl From<&MulticamAudioPolicy> for MulticamAudioPolicyWire {
    fn from(value: &MulticamAudioPolicy) -> Self {
        match value {
            MulticamAudioPolicy::FollowVideo => Self::FollowVideo,
            MulticamAudioPolicy::Fixed(id) => Self::Fixed(*id),
            MulticamAudioPolicy::AllAngles => Self::AllAngles,
        }
    }
}

impl From<MulticamAudioPolicyWire> for MulticamAudioPolicy {
    fn from(value: MulticamAudioPolicyWire) -> Self {
        match value {
            MulticamAudioPolicyWire::FollowVideo => Self::FollowVideo,
            MulticamAudioPolicyWire::Fixed(id) => Self::Fixed(id),
            MulticamAudioPolicyWire::AllAngles => Self::AllAngles,
        }
    }
}

fn encode_current_document(payload: &TimelineStatePayloadWire) -> Result<Vec<u8>> {
    let payload_bytes = serde_json::to_vec(payload).map_err(|source| {
        document_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "encode_document",
            "timeline state payload cannot be represented as JSON",
            [("source", source.to_string())],
        )
    })?;
    let envelope = CurrentEnvelopeRef {
        format: TIMELINE_STATE_FORMAT,
        format_revision: TIMELINE_STATE_FORMAT_REVISION,
        primitive_schema_revision: STABLE_PRIMITIVE_SCHEMA_REVISION,
        payload_sha256: sha256_hex(&payload_bytes),
        payload,
    };
    serde_json::to_vec(&envelope).map_err(|source| {
        document_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "encode_document",
            "timeline state envelope cannot be represented as JSON",
            [("source", source.to_string())],
        )
    })
}

fn inspect_header(raw: &Value) -> Result<u32> {
    let object = raw.as_object().ok_or_else(|| {
        document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "inspect_document",
            "timeline state document root must be an object",
            std::iter::empty::<(&str, String)>(),
        )
    })?;
    let format = object
        .get("format")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            document_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "inspect_document",
                "timeline state document format is missing or invalid",
                std::iter::empty::<(&str, String)>(),
            )
        })?;
    let revision = object
        .get("format_revision")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| {
            document_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "inspect_document",
                "timeline state format revision is missing or invalid",
                std::iter::empty::<(&str, String)>(),
            )
        })?;
    if format != TIMELINE_STATE_FORMAT {
        return Err(unsupported_format_error(format, revision));
    }
    Ok(revision)
}

fn validate_current_envelope(envelope: &CurrentEnvelopeWire) -> Result<()> {
    if envelope.format != TIMELINE_STATE_FORMAT
        || envelope.format_revision != TIMELINE_STATE_FORMAT_REVISION
    {
        return Err(unsupported_format_error(
            &envelope.format,
            envelope.format_revision,
        ));
    }
    if envelope.primitive_schema_revision != STABLE_PRIMITIVE_SCHEMA_REVISION {
        return Err(document_error(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "decode_document",
            "timeline state document uses an unsupported primitive schema revision",
            [(
                "primitive_schema_revision",
                envelope.primitive_schema_revision.to_string(),
            )],
        ));
    }
    let payload_bytes = serde_json::to_vec(&envelope.payload).map_err(|source| {
        document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "verify_document",
            "timeline state payload cannot be canonicalized for integrity verification",
            [("source", source.to_string())],
        )
    })?;
    let actual = sha256_hex(&payload_bytes);
    if envelope.payload_sha256.len() != 64
        || !envelope
            .payload_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || envelope.payload_sha256 != actual
    {
        return Err(document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "verify_document",
            "timeline state payload integrity check failed",
            std::iter::empty::<(&str, String)>(),
        ));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn canonicalize_relations(relations: &mut [Vec<ClipId>]) {
    for relation in relations.iter_mut() {
        relation.sort_unstable();
    }
    relations.sort();
}

fn validate_relations(relations: &[Vec<ClipId>], label: &str) -> Result<()> {
    let mut all_members = BTreeSet::new();
    for relation in relations {
        if relation.len() < 2 {
            return Err(document_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "decode_edit_state",
                "timeline relationship must retain at least two clips",
                [("kind", label.to_owned())],
            ));
        }
        ensure_unique(relation, label)?;
        for member in relation {
            if !all_members.insert(*member) {
                return Err(document_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "decode_edit_state",
                    "one clip cannot appear in multiple components of the same relationship kind",
                    [("kind", label.to_owned()), ("clip", member.to_string())],
                ));
            }
        }
    }
    Ok(())
}

fn ensure_unique<T>(values: &[T], label: &str) -> Result<()>
where
    T: Ord + std::fmt::Debug,
{
    if let Some(pair) = values.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(duplicate_document_identity(label, &pair[0]));
    }
    Ok(())
}

fn parse_i64(value: &str, operation: &'static str) -> Result<i64> {
    let parsed = value.parse::<i64>().map_err(|source| {
        document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            operation,
            "timeline state signed integer is not canonical",
            [("source", source.to_string())],
        )
    })?;
    if parsed.to_string() != value {
        return Err(document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            operation,
            "timeline state signed integer is not canonical",
            [("value", value.to_owned())],
        ));
    }
    Ok(parsed)
}

fn parse_u64(value: &str, operation: &'static str) -> Result<u64> {
    let parsed = value.parse::<u64>().map_err(|source| {
        document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            operation,
            "timeline state unsigned integer is not canonical",
            [("source", source.to_string())],
        )
    })?;
    if parsed.to_string() != value {
        return Err(document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            operation,
            "timeline state unsigned integer is not canonical",
            [("value", value.to_owned())],
        ));
    }
    Ok(parsed)
}

fn duplicate_document_identity(label: &str, value: impl std::fmt::Debug) -> Error {
    document_error(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        "decode_document",
        "timeline state document contains a duplicate identity",
        [("kind", label.to_owned()), ("value", format!("{value:?}"))],
    )
}

fn unsupported_format_error(format: &str, revision: u32) -> Error {
    document_error(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        "inspect_document",
        "timeline state document format or revision is unsupported",
        [
            ("format", format.to_owned()),
            ("format_revision", revision.to_string()),
        ],
    )
}

fn document_error<K, V, I>(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
    fields: I,
) -> Error
where
    K: Into<String>,
    V: Into<String>,
    I: IntoIterator<Item = (K, V)>,
{
    let mut context = ErrorContext::new(COMPONENT, operation);
    for (key, value) in fields {
        context = context.with_field(key, value);
    }
    Error::new(category, recoverability, message).with_context(context)
}
