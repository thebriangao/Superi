//! OpenTimelineIO JSON import and export over the native editorial model.
//!
//! The runtime is intentionally independent of the OpenTimelineIO Python and C++
//! packages. The pinned `OTIO_CORE 0.18.1` schema is decoded into ordinary
//! [`EditorialProject`] state, while complete source objects remain available as
//! opaque templates for faithful export of fields that Superi does not yet model.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Number, Value};
use superi_core::diagnostics::FiniteF64;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{
    ClipId, GapId, GeneratorId, MarkerId, MediaId, ProjectId, TimelineId, TrackId, TransitionId,
};
use superi_core::pixel::ChannelLayout;
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};

use crate::markers::{
    Marker, MarkerFlag, MarkerLabel, MarkerNote, MarkerOwner, MetadataKey, MetadataOwner,
    MetadataValue, TimelineMetadata,
};
use crate::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting,
    AudioTrackSemantics, Clip, ClipSource, DataSchema, DataTrackSemantics, EditorialObjectId,
    EditorialProject, Gap, Generator, LinkedMediaReference, Timeline, Track, TrackItem,
    TrackSemantics, Transition, VideoCompositing, VideoTrackSemantics,
};
use crate::retime::{ClipTimeMap, PlaybackRate};

/// Stable warning code emitted for preserved OTIO constructs outside the native subset.
pub const UNSUPPORTED_CONSTRUCT_CODE: &str = "timeline.otio.unsupported_construct";

const OTIO_CORE_0181_SCHEMA_MAP: &[(&str, u32)] = &[
    ("Adapter", 1),
    ("Clip", 2),
    ("Composable", 1),
    ("Composition", 1),
    ("Effect", 1),
    ("ExternalReference", 1),
    ("FreezeFrame", 1),
    ("Gap", 1),
    ("GeneratorReference", 1),
    ("HookScript", 1),
    ("ImageSequenceReference", 1),
    ("Item", 1),
    ("LinearTimeWarp", 1),
    ("Marker", 2),
    ("MediaLinker", 1),
    ("MediaReference", 1),
    ("MissingReference", 1),
    ("PluginManifest", 1),
    ("SchemaDef", 1),
    ("SerializableCollection", 1),
    ("SerializableObject", 1),
    ("SerializableObjectWithMetadata", 1),
    ("Stack", 1),
    ("Test", 1),
    ("TimeEffect", 1),
    ("Timeline", 1),
    ("Track", 1),
    ("Transition", 1),
    ("UnknownSchema", 1),
];

/// OTIO schema family targeted by an export.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum OtioSchemaTarget {
    /// The repository-pinned `OTIO_CORE 0.18.1` schema map.
    OtioCore0181,
}

impl OtioSchemaTarget {
    /// Returns the stable schema family label.
    #[must_use]
    pub const fn family(self) -> &'static str {
        match self {
            Self::OtioCore0181 => "OTIO_CORE",
        }
    }

    /// Returns the stable release label.
    #[must_use]
    pub const fn release(self) -> &'static str {
        match self {
            Self::OtioCore0181 => "0.18.1",
        }
    }

    /// Returns the complete pinned type-version map in deterministic order.
    pub fn schema_version_map(self) -> BTreeMap<&'static str, u32> {
        match self {
            Self::OtioCore0181 => OTIO_CORE_0181_SCHEMA_MAP.iter().copied().collect(),
        }
    }
}

/// Severity assigned to one OTIO interchange diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum OtioDiagnosticSeverity {
    /// The construct was retained, but it is not directly modeled yet.
    Warning,
}

impl OtioDiagnosticSeverity {
    /// Returns the permanent lowercase severity code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Warning => "warning",
        }
    }
}

/// One stable diagnostic for an opaque OTIO construct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtioDiagnostic {
    code: &'static str,
    severity: OtioDiagnosticSeverity,
    json_pointer: String,
    object_id: Option<String>,
    otio_schema: String,
}

impl OtioDiagnostic {
    /// Returns the permanent machine-readable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the diagnostic severity.
    #[must_use]
    pub const fn severity(&self) -> OtioDiagnosticSeverity {
        self.severity
    }

    /// Returns the exact JSON pointer in the imported document.
    #[must_use]
    pub fn json_pointer(&self) -> &str {
        &self.json_pointer
    }

    /// Returns the source object identity when metadata supplied one.
    #[must_use]
    pub fn object_id(&self) -> Option<&str> {
        self.object_id.as_deref()
    }

    /// Returns the serialized OTIO schema name.
    #[must_use]
    pub fn otio_schema(&self) -> &str {
        &self.otio_schema
    }
}

/// Explicit policy for OTIO details that do not carry native semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtioImportOptions {
    audio_sample_rate: u32,
    audio_channel_layout: ChannelLayout,
}

impl Default for OtioImportOptions {
    fn default() -> Self {
        Self {
            audio_sample_rate: 48_000,
            audio_channel_layout: ChannelLayout::stereo(),
        }
    }
}

impl OtioImportOptions {
    /// Creates explicit defaults for generic OTIO audio tracks.
    pub fn new(audio_sample_rate: u32, audio_channel_layout: ChannelLayout) -> Result<Self> {
        Timebase::integer(audio_sample_rate)?;
        Ok(Self {
            audio_sample_rate,
            audio_channel_layout,
        })
    }

    /// Returns the sample rate assigned to generic OTIO audio tracks.
    #[must_use]
    pub const fn audio_sample_rate(&self) -> u32 {
        self.audio_sample_rate
    }

    /// Returns the channel layout assigned to generic OTIO audio tracks.
    #[must_use]
    pub const fn audio_channel_layout(&self) -> &ChannelLayout {
        &self.audio_channel_layout
    }
}

/// One imported OTIO document backed by ordinary native editorial state.
#[derive(Clone, Debug)]
pub struct OtioDocument {
    project: EditorialProject,
    root_timeline_id: TimelineId,
    diagnostics: Vec<OtioDiagnostic>,
    preserved: PreservedOtio,
}

impl OtioDocument {
    /// Returns the editable native project snapshot.
    #[must_use]
    pub const fn project(&self) -> &EditorialProject {
        &self.project
    }

    /// Returns the native project for ordinary atomic editing.
    pub fn project_mut(&mut self) -> &mut EditorialProject {
        &mut self.project
    }

    /// Returns the root native timeline identity.
    #[must_use]
    pub const fn root_timeline_id(&self) -> TimelineId {
        self.root_timeline_id
    }

    /// Returns stable diagnostics in source traversal order.
    #[must_use]
    pub fn diagnostics(&self) -> &[OtioDiagnostic] {
        &self.diagnostics
    }
}

#[derive(Clone, Debug, Default)]
struct PreservedOtio {
    root_template: Value,
    root_stack_template: Value,
    root_stack_source_id: String,
    timeline_templates: BTreeMap<TimelineId, Value>,
    track_templates: BTreeMap<TrackId, Value>,
    item_templates: BTreeMap<EditorialObjectId, Value>,
    media_templates: BTreeMap<MediaId, Value>,
    marker_templates: BTreeMap<MarkerId, Value>,
    timeline_source_ids: BTreeMap<TimelineId, String>,
    track_source_ids: BTreeMap<TrackId, String>,
    item_source_ids: BTreeMap<EditorialObjectId, String>,
    media_source_ids: BTreeMap<MediaId, String>,
    marker_source_ids: BTreeMap<MarkerId, String>,
}

