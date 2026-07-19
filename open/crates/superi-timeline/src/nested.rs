//! Atomic nested-sequence and compound-clip operations.

use std::collections::{BTreeMap, BTreeSet};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ClipId, TimelineId, TrackId};
use superi_core::time::{RationalTime, TimeRange, TimeRounding};

use crate::edit_ops::{apply_operation, EditOperation, EditOutcome};
use crate::edit_state::{SelectionExpansion, SelectionUpdate, TrackEditState};
use crate::markers::{MarkerOwner, MetadataOwner, TimelineMetadata};
use crate::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, ProjectDraft, Timeline, Track, TrackItem,
};

/// How a nested sequence clip is placed on its parent track.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum NestedSequencePlacement {
    /// Insert the nested material and ripple later parent-track content right.
    Insert {
        /// Exact insertion point in the parent track clock.
        at: RationalTime,
        /// Caller-owned identities for right fragments created at the boundary.
        fragment_ids: Vec<EditorialObjectId>,
    },
    /// Overwrite parent-track material for the nested source duration.
    Overwrite {
        /// Exact overwrite point in the parent track clock.
        at: RationalTime,
        /// Caller-owned identities for right fragments created at the boundary.
        fragment_ids: Vec<EditorialObjectId>,
    },
    /// Append the nested material at the current parent-track end.
    Append,
    /// Replace one exact equal-duration timed object.
    Replace {
        /// Stable identity of the parent-track object being replaced.
        target_id: EditorialObjectId,
    },
}

impl NestedSequencePlacement {
    /// Creates an insert placement.
    pub fn insert<I>(at: RationalTime, fragment_ids: I) -> Self
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        Self::Insert {
            at,
            fragment_ids: fragment_ids.into_iter().collect(),
        }
    }

    /// Creates an overwrite placement.
    pub fn overwrite<I>(at: RationalTime, fragment_ids: I) -> Self
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        Self::Overwrite {
            at,
            fragment_ids: fragment_ids.into_iter().collect(),
        }
    }

    /// Creates an append placement.
    #[must_use]
    pub const fn append() -> Self {
        Self::Append
    }

    /// Creates an exact-object replacement placement.
    #[must_use]
    pub const fn replace(target_id: EditorialObjectId) -> Self {
        Self::Replace { target_id }
    }
}

/// Complete caller-owned intent for one nested sequence clip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NestedSequenceRequest {
    parent_timeline_id: TimelineId,
    parent_track_id: TrackId,
    clip_id: ClipId,
    name: String,
    source_range: TimeRange,
    placement: NestedSequencePlacement,
}

impl NestedSequenceRequest {
    /// Creates a nested sequence request with explicit parent and source timing.
    pub fn new(
        parent_timeline_id: TimelineId,
        parent_track_id: TrackId,
        clip_id: ClipId,
        name: impl Into<String>,
        source_range: TimeRange,
        placement: NestedSequencePlacement,
    ) -> Self {
        Self {
            parent_timeline_id,
            parent_track_id,
            clip_id,
            name: name.into(),
            source_range,
            placement,
        }
    }

    /// Returns the parent timeline identity.
    #[must_use]
    pub const fn parent_timeline_id(&self) -> TimelineId {
        self.parent_timeline_id
    }

    /// Returns the parent track identity.
    #[must_use]
    pub const fn parent_track_id(&self) -> TrackId {
        self.parent_track_id
    }

    /// Returns the stable nested clip identity.
    #[must_use]
    pub const fn clip_id(&self) -> ClipId {
        self.clip_id
    }

    /// Returns the editor-facing nested clip name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the exact range selected from the child timeline.
    #[must_use]
    pub const fn source_range(&self) -> TimeRange {
        self.source_range
    }

    /// Returns the requested parent-track placement.
    #[must_use]
    pub const fn placement(&self) -> &NestedSequencePlacement {
        &self.placement
    }
}

/// Caller-owned identities for one parent track represented inside a compound clip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompoundClipTrackRequest {
    parent_track_id: TrackId,
    compound_track_id: TrackId,
    clip_id: ClipId,
}

