//! Sample-accurate clip mixing and editable clip-owned mix intent.
//!
//! [`ClipMixState`] is edited transactionally away from the audio callback. A cloned
//! [`ClipMixSnapshot`] resolves project-wide solo state and prepares a fixed routing matrix for a
//! [`ClipMixProcessor`]. The processor then applies channel mapping, equal-power stereo pan, phase,
//! linear gain, and exact sample fades without allocating or locking in `process`.

use std::collections::{BTreeMap, BTreeSet};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::ClipId;
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::SampleTime;

use crate::graph::{AudioProcessBlock, AudioProcessor};

const COMPONENT: &str = "superi-audio.mixing";
const MAX_GAIN: f32 = 64.0;
const MAX_ROUTE_GAIN: f32 = 8.0;

/// One coefficient in a semantic source-to-destination channel matrix.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelMap {
    source: ChannelPosition,
    destination: ChannelPosition,
    gain: f32,
}

impl ChannelMap {
    /// Creates one finite, nonnegative routing coefficient.
    pub fn new(source: ChannelPosition, destination: ChannelPosition, gain: f32) -> Result<Self> {
        if !gain.is_finite() || !(0.0..=MAX_ROUTE_GAIN).contains(&gain) {
            return Err(invalid(
                "create_channel_map",
                "channel-map gain must be finite and between 0 and 8",
            ));
        }
        Ok(Self {
            source,
            destination,
            gain,
        })
    }

    /// Returns the semantic input channel.
    #[must_use]
    pub const fn source(self) -> ChannelPosition {
        self.source
    }

    /// Returns the semantic output channel.
    #[must_use]
    pub const fn destination(self) -> ChannelPosition {
        self.destination
    }

    /// Returns the linear routing coefficient.
    #[must_use]
    pub const fn gain(self) -> f32 {
        self.gain
    }
}

/// Complete user-editable mix intent for one clip identity.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipMixControls {
    input_layout: ChannelLayout,
    output_layout: ChannelLayout,
    channel_map: Vec<ChannelMap>,
    gain: f32,
    fade_in_frames: u64,
    fade_out_frames: u64,
    pan: f32,
    muted: bool,
    solo: bool,
    phase_inverted: BTreeSet<ChannelPosition>,
}