/// Imports one native OTIO JSON document into editable Superi state.
pub fn import_otio(bytes: &[u8], options: OtioImportOptions) -> Result<OtioDocument> {
    let value: Value = serde_json::from_slice(bytes).map_err(|source| {
        Error::with_source(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "OTIO input is not valid JSON",
            source,
        )
        .with_context(ErrorContext::new("superi-timeline.otio", "parse_json"))
    })?;
    Importer::new(options).import(value)
}

/// Exports the current native editorial state to deterministic OTIO JSON.
pub fn export_otio(document: &OtioDocument, target: OtioSchemaTarget) -> Result<Vec<u8>> {
    let value = Exporter::new(document, target).export()?;
    serde_json::to_vec_pretty(&value).map_err(|source| {
        Error::with_source(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "failed to serialize validated OTIO output",
            source,
        )
        .with_context(ErrorContext::new("superi-timeline.otio", "serialize_json"))
    })
}

struct Importer {
    options: OtioImportOptions,
    next_id: u128,
    seen_source_ids: BTreeSet<String>,
    media_by_source_id: BTreeMap<String, MediaId>,
    media_references: BTreeMap<MediaId, LinkedMediaReference>,
    timelines: Vec<Timeline>,
    diagnostics: Vec<OtioDiagnostic>,
    preserved: PreservedOtio,
}

impl Importer {
    fn new(options: OtioImportOptions) -> Self {
        Self {
            options,
            next_id: 1,
            seen_source_ids: BTreeSet::new(),
            media_by_source_id: BTreeMap::new(),
            media_references: BTreeMap::new(),
            timelines: Vec::new(),
            diagnostics: Vec::new(),
            preserved: PreservedOtio::default(),
        }
    }

