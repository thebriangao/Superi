//! Atomic foundational and advanced editorial operations over validated project drafts.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::ids::{ClipId, GapId, TimelineId, TrackId, TransitionId};
use superi_core::time::{Duration, RationalTime, TimeRange, TimeRounding, Timebase};

use crate::edit_state::{SelectionExpansion, SelectionUpdate};
use crate::markers::MetadataOwner;
use crate::model::{
    Caption, Clip, EditorialObjectId, EditorialProject, Gap, Generator, ProjectDraft, Timeline,
    Track, TrackItem, TrackKind, Transition,
};
use crate::retime::ClipTimeMap;

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
    /// Link one video clip and one audio clip for synchronized editorial selection.
    LinkAudioVideo,
    /// Align one audio clip's source time to one video clip and link the pair.
    SynchronizeAudioVideo,
    /// Remove one audio clip from its linked video relationship.
    DetachAudio,
    /// Replace a record range with an explicit gap.
    Lift,
    /// Remove a record range and ripple later content left.
    Extract,
    /// Move one edit edge and shift all later material by the same exact delta.
    Ripple,
    /// Move one shared cut while preserving the combined span.
    Roll,
    /// Move one clip's source window without changing record placement.
    Slip,
    /// Move one clip while compensating both adjacent source windows.
    Slide,
    /// Split one timed object at an exact interior point.
    Razor,
    /// Move one edge while preserving surrounding record placement.
    Trim,
    /// Move one edit point through explicit ripple or roll semantics.
    Extend,
    /// Replace both exact handles of one existing transition atomically.
    SetTransition,
    /// Derive one missing source or record boundary from three supplied points.
    ThreePoint,
    /// Place one exact source range over one exact record range.
    FourPoint,
    /// Replace one clip's exact source-to-record time mapping.
    Retime,
}

impl EditKind {
    const fn code(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Overwrite => "overwrite",
            Self::Append => "append",
            Self::Replace => "replace",
            Self::LinkAudioVideo => "link_audio_video",
            Self::SynchronizeAudioVideo => "synchronize_audio_video",
            Self::DetachAudio => "detach_audio",
            Self::Lift => "lift",
            Self::Extract => "extract",
            Self::Ripple => "ripple",
            Self::Roll => "roll",
            Self::Slip => "slip",
            Self::Slide => "slide",
            Self::Razor => "razor",
            Self::Trim => "trim",
            Self::Extend => "extend",
            Self::SetTransition => "set_transition",
            Self::ThreePoint => "three_point",
            Self::FourPoint => "four_point",
            Self::Retime => "retime",
        }
    }
}

/// The independently controllable edge of one timed editorial object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditSide {
    /// The inclusive start of the record range.
    Start,
    /// The exclusive end of the record range.
    End,
}

/// Existing trim behavior reused by an extend-to-point command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ExtendMode {
    /// Change duration and ripple later material.
    Ripple,
    /// Move the shared cut against the adjacent timed object.
    Roll,
}

/// One of the four valid three-point placements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ThreePointPlacement {
    /// Use a source range and forward record start.
    SourceRangeAtRecordStart {
        /// Selected source interval.
        source_range: TimeRange,
        /// Forward placement start.
        record_start: RationalTime,
    },
    /// Use a source start and complete record range.
    SourceStartOverRecordRange {
        /// Selected source start.
        source_start: RationalTime,
        /// Complete record placement.
        record_range: TimeRange,
    },
    /// Backtime a source range to a record end.
    SourceRangeBacktimedToRecordEnd {
        /// Selected source interval.
        source_range: TimeRange,
        /// Exclusive record end.
        record_end: RationalTime,
    },
    /// Backtime a source end over a complete record range.
    SourceEndBacktimedOverRecordRange {
        /// Exclusive selected source end.
        source_end: RationalTime,
        /// Complete record placement.
        record_range: TimeRange,
    },
}

/// Caller-owned identities needed to ripple one sync-locked companion track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RippleSyncAdjustment {
    track_id: TrackId,
    gap_id: GapId,
    fragment_ids: Vec<EditorialObjectId>,
}