impl ClipMixControls {
    /// Creates validated controls with unity gain and no fades, pan, mute, solo, or phase changes.
    pub fn new<I>(
        input_layout: ChannelLayout,
        output_layout: ChannelLayout,
        channel_map: I,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = ChannelMap>,
    {
        let channel_map: Vec<_> = channel_map.into_iter().collect();
        if channel_map.is_empty() {
            return Err(invalid(
                "create_clip_mix_controls",
                "clip channel mapping must contain at least one route",
            ));
        }
        let mut pairs = BTreeSet::new();
        for route in &channel_map {
            if !input_layout.positions().contains(&route.source) {
                return Err(invalid(
                    "create_clip_mix_controls",
                    "channel-map source is absent from the input layout",
                ));
            }
            if !output_layout.positions().contains(&route.destination) {
                return Err(invalid(
                    "create_clip_mix_controls",
                    "channel-map destination is absent from the output layout",
                ));
            }
            if !pairs.insert((route.source, route.destination)) {
                return Err(invalid(
                    "create_clip_mix_controls",
                    "channel mapping contains a duplicate source-destination pair",
                ));
            }
        }
        Ok(Self {
            input_layout,
            output_layout,
            channel_map,
            gain: 1.0,
            fade_in_frames: 0,
            fade_out_frames: 0,
            pan: 0.0,
            muted: false,
            solo: false,
            phase_inverted: BTreeSet::new(),
        })
    }

    /// Replaces the bounded linear clip gain.
    pub fn with_gain(mut self, gain: f32) -> Result<Self> {
        if !gain.is_finite() || !(0.0..=MAX_GAIN).contains(&gain) {
            return Err(invalid(
                "set_clip_gain",
                "clip gain must be finite and between 0 and 64",
            ));
        }
        self.gain = gain;
        Ok(self)
    }

    /// Replaces exact fade lengths in sample frames.
    ///
    /// A nonzero fade contains both endpoints and therefore requires at least two samples.
    pub fn with_fades(mut self, fade_in_frames: u64, fade_out_frames: u64) -> Result<Self> {
        if matches!(fade_in_frames, 1) || matches!(fade_out_frames, 1) {
            return Err(invalid(
                "set_clip_fades",
                "a nonzero clip fade must contain at least two sample frames",
            ));
        }
        self.fade_in_frames = fade_in_frames;
        self.fade_out_frames = fade_out_frames;
        Ok(self)
    }

    /// Replaces equal-power stereo pan in the inclusive range `-1..=1`.
    pub fn with_pan(mut self, pan: f32) -> Result<Self> {
        if !pan.is_finite() || !(-1.0..=1.0).contains(&pan) {
            return Err(invalid(
                "set_clip_pan",
                "clip pan must be finite and between -1 and 1",
            ));
        }
        if pan != 0.0 && self.output_layout != ChannelLayout::stereo() {
            return Err(invalid(
                "set_clip_pan",
                "nonzero clip pan requires the canonical stereo output layout",
            ));
        }
        self.pan = pan;
        Ok(self)
    }

    /// Replaces the explicit mute state.
    #[must_use]
    pub const fn with_muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    /// Replaces the explicit solo state.
    #[must_use]
    pub const fn with_solo(mut self, solo: bool) -> Self {
        self.solo = solo;
        self
    }

    /// Replaces the set of destination channels whose phase is inverted.
    pub fn with_phase_inverted<I>(mut self, positions: I) -> Result<Self>
    where
        I: IntoIterator<Item = ChannelPosition>,
    {
        let positions: BTreeSet<_> = positions.into_iter().collect();
        if positions
            .iter()
            .any(|position| !self.output_layout.positions().contains(position))
        {
            return Err(invalid(
                "set_clip_phase",
                "phase inversion names a channel absent from the output layout",
            ));
        }
        self.phase_inverted = positions;
        Ok(self)
    }

    /// Returns the required input channel meaning.
    #[must_use]
    pub const fn input_layout(&self) -> &ChannelLayout {
        &self.input_layout
    }

    /// Returns the emitted output channel meaning.
    #[must_use]
    pub const fn output_layout(&self) -> &ChannelLayout {
        &self.output_layout
    }

    /// Returns semantic routing entries in stable user-supplied order.
    #[must_use]
    pub fn channel_map(&self) -> &[ChannelMap] {
        &self.channel_map
    }

    /// Returns the bounded linear clip gain.
    #[must_use]
    pub const fn gain(&self) -> f32 {
        self.gain
    }

    /// Returns the exact fade-in length in sample frames.
    #[must_use]
    pub const fn fade_in_frames(&self) -> u64 {
        self.fade_in_frames
    }

    /// Returns the exact fade-out length in sample frames.
    #[must_use]
    pub const fn fade_out_frames(&self) -> u64 {
        self.fade_out_frames
    }

    /// Returns equal-power stereo pan in the inclusive range `-1..=1`.
    #[must_use]
    pub const fn pan(&self) -> f32 {
        self.pan
    }

    /// Returns whether the clip is explicitly muted.
    #[must_use]
    pub const fn muted(&self) -> bool {
        self.muted
    }

    /// Returns whether the clip participates in the project-wide solo set.
    #[must_use]
    pub const fn solo(&self) -> bool {
        self.solo
    }

    /// Returns destination channels whose phase is inverted.
    #[must_use]
    pub const fn phase_inverted(&self) -> &BTreeSet<ChannelPosition> {
        &self.phase_inverted
    }
}

/// One atomic clip-mix state mutation.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ClipMixMutation {
    /// Create or replace controls for one clip.
    Set {
        /// Stable editorial clip identity.
        clip_id: ClipId,
        /// Complete replacement intent.
        controls: ClipMixControls,
    },
    /// Copy intent to a caller-created clip fragment while retaining the original.
    Inherit {
        /// Existing clip identity.
        original: ClipId,
        /// New fragment identity.
        created: ClipId,
    },
    /// Move intent from a removed clip identity to its replacement.
    Transfer {
        /// Existing identity being replaced.
        removed: ClipId,
        /// New replacement identity.
        inserted: ClipId,
    },
    /// Remove intent for a removed clip.
    Remove {
        /// Existing clip identity.
        clip_id: ClipId,
    },
}