    fn import(mut self, value: Value) -> Result<OtioDocument> {
        require_schema(&value, "Timeline.1", "")?;
        let root_source_id = self.register_identity(&value, "")?;
        let tracks = required_member(&value, "tracks", "/tracks")?.clone();
        require_schema(&tracks, "Stack.1", "/tracks")?;
        let root_stack_source_id = self.register_identity(&tracks, "/tracks")?;
        let root_id = self.timeline_id();
        let edit_rate = timeline_edit_rate(&value, &tracks)?;
        let global_start = match value.get("global_start_time") {
            Some(Value::Null) | None => RationalTime::zero(edit_rate),
            Some(time) => parse_time_at(time, edit_rate, "/global_start_time")?,
        };
        let name = required_name(&value, &root_source_id);
        let root = self.build_timeline(
            root_id,
            &name,
            edit_rate,
            global_start,
            &tracks,
            &value,
            "/tracks",
        )?;
        self.timelines.push(root);

        self.preserved.root_template = value;
        self.preserved.root_stack_template = tracks;
        self.preserved.root_stack_source_id = root_stack_source_id;
        self.preserved
            .timeline_source_ids
            .insert(root_id, root_source_id);

        let project_name = name.clone();
        let project = EditorialProject::new(
            ProjectId::from_raw(self.allocate_raw()),
            project_name,
            self.media_references.into_values(),
            self.timelines,
        )?;
        Ok(OtioDocument {
            project,
            root_timeline_id: root_id,
            diagnostics: self.diagnostics,
            preserved: self.preserved,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_timeline(
        &mut self,
        timeline_id: TimelineId,
        name: &str,
        edit_rate: Timebase,
        global_start: RationalTime,
        stack: &Value,
        metadata_source: &Value,
        pointer: &str,
    ) -> Result<Timeline> {
        self.scan_effects(stack, pointer)?;
        let children = array_member(stack, "children", &format!("{pointer}/children"))?;
        let mut tracks = Vec::with_capacity(children.len());
        let mut annotations = Vec::new();
        for (index, track_value) in children.iter().enumerate() {
            let track_pointer = format!("{pointer}/children/{index}");
            let built = self.build_track(track_value, edit_rate, &track_pointer)?;
            annotations.extend(built.annotations);
            tracks.push(built.track);
        }
        let mut timeline = Timeline::new(timeline_id, name, edit_rate, global_start, tracks);
        let timeline_metadata = metadata_from_object(metadata_source)?;
        if !timeline_metadata.is_empty() {
            timeline.set_metadata(MetadataOwner::Timeline, timeline_metadata)?;
        }
        for marker in self.build_markers(
            stack,
            MarkerOwner::Timeline,
            edit_rate,
            &format!("{pointer}/markers"),
        )? {
            timeline.upsert_marker(marker)?;
        }
        for annotation in annotations {
            match annotation {
                PendingAnnotation::Metadata(owner, metadata) if !metadata.is_empty() => {
                    timeline.set_metadata(owner, metadata)?;
                }
                PendingAnnotation::Metadata(_, _) => {}
                PendingAnnotation::Marker(marker) => {
                    timeline.upsert_marker(marker)?;
                }
            }
        }
        Ok(timeline)
    }

    fn build_track(
        &mut self,
        value: &Value,
        timeline_rate: Timebase,
        pointer: &str,
    ) -> Result<TrackBuild> {
        require_schema(value, "Track.1", pointer)?;
        let source_id = self.register_identity(value, pointer)?;
        let track_id = self.track_id();
        let semantics = self.track_semantics(value, timeline_rate, pointer)?;
        let track_rate = semantics.timebase();
        self.scan_effects(value, pointer)?;
        let children = array_member(value, "children", &format!("{pointer}/children"))?;
        let mut built: Vec<Option<BuiltItem>> = Vec::with_capacity(children.len());
        let mut cursor = RationalTime::zero(track_rate);
        for (index, child) in children.iter().enumerate() {
            let child_pointer = format!("{pointer}/children/{index}");
            if schema(child) == Some("Transition.1") {
                self.register_identity(child, &child_pointer)?;
                self.scan_effects(child, &child_pointer)?;
                built.push(None);
                continue;
            }
            let item = self.build_timed_item(child, track_rate, cursor, &child_pointer)?;
            cursor = item
                .item
                .record_range()
                .expect("timed import item must have a record range")
                .end_exclusive()?;
            built.push(Some(item));
        }

        let mut annotations = Vec::new();
        let mut items = Vec::with_capacity(children.len());
        for (index, child) in children.iter().enumerate() {
            if let Some(item) = built[index].take() {
                annotations.extend(item.annotations);
                items.push(item.item);
                continue;
            }
            let prior = items
                .last()
                .ok_or_else(|| {
                    invalid_otio(
                        "import_transition",
                        "OTIO transition has no preceding item",
                        &format!("{pointer}/children/{index}"),
                    )
                })?
                .id();
            let next = built
                .iter()
                .skip(index + 1)
                .find_map(|candidate| candidate.as_ref().map(|item| item.item.id()))
                .ok_or_else(|| {
                    invalid_otio(
                        "import_transition",
                        "OTIO transition has no following item",
                        &format!("{pointer}/children/{index}"),
                    )
                })?;
            let transition_pointer = format!("{pointer}/children/{index}");
            let transition_id = self.transition_id();
            let source_id = source_identity(child, &transition_pointer);
            let from_offset = parse_duration_at(
                required_member(
                    child,
                    "in_offset",
                    &format!("{transition_pointer}/in_offset"),
                )?,
                track_rate,
                &format!("{transition_pointer}/in_offset"),
            )?;
            let to_offset = parse_duration_at(
                required_member(
                    child,
                    "out_offset",
                    &format!("{transition_pointer}/out_offset"),
                )?,
                track_rate,
                &format!("{transition_pointer}/out_offset"),
            )?;
            let transition = Transition::new(
                transition_id,
                required_name(child, &source_id),
                prior,
                next,
                from_offset,
                to_offset,
            );
            let object_id = EditorialObjectId::Transition(transition_id);
            self.preserved
                .item_templates
                .insert(object_id, child.clone());
            self.preserved.item_source_ids.insert(object_id, source_id);
            annotations.push(PendingAnnotation::Metadata(
                MetadataOwner::Object(object_id),
                metadata_from_object(child)?,
            ));
            items.push(TrackItem::Transition(transition));
        }

        let mut track_annotations = Vec::new();
        track_annotations.push(PendingAnnotation::Metadata(
            MetadataOwner::Track(track_id),
            metadata_from_object(value)?,
        ));
        for marker in self.build_markers(
            value,
            MarkerOwner::Track(track_id),
            track_rate,
            &format!("{pointer}/markers"),
        )? {
            track_annotations.push(PendingAnnotation::Marker(marker));
        }
        track_annotations.extend(annotations);

        self.preserved
            .track_templates
            .insert(track_id, value.clone());
        self.preserved.track_source_ids.insert(track_id, source_id);
        Ok(TrackBuild {
            track: Track::new(track_id, required_name(value, "track"), semantics, items),
            annotations: track_annotations,
        })
    }

    fn build_timed_item(
        &mut self,
        value: &Value,
        track_rate: Timebase,
        record_start: RationalTime,
        pointer: &str,
    ) -> Result<BuiltItem> {
        let source_id = self.register_identity(value, pointer)?;
        match schema(value) {
            Some("Clip.2") => {
                self.scan_effects(value, pointer)?;
                self.build_clip(value, track_rate, record_start, pointer, source_id)
            }
            Some("Gap.1") => {
                self.scan_effects(value, pointer)?;
                let source_range_value =
                    required_member(value, "source_range", &format!("{pointer}/source_range"))?;
                let duration = parse_range_at(
                    source_range_value,
                    track_rate,
                    &format!("{pointer}/source_range"),
                )?
                .duration();
                let record_range = TimeRange::new(record_start, duration)?;
                let id = self.gap_id();
                let item_id = EditorialObjectId::Gap(id);
                self.preserved.item_templates.insert(item_id, value.clone());
                self.preserved
                    .item_source_ids
                    .insert(item_id, source_id.clone());
                Ok(BuiltItem {
                    item: TrackItem::Gap(Gap::new(
                        id,
                        required_name(value, &source_id),
                        record_range,
                    )),
                    annotations: self.item_annotations(value, item_id, track_rate, pointer)?,
                })
            }
            Some("Stack.1") => {
                let child_rate = discover_rate(value).unwrap_or(track_rate);
                let child_timeline_id = self.timeline_id();
                let child_name = required_name(value, &source_id);
                let child_timeline = self.build_timeline(
                    child_timeline_id,
                    &child_name,
                    child_rate,
                    RationalTime::zero(child_rate),
                    value,
                    value,
                    pointer,
                )?;
                self.timelines.push(child_timeline);
                let selected_value =
                    required_member(value, "source_range", &format!("{pointer}/source_range"))?;
                let source_range = parse_range_at(
                    selected_value,
                    child_rate,
                    &format!("{pointer}/source_range"),
                )?;
                let record_duration = parse_range_at(
                    selected_value,
                    track_rate,
                    &format!("{pointer}/source_range"),
                )?
                .duration();
                let record_range = TimeRange::new(record_start, record_duration)?;
                let clip_id = self.clip_id();
                let item_id = EditorialObjectId::Clip(clip_id);
                self.preserved
                    .timeline_templates
                    .insert(child_timeline_id, value.clone());
                self.preserved
                    .timeline_source_ids
                    .insert(child_timeline_id, source_id.clone());
                self.preserved.item_templates.insert(item_id, value.clone());
                self.preserved
                    .item_source_ids
                    .insert(item_id, source_id.clone());
                Ok(BuiltItem {
                    item: TrackItem::Clip(Clip::new(
                        clip_id,
                        child_name,
                        ClipSource::Timeline(child_timeline_id),
                        source_range,
                        record_range,
                    )?),
                    annotations: self
                        .item_marker_annotations(value, item_id, track_rate, pointer)?,
                })
            }
            Some(other) => {
                self.scan_effects(value, pointer)?;
                let range_value =
                    required_member(value, "source_range", &format!("{pointer}/source_range"))?;
                let duration =
                    parse_range_at(range_value, track_rate, &format!("{pointer}/source_range"))?
                        .duration();
                let record_range = TimeRange::new(record_start, duration)?;
                let id = self.generator_id();
                let item_id = EditorialObjectId::Generator(id);
                let mut parameters = BTreeMap::new();
                parameters.insert("otio_schema".into(), other.into());
                self.preserved.item_templates.insert(item_id, value.clone());
                self.preserved
                    .item_source_ids
                    .insert(item_id, source_id.clone());
                self.diagnostics.push(OtioDiagnostic {
                    code: UNSUPPORTED_CONSTRUCT_CODE,
                    severity: OtioDiagnosticSeverity::Warning,
                    json_pointer: pointer.into(),
                    object_id: explicit_source_identity(value).map(str::to_owned),
                    otio_schema: other.into(),
                });
                Ok(BuiltItem {
                    item: TrackItem::Generator(Generator::new(
                        id,
                        required_name(value, &source_id),
                        format!("opentimelineio:{other}"),
                        parameters,
                        record_range,
                    )),
                    annotations: self.item_annotations(value, item_id, track_rate, pointer)?,
                })
            }
            None => Err(invalid_otio(
                "import_item",
                "OTIO child is missing OTIO_SCHEMA",
                pointer,
            )),
        }
    }

    fn build_clip(
        &mut self,
        value: &Value,
        track_rate: Timebase,
        record_start: RationalTime,
        pointer: &str,
        source_id: String,
    ) -> Result<BuiltItem> {
        let source_range_value =
            required_member(value, "source_range", &format!("{pointer}/source_range"))?;
        let source_rate = range_rate(source_range_value, &format!("{pointer}/source_range"))?;
        let source_range = parse_range_at(
            source_range_value,
            source_rate,
            &format!("{pointer}/source_range"),
        )?;
        let record_duration = parse_range_at(
            source_range_value,
            track_rate,
            &format!("{pointer}/source_range"),
        )?
        .duration();
        let record_range = TimeRange::new(record_start, record_duration)?;
        let media_id = self.import_active_media(value, pointer)?;
        let clip_id = self.clip_id();
        let mut clip = Clip::new(
            clip_id,
            required_name(value, &source_id),
            ClipSource::Media(media_id),
            source_range,
            record_range,
        )?;
        if let Some(rate) = linear_time_warp(value, pointer)? {
            clip.set_time_map(ClipTimeMap::speed(
                record_duration,
                source_range.start(),
                rate,
            )?)?;
        }
        let item_id = EditorialObjectId::Clip(clip_id);
        self.preserved.item_templates.insert(item_id, value.clone());
        self.preserved.item_source_ids.insert(item_id, source_id);
        Ok(BuiltItem {
            item: TrackItem::Clip(clip),
            annotations: self.item_annotations(value, item_id, track_rate, pointer)?,
        })
    }

    fn import_active_media(&mut self, clip: &Value, pointer: &str) -> Result<MediaId> {
        let active_key = clip
            .get("active_media_reference_key")
            .and_then(Value::as_str)
            .unwrap_or("DEFAULT_MEDIA");
        let media_value = clip
            .get("media_references")
            .and_then(Value::as_object)
            .and_then(|media| media.get(active_key));
        let media_pointer = format!(
            "{pointer}/media_references/{}",
            escape_json_pointer(active_key)
        );
        let synthetic;
        let media_value = if let Some(value) = media_value {
            value
        } else {
            synthetic = json!({
                "OTIO_SCHEMA": "MissingReference.1",
                "name": "missing-media",
                "metadata": {"superi": {"id": format!("missing@{pointer}")}}
            });
            &synthetic
        };
        let source_id = source_identity(media_value, &media_pointer);
        if let Some(id) = self.media_by_source_id.get(&source_id) {
            if self.preserved.media_templates.get(id) != Some(media_value) {
                return Err(invalid_otio(
                    "import_media",
                    format!("conflicting repeated OTIO media identity {source_id}"),
                    &media_pointer,
                ));
            }
            return Ok(*id);
        }
        if !self.seen_source_ids.insert(source_id.clone()) {
            return Err(duplicate_identity(&source_id, &media_pointer));
        }
        let id = self.media_id();
        let name = required_name(media_value, &source_id);
        let target = media_value
            .get("target_url")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("urn:opentimelineio:missing:{source_id}"));
        let available_range = media_value
            .get("available_range")
            .filter(|value| !value.is_null())
            .map(|value| {
                let rate = range_rate(value, &format!("{media_pointer}/available_range"))?;
                parse_range_at(value, rate, &format!("{media_pointer}/available_range"))
            })
            .transpose()?;
        self.media_references.insert(
            id,
            LinkedMediaReference::new(id, name, target, available_range),
        );
        self.media_by_source_id.insert(source_id.clone(), id);
        self.preserved
            .media_templates
            .insert(id, media_value.clone());
        self.preserved.media_source_ids.insert(id, source_id);
        Ok(id)
    }

    fn item_annotations(
        &mut self,
        value: &Value,
        item_id: EditorialObjectId,
        timebase: Timebase,
        pointer: &str,
    ) -> Result<Vec<PendingAnnotation>> {
        let mut annotations = vec![PendingAnnotation::Metadata(
            MetadataOwner::Object(item_id),
            metadata_from_object(value)?,
        )];
        for marker in self.build_markers(
            value,
            MarkerOwner::Object(item_id),
            timebase,
            &format!("{pointer}/markers"),
        )? {
            annotations.push(PendingAnnotation::Marker(marker));
        }
        Ok(annotations)
    }

    fn item_marker_annotations(
        &mut self,
        value: &Value,
        item_id: EditorialObjectId,
        timebase: Timebase,
        pointer: &str,
    ) -> Result<Vec<PendingAnnotation>> {
        let markers = self.build_markers(
            value,
            MarkerOwner::Object(item_id),
            timebase,
            &format!("{pointer}/markers"),
        )?;
        Ok(markers.into_iter().map(PendingAnnotation::Marker).collect())
    }

    fn build_markers(
        &mut self,
        owner_value: &Value,
        owner: MarkerOwner,
        timebase: Timebase,
        pointer: &str,
    ) -> Result<Vec<Marker>> {
        let Some(markers) = owner_value.get("markers") else {
            return Ok(Vec::new());
        };
        if markers.is_null() {
            return Ok(Vec::new());
        }
        let markers = markers.as_array().ok_or_else(|| {
            invalid_otio("import_markers", "OTIO markers must be an array", pointer)
        })?;
        let mut imported = Vec::with_capacity(markers.len());
        for (index, value) in markers.iter().enumerate() {
            let marker_pointer = format!("{pointer}/{index}");
            require_schema(value, "Marker.2", &marker_pointer)?;
            let source_id = self.register_identity(value, &marker_pointer)?;
            let marked_range = parse_range_at(
                required_member(
                    value,
                    "marked_range",
                    &format!("{marker_pointer}/marked_range"),
                )?,
                timebase,
                &format!("{marker_pointer}/marked_range"),
            )?;
            let id = self.marker_id();
            let mut marker = Marker::new(id, owner, marked_range)?;
            if let Some(name) = value
                .get("name")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                marker.set_label(Some(MarkerLabel::new(name)?));
            }
            if let Some(comment) = value
                .get("comment")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                marker.set_note(Some(MarkerNote::new(comment)?));
            }
            if let Some(flag) = value
                .get("color")
                .and_then(Value::as_str)
                .and_then(marker_flag_from_otio)
            {
                marker.set_flag(Some(flag));
            }
            marker.set_metadata(metadata_from_object(value)?);
            self.preserved.marker_templates.insert(id, value.clone());
            self.preserved.marker_source_ids.insert(id, source_id);
            imported.push(marker);
        }
        Ok(imported)
    }

