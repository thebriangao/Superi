//! Sample-accurate timeline scheduling against an audio device clock.
//!
//! Schedule construction runs away from the real-time callback. It validates
//! immutable clip placements, preserves caller-authored track order, and
//! stores one canonical sequence. [`AudioTimelineScheduler::plan_callback`]
//! then performs only checked scalar arithmetic over borrowed storage. The
//! returned iterator maps a device callback window to exact source sample
//! slices without copying, resampling, routing, mixing, blocking, or
//! allocating.
//!
//! Audio remains the playback master. After a callback has presented its
//! complete window, [`AudioCallbackPlan::publish_presented`] advances the
//! shared [`AudioMasterClock`]. The existing playback-domain A/V scheduler can
//! then pace video against that audible sample position.

use std::collections::{BTreeMap, BTreeSet};

use superi_concurrency::clock::{AudioClockUpdate, AudioMasterClock};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ClipId, TimelineId, TrackId};
use superi_core::time::{SampleTime, Timebase};

const COMPONENT: &str = "superi-audio.sync";

/// One sample-exact source placement in a timeline audio track.
///
/// The placement contains timing and identity only. Channel meaning and
/// routing remain in editorial and graph state, while decoded sample storage
/// remains owned by media I/O and the future audio graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioTimelinePlacement {
    track_id: TrackId,
    clip_id: ClipId,
    track_order: u32,
    record_start: SampleTime,
    source_start: SampleTime,
    frame_count: u64,
}

impl AudioTimelinePlacement {
    /// Creates one nonempty placement on a single integral sample clock.
    pub fn new(
        track_id: TrackId,
        clip_id: ClipId,
        track_order: u32,
        record_start: SampleTime,
        source_start: SampleTime,
        frame_count: u64,
    ) -> Result<Self> {
        if record_start.sample_rate() != source_start.sample_rate() {
            return Err(invalid(
                "create_placement",
                "record and source positions must use one sample rate",
            ));
        }
        if record_start.sample() < 0 {
            return Err(invalid(
                "create_placement",
                "timeline audio placement must not begin before timeline zero",
            ));
        }
        if frame_count == 0 {
            return Err(invalid(
                "create_placement",
                "timeline audio placement must contain at least one sample frame",
            ));
        }
        checked_sample_end(record_start, frame_count, "create_placement")?;
        checked_sample_end(source_start, frame_count, "create_placement")?;

        Ok(Self {
            track_id,
            clip_id,
            track_order,
            record_start,
            source_start,
            frame_count,
        })
    }

    /// Returns the stable editorial track identity.
    #[must_use]
    pub const fn track_id(&self) -> TrackId {
        self.track_id
    }

    /// Returns the stable editorial clip identity.
    #[must_use]
    pub const fn clip_id(&self) -> ClipId {
        self.clip_id
    }

    /// Returns the caller-authored bottom-to-top track order.
    #[must_use]
    pub const fn track_order(&self) -> u32 {
        self.track_order
    }

    /// Returns the first covered timeline sample.
    #[must_use]
    pub const fn record_start(&self) -> SampleTime {
        self.record_start
    }

    /// Returns the first selected source sample.
    #[must_use]
    pub const fn source_start(&self) -> SampleTime {
        self.source_start
    }

    /// Returns the number of inter-channel sample frames.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Returns the exclusive timeline end.
    pub fn record_end(&self) -> Result<SampleTime> {
        checked_sample_end(self.record_start, self.frame_count, "placement_record_end")
    }

    /// Returns the exclusive source end.
    pub fn source_end(&self) -> Result<SampleTime> {
        checked_sample_end(self.source_start, self.frame_count, "placement_source_end")
    }
}

/// One immutable, revision-tagged timeline audio schedule.
#[derive(Debug, Eq, PartialEq)]
pub struct AudioTimelineSchedule {
    timeline_id: TimelineId,
    timeline_revision: u64,
    sample_rate: u32,
    placements: Box<[AudioTimelinePlacement]>,
}