impl ClipMixMutation {
    /// Creates a complete set mutation.
    #[must_use]
    pub const fn set(clip_id: ClipId, controls: ClipMixControls) -> Self {
        Self::Set { clip_id, controls }
    }

    /// Creates a fragment-inheritance mutation.
    #[must_use]
    pub const fn inherit(original: ClipId, created: ClipId) -> Self {
        Self::Inherit { original, created }
    }

    /// Creates a replacement-transfer mutation.
    #[must_use]
    pub const fn transfer(removed: ClipId, inserted: ClipId) -> Self {
        Self::Transfer { removed, inserted }
    }

    /// Creates a removal mutation.
    #[must_use]
    pub const fn remove(clip_id: ClipId) -> Self {
        Self::Remove { clip_id }
    }
}

/// Revisioned editable clip-mix state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipMixState {
    revision: u64,
    controls: BTreeMap<ClipId, ClipMixControls>,
}

impl ClipMixState {
    /// Creates empty state at revision zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            revision: 0,
            controls: BTreeMap::new(),
        }
    }

    /// Returns the published revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns controls for one clip identity.
    #[must_use]
    pub fn controls(&self, clip_id: ClipId) -> Option<&ClipMixControls> {
        self.controls.get(&clip_id)
    }

    /// Validates an expected revision without publishing a mutation.
    pub fn require_revision(&self, expected_revision: u64) -> Result<()> {
        if expected_revision != self.revision {
            return Err(conflict(
                "check_mix_revision",
                "clip-mix revision does not match the expected revision",
            ));
        }
        Ok(())
    }

    /// Applies a nonempty mutation batch atomically at the expected revision.
    pub fn apply(&mut self, expected_revision: u64, mutations: &[ClipMixMutation]) -> Result<u64> {
        self.require_revision(expected_revision)?;
        if mutations.is_empty() {
            return Err(invalid(
                "apply_mix_mutations",
                "clip-mix mutation batch must not be empty",
            ));
        }
        let revision = self.revision.checked_add(1).ok_or_else(|| {
            conflict(
                "apply_mix_mutations",
                "clip-mix revision cannot advance beyond its integer domain",
            )
        })?;
        let mut next = self.controls.clone();
        for mutation in mutations {
            match mutation {
                ClipMixMutation::Set { clip_id, controls } => {
                    next.insert(*clip_id, controls.clone());
                }
                ClipMixMutation::Inherit { original, created } => {
                    if next.contains_key(created) {
                        return Err(conflict(
                            "inherit_clip_mix",
                            "created clip identity already owns mix intent",
                        ));
                    }
                    let controls = next.get(original).cloned().ok_or_else(|| {
                        not_found(
                            "inherit_clip_mix",
                            "original clip identity has no mix intent",
                        )
                    })?;
                    next.insert(*created, controls);
                }
                ClipMixMutation::Transfer { removed, inserted } => {
                    if removed == inserted || next.contains_key(inserted) {
                        return Err(conflict(
                            "transfer_clip_mix",
                            "replacement clip identity must be new",
                        ));
                    }
                    let controls = next.remove(removed).ok_or_else(|| {
                        not_found(
                            "transfer_clip_mix",
                            "removed clip identity has no mix intent",
                        )
                    })?;
                    next.insert(*inserted, controls);
                }
                ClipMixMutation::Remove { clip_id } => {
                    if next.remove(clip_id).is_none() {
                        return Err(not_found(
                            "remove_clip_mix",
                            "removed clip identity has no mix intent",
                        ));
                    }
                }
            }
        }
        self.controls = next;
        self.revision = revision;
        Ok(revision)
    }

    /// Clones immutable intent for preparation outside the audio callback.
    #[must_use]
    pub fn snapshot(&self) -> ClipMixSnapshot {
        let any_solo = self.controls.values().any(|controls| controls.solo);
        ClipMixSnapshot {
            revision: self.revision,
            controls: self.controls.clone(),
            any_solo,
        }
    }
}