    fn scan_effects(&mut self, value: &Value, pointer: &str) -> Result<()> {
        let Some(effects) = value.get("effects") else {
            return Ok(());
        };
        if effects.is_null() {
            return Ok(());
        }
        let effects = effects.as_array().ok_or_else(|| {
            invalid_otio(
                "import_effects",
                "OTIO effects must be an array",
                &format!("{pointer}/effects"),
            )
        })?;
        for (index, effect) in effects.iter().enumerate() {
            let effect_pointer = format!("{pointer}/effects/{index}");
            let source_id = self.register_identity(effect, &effect_pointer)?;
            let effect_schema = schema(effect).unwrap_or("UnknownSchema.1");
            let supported = effect_schema == "LinearTimeWarp.1"
                || (effect_schema == "Effect.1"
                    && effect
                        .get("effect_name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name.starts_with("superi.effect.")));
            if !supported {
                self.diagnostics.push(OtioDiagnostic {
                    code: UNSUPPORTED_CONSTRUCT_CODE,
                    severity: OtioDiagnosticSeverity::Warning,
                    json_pointer: effect_pointer,
                    object_id: Some(source_id),
                    otio_schema: effect_schema.into(),
                });
            }
        }
        Ok(())
    }

    fn track_semantics(
        &self,
        value: &Value,
        timeline_rate: Timebase,
        pointer: &str,
    ) -> Result<TrackSemantics> {
        match value.get("kind").and_then(Value::as_str).unwrap_or("Video") {
            "Video" => Ok(TrackSemantics::Video(VideoTrackSemantics::new(
                FrameRate::from_timebase(timeline_rate),
                VideoCompositing::Over,
            ))),
            "Audio" => {
                let layout = self.options.audio_channel_layout.clone();
                let routes = layout.positions().iter().copied().map(|position| {
                    AudioChannelRoute::new(position, AudioChannelTarget::Channel(position))
                });
                let routing =
                    AudioRouting::new(AudioRouteDestination::Main, layout.clone(), routes)?;
                Ok(TrackSemantics::Audio(AudioTrackSemantics::new(
                    self.options.audio_sample_rate,
                    layout,
                    routing,
                )?))
            }
            kind => Ok(TrackSemantics::Data(DataTrackSemantics::new(
                timeline_rate,
                DataSchema::new("urn:opentimelineio:track-kind", Some(kind)).map_err(|error| {
                    error.with_context(
                        ErrorContext::new("superi-timeline.otio", "import_track_kind")
                            .with_field("json_pointer", format!("{pointer}/kind")),
                    )
                })?,
            ))),
        }
    }

    fn register_identity(&mut self, value: &Value, pointer: &str) -> Result<String> {
        let id = source_identity(value, pointer);
        if !self.seen_source_ids.insert(id.clone()) {
            return Err(duplicate_identity(&id, pointer));
        }
        Ok(id)
    }

    fn allocate_raw(&mut self) -> u128 {
        let value = self.next_id;
        self.next_id += 1;
        value
    }

    fn timeline_id(&mut self) -> TimelineId {
        TimelineId::from_raw(self.allocate_raw())
    }

    fn track_id(&mut self) -> TrackId {
        TrackId::from_raw(self.allocate_raw())
    }

    fn clip_id(&mut self) -> ClipId {
        ClipId::from_raw(self.allocate_raw())
    }

    fn gap_id(&mut self) -> GapId {
        GapId::from_raw(self.allocate_raw())
    }

    fn transition_id(&mut self) -> TransitionId {
        TransitionId::from_raw(self.allocate_raw())
    }

    fn generator_id(&mut self) -> GeneratorId {
        GeneratorId::from_raw(self.allocate_raw())
    }

    fn marker_id(&mut self) -> MarkerId {
        MarkerId::from_raw(self.allocate_raw())
    }

    fn media_id(&mut self) -> MediaId {
        MediaId::from_raw(self.allocate_raw())
    }
}

struct TrackBuild {
    track: Track,
    annotations: Vec<PendingAnnotation>,
}

struct BuiltItem {
    item: TrackItem,
    annotations: Vec<PendingAnnotation>,
}

enum PendingAnnotation {
    Metadata(MetadataOwner, TimelineMetadata),
    Marker(Marker),
}

struct Exporter<'a> {
    document: &'a OtioDocument,
    target: OtioSchemaTarget,
}

impl<'a> Exporter<'a> {
    fn new(document: &'a OtioDocument, target: OtioSchemaTarget) -> Self {
        Self { document, target }
    }

