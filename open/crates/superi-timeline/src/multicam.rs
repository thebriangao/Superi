//! Native synchronized multicam sources, clip-local switching, and exact resolution.

use std::collections::BTreeSet;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ClipId, MulticamAngleId, TimelineId};
use superi_core::time::{RationalTime, TimeRange, TimeRounding};

use crate::markers::TimelineMetadata;
use crate::model::{Clip, ClipSource, EditorialProject, Timeline, TrackItem};

/// The authored evidence used to synchronize a multicam source timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MulticamSyncMethod {
    /// Source clips were aligned directly by an editor.
    Manual,
    /// Source clips were aligned by embedded or assigned timecode.
    Timecode,
    /// Source clips were aligned by their marked in points.
    InPoints,
    /// Source clips were aligned by their marked out points.
    OutPoints,
    /// Source clips were aligned by one named clip marker.
    ClipMarker(String),
    /// Source clips were aligned by waveform correlation.
    Audio,
}

impl MulticamSyncMethod {
    fn validate(&self) -> Result<()> {
        if let Self::ClipMarker(name) = self {
            require_text(
                "validate_sync_method",
                "multicam synchronization marker name",
                name,
            )?;
        }
        Ok(())
    }
}

/// One stable camera angle and its ordered synchronized source membership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MulticamAngle {
    id: MulticamAngleId,
    name: String,
    camera_label: String,
    enabled: bool,
    metadata: TimelineMetadata,
    source_clips: Vec<ClipId>,
}

impl MulticamAngle {
    /// Creates one enabled angle with caller-owned source clip order.
    pub fn new<I>(
        id: MulticamAngleId,
        name: impl Into<String>,
        camera_label: impl Into<String>,
        source_clips: I,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = ClipId>,
    {
        let angle = Self {
            id,
            name: name.into(),
            camera_label: camera_label.into(),
            enabled: true,
            metadata: TimelineMetadata::new(),
            source_clips: source_clips.into_iter().collect(),
        };
        angle.validate()?;
        Ok(angle)
    }

    /// Returns the stable angle identity.
    #[must_use]
    pub const fn id(&self) -> MulticamAngleId {
        self.id
    }

    /// Returns the editor-facing angle name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the camera label used for professional angle naming.
    #[must_use]
    pub fn camera_label(&self) -> &str {
        &self.camera_label
    }

    /// Returns whether this angle may be selected for playback.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns deterministic angle metadata.
    #[must_use]
    pub const fn metadata(&self) -> &TimelineMetadata {
        &self.metadata
    }

    /// Returns synchronized source clips in authored angle order.
    #[must_use]
    pub fn source_clips(&self) -> &[ClipId] {
        &self.source_clips
    }

    /// Replaces the editor-facing angle name.
    pub fn set_name(&mut self, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        require_text("rename_angle", "multicam angle name", &name)?;
        self.name = name;
        Ok(())
    }

    /// Replaces the professional camera label.
    pub fn set_camera_label(&mut self, camera_label: impl Into<String>) -> Result<()> {
        let camera_label = camera_label.into();
        require_text("label_angle", "multicam camera label", &camera_label)?;
        self.camera_label = camera_label;
        Ok(())
    }

    /// Enables or disables this angle inside an unpublished project draft.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Replaces deterministic angle metadata.
    pub fn set_metadata(&mut self, metadata: TimelineMetadata) {
        self.metadata = metadata;
    }

    /// Replaces ordered source membership.
    pub fn replace_source_clips<I>(&mut self, source_clips: I) -> Result<()>
    where
        I: IntoIterator<Item = ClipId>,
    {
        let source_clips: Vec<_> = source_clips.into_iter().collect();
        validate_unique_clips(&source_clips)?;
        self.source_clips = source_clips;
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        require_text("validate_angle", "multicam angle name", &self.name)?;
        require_text(
            "validate_angle",
            "multicam camera label",
            &self.camera_label,
        )?;
        validate_unique_clips(&self.source_clips)
    }

    fn inherit_source_fragment(&mut self, original: ClipId, created: ClipId) {
        if self.source_clips.contains(&created) {
            return;
        }
        if let Some(index) = self.source_clips.iter().position(|id| *id == original) {
            self.source_clips.insert(index + 1, created);
        }
    }

    fn transfer_source_clip(&mut self, removed: ClipId, inserted: ClipId) {
        if let Some(index) = self.source_clips.iter().position(|id| *id == removed) {
            self.source_clips[index] = inserted;
            self.source_clips.dedup();
        }
    }

    fn reconcile(&mut self, existing: &BTreeSet<ClipId>) {
        self.source_clips.retain(|id| existing.contains(id));
    }
}

/// The ordered angle catalog owned by one synchronized source timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MulticamSource {
    sync_method: MulticamSyncMethod,
    angles: Vec<MulticamAngle>,
}