/// Immutable mix state used to prepare clip processors consistently.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipMixSnapshot {
    revision: u64,
    controls: BTreeMap<ClipId, ClipMixControls>,
    any_solo: bool,
}

impl ClipMixSnapshot {
    /// Returns the source editable revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Prepares one clip processor with fixed bounds and routing storage.
    pub fn prepare_processor(
        &self,
        clip_id: ClipId,
        start_time: SampleTime,
        frame_count: u64,
    ) -> Result<ClipMixProcessor> {
        let controls = self.controls.get(&clip_id).ok_or_else(|| {
            not_found(
                "prepare_clip_mix",
                "clip identity has no mix controls in this snapshot",
            )
        })?;
        if frame_count == 0 {
            return Err(invalid(
                "prepare_clip_mix",
                "clip frame count must be greater than zero",
            ));
        }
        if controls.fade_in_frames > frame_count || controls.fade_out_frames > frame_count {
            return Err(invalid(
                "prepare_clip_mix",
                "clip fades must fit within the clip sample duration",
            ));
        }
        let frame_count_i64 = i64::try_from(frame_count).map_err(|_| {
            invalid(
                "prepare_clip_mix",
                "clip frame count exceeds the sample coordinate domain",
            )
        })?;
        let end_sample = start_time
            .sample()
            .checked_add(frame_count_i64)
            .ok_or_else(|| {
                invalid(
                    "prepare_clip_mix",
                    "clip end exceeds the sample coordinate domain",
                )
            })?;
        let input_channels = controls.input_layout.len();
        let output_channels = controls.output_layout.len();
        let mut matrix = vec![0.0; input_channels * output_channels];
        for route in &controls.channel_map {
            let source = controls
                .input_layout
                .positions()
                .iter()
                .position(|position| *position == route.source)
                .expect("validated route source remains present");
            let destination = controls
                .output_layout
                .positions()
                .iter()
                .position(|position| *position == route.destination)
                .expect("validated route destination remains present");
            matrix[destination * input_channels + source] = route.gain;
        }
        let phase = controls
            .output_layout
            .positions()
            .iter()
            .map(|position| {
                if controls.phase_inverted.contains(position) {
                    -1.0
                } else {
                    1.0
                }
            })
            .collect();
        Ok(ClipMixProcessor {
            clip_id,
            sample_rate: start_time.sample_rate(),
            start_sample: start_time.sample(),
            end_sample,
            frame_count,
            input_layout: controls.input_layout.clone(),
            output_layout: controls.output_layout.clone(),
            matrix,
            phase,
            gain: controls.gain,
            fade_in_frames: controls.fade_in_frames,
            fade_out_frames: controls.fade_out_frames,
            pan: controls.pan,
            silenced: controls.muted || (self.any_solo && !controls.solo),
        })
    }
}

/// Fixed, allocation-free processor for one clip interval.
#[derive(Debug)]
pub struct ClipMixProcessor {
    clip_id: ClipId,
    sample_rate: u32,
    start_sample: i64,
    end_sample: i64,
    frame_count: u64,
    input_layout: ChannelLayout,
    output_layout: ChannelLayout,
    matrix: Vec<f32>,
    phase: Vec<f32>,
    gain: f32,
    fade_in_frames: u64,
    fade_out_frames: u64,
    pan: f32,
    silenced: bool,
}

impl ClipMixProcessor {
    /// Returns the editorial clip identity bound during preparation.
    #[must_use]
    pub const fn clip_id(&self) -> ClipId {
        self.clip_id
    }
}