impl AudioTimelineSchedule {
    /// Validates and canonicalizes sample-exact placements.
    ///
    /// Distinct tracks may overlap for later mixing. Placements on one track
    /// may be adjacent or separated by intentional silence, but may not
    /// overlap. One track order belongs to exactly one track identity.
    pub fn new<I>(
        timeline_id: TimelineId,
        timeline_revision: u64,
        sample_rate: u32,
        placements: I,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = AudioTimelinePlacement>,
    {
        Timebase::integer(sample_rate)?;
        let mut placements: Vec<_> = placements.into_iter().collect();
        let mut tracks_by_order = BTreeMap::new();
        let mut orders_by_track = BTreeMap::new();
        let mut clip_ids = BTreeSet::new();

        for placement in &placements {
            if placement.record_start.sample_rate() != sample_rate {
                return Err(invalid(
                    "create_schedule",
                    "every placement must use the schedule sample rate",
                ));
            }
            if let Some(existing) =
                tracks_by_order.insert(placement.track_order, placement.track_id)
            {
                if existing != placement.track_id {
                    return Err(conflict(
                        "create_schedule",
                        "one track order cannot identify multiple tracks",
                    ));
                }
            }
            if let Some(existing) =
                orders_by_track.insert(placement.track_id, placement.track_order)
            {
                if existing != placement.track_order {
                    return Err(conflict(
                        "create_schedule",
                        "one track cannot use multiple authored orders",
                    ));
                }
            }
            if !clip_ids.insert(placement.clip_id) {
                return Err(conflict(
                    "create_schedule",
                    "timeline audio schedule contains a duplicate clip identity",
                ));
            }
        }

        placements.sort_by_key(|placement| {
            (
                placement.track_order,
                placement.record_start.sample(),
                placement.clip_id,
            )
        });
        for pair in placements.windows(2) {
            let left = &pair[0];
            let right = &pair[1];
            if left.track_id == right.track_id
                && left.record_end()?.sample() > right.record_start.sample()
            {
                return Err(conflict(
                    "create_schedule",
                    "timeline audio placements on one track must not overlap",
                ));
            }
        }

        Ok(Self {
            timeline_id,
            timeline_revision,
            sample_rate,
            placements: placements.into_boxed_slice(),
        })
    }

    /// Returns the scheduled timeline identity.
    #[must_use]
    pub const fn timeline_id(&self) -> TimelineId {
        self.timeline_id
    }

    /// Returns the immutable editorial revision used to build this schedule.
    #[must_use]
    pub const fn timeline_revision(&self) -> u64 {
        self.timeline_revision
    }

    /// Returns the common integral sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns placements in authored track order and then record order.
    #[must_use]
    pub const fn placements(&self) -> &[AudioTimelinePlacement] {
        &self.placements
    }
}

/// A fixed mapping between device and timeline sample coordinates.
///
/// A seek, device reset, or sample-rate change creates a new epoch instead of
/// integrating previously rounded positions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioScheduleEpoch {
    device_anchor: SampleTime,
    timeline_anchor: SampleTime,
}

impl AudioScheduleEpoch {
    /// Creates an exact same-rate device-to-timeline anchor.
    pub fn new(device_anchor: SampleTime, timeline_anchor: SampleTime) -> Result<Self> {
        if device_anchor.sample_rate() != timeline_anchor.sample_rate() {
            return Err(invalid(
                "create_epoch",
                "device and timeline anchors must use one sample rate",
            ));
        }
        Ok(Self {
            device_anchor,
            timeline_anchor,
        })
    }

    /// Returns the device coordinate at the anchor.
    #[must_use]
    pub const fn device_anchor(self) -> SampleTime {
        self.device_anchor
    }

    /// Returns the corresponding timeline coordinate.
    #[must_use]
    pub const fn timeline_anchor(self) -> SampleTime {
        self.timeline_anchor
    }

    /// Returns the common integral sample rate.
    #[must_use]
    pub const fn sample_rate(self) -> u32 {
        self.device_anchor.sample_rate()
    }
}

/// Immutable schedule plus the current explicit transport epoch.
#[derive(Debug, Eq, PartialEq)]
pub struct AudioTimelineScheduler {
    schedule: AudioTimelineSchedule,
    epoch: AudioScheduleEpoch,
}

impl AudioTimelineScheduler {
    /// Binds one immutable schedule to a same-rate transport epoch.
    pub fn new(schedule: AudioTimelineSchedule, epoch: AudioScheduleEpoch) -> Result<Self> {
        validate_epoch(schedule.sample_rate, epoch, "create_scheduler")?;
        Ok(Self { schedule, epoch })
    }

