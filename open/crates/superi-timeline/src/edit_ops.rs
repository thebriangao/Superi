//! Atomic foundational editorial operations over validated project drafts.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::ids::{TimelineId, TrackId, TransitionId};
use superi_core::time::{Duration, RationalTime, TimeRange, TimeRounding, Timebase};

use crate::model::{
    Caption, Clip, EditorialObjectId, EditorialProject, Gap, Generator, ProjectDraft, Track,
    TrackItem, Transition,
};

/// Stable operation category reported to direct edit consumers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditKind {
    /// Insert material and ripple later track content right.
    Insert,
    /// Replace material for the source duration without changing track duration.
    Overwrite,
    /// Add material at the current track end.
    Append,
    /// Exchange one exact timed object without changing its placement or duration.
    Replace,
    /// Replace a record range with an explicit gap.
    Lift,
    /// Remove a record range and ripple later content left.
    Extract,
}

impl EditKind {
    const fn code(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Overwrite => "overwrite",
            Self::Append => "append",
            Self::Replace => "replace",
            Self::Lift => "lift",
            Self::Extract => "extract",
        }
    }
}

/// One directly executable edit command.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditOperation {
    /// Insert material at an exact target-track time.
    Insert {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track receiving the material.
        track_id: TrackId,
        /// Exact insertion point in the target track clock.
        at: RationalTime,
        /// One non-transition item whose record start is normalized to `at`.
        material: TrackItem,
        /// Typed identities for right fragments, consumed in record order.
        fragment_ids: Vec<EditorialObjectId>,
    },
    /// Overwrite existing material for the incoming material duration.
    Overwrite {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track receiving the material.
        track_id: TrackId,
        /// Exact overwrite start in the target track clock.
        at: RationalTime,
        /// One non-transition item whose record start is normalized to `at`.
        material: TrackItem,
        /// Typed identities for right fragments, consumed in record order.
        fragment_ids: Vec<EditorialObjectId>,
    },
    /// Append material at the current target-track end.
    Append {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track receiving the material.
        track_id: TrackId,
        /// One non-transition item whose record start is normalized to the track end.
        material: TrackItem,
    },
    /// Replace one exact timed object with equal-duration material.
    Replace {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the target object.
        track_id: TrackId,
        /// Stable identity of the timed object to replace.
        target_id: EditorialObjectId,
        /// One non-transition item normalized to the target placement.
        material: TrackItem,
    },
    /// Lift a record range into a caller-identified explicit gap.
    Lift {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the lifted range.
        track_id: TrackId,
        /// Exact nonempty record range in the target track clock.
        range: TimeRange,
        /// Caller-owned gap identity and editor-facing name.
        gap: Gap,
        /// Typed identities for right fragments, consumed in record order.
        fragment_ids: Vec<EditorialObjectId>,
    },
    /// Extract a record range and close it by rippling later content left.
    Extract {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the extracted range.
        track_id: TrackId,
        /// Exact nonempty record range in the target track clock.
        range: TimeRange,
        /// Typed identities for right fragments, consumed in record order.
        fragment_ids: Vec<EditorialObjectId>,
    },
}