impl AudioProcessor for ClipMixProcessor {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let input = block.input.ok_or_else(|| {
            invalid(
                "process_clip_mix",
                "clip mix processor requires connected input samples",
            )
        })?;
        if block.start_time.sample_rate() != self.sample_rate
            || block.input_layout != Some(&self.input_layout)
            || block.output_layout != &self.output_layout
        {
            return Err(invalid(
                "process_clip_mix",
                "clip mix block does not match its prepared clock and channel layouts",
            ));
        }
        let expected_input = block
            .frame_count
            .checked_mul(self.input_layout.len())
            .ok_or_else(|| {
                invalid(
                    "process_clip_mix",
                    "clip mix input sample count exceeds the addressable domain",
                )
            })?;
        let expected_output = block
            .frame_count
            .checked_mul(self.output_layout.len())
            .ok_or_else(|| {
                invalid(
                    "process_clip_mix",
                    "clip mix output sample count exceeds the addressable domain",
                )
            })?;
        if input.len() != expected_input || block.output.len() != expected_output {
            return Err(invalid(
                "process_clip_mix",
                "clip mix sample buffers do not match frame count and channel layouts",
            ));
        }
        let block_frames = i64::try_from(block.frame_count).map_err(|_| {
            invalid(
                "process_clip_mix",
                "clip mix block exceeds the sample coordinate domain",
            )
        })?;
        let block_end = block
            .start_time
            .sample()
            .checked_add(block_frames)
            .ok_or_else(|| {
                invalid(
                    "process_clip_mix",
                    "clip mix block end exceeds the sample coordinate domain",
                )
            })?;
        if block.start_time.sample() < self.start_sample || block_end > self.end_sample {
            return Err(invalid(
                "process_clip_mix",
                "clip mix block lies outside the prepared clip interval",
            ));
        }

        let input_channels = self.input_layout.len();
        let output_channels = self.output_layout.len();
        for (frame_index, (input_frame, output_frame)) in input
            .chunks_exact(input_channels)
            .zip(block.output.chunks_exact_mut(output_channels))
            .enumerate()
        {
            if self.silenced {
                output_frame.fill(0.0);
                continue;
            }
            for (destination, output) in output_frame.iter_mut().enumerate() {
                let row =
                    &self.matrix[destination * input_channels..(destination + 1) * input_channels];
                *output = input_frame
                    .iter()
                    .zip(row)
                    .map(|(sample, coefficient)| sample * coefficient)
                    .sum();
            }
            apply_stereo_pan(output_frame, self.pan);
            let frame_index = i64::try_from(frame_index).expect("bounded block index fits i64");
            let absolute = block.start_time.sample() + frame_index;
            let position = u64::try_from(absolute - self.start_sample)
                .expect("validated block starts within clip");
            let envelope = fade_gain(
                position,
                self.frame_count,
                self.fade_in_frames,
                self.fade_out_frames,
            );
            for (sample, phase) in output_frame.iter_mut().zip(&self.phase) {
                *sample *= self.gain * envelope * phase;
            }
        }
        Ok(())
    }
}

fn fade_gain(position: u64, frame_count: u64, fade_in: u64, fade_out: u64) -> f32 {
    let fade_in_gain = if fade_in == 0 || position >= fade_in {
        1.0
    } else {
        position as f32 / (fade_in - 1) as f32
    };
    let fade_out_start = frame_count - fade_out;
    let fade_out_gain = if fade_out == 0 || position < fade_out_start {
        1.0
    } else {
        (frame_count - 1 - position) as f32 / (fade_out - 1) as f32
    };
    fade_in_gain * fade_out_gain
}

fn apply_stereo_pan(samples: &mut [f32], pan: f32) {
    if pan == 0.0 {
        return;
    }
    debug_assert_eq!(samples.len(), 2);
    let left = samples[0];
    let right = samples[1];
    if pan == -1.0 {
        samples[0] = left + right;
        samples[1] = 0.0;
        return;
    }
    if pan == 1.0 {
        samples[0] = 0.0;
        samples[1] = right + left;
        return;
    }
    if pan <= 0.0 {
        let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_2;
        samples[0] = left + right * angle.cos();
        samples[1] = right * angle.sin();
    } else {
        let angle = pan * std::f32::consts::FRAC_PI_2;
        samples[0] = left * angle.cos();
        samples[1] = right + left * angle.sin();
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    mix_error(ErrorCategory::InvalidInput, operation, message)
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    mix_error(ErrorCategory::Conflict, operation, message)
}

fn not_found(operation: &'static str, message: &'static str) -> Error {
    mix_error(ErrorCategory::NotFound, operation, message)
}

fn mix_error(category: ErrorCategory, operation: &'static str, message: &'static str) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