impl CompoundClipTrackRequest {
    /// Creates one exact parent-to-child track mapping and parent instance identity.
    #[must_use]
    pub const fn new(
        parent_track_id: TrackId,
        compound_track_id: TrackId,
        clip_id: ClipId,
    ) -> Self {
        Self {
            parent_track_id,
            compound_track_id,
            clip_id,
        }
    }

    /// Returns the parent track whose selected span is moved.
    #[must_use]
    pub const fn parent_track_id(self) -> TrackId {
        self.parent_track_id
    }

    /// Returns the new track identity inside the compound timeline.
    #[must_use]
    pub const fn compound_track_id(self) -> TrackId {
        self.compound_track_id
    }

    /// Returns the new nested clip identity placed on the parent track.
    #[must_use]
    pub const fn clip_id(self) -> ClipId {
        self.clip_id
    }
}

/// Complete intent for turning exact parent objects into one compound timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompoundClipRequest {
    parent_timeline_id: TimelineId,
    compound_timeline_id: TimelineId,
    name: String,
    selected_objects: Vec<EditorialObjectId>,
    tracks: Vec<CompoundClipTrackRequest>,
}

impl CompoundClipRequest {
    /// Creates a selection-based compound request with explicit stable identities.
    pub fn new<S, T>(
        parent_timeline_id: TimelineId,
        compound_timeline_id: TimelineId,
        name: impl Into<String>,
        selected_objects: S,
        tracks: T,
    ) -> Self
    where
        S: IntoIterator<Item = EditorialObjectId>,
        T: IntoIterator<Item = CompoundClipTrackRequest>,
    {
        Self {
            parent_timeline_id,
            compound_timeline_id,
            name: name.into(),
            selected_objects: selected_objects.into_iter().collect(),
            tracks: tracks.into_iter().collect(),
        }
    }

    /// Returns the parent timeline whose objects are selected.
    #[must_use]
    pub const fn parent_timeline_id(&self) -> TimelineId {
        self.parent_timeline_id
    }

    /// Returns the new compound timeline identity.
    #[must_use]
    pub const fn compound_timeline_id(&self) -> TimelineId {
        self.compound_timeline_id
    }

    /// Returns the editor-facing compound name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the exact parent objects to move into the compound.
    #[must_use]
    pub fn selected_objects(&self) -> &[EditorialObjectId] {
        &self.selected_objects
    }

    /// Returns caller-owned identities for every affected parent track.
    #[must_use]
    pub fn tracks(&self) -> &[CompoundClipTrackRequest] {
        &self.tracks
    }
}

/// One directly inspectable parent-to-child timeline relationship.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NestedSequenceLink {
    parent_timeline_id: TimelineId,
    parent_track_id: TrackId,
    clip_id: ClipId,
    source_timeline_id: TimelineId,
    source_range: TimeRange,
    record_range: TimeRange,
    depth: usize,
}

impl NestedSequenceLink {
    /// Returns the timeline that owns the nested clip.
    #[must_use]
    pub const fn parent_timeline_id(&self) -> TimelineId {
        self.parent_timeline_id
    }

    /// Returns the track that owns the nested clip.
    #[must_use]
    pub const fn parent_track_id(&self) -> TrackId {
        self.parent_track_id
    }

    /// Returns the stable nested clip identity.
    #[must_use]
    pub const fn clip_id(&self) -> ClipId {
        self.clip_id
    }

    /// Returns the directly linked child timeline identity.
    #[must_use]
    pub const fn source_timeline_id(&self) -> TimelineId {
        self.source_timeline_id
    }

    /// Returns the exact range selected from the child timeline.
    #[must_use]
    pub const fn source_range(&self) -> TimeRange {
        self.source_range
    }

    /// Returns the exact placement range in the parent track.
    #[must_use]
    pub const fn record_range(&self) -> TimeRange {
        self.record_range
    }

    /// Returns the zero-based nesting depth from an inspected root timeline.
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }
}

/// Result of atomically placing a nested sequence clip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NestedSequenceResult {
    revision: u64,
    source_timeline_id: TimelineId,
    outcome: EditOutcome,
}