impl EditOperation {
    /// Creates an insert operation.
    pub fn insert<I>(
        timeline_id: TimelineId,
        track_id: TrackId,
        at: RationalTime,
        material: TrackItem,
        fragment_ids: I,
    ) -> Self
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        Self::Insert {
            timeline_id,
            track_id,
            at,
            material,
            fragment_ids: fragment_ids.into_iter().collect(),
        }
    }

    /// Creates an overwrite operation.
    pub fn overwrite<I>(
        timeline_id: TimelineId,
        track_id: TrackId,
        at: RationalTime,
        material: TrackItem,
        fragment_ids: I,
    ) -> Self
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        Self::Overwrite {
            timeline_id,
            track_id,
            at,
            material,
            fragment_ids: fragment_ids.into_iter().collect(),
        }
    }

    /// Creates an append operation.
    #[must_use]
    pub fn append(timeline_id: TimelineId, track_id: TrackId, material: TrackItem) -> Self {
        Self::Append {
            timeline_id,
            track_id,
            material,
        }
    }

    /// Creates a replace operation.
    #[must_use]
    pub fn replace(
        timeline_id: TimelineId,
        track_id: TrackId,
        target_id: EditorialObjectId,
        material: TrackItem,
    ) -> Self {
        Self::Replace {
            timeline_id,
            track_id,
            target_id,
            material,
        }
    }

    /// Creates a lift operation.
    pub fn lift<I>(
        timeline_id: TimelineId,
        track_id: TrackId,
        range: TimeRange,
        gap: Gap,
        fragment_ids: I,
    ) -> Self
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        Self::Lift {
            timeline_id,
            track_id,
            range,
            gap,
            fragment_ids: fragment_ids.into_iter().collect(),
        }
    }

    /// Creates an extract operation.
    pub fn extract<I>(
        timeline_id: TimelineId,
        track_id: TrackId,
        range: TimeRange,
        fragment_ids: I,
    ) -> Self
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        Self::Extract {
            timeline_id,
            track_id,
            range,
            fragment_ids: fragment_ids.into_iter().collect(),
        }
    }

    /// Returns the stable operation category.
    #[must_use]
    pub const fn kind(&self) -> EditKind {
        match self {
            Self::Insert { .. } => EditKind::Insert,
            Self::Overwrite { .. } => EditKind::Overwrite,
            Self::Append { .. } => EditKind::Append,
            Self::Replace { .. } => EditKind::Replace,
            Self::Lift { .. } => EditKind::Lift,
            Self::Extract { .. } => EditKind::Extract,
        }
    }

    const fn timeline_id(&self) -> TimelineId {
        match self {
            Self::Insert { timeline_id, .. }
            | Self::Overwrite { timeline_id, .. }
            | Self::Append { timeline_id, .. }
            | Self::Replace { timeline_id, .. }
            | Self::Lift { timeline_id, .. }
            | Self::Extract { timeline_id, .. } => *timeline_id,
        }
    }

    const fn track_id(&self) -> TrackId {
        match self {
            Self::Insert { track_id, .. }
            | Self::Overwrite { track_id, .. }
            | Self::Append { track_id, .. }
            | Self::Replace { track_id, .. }
            | Self::Lift { track_id, .. }
            | Self::Extract { track_id, .. } => *track_id,
        }
    }
}

/// Exact track-duration effect of one operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TrackDurationChange {
    /// The operation retained the current track duration.
    Unchanged,
    /// The operation extended the track by this exact duration.
    Extended(Duration),
    /// The operation shortened the track by this exact duration.
    Shortened(Duration),
}

/// One explicit identity created by splitting a timed object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditFragment {
    original: EditorialObjectId,
    created: EditorialObjectId,
}

impl EditFragment {
    /// Returns the object whose record interval was divided.
    #[must_use]
    pub const fn original(&self) -> EditorialObjectId {
        self.original
    }

    /// Returns the caller-supplied identity of the right fragment.
    #[must_use]
    pub const fn created(&self) -> EditorialObjectId {
        self.created
    }
}

/// Directly inspectable result of one operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditOutcome {
    kind: EditKind,
    timeline_id: TimelineId,
    track_id: TrackId,
    affected_range: TimeRange,
    duration_change: TrackDurationChange,
    inserted_ids: Vec<EditorialObjectId>,
    removed_ids: Vec<EditorialObjectId>,
    modified_ids: Vec<EditorialObjectId>,
    fragments: Vec<EditFragment>,
    removed_transitions: Vec<TransitionId>,
}

impl EditOutcome {
    /// Returns the operation category.
    #[must_use]
    pub const fn kind(&self) -> EditKind {
        self.kind
    }

    /// Returns the affected timeline.
    #[must_use]
    pub const fn timeline_id(&self) -> TimelineId {
        self.timeline_id
    }

    /// Returns the affected track.
    #[must_use]
    pub const fn track_id(&self) -> TrackId {
        self.track_id
    }

    /// Returns the exact record interval addressed by the operation.
    #[must_use]
    pub const fn affected_range(&self) -> TimeRange {
        self.affected_range
    }

