//! Timeline-local markers, deterministic metadata, and exact snapping.

use std::collections::{BTreeMap, BTreeSet};

use superi_core::diagnostics::FiniteF64;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{MarkerId, TrackId};
use superi_core::time::{Duration, RationalTime, TimeRange, TimeRounding, Timebase};

use crate::model::{EditorialObjectId, Track, TrackItem};

/// The stable editorial object that owns a marker's authored time range.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MarkerOwner {
    /// Record coordinates on this timeline.
    Timeline,
    /// Record coordinates on one timeline track.
    Track(TrackId),
    /// Coordinates relative to one stable timed object's record start.
    Object(EditorialObjectId),
}

/// The stable local owner of one deterministic metadata map.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MetadataOwner {
    /// Metadata describing the timeline itself.
    Timeline,
    /// Metadata describing one track.
    Track(TrackId),
    /// Metadata describing one timed object.
    Object(EditorialObjectId),
    /// Metadata carried by one marker.
    Marker(MarkerId),
}

/// One canonical key in an editorial metadata map.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MetadataKey(String);

impl MetadataKey {
    /// Creates a nonblank, whitespace-free metadata key.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty()
            || value.trim() != value
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':')
            })
        {
            return Err(invalid(
                "create_metadata_key",
                "metadata key must be nonblank canonical ASCII without whitespace",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the canonical key text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One exact, recursively nestable metadata value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MetadataValue {
    /// An explicit empty value.
    Null,
    /// A Boolean value.
    Boolean(bool),
    /// A signed integer.
    Signed(i64),
    /// An unsigned integer.
    Unsigned(u64),
    /// A finite floating-point value with canonical zero.
    Float(FiniteF64),
    /// UTF-8 text.
    Text(String),
    /// An exact rational coordinate.
    Time(RationalTime),
    /// An exact rational range.
    Range(TimeRange),
    /// An ordered list.
    List(Vec<Self>),
    /// A deterministically ordered nested map.
    Map(TimelineMetadata),
}

/// A deterministic metadata map used by timeline editorial state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TimelineMetadata(BTreeMap<MetadataKey, MetadataValue>);

impl TimelineMetadata {
    /// Creates an empty metadata map.
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Creates a map from entries, replacing duplicate keys with their last value.
    #[must_use]
    pub fn from_entries<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = (MetadataKey, MetadataValue)>,
    {
        Self(entries.into_iter().collect())
    }

    /// Inserts or replaces one value.
    pub fn insert(&mut self, key: MetadataKey, value: MetadataValue) -> Option<MetadataValue> {
        self.0.insert(key, value)
    }

    /// Looks up one value.
    #[must_use]
    pub fn get(&self, key: &MetadataKey) -> Option<&MetadataValue> {
        self.0.get(key)
    }

    /// Removes one value.
    pub fn remove(&mut self, key: &MetadataKey) -> Option<MetadataValue> {
        self.0.remove(key)
    }

    /// Iterates keys in canonical order.
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &MetadataKey> {
        self.0.keys()
    }

    /// Iterates key-value pairs in canonical order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&MetadataKey, &MetadataValue)> {
        self.0.iter()
    }

    /// Returns whether the map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// An editor-facing marker label.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkerLabel(String);

impl MarkerLabel {
    /// Creates a nonblank single-line label.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(invalid(
                "create_marker_label",
                "marker label must be nonblank visible text",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the exact authored label.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An editor-facing marker note.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkerNote(String);

impl MarkerNote {
    /// Creates a nonblank note without embedded null bytes.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() || value.contains('\0') {
            return Err(invalid(
                "create_marker_note",
                "marker note must be nonblank text without null bytes",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the exact authored note.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The portable visible marker flag palette.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MarkerFlag {
    /// Red.
    Red,
    /// Pink.
    Pink,
    /// Orange.
    Orange,
    /// Yellow.
    Yellow,
    /// Green.
    Green,
    /// Cyan.
    Cyan,
    /// Blue.
    Blue,
    /// Purple.
    Purple,
    /// Magenta.
    Magenta,
    /// Black.
    Black,
    /// White.
    White,
}

impl MarkerFlag {
    /// Every portable marker flag in permanent code order.
    pub const ALL: [Self; 11] = [
        Self::Red,
        Self::Pink,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Cyan,
        Self::Blue,
        Self::Purple,
        Self::Magenta,
        Self::Black,
        Self::White,
    ];

    /// Returns the permanent lowercase flag code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Pink => "pink",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Cyan => "cyan",
            Self::Blue => "blue",
            Self::Purple => "purple",
            Self::Magenta => "magenta",
            Self::Black => "black",
            Self::White => "white",
        }
    }

    /// Looks up one portable flag by permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|flag| flag.code() == code)
    }
}

/// One stable marker with explicit ownership and visible editorial semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Marker {
    id: MarkerId,
    owner: MarkerOwner,
    marked_range: TimeRange,
    label: Option<MarkerLabel>,
    flag: Option<MarkerFlag>,
    note: Option<MarkerNote>,
    metadata: TimelineMetadata,
}

impl Marker {
    /// Creates a marker. Owner clock compatibility is checked by its timeline.
    pub fn new(id: MarkerId, owner: MarkerOwner, marked_range: TimeRange) -> Result<Self> {
        if marked_range.start().is_negative() {
            return Err(invalid(
                "create_marker",
                "marker range must not start before its owner zero",
            ));
        }
        Ok(Self {
            id,
            owner,
            marked_range,
            label: None,
            flag: None,
            note: None,
            metadata: TimelineMetadata::new(),
        })
    }

    /// Returns the stable marker identity.
    #[must_use]
    pub const fn id(&self) -> MarkerId {
        self.id
    }

    /// Returns the stable local owner.
    #[must_use]
    pub const fn owner(&self) -> MarkerOwner {
        self.owner
    }

    /// Returns the exact authored owner-relative range.
    #[must_use]
    pub const fn marked_range(&self) -> TimeRange {
        self.marked_range
    }

    /// Returns the visible label.
    #[must_use]
    pub fn label(&self) -> Option<&MarkerLabel> {
        self.label.as_ref()
    }

    /// Returns the visible flag.
    #[must_use]
    pub const fn flag(&self) -> Option<MarkerFlag> {
        self.flag
    }

    /// Returns the authored note.
    #[must_use]
    pub fn note(&self) -> Option<&MarkerNote> {
        self.note.as_ref()
    }

    /// Returns the marker's deterministic metadata.
    #[must_use]
    pub const fn metadata(&self) -> &TimelineMetadata {
        &self.metadata
    }

    /// Replaces the stable local owner.
    pub fn set_owner(&mut self, owner: MarkerOwner) {
        self.owner = owner;
    }

    /// Replaces the authored owner-relative range.
    pub fn set_marked_range(&mut self, marked_range: TimeRange) -> Result<()> {
        if marked_range.start().is_negative() {
            return Err(invalid(
                "set_marker_range",
                "marker range must not start before its owner zero",
            ));
        }
        self.marked_range = marked_range;
        Ok(())
    }

    /// Replaces or clears the visible label.
    pub fn set_label(&mut self, label: Option<MarkerLabel>) {
        self.label = label;
    }

    /// Replaces or clears the visible flag.
    pub fn set_flag(&mut self, flag: Option<MarkerFlag>) {
        self.flag = flag;
    }

    /// Replaces or clears the authored note.
    pub fn set_note(&mut self, note: Option<MarkerNote>) {
        self.note = note;
    }

    /// Replaces the complete marker metadata map.
    pub fn set_metadata(&mut self, metadata: TimelineMetadata) {
        self.metadata = metadata;
    }
}

/// A category of exact snap target.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum SnapTargetKind {
    /// Timeline record zero.
    TimelineStart,
    /// Caller-supplied playhead coordinate.
    Playhead,
    /// Timed object inclusive start.
    ItemStart,
    /// Timed object exclusive end.
    ItemEnd,
    /// Marker inclusive start.
    MarkerStart,
    /// Marker exclusive end.
    MarkerEnd,
}

impl SnapTargetKind {
    const ALL: [Self; 6] = [
        Self::TimelineStart,
        Self::Playhead,
        Self::ItemStart,
        Self::ItemEnd,
        Self::MarkerStart,
        Self::MarkerEnd,
    ];
}

/// The stable identity of one resolved snap target.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum SnapTarget {
    /// Timeline record zero.
    TimelineStart,
    /// Caller-supplied playhead.
    Playhead,
    /// Timed object inclusive start.
    ItemStart(EditorialObjectId),
    /// Timed object exclusive end.
    ItemEnd(EditorialObjectId),
    /// Marker inclusive start.
    MarkerStart(MarkerId),
    /// Marker exclusive end.
    MarkerEnd(MarkerId),
}

impl SnapTarget {
    const fn kind(self) -> SnapTargetKind {
        match self {
            Self::TimelineStart => SnapTargetKind::TimelineStart,
            Self::Playhead => SnapTargetKind::Playhead,
            Self::ItemStart(_) => SnapTargetKind::ItemStart,
            Self::ItemEnd(_) => SnapTargetKind::ItemEnd,
            Self::MarkerStart(_) => SnapTargetKind::MarkerStart,
            Self::MarkerEnd(_) => SnapTargetKind::MarkerEnd,
        }
    }
}

/// One exact snapping query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapRequest {
    at: RationalTime,
    tolerance: Duration,
    playhead: Option<RationalTime>,
    target_kinds: BTreeSet<SnapTargetKind>,
    excluded_objects: BTreeSet<EditorialObjectId>,
    excluded_markers: BTreeSet<MarkerId>,
}