impl MulticamSource {
    /// Creates a synchronized source with at least two stable camera angles.
    pub fn new<I>(sync_method: MulticamSyncMethod, angles: I) -> Result<Self>
    where
        I: IntoIterator<Item = MulticamAngle>,
    {
        let source = Self {
            sync_method,
            angles: angles.into_iter().collect(),
        };
        source.validate()?;
        Ok(source)
    }

    /// Returns the authored synchronization provenance.
    #[must_use]
    pub const fn sync_method(&self) -> &MulticamSyncMethod {
        &self.sync_method
    }

    /// Returns camera angles in stable editor-facing order.
    #[must_use]
    pub fn angles(&self) -> &[MulticamAngle] {
        &self.angles
    }

    /// Looks up one camera angle by stable identity.
    #[must_use]
    pub fn angle(&self, id: MulticamAngleId) -> Option<&MulticamAngle> {
        self.angles.iter().find(|angle| angle.id() == id)
    }

    /// Mutably looks up one camera angle inside an unpublished draft.
    pub fn angle_mut(&mut self, id: MulticamAngleId) -> Result<&mut MulticamAngle> {
        self.angles
            .iter_mut()
            .find(|angle| angle.id() == id)
            .ok_or_else(|| not_found("find_angle", "multicam angle was not found", "angle", id))
    }

    /// Replaces synchronization provenance after validating method-specific data.
    pub fn set_sync_method(&mut self, sync_method: MulticamSyncMethod) -> Result<()> {
        sync_method.validate()?;
        self.sync_method = sync_method;
        Ok(())
    }