    /// Returns the exact track-duration effect.
    #[must_use]
    pub const fn duration_change(&self) -> TrackDurationChange {
        self.duration_change
    }

    /// Returns material identities introduced by the operation.
    #[must_use]
    pub fn inserted_ids(&self) -> &[EditorialObjectId] {
        &self.inserted_ids
    }

    /// Returns whole object identities removed by the operation.
    #[must_use]
    pub fn removed_ids(&self) -> &[EditorialObjectId] {
        &self.removed_ids
    }

    /// Returns retained object identities whose source or record mapping changed.
    #[must_use]
    pub fn modified_ids(&self) -> &[EditorialObjectId] {
        &self.modified_ids
    }

    /// Returns every caller-identified right fragment in record order.
    #[must_use]
    pub fn fragments(&self) -> &[EditFragment] {
        &self.fragments
    }

    /// Returns transitions invalidated by changed adjacency or available handles.
    #[must_use]
    pub fn removed_transitions(&self) -> &[TransitionId] {
        &self.removed_transitions
    }
}

/// Results published together at one project revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditBatchResult {
    revision: u64,
    outcomes: Vec<EditOutcome>,
}

impl EditBatchResult {
    /// Returns the newly published project revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns operation results in command order.
    #[must_use]
    pub fn outcomes(&self) -> &[EditOutcome] {
        &self.outcomes
    }
}

/// Applies related timeline operations as one validated project edit.
///
/// The existing project draft boundary owns publication. Any operation error or
/// complete-project validation error leaves the project and its revision unchanged.
pub fn apply_edit_batch(
    project: &mut EditorialProject,
    expected_revision: u64,
    operations: &[EditOperation],
) -> Result<EditBatchResult> {
    if operations.is_empty() {
        return Err(invalid_edit(
            "apply_edit_batch",
            "an edit batch must contain at least one operation",
        ));
    }

    let mut outcomes = Vec::with_capacity(operations.len());
    project.edit(expected_revision, |draft| {
        for (index, operation) in operations.iter().enumerate() {
            let outcome = apply_operation(draft, operation).with_error_context(
                ErrorContext::new("superi-timeline.edit_ops", "apply_operation")
                    .with_field("index", index.to_string())
                    .with_field("kind", operation.kind().code()),
            )?;
            outcomes.push(outcome);
        }
        Ok(())
    })?;

    Ok(EditBatchResult {
        revision: project.revision(),
        outcomes,
    })
}

fn apply_operation(draft: &mut ProjectDraft, operation: &EditOperation) -> Result<EditOutcome> {
    let timeline_id = operation.timeline_id();
    let track_id = operation.track_id();
    let track = draft.timeline_mut(timeline_id)?.track_mut(track_id)?;

    match operation {
        EditOperation::Insert {
            at,
            material,
            fragment_ids,
            ..
        } => apply_insert(track, timeline_id, track_id, *at, material, fragment_ids),
        EditOperation::Overwrite {
            at,
            material,
            fragment_ids,
            ..
        } => apply_overwrite(track, timeline_id, track_id, *at, material, fragment_ids),
        EditOperation::Append { material, .. } => {
            apply_append(track, timeline_id, track_id, material)
        }
        EditOperation::Replace {
            target_id,
            material,
            ..
        } => apply_replace(track, timeline_id, track_id, *target_id, material),
        EditOperation::Lift {
            range,
            gap,
            fragment_ids,
            ..
        } => apply_lift(track, timeline_id, track_id, *range, gap, fragment_ids),
        EditOperation::Extract {
            range,
            fragment_ids,
            ..
        } => apply_extract(track, timeline_id, track_id, *range, fragment_ids),
    }
}