    /// Returns the common sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.schedule.sample_rate
    }

    /// Returns the immutable schedule snapshot.
    #[must_use]
    pub const fn schedule(&self) -> &AudioTimelineSchedule {
        &self.schedule
    }

    /// Returns the active transport epoch.
    #[must_use]
    pub const fn epoch(&self) -> AudioScheduleEpoch {
        self.epoch
    }

    /// Replaces the transport anchor after a seek or explicit discontinuity.
    pub fn reanchor(&mut self, epoch: AudioScheduleEpoch) -> Result<()> {
        validate_epoch(self.schedule.sample_rate, epoch, "reanchor_scheduler")?;
        self.epoch = epoch;
        Ok(())
    }

    /// Maps one device callback window to exact borrowed source slices.
    ///
    /// The successful path requires the platform-owned audio domain and does
    /// not lock, allocate, free, sleep, copy sample storage, or mutate either
    /// the schedule or master clock.
    pub fn plan_callback(
        &self,
        device_start: SampleTime,
        frame_count: u64,
    ) -> Result<AudioCallbackPlan<'_>> {
        ExecutionDomain::Audio.require_current()?;
        if device_start.sample_rate() != self.sample_rate() {
            return Err(invalid(
                "plan_callback",
                "device callback must use the schedule sample rate",
            ));
        }
        if frame_count == 0 {
            return Err(invalid(
                "plan_callback",
                "device callback must contain at least one sample frame",
            ));
        }
        let device_end = checked_sample_end(device_start, frame_count, "plan_callback")?;
        let elapsed = device_start
            .sample()
            .checked_sub(self.epoch.device_anchor.sample())
            .ok_or_else(|| invalid("plan_callback", "device epoch distance overflowed"))?;
        if elapsed < 0 {
            return Err(invalid(
                "plan_callback",
                "device callback must not precede the active transport epoch",
            ));
        }
        let timeline_start_value = self
            .epoch
            .timeline_anchor
            .sample()
            .checked_add(elapsed)
            .ok_or_else(|| invalid("plan_callback", "timeline callback start overflowed"))?;
        let timeline_start = SampleTime::new(timeline_start_value, self.sample_rate())?;
        let timeline_end = checked_sample_end(timeline_start, frame_count, "plan_callback")?;

        Ok(AudioCallbackPlan {
            placements: &self.schedule.placements,
            device_start,
            device_end,
            timeline_start,
            timeline_end,
            frame_count,
        })
    }
}

/// One validated device callback window and its borrowed schedule view.
#[derive(Clone, Copy, Debug)]
pub struct AudioCallbackPlan<'a> {
    placements: &'a [AudioTimelinePlacement],
    device_start: SampleTime,
    device_end: SampleTime,
    timeline_start: SampleTime,
    timeline_end: SampleTime,
    frame_count: u64,
}

impl<'a> AudioCallbackPlan<'a> {
    /// Returns the first device sample in this callback.
    #[must_use]
    pub const fn device_start(self) -> SampleTime {
        self.device_start
    }

    /// Returns the exclusive device end.
    #[must_use]
    pub const fn device_end(self) -> SampleTime {
        self.device_end
    }

    /// Returns the first mapped timeline sample.
    #[must_use]
    pub const fn timeline_start(self) -> SampleTime {
        self.timeline_start
    }

    /// Returns the exclusive mapped timeline end.
    #[must_use]
    pub const fn timeline_end(self) -> SampleTime {
        self.timeline_end
    }

    /// Returns the number of output sample frames.
    #[must_use]
    pub const fn frame_count(self) -> u64 {
        self.frame_count
    }

    /// Lazily yields every placement intersecting this callback.
    ///
    /// Output order is authored track order followed by record order. Portions
    /// with no yielded slice are intentional silence for the later mixer.
    #[must_use]
    pub const fn slices(self) -> ScheduledAudioSlices<'a> {
        ScheduledAudioSlices {
            placements: self.placements,
            next_index: 0,
            timeline_start: self.timeline_start,
            timeline_end: self.timeline_end,
        }
    }

    /// Publishes the exclusive presented device end to the shared audio clock.
    ///
    /// Call this only after the complete window is audible. Video timing then
    /// observes the same sample position through `PlaybackClock`.
    pub fn publish_presented(self, clock: &AudioMasterClock) -> Result<AudioClockUpdate> {
        ExecutionDomain::Audio.require_current()?;
        if clock.sample_rate() != self.device_end.sample_rate() {
            return Err(invalid(
                "publish_presented",
                "audio master clock must use the callback sample rate",
            ));
        }
        clock.publish(self.device_end)
    }
}