impl SnapRequest {
    /// Creates a request that considers every target category.
    #[must_use]
    pub fn new(at: RationalTime, tolerance: Duration) -> Self {
        Self {
            at,
            tolerance,
            playhead: None,
            target_kinds: SnapTargetKind::ALL.into_iter().collect(),
            excluded_objects: BTreeSet::new(),
            excluded_markers: BTreeSet::new(),
        }
    }

    /// Adds a playhead candidate.
    #[must_use]
    pub fn with_playhead(mut self, playhead: RationalTime) -> Self {
        self.playhead = Some(playhead);
        self
    }

    /// Replaces the included target categories.
    #[must_use]
    pub fn with_target_kinds<I>(mut self, target_kinds: I) -> Self
    where
        I: IntoIterator<Item = SnapTargetKind>,
    {
        self.target_kinds = target_kinds.into_iter().collect();
        self
    }

    /// Excludes one timed object and markers owned by it.
    #[must_use]
    pub fn excluding_object(mut self, object: EditorialObjectId) -> Self {
        self.excluded_objects.insert(object);
        self
    }

    /// Excludes one marker.
    #[must_use]
    pub fn excluding_marker(mut self, marker: MarkerId) -> Self {
        self.excluded_markers.insert(marker);
        self
    }
}

/// The deterministic result of one exact snap query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapMatch {
    target: SnapTarget,
    time: RationalTime,
    distance: Duration,
}