impl NestedSequenceResult {
    /// Returns the published project revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the linked child timeline identity.
    #[must_use]
    pub const fn source_timeline_id(&self) -> TimelineId {
        self.source_timeline_id
    }

    /// Returns the exact parent-track edit outcome.
    #[must_use]
    pub const fn outcome(&self) -> &EditOutcome {
        &self.outcome
    }
}

/// Result of atomically creating a compound timeline from parent selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompoundClipResult {
    revision: u64,
    compound_timeline_id: TimelineId,
    clip_ids: Vec<ClipId>,
    outcomes: Vec<EditOutcome>,
}

impl CompoundClipResult {
    /// Returns the published project revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the new compound timeline identity.
    #[must_use]
    pub const fn compound_timeline_id(&self) -> TimelineId {
        self.compound_timeline_id
    }

    /// Returns parent nested clip identities in canonical parent-track order.
    #[must_use]
    pub fn clip_ids(&self) -> &[ClipId] {
        &self.clip_ids
    }

    /// Returns one exact overwrite outcome per affected parent track.
    #[must_use]
    pub fn outcomes(&self) -> &[EditOutcome] {
        &self.outcomes
    }
}

/// Result of directly editing one child timeline through any of its instances.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NestedSequenceEditResult {
    revision: u64,
    source_timeline_id: TimelineId,
    instances: Vec<NestedSequenceLink>,
}

impl NestedSequenceEditResult {
    /// Returns the published project revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the edited child timeline identity.
    #[must_use]
    pub const fn source_timeline_id(&self) -> TimelineId {
        self.source_timeline_id
    }

    /// Returns every current project instance of the edited child.
    #[must_use]
    pub fn instances(&self) -> &[NestedSequenceLink] {
        &self.instances
    }
}

/// Places an existing project timeline as directly editable nested material.
pub fn place_nested_sequence(
    project: &mut EditorialProject,
    expected_revision: u64,
    source_timeline_id: TimelineId,
    request: NestedSequenceRequest,
) -> Result<NestedSequenceResult> {
    let mut outcome = None;
    project.edit(expected_revision, |draft| {
        if draft.timeline(source_timeline_id).is_none() {
            return Err(not_found(
                "place_nested_sequence",
                "nested source timeline was not found",
                "timeline",
                source_timeline_id,
            ));
        }
        outcome = Some(apply_nested_placement(draft, source_timeline_id, &request)?);
        Ok(())
    })?;
    Ok(NestedSequenceResult {
        revision: project.revision(),
        source_timeline_id,
        outcome: outcome.expect("successful edit produced one nested placement"),
    })
}

/// Creates a new compound timeline and places its parent clip in one project revision.
pub fn create_compound_clip(
    project: &mut EditorialProject,
    expected_revision: u64,
    compound_timeline: Timeline,
    request: NestedSequenceRequest,
) -> Result<NestedSequenceResult> {
    let source_timeline_id = compound_timeline.id();
    let mut outcome = None;
    project.edit(expected_revision, |draft| {
        if draft.timeline(source_timeline_id).is_some() {
            return Err(conflict(
                "create_compound_clip",
                "compound timeline identity already exists",
                "timeline",
                source_timeline_id,
            ));
        }
        draft.upsert_timeline(compound_timeline);
        outcome = Some(apply_nested_placement(draft, source_timeline_id, &request)?);
        Ok(())
    })?;
    Ok(NestedSequenceResult {
        revision: project.revision(),
        source_timeline_id,
        outcome: outcome.expect("successful edit produced one compound placement"),
    })
}

