//! Authoritative timeline selection, targeting, synchronization, and clip relationships.

use std::collections::{BTreeMap, BTreeSet};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ClipId, TrackId};

use crate::model::{EditorialObjectId, Track, TrackKind};

/// Default editor lane height used by new and migrated track state.
pub const DEFAULT_TRACK_HEIGHT: u16 = 72;
/// Smallest usable editor lane height.
pub const MIN_TRACK_HEIGHT: u16 = 48;
/// Largest bounded editor lane height.
pub const MAX_TRACK_HEIGHT: u16 = 320;

/// How one selection command changes the current selected object set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SelectionUpdate {
    /// Replace the complete current selection.
    Replace,
    /// Add the requested objects to the current selection.
    Add,
    /// Remove the requested objects from the current selection.
    Remove,
}

/// Whether selection follows authored clip relationships or addresses exact objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SelectionExpansion {
    /// Follow clip groups and, when enabled, linked selection to a fixed point.
    Related,
    /// Address only the exact requested objects, including one member of a group or link.
    Direct,
}

/// Command intent attached to one stable editorial track.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrackEditState {
    track_id: TrackId,
    height: u16,
    targeted: bool,
    locked: bool,
    sync_locked: bool,
    muted: bool,
    solo: bool,
    enabled: bool,
}

impl TrackEditState {
    fn new(track_id: TrackId) -> Self {
        Self {
            track_id,
            height: DEFAULT_TRACK_HEIGHT,
            targeted: false,
            locked: false,
            sync_locked: true,
            muted: false,
            solo: false,
            enabled: true,
        }
    }

    /// Returns the stable track identity.
    #[must_use]
    pub const fn track_id(self) -> TrackId {
        self.track_id
    }

    /// Returns the bounded editor lane height in logical pixels.
    #[must_use]
    pub const fn height(self) -> u16 {
        self.height
    }

    /// Returns whether commands currently target this track.
    #[must_use]
    pub const fn targeted(self) -> bool {
        self.targeted
    }

    /// Returns whether authored item changes are prevented on this track.
    #[must_use]
    pub const fn locked(self) -> bool {
        self.locked
    }

    /// Returns whether ripple-style changes on other tracks keep this track synchronized.
    #[must_use]
    pub const fn sync_locked(self) -> bool {
        self.sync_locked
    }

    /// Returns whether this audio track is suppressed from output.
    #[must_use]
    pub const fn muted(self) -> bool {
        self.muted
    }

    /// Returns whether this audio track is isolated from nonsolo audio tracks.
    #[must_use]
    pub const fn solo(self) -> bool {
        self.solo
    }

    /// Returns whether this track contributes to timeline output.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }
}

/// One canonical relationship component addressed through any stable member clip identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipRelation {
    members: BTreeSet<ClipId>,
}

impl ClipRelation {
    fn new(members: BTreeSet<ClipId>) -> Self {
        Self { members }
    }

    /// Iterates stable clip identities in deterministic order.
    pub fn members(&self) -> impl ExactSizeIterator<Item = ClipId> + '_ {
        self.members.iter().copied()
    }

    /// Returns whether this relationship contains the named clip.
    #[must_use]
    pub fn contains(&self, clip_id: ClipId) -> bool {
        self.members.contains(&clip_id)
    }
}

/// Timeline-local command state published atomically with the editorial project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineEditState {
    selected_objects: BTreeSet<EditorialObjectId>,
    track_states: BTreeMap<TrackId, TrackEditState>,
    linked_selection_enabled: bool,
    links: Vec<ClipRelation>,
    groups: Vec<ClipRelation>,
}

impl TimelineEditState {
    pub(crate) fn from_tracks(tracks: &[Track]) -> Self {
        Self {
            selected_objects: BTreeSet::new(),
            track_states: tracks
                .iter()
                .map(|track| (track.id(), TrackEditState::new(track.id())))
                .collect(),
            linked_selection_enabled: true,
            links: Vec::new(),
            groups: Vec::new(),
        }
    }