impl SnapMatch {
    /// Returns the stable target identity.
    #[must_use]
    pub const fn target(self) -> SnapTarget {
        self.target
    }

    /// Returns the target coordinate in the request clock.
    #[must_use]
    pub const fn time(self) -> RationalTime {
        self.time
    }

    /// Returns the absolute distance in the request clock.
    #[must_use]
    pub const fn distance(self) -> Duration {
        self.distance
    }
}

/// Timeline-owned annotation state published with the editorial snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineAnnotations {
    snapping_enabled: bool,
    markers: BTreeMap<MarkerId, Marker>,
    metadata: BTreeMap<MetadataOwner, TimelineMetadata>,
}

impl Default for TimelineAnnotations {
    fn default() -> Self {
        Self {
            snapping_enabled: true,
            markers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        }
    }
}

impl TimelineAnnotations {
    /// Returns whether snapping is enabled for ordinary edit queries.
    #[must_use]
    pub const fn snapping_enabled(&self) -> bool {
        self.snapping_enabled
    }

    /// Sets the persistent snapping preference.
    pub fn set_snapping_enabled(&mut self, enabled: bool) {
        self.snapping_enabled = enabled;
    }

    /// Looks up one marker by stable identity.
    #[must_use]
    pub fn marker(&self, id: MarkerId) -> Option<&Marker> {
        self.markers.get(&id)
    }

    /// Mutably looks up one marker inside an unpublished project draft.
    pub fn marker_mut(&mut self, id: MarkerId) -> Result<&mut Marker> {
        self.markers.get_mut(&id).ok_or_else(|| {
            not_found(
                "find_marker",
                "editorial marker was not found",
                "marker",
                id,
            )
        })
    }

    /// Iterates markers in stable identity order.
    pub fn markers(&self) -> impl ExactSizeIterator<Item = &Marker> {
        self.markers.values()
    }

    pub(crate) fn upsert_marker(
        &mut self,
        marker: Marker,
        edit_rate: Timebase,
        tracks: &[Track],
    ) -> Result<Option<Marker>> {
        validate_marker(&marker, edit_rate, tracks)?;
        Ok(self.markers.insert(marker.id(), marker))
    }

    /// Removes one marker and its attached metadata.
    pub fn remove_marker(&mut self, id: MarkerId) -> Option<Marker> {
        self.markers.remove(&id)
    }