/// Moves one exact parent selection into a new nested compound timeline.
///
/// The complete child, every parent instance, relationship reconciliation, and
/// selection transfer publish through one project revision or roll back together.
pub fn create_compound_clip_from_selection(
    project: &mut EditorialProject,
    expected_revision: u64,
    request: CompoundClipRequest,
) -> Result<CompoundClipResult> {
    let mut outcomes = Vec::new();
    let mut clip_ids = Vec::new();
    project.edit(expected_revision, |draft| {
        if draft.timeline(request.compound_timeline_id).is_some() {
            return Err(conflict(
                "create_selection_compound",
                "compound timeline identity already exists",
                "timeline",
                request.compound_timeline_id,
            ));
        }
        let parent = draft
            .timeline(request.parent_timeline_id)
            .cloned()
            .ok_or_else(|| {
                not_found(
                    "create_selection_compound",
                    "parent timeline was not found",
                    "timeline",
                    request.parent_timeline_id,
                )
            })?;
        let prepared = prepare_selection_compound(&parent, &request)?;
        draft.upsert_timeline(prepared.timeline);
        for placement in &prepared.placements {
            outcomes.push(apply_nested_placement(
                draft,
                request.compound_timeline_id,
                &placement.request,
            )?);
            clip_ids.push(placement.request.clip_id());
        }
        let parent = draft.timeline_mut(request.parent_timeline_id)?;
        if clip_ids.len() > 1 {
            parent.link_clips(clip_ids.iter().copied())?;
            parent.group_clips(clip_ids.iter().copied())?;
        }
        parent.update_selection(
            clip_ids.iter().copied().map(EditorialObjectId::Clip),
            SelectionUpdate::Add,
            SelectionExpansion::Direct,
        )?;
        Ok(())
    })?;
    Ok(CompoundClipResult {
        revision: project.revision(),
        compound_timeline_id: request.compound_timeline_id,
        clip_ids,
        outcomes,
    })
}

/// Edits a child timeline through one stable parent clip identity.
///
/// Every current instance is validated before publication and returned after
/// the edit, so a shared nested source cannot change one parent silently.
pub fn edit_nested_sequence<F>(
    project: &mut EditorialProject,
    expected_revision: u64,
    instance_clip_id: ClipId,
    edit: F,
) -> Result<NestedSequenceEditResult>
where
    F: FnOnce(&mut Timeline) -> Result<()>,
{
    let source_timeline_id = nested_link_for_clip(project, instance_clip_id)?.source_timeline_id;
    project.edit(expected_revision, |draft| {
        let timeline = draft.timeline_mut(source_timeline_id)?;
        edit(timeline)
    })?;
    Ok(NestedSequenceEditResult {
        revision: project.revision(),
        source_timeline_id,
        instances: nested_sequence_instances(project, source_timeline_id),
    })
}

/// Returns every project clip that directly instances one child timeline.
#[must_use]
pub fn nested_sequence_instances(
    project: &EditorialProject,
    source_timeline_id: TimelineId,
) -> Vec<NestedSequenceLink> {
    project
        .timelines()
        .flat_map(|timeline| {
            timeline.tracks().iter().flat_map(move |track| {
                track.items().iter().filter_map(move |item| {
                    let clip = item.as_clip()?;
                    (clip.source() == ClipSource::Timeline(source_timeline_id)).then_some(
                        NestedSequenceLink {
                            parent_timeline_id: timeline.id(),
                            parent_track_id: track.id(),
                            clip_id: clip.id(),
                            source_timeline_id,
                            source_range: clip.source_range(),
                            record_range: clip.record_range(),
                            depth: 0,
                        },
                    )
                })
            })
        })
        .collect()
}

/// Returns the complete recursive nesting tree in parent track and item order.
pub fn nested_sequence_tree(
    project: &EditorialProject,
    root_timeline_id: TimelineId,
) -> Result<Vec<NestedSequenceLink>> {
    if project.timeline(root_timeline_id).is_none() {
        return Err(not_found(
            "inspect_nested_sequence_tree",
            "root timeline was not found",
            "timeline",
            root_timeline_id,
        ));
    }
    let mut links = Vec::new();
    collect_nested_links(project, root_timeline_id, 0, &mut links)?;
    Ok(links)
}

struct PreparedCompound {
    timeline: Timeline,
    placements: Vec<PreparedCompoundPlacement>,
}

struct PreparedCompoundPlacement {
    request: NestedSequenceRequest,
}