impl RippleSyncAdjustment {
    /// Creates one deterministic companion-track adjustment.
    pub fn new<I>(track_id: TrackId, gap_id: GapId, fragment_ids: I) -> Self
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        Self {
            track_id,
            gap_id,
            fragment_ids: fragment_ids.into_iter().collect(),
        }
    }

    /// Returns the sync-locked companion track.
    #[must_use]
    pub const fn track_id(&self) -> TrackId {
        self.track_id
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
    /// Link one exact video clip and audio clip without changing timing.
    LinkAudioVideo {
        timeline_id: TimelineId,
        video_track_id: TrackId,
        video_clip_id: ClipId,
        audio_track_id: TrackId,
        audio_clip_id: ClipId,
    },
    /// Align audio source time to video source time at their record positions and link the pair.
    SynchronizeAudioVideo {
        timeline_id: TimelineId,
        video_track_id: TrackId,
        video_clip_id: ClipId,
        audio_track_id: TrackId,
        audio_clip_id: ClipId,
    },
    /// Remove one exact audio clip from its current linked video component.
    DetachAudio {
        timeline_id: TimelineId,
        video_track_id: TrackId,
        video_clip_id: ClipId,
        audio_track_id: TrackId,
        audio_clip_id: ClipId,
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
    /// Move one edit edge and ripple every later item on the same track.
    Ripple {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the target object.
        track_id: TrackId,
        /// Stable identity of the timed object whose edge moves.
        target_id: EditorialObjectId,
        /// Edge to move.
        side: EditSide,
        /// Exact destination in the target track clock.
        to: RationalTime,
        /// Deterministic companion-track material in canonical track order.
        sync_adjustments: Vec<RippleSyncAdjustment>,
    },
    /// Move one shared cut while preserving its combined two-item span.
    Roll {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing both adjacent timed objects.
        track_id: TrackId,
        /// Timed object before the shared cut.
        left_id: EditorialObjectId,
        /// Timed object after the shared cut.
        right_id: EditorialObjectId,
        /// Exact new cut point.
        to: RationalTime,
    },
    /// Move one clip's source window without changing its record range.
    Slip {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the clip.
        track_id: TrackId,
        /// Stable clip identity.
        clip_id: ClipId,
        /// Exact new source start in the clip source clock.
        source_start: RationalTime,
    },
    /// Move one center clip while compensating its two adjacent clips.
    Slide {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the three-clip span.
        track_id: TrackId,
        /// Stable identity of the center clip.
        clip_id: ClipId,
        /// Exact new center record start.
        to: RationalTime,
    },
    /// Split one timed object at an exact interior record point.
    Razor {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the target object.
        track_id: TrackId,
        /// Stable identity retained by the left fragment.
        target_id: EditorialObjectId,
        /// Exact interior split point.
        at: RationalTime,
        /// Caller-supplied stable identity for the right fragment.
        fragment_id: EditorialObjectId,
    },
    /// Move one edge without moving surrounding non-gap material.
    Trim {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the target object.
        track_id: TrackId,
        /// Stable identity of the timed object whose edge moves.
        target_id: EditorialObjectId,
        /// Edge to move.
        side: EditSide,
        /// Exact destination in the target track clock.
        to: RationalTime,
        /// Caller-supplied gap identity used only when an inward trim creates a gap.
        gap_id: Option<GapId>,
    },
    /// Extend one edit point through explicit ripple or roll behavior.
    Extend {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the target object.
        track_id: TrackId,
        /// Stable identity of the timed object whose edge moves.
        target_id: EditorialObjectId,
        /// Edge to move.
        side: EditSide,
        /// Exact destination in the target track clock.
        to: RationalTime,
        /// Existing trim behavior to reuse.
        mode: ExtendMode,
        /// Deterministic companion-track material for ripple-mode extension.
        sync_adjustments: Vec<RippleSyncAdjustment>,
    },
    /// Replace both overlap handles of one existing transition.
    SetTransition {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the transition.
        track_id: TrackId,
        /// Stable identity of the transition to update.
        transition_id: TransitionId,
        /// Exact time consumed from the preceding item.
        from_offset: Duration,
        /// Exact time consumed from the following item.
        to_offset: Duration,
    },
    /// Derive one missing boundary and overwrite the resulting record range.
    ThreePoint {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track receiving the clip.
        track_id: TrackId,
        /// Source-bearing clip identity, name, and source relationship.
        clip: Clip,
        /// Three supplied boundaries and the missing boundary rule.
        placement: ThreePointPlacement,
        /// Typed identities for right fragments, consumed in record order.
        fragment_ids: Vec<EditorialObjectId>,
    },
    /// Overwrite one exact record range from one equal-duration source range.
    FourPoint {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track receiving the clip.
        track_id: TrackId,
        /// Source-bearing clip identity, name, and source relationship.
        clip: Clip,
        /// Complete selected source interval.
        source_range: TimeRange,
        /// Complete record placement.
        record_range: TimeRange,
        /// Typed identities for right fragments, consumed in record order.
        fragment_ids: Vec<EditorialObjectId>,
    },
    /// Replace one clip's exact source-to-record time mapping.
    Retime {
        /// Timeline containing the target track.
        timeline_id: TimelineId,
        /// Track containing the target clip.
        track_id: TrackId,
        /// Stable identity of the clip whose mapping changes.
        clip_id: ClipId,
        /// Complete validated mapping for the clip's unchanged record duration.
        time_map: ClipTimeMap,
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

    /// Creates an exact audio-video link operation.
    #[must_use]
    pub const fn link_audio_video(
        timeline_id: TimelineId,
        video_track_id: TrackId,
        video_clip_id: ClipId,
        audio_track_id: TrackId,
        audio_clip_id: ClipId,
    ) -> Self {
        Self::LinkAudioVideo {
            timeline_id,
            video_track_id,
            video_clip_id,
            audio_track_id,
            audio_clip_id,
        }
    }

    /// Creates an exact source-time audio-video synchronization operation.
    #[must_use]
    pub const fn synchronize_audio_video(
        timeline_id: TimelineId,
        video_track_id: TrackId,
        video_clip_id: ClipId,
        audio_track_id: TrackId,
        audio_clip_id: ClipId,
    ) -> Self {
        Self::SynchronizeAudioVideo {
            timeline_id,
            video_track_id,
            video_clip_id,
            audio_track_id,
            audio_clip_id,
        }
    }

    /// Creates an exact audio detach operation.
    #[must_use]
    pub const fn detach_audio(
        timeline_id: TimelineId,
        video_track_id: TrackId,
        video_clip_id: ClipId,
        audio_track_id: TrackId,
        audio_clip_id: ClipId,
    ) -> Self {
        Self::DetachAudio {
            timeline_id,
            video_track_id,
            video_clip_id,
            audio_track_id,
            audio_clip_id,
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

    /// Creates a ripple operation.
    #[must_use]
    pub fn ripple(
        timeline_id: TimelineId,
        track_id: TrackId,
        target_id: EditorialObjectId,
        side: EditSide,
        to: RationalTime,
    ) -> Self {
        Self::Ripple {
            timeline_id,
            track_id,
            target_id,
            side,
            to,
            sync_adjustments: Vec::new(),
        }
    }

    /// Creates a ripple operation with explicit sync-locked companion tracks.
    pub fn ripple_synchronized<I>(
        timeline_id: TimelineId,
        track_id: TrackId,
        target_id: EditorialObjectId,
        side: EditSide,
        to: RationalTime,
        sync_adjustments: I,
    ) -> Self
    where
        I: IntoIterator<Item = RippleSyncAdjustment>,
    {
        Self::Ripple {
            timeline_id,
            track_id,
            target_id,
            side,
            to,
            sync_adjustments: sync_adjustments.into_iter().collect(),
        }
    }

    /// Creates a roll operation.
    #[must_use]
    pub fn roll(
        timeline_id: TimelineId,
        track_id: TrackId,
        left_id: EditorialObjectId,
        right_id: EditorialObjectId,
        to: RationalTime,
    ) -> Self {
        Self::Roll {
            timeline_id,
            track_id,
            left_id,
            right_id,
            to,
        }
    }

    /// Creates a slip operation.
    #[must_use]
    pub fn slip(
        timeline_id: TimelineId,
        track_id: TrackId,
        clip_id: ClipId,
        source_start: RationalTime,
    ) -> Self {
        Self::Slip {
            timeline_id,
            track_id,
            clip_id,
            source_start,
        }
    }

    /// Creates a slide operation.
    #[must_use]
    pub fn slide(
        timeline_id: TimelineId,
        track_id: TrackId,
        clip_id: ClipId,
        to: RationalTime,
    ) -> Self {
        Self::Slide {
            timeline_id,
            track_id,
            clip_id,
            to,
        }
    }

    /// Creates a razor operation.
    #[must_use]
    pub fn razor(
        timeline_id: TimelineId,
        track_id: TrackId,
        target_id: EditorialObjectId,
        at: RationalTime,
        fragment_id: EditorialObjectId,
    ) -> Self {
        Self::Razor {
            timeline_id,
            track_id,
            target_id,
            at,
            fragment_id,
        }
    }

    /// Creates a placement-preserving trim operation.
    #[must_use]
    pub fn trim(
        timeline_id: TimelineId,
        track_id: TrackId,
        target_id: EditorialObjectId,
        side: EditSide,
        to: RationalTime,
        gap_id: Option<GapId>,
    ) -> Self {
        Self::Trim {
            timeline_id,
            track_id,
            target_id,
            side,
            to,
            gap_id,
        }
    }

    /// Creates an extend-to-point operation.
    #[must_use]
    pub fn extend(
        timeline_id: TimelineId,
        track_id: TrackId,
        target_id: EditorialObjectId,
        side: EditSide,
        to: RationalTime,
        mode: ExtendMode,
    ) -> Self {
        Self::Extend {
            timeline_id,
            track_id,
            target_id,
            side,
            to,
            mode,
            sync_adjustments: Vec::new(),
        }
    }

    /// Creates a ripple-mode extension with explicit sync-locked companion tracks.
    pub fn extend_synchronized<I>(
        timeline_id: TimelineId,
        track_id: TrackId,
        target_id: EditorialObjectId,
        side: EditSide,
        to: RationalTime,
        sync_adjustments: I,
    ) -> Self
    where
        I: IntoIterator<Item = RippleSyncAdjustment>,
    {
        Self::Extend {
            timeline_id,
            track_id,
            target_id,
            side,
            to,
            mode: ExtendMode::Ripple,
            sync_adjustments: sync_adjustments.into_iter().collect(),
        }
    }

    /// Creates an atomic transition-handle update.
    #[must_use]
    pub fn set_transition(
        timeline_id: TimelineId,
        track_id: TrackId,
        transition_id: TransitionId,
        from_offset: Duration,
        to_offset: Duration,
    ) -> Self {
        Self::SetTransition {
            timeline_id,
            track_id,
            transition_id,
            from_offset,
            to_offset,
        }
    }

    /// Creates a three-point overwrite operation.
    pub fn three_point<I>(
        timeline_id: TimelineId,
        track_id: TrackId,
        clip: Clip,
        placement: ThreePointPlacement,
        fragment_ids: I,
    ) -> Self
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        Self::ThreePoint {
            timeline_id,
            track_id,
            clip,
            placement,
            fragment_ids: fragment_ids.into_iter().collect(),
        }
    }

    /// Creates an exact four-point overwrite operation.
    pub fn four_point<I>(
        timeline_id: TimelineId,
        track_id: TrackId,
        clip: Clip,
        source_range: TimeRange,
        record_range: TimeRange,
        fragment_ids: I,
    ) -> Self
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        Self::FourPoint {
            timeline_id,
            track_id,
            clip,
            source_range,
            record_range,
            fragment_ids: fragment_ids.into_iter().collect(),
        }
    }

    /// Creates an exact clip retime operation.
    #[must_use]
    pub fn retime(
        timeline_id: TimelineId,
        track_id: TrackId,
        clip_id: ClipId,
        time_map: ClipTimeMap,
    ) -> Self {
        Self::Retime {
            timeline_id,
            track_id,
            clip_id,
            time_map,
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
            Self::LinkAudioVideo { .. } => EditKind::LinkAudioVideo,
            Self::SynchronizeAudioVideo { .. } => EditKind::SynchronizeAudioVideo,
            Self::DetachAudio { .. } => EditKind::DetachAudio,
            Self::Lift { .. } => EditKind::Lift,
            Self::Extract { .. } => EditKind::Extract,
            Self::Ripple { .. } => EditKind::Ripple,
            Self::Roll { .. } => EditKind::Roll,
            Self::Slip { .. } => EditKind::Slip,
            Self::Slide { .. } => EditKind::Slide,
            Self::Razor { .. } => EditKind::Razor,
            Self::Trim { .. } => EditKind::Trim,
            Self::Extend { .. } => EditKind::Extend,
            Self::SetTransition { .. } => EditKind::SetTransition,
            Self::ThreePoint { .. } => EditKind::ThreePoint,
            Self::FourPoint { .. } => EditKind::FourPoint,
            Self::Retime { .. } => EditKind::Retime,
        }
    }

    const fn timeline_id(&self) -> TimelineId {
        match self {
            Self::Insert { timeline_id, .. }
            | Self::Overwrite { timeline_id, .. }
            | Self::Append { timeline_id, .. }
            | Self::Replace { timeline_id, .. }
            | Self::LinkAudioVideo { timeline_id, .. }
            | Self::SynchronizeAudioVideo { timeline_id, .. }
            | Self::DetachAudio { timeline_id, .. }
            | Self::Lift { timeline_id, .. }
            | Self::Extract { timeline_id, .. } => *timeline_id,
            Self::Ripple { timeline_id, .. }
            | Self::Roll { timeline_id, .. }
            | Self::Slip { timeline_id, .. }
            | Self::Slide { timeline_id, .. }
            | Self::Razor { timeline_id, .. }
            | Self::Trim { timeline_id, .. }
            | Self::Extend { timeline_id, .. }
            | Self::SetTransition { timeline_id, .. }
            | Self::ThreePoint { timeline_id, .. }
            | Self::FourPoint { timeline_id, .. }
            | Self::Retime { timeline_id, .. } => *timeline_id,
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
            Self::LinkAudioVideo { audio_track_id, .. }
            | Self::SynchronizeAudioVideo { audio_track_id, .. }
            | Self::DetachAudio { audio_track_id, .. } => *audio_track_id,
            Self::Ripple { track_id, .. }
            | Self::Roll { track_id, .. }
            | Self::Slip { track_id, .. }
            | Self::Slide { track_id, .. }
            | Self::Razor { track_id, .. }
            | Self::Trim { track_id, .. }
            | Self::Extend { track_id, .. }
            | Self::SetTransition { track_id, .. }
            | Self::ThreePoint { track_id, .. }
            | Self::FourPoint { track_id, .. }
            | Self::Retime { track_id, .. } => *track_id,
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
    synchronized_tracks: Vec<TrackId>,
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

    /// Returns sync-locked companion tracks changed by this operation.
    #[must_use]
    pub fn synchronized_tracks(&self) -> &[TrackId] {
        &self.synchronized_tracks
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

pub(crate) fn apply_operation(
    draft: &mut ProjectDraft,
    operation: &EditOperation,
) -> Result<EditOutcome> {
    let timeline_id = operation.timeline_id();
    let track_id = operation.track_id();
    let timeline = draft.timeline_mut(timeline_id)?;
    require_unlocked_track(timeline, track_id)?;
    match operation {
        EditOperation::LinkAudioVideo {
            video_track_id,
            video_clip_id,
            audio_track_id,
            audio_clip_id,
            ..
        } => {
            return apply_link_audio_video(
                timeline,
                timeline_id,
                *video_track_id,
                *video_clip_id,
                *audio_track_id,
                *audio_clip_id,
            );
        }
        EditOperation::SynchronizeAudioVideo {
            video_track_id,
            video_clip_id,
            audio_track_id,
            audio_clip_id,
            ..
        } => {
            return apply_synchronize_audio_video(
                timeline,
                timeline_id,
                *video_track_id,
                *video_clip_id,
                *audio_track_id,
                *audio_clip_id,
            );
        }
        EditOperation::DetachAudio {
            video_track_id,
            video_clip_id,
            audio_track_id,
            audio_clip_id,
            ..
        } => {
            return apply_detach_audio(
                timeline,
                timeline_id,
                *video_track_id,
                *video_clip_id,
                *audio_track_id,
                *audio_clip_id,
            );
        }
        _ => {}
    }
    if let EditOperation::Ripple {
        target_id,
        side,
        to,
        sync_adjustments,
        ..
    } = operation
    {
        let outcome = apply_synchronized_ripple(
            timeline,
            timeline_id,
            track_id,
            *target_id,
            *side,
            *to,
            sync_adjustments,
        )?;
        inherit_fragment_intent(timeline, outcome.fragments())?;
        return Ok(outcome);
    }
    if let EditOperation::Extend {
        target_id,
        side,
        to,
        mode: ExtendMode::Ripple,
        sync_adjustments,
        ..
    } = operation
    {
        let mut outcome = apply_synchronized_ripple(
            timeline,
            timeline_id,
            track_id,
            *target_id,
            *side,
            *to,
            sync_adjustments,
        )?;
        outcome.kind = EditKind::Extend;
        inherit_fragment_intent(timeline, outcome.fragments())?;
        return Ok(outcome);
    }
    let outcome = {
        let track = timeline.track_mut(track_id)?;
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
            EditOperation::LinkAudioVideo { .. }
            | EditOperation::SynchronizeAudioVideo { .. }
            | EditOperation::DetachAudio { .. } => {
                unreachable!("audio-video operations are handled before track borrow")
            }
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
            EditOperation::Ripple { .. } => unreachable!("ripple handled before track borrow"),
            EditOperation::Roll {
                left_id,
                right_id,
                to,
                ..
            } => apply_roll(track, timeline_id, track_id, *left_id, *right_id, *to),
            EditOperation::Slip {
                clip_id,
                source_start,
                ..
            } => apply_slip(track, timeline_id, track_id, *clip_id, *source_start),
            EditOperation::Retime {
                clip_id, time_map, ..
            } => apply_retime(track, timeline_id, track_id, *clip_id, time_map),
            EditOperation::Slide { clip_id, to, .. } => {
                apply_slide(track, timeline_id, track_id, *clip_id, *to)
            }
            EditOperation::Razor {
                target_id,
                at,
                fragment_id,
                ..
            } => apply_razor(track, timeline_id, track_id, *target_id, *at, *fragment_id),
            EditOperation::Trim {
                target_id,
                side,
                to,
                gap_id,
                ..
            } => apply_trim(
                track,
                timeline_id,
                track_id,
                *target_id,
                *side,
                *to,
                *gap_id,
            ),
            EditOperation::Extend {
                target_id,
                side,
                to,
                mode,
                ..
            } => apply_extend(track, timeline_id, track_id, *target_id, *side, *to, *mode),
            EditOperation::SetTransition {
                transition_id,
                from_offset,
                to_offset,
                ..
            } => apply_set_transition(
                track,
                timeline_id,
                track_id,
                *transition_id,
                *from_offset,
                *to_offset,
            ),
            EditOperation::ThreePoint {
                clip,
                placement,
                fragment_ids,
                ..
            } => apply_three_point(track, timeline_id, track_id, clip, *placement, fragment_ids),
            EditOperation::FourPoint {
                clip,
                source_range,
                record_range,
                fragment_ids,
                ..
            } => apply_four_point(
                track,
                timeline_id,
                track_id,
                clip,
                *source_range,
                *record_range,
                fragment_ids,
            ),
        }
    }?;
    inherit_fragment_intent(timeline, outcome.fragments())?;
    inherit_replacement_intent(timeline, &outcome)?;
    Ok(outcome)
}

fn inherit_fragment_intent(timeline: &mut Timeline, fragments: &[EditFragment]) -> Result<()> {
    for fragment in fragments {
        if let (EditorialObjectId::Caption(original), EditorialObjectId::Caption(created)) =
            (fragment.original(), fragment.created())
        {
            if let Some(metadata) = timeline
                .metadata(MetadataOwner::Object(EditorialObjectId::Caption(original)))
                .cloned()
            {
                timeline.set_metadata(
                    MetadataOwner::Object(EditorialObjectId::Caption(created)),
                    metadata,
                )?;
            }
            continue;
        }
        let (EditorialObjectId::Clip(original), EditorialObjectId::Clip(created)) =
            (fragment.original(), fragment.created())
        else {
            continue;
        };
        let selected = timeline
            .edit_state()
            .selected_objects()
            .any(|id| id == EditorialObjectId::Clip(original));
        let linked: Option<Vec<_>> = timeline
            .edit_state()
            .link_for(original)
            .map(|relation| relation.members().collect());
        let grouped: Option<Vec<_>> = timeline
            .edit_state()
            .group_for(original)
            .map(|relation| relation.members().collect());
        if selected {
            timeline.update_selection(
                [EditorialObjectId::Clip(created)],
                SelectionUpdate::Add,
                SelectionExpansion::Direct,
            )?;
        }
        if let Some(mut members) = linked {
            members.push(created);
            timeline.link_clips(members)?;
        }
        if let Some(mut members) = grouped {
            members.push(created);
            timeline.group_clips(members)?;
        }
        timeline.inherit_multicam_fragment(original, created);
    }
    Ok(())
}

fn inherit_replacement_intent(timeline: &mut Timeline, outcome: &EditOutcome) -> Result<()> {
    if outcome.kind() != EditKind::Replace {
        return Ok(());
    }
    let ([EditorialObjectId::Clip(removed)], [EditorialObjectId::Clip(inserted)]) =
        (outcome.removed_ids(), outcome.inserted_ids())
    else {
        return Ok(());
    };
    timeline.transfer_clip_intent(*removed, *inserted)?;
    timeline.transfer_multicam_clip(*removed, *inserted);
    Ok(())
}

fn checked_audio_video_clips(
    timeline: &Timeline,
    operation: &'static str,
    video_track_id: TrackId,
    video_clip_id: ClipId,
    audio_track_id: TrackId,
    audio_clip_id: ClipId,
) -> Result<(Clip, Clip)> {
    require_unlocked_track(timeline, video_track_id)?;
    let video_track = timeline.track(video_track_id).ok_or_else(|| {
        not_found_edit(operation, "the video track was not found on the timeline")
    })?;
    if video_track.kind() != TrackKind::Video {
        return Err(invalid_edit(
            operation,
            "the video side must name a video track",
        ));
    }
    let audio_track = timeline.track(audio_track_id).ok_or_else(|| {
        not_found_edit(operation, "the audio track was not found on the timeline")
    })?;
    if audio_track.kind() != TrackKind::Audio {
        return Err(invalid_edit(
            operation,
            "the audio side must name an audio track",
        ));
    }
    let video = video_track
        .item(EditorialObjectId::Clip(video_clip_id))
        .and_then(TrackItem::as_clip)
        .cloned()
        .ok_or_else(|| {
            not_found_edit(
                operation,
                "the video clip was not found on the named video track",
            )
        })?;
    let audio = audio_track
        .item(EditorialObjectId::Clip(audio_clip_id))
        .and_then(TrackItem::as_clip)
        .cloned()
        .ok_or_else(|| {
            not_found_edit(
                operation,
                "the audio clip was not found on the named audio track",
            )
        })?;
    Ok((video, audio))
}

fn apply_link_audio_video(
    timeline: &mut Timeline,
    timeline_id: TimelineId,
    video_track_id: TrackId,
    video_clip_id: ClipId,
    audio_track_id: TrackId,
    audio_clip_id: ClipId,
) -> Result<EditOutcome> {
    let (_, audio) = checked_audio_video_clips(
        timeline,
        "link_audio_video",
        video_track_id,
        video_clip_id,
        audio_track_id,
        audio_clip_id,
    )?;
    if timeline
        .edit_state()
        .link_for(audio_clip_id)
        .is_some_and(|relation| relation.contains(video_clip_id))
    {
        return Err(invalid_edit(
            "link_audio_video",
            "the audio and video clips are already linked",
        ));
    }
    timeline.link_clips([video_clip_id, audio_clip_id])?;
    Ok(audio_video_outcome(
        EditKind::LinkAudioVideo,
        timeline_id,
        video_track_id,
        video_clip_id,
        audio_track_id,
        audio_clip_id,
        audio.record_range(),
    ))
}

fn apply_synchronize_audio_video(
    timeline: &mut Timeline,
    timeline_id: TimelineId,
    video_track_id: TrackId,
    video_clip_id: ClipId,
    audio_track_id: TrackId,
    audio_clip_id: ClipId,
) -> Result<EditOutcome> {
    let (video, audio) = checked_audio_video_clips(
        timeline,
        "synchronize_audio_video",
        video_track_id,
        video_clip_id,
        audio_track_id,
        audio_clip_id,
    )?;
    let source_timebase = audio.source_range().timebase();
    let video_source_start = video
        .source_range()
        .start()
        .checked_rescale(source_timebase, TimeRounding::Exact)?;
    let record_offset = audio.record_range().start().checked_sub_at(
        video.record_range().start(),
        source_timebase,
        TimeRounding::Exact,
    )?;
    let source_start =
        video_source_start.checked_add_at(record_offset, source_timebase, TimeRounding::Exact)?;
    let source_range = TimeRange::new(source_start, audio.source_range().duration())?;
    let already_linked = timeline
        .edit_state()
        .link_for(audio_clip_id)
        .is_some_and(|relation| relation.contains(video_clip_id));
    if source_range == audio.source_range() && already_linked {
        return Err(invalid_edit(
            "synchronize_audio_video",
            "the audio clip is already synchronized and linked to the video clip",
        ));
    }
    if source_range != audio.source_range() {
        timeline
            .track_mut(audio_track_id)?
            .item_mut(EditorialObjectId::Clip(audio_clip_id))?
            .as_clip_mut()
            .expect("checked audio clip remains a clip")
            .set_source_range(source_range)?;
    }
    if !already_linked {
        timeline.link_clips([video_clip_id, audio_clip_id])?;
    }
    Ok(audio_video_outcome(
        EditKind::SynchronizeAudioVideo,
        timeline_id,
        video_track_id,
        video_clip_id,
        audio_track_id,
        audio_clip_id,
        audio.record_range(),
    ))
}

fn apply_detach_audio(
    timeline: &mut Timeline,
    timeline_id: TimelineId,
    video_track_id: TrackId,
    video_clip_id: ClipId,
    audio_track_id: TrackId,
    audio_clip_id: ClipId,
) -> Result<EditOutcome> {
    let (_, audio) = checked_audio_video_clips(
        timeline,
        "detach_audio",
        video_track_id,
        video_clip_id,
        audio_track_id,
        audio_clip_id,
    )?;
    if !timeline
        .edit_state()
        .link_for(audio_clip_id)
        .is_some_and(|relation| relation.contains(video_clip_id))
    {
        return Err(invalid_edit(
            "detach_audio",
            "the audio clip is not linked to the named video clip",
        ));
    }
    timeline.unlink_clips([audio_clip_id])?;
    Ok(audio_video_outcome(
        EditKind::DetachAudio,
        timeline_id,
        video_track_id,
        video_clip_id,
        audio_track_id,
        audio_clip_id,
        audio.record_range(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn audio_video_outcome(
    kind: EditKind,
    timeline_id: TimelineId,
    video_track_id: TrackId,
    video_clip_id: ClipId,
    audio_track_id: TrackId,
    audio_clip_id: ClipId,
    affected_range: TimeRange,
) -> EditOutcome {
    EditOutcome {
        kind,
        timeline_id,
        track_id: audio_track_id,
        affected_range,
        duration_change: TrackDurationChange::Unchanged,
        inserted_ids: Vec::new(),
        removed_ids: Vec::new(),
        modified_ids: vec![
            EditorialObjectId::Clip(audio_clip_id),
            EditorialObjectId::Clip(video_clip_id),
        ],
        fragments: Vec::new(),
        removed_transitions: Vec::new(),
        synchronized_tracks: vec![video_track_id],
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_synchronized_ripple(
    timeline: &mut Timeline,
    timeline_id: TimelineId,
    track_id: TrackId,
    target_id: EditorialObjectId,
    side: EditSide,
    to: RationalTime,
    sync_adjustments: &[RippleSyncAdjustment],
) -> Result<EditOutcome> {
    for adjustment in sync_adjustments {
        require_unlocked_track(timeline, adjustment.track_id())?;
    }
    let expected: Vec<_> = timeline
        .tracks_affected_by_sync([track_id])?
        .into_iter()
        .filter(|candidate| *candidate != track_id)
        .collect();
    let provided: Vec<_> = sync_adjustments
        .iter()
        .map(RippleSyncAdjustment::track_id)
        .collect();
    if provided != expected {
        return Err(invalid_edit(
            "ripple",
            "ripple sync adjustments must name every other sync-locked track exactly once in timeline order",
        ));
    }
    let primary_range = timeline
        .track(track_id)
        .and_then(|track| track.item(target_id))
        .and_then(TrackItem::record_range)
        .ok_or_else(|| {
            not_found_edit("ripple", "ripple target was not found on the target track")
        })?;
    let pivot = primary_range.end_exclusive()?;
    let mut outcome = {
        let track = timeline.track_mut(track_id)?;
        apply_ripple(track, timeline_id, track_id, target_id, side, to)?
    };
    let (magnitude, extending) = match outcome.duration_change() {
        TrackDurationChange::Extended(duration) => (duration, true),
        TrackDurationChange::Shortened(duration) => (duration, false),
        TrackDurationChange::Unchanged => {
            return Err(invalid_edit(
                "ripple",
                "ripple must change the target-track duration",
            ));
        }
    };
    let mut companion_fragments = Vec::new();
    for adjustment in sync_adjustments {
        let companion = timeline.track_mut(adjustment.track_id)?;
        let timebase = companion.semantics().timebase();
        let companion_pivot = pivot.checked_rescale(timebase, TimeRounding::Exact)?;
        let companion_duration = exact_duration_at(magnitude, timebase, "ripple")?;
        let companion_outcome = if extending {
            apply_insert(
                companion,
                timeline_id,
                adjustment.track_id,
                companion_pivot,
                &TrackItem::Gap(Gap::new(
                    adjustment.gap_id,
                    "sync ripple gap",
                    TimeRange::new(companion_pivot, companion_duration)?,
                )),
                &adjustment.fragment_ids,
            )?
        } else {
            let start = companion_pivot.checked_sub_at(
                companion_duration.rational_time(),
                timebase,
                TimeRounding::Exact,
            )?;
            apply_extract(
                companion,
                timeline_id,
                adjustment.track_id,
                TimeRange::new(start, companion_duration)?,
                &adjustment.fragment_ids,
            )?
        };
        companion_fragments.extend_from_slice(companion_outcome.fragments());
    }
    inherit_fragment_intent(timeline, &companion_fragments)?;
    outcome.synchronized_tracks = expected;
    Ok(outcome)
}

fn apply_ripple(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    target_id: EditorialObjectId,
    side: EditSide,
    to: RationalTime,
) -> Result<EditOutcome> {
    if matches!(target_id, EditorialObjectId::Transition(_)) {
        return Err(invalid_edit(
            "ripple",
            "ripple targets must be timed editorial objects",
        ));
    }
    let timebase = track.semantics().timebase();
    validate_point_clock(to, timebase, "ripple")?;
    let (mut timed, transitions) = separate_items(track.items());
    let target_index = timed
        .iter()
        .position(|item| item.id() == target_id)
        .ok_or_else(|| {
            not_found_edit("ripple", "ripple target was not found on the target track")
        })?;
    let old_range = timed[target_index]
        .record_range()
        .expect("separated item is timed");
    let old_end = old_range.end_exclusive()?;
    let (new_range, delta) = match side {
        EditSide::Start => {
            if to >= old_end {
                return Err(invalid_edit(
                    "ripple",
                    "ripple start must remain before the target end",
                ));
            }
            let removed = to.checked_sub_at(old_range.start(), timebase, TimeRounding::Exact)?;
            let new_end = old_end.checked_sub_at(removed, timebase, TimeRounding::Exact)?;
            let range = TimeRange::from_start_end(old_range.start(), new_end)?;
            let delta = new_end.checked_sub_at(old_end, timebase, TimeRounding::Exact)?;
            (range, delta)
        }
        EditSide::End => {
            if to <= old_range.start() {
                return Err(invalid_edit(
                    "ripple",
                    "ripple end must remain after the target start",
                ));
            }
            let range = TimeRange::from_start_end(old_range.start(), to)?;
            let delta = to.checked_sub_at(old_end, timebase, TimeRounding::Exact)?;
            (range, delta)
        }
    };
    if delta.is_zero() {
        return Err(invalid_edit("ripple", "ripple edit must move its edge"));
    }

    timed[target_index] = trim_item(&timed[target_index], new_range, side, "ripple")?;
    let mut modified_ids = vec![target_id];
    for item in timed.iter_mut().skip(target_index + 1) {
        *item = shift_item_by(item, delta, timebase, "ripple")?;
        push_unique(&mut modified_ids, item.id());
    }
    let (items, removed_transitions) = rebuild_with_transitions(timed, transitions)?;
    track.replace_items(items);

    let magnitude = Duration::new(delta.value().unsigned_abs(), timebase)?;
    let duration_change = if delta.is_negative() {
        TrackDurationChange::Shortened(magnitude)
    } else {
        TrackDurationChange::Extended(magnitude)
    };
    let affected_start = if to
        < match side {
            EditSide::Start => old_range.start(),
            EditSide::End => old_end,
        } {
        to
    } else {
        match side {
            EditSide::Start => old_range.start(),
            EditSide::End => old_end,
        }
    };

    Ok(EditOutcome {
        kind: EditKind::Ripple,
        timeline_id,
        track_id,
        affected_range: TimeRange::new(affected_start, magnitude)?,
        duration_change,
        inserted_ids: Vec::new(),
        removed_ids: Vec::new(),
        modified_ids,
        fragments: Vec::new(),
        removed_transitions,
        synchronized_tracks: Vec::new(),
    })
}

fn apply_roll(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    left_id: EditorialObjectId,
    right_id: EditorialObjectId,
    to: RationalTime,
) -> Result<EditOutcome> {
    let timebase = track.semantics().timebase();
    validate_point_clock(to, timebase, "roll")?;
    let (mut timed, transitions) = separate_items(track.items());
    let left_index = timed
        .iter()
        .position(|item| item.id() == left_id)
        .ok_or_else(|| not_found_edit("roll", "left roll target was not found"))?;
    let right_index = timed
        .iter()
        .position(|item| item.id() == right_id)
        .ok_or_else(|| not_found_edit("roll", "right roll target was not found"))?;
    if right_index != left_index + 1 {
        return Err(invalid_edit(
            "roll",
            "roll targets must be adjacent timed objects in record order",
        ));
    }
    let left_range = timed[left_index].record_range().expect("timed roll target");
    let right_range = timed[right_index]
        .record_range()
        .expect("timed roll target");
    if left_range.end_exclusive()? != right_range.start() {
        return Err(invalid_edit(
            "roll",
            "roll targets must share one exact edit point",
        ));
    }
    if to <= left_range.start() || to >= right_range.end_exclusive()? {
        return Err(invalid_edit(
            "roll",
            "roll point must remain inside the combined two-item span",
        ));
    }
    if to == right_range.start() {
        return Err(invalid_edit("roll", "roll edit must move its cut"));
    }
    let combined_range =
        TimeRange::from_start_end(left_range.start(), right_range.end_exclusive()?)?;
    timed[left_index] = trim_item(
        &timed[left_index],
        TimeRange::from_start_end(left_range.start(), to)?,
        EditSide::End,
        "roll",
    )?;
    timed[right_index] = trim_item(
        &timed[right_index],
        TimeRange::from_start_end(to, right_range.end_exclusive()?)?,
        EditSide::Start,
        "roll",
    )?;
    let (items, removed_transitions) = rebuild_with_transitions(timed, transitions)?;
    track.replace_items(items);

    Ok(EditOutcome {
        kind: EditKind::Roll,
        timeline_id,
        track_id,
        affected_range: combined_range,
        duration_change: TrackDurationChange::Unchanged,
        inserted_ids: Vec::new(),
        removed_ids: Vec::new(),
        modified_ids: vec![left_id, right_id],
        fragments: Vec::new(),
        removed_transitions,
        synchronized_tracks: Vec::new(),
    })
}

fn apply_slip(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    clip_id: ClipId,
    source_start: RationalTime,
) -> Result<EditOutcome> {
    let object_id = EditorialObjectId::Clip(clip_id);
    let item = track.item_mut(object_id)?;
    let clip = item
        .as_clip_mut()
        .ok_or_else(|| invalid_edit("slip", "slip targets must be source-bearing clips"))?;
    let old_source = clip.source_range();
    if source_start.timebase() != old_source.timebase() {
        return Err(invalid_edit(
            "slip",
            "slip source point must use the exact clip source timebase",
        ));
    }
    if source_start == old_source.start() {
        return Err(invalid_edit("slip", "slip edit must move its source range"));
    }
    clip.set_source_range(TimeRange::new(source_start, old_source.duration())?)?;
    let record_range = clip.record_range();
    Ok(EditOutcome {
        kind: EditKind::Slip,
        timeline_id,
        track_id,
        affected_range: record_range,
        duration_change: TrackDurationChange::Unchanged,
        inserted_ids: Vec::new(),
        removed_ids: Vec::new(),
        modified_ids: vec![object_id],
        fragments: Vec::new(),
        removed_transitions: Vec::new(),
        synchronized_tracks: Vec::new(),
    })
}

fn apply_retime(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    clip_id: ClipId,
    time_map: &ClipTimeMap,
) -> Result<EditOutcome> {
    let object_id = EditorialObjectId::Clip(clip_id);
    let item = track.item_mut(object_id)?;
    let clip = item
        .as_clip_mut()
        .ok_or_else(|| invalid_edit("retime", "retime targets must be source-bearing clips"))?;
    if clip.time_map() == time_map {
        return Err(invalid_edit(
            "retime",
            "retime edit must change the clip time map",
        ));
    }
    clip.set_time_map(time_map.clone())?;
    let record_range = clip.record_range();
    Ok(EditOutcome {
        kind: EditKind::Retime,
        timeline_id,
        track_id,
        affected_range: record_range,
        duration_change: TrackDurationChange::Unchanged,
        inserted_ids: Vec::new(),
        removed_ids: Vec::new(),
        modified_ids: vec![object_id],
        fragments: Vec::new(),
        removed_transitions: Vec::new(),
        synchronized_tracks: Vec::new(),
    })
}

fn apply_slide(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    clip_id: ClipId,
    to: RationalTime,
) -> Result<EditOutcome> {
    let timebase = track.semantics().timebase();
    validate_point_clock(to, timebase, "slide")?;
    let (mut timed, transitions) = separate_items(track.items());
    let center_id = EditorialObjectId::Clip(clip_id);
    let center_index = timed
        .iter()
        .position(|item| item.id() == center_id)
        .ok_or_else(|| not_found_edit("slide", "slide clip was not found on the target track"))?;
    if center_index == 0 || center_index + 1 >= timed.len() {
        return Err(invalid_edit(
            "slide",
            "slide requires one adjacent clip on each side",
        ));
    }
    if timed[center_index - 1].as_clip().is_none()
        || timed[center_index].as_clip().is_none()
        || timed[center_index + 1].as_clip().is_none()
    {
        return Err(invalid_edit(
            "slide",
            "slide requires three adjacent source-bearing clips",
        ));
    }
    let left_range = timed[center_index - 1]
        .record_range()
        .expect("timed slide neighbor");
    let center_range = timed[center_index]
        .record_range()
        .expect("timed slide center");
    let right_range = timed[center_index + 1]
        .record_range()
        .expect("timed slide neighbor");
    let new_center_end = to.checked_add_at(
        center_range.duration().rational_time(),
        timebase,
        TimeRounding::Exact,
    )?;
    if to <= left_range.start() || new_center_end >= right_range.end_exclusive()? {
        return Err(invalid_edit(
            "slide",
            "slide must leave both adjacent clips with nonzero duration",
        ));
    }
    if to == center_range.start() {
        return Err(invalid_edit(
            "slide",
            "slide edit must move its center clip",
        ));
    }
    timed[center_index - 1] = trim_item(
        &timed[center_index - 1],
        TimeRange::from_start_end(left_range.start(), to)?,
        EditSide::End,
        "slide",
    )?;
    timed[center_index] = reposition_item(
        &timed[center_index],
        TimeRange::new(to, center_range.duration())?,
        "slide",
    )?;
    timed[center_index + 1] = trim_item(
        &timed[center_index + 1],
        TimeRange::from_start_end(new_center_end, right_range.end_exclusive()?)?,
        EditSide::Start,
        "slide",
    )?;
    let left_id = timed[center_index - 1].id();
    let right_id = timed[center_index + 1].id();
    let affected_range =
        TimeRange::from_start_end(left_range.start(), right_range.end_exclusive()?)?;
    let (items, removed_transitions) = rebuild_with_transitions(timed, transitions)?;
    track.replace_items(items);

    Ok(EditOutcome {
        kind: EditKind::Slide,
        timeline_id,
        track_id,
        affected_range,
        duration_change: TrackDurationChange::Unchanged,
        inserted_ids: Vec::new(),
        removed_ids: Vec::new(),
        modified_ids: vec![left_id, center_id, right_id],
        fragments: Vec::new(),
        removed_transitions,
        synchronized_tracks: Vec::new(),
    })
}

fn apply_razor(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    target_id: EditorialObjectId,
    at: RationalTime,
    fragment_id: EditorialObjectId,
) -> Result<EditOutcome> {
    if matches!(target_id, EditorialObjectId::Transition(_)) {
        return Err(invalid_edit(
            "razor",
            "transitions cannot be independently razored",
        ));
    }
    if !same_typed_domain(target_id, fragment_id) {
        return Err(invalid_edit(
            "razor",
            "razor fragment identity must have the same typed domain as its original object",
        ));
    }
    let timebase = track.semantics().timebase();
    validate_point_clock(at, timebase, "razor")?;
    let (mut timed, transitions) = separate_items(track.items());
    let index = timed
        .iter()
        .position(|item| item.id() == target_id)
        .ok_or_else(|| not_found_edit("razor", "razor target was not found on the target track"))?;
    let range = timed[index].record_range().expect("timed razor target");
    if at <= range.start() || at >= range.end_exclusive()? {
        return Err(invalid_edit(
            "razor",
            "razor point must lie strictly inside the target record range",
        ));
    }
    let left = slice_item(&timed[index], range.start(), at, target_id, "razor")?;
    let right = slice_item(
        &timed[index],
        at,
        range.end_exclusive()?,
        fragment_id,
        "razor",
    )?;
    timed.splice(index..=index, [left, right]);
    let (items, removed_transitions) = rebuild_with_transitions(timed, transitions)?;
    track.replace_items(items);

    Ok(EditOutcome {
        kind: EditKind::Razor,
        timeline_id,
        track_id,
        affected_range: range,
        duration_change: TrackDurationChange::Unchanged,
        inserted_ids: vec![fragment_id],
        removed_ids: Vec::new(),
        modified_ids: vec![target_id, fragment_id],
        fragments: vec![EditFragment {
            original: target_id,
            created: fragment_id,
        }],
        removed_transitions,
        synchronized_tracks: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_trim(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    target_id: EditorialObjectId,
    side: EditSide,
    to: RationalTime,
    gap_id: Option<GapId>,
) -> Result<EditOutcome> {
    if matches!(target_id, EditorialObjectId::Transition(_)) {
        return Err(invalid_edit(
            "trim",
            "trim targets must be timed editorial objects",
        ));
    }
    let timebase = track.semantics().timebase();
    validate_point_clock(to, timebase, "trim")?;
    let (mut timed, transitions) = separate_items(track.items());
    let target_index = timed
        .iter()
        .position(|item| item.id() == target_id)
        .ok_or_else(|| not_found_edit("trim", "trim target was not found on the target track"))?;
    let old_range = timed[target_index]
        .record_range()
        .expect("timed trim target");
    let old_boundary = match side {
        EditSide::Start => old_range.start(),
        EditSide::End => old_range.end_exclusive()?,
    };
    if to == old_boundary {
        return Err(invalid_edit("trim", "trim edit must move its edge"));
    }
    let new_range = match side {
        EditSide::Start if to < old_range.end_exclusive()? => {
            TimeRange::from_start_end(to, old_range.end_exclusive()?)?
        }
        EditSide::End if to > old_range.start() => {
            TimeRange::from_start_end(old_range.start(), to)?
        }
        _ => {
            return Err(invalid_edit(
                "trim",
                "trimmed object must retain a nonzero record duration",
            ));
        }
    };

    let mut inserted_ids = Vec::new();
    let mut removed_ids = Vec::new();
    let mut modified_ids = vec![target_id];
    match side {
        EditSide::End if to < old_boundary => {
            timed[target_index] = trim_item(&timed[target_index], new_range, side, "trim")?;
            let gap_range = TimeRange::from_start_end(to, old_boundary)?;
            if target_index + 1 < timed.len()
                && matches!(timed[target_index + 1], TrackItem::Gap(_))
            {
                let next_range = timed[target_index + 1].record_range().expect("timed gap");
                let next_end = next_range.end_exclusive()?;
                set_record_range(
                    &mut timed[target_index + 1],
                    TimeRange::from_start_end(to, next_end)?,
                    "trim",
                )?;
                push_unique(&mut modified_ids, timed[target_index + 1].id());
            } else {
                let id = gap_id.ok_or_else(|| {
                    invalid_edit(
                        "trim",
                        "an inward trim without an adjacent gap requires a caller-supplied gap identity",
                    )
                })?;
                let gap = TrackItem::Gap(Gap::new(id, "trim gap", gap_range));
                inserted_ids.push(gap.id());
                timed.insert(target_index + 1, gap);
            }
        }
        EditSide::End => {
            let next_index = target_index + 1;
            let next = timed
                .get(next_index)
                .ok_or_else(|| invalid_edit("trim", "outward end trim requires an adjacent gap"))?;
            if !matches!(next, TrackItem::Gap(_)) {
                return Err(invalid_edit(
                    "trim",
                    "outward end trim may consume only an adjacent gap",
                ));
            }
            let gap_range = next.record_range().expect("timed gap");
            let gap_end = gap_range.end_exclusive()?;
            if to > gap_end {
                return Err(invalid_edit(
                    "trim",
                    "outward end trim exceeds the adjacent gap",
                ));
            }
            timed[target_index] = trim_item(&timed[target_index], new_range, side, "trim")?;
            if to == gap_end {
                removed_ids.push(timed.remove(next_index).id());
            } else {
                set_record_range(
                    &mut timed[next_index],
                    TimeRange::from_start_end(to, gap_end)?,
                    "trim",
                )?;
                push_unique(&mut modified_ids, timed[next_index].id());
            }
        }
        EditSide::Start if to > old_boundary => {
            timed[target_index] = trim_item(&timed[target_index], new_range, side, "trim")?;
            if target_index > 0 && matches!(timed[target_index - 1], TrackItem::Gap(_)) {
                let previous_range = timed[target_index - 1].record_range().expect("timed gap");
                set_record_range(
                    &mut timed[target_index - 1],
                    TimeRange::from_start_end(previous_range.start(), to)?,
                    "trim",
                )?;
                push_unique(&mut modified_ids, timed[target_index - 1].id());
            } else {
                let id = gap_id.ok_or_else(|| {
                    invalid_edit(
                        "trim",
                        "an inward trim without an adjacent gap requires a caller-supplied gap identity",
                    )
                })?;
                let gap = TrackItem::Gap(Gap::new(
                    id,
                    "trim gap",
                    TimeRange::from_start_end(old_boundary, to)?,
                ));
                inserted_ids.push(gap.id());
                timed.insert(target_index, gap);
            }
        }
        EditSide::Start => {
            if target_index == 0 || !matches!(timed[target_index - 1], TrackItem::Gap(_)) {
                return Err(invalid_edit(
                    "trim",
                    "outward start trim requires an adjacent gap",
                ));
            }
            let previous_index = target_index - 1;
            let gap_range = timed[previous_index].record_range().expect("timed gap");
            if to < gap_range.start() {
                return Err(invalid_edit(
                    "trim",
                    "outward start trim exceeds the adjacent gap",
                ));
            }
            timed[target_index] = trim_item(&timed[target_index], new_range, side, "trim")?;
            if to == gap_range.start() {
                removed_ids.push(timed.remove(previous_index).id());
            } else {
                set_record_range(
                    &mut timed[previous_index],
                    TimeRange::from_start_end(gap_range.start(), to)?,
                    "trim",
                )?;
                push_unique(&mut modified_ids, timed[previous_index].id());
            }
        }
    }

    let magnitude = Duration::new(
        to.checked_sub_at(old_boundary, timebase, TimeRounding::Exact)?
            .value()
            .unsigned_abs(),
        timebase,
    )?;
    let affected_start = if to < old_boundary { to } else { old_boundary };
    let (items, removed_transitions) = rebuild_with_transitions(timed, transitions)?;
    track.replace_items(items);
    Ok(EditOutcome {
        kind: EditKind::Trim,
        timeline_id,
        track_id,
        affected_range: TimeRange::new(affected_start, magnitude)?,
        duration_change: TrackDurationChange::Unchanged,
        inserted_ids,
        removed_ids,
        modified_ids,
        fragments: Vec::new(),
        removed_transitions,
        synchronized_tracks: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_extend(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    target_id: EditorialObjectId,
    side: EditSide,
    to: RationalTime,
    mode: ExtendMode,
) -> Result<EditOutcome> {
    let mut outcome = match mode {
        ExtendMode::Ripple => apply_ripple(track, timeline_id, track_id, target_id, side, to)?,
        ExtendMode::Roll => {
            let (timed, _) = separate_items(track.items());
            let index = timed
                .iter()
                .position(|item| item.id() == target_id)
                .ok_or_else(|| not_found_edit("extend", "extend target was not found"))?;
            let (left_id, right_id) = match side {
                EditSide::End if index + 1 < timed.len() => (target_id, timed[index + 1].id()),
                EditSide::Start if index > 0 => (timed[index - 1].id(), target_id),
                _ => {
                    return Err(invalid_edit(
                        "extend",
                        "roll extend requires an adjacent timed object on the moved edge",
                    ));
                }
            };
            apply_roll(track, timeline_id, track_id, left_id, right_id, to)?
        }
    };
    outcome.kind = EditKind::Extend;
    Ok(outcome)
}

fn apply_set_transition(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    transition_id: TransitionId,
    from_offset: Duration,
    to_offset: Duration,
) -> Result<EditOutcome> {
    let timebase = track.semantics().timebase();
    if from_offset.timebase() != timebase || to_offset.timebase() != timebase {
        return Err(invalid_edit(
            "set_transition",
            "transition handles must use the exact target track clock",
        ));
    }
    if from_offset.is_zero() && to_offset.is_zero() {
        return Err(invalid_edit(
            "set_transition",
            "transition handle duration must be nonzero",
        ));
    }

    let object_id = EditorialObjectId::Transition(transition_id);
    let index = track
        .items()
        .iter()
        .position(|item| item.id() == object_id)
        .ok_or_else(|| not_found_edit("set_transition", "transition was not found"))?;
    let transition = match &track.items()[index] {
        TrackItem::Transition(transition) => transition,
        _ => {
            return Err(invalid_edit(
                "set_transition",
                "transition identity resolved to an unrelated item",
            ));
        }
    };
    if transition.from_offset() == from_offset && transition.to_offset() == to_offset {
        return Err(invalid_edit(
            "set_transition",
            "transition handle edit must change at least one offset",
        ));
    }
    let cut = index
        .checked_sub(1)
        .and_then(|value| track.items().get(value))
        .and_then(TrackItem::record_range)
        .ok_or_else(|| {
            invalid_edit(
                "set_transition",
                "transition must retain a preceding timed endpoint",
            )
        })?
        .end_exclusive()?;
    let old_range = transition_handle_range(cut, transition.from_offset(), transition.to_offset())?;
    let new_range = transition_handle_range(cut, from_offset, to_offset)?;

    let item = track.item_mut(object_id)?;
    let TrackItem::Transition(transition) = item else {
        return Err(invalid_edit(
            "set_transition",
            "transition identity resolved to an unrelated item",
        ));
    };
    transition.set_offsets(from_offset, to_offset);

    let affected_start = if old_range.start() < new_range.start() {
        old_range.start()
    } else {
        new_range.start()
    };
    let old_end = old_range.end_exclusive()?;
    let new_end = new_range.end_exclusive()?;
    let affected_end = if old_end > new_end { old_end } else { new_end };

    Ok(EditOutcome {
        kind: EditKind::SetTransition,
        timeline_id,
        track_id,
        affected_range: TimeRange::from_start_end(affected_start, affected_end)?,
        duration_change: TrackDurationChange::Unchanged,
        inserted_ids: Vec::new(),
        removed_ids: Vec::new(),
        modified_ids: vec![object_id],
        fragments: Vec::new(),
        removed_transitions: Vec::new(),
        synchronized_tracks: Vec::new(),
    })
}

fn transition_handle_range(
    cut: RationalTime,
    from_offset: Duration,
    to_offset: Duration,
) -> Result<TimeRange> {
    let from_value = i64::try_from(from_offset.value()).map_err(|_| {
        invalid_edit(
            "set_transition",
            "transition from handle exceeds the supported exact range",
        )
    })?;
    let to_value = i64::try_from(to_offset.value()).map_err(|_| {
        invalid_edit(
            "set_transition",
            "transition to handle exceeds the supported exact range",
        )
    })?;
    let start = cut.value().checked_sub(from_value).ok_or_else(|| {
        invalid_edit(
            "set_transition",
            "transition from handle exceeds the supported exact range",
        )
    })?;
    let end = cut.value().checked_add(to_value).ok_or_else(|| {
        invalid_edit(
            "set_transition",
            "transition to handle exceeds the supported exact range",
        )
    })?;
    TimeRange::from_start_end(
        RationalTime::new(start, cut.timebase()),
        RationalTime::new(end, cut.timebase()),
    )
}

fn apply_three_point(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    clip: &Clip,
    placement: ThreePointPlacement,
    fragment_ids: &[EditorialObjectId],
) -> Result<EditOutcome> {
    let track_timebase = track.semantics().timebase();
    let (source_range, record_range) = match placement {
        ThreePointPlacement::SourceRangeAtRecordStart {
            source_range,
            record_start,
        } => {
            validate_point_clock(record_start, track_timebase, "three_point")?;
            let duration =
                exact_duration_at(source_range.duration(), track_timebase, "three_point")?;
            (source_range, TimeRange::new(record_start, duration)?)
        }
        ThreePointPlacement::SourceStartOverRecordRange {
            source_start,
            record_range,
        } => {
            validate_record_clock(record_range, track_timebase, "three_point")?;
            let duration = exact_duration_at(
                record_range.duration(),
                source_start.timebase(),
                "three_point",
            )?;
            (TimeRange::new(source_start, duration)?, record_range)
        }
        ThreePointPlacement::SourceRangeBacktimedToRecordEnd {
            source_range,
            record_end,
        } => {
            validate_point_clock(record_end, track_timebase, "three_point")?;
            let duration =
                exact_duration_at(source_range.duration(), track_timebase, "three_point")?;
            let record_start = record_end.checked_sub_at(
                duration.rational_time(),
                track_timebase,
                TimeRounding::Exact,
            )?;
            (source_range, TimeRange::new(record_start, duration)?)
        }
        ThreePointPlacement::SourceEndBacktimedOverRecordRange {
            source_end,
            record_range,
        } => {
            validate_record_clock(record_range, track_timebase, "three_point")?;
            let duration = exact_duration_at(
                record_range.duration(),
                source_end.timebase(),
                "three_point",
            )?;
            let source_start = source_end.checked_sub_at(
                duration.rational_time(),
                source_end.timebase(),
                TimeRounding::Exact,
            )?;
            (TimeRange::new(source_start, duration)?, record_range)
        }
    };
    let mut placed = clip.clone();
    placed.set_ranges(source_range, record_range)?;
    let mut outcome = apply_overwrite(
        track,
        timeline_id,
        track_id,
        record_range.start(),
        &TrackItem::Clip(placed),
        fragment_ids,
    )?;
    outcome.kind = EditKind::ThreePoint;
    Ok(outcome)
}

fn apply_four_point(
    track: &mut Track,
    timeline_id: TimelineId,
    track_id: TrackId,
    clip: &Clip,
    source_range: TimeRange,
    record_range: TimeRange,
    fragment_ids: &[EditorialObjectId],
) -> Result<EditOutcome> {
    validate_record_clock(record_range, track.semantics().timebase(), "four_point")?;
    if source_range.duration().rational_time() != record_range.duration().rational_time() {
        return Err(unsupported_edit(
            "four_point",
            "four-point fit-to-fill requires the P2.W02.C008 time-remapping capability",
        ));
    }
    let mut placed = clip.clone();
    placed.set_ranges(source_range, record_range)?;
    let mut outcome = apply_overwrite(
        track,
        timeline_id,
        track_id,
        record_range.start(),
        &TrackItem::Clip(placed),
        fragment_ids,
    )?;
    outcome.kind = EditKind::FourPoint;
    Ok(outcome)
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
        synchronized_tracks: Vec::new(),
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
        synchronized_tracks: Vec::new(),
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
        synchronized_tracks: Vec::new(),
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
        synchronized_tracks: Vec::new(),
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

fn validate_record_clock(
    range: TimeRange,
    expected: Timebase,
    operation: &'static str,
) -> Result<()> {
    if range.timebase() != expected {
        return Err(invalid_edit(
            operation,
            "record range must use the exact target track timebase",
        ));
    }
    if range.is_empty() {
        return Err(invalid_edit(operation, "record range must be nonempty"));
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
    if let TrackItem::Clip(clip) = item {
        let EditorialObjectId::Clip(id) = id else {
            return Err(invalid_edit(
                operation,
                "fragment identity must have the same typed domain as its original object",
            ));
        };
        return clip.slice_with_id(id, new_range).map(TrackItem::Clip);
    }
    let mut sliced = clone_with_id(item, id, operation)?;
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

fn trim_item(
    item: &TrackItem,
    new_range: TimeRange,
    moved_side: EditSide,
    operation: &'static str,
) -> Result<TrackItem> {
    let old_range = item
        .record_range()
        .ok_or_else(|| invalid_edit(operation, "transitions cannot be trimmed as timed objects"))?;
    let mut trimmed = item.clone();
    if let TrackItem::Clip(clip) = &mut trimmed {
        let old_source = clip.source_range();
        let source_start = match moved_side {
            EditSide::Start => {
                let record_offset = new_range.start().checked_sub_at(
                    old_range.start(),
                    old_range.timebase(),
                    TimeRounding::Exact,
                )?;
                let source_offset =
                    record_offset.checked_rescale(old_source.timebase(), TimeRounding::Exact)?;
                old_source.start().checked_add_at(
                    source_offset,
                    old_source.timebase(),
                    TimeRounding::Exact,
                )?
            }
            EditSide::End => old_source.start(),
        };
        let source_duration =
            exact_duration_at(new_range.duration(), old_source.timebase(), operation)?;
        clip.set_ranges(TimeRange::new(source_start, source_duration)?, new_range)?;
    } else {
        set_record_range(&mut trimmed, new_range, operation)?;
    }
    Ok(trimmed)
}

fn shift_item_by(
    item: &TrackItem,
    delta: RationalTime,
    timebase: Timebase,
    operation: &'static str,
) -> Result<TrackItem> {
    let range = item.record_range().expect("timed edit output");
    let start = range
        .start()
        .checked_add_at(delta, timebase, TimeRounding::Exact)?;
    reposition_item(item, TimeRange::new(start, range.duration())?, operation)
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
        TrackItem::Clip(value) => value.set_record_range(range)?,
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
        (TrackItem::Clip(value), EditorialObjectId::Clip(id)) => {
            TrackItem::Clip(value.clone_with_id(id))
        }
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

fn require_unlocked_track(timeline: &Timeline, track_id: TrackId) -> Result<()> {
    let state = timeline
        .edit_state()
        .track_state(track_id)
        .ok_or_else(|| not_found_edit("edit_locked_track", "editorial track was not found"))?;
    if state.locked() {
        return Err(Error::new(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "locked tracks must be unlocked before authored item changes",
        )
        .with_context(
            ErrorContext::new("superi-timeline.edit_ops", "edit_locked_track")
                .with_field("track", track_id.to_string()),
        ));
    }
    Ok(())
}

fn not_found_edit(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.edit_ops", operation))
}

fn unsupported_edit(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.edit_ops", operation))
}