/// One exact source slice for a later channel-preserving renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledAudioSlice {
    track_id: TrackId,
    clip_id: ClipId,
    track_order: u32,
    output_offset: u64,
    source_start: SampleTime,
    frame_count: u64,
}

impl ScheduledAudioSlice {
    const fn new(
        track_id: TrackId,
        clip_id: ClipId,
        track_order: u32,
        output_offset: u64,
        source_start: SampleTime,
        frame_count: u64,
    ) -> Self {
        Self {
            track_id,
            clip_id,
            track_order,
            output_offset,
            source_start,
            frame_count,
        }
    }

    /// Returns the source track identity.
    #[must_use]
    pub const fn track_id(self) -> TrackId {
        self.track_id
    }

    /// Returns the source clip identity.
    #[must_use]
    pub const fn clip_id(self) -> ClipId {
        self.clip_id
    }

    /// Returns the caller-authored track order.
    #[must_use]
    pub const fn track_order(self) -> u32 {
        self.track_order
    }

    /// Returns the first output frame within the callback window.
    #[must_use]
    pub const fn output_offset(self) -> u64 {
        self.output_offset
    }

    /// Returns the first exact source sample to render.
    #[must_use]
    pub const fn source_start(self) -> SampleTime {
        self.source_start
    }

    /// Returns the number of inter-channel sample frames to render.
    #[must_use]
    pub const fn frame_count(self) -> u64 {
        self.frame_count
    }
}

/// Allocation-free iterator over callback intersections.
#[derive(Clone, Debug)]
pub struct ScheduledAudioSlices<'a> {
    placements: &'a [AudioTimelinePlacement],
    next_index: usize,
    timeline_start: SampleTime,
    timeline_end: SampleTime,
}

impl Iterator for ScheduledAudioSlices<'_> {
    type Item = ScheduledAudioSlice;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(placement) = self.placements.get(self.next_index) {
            self.next_index += 1;
            let record_start = placement.record_start.sample();
            let record_end =
                record_start.checked_add(i64::try_from(placement.frame_count).ok()?)?;
            let intersection_start = record_start.max(self.timeline_start.sample());
            let intersection_end = record_end.min(self.timeline_end.sample());
            if intersection_start >= intersection_end {
                continue;
            }

            let output_offset =
                u64::try_from(intersection_start.checked_sub(self.timeline_start.sample())?)
                    .ok()?;
            let source_offset = intersection_start.checked_sub(record_start)?;
            let source_start = placement.source_start.sample().checked_add(source_offset)?;
            let frame_count =
                u64::try_from(intersection_end.checked_sub(intersection_start)?).ok()?;

            return Some(ScheduledAudioSlice::new(
                placement.track_id,
                placement.clip_id,
                placement.track_order,
                output_offset,
                SampleTime::new(source_start, placement.source_start.sample_rate()).ok()?,
                frame_count,
            ));
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (
            0,
            Some(self.placements.len().saturating_sub(self.next_index)),
        )
    }
}

impl std::iter::FusedIterator for ScheduledAudioSlices<'_> {}

fn validate_epoch(
    sample_rate: u32,
    epoch: AudioScheduleEpoch,
    operation: &'static str,
) -> Result<()> {
    if epoch.sample_rate() != sample_rate {
        return Err(invalid(
            operation,
            "transport epoch must use the schedule sample rate",
        ));
    }
    Ok(())
}

fn checked_sample_end(
    start: SampleTime,
    frame_count: u64,
    operation: &'static str,
) -> Result<SampleTime> {
    let frame_count = i64::try_from(frame_count).map_err(|_| {
        invalid(
            operation,
            "sample window exceeds the supported coordinate range",
        )
    })?;
    let end = start.sample().checked_add(frame_count).ok_or_else(|| {
        invalid(
            operation,
            "sample window exceeds the supported coordinate range",
        )
    })?;
    SampleTime::new(end, start.sample_rate())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