    pub(crate) fn set_metadata(
        &mut self,
        owner: MetadataOwner,
        metadata: TimelineMetadata,
        tracks: &[Track],
    ) -> Result<Option<TimelineMetadata>> {
        validate_metadata_owner(owner, tracks, &self.markers)?;
        if let MetadataOwner::Marker(id) = owner {
            let marker = self.markers.get_mut(&id).expect("owner validated");
            return Ok(Some(std::mem::replace(&mut marker.metadata, metadata)));
        }
        Ok(self.metadata.insert(owner, metadata))
    }

    /// Looks up metadata by stable local owner.
    #[must_use]
    pub fn metadata(&self, owner: MetadataOwner) -> Option<&TimelineMetadata> {
        match owner {
            MetadataOwner::Marker(id) => self.markers.get(&id).map(Marker::metadata),
            _ => self.metadata.get(&owner),
        }
    }

    /// Removes one owner's metadata.
    pub fn remove_metadata(&mut self, owner: MetadataOwner) -> Option<TimelineMetadata> {
        match owner {
            MetadataOwner::Marker(id) => self
                .markers
                .get_mut(&id)
                .map(|marker| std::mem::replace(&mut marker.metadata, TimelineMetadata::new())),
            _ => self.metadata.remove(&owner),
        }
    }

    pub(crate) fn resolved_marker_range(
        &self,
        id: MarkerId,
        edit_rate: Timebase,
        tracks: &[Track],
    ) -> Result<Option<TimeRange>> {
        let marker = self.markers.get(&id).ok_or_else(|| {
            not_found(
                "resolve_marker_range",
                "editorial marker was not found",
                "marker",
                id,
            )
        })?;
        resolve_marker_range(marker, edit_rate, tracks)
    }

    pub(crate) fn snap(
        &self,
        request: &SnapRequest,
        edit_rate: Timebase,
        tracks: &[Track],
    ) -> Result<Option<SnapMatch>> {
        if request.at.timebase() != request.tolerance.timebase() {
            return Err(invalid(
                "snap",
                "snap coordinate and tolerance must use the same clock",
            ));
        }
        if !self.snapping_enabled {
            return Ok(None);
        }

        let mut candidates = Vec::new();
        candidates.push((SnapTarget::TimelineStart, RationalTime::zero(edit_rate)));
        if let Some(playhead) = request.playhead {
            candidates.push((SnapTarget::Playhead, playhead));
        }
        for track in tracks {
            for item in track.items() {
                if request.excluded_objects.contains(&item.id()) {
                    continue;
                }
                let Some(range) = item.record_range() else {
                    continue;
                };
                candidates.push((SnapTarget::ItemStart(item.id()), range.start()));
                candidates.push((SnapTarget::ItemEnd(item.id()), range.end_exclusive()?));
            }
        }
        for marker in self.markers.values() {
            if request.excluded_markers.contains(&marker.id())
                || matches!(
                    marker.owner(),
                    MarkerOwner::Object(object) if request.excluded_objects.contains(&object)
                )
            {
                continue;
            }
            let Some(range) = resolve_marker_range(marker, edit_rate, tracks)? else {
                continue;
            };
            candidates.push((SnapTarget::MarkerStart(marker.id()), range.start()));
            candidates.push((SnapTarget::MarkerEnd(marker.id()), range.end_exclusive()?));
        }

        let mut best: Option<(u128, SnapTarget, RationalTime)> = None;
        for (target, time) in candidates {
            if !request.target_kinds.contains(&target.kind()) {
                continue;
            }
            let Ok(time) = time.checked_rescale(request.at.timebase(), TimeRounding::Exact) else {
                continue;
            };
            let distance =
                (i128::from(time.value()) - i128::from(request.at.value())).unsigned_abs();
            if distance > u128::from(request.tolerance.value()) {
                continue;
            }
            let key = (distance, target, time);
            if match best {
                Some(current) => key < current,
                None => true,
            } {
                best = Some(key);
            }
        }
        best.map(|(distance, target, time)| {
            let distance = u64::try_from(distance).map_err(|_| {
                invalid("snap", "snap distance exceeds the supported duration range")
            })?;
            Ok(SnapMatch {
                target,
                time,
                distance: Duration::new(distance, request.at.timebase())?,
            })
        })
        .transpose()
    }

    pub(crate) fn reconcile(&mut self, tracks: &[Track]) {
        self.markers
            .retain(|_, marker| marker_owner_exists(marker.owner(), tracks));
        self.metadata
            .retain(|owner, _| metadata_owner_exists(*owner, tracks, &self.markers));
    }