fn prepare_selection_compound(
    parent: &Timeline,
    request: &CompoundClipRequest,
) -> Result<PreparedCompound> {
    if request.selected_objects.is_empty() {
        return Err(invalid(
            "create_selection_compound",
            "compound selection must contain at least one timed object",
        ));
    }
    let selected: BTreeSet<_> = request.selected_objects.iter().copied().collect();
    if selected.len() != request.selected_objects.len() {
        return Err(invalid(
            "create_selection_compound",
            "compound selection must not repeat an object identity",
        ));
    }

    let mut locations = BTreeMap::new();
    for track in parent.tracks() {
        for item in track.items() {
            let Some(record_range) = item.record_range() else {
                if selected.contains(&item.id()) {
                    return Err(invalid(
                        "create_selection_compound",
                        "transitions cannot be direct compound selection boundaries",
                    ));
                }
                continue;
            };
            locations.insert(item.id(), (track.id(), record_range));
        }
    }

    let mut affected_tracks = BTreeSet::new();
    let mut span_start = None;
    let mut span_end = None;
    for object_id in &selected {
        let (track_id, record_range) = locations.get(object_id).copied().ok_or_else(|| {
            not_found(
                "create_selection_compound",
                "selected parent object was not found",
                "object",
                object_id,
            )
        })?;
        let record_range = record_range.checked_rescale(parent.edit_rate(), TimeRounding::Exact)?;
        let end = record_range.end_exclusive()?;
        affected_tracks.insert(track_id);
        span_start = Some(match span_start {
            Some(current) if current <= record_range.start() => current,
            _ => record_range.start(),
        });
        span_end = Some(match span_end {
            Some(current) if current >= end => current,
            _ => end,
        });
    }
    let common_span = TimeRange::from_start_end(
        span_start.expect("nonempty selection produced a start"),
        span_end.expect("nonempty selection produced an end"),
    )?;
    if common_span.is_empty() {
        return Err(invalid(
            "create_selection_compound",
            "compound selection must cover a nonempty exact record span",
        ));
    }

    let selected_clip_ids: BTreeSet<_> = selected
        .iter()
        .filter_map(|object| match object {
            EditorialObjectId::Clip(id) => Some(*id),
            _ => None,
        })
        .collect();
    reject_multicam_selection(parent, &selected_clip_ids)?;
    let links = retained_relations(
        parent
            .edit_state()
            .links()
            .map(|relation| relation.members()),
        &selected_clip_ids,
    )?;
    let groups = retained_relations(
        parent
            .edit_state()
            .groups()
            .map(|relation| relation.members()),
        &selected_clip_ids,
    )?;

    let mut mappings = BTreeMap::new();
    let mut compound_track_ids = BTreeSet::new();
    let mut compound_clip_ids = BTreeSet::new();
    for mapping in &request.tracks {
        if mappings.insert(mapping.parent_track_id, *mapping).is_some() {
            return Err(invalid(
                "create_selection_compound",
                "each affected parent track must have exactly one compound mapping",
            ));
        }
        if !compound_track_ids.insert(mapping.compound_track_id) {
            return Err(invalid(
                "create_selection_compound",
                "compound track identities must be unique",
            ));
        }
        if !compound_clip_ids.insert(mapping.clip_id)
            || selected_clip_ids.contains(&mapping.clip_id)
        {
            return Err(invalid(
                "create_selection_compound",
                "parent compound clip identities must be unique and newly owned",
            ));
        }
    }
    if mappings.len() != affected_tracks.len()
        || mappings
            .keys()
            .any(|track_id| !affected_tracks.contains(track_id))
        || affected_tracks
            .iter()
            .any(|track_id| !mappings.contains_key(track_id))
    {
        return Err(invalid(
            "create_selection_compound",
            "compound track mappings must cover every affected parent track exactly",
        ));
    }

    let source_range = TimeRange::new(
        RationalTime::zero(parent.edit_rate()),
        common_span.duration(),
    )?;
    let mut child_tracks = Vec::with_capacity(affected_tracks.len());
    let mut child_states = Vec::with_capacity(affected_tracks.len());
    let mut placements = Vec::with_capacity(affected_tracks.len());
    let mut track_metadata: Vec<(MetadataOwner, TimelineMetadata)> = Vec::new();
    for parent_track in parent.tracks() {
        if !affected_tracks.contains(&parent_track.id()) {
            continue;
        }
        let mapping = mappings
            .get(&parent_track.id())
            .copied()
            .expect("mapping coverage checked");
        let state = parent
            .edit_state()
            .track_state(parent_track.id())
            .expect("validated timeline covers every track state");
        if state.locked() {
            return Err(conflict(
                "create_selection_compound",
                "locked parent tracks cannot be compounded",
                "track",
                parent_track.id(),
            ));
        }
        let track_span = common_span
            .checked_rescale(parent_track.semantics().timebase(), TimeRounding::Exact)?;
        let items = selected_track_items(parent_track, track_span, &selected)?;
        child_tracks.push(Track::new(
            mapping.compound_track_id,
            parent_track.name(),
            parent_track.semantics().clone(),
            items,
        ));
        child_states.push((mapping.compound_track_id, state));
        if let Some(metadata) = parent
            .metadata(MetadataOwner::Track(parent_track.id()))
            .cloned()
        {
            track_metadata.push((MetadataOwner::Track(mapping.compound_track_id), metadata));
        }
        placements.push(PreparedCompoundPlacement {
            request: NestedSequenceRequest::new(
                parent.id(),
                parent_track.id(),
                mapping.clip_id,
                &request.name,
                source_range,
                NestedSequencePlacement::overwrite(track_span.start(), []),
            ),
        });
    }

    let mut timeline = Timeline::new(
        request.compound_timeline_id,
        &request.name,
        parent.edit_rate(),
        RationalTime::zero(parent.edit_rate()),
        child_tracks,
    );
    timeline.set_snapping_enabled(parent.snapping_enabled());
    timeline.set_linked_selection_enabled(parent.edit_state().linked_selection_enabled());
    for (track_id, state) in child_states {
        apply_compound_track_state(&mut timeline, track_id, state)?;
    }
    for relation in links {
        timeline.link_clips(relation)?;
    }
    for relation in groups {
        timeline.group_clips(relation)?;
    }
    timeline.update_selection(
        selected.iter().copied(),
        SelectionUpdate::Replace,
        SelectionExpansion::Direct,
    )?;
    for marker in parent.markers() {
        if matches!(marker.owner(), MarkerOwner::Object(owner) if selected.contains(&owner)) {
            timeline.upsert_marker(marker.clone())?;
        }
    }
    for object_id in &selected {
        if let Some(metadata) = parent.metadata(MetadataOwner::Object(*object_id)).cloned() {
            timeline.set_metadata(MetadataOwner::Object(*object_id), metadata)?;
        }
    }
    for (owner, metadata) in track_metadata {
        timeline.set_metadata(owner, metadata)?;
    }

    Ok(PreparedCompound {
        timeline,
        placements,
    })
}

