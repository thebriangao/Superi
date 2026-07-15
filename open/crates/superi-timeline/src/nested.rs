//! Atomic nested-sequence and compound-clip operations.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ClipId, TimelineId, TrackId};
use superi_core::time::{RationalTime, TimeRange, TimeRounding};

use crate::edit_ops::{apply_operation, EditOperation, EditOutcome};
use crate::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, ProjectDraft, Timeline, TrackItem,
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