    fn export(&self) -> Result<Value> {
        let timeline = self
            .document
            .project
            .timeline(self.document.root_timeline_id)
            .ok_or_else(|| {
                invalid_otio(
                    "export_timeline",
                    "OTIO root timeline is absent from the native project",
                    "",
                )
            })?;
        let mut root = object_template(&self.document.preserved.root_template);
        set_string(&mut root, "OTIO_SCHEMA", "Timeline.1");
        set_string(&mut root, "name", timeline.name());
        set_value(
            &mut root,
            "global_start_time",
            time_to_otio(timeline.global_start()),
        );
        self.merge_owner_metadata(&mut root, timeline, MetadataOwner::Timeline);
        let stack = self.build_stack(
            timeline,
            &self.document.preserved.root_stack_template,
            &self.document.preserved.root_stack_source_id,
            false,
        )?;
        set_value(&mut root, "tracks", stack);
        let source_id = self
            .document
            .preserved
            .timeline_source_ids
            .get(&timeline.id())
            .cloned()
            .unwrap_or_else(|| timeline.id().to_string());
        ensure_source_identity(&mut root, &source_id);
        let _ = self.target.schema_version_map();
        Ok(root)
    }

    fn build_stack(
        &self,
        timeline: &Timeline,
        template: &Value,
        source_id: &str,
        merge_timeline_metadata: bool,
    ) -> Result<Value> {
        let mut stack = object_template(template);
        set_string(&mut stack, "OTIO_SCHEMA", "Stack.1");
        let children = timeline
            .tracks()
            .iter()
            .map(|track| self.build_track(timeline, track))
            .collect::<Result<Vec<_>>>()?;
        set_value(&mut stack, "children", Value::Array(children));
        set_value(
            &mut stack,
            "markers",
            Value::Array(self.markers_for_owner(timeline, MarkerOwner::Timeline)?),
        );
        if merge_timeline_metadata {
            self.merge_owner_metadata(&mut stack, timeline, MetadataOwner::Timeline);
        }
        ensure_source_identity(&mut stack, source_id);
        Ok(stack)
    }

    fn build_track(&self, timeline: &Timeline, track: &Track) -> Result<Value> {
        let mut value = self
            .document
            .preserved
            .track_templates
            .get(&track.id())
            .map_or_else(|| json!({}), object_template);
        set_string(&mut value, "OTIO_SCHEMA", "Track.1");
        set_string(&mut value, "name", track.name());
        set_string(
            &mut value,
            "kind",
            match track.kind() {
                crate::model::TrackKind::Video => "Video",
                crate::model::TrackKind::Audio => "Audio",
                crate::model::TrackKind::Caption => "Text",
                crate::model::TrackKind::Data => "Data",
            },
        );
        let children = track
            .items()
            .iter()
            .map(|item| self.build_item(timeline, item))
            .collect::<Result<Vec<_>>>()?;
        set_value(&mut value, "children", Value::Array(children));
        set_value(
            &mut value,
            "markers",
            Value::Array(self.markers_for_owner(timeline, MarkerOwner::Track(track.id()))?),
        );
        self.merge_owner_metadata(&mut value, timeline, MetadataOwner::Track(track.id()));
        let source_id = self
            .document
            .preserved
            .track_source_ids
            .get(&track.id())
            .cloned()
            .unwrap_or_else(|| track.id().to_string());
        ensure_source_identity(&mut value, &source_id);
        Ok(value)
    }

    fn build_item(&self, timeline: &Timeline, item: &TrackItem) -> Result<Value> {
        let item_id = item.id();
        let mut value = self
            .document
            .preserved
            .item_templates
            .get(&item_id)
            .map_or_else(|| json!({}), object_template);
        match item {
            TrackItem::Clip(clip) => match clip.source() {
                ClipSource::Media(media_id) => {
                    set_string(&mut value, "OTIO_SCHEMA", "Clip.2");
                    set_string(&mut value, "name", clip.name());
                    set_value(
                        &mut value,
                        "source_range",
                        range_to_otio(clip.source_range()),
                    );
                    patch_linear_time_warp(&mut value, clip);
                    let active_key = value
                        .get("active_media_reference_key")
                        .and_then(Value::as_str)
                        .unwrap_or("DEFAULT_MEDIA")
                        .to_owned();
                    set_string(&mut value, "active_media_reference_key", &active_key);
                    let media =
                        self.document
                            .project
                            .media_reference(media_id)
                            .ok_or_else(|| {
                                invalid_otio(
                                    "export_media",
                                    "native clip references missing media",
                                    "",
                                )
                            })?;
                    let media_value = self.build_media(media);
                    let media_map = value
                        .as_object_mut()
                        .expect("object template")
                        .entry("media_references")
                        .or_insert_with(|| Value::Object(Map::new()));
                    if !media_map.is_object() {
                        *media_map = Value::Object(Map::new());
                    }
                    media_map
                        .as_object_mut()
                        .expect("media map normalized")
                        .insert(active_key, media_value);
                }
                ClipSource::Timeline(child_id) => {
                    let child = self.document.project.timeline(child_id).ok_or_else(|| {
                        invalid_otio("export_stack", "native nested timeline is missing", "")
                    })?;
                    let template = self
                        .document
                        .preserved
                        .timeline_templates
                        .get(&child_id)
                        .or_else(|| self.document.preserved.item_templates.get(&item_id))
                        .unwrap_or(&Value::Null);
                    let source_id = self
                        .document
                        .preserved
                        .timeline_source_ids
                        .get(&child_id)
                        .or_else(|| self.document.preserved.item_source_ids.get(&item_id))
                        .cloned()
                        .unwrap_or_else(|| child_id.to_string());
                    value = self.build_stack(child, template, &source_id, true)?;
                    set_string(&mut value, "name", clip.name());
                    set_value(
                        &mut value,
                        "source_range",
                        range_to_otio(clip.source_range()),
                    );
                }
            },
            TrackItem::Gap(gap) => {
                set_string(&mut value, "OTIO_SCHEMA", "Gap.1");
                set_string(&mut value, "name", gap.name());
                set_value(
                    &mut value,
                    "source_range",
                    zero_start_range_to_otio(gap.record_range().duration()),
                );
            }
            TrackItem::Transition(transition) => {
                set_string(&mut value, "OTIO_SCHEMA", "Transition.1");
                set_string(&mut value, "name", transition.name());
                set_value(
                    &mut value,
                    "in_offset",
                    duration_to_otio(transition.from_offset()),
                );
                set_value(
                    &mut value,
                    "out_offset",
                    duration_to_otio(transition.to_offset()),
                );
                let from_source = self
                    .document
                    .preserved
                    .item_source_ids
                    .get(&transition.from())
                    .cloned()
                    .unwrap_or_else(|| transition.from().to_string());
                let to_source = self
                    .document
                    .preserved
                    .item_source_ids
                    .get(&transition.to())
                    .cloned()
                    .unwrap_or_else(|| transition.to().to_string());
                set_superi_metadata_string(&mut value, "from_clip_id", &from_source);
                set_superi_metadata_string(&mut value, "to_clip_id", &to_source);
            }
            TrackItem::Generator(generator) => {
                if schema(&value).is_none() {
                    set_string(&mut value, "OTIO_SCHEMA", "UnknownSchema.1");
                }
                set_string(&mut value, "name", generator.name());
                set_value(
                    &mut value,
                    "source_range",
                    zero_start_range_to_otio(generator.record_range().duration()),
                );
            }
            TrackItem::Caption(caption) => {
                if schema(&value).is_none() {
                    set_string(&mut value, "OTIO_SCHEMA", "UnknownSchema.1");
                }
                set_string(&mut value, "name", caption.name());
                set_value(
                    &mut value,
                    "source_range",
                    zero_start_range_to_otio(caption.record_range().duration()),
                );
            }
        }
        if !matches!(item, TrackItem::Transition(_)) {
            set_value(
                &mut value,
                "markers",
                Value::Array(self.markers_for_owner(timeline, MarkerOwner::Object(item_id))?),
            );
        }
        self.merge_owner_metadata(&mut value, timeline, MetadataOwner::Object(item_id));
        let source_id = self
            .document
            .preserved
            .item_source_ids
            .get(&item_id)
            .cloned()
            .unwrap_or_else(|| item_id.to_string());
        ensure_source_identity(&mut value, &source_id);
        Ok(value)
    }