    /// Iterates the exact selected object identities in deterministic order.
    pub fn selected_objects(&self) -> impl ExactSizeIterator<Item = EditorialObjectId> + '_ {
        self.selected_objects.iter().copied()
    }

    /// Returns whether ordinary clip selection follows authored clip links.
    #[must_use]
    pub const fn linked_selection_enabled(&self) -> bool {
        self.linked_selection_enabled
    }

    /// Looks up one track's command intent.
    #[must_use]
    pub fn track_state(&self, track_id: TrackId) -> Option<TrackEditState> {
        self.track_states.get(&track_id).copied()
    }

    /// Iterates linked clip components in canonical member order.
    pub fn links(&self) -> impl ExactSizeIterator<Item = &ClipRelation> {
        self.links.iter()
    }

    /// Iterates clip groups in canonical member order.
    pub fn groups(&self) -> impl ExactSizeIterator<Item = &ClipRelation> {
        self.groups.iter()
    }

    /// Finds the linked component containing one clip.
    #[must_use]
    pub fn link_for(&self, clip_id: ClipId) -> Option<&ClipRelation> {
        relation_for(&self.links, clip_id)
    }

    /// Finds the group containing one clip.
    #[must_use]
    pub fn group_for(&self, clip_id: ClipId) -> Option<&ClipRelation> {
        relation_for(&self.groups, clip_id)
    }

    pub(crate) fn set_linked_selection_enabled(&mut self, enabled: bool) {
        self.linked_selection_enabled = enabled;
    }

    pub(crate) fn set_track_targeted(&mut self, track_id: TrackId, targeted: bool) -> Result<()> {
        let state = self.track_states.get_mut(&track_id).ok_or_else(|| {
            not_found(
                "set_track_targeted",
                "editorial track was not found",
                "track",
                track_id,
            )
        })?;
        state.targeted = targeted;
        Ok(())
    }

    pub(crate) fn set_track_height(&mut self, track_id: TrackId, height: u16) -> Result<()> {
        if !(MIN_TRACK_HEIGHT..=MAX_TRACK_HEIGHT).contains(&height) {
            return Err(invalid(
                "set_track_height",
                format!(
                    "track height must be between {MIN_TRACK_HEIGHT} and {MAX_TRACK_HEIGHT} logical pixels"
                ),
            ));
        }
        let state = self.track_state_mut("set_track_height", track_id)?;
        state.height = height;
        Ok(())
    }

    pub(crate) fn set_track_locked(&mut self, track_id: TrackId, locked: bool) -> Result<()> {
        let state = self.track_state_mut("set_track_locked", track_id)?;
        state.locked = locked;
        Ok(())
    }

    pub(crate) fn set_track_sync_locked(
        &mut self,
        track_id: TrackId,
        sync_locked: bool,
    ) -> Result<()> {
        let state = self.track_states.get_mut(&track_id).ok_or_else(|| {
            not_found(
                "set_track_sync_locked",
                "editorial track was not found",
                "track",
                track_id,
            )
        })?;
        state.sync_locked = sync_locked;
        Ok(())
    }

    pub(crate) fn set_track_muted(&mut self, track_id: TrackId, muted: bool) -> Result<()> {
        let state = self.track_state_mut("set_track_muted", track_id)?;
        state.muted = muted;
        Ok(())
    }

    pub(crate) fn set_track_solo(&mut self, track_id: TrackId, solo: bool) -> Result<()> {
        let state = self.track_state_mut("set_track_solo", track_id)?;
        state.solo = solo;
        Ok(())
    }

    pub(crate) fn set_track_enabled(&mut self, track_id: TrackId, enabled: bool) -> Result<()> {
        let state = self.track_state_mut("set_track_enabled", track_id)?;
        state.enabled = enabled;
        Ok(())
    }

    fn track_state_mut(
        &mut self,
        operation: &'static str,
        track_id: TrackId,
    ) -> Result<&mut TrackEditState> {
        self.track_states.get_mut(&track_id).ok_or_else(|| {
            not_found(
                operation,
                "editorial track was not found",
                "track",
                track_id,
            )
        })
    }

    pub(crate) fn update_selection<I>(
        &mut self,
        objects: I,
        update: SelectionUpdate,
        expansion: SelectionExpansion,
        tracks: &[Track],
    ) -> Result<()>
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        let (_, object_ids, _) = timeline_ids(tracks);
        let requested: BTreeSet<_> = objects.into_iter().collect();
        for object_id in &requested {
            if !object_ids.contains(object_id) {
                return Err(not_found(
                    "update_selection",
                    "editorial object was not found",
                    "object",
                    object_id,
                ));
            }
        }

        let expanded = match expansion {
            SelectionExpansion::Direct => requested,
            SelectionExpansion::Related => self.expand_selection(requested),
        };
        match update {
            SelectionUpdate::Replace => self.selected_objects = expanded,
            SelectionUpdate::Add => self.selected_objects.extend(expanded),
            SelectionUpdate::Remove => {
                self.selected_objects
                    .retain(|object_id| !expanded.contains(object_id));
            }
        }
        Ok(())
    }

    pub(crate) fn link_clips<I>(&mut self, clips: I, tracks: &[Track]) -> Result<()>
    where
        I: IntoIterator<Item = ClipId>,
    {
        let members = checked_clips("link_clips", clips, tracks)?;
        require_relationship_size("link_clips", &members)?;
        let anchor = *members.first().expect("relationship size checked");
        merge_relation(&mut self.links, members);
        let linked_members = self
            .link_for(anchor)
            .expect("merged relationship contains its anchor")
            .members
            .clone();
        if self
            .groups
            .iter()
            .any(|group| !group.members.is_disjoint(&linked_members))
        {
            merge_relation(&mut self.groups, linked_members);
        }
        Ok(())
    }

    pub(crate) fn unlink_clips<I>(&mut self, clips: I, tracks: &[Track]) -> Result<()>
    where
        I: IntoIterator<Item = ClipId>,
    {
        let clips = checked_clips("unlink_clips", clips, tracks)?;
        remove_relation_members(&mut self.links, &clips);
        Ok(())
    }

    pub(crate) fn group_clips<I>(&mut self, clips: I, tracks: &[Track]) -> Result<()>
    where
        I: IntoIterator<Item = ClipId>,
    {
        let mut members = checked_clips("group_clips", clips, tracks)?;
        let linked_members: Vec<_> = members
            .iter()
            .filter_map(|clip_id| self.link_for(*clip_id))
            .flat_map(ClipRelation::members)
            .collect();
        members.extend(linked_members);
        require_relationship_size("group_clips", &members)?;
        merge_relation(&mut self.groups, members);
        Ok(())
    }

    pub(crate) fn ungroup_clips<I>(&mut self, clips: I, tracks: &[Track]) -> Result<()>
    where
        I: IntoIterator<Item = ClipId>,
    {
        let clips = checked_clips("ungroup_clips", clips, tracks)?;
        self.groups
            .retain(|relation| relation.members.is_disjoint(&clips));
        Ok(())
    }

    pub(crate) fn reconcile(&mut self, tracks: &[Track]) {
        let (track_ids, object_ids, clip_ids) = timeline_ids(tracks);
        self.track_states
            .retain(|track_id, _| track_ids.contains(track_id));
        for track_id in track_ids {
            self.track_states
                .entry(track_id)
                .or_insert_with(|| TrackEditState::new(track_id));
        }
        self.selected_objects
            .retain(|object_id| object_ids.contains(object_id));
        reconcile_relations(&mut self.links, &clip_ids);
        reconcile_relations(&mut self.groups, &clip_ids);
    }

    pub(crate) fn validate(&self, tracks: &[Track]) -> Result<()> {
        let (track_ids, object_ids, clip_ids) = timeline_ids(tracks);
        if self.track_states.len() != track_ids.len()
            || self
                .track_states
                .keys()
                .any(|track_id| !track_ids.contains(track_id))
        {
            return Err(invalid(
                "validate_edit_state",
                "track edit state must cover every timeline track exactly once",
            ));
        }
        for track in tracks {
            let state = self
                .track_states
                .get(&track.id())
                .expect("track-state coverage checked");
            if !(MIN_TRACK_HEIGHT..=MAX_TRACK_HEIGHT).contains(&state.height) {
                return Err(invalid(
                    "validate_edit_state",
                    "track height is outside the supported editor lane bounds",
                ));
            }
            if track.kind() != TrackKind::Audio && (state.muted || state.solo) {
                return Err(invalid(
                    "validate_edit_state",
                    "mute and solo state is supported only for audio tracks",
                ));
            }
        }
        if self
            .selected_objects
            .iter()
            .any(|object_id| !object_ids.contains(object_id))
        {
            return Err(not_found(
                "validate_edit_state",
                "selection references a missing editorial object",
                "timeline",
                "selection",
            ));
        }
        validate_relations("validate_links", &self.links, &clip_ids)?;
        validate_relations("validate_groups", &self.groups, &clip_ids)?;
        Ok(())
    }

    fn expand_selection(
        &self,
        requested: BTreeSet<EditorialObjectId>,
    ) -> BTreeSet<EditorialObjectId> {
        let mut expanded = requested;
        let mut pending: Vec<_> = expanded
            .iter()
            .filter_map(|object_id| match object_id {
                EditorialObjectId::Clip(clip_id) => Some(*clip_id),
                _ => None,
            })
            .collect();
        let mut visited = BTreeSet::new();
        while let Some(clip_id) = pending.pop() {
            if !visited.insert(clip_id) {
                continue;
            }
            if let Some(group) = self.group_for(clip_id) {
                pending.extend(group.members().filter(|member| !visited.contains(member)));
            }
            if self.linked_selection_enabled {
                if let Some(link) = self.link_for(clip_id) {
                    pending.extend(link.members().filter(|member| !visited.contains(member)));
                }
            }
        }
        expanded.extend(visited.into_iter().map(EditorialObjectId::Clip));
        expanded
    }
}