    /// Replaces the complete ordered angle catalog atomically.
    pub fn replace_angles<I>(&mut self, angles: I) -> Result<()>
    where
        I: IntoIterator<Item = MulticamAngle>,
    {
        let candidate = Self::new(self.sync_method.clone(), angles)?;
        self.angles = candidate.angles;
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<()> {
        self.sync_method.validate()?;
        if self.angles.len() < 2 {
            return Err(invalid(
                "validate_source",
                "a multicam source must retain at least two camera angles",
            ));
        }
        let mut angle_ids = BTreeSet::new();
        let mut clip_ids = BTreeSet::new();
        for angle in &self.angles {
            angle.validate()?;
            if !angle_ids.insert(angle.id()) {
                return Err(conflict(
                    "validate_source",
                    "duplicate multicam angle identity",
                    "angle",
                    angle.id(),
                ));
            }
            for clip_id in angle.source_clips() {
                if !clip_ids.insert(*clip_id) {
                    return Err(conflict(
                        "validate_source",
                        "one source clip cannot belong to multiple multicam angles",
                        "clip",
                        clip_id,
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn inherit_source_fragment(&mut self, original: ClipId, created: ClipId) {
        for angle in &mut self.angles {
            angle.inherit_source_fragment(original, created);
        }
    }

    pub(crate) fn transfer_source_clip(&mut self, removed: ClipId, inserted: ClipId) {
        for angle in &mut self.angles {
            angle.transfer_source_clip(removed, inserted);
        }
    }

    pub(crate) fn reconcile(&mut self, existing: &BTreeSet<ClipId>) {
        for angle in &mut self.angles {
            angle.reconcile(existing);
        }
    }
}

/// Audio selection intent for one multicam target clip.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MulticamAudioPolicy {
    /// Audio follows each video angle switch.
    FollowVideo,
    /// Audio remains on one explicit angle.
    Fixed(MulticamAngleId),
    /// Every enabled angle remains active for downstream mixing.
    AllAngles,
}

/// One half-open active-angle interval in synchronized source coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MulticamSwitch {
    source_range: TimeRange,
    angle_id: MulticamAngleId,
}

impl MulticamSwitch {
    /// Returns the exact interval in synchronized source-timeline coordinates.
    #[must_use]
    pub const fn source_range(self) -> TimeRange {
        self.source_range
    }

    /// Returns the selected camera angle.
    #[must_use]
    pub const fn angle_id(self) -> MulticamAngleId {
        self.angle_id
    }
}

/// Clip-local multicam switch intent over one complete synchronized source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MulticamClip {
    clip_id: ClipId,
    switches: Vec<MulticamSwitch>,
    audio_policy: MulticamAudioPolicy,
}

impl MulticamClip {
    /// Creates one clip-local switch program covering the supplied source extent.
    pub fn new(
        clip_id: ClipId,
        source_range: TimeRange,
        initial_angle: MulticamAngleId,
        audio_policy: MulticamAudioPolicy,
    ) -> Result<Self> {
        if source_range.is_empty() || source_range.start().is_negative() {
            return Err(invalid(
                "create_multicam_clip",
                "multicam switch coverage must be nonempty and start at or after source zero",
            ));
        }
        Ok(Self {
            clip_id,
            switches: vec![MulticamSwitch {
                source_range,
                angle_id: initial_angle,
            }],
            audio_policy,
        })
    }

    /// Returns the ordinary nested clip identity that owns this switch program.
    #[must_use]
    pub const fn clip_id(&self) -> ClipId {
        self.clip_id
    }

    /// Returns the complete gapless switch partition in source order.
    #[must_use]
    pub fn switches(&self) -> &[MulticamSwitch] {
        &self.switches
    }

    /// Returns the complete synchronized source extent owned by this switch program.
    #[must_use]
    pub fn source_range(&self) -> TimeRange {
        self.coverage()
    }

    /// Returns explicit audio selection intent.
    #[must_use]
    pub const fn audio_policy(&self) -> &MulticamAudioPolicy {
        &self.audio_policy
    }

    /// Replaces explicit audio selection intent.
    pub fn set_audio_policy(&mut self, audio_policy: MulticamAudioPolicy) {
        self.audio_policy = audio_policy;
    }

    /// Resolves the active angle at one synchronized source coordinate.
    pub fn angle_at(&self, source_time: RationalTime) -> Result<MulticamAngleId> {
        for switch in &self.switches {
            if switch.source_range.contains(source_time)? {
                return Ok(switch.angle_id);
            }
        }
        Err(invalid(
            "resolve_angle",
            "multicam source time lies outside the clip switch program",
        ))
    }

    /// Selects one angle over an exact source subrange and coalesces equal neighbors.
    pub fn switch_range(
        &mut self,
        source_range: TimeRange,
        angle_id: MulticamAngleId,
    ) -> Result<()> {
        let coverage = self.coverage();
        if source_range.is_empty()
            || source_range.timebase() != coverage.timebase()
            || source_range.start() < coverage.start()
            || source_range.end_exclusive()? > coverage.end_exclusive()?
        {
            return Err(invalid(
                "switch_angle",
                "multicam switch range must be a nonempty subrange of complete source coverage",
            ));
        }
        let range_end = source_range.end_exclusive()?;
        let mut output = Vec::with_capacity(self.switches.len() + 2);
        for switch in &self.switches {
            let switch_end = switch.source_range.end_exclusive()?;
            if switch_end <= source_range.start() || switch.source_range.start() >= range_end {
                push_switch(&mut output, *switch)?;
                continue;
            }
            if switch.source_range.start() < source_range.start() {
                push_switch(
                    &mut output,
                    MulticamSwitch {
                        source_range: TimeRange::from_start_end(
                            switch.source_range.start(),
                            source_range.start(),
                        )?,
                        angle_id: switch.angle_id,
                    },
                )?;
            }
            let overlap_start = if switch.source_range.start() > source_range.start() {
                switch.source_range.start()
            } else {
                source_range.start()
            };
            let overlap_end = if switch_end < range_end {
                switch_end
            } else {
                range_end
            };
            push_switch(
                &mut output,
                MulticamSwitch {
                    source_range: TimeRange::from_start_end(overlap_start, overlap_end)?,
                    angle_id,
                },
            )?;
            if switch_end > range_end {
                push_switch(
                    &mut output,
                    MulticamSwitch {
                        source_range: TimeRange::from_start_end(range_end, switch_end)?,
                        angle_id: switch.angle_id,
                    },
                )?;
            }
        }
        self.switches = output;
        self.validate_partition()?;
        Ok(())
    }

    /// Moves one existing angle cut while retaining the angle on each side.
    pub fn move_cut(&mut self, at: RationalTime, to: RationalTime) -> Result<()> {
        if at.timebase() != to.timebase() {
            return Err(invalid(
                "move_switch_cut",
                "multicam cut coordinates must use one exact source clock",
            ));
        }
        let Some(index) = self.switches.windows(2).position(|pair| {
            pair[0].source_range.end_exclusive().ok() == Some(at)
                && pair[1].source_range.start() == at
        }) else {
            return Err(not_found(
                "move_switch_cut",
                "multicam switch cut was not found",
                "cut",
                at,
            ));
        };
        let left_start = self.switches[index].source_range.start();
        let right_end = self.switches[index + 1].source_range.end_exclusive()?;
        if to <= left_start || to >= right_end || to == at {
            return Err(invalid(
                "move_switch_cut",
                "moved multicam cut must stay inside both neighboring switch extents",
            ));
        }
        self.switches[index].source_range = TimeRange::from_start_end(left_start, to)?;
        self.switches[index + 1].source_range = TimeRange::from_start_end(to, right_end)?;
        self.validate_partition()
    }

    pub(crate) fn clone_with_clip_id(&self, clip_id: ClipId) -> Self {
        let mut cloned = self.clone();
        cloned.clip_id = clip_id;
        cloned
    }

    pub(crate) fn validate_against(
        &self,
        source: &MulticamSource,
        complete_source_range: TimeRange,
    ) -> Result<()> {
        self.validate_partition()?;
        if self.coverage() != complete_source_range {
            return Err(invalid(
                "validate_multicam_clip",
                "multicam switch program must exactly cover the complete source timeline",
            ));
        }
        for switch in &self.switches {
            let angle = source.angle(switch.angle_id).ok_or_else(|| {
                not_found(
                    "validate_multicam_clip",
                    "multicam switch references a missing source angle",
                    "angle",
                    switch.angle_id,
                )
            })?;
            if !angle.enabled() {
                return Err(invalid(
                    "validate_multicam_clip",
                    "multicam switch references a disabled source angle",
                ));
            }
        }
        if let MulticamAudioPolicy::Fixed(angle_id) = self.audio_policy {
            let angle = source.angle(angle_id).ok_or_else(|| {
                not_found(
                    "validate_multicam_clip",
                    "fixed multicam audio references a missing source angle",
                    "angle",
                    angle_id,
                )
            })?;
            if !angle.enabled() {
                return Err(invalid(
                    "validate_multicam_clip",
                    "fixed multicam audio references a disabled source angle",
                ));
            }
        }
        Ok(())
    }

    fn coverage(&self) -> TimeRange {
        let first = self.switches.first().expect("multicam clip has coverage");
        let last = self.switches.last().expect("multicam clip has coverage");
        TimeRange::from_start_end(
            first.source_range.start(),
            last.source_range
                .end_exclusive()
                .expect("valid switch range"),
        )
        .expect("validated switch clock")
    }

    fn validate_partition(&self) -> Result<()> {
        if self.switches.is_empty() {
            return Err(invalid(
                "validate_switches",
                "multicam clip must retain one or more switch intervals",
            ));
        }
        let mut expected = self.switches[0].source_range.start();
        for switch in &self.switches {
            if switch.source_range.is_empty()
                || switch.source_range.start() != expected
                || switch.source_range.timebase() != expected.timebase()
            {
                return Err(invalid(
                    "validate_switches",
                    "multicam switch intervals must be nonempty, gapless, and use one source clock",
                ));
            }
            expected = switch.source_range.end_exclusive()?;
        }
        Ok(())
    }
}

/// One exact target-to-angle-to-source resolution for immediate transport use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedMulticamFrame {
    target_timeline_id: TimelineId,
    target_clip_id: ClipId,
    source_timeline_id: TimelineId,
    source_timeline_time: RationalTime,
    angle_id: MulticamAngleId,
    source_clip_id: ClipId,
    source: ClipSource,
    source_time: RationalTime,
    audio_angle_ids: Vec<MulticamAngleId>,
}

impl ResolvedMulticamFrame {
    /// Returns the timeline that owns the target multicam clip.
    #[must_use]
    pub const fn target_timeline_id(&self) -> TimelineId {
        self.target_timeline_id
    }

    /// Returns the ordinary nested clip that owns switch intent.
    #[must_use]
    pub const fn target_clip_id(&self) -> ClipId {
        self.target_clip_id
    }

    /// Returns the synchronized source timeline.
    #[must_use]
    pub const fn source_timeline_id(&self) -> TimelineId {
        self.source_timeline_id
    }

    /// Returns the exact synchronized source-timeline coordinate.
    #[must_use]
    pub const fn source_timeline_time(&self) -> RationalTime {
        self.source_timeline_time
    }

    /// Returns the active video angle.
    #[must_use]
    pub const fn angle_id(&self) -> MulticamAngleId {
        self.angle_id
    }

    /// Returns the selected source clip within the active angle.
    #[must_use]
    pub const fn source_clip_id(&self) -> ClipId {
        self.source_clip_id
    }

    /// Returns the selected clip's direct media or nested-timeline relationship.
    #[must_use]
    pub const fn source(&self) -> ClipSource {
        self.source
    }

    /// Returns the exact coordinate selected through the source clip time map.
    #[must_use]
    pub const fn source_time(&self) -> RationalTime {
        self.source_time
    }

    /// Returns active audio angles in source angle order.
    #[must_use]
    pub fn audio_angle_ids(&self) -> &[MulticamAngleId] {
        &self.audio_angle_ids
    }
}

/// Resolves a target record coordinate through clip retiming and synchronized angle state.
pub fn resolve_multicam_frame(
    project: &EditorialProject,
    target_timeline_id: TimelineId,
    target_clip_id: ClipId,
    target_record_time: RationalTime,
) -> Result<ResolvedMulticamFrame> {
    let target_timeline = project.timeline(target_timeline_id).ok_or_else(|| {
        not_found(
            "resolve_multicam_frame",
            "target timeline was not found",
            "timeline",
            target_timeline_id,
        )
    })?;
    let target_clip = find_clip(target_timeline, target_clip_id).ok_or_else(|| {
        not_found(
            "resolve_multicam_frame",
            "target multicam clip was not found",
            "clip",
            target_clip_id,
        )
    })?;
    let clip_state = target_timeline
        .multicam_clip(target_clip_id)
        .ok_or_else(|| {
            not_found(
                "resolve_multicam_frame",
                "target clip has no multicam switch state",
                "clip",
                target_clip_id,
            )
        })?;
    let ClipSource::Timeline(source_timeline_id) = target_clip.source() else {
        return Err(invalid(
            "resolve_multicam_frame",
            "multicam target must retain an ordinary nested timeline source",
        ));
    };
    let source_timeline_time = target_clip
        .source_time_at(target_record_time, TimeRounding::Exact)?
        .time();
    let angle_id = clip_state.angle_at(source_timeline_time)?;
    let source_timeline = project
        .timeline(source_timeline_id)
        .expect("validated nested source exists");
    let source_state = source_timeline
        .multicam_source()
        .expect("validated multicam source exists");
    let angle = source_state
        .angle(angle_id)
        .expect("validated switch angle exists");
    let mut selected_source_clip = None;
    for clip_id in angle.source_clips() {
        let clip = find_clip(source_timeline, *clip_id).expect("validated angle member exists");
        if clip.record_range().contains(source_timeline_time)? {
            selected_source_clip = Some(clip);
            break;
        }
    }
    let source_clip = selected_source_clip.ok_or_else(|| {
        not_found(
            "resolve_multicam_frame",
            "active multicam angle has no source clip at this synchronized time",
            "angle",
            angle_id,
        )
    })?;
    let source_time = source_clip
        .source_time_at(source_timeline_time, TimeRounding::Exact)?
        .time();
    let audio_angle_ids = match clip_state.audio_policy() {
        MulticamAudioPolicy::FollowVideo => vec![angle_id],
        MulticamAudioPolicy::Fixed(angle_id) => vec![*angle_id],
        MulticamAudioPolicy::AllAngles => source_state
            .angles()
            .iter()
            .filter(|angle| angle.enabled())
            .map(MulticamAngle::id)
            .collect(),
    };
    Ok(ResolvedMulticamFrame {
        target_timeline_id,
        target_clip_id,
        source_timeline_id,
        source_timeline_time,
        angle_id,
        source_clip_id: source_clip.id(),
        source: source_clip.source(),
        source_time,
        audio_angle_ids,
    })
}

pub(crate) fn find_clip(timeline: &Timeline, id: ClipId) -> Option<&Clip> {
    timeline
        .tracks()
        .iter()
        .flat_map(|track| track.items())
        .filter_map(TrackItem::as_clip)
        .find(|clip| clip.id() == id)
}

fn push_switch(output: &mut Vec<MulticamSwitch>, next: MulticamSwitch) -> Result<()> {
    if let Some(last) = output.last_mut() {
        if last.angle_id == next.angle_id
            && last.source_range.end_exclusive()? == next.source_range.start()
        {
            last.source_range = TimeRange::from_start_end(
                last.source_range.start(),
                next.source_range.end_exclusive()?,
            )?;
            return Ok(());
        }
    }
    output.push(next);
    Ok(())
}

fn validate_unique_clips(source_clips: &[ClipId]) -> Result<()> {
    let mut ids = BTreeSet::new();
    for id in source_clips {
        if !ids.insert(*id) {
            return Err(conflict(
                "validate_angle",
                "duplicate source clip identity within one multicam angle",
                "clip",
                id,
            ));
        }
    }
    Ok(())
}

fn require_text(operation: &'static str, field: &'static str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::new(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "multicam text value must not be blank",
        )
        .with_context(
            ErrorContext::new("superi-timeline.multicam", operation).with_field("field", field),
        ));
    }
    Ok(())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.multicam", operation))
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
        ErrorContext::new("superi-timeline.multicam", operation)
            .with_field(field, value.to_string()),
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
        ErrorContext::new("superi-timeline.multicam", operation)
            .with_field(field, value.to_string()),
    )
}