    fn build_media(&self, media: &LinkedMediaReference) -> Value {
        let mut value = self
            .document
            .preserved
            .media_templates
            .get(&media.id())
            .map_or_else(|| json!({}), object_template);
        if schema(&value).is_none() {
            set_string(&mut value, "OTIO_SCHEMA", "ExternalReference.1");
        }
        set_string(&mut value, "name", media.name());
        set_string(&mut value, "target_url", media.target());
        set_value(
            &mut value,
            "available_range",
            media.available_range().map_or(Value::Null, range_to_otio),
        );
        let source_id = self
            .document
            .preserved
            .media_source_ids
            .get(&media.id())
            .cloned()
            .unwrap_or_else(|| media.id().to_string());
        ensure_source_identity(&mut value, &source_id);
        value
    }

    fn markers_for_owner(&self, timeline: &Timeline, owner: MarkerOwner) -> Result<Vec<Value>> {
        timeline
            .markers()
            .filter(|marker| marker.owner() == owner)
            .map(|marker| self.build_marker(timeline, marker))
            .collect()
    }

    fn build_marker(&self, timeline: &Timeline, marker: &Marker) -> Result<Value> {
        let mut value = self
            .document
            .preserved
            .marker_templates
            .get(&marker.id())
            .map_or_else(|| json!({}), object_template);
        set_string(&mut value, "OTIO_SCHEMA", "Marker.2");
        set_string(
            &mut value,
            "name",
            marker.label().map_or("", MarkerLabel::as_str),
        );
        set_string(
            &mut value,
            "comment",
            marker.note().map_or("", MarkerNote::as_str),
        );
        set_value(
            &mut value,
            "color",
            marker.flag().map_or(Value::Null, |flag| {
                Value::String(marker_flag_to_otio(flag).into())
            }),
        );
        set_value(
            &mut value,
            "marked_range",
            range_to_otio(marker.marked_range()),
        );
        if !marker.metadata().is_empty() {
            merge_metadata(&mut value, marker.metadata());
        }
        if let Some(metadata) = timeline.metadata(MetadataOwner::Marker(marker.id())) {
            merge_metadata(&mut value, metadata);
        }
        let source_id = self
            .document
            .preserved
            .marker_source_ids
            .get(&marker.id())
            .cloned()
            .unwrap_or_else(|| marker.id().to_string());
        ensure_source_identity(&mut value, &source_id);
        Ok(value)
    }