fn apply_insert(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    at: RationalTime,
    material: &TrackItem,
    fragment_ids: &[EditorialObjectId],
) -> Result<EditOutcome> {
    let timebase = track.semantics().timebase();
    validate_point_clock(at, timebase, "insert")?;
    let end = track_end(track)?;
    if at.is_negative() || at > end {
        return Err(invalid_edit(
            "insert",
            "insert point must lie between track start and track end",
        ));
    }
    let material = normalize_material(material, at, timebase, "insert")?;
    let material_duration = material
        .record_range()
        .expect("material is timed")
        .duration();
    let affected_range = TimeRange::new(at, material_duration)?;
    let (timed, transitions) = separate_items(track.items());
    let mut fragments = FragmentCursor::new(fragment_ids);
    let mut output = Vec::with_capacity(timed.len() + 2);
    let mut modified_ids = Vec::new();
    let mut created_fragments = Vec::new();

    for item in timed {
        let item_range = item.record_range().expect("separated item is timed");
        let item_end = item_range.end_exclusive()?;
        if item_range.start() < at && at < item_end {
            let left = slice_item(&item, item_range.start(), at, item.id(), "insert")?;
            let right_id = fragments.next_for(item.id(), "insert")?;
            let right = slice_item(&item, at, item_end, right_id, "insert")?;
            let right = shift_item_right(&right, material_duration, timebase, "insert")?;
            push_unique(&mut modified_ids, item.id());
            push_unique(&mut modified_ids, right_id);
            created_fragments.push(EditFragment {
                original: item.id(),
                created: right_id,
            });
            output.push(left);
            output.push(right);
        } else if item_range.start() >= at {
            let shifted = shift_item_right(&item, material_duration, timebase, "insert")?;
            push_unique(&mut modified_ids, item.id());
            output.push(shifted);
        } else {
            output.push(item);
        }
    }
    fragments.finish("insert")?;
    let inserted_id = material.id();
    output.push(material);
    sort_timed(&mut output);
    let (items, removed_transitions) = rebuild_with_transitions(output, transitions)?;
    track.replace_items(items);

    Ok(EditOutcome {
        kind: EditKind::Insert,
        timeline_id,
        track_id,
        affected_range,
        duration_change: TrackDurationChange::Extended(material_duration),
        inserted_ids: vec![inserted_id],
        removed_ids: Vec::new(),
        modified_ids,
        fragments: created_fragments,
        removed_transitions,
    })
}

fn apply_overwrite(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    at: RationalTime,
    material: &TrackItem,
    fragment_ids: &[EditorialObjectId],
) -> Result<EditOutcome> {
    let timebase = track.semantics().timebase();
    validate_point_clock(at, timebase, "overwrite")?;
    let material = normalize_material(material, at, timebase, "overwrite")?;
    let duration = material
        .record_range()
        .expect("material is timed")
        .duration();
    let range = TimeRange::new(at, duration)?;
    validate_edit_range(track, range, "overwrite")?;
    apply_range_replacement(
        track,
        timeline_id,
        track_id,
        EditKind::Overwrite,
        range,
        Some(material),
        fragment_ids,
        false,
        TrackDurationChange::Unchanged,
    )
}

fn apply_append(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    material: &TrackItem,
) -> Result<EditOutcome> {
    let timebase = track.semantics().timebase();
    let at = track_end(track)?;
    let material = normalize_material(material, at, timebase, "append")?;
    let duration = material
        .record_range()
        .expect("material is timed")
        .duration();
    let affected_range = TimeRange::new(at, duration)?;
    let inserted_id = material.id();
    let (mut timed, transitions) = separate_items(track.items());
    timed.push(material);
    sort_timed(&mut timed);
    let (items, removed_transitions) = rebuild_with_transitions(timed, transitions)?;
    track.replace_items(items);

    Ok(EditOutcome {
        kind: EditKind::Append,
        timeline_id,
        track_id,
        affected_range,
        duration_change: TrackDurationChange::Extended(duration),
        inserted_ids: vec![inserted_id],
        removed_ids: Vec::new(),
        modified_ids: Vec::new(),
        fragments: Vec::new(),
        removed_transitions,
    })
}