    pub(crate) fn validate(
        &self,
        edit_rate: Timebase,
        tracks: &[Track],
        marker_ids: &mut BTreeSet<MarkerId>,
    ) -> Result<()> {
        for marker in self.markers.values() {
            if !marker_ids.insert(marker.id()) {
                return Err(conflict(
                    "validate_markers",
                    "duplicate marker identity",
                    "marker",
                    marker.id(),
                ));
            }
            validate_marker(marker, edit_rate, tracks)?;
        }
        for owner in self.metadata.keys().copied() {
            validate_metadata_owner(owner, tracks, &self.markers)?;
        }
        Ok(())
    }
}

fn validate_marker(marker: &Marker, edit_rate: Timebase, tracks: &[Track]) -> Result<()> {
    let clock = owner_clock(marker.owner(), edit_rate, tracks).ok_or_else(|| {
        not_found(
            "validate_marker",
            "marker owner was not found",
            "marker",
            marker.id(),
        )
    })?;
    if marker.marked_range().timebase() != clock {
        return Err(invalid(
            "validate_marker",
            "marker range must use its owner's exact record clock",
        ));
    }
    if marker.marked_range().start().is_negative() {
        return Err(invalid(
            "validate_marker",
            "marker range must not start before its owner zero",
        ));
    }
    Ok(())
}

fn resolve_marker_range(
    marker: &Marker,
    edit_rate: Timebase,
    tracks: &[Track],
) -> Result<Option<TimeRange>> {
    validate_marker(marker, edit_rate, tracks)?;
    match marker.owner() {
        MarkerOwner::Timeline | MarkerOwner::Track(_) => Ok(Some(marker.marked_range())),
        MarkerOwner::Object(object) => {
            let (_, item) = find_object(tracks, object).expect("marker owner validated");
            let record_range = item.record_range().ok_or_else(|| {
                invalid(
                    "resolve_marker_range",
                    "object marker owner must have a record range",
                )
            })?;
            if marker.marked_range().end_exclusive()?.value()
                > i64::try_from(record_range.duration().value()).unwrap_or(i64::MAX)
            {
                return Ok(None);
            }
            let start = record_range.start().checked_add_at(
                marker.marked_range().start(),
                record_range.timebase(),
                TimeRounding::Exact,
            )?;
            Ok(Some(TimeRange::new(
                start,
                marker.marked_range().duration(),
            )?))
        }
    }
}

fn owner_clock(owner: MarkerOwner, edit_rate: Timebase, tracks: &[Track]) -> Option<Timebase> {
    match owner {
        MarkerOwner::Timeline => Some(edit_rate),
        MarkerOwner::Track(track_id) => tracks
            .iter()
            .find(|track| track.id() == track_id)
            .map(|track| track.semantics().timebase()),
        MarkerOwner::Object(object) => find_object(tracks, object)
            .and_then(|(track, item)| item.record_range().map(|_| track.semantics().timebase())),
    }
}

fn marker_owner_exists(owner: MarkerOwner, tracks: &[Track]) -> bool {
    match owner {
        MarkerOwner::Timeline => true,
        MarkerOwner::Track(track_id) => tracks.iter().any(|track| track.id() == track_id),
        MarkerOwner::Object(object) => find_object(tracks, object).is_some(),
    }
}

fn find_object(tracks: &[Track], id: EditorialObjectId) -> Option<(&Track, &TrackItem)> {
    tracks
        .iter()
        .find_map(|track| track.item(id).map(|item| (track, item)))
}

fn validate_metadata_owner(
    owner: MetadataOwner,
    tracks: &[Track],
    markers: &BTreeMap<MarkerId, Marker>,
) -> Result<()> {
    if metadata_owner_exists(owner, tracks, markers) {
        return Ok(());
    }
    Err(not_found(
        "validate_metadata_owner",
        "metadata owner was not found",
        "owner",
        format!("{owner:?}"),
    ))
}

fn metadata_owner_exists(
    owner: MetadataOwner,
    tracks: &[Track],
    markers: &BTreeMap<MarkerId, Marker>,
) -> bool {
    match owner {
        MetadataOwner::Timeline => true,
        MetadataOwner::Track(track_id) => tracks.iter().any(|track| track.id() == track_id),
        MetadataOwner::Object(object) => find_object(tracks, object).is_some(),
        MetadataOwner::Marker(marker_id) => markers.contains_key(&marker_id),
    }
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.markers", operation))
}

fn not_found(
    operation: &'static str,
    message: impl Into<String>,
    field: &'static str,
    value: impl ToString,
) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-timeline.markers", operation)
            .with_field(field, value.to_string()),
    )
}

fn conflict(
    operation: &'static str,
    message: impl Into<String>,
    field: &'static str,
    value: impl ToString,
) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-timeline.markers", operation)
            .with_field(field, value.to_string()),
    )
}