fn timeline_ids(
    tracks: &[Track],
) -> (
    BTreeSet<TrackId>,
    BTreeSet<EditorialObjectId>,
    BTreeSet<ClipId>,
) {
    let track_ids = tracks.iter().map(Track::id).collect();
    let object_ids: BTreeSet<_> = tracks
        .iter()
        .flat_map(|track| track.items().iter().map(|item| item.id()))
        .collect();
    let clip_ids = object_ids
        .iter()
        .filter_map(|object_id| match object_id {
            EditorialObjectId::Clip(clip_id) => Some(*clip_id),
            _ => None,
        })
        .collect();
    (track_ids, object_ids, clip_ids)
}

fn checked_clips<I>(operation: &'static str, clips: I, tracks: &[Track]) -> Result<BTreeSet<ClipId>>
where
    I: IntoIterator<Item = ClipId>,
{
    let (_, _, available) = timeline_ids(tracks);
    let clips: BTreeSet<_> = clips.into_iter().collect();
    for clip_id in &clips {
        if !available.contains(clip_id) {
            return Err(not_found(
                operation,
                "editorial clip was not found",
                "clip",
                clip_id,
            ));
        }
    }
    Ok(clips)
}

fn require_relationship_size(operation: &'static str, members: &BTreeSet<ClipId>) -> Result<()> {
    if members.len() < 2 {
        return Err(invalid(
            operation,
            "a clip relationship requires at least two distinct clips",
        ));
    }
    Ok(())
}

fn relation_for(relations: &[ClipRelation], clip_id: ClipId) -> Option<&ClipRelation> {
    relations.iter().find(|relation| relation.contains(clip_id))
}

fn merge_relation(relations: &mut Vec<ClipRelation>, mut members: BTreeSet<ClipId>) {
    let mut retained = Vec::with_capacity(relations.len() + 1);
    for relation in relations.drain(..) {
        if relation.members.is_disjoint(&members) {
            retained.push(relation);
        } else {
            members.extend(relation.members);
        }
    }
    retained.push(ClipRelation::new(members));
    sort_relations(&mut retained);
    *relations = retained;
}

fn remove_relation_members(relations: &mut Vec<ClipRelation>, clips: &BTreeSet<ClipId>) {
    let mut retained = Vec::with_capacity(relations.len());
    for mut relation in relations.drain(..) {
        relation.members.retain(|clip_id| !clips.contains(clip_id));
        if relation.members.len() >= 2 {
            retained.push(relation);
        }
    }
    sort_relations(&mut retained);
    *relations = retained;
}

fn reconcile_relations(relations: &mut Vec<ClipRelation>, clip_ids: &BTreeSet<ClipId>) {
    let mut retained = Vec::with_capacity(relations.len());
    for mut relation in relations.drain(..) {
        relation
            .members
            .retain(|clip_id| clip_ids.contains(clip_id));
        if relation.members.len() >= 2 {
            retained.push(relation);
        }
    }
    sort_relations(&mut retained);
    *relations = retained;
}

fn sort_relations(relations: &mut [ClipRelation]) {
    relations.sort_by(|left, right| left.members.iter().cmp(right.members.iter()));
}

fn validate_relations(
    operation: &'static str,
    relations: &[ClipRelation],
    clip_ids: &BTreeSet<ClipId>,
) -> Result<()> {
    let mut claimed = BTreeSet::new();
    for relation in relations {
        require_relationship_size(operation, &relation.members)?;
        for clip_id in &relation.members {
            if !clip_ids.contains(clip_id) {
                return Err(not_found(
                    operation,
                    "clip relationship references a missing clip",
                    "clip",
                    clip_id,
                ));
            }
            if !claimed.insert(*clip_id) {
                return Err(invalid(
                    operation,
                    "one clip cannot belong to multiple components of the same relationship",
                ));
            }
        }
    }
    Ok(())
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.edit-state", operation))
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
        ErrorContext::new("superi-timeline.edit-state", operation)
            .with_field(field, value.to_string()),
    )
}