fn apply_replace(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    target_id: EditorialObjectId,
    material: &TrackItem,
) -> Result<EditOutcome> {
    if matches!(target_id, EditorialObjectId::Transition(_)) {
        return Err(invalid_edit(
            "replace",
            "replace targets must be timed editorial objects",
        ));
    }
    let timebase = track.semantics().timebase();
    let (mut timed, transitions) = separate_items(track.items());
    let target_index = timed
        .iter()
        .position(|item| item.id() == target_id)
        .ok_or_else(|| {
            not_found_edit(
                "replace",
                "replace target was not found on the target track",
            )
        })?;
    let target_range = timed[target_index]
        .record_range()
        .expect("separated item is timed");
    let material = normalize_material(material, target_range.start(), timebase, "replace")?;
    if material
        .record_range()
        .expect("material is timed")
        .duration()
        != target_range.duration()
    {
        return Err(invalid_edit(
            "replace",
            "replacement material must exactly match the target record duration",
        ));
    }
    let inserted_id = material.id();
    timed[target_index] = material;
    let (items, removed_transitions) = rebuild_with_transitions(timed, transitions)?;
    track.replace_items(items);
    let same_identity = target_id == inserted_id;

    Ok(EditOutcome {
        kind: EditKind::Replace,
        timeline_id,
        track_id,
        affected_range: target_range,
        duration_change: TrackDurationChange::Unchanged,
        inserted_ids: if same_identity {
            Vec::new()
        } else {
            vec![inserted_id]
        },
        removed_ids: if same_identity {
            Vec::new()
        } else {
            vec![target_id]
        },
        modified_ids: if same_identity {
            vec![target_id]
        } else {
            Vec::new()
        },
        fragments: Vec::new(),
        removed_transitions,
    })
}

fn apply_lift(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    range: TimeRange,
    gap: &Gap,
    fragment_ids: &[EditorialObjectId],
) -> Result<EditOutcome> {
    validate_edit_range(track, range, "lift")?;
    if gap.record_range().duration() != range.duration() {
        return Err(invalid_edit(
            "lift",
            "lift gap duration must exactly match the lifted record range",
        ));
    }
    let replacement = TrackItem::Gap(Gap::new(gap.id(), gap.name(), range));
    apply_range_replacement(
        track,
        timeline_id,
        track_id,
        EditKind::Lift,
        range,
        Some(replacement),
        fragment_ids,
        false,
        TrackDurationChange::Unchanged,
    )
}