fn selected_track_items(
    track: &Track,
    span: TimeRange,
    selected: &BTreeSet<EditorialObjectId>,
) -> Result<Vec<TrackItem>> {
    let mut included = BTreeSet::new();
    for item in track.items() {
        let Some(record_range) = item.record_range() else {
            continue;
        };
        let intersects = record_range.intersects(span)?;
        if selected.contains(&item.id()) && !intersects {
            return Err(invalid(
                "create_selection_compound",
                "selected object does not intersect the common compound span",
            ));
        }
        if !intersects {
            continue;
        }
        if record_range.start() < span.start()
            || record_range.end_exclusive()? > span.end_exclusive()?
        {
            return Err(invalid(
                "create_selection_compound",
                "compound boundaries must align to complete timed objects",
            ));
        }
        if !matches!(item, TrackItem::Gap(_)) && !selected.contains(&item.id()) {
            return Err(invalid(
                "create_selection_compound",
                "every non-gap object inside the compound span must be selected",
            ));
        }
        included.insert(item.id());
    }
    let included_ranges: Vec<_> = track
        .items()
        .iter()
        .filter(|item| included.contains(&item.id()))
        .filter_map(TrackItem::record_range)
        .collect();
    let first = included_ranges.first().copied().ok_or_else(|| {
        invalid(
            "create_selection_compound",
            "each affected track must contribute timed material",
        )
    })?;
    let last = included_ranges
        .last()
        .copied()
        .expect("first included range exists");
    if first.start() != span.start() || last.end_exclusive()? != span.end_exclusive()? {
        return Err(invalid(
            "create_selection_compound",
            "each affected track must cover the complete common compound span",
        ));
    }

    let mut output = Vec::new();
    for item in track.items() {
        match item {
            TrackItem::Transition(transition) => {
                let from = included.contains(&transition.from());
                let to = included.contains(&transition.to());
                if from != to {
                    return Err(invalid(
                        "create_selection_compound",
                        "a transition cannot cross a compound boundary",
                    ));
                }
                if from {
                    output.push(item.clone());
                }
            }
            _ if included.contains(&item.id()) => {
                output.push(rebase_compound_item(item.clone(), span.start())?);
            }
            _ => {}
        }
    }
    Ok(output)
}