    fn merge_owner_metadata(&self, value: &mut Value, timeline: &Timeline, owner: MetadataOwner) {
        if let Some(metadata) = timeline.metadata(owner) {
            merge_metadata(value, metadata);
        }
    }
}

fn timeline_edit_rate(root: &Value, tracks: &Value) -> Result<Timebase> {
    if let Some(global) = root
        .get("global_start_time")
        .filter(|value| !value.is_null())
    {
        return rate_from_time(global, "/global_start_time");
    }
    Ok(discover_rate(tracks).unwrap_or(FrameRate::FPS_24.timebase()))
}

fn discover_rate(value: &Value) -> Option<Timebase> {
    if let Some(range) = value.get("source_range").filter(|range| !range.is_null()) {
        if let Ok(rate) = range_rate(range, "/source_range") {
            return Some(rate);
        }
    }
    if let Some(children) = value.get("children").and_then(Value::as_array) {
        return children.iter().find_map(discover_rate);
    }
    None
}

fn patch_linear_time_warp(value: &mut Value, clip: &Clip) {
    let segments = clip.time_map().segments();
    if segments.len() != 1 {
        return;
    }
    let rate = segments[0].rate();
    let scalar = rate.numerator() as f64 / rate.denominator() as f64;
    let object = value.as_object_mut().expect("object template");
    let effects = object
        .entry("effects")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !effects.is_array() {
        *effects = Value::Array(Vec::new());
    }
    let effects = effects.as_array_mut().expect("effects normalized");
    if let Some(effect) = effects
        .iter_mut()
        .find(|effect| schema(effect) == Some("LinearTimeWarp.1"))
    {
        set_value(effect, "time_scalar", json!(scalar));
    } else if rate != PlaybackRate::NORMAL {
        effects.push(json!({
            "OTIO_SCHEMA": "LinearTimeWarp.1",
            "effect_name": "LinearTimeWarp",
            "enabled": true,
            "metadata": {
                "superi": {
                    "id": format!("effect.{}.linear-time-warp", clip.id())
                }
            },
            "name": "superi-linear-time-warp",
            "time_scalar": scalar
        }));
    }
}

fn linear_time_warp(value: &Value, pointer: &str) -> Result<Option<PlaybackRate>> {
    let Some(effects) = value.get("effects").and_then(Value::as_array) else {
        return Ok(None);
    };
    let mut found = None;
    for (index, effect) in effects.iter().enumerate() {
        if schema(effect) != Some("LinearTimeWarp.1") {
            continue;
        }
        if found.is_some() {
            return Err(invalid_otio(
                "import_retime",
                "a clip may contain only one native LinearTimeWarp",
                &format!("{pointer}/effects/{index}"),
            ));
        }
        let scalar = effect
            .get("time_scalar")
            .and_then(Value::as_number)
            .ok_or_else(|| {
                invalid_otio(
                    "import_retime",
                    "LinearTimeWarp time_scalar must be numeric",
                    &format!("{pointer}/effects/{index}/time_scalar"),
                )
            })?;
        let (numerator, denominator) = decimal_rational(scalar)?;
        let numerator = i64::try_from(numerator).map_err(|_| {
            invalid_otio(
                "import_retime",
                "LinearTimeWarp scalar exceeds the native signed range",
                &format!("{pointer}/effects/{index}/time_scalar"),
            )
        })?;
        let denominator = u64::try_from(denominator).map_err(|_| {
            invalid_otio(
                "import_retime",
                "LinearTimeWarp scalar exceeds the native denominator range",
                &format!("{pointer}/effects/{index}/time_scalar"),
            )
        })?;
        found = Some(PlaybackRate::new(numerator, denominator)?);
    }
    Ok(found)
}

fn range_rate(value: &Value, pointer: &str) -> Result<Timebase> {
    let start = required_member(value, "start_time", &format!("{pointer}/start_time"))?;
    rate_from_time(start, &format!("{pointer}/start_time"))
}

fn rate_from_time(value: &Value, pointer: &str) -> Result<Timebase> {
    let rate = value
        .get("rate")
        .and_then(Value::as_number)
        .ok_or_else(|| invalid_otio("import_time", "OTIO time rate must be numeric", pointer))?;
    rate_from_number(rate, pointer)
}

fn rate_from_number(value: &Number, pointer: &str) -> Result<Timebase> {
    let approximate = value
        .as_f64()
        .ok_or_else(|| invalid_otio("import_time", "OTIO time rate is not finite", pointer))?;
    for (candidate, numerator, denominator) in [
        (24_000.0 / 1_001.0, 24_000, 1_001),
        (30_000.0 / 1_001.0, 30_000, 1_001),
        (60_000.0 / 1_001.0, 60_000, 1_001),
    ] {
        if (approximate - candidate).abs() <= 1.0e-9 {
            return Timebase::new(numerator, denominator);
        }
    }
    let (numerator, denominator) = decimal_rational(value)?;
    if numerator <= 0 {
        return Err(invalid_otio(
            "import_time",
            "OTIO time rate must be greater than zero",
            pointer,
        ));
    }
    let numerator = u32::try_from(numerator).map_err(|_| {
        invalid_otio(
            "import_time",
            "OTIO time rate numerator exceeds the native range",
            pointer,
        )
    })?;
    let denominator = u32::try_from(denominator).map_err(|_| {
        invalid_otio(
            "import_time",
            "OTIO time rate denominator exceeds the native range",
            pointer,
        )
    })?;
    Timebase::new(numerator, denominator)
}

fn parse_range_at(value: &Value, target: Timebase, pointer: &str) -> Result<TimeRange> {
    let start = parse_time_at(
        required_member(value, "start_time", &format!("{pointer}/start_time"))?,
        target,
        &format!("{pointer}/start_time"),
    )?;
    let duration = parse_duration_at(
        required_member(value, "duration", &format!("{pointer}/duration"))?,
        target,
        &format!("{pointer}/duration"),
    )?;
    TimeRange::new(start, duration)
}

fn parse_duration_at(value: &Value, target: Timebase, pointer: &str) -> Result<Duration> {
    let time = parse_time_at(value, target, pointer)?;
    let coordinate = u64::try_from(time.value()).map_err(|_| {
        invalid_otio(
            "import_duration",
            "OTIO duration must be nonnegative",
            pointer,
        )
    })?;
    Duration::new(coordinate, target)
}

fn parse_time_at(value: &Value, target: Timebase, pointer: &str) -> Result<RationalTime> {
    let coordinate = value
        .get("value")
        .and_then(Value::as_number)
        .ok_or_else(|| invalid_otio("import_time", "OTIO time value must be numeric", pointer))?;
    let rate_number = value
        .get("rate")
        .and_then(Value::as_number)
        .ok_or_else(|| invalid_otio("import_time", "OTIO time rate must be numeric", pointer))?;
    let source_rate = rate_from_number(rate_number, pointer)?;
    let (value_numerator, value_denominator) = decimal_rational(coordinate)?;
    let numerator = value_numerator
        .checked_mul(i128::from(source_rate.denominator()))
        .and_then(|value| value.checked_mul(i128::from(target.numerator())))
        .ok_or_else(|| invalid_otio("import_time", "OTIO time conversion overflowed", pointer))?;
    let denominator = value_denominator
        .checked_mul(i128::from(source_rate.numerator()))
        .and_then(|value| value.checked_mul(i128::from(target.denominator())))
        .ok_or_else(|| invalid_otio("import_time", "OTIO time conversion overflowed", pointer))?;
    if denominator == 0 || numerator % denominator != 0 {
        return Err(invalid_otio(
            "import_time",
            "OTIO time is not exactly representable on the native target clock",
            pointer,
        ));
    }
    let coordinate = i64::try_from(numerator / denominator).map_err(|_| {
        invalid_otio(
            "import_time",
            "OTIO time coordinate exceeds the native signed range",
            pointer,
        )
    })?;
    Ok(RationalTime::new(coordinate, target))
}

fn decimal_rational(value: &Number) -> Result<(i128, i128)> {
    let text = value.to_string();
    let (mantissa, exponent) = text
        .split_once(['e', 'E'])
        .map_or((text.as_str(), 0_i32), |(mantissa, exponent)| {
            (mantissa, exponent.parse::<i32>().unwrap_or(i32::MAX))
        });
    if exponent == i32::MAX {
        return Err(invalid_otio(
            "parse_number",
            "OTIO number has an invalid exponent",
            "",
        ));
    }
    let negative = mantissa.starts_with('-');
    let unsigned = mantissa.trim_start_matches(['-', '+']);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let digits = format!("{whole}{fraction}");
    let mut numerator = digits.parse::<i128>().map_err(|source| {
        Error::with_source(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "OTIO number exceeds the supported decimal range",
            source,
        )
        .with_context(ErrorContext::new("superi-timeline.otio", "parse_number"))
    })?;
    if negative {
        numerator = -numerator;
    }
    let fractional_digits = i32::try_from(fraction.len()).map_err(|_| {
        invalid_otio(
            "parse_number",
            "OTIO decimal precision exceeds the supported range",
            "",
        )
    })?;
    let scale = fractional_digits
        .checked_sub(exponent)
        .ok_or_else(|| invalid_otio("parse_number", "OTIO decimal exponent overflowed", ""))?;
    let (numerator, denominator) = if scale >= 0 {
        let denominator = checked_power_of_ten(scale as u32)?;
        (numerator, denominator)
    } else {
        let multiplier = checked_power_of_ten(scale.unsigned_abs())?;
        (
            numerator.checked_mul(multiplier).ok_or_else(|| {
                invalid_otio("parse_number", "OTIO decimal exponent overflowed", "")
            })?,
            1,
        )
    };
    let magnitude = numerator.checked_abs().ok_or_else(|| {
        invalid_otio(
            "parse_number",
            "OTIO decimal magnitude exceeds the supported signed range",
            "",
        )
    })?;
    let divisor = gcd_i128(magnitude, denominator);
    Ok((numerator / divisor, denominator / divisor))
}

fn checked_power_of_ten(exponent: u32) -> Result<i128> {
    (0..exponent).try_fold(1_i128, |value, _| {
        value.checked_mul(10).ok_or_else(|| {
            invalid_otio(
                "parse_number",
                "OTIO decimal precision exceeds the supported range",
                "",
            )
        })
    })
}

fn gcd_i128(mut left: i128, mut right: i128) -> i128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

fn metadata_from_object(value: &Value) -> Result<TimelineMetadata> {
    value
        .get("metadata")
        .map(metadata_from_json)
        .transpose()
        .map(Option::unwrap_or_default)
}

fn metadata_from_json(value: &Value) -> Result<TimelineMetadata> {
    let Some(object) = value.as_object() else {
        return Ok(TimelineMetadata::new());
    };
    let mut entries = Vec::new();
    for (key, value) in object {
        let Ok(key) = MetadataKey::new(key) else {
            continue;
        };
        entries.push((key, metadata_value_from_json(value)?));
    }
    Ok(TimelineMetadata::from_entries(entries))
}

fn metadata_value_from_json(value: &Value) -> Result<MetadataValue> {
    match value {
        Value::Null => Ok(MetadataValue::Null),
        Value::Bool(value) => Ok(MetadataValue::Boolean(*value)),
        Value::Number(value) if value.is_i64() => Ok(MetadataValue::Signed(
            value.as_i64().expect("number classified as signed"),
        )),
        Value::Number(value) if value.is_u64() => Ok(MetadataValue::Unsigned(
            value.as_u64().expect("number classified as unsigned"),
        )),
        Value::Number(value) => Ok(MetadataValue::Float(FiniteF64::new(
            value.as_f64().ok_or_else(|| {
                invalid_otio("import_metadata", "OTIO metadata number is not finite", "")
            })?,
        )?)),
        Value::String(value) => Ok(MetadataValue::Text(value.clone())),
        Value::Array(values) => values
            .iter()
            .map(metadata_value_from_json)
            .collect::<Result<Vec<_>>>()
            .map(MetadataValue::List),
        Value::Object(_) => metadata_from_json(value).map(MetadataValue::Map),
    }
}

fn metadata_to_json(metadata: &TimelineMetadata) -> Value {
    Value::Object(
        metadata
            .iter()
            .map(|(key, value)| (key.as_str().to_owned(), metadata_value_to_json(value)))
            .collect(),
    )
}

fn metadata_value_to_json(value: &MetadataValue) -> Value {
    match value {
        MetadataValue::Null => Value::Null,
        MetadataValue::Boolean(value) => Value::Bool(*value),
        MetadataValue::Signed(value) => Value::Number(Number::from(*value)),
        MetadataValue::Unsigned(value) => Value::Number(Number::from(*value)),
        MetadataValue::Float(value) => {
            Number::from_f64(value.get()).map_or(Value::Null, Value::Number)
        }
        MetadataValue::Text(value) => Value::String(value.clone()),
        MetadataValue::Time(value) => time_to_otio(*value),
        MetadataValue::Range(value) => range_to_otio(*value),
        MetadataValue::List(values) => {
            Value::Array(values.iter().map(metadata_value_to_json).collect())
        }
        MetadataValue::Map(value) => metadata_to_json(value),
    }
}

fn merge_metadata(value: &mut Value, metadata: &TimelineMetadata) {
    if metadata.is_empty() {
        return;
    }
    let overlay = metadata_to_json(metadata);
    let object = value.as_object_mut().expect("object template");
    let base = object
        .entry("metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    merge_json(base, overlay);
}

fn merge_json(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                if let Some(existing) = base.get_mut(&key) {
                    merge_json(existing, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

fn marker_flag_from_otio(value: &str) -> Option<MarkerFlag> {
    MarkerFlag::from_code(&value.to_ascii_lowercase())
}

fn marker_flag_to_otio(value: MarkerFlag) -> &'static str {
    match value {
        MarkerFlag::Red => "RED",
        MarkerFlag::Pink => "PINK",
        MarkerFlag::Orange => "ORANGE",
        MarkerFlag::Yellow => "YELLOW",
        MarkerFlag::Green => "GREEN",
        MarkerFlag::Cyan => "CYAN",
        MarkerFlag::Blue => "BLUE",
        MarkerFlag::Purple => "PURPLE",
        MarkerFlag::Magenta => "MAGENTA",
        MarkerFlag::Black => "BLACK",
        MarkerFlag::White => "WHITE",
    }
}

fn time_to_otio(value: RationalTime) -> Value {
    json!({
        "OTIO_SCHEMA": "RationalTime.1",
        "rate": f64::from(value.timebase().numerator()) / f64::from(value.timebase().denominator()),
        "value": value.value() as f64,
    })
}

fn duration_to_otio(value: Duration) -> Value {
    time_to_otio(value.rational_time())
}

fn range_to_otio(value: TimeRange) -> Value {
    json!({
        "OTIO_SCHEMA": "TimeRange.1",
        "duration": duration_to_otio(value.duration()),
        "start_time": time_to_otio(value.start()),
    })
}

fn zero_start_range_to_otio(value: Duration) -> Value {
    range_to_otio(
        TimeRange::new(RationalTime::zero(value.timebase()), value)
            .expect("zero and duration share a timebase"),
    )
}

fn source_identity(value: &Value, pointer: &str) -> String {
    explicit_source_identity(value)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}@{}", schema(value).unwrap_or("UnknownSchema.1"), pointer))
}

fn explicit_source_identity(value: &Value) -> Option<&str> {
    value
        .get("metadata")
        .and_then(|value| value.get("superi"))
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn ensure_source_identity(value: &mut Value, source_id: &str) {
    let object = value.as_object_mut().expect("object template");
    let metadata = object
        .entry("metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    let superi = metadata
        .as_object_mut()
        .expect("metadata normalized")
        .entry("superi")
        .or_insert_with(|| Value::Object(Map::new()));
    if !superi.is_object() {
        *superi = Value::Object(Map::new());
    }
    superi
        .as_object_mut()
        .expect("superi metadata normalized")
        .insert("id".into(), Value::String(source_id.into()));
}

fn set_superi_metadata_string(value: &mut Value, key: &str, member: &str) {
    let object = value.as_object_mut().expect("object template");
    let metadata = object
        .entry("metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if !metadata.is_object() {
        *metadata = Value::Object(Map::new());
    }
    let superi = metadata
        .as_object_mut()
        .expect("metadata normalized")
        .entry("superi")
        .or_insert_with(|| Value::Object(Map::new()));
    if !superi.is_object() {
        *superi = Value::Object(Map::new());
    }
    superi
        .as_object_mut()
        .expect("superi metadata normalized")
        .insert(key.into(), Value::String(member.into()));
}

fn object_template(value: &Value) -> Value {
    if value.is_object() {
        value.clone()
    } else {
        Value::Object(Map::new())
    }
}

fn set_value(value: &mut Value, key: &str, member: Value) {
    value
        .as_object_mut()
        .expect("object template")
        .insert(key.into(), member);
}

fn set_string(value: &mut Value, key: &str, member: &str) {
    set_value(value, key, Value::String(member.into()));
}

fn schema(value: &Value) -> Option<&str> {
    value.get("OTIO_SCHEMA").and_then(Value::as_str)
}

fn require_schema(value: &Value, expected: &str, pointer: &str) -> Result<()> {
    match schema(value) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(invalid_otio(
            "validate_schema",
            format!("expected {expected}, found {actual}"),
            pointer,
        )),
        None => Err(invalid_otio(
            "validate_schema",
            format!("expected {expected}, found no OTIO_SCHEMA"),
            pointer,
        )),
    }
}

fn required_member<'a>(value: &'a Value, key: &str, pointer: &str) -> Result<&'a Value> {
    value
        .get(key)
        .filter(|value| !value.is_null())
        .ok_or_else(|| {
            invalid_otio(
                "read_member",
                format!("OTIO object is missing required member {key}"),
                pointer,
            )
        })
}

fn array_member<'a>(value: &'a Value, key: &str, pointer: &str) -> Result<&'a [Value]> {
    required_member(value, key, pointer)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| {
            invalid_otio(
                "read_array",
                format!("OTIO member {key} must be an array"),
                pointer,
            )
        })
}

fn required_name(value: &Value, fallback: &str) -> String {
    value
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| fallback.to_owned())
}

fn escape_json_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn duplicate_identity(identity: &str, pointer: &str) -> Error {
    invalid_otio(
        "register_identity",
        format!("duplicate OTIO identity {identity}"),
        pointer,
    )
}

fn invalid_otio(operation: &'static str, message: impl Into<String>, pointer: &str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-timeline.otio", operation).with_field("json_pointer", pointer),
    )
}