fn apply_extract(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    range: TimeRange,
    fragment_ids: &[EditorialObjectId],
) -> Result<EditOutcome> {
    validate_edit_range(track, range, "extract")?;
    apply_range_replacement(
        track,
        timeline_id,
        track_id,
        EditKind::Extract,
        range,
        None,
        fragment_ids,
        true,
        TrackDurationChange::Shortened(range.duration()),
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_range_replacement(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    kind: EditKind,
    range: TimeRange,
    replacement: Option<TrackItem>,
    fragment_ids: &[EditorialObjectId],
    close_range: bool,
    duration_change: TrackDurationChange,
) -> Result<EditOutcome> {
    let operation = kind.code();
    let timebase = track.semantics().timebase();
    let range_end = range.end_exclusive()?;
    let (timed, transitions) = separate_items(track.items());
    let mut fragments = FragmentCursor::new(fragment_ids);
    let mut output = Vec::with_capacity(timed.len() + 2);
    let mut removed_ids = Vec::new();
    let mut modified_ids = Vec::new();
    let mut created_fragments = Vec::new();

    for item in timed {
        let item_range = item.record_range().expect("separated item is timed");
        let item_end = item_range.end_exclusive()?;
        if item_end <= range.start() {
            output.push(item);
            continue;
        }
        if item_range.start() >= range_end {
            if close_range {
                let shifted = shift_item_left(&item, range.duration(), timebase, operation)?;
                push_unique(&mut modified_ids, item.id());
                output.push(shifted);
            } else {
                output.push(item);
            }
            continue;
        }

        let keep_left = item_range.start() < range.start();
        let keep_right = item_end > range_end;
        if keep_left {
            output.push(slice_item(
                &item,
                item_range.start(),
                range.start(),
                item.id(),
                operation,
            )?);
            push_unique(&mut modified_ids, item.id());
        }
        if keep_right {
            let right_id = if keep_left {
                let created = fragments.next_for(item.id(), operation)?;
                created_fragments.push(EditFragment {
                    original: item.id(),
                    created,
                });
                created
            } else {
                item.id()
            };
            let mut right = slice_item(&item, range_end, item_end, right_id, operation)?;
            if close_range {
                right = shift_item_left(&right, range.duration(), timebase, operation)?;
            }
            push_unique(&mut modified_ids, right_id);
            output.push(right);
        }
        if !keep_left && !keep_right {
            removed_ids.push(item.id());
        }
    }
    fragments.finish(operation)?;

    let mut inserted_ids = Vec::new();
    if let Some(replacement) = replacement {
        inserted_ids.push(replacement.id());
        output.push(replacement);
    }
    sort_timed(&mut output);
    let (items, removed_transitions) = rebuild_with_transitions(output, transitions)?;
    track.replace_items(items);

    Ok(EditOutcome {
        kind,
        timeline_id,
        track_id,
        affected_range: range,
        duration_change,
        inserted_ids,
        removed_ids,
        modified_ids,
        fragments: created_fragments,
        removed_transitions,
    })
}

fn normalize_material(
    material: &TrackItem,
    at: RationalTime,
    timebase: Timebase,
    operation: &'static str,
) -> Result<TrackItem> {
    let range = material.record_range().ok_or_else(|| {
        invalid_edit(
            operation,
            "transitions cannot be used as timed edit material",
        )
    })?;
    let duration = exact_duration_at(range.duration(), timebase, operation)?;
    if duration.is_zero() {
        return Err(invalid_edit(
            operation,
            "edit material must have a nonzero record duration",
        ));
    }
    reposition_item(material, TimeRange::new(at, duration)?, operation)
}

fn validate_point_clock(
    point: RationalTime,
    expected: Timebase,
    operation: &'static str,
) -> Result<()> {
    if point.timebase() != expected {
        return Err(invalid_edit(
            operation,
            "edit point must use the exact target track timebase",
        ));
    }
    Ok(())
}

fn validate_edit_range(track: &Track, range: TimeRange, operation: &'static str) -> Result<()> {
    let timebase = track.semantics().timebase();
    if range.timebase() != timebase {
        return Err(invalid_edit(
            operation,
            "edit range must use the exact target track timebase",
        ));
    }
    if range.is_empty() {
        return Err(invalid_edit(operation, "edit range must be nonempty"));
    }
    let end = track_end(track)?;
    if range.start().is_negative() || range.end_exclusive()? > end {
        return Err(invalid_edit(
            operation,
            "edit range must lie within the current target track duration",
        ));
    }
    Ok(())
}

fn track_end(track: &Track) -> Result<RationalTime> {
    track
        .items()
        .iter()
        .rev()
        .find_map(TrackItem::record_range)
        .map(TimeRange::end_exclusive)
        .transpose()?
        .map_or_else(|| Ok(RationalTime::zero(track.semantics().timebase())), Ok)
}

fn exact_duration_at(
    duration: Duration,
    timebase: Timebase,
    operation: &'static str,
) -> Result<Duration> {
    duration
        .checked_rescale(timebase, TimeRounding::Exact)
        .with_error_context(ErrorContext::new("superi-timeline.edit_ops", operation))
}

fn separate_items(items: &[TrackItem]) -> (Vec<TrackItem>, Vec<Transition>) {
    let mut timed = Vec::new();
    let mut transitions = Vec::new();
    for item in items {
        match item {
            TrackItem::Transition(transition) => transitions.push(transition.clone()),
            _ => timed.push(item.clone()),
        }
    }
    (timed, transitions)
}

fn sort_timed(items: &mut [TrackItem]) {
    items.sort_by_key(|item| {
        item.record_range()
            .expect("timed edit output")
            .start()
            .value()
    });
}

fn rebuild_with_transitions(
    timed: Vec<TrackItem>,
    transitions: Vec<Transition>,
) -> Result<(Vec<TrackItem>, Vec<TransitionId>)> {
    let mut boundaries: Vec<Option<Transition>> = timed
        .windows(2)
        .map(|window| {
            transitions
                .iter()
                .find(|transition| {
                    transition.from() == window[0].id() && transition.to() == window[1].id()
                })
                .cloned()
        })
        .collect();

    for (index, boundary) in boundaries.iter_mut().enumerate() {
        let Some(transition) = boundary else {
            continue;
        };
        let from_duration = timed[index]
            .record_range()
            .expect("timed edit output")
            .duration();
        let to_duration = timed[index + 1]
            .record_range()
            .expect("timed edit output")
            .duration();
        if transition.from_offset() > from_duration || transition.to_offset() > to_duration {
            *boundary = None;
        }
    }

    for item_index in 0..timed.len() {
        if item_index == 0 || item_index >= boundaries.len() {
            continue;
        }
        let Some(incoming) = boundaries[item_index - 1].as_ref() else {
            continue;
        };
        let Some(outgoing) = boundaries[item_index].as_ref() else {
            continue;
        };
        let item_duration = timed[item_index]
            .record_range()
            .expect("timed edit output")
            .duration();
        let combined = incoming.to_offset().rational_time().checked_add_at(
            outgoing.from_offset().rational_time(),
            item_duration.timebase(),
            TimeRounding::Exact,
        )?;
        if combined > item_duration.rational_time() {
            boundaries[item_index] = None;
        }
    }

    let kept: Vec<TransitionId> = boundaries
        .iter()
        .filter_map(|transition| transition.as_ref().map(Transition::id))
        .collect();
    let removed = transitions
        .iter()
        .filter_map(|transition| (!kept.contains(&transition.id())).then_some(transition.id()))
        .collect();
    let mut items = Vec::with_capacity(timed.len() + kept.len());
    for (index, item) in timed.into_iter().enumerate() {
        if index > 0 {
            if let Some(transition) = boundaries[index - 1].take() {
                items.push(TrackItem::Transition(transition));
            }
        }
        items.push(item);
    }
    Ok((items, removed))
}

fn slice_item(
    item: &TrackItem,
    start: RationalTime,
    end: RationalTime,
    id: EditorialObjectId,
    operation: &'static str,
) -> Result<TrackItem> {
    let old_range = item
        .record_range()
        .ok_or_else(|| invalid_edit(operation, "transitions cannot be split as timed objects"))?;
    if start.timebase() != old_range.timebase()
        || end.timebase() != old_range.timebase()
        || start < old_range.start()
        || end > old_range.end_exclusive()?
        || end < start
    {
        return Err(invalid_edit(
            operation,
            "item slice must lie within its exact record range",
        ));
    }
    let new_range = TimeRange::from_start_end(start, end)?;
    let mut sliced = clone_with_id(item, id, operation)?;
    if let TrackItem::Clip(clip) = &mut sliced {
        let old_source = clip.source_range();
        let record_offset =
            start.checked_sub_at(old_range.start(), old_range.timebase(), TimeRounding::Exact)?;
        let source_offset =
            record_offset.checked_rescale(old_source.timebase(), TimeRounding::Exact)?;
        let source_start = old_source.start().checked_add_at(
            source_offset,
            old_source.timebase(),
            TimeRounding::Exact,
        )?;
        let source_duration =
            exact_duration_at(new_range.duration(), old_source.timebase(), operation)?;
        clip.set_source_range(TimeRange::new(source_start, source_duration)?);
    }
    set_record_range(&mut sliced, new_range, operation)?;
    Ok(sliced)
}

fn reposition_item(
    item: &TrackItem,
    range: TimeRange,
    operation: &'static str,
) -> Result<TrackItem> {
    let mut positioned = item.clone();
    set_record_range(&mut positioned, range, operation)?;
    Ok(positioned)
}

fn shift_item_right(
    item: &TrackItem,
    duration: Duration,
    timebase: Timebase,
    operation: &'static str,
) -> Result<TrackItem> {
    let range = item.record_range().expect("timed edit output");
    let start =
        range
            .start()
            .checked_add_at(duration.rational_time(), timebase, TimeRounding::Exact)?;
    reposition_item(item, TimeRange::new(start, range.duration())?, operation)
}

fn shift_item_left(
    item: &TrackItem,
    duration: Duration,
    timebase: Timebase,
    operation: &'static str,
) -> Result<TrackItem> {
    let range = item.record_range().expect("timed edit output");
    let start =
        range
            .start()
            .checked_sub_at(duration.rational_time(), timebase, TimeRounding::Exact)?;
    reposition_item(item, TimeRange::new(start, range.duration())?, operation)
}

fn set_record_range(item: &mut TrackItem, range: TimeRange, operation: &'static str) -> Result<()> {
    match item {
        TrackItem::Clip(value) => value.set_record_range(range),
        TrackItem::Gap(value) => value.set_record_range(range),
        TrackItem::Generator(value) => value.set_record_range(range),
        TrackItem::Caption(value) => value.set_record_range(range),
        TrackItem::Transition(_) => {
            return Err(invalid_edit(
                operation,
                "transitions do not own a record range",
            ));
        }
    }
    Ok(())
}

fn clone_with_id(
    item: &TrackItem,
    id: EditorialObjectId,
    operation: &'static str,
) -> Result<TrackItem> {
    let value = match (item, id) {
        (TrackItem::Clip(value), EditorialObjectId::Clip(id)) => TrackItem::Clip(Clip::new(
            id,
            value.name(),
            value.source(),
            value.source_range(),
            value.record_range(),
        )),
        (TrackItem::Gap(value), EditorialObjectId::Gap(id)) => {
            TrackItem::Gap(Gap::new(id, value.name(), value.record_range()))
        }
        (TrackItem::Generator(value), EditorialObjectId::Generator(id)) => {
            TrackItem::Generator(Generator::new(
                id,
                value.name(),
                value.kind(),
                value.parameters().clone(),
                value.record_range(),
            ))
        }
        (TrackItem::Caption(value), EditorialObjectId::Caption(id)) => {
            TrackItem::Caption(Caption::new(
                id,
                value.name(),
                value.text(),
                value.language().map(str::to_owned),
                value.record_range(),
            ))
        }
        (TrackItem::Transition(_), EditorialObjectId::Transition(_)) => {
            return Err(invalid_edit(
                operation,
                "transitions cannot be fragmented as timed objects",
            ));
        }
        _ => {
            return Err(invalid_edit(
                operation,
                "fragment identity must have the same typed domain as its original object",
            ));
        }
    };
    Ok(value)
}

struct FragmentCursor<'a> {
    ids: &'a [EditorialObjectId],
    next: usize,
}

impl<'a> FragmentCursor<'a> {
    const fn new(ids: &'a [EditorialObjectId]) -> Self {
        Self { ids, next: 0 }
    }

    fn next_for(
        &mut self,
        original: EditorialObjectId,
        operation: &'static str,
    ) -> Result<EditorialObjectId> {
        let id = self.ids.get(self.next).copied().ok_or_else(|| {
            invalid_edit(
                operation,
                "a caller-supplied right fragment identity is required at this edit boundary",
            )
        })?;
        self.next += 1;
        if !same_typed_domain(original, id) {
            return Err(invalid_edit(
                operation,
                "fragment identity must have the same typed domain as its original object",
            ));
        }
        Ok(id)
    }

    fn finish(self, operation: &'static str) -> Result<()> {
        if self.next != self.ids.len() {
            return Err(invalid_edit(
                operation,
                "fragment identity list contains unused identities",
            ));
        }
        Ok(())
    }
}

const fn same_typed_domain(left: EditorialObjectId, right: EditorialObjectId) -> bool {
    matches!(
        (left, right),
        (EditorialObjectId::Clip(_), EditorialObjectId::Clip(_))
            | (EditorialObjectId::Gap(_), EditorialObjectId::Gap(_))
            | (
                EditorialObjectId::Transition(_),
                EditorialObjectId::Transition(_)
            )
            | (
                EditorialObjectId::Generator(_),
                EditorialObjectId::Generator(_)
            )
            | (EditorialObjectId::Caption(_), EditorialObjectId::Caption(_))
    )
}

fn push_unique(values: &mut Vec<EditorialObjectId>, id: EditorialObjectId) {
    if !values.contains(&id) {
        values.push(id);
    }
}

fn invalid_edit(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.edit_ops", operation))
}

fn not_found_edit(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.edit_ops", operation))
}