fn rebase_compound_item(mut item: TrackItem, origin: RationalTime) -> Result<TrackItem> {
    let record_range = item
        .record_range()
        .expect("compound rebasing receives one timed item");
    let start = record_range.start().checked_sub_at(
        origin,
        record_range.timebase(),
        TimeRounding::Exact,
    )?;
    let rebased = TimeRange::new(start, record_range.duration())?;
    match &mut item {
        TrackItem::Clip(clip) => clip.set_record_range(rebased)?,
        TrackItem::Gap(gap) => gap.set_record_range(rebased),
        TrackItem::Generator(generator) => generator.set_record_range(rebased),
        TrackItem::Caption(caption) => caption.set_record_range(rebased),
        TrackItem::Transition(_) => unreachable!("transitions are not rebased directly"),
    }
    Ok(item)
}

fn retained_relations<I, M>(relations: I, selected: &BTreeSet<ClipId>) -> Result<Vec<Vec<ClipId>>>
where
    I: IntoIterator<Item = M>,
    M: IntoIterator<Item = ClipId>,
{
    let mut retained = Vec::new();
    for relation in relations {
        let members: Vec<_> = relation.into_iter().collect();
        let selected_count = members
            .iter()
            .filter(|clip_id| selected.contains(clip_id))
            .count();
        if selected_count > 0 && selected_count != members.len() {
            return Err(invalid(
                "create_selection_compound",
                "clip links and groups cannot cross a compound boundary",
            ));
        }
        if selected_count == members.len() && selected_count > 0 {
            retained.push(members);
        }
    }
    Ok(retained)
}

fn reject_multicam_selection(parent: &Timeline, selected: &BTreeSet<ClipId>) -> Result<()> {
    let source_member = parent.multicam_source().is_some_and(|source| {
        source.angles().iter().any(|angle| {
            angle
                .source_clips()
                .iter()
                .any(|clip_id| selected.contains(clip_id))
        })
    });
    let target_member = selected
        .iter()
        .any(|clip_id| parent.multicam_clip(*clip_id).is_some());
    if source_member || target_member {
        return Err(invalid(
            "create_selection_compound",
            "multicam source or switch intent must be resolved before compounding",
        ));
    }
    Ok(())
}

fn apply_compound_track_state(
    timeline: &mut Timeline,
    track_id: TrackId,
    state: TrackEditState,
) -> Result<()> {
    timeline.set_track_height(track_id, state.height())?;
    timeline.set_track_targeted(track_id, state.targeted())?;
    timeline.set_track_sync_locked(track_id, state.sync_locked())?;
    timeline.set_track_muted(track_id, state.muted())?;
    timeline.set_track_solo(track_id, state.solo())?;
    timeline.set_track_enabled(track_id, state.enabled())?;
    timeline.set_track_locked(track_id, state.locked())
}

fn apply_nested_placement(
    draft: &mut ProjectDraft,
    source_timeline_id: TimelineId,
    request: &NestedSequenceRequest,
) -> Result<EditOutcome> {
    let source = draft.timeline(source_timeline_id).ok_or_else(|| {
        not_found(
            "resolve_nested_source",
            "nested source timeline was not found",
            "timeline",
            source_timeline_id,
        )
    })?;
    if request.source_range.timebase() != source.edit_rate() {
        return Err(invalid(
            "place_nested_sequence",
            "nested source range must use the child timeline edit rate",
        ));
    }
    let parent = draft.timeline(request.parent_timeline_id).ok_or_else(|| {
        not_found(
            "resolve_nested_parent",
            "parent timeline was not found",
            "timeline",
            request.parent_timeline_id,
        )
    })?;
    let parent_track = parent.track(request.parent_track_id).ok_or_else(|| {
        not_found(
            "resolve_nested_parent",
            "parent track was not found",
            "track",
            request.parent_track_id,
        )
    })?;
    let parent_timebase = parent_track.semantics().timebase();
    let record_duration = request
        .source_range
        .duration()
        .checked_rescale(parent_timebase, TimeRounding::Exact)?;
    let material = TrackItem::Clip(Clip::new(
        request.clip_id,
        &request.name,
        ClipSource::Timeline(source_timeline_id),
        request.source_range,
        TimeRange::new(RationalTime::zero(parent_timebase), record_duration)?,
    )?);
    let operation = match &request.placement {
        NestedSequencePlacement::Insert { at, fragment_ids } => EditOperation::insert(
            request.parent_timeline_id,
            request.parent_track_id,
            *at,
            material,
            fragment_ids.iter().copied(),
        ),
        NestedSequencePlacement::Overwrite { at, fragment_ids } => EditOperation::overwrite(
            request.parent_timeline_id,
            request.parent_track_id,
            *at,
            material,
            fragment_ids.iter().copied(),
        ),
        NestedSequencePlacement::Append => EditOperation::append(
            request.parent_timeline_id,
            request.parent_track_id,
            material,
        ),
        NestedSequencePlacement::Replace { target_id } => EditOperation::replace(
            request.parent_timeline_id,
            request.parent_track_id,
            *target_id,
            material,
        ),
    };
    apply_operation(draft, &operation)
}

fn nested_link_for_clip(project: &EditorialProject, clip_id: ClipId) -> Result<NestedSequenceLink> {
    for timeline in project.timelines() {
        for track in timeline.tracks() {
            let Some(item) = track.item(EditorialObjectId::Clip(clip_id)) else {
                continue;
            };
            let clip = item
                .as_clip()
                .expect("typed clip identity resolved to a clip");
            let ClipSource::Timeline(source_timeline_id) = clip.source() else {
                return Err(invalid(
                    "edit_nested_sequence",
                    "requested clip is not sourced from a child timeline",
                ));
            };
            return Ok(NestedSequenceLink {
                parent_timeline_id: timeline.id(),
                parent_track_id: track.id(),
                clip_id,
                source_timeline_id,
                source_range: clip.source_range(),
                record_range: clip.record_range(),
                depth: 0,
            });
        }
    }
    Err(not_found(
        "edit_nested_sequence",
        "nested sequence clip was not found",
        "clip",
        clip_id,
    ))
}

fn collect_nested_links(
    project: &EditorialProject,
    parent_timeline_id: TimelineId,
    depth: usize,
    output: &mut Vec<NestedSequenceLink>,
) -> Result<()> {
    let timeline = project
        .timeline(parent_timeline_id)
        .expect("validated nested source exists");
    for track in timeline.tracks() {
        for item in track.items() {
            let Some(clip) = item.as_clip() else {
                continue;
            };
            let ClipSource::Timeline(source_timeline_id) = clip.source() else {
                continue;
            };
            output.push(NestedSequenceLink {
                parent_timeline_id,
                parent_track_id: track.id(),
                clip_id: clip.id(),
                source_timeline_id,
                source_range: clip.source_range(),
                record_range: clip.record_range(),
                depth,
            });
            let child_depth = depth.checked_add(1).ok_or_else(|| {
                invalid(
                    "inspect_nested_sequence_tree",
                    "nested sequence depth exceeds the supported range",
                )
            })?;
            collect_nested_links(project, source_timeline_id, child_depth, output)?;
        }
    }
    Ok(())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.nested", operation))
}

fn not_found(
    operation: &'static str,
    message: &'static str,
    field: &'static str,
    value: impl ToString,
) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-timeline.nested", operation).with_field(field, value.to_string()),
    )
}

fn conflict(
    operation: &'static str,
    message: &'static str,
    field: &'static str,
    value: impl ToString,
) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-timeline.nested", operation).with_field(field, value.to_string()),
    )
}
