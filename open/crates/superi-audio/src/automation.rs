//! Revisioned sample-accurate clip-gain automation and prepared curve evaluation.
//!
//! Authored lanes and mode transitions are mutated on a control owner. Immutable snapshots compile
//! one clip-gain lane into a prepared curve before audio processing, so callback evaluation uses
//! only bounded scalar work over owned storage.

use std::collections::BTreeMap;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::ClipId;
use superi_core::time::SampleTime;

const COMPONENT: &str = "superi-audio.automation";
const MAX_GAIN: f32 = 64.0;

/// Maximum ordered mutations accepted in one revision-fenced transaction.
pub const MAX_AUDIO_AUTOMATION_MUTATIONS: usize = 64;
/// Maximum authored automation lanes retained by one state owner.
pub const MAX_AUDIO_AUTOMATION_LANES: usize = 4_096;
/// Maximum total keyframes retained across every authored lane.
pub const MAX_AUDIO_AUTOMATION_KEYFRAMES: usize = 1_048_576;

/// The professional playback and recording behavior of one automation lane.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AudioAutomationMode {
    /// Evaluate the existing curve without accepting a write pass.
    Read,
    /// Replace the complete played pass with recorded control values.
    Write,
    /// Replace only intervals during which the control is physically touched.
    Touch,
    /// Continue the last touched value until the write pass ends.
    Latch,
}

/// One typed parameter address supported by audio automation schema 1.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AudioAutomationTarget {
    /// Linear gain for one editorial clip identity.
    ClipGain {
        /// Stable clip identity whose existing mix processor consumes the lane.
        clip_id: ClipId,
    },
}

impl AudioAutomationTarget {
    /// Creates the schema-1 clip-gain target.
    #[must_use]
    pub const fn clip_gain(clip_id: ClipId) -> Self {
        Self::ClipGain { clip_id }
    }

    /// Returns the clip identity addressed by this schema version.
    #[must_use]
    pub const fn clip_id(self) -> ClipId {
        match self {
            Self::ClipGain { clip_id } => clip_id,
        }
    }
}

/// One finite bounded gain value at an exact signed sample coordinate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioAutomationKeyframe {
    at: SampleTime,
    value: f32,
}

impl Eq for AudioAutomationKeyframe {}

impl AudioAutomationKeyframe {
    /// Creates one validated clip-gain keyframe.
    pub fn new(at: SampleTime, value: f32) -> Result<Self> {
        validate_gain(value, "create_keyframe")?;
        Ok(Self { at, value })
    }

    /// Returns the exact sample coordinate.
    #[must_use]
    pub const fn at(self) -> SampleTime {
        self.at
    }

    /// Returns the finite linear gain.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.value
    }
}

/// One ordered authored automation mutation.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AudioAutomationMutation {
    /// Creates a new lane in Read mode.
    CreateLane {
        /// Typed parameter address.
        target: AudioAutomationTarget,
        /// Positive integral sample clock used by every keyframe and pass mutation.
        sample_rate: u32,
        /// Value used before the first keyframe and when the lane is empty.
        default_gain: f32,
    },
    /// Removes one lane when no pass is active.
    RemoveLane {
        /// Typed parameter address.
        target: AudioAutomationTarget,
    },
    /// Changes one lane's professional automation mode.
    SetMode {
        /// Typed parameter address.
        target: AudioAutomationTarget,
        /// New mode.
        mode: AudioAutomationMode,
    },
    /// Inserts or replaces one exact keyframe.
    SetKeyframe {
        /// Typed parameter address.
        target: AudioAutomationTarget,
        /// Exact point.
        keyframe: AudioAutomationKeyframe,
    },
    /// Removes a keyframe at one exact coordinate.
    RemoveKeyframe {
        /// Typed parameter address.
        target: AudioAutomationTarget,
        /// Exact point to remove.
        at: SampleTime,
    },
    /// Begins one playback pass in Write, Touch, or Latch mode.
    BeginPass {
        /// Typed parameter address.
        target: AudioAutomationTarget,
        /// First sample covered by the pass.
        at: SampleTime,
        /// Current finite control value at the pass boundary.
        current_value: f32,
    },
    /// Begins physical manipulation in Touch or Latch mode.
    BeginTouch {
        /// Typed parameter address.
        target: AudioAutomationTarget,
        /// First touched sample.
        at: SampleTime,
        /// First touched control value.
        value: f32,
    },
    /// Records one ordered control value during a writable interval.
    SetControlValue {
        /// Typed parameter address.
        target: AudioAutomationTarget,
        /// Exact sample coordinate.
        at: SampleTime,
        /// Finite control value.
        value: f32,
    },
    /// Releases physical manipulation in Touch or Latch mode.
    EndTouch {
        /// Typed parameter address.
        target: AudioAutomationTarget,
        /// Exclusive release coordinate.
        at: SampleTime,
    },
    /// Ends one pass and publishes its exact half-open overwrite regions.
    EndPass {
        /// Typed parameter address.
        target: AudioAutomationTarget,
        /// Exclusive pass end.
        at: SampleTime,
    },
}

impl Eq for AudioAutomationMutation {}

/// One optimistic ordered automation transaction.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioAutomationTransaction {
    expected_revision: u64,
    mutations: Vec<AudioAutomationMutation>,
}

impl Eq for AudioAutomationTransaction {}

impl AudioAutomationTransaction {
    /// Creates a bounded nonempty transaction fenced to one exact revision.
    pub fn new(expected_revision: u64, mutations: Vec<AudioAutomationMutation>) -> Result<Self> {
        if mutations.is_empty() {
            return Err(invalid(
                "create_transaction",
                "audio automation transaction must contain at least one mutation",
            ));
        }
        if mutations.len() > MAX_AUDIO_AUTOMATION_MUTATIONS {
            return Err(resource_exhausted(
                "create_transaction",
                "audio automation transaction exceeds the mutation bound",
            ));
        }
        Ok(Self {
            expected_revision,
            mutations,
        })
    }

    /// Returns the exact required state revision.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Returns mutations in caller-authored execution order.
    #[must_use]
    pub fn mutations(&self) -> &[AudioAutomationMutation] {
        &self.mutations
    }
}

#[derive(Clone, Debug, PartialEq)]
struct AutomationLane {
    sample_rate: u32,
    default_gain: f32,
    mode: AudioAutomationMode,
    keyframes: BTreeMap<i64, f32>,
    active_pass: Option<ActivePass>,
}

impl Eq for AutomationLane {}

#[derive(Clone, Debug, PartialEq)]
struct ActivePass {
    mode: AudioAutomationMode,
    start: i64,
    last_time: i64,
    current_value: f32,
    touch_active: bool,
    baseline: BTreeMap<i64, f32>,
    completed_regions: Vec<WriteRegion>,
    active_touch_region: Option<WriteRegion>,
    continuous_region: Option<WriteRegion>,
}

impl Eq for ActivePass {}

#[derive(Clone, Debug, PartialEq)]
struct WriteRegion {
    start: i64,
    end: Option<i64>,
    points: BTreeMap<i64, f32>,
}

impl Eq for WriteRegion {}

impl WriteRegion {
    fn new(start: i64, value: f32) -> Self {
        Self {
            start,
            end: None,
            points: BTreeMap::from([(start, value)]),
        }
    }

    fn finish(mut self, end: i64, operation: &'static str) -> Result<Self> {
        if end <= self.start {
            return Err(conflict(
                operation,
                "audio automation write interval must contain at least one sample",
            ));
        }
        self.end = Some(end);
        Ok(self)
    }
}

impl ActivePass {
    fn new(
        mode: AudioAutomationMode,
        start: i64,
        current_value: f32,
        baseline: BTreeMap<i64, f32>,
    ) -> Self {
        Self {
            mode,
            start,
            last_time: start,
            current_value,
            touch_active: false,
            baseline,
            completed_regions: Vec::new(),
            active_touch_region: None,
            continuous_region: (mode == AudioAutomationMode::Write)
                .then(|| WriteRegion::new(start, current_value)),
        }
    }

    fn advance(&mut self, sample: i64, operation: &'static str) -> Result<()> {
        if sample < self.last_time {
            return Err(conflict(
                operation,
                "audio automation pass coordinates must not move backward",
            ));
        }
        self.last_time = sample;
        Ok(())
    }
}

/// Revisioned authoritative authored automation state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioAutomationState {
    revision: u64,
    lanes: BTreeMap<AudioAutomationTarget, AutomationLane>,
}

impl Eq for AudioAutomationState {}

impl AudioAutomationState {
    /// Creates empty automation state at revision zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            revision: 0,
            lanes: BTreeMap::new(),
        }
    }

    /// Returns the current published revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Clones one immutable complete replacement snapshot.
    #[must_use]
    pub fn snapshot(&self) -> AudioAutomationSnapshot {
        AudioAutomationSnapshot {
            revision: self.revision,
            lanes: self
                .lanes
                .iter()
                .map(|(target, lane)| (*target, lane.snapshot(*target)))
                .collect(),
        }
    }

    /// Validates whether a transaction would change semantic state without publishing it.
    pub fn would_change(&self, transaction: &AudioAutomationTransaction) -> Result<bool> {
        let candidate = self.candidate(transaction)?;
        Ok(candidate.lanes != self.lanes)
    }

    /// Applies one complete transaction atomically and returns replacement state.
    pub fn apply(
        &mut self,
        transaction: AudioAutomationTransaction,
    ) -> Result<AudioAutomationSnapshot> {
        let mut candidate = self.candidate(&transaction)?;
        if candidate.lanes == self.lanes {
            return Ok(self.snapshot());
        }
        candidate.revision = self.revision.checked_add(1).ok_or_else(|| {
            resource_exhausted(
                "apply_transaction",
                "audio automation revision cannot advance beyond its integer domain",
            )
        })?;
        *self = candidate;
        Ok(self.snapshot())
    }

    fn candidate(&self, transaction: &AudioAutomationTransaction) -> Result<Self> {
        if transaction.expected_revision != self.revision {
            return Err(conflict(
                "apply_transaction",
                "audio automation revision does not match the transaction fence",
            ));
        }
        let mut candidate = self.clone();
        for mutation in &transaction.mutations {
            candidate.apply_mutation(mutation.clone())?;
        }
        candidate.validate_capacity()?;
        Ok(candidate)
    }

    fn validate_capacity(&self) -> Result<()> {
        if self.lanes.len() > MAX_AUDIO_AUTOMATION_LANES {
            return Err(resource_exhausted(
                "apply_transaction",
                "audio automation lane capacity was exceeded",
            ));
        }
        let keyframes = self
            .lanes
            .values()
            .map(|lane| lane.keyframes.len())
            .sum::<usize>();
        if keyframes > MAX_AUDIO_AUTOMATION_KEYFRAMES {
            return Err(resource_exhausted(
                "apply_transaction",
                "audio automation keyframe capacity was exceeded",
            ));
        }
        Ok(())
    }

    fn apply_mutation(&mut self, mutation: AudioAutomationMutation) -> Result<()> {
        match mutation {
            AudioAutomationMutation::CreateLane {
                target,
                sample_rate,
                default_gain,
            } => {
                if sample_rate == 0 {
                    return Err(invalid(
                        "create_lane",
                        "audio automation lane sample rate must be greater than zero",
                    ));
                }
                validate_gain(default_gain, "create_lane")?;
                if self.lanes.contains_key(&target) {
                    return Err(conflict(
                        "create_lane",
                        "audio automation target already owns a lane",
                    ));
                }
                self.lanes.insert(
                    target,
                    AutomationLane {
                        sample_rate,
                        default_gain,
                        mode: AudioAutomationMode::Read,
                        keyframes: BTreeMap::new(),
                        active_pass: None,
                    },
                );
            }
            AudioAutomationMutation::RemoveLane { target } => {
                if self
                    .lanes
                    .get(&target)
                    .is_some_and(|lane| lane.active_pass.is_some())
                {
                    return Err(conflict(
                        "remove_lane",
                        "audio automation lane cannot be removed during an active pass",
                    ));
                }
                self.lanes.remove(&target);
            }
            AudioAutomationMutation::SetMode { target, mode } => {
                let lane = self.lane_mut(target, "set_mode")?;
                require_idle(lane, "set_mode")?;
                lane.mode = mode;
            }
            AudioAutomationMutation::SetKeyframe { target, keyframe } => {
                let lane = self.lane_mut(target, "set_keyframe")?;
                require_idle(lane, "set_keyframe")?;
                require_clock(lane, keyframe.at, "set_keyframe")?;
                lane.keyframes.insert(keyframe.at.sample(), keyframe.value);
            }
            AudioAutomationMutation::RemoveKeyframe { target, at } => {
                let lane = self.lane_mut(target, "remove_keyframe")?;
                require_idle(lane, "remove_keyframe")?;
                require_clock(lane, at, "remove_keyframe")?;
                lane.keyframes.remove(&at.sample());
            }
            AudioAutomationMutation::BeginPass {
                target,
                at,
                current_value,
            } => {
                validate_gain(current_value, "begin_pass")?;
                require_recordable_coordinate(at, "begin_pass")?;
                let lane = self.lane_mut(target, "begin_pass")?;
                require_clock(lane, at, "begin_pass")?;
                if lane.mode == AudioAutomationMode::Read {
                    return Err(conflict(
                        "begin_pass",
                        "Read automation mode does not accept a write pass",
                    ));
                }
                if lane.active_pass.is_some() {
                    return Err(conflict(
                        "begin_pass",
                        "audio automation lane already owns an active pass",
                    ));
                }
                lane.active_pass = Some(ActivePass::new(
                    lane.mode,
                    at.sample(),
                    current_value,
                    lane.keyframes.clone(),
                ));
            }
            AudioAutomationMutation::BeginTouch { target, at, value } => {
                validate_gain(value, "begin_touch")?;
                require_recordable_coordinate(at, "begin_touch")?;
                let lane = self.lane_mut(target, "begin_touch")?;
                require_clock(lane, at, "begin_touch")?;
                let pass = lane.active_pass.as_mut().ok_or_else(|| {
                    conflict(
                        "begin_touch",
                        "audio automation touch requires an active pass",
                    )
                })?;
                if !matches!(
                    pass.mode,
                    AudioAutomationMode::Touch | AudioAutomationMode::Latch
                ) {
                    return Err(conflict(
                        "begin_touch",
                        "physical touch is available only in Touch or Latch mode",
                    ));
                }
                if pass.touch_active {
                    return Err(conflict(
                        "begin_touch",
                        "audio automation control is already touched",
                    ));
                }
                pass.advance(at.sample(), "begin_touch")?;
                match pass.mode {
                    AudioAutomationMode::Touch => {
                        pass.active_touch_region = Some(WriteRegion::new(at.sample(), value));
                    }
                    AudioAutomationMode::Latch => {
                        let region = pass
                            .continuous_region
                            .get_or_insert_with(|| WriteRegion::new(at.sample(), value));
                        region.points.insert(at.sample(), value);
                    }
                    _ => unreachable!("mode was validated"),
                }
                pass.current_value = value;
                pass.touch_active = true;
            }
            AudioAutomationMutation::SetControlValue { target, at, value } => {
                validate_gain(value, "set_control_value")?;
                require_recordable_coordinate(at, "set_control_value")?;
                let lane = self.lane_mut(target, "set_control_value")?;
                require_clock(lane, at, "set_control_value")?;
                let pass = lane.active_pass.as_mut().ok_or_else(|| {
                    conflict(
                        "set_control_value",
                        "audio automation control value requires an active pass",
                    )
                })?;
                pass.advance(at.sample(), "set_control_value")?;
                match pass.mode {
                    AudioAutomationMode::Write => {
                        pass.continuous_region
                            .as_mut()
                            .expect("Write pass starts one region")
                            .points
                            .insert(at.sample(), value);
                    }
                    AudioAutomationMode::Touch => {
                        if !pass.touch_active {
                            return Err(conflict(
                                "set_control_value",
                                "Touch mode records only while the control is touched",
                            ));
                        }
                        pass.active_touch_region
                            .as_mut()
                            .expect("active touch owns one region")
                            .points
                            .insert(at.sample(), value);
                    }
                    AudioAutomationMode::Latch => {
                        if !pass.touch_active {
                            return Err(conflict(
                                "set_control_value",
                                "Latch control changes require an active physical touch",
                            ));
                        }
                        pass.continuous_region
                            .as_mut()
                            .expect("touched Latch pass owns one region")
                            .points
                            .insert(at.sample(), value);
                    }
                    AudioAutomationMode::Read => unreachable!("Read cannot begin a pass"),
                }
                pass.current_value = value;
            }
            AudioAutomationMutation::EndTouch { target, at } => {
                let lane = self.lane_mut(target, "end_touch")?;
                require_clock(lane, at, "end_touch")?;
                let pass = lane.active_pass.as_mut().ok_or_else(|| {
                    conflict(
                        "end_touch",
                        "audio automation touch release requires an active pass",
                    )
                })?;
                if !matches!(
                    pass.mode,
                    AudioAutomationMode::Touch | AudioAutomationMode::Latch
                ) || !pass.touch_active
                {
                    return Err(conflict(
                        "end_touch",
                        "audio automation control is not currently touched",
                    ));
                }
                if pass.mode == AudioAutomationMode::Latch {
                    require_recordable_coordinate(at, "end_touch")?;
                }
                pass.advance(at.sample(), "end_touch")?;
                if pass.mode == AudioAutomationMode::Touch {
                    let region = pass
                        .active_touch_region
                        .take()
                        .expect("active touch owns one region")
                        .finish(at.sample(), "end_touch")?;
                    pass.completed_regions.push(region);
                }
                pass.touch_active = false;
            }
            AudioAutomationMutation::EndPass { target, at } => {
                let lane = self.lane_mut(target, "end_pass")?;
                require_clock(lane, at, "end_pass")?;
                let mut pass = lane
                    .active_pass
                    .take()
                    .ok_or_else(|| conflict("end_pass", "audio automation pass is not active"))?;
                pass.advance(at.sample(), "end_pass")?;
                if at.sample() <= pass.start {
                    return Err(conflict(
                        "end_pass",
                        "audio automation pass must contain at least one sample",
                    ));
                }
                match pass.mode {
                    AudioAutomationMode::Write => {
                        let region = pass
                            .continuous_region
                            .take()
                            .expect("Write pass owns one region")
                            .finish(at.sample(), "end_pass")?;
                        pass.completed_regions.push(region);
                    }
                    AudioAutomationMode::Touch => {
                        if let Some(region) = pass.active_touch_region.take() {
                            pass.completed_regions
                                .push(region.finish(at.sample(), "end_pass")?);
                        }
                    }
                    AudioAutomationMode::Latch => {
                        if let Some(region) = pass.continuous_region.take() {
                            pass.completed_regions
                                .push(region.finish(at.sample(), "end_pass")?);
                        }
                    }
                    AudioAutomationMode::Read => unreachable!("Read cannot begin a pass"),
                }
                let mut keyframes = pass.baseline.clone();
                for region in &pass.completed_regions {
                    splice_region(&mut keyframes, lane.default_gain, &pass.baseline, region)?;
                }
                lane.keyframes = keyframes;
            }
        }
        Ok(())
    }

    fn lane_mut(
        &mut self,
        target: AudioAutomationTarget,
        operation: &'static str,
    ) -> Result<&mut AutomationLane> {
        self.lanes
            .get_mut(&target)
            .ok_or_else(|| not_found(operation, "audio automation target does not own a lane"))
    }
}

impl AutomationLane {
    fn snapshot(&self, target: AudioAutomationTarget) -> AudioAutomationLaneSnapshot {
        let mut effective = self.keyframes.clone();
        if let Some(pass) = &self.active_pass {
            effective = pass.baseline.clone();
            for region in &pass.completed_regions {
                let _ = splice_region(&mut effective, self.default_gain, &pass.baseline, region);
            }
            let pending = match pass.mode {
                AudioAutomationMode::Write | AudioAutomationMode::Latch => {
                    pass.continuous_region.as_ref()
                }
                AudioAutomationMode::Touch => pass.active_touch_region.as_ref(),
                AudioAutomationMode::Read => None,
            };
            if let (Some(region), Some(end)) = (pending, pass.last_time.checked_add(1)) {
                if end > region.start {
                    let mut region = region.clone();
                    region.end = Some(end);
                    let _ =
                        splice_region(&mut effective, self.default_gain, &pass.baseline, &region);
                }
            }
        }
        AudioAutomationLaneSnapshot {
            target,
            sample_rate: self.sample_rate,
            default_gain: self.default_gain,
            mode: self.mode,
            keyframes: effective
                .into_iter()
                .map(|(sample, value)| AudioAutomationKeyframe {
                    at: SampleTime::new(sample, self.sample_rate)
                        .expect("lane retains a positive sample rate"),
                    value,
                })
                .collect(),
            active_pass: self
                .active_pass
                .as_ref()
                .map(|pass| AudioAutomationActivePassSnapshot {
                    start: SampleTime::new(pass.start, self.sample_rate)
                        .expect("lane retains a positive sample rate"),
                    current_value: pass.current_value,
                    touch_active: pass.touch_active,
                    latch_active: pass.mode == AudioAutomationMode::Latch
                        && pass.continuous_region.is_some(),
                }),
        }
    }
}

/// Bounded public state for one active write pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioAutomationActivePassSnapshot {
    start: SampleTime,
    current_value: f32,
    touch_active: bool,
    latch_active: bool,
}

impl Eq for AudioAutomationActivePassSnapshot {}

impl AudioAutomationActivePassSnapshot {
    /// Returns the first pass sample.
    #[must_use]
    pub const fn start(self) -> SampleTime {
        self.start
    }

    /// Returns the most recently accepted control value.
    #[must_use]
    pub const fn current_value(self) -> f32 {
        self.current_value
    }

    /// Returns whether the physical control is currently touched.
    #[must_use]
    pub const fn touch_active(self) -> bool {
        self.touch_active
    }

    /// Returns whether Latch has begun holding a touched value.
    #[must_use]
    pub const fn latch_active(self) -> bool {
        self.latch_active
    }
}

/// Complete immutable replacement state for one automation lane.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioAutomationLaneSnapshot {
    target: AudioAutomationTarget,
    sample_rate: u32,
    default_gain: f32,
    mode: AudioAutomationMode,
    keyframes: Vec<AudioAutomationKeyframe>,
    active_pass: Option<AudioAutomationActivePassSnapshot>,
}

impl Eq for AudioAutomationLaneSnapshot {}

impl AudioAutomationLaneSnapshot {
    /// Returns the typed parameter address.
    #[must_use]
    pub const fn target(&self) -> AudioAutomationTarget {
        self.target
    }

    /// Returns the fixed integral sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the value before the first keyframe.
    #[must_use]
    pub const fn default_gain(&self) -> f32 {
        self.default_gain
    }

    /// Returns the lane's professional automation mode.
    #[must_use]
    pub const fn mode(&self) -> AudioAutomationMode {
        self.mode
    }

    /// Returns effective points in exact signed sample order.
    #[must_use]
    pub fn keyframes(&self) -> &[AudioAutomationKeyframe] {
        &self.keyframes
    }

    /// Returns bounded active-pass state when one is in progress.
    #[must_use]
    pub const fn active_pass(&self) -> Option<AudioAutomationActivePassSnapshot> {
        self.active_pass
    }
}

/// Complete immutable replacement state for every authored automation lane.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioAutomationSnapshot {
    revision: u64,
    lanes: BTreeMap<AudioAutomationTarget, AudioAutomationLaneSnapshot>,
}

impl Eq for AudioAutomationSnapshot {}

impl AudioAutomationSnapshot {
    /// Returns the source authored-state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns one complete lane by typed target.
    #[must_use]
    pub fn lane(&self, target: AudioAutomationTarget) -> Option<&AudioAutomationLaneSnapshot> {
        self.lanes.get(&target)
    }

    /// Iterates complete lanes in deterministic target order.
    pub fn lanes(&self) -> impl ExactSizeIterator<Item = &AudioAutomationLaneSnapshot> {
        self.lanes.values()
    }

    /// Compiles one matching lane for allocation-free callback evaluation.
    pub fn prepare_curve(
        &self,
        target: AudioAutomationTarget,
        sample_rate: u32,
    ) -> Result<Option<PreparedAudioAutomationCurve>> {
        let Some(lane) = self.lanes.get(&target) else {
            return Ok(None);
        };
        if sample_rate != lane.sample_rate {
            return Err(invalid(
                "prepare_curve",
                "audio automation lane clock does not match the prepared processor",
            ));
        }
        Ok(Some(PreparedAudioAutomationCurve {
            sample_rate,
            default_gain: lane.default_gain,
            keyframes: lane
                .keyframes
                .iter()
                .map(|keyframe| (keyframe.at.sample(), keyframe.value))
                .collect(),
        }))
    }
}

/// Immutable prepared scalar curve evaluated from absolute sample coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedAudioAutomationCurve {
    sample_rate: u32,
    default_gain: f32,
    keyframes: Vec<(i64, f32)>,
}

impl Eq for PreparedAudioAutomationCurve {}

impl PreparedAudioAutomationCurve {
    /// Returns the fixed sample rate validated during preparation.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Reconstructs the finite gain at one exact absolute sample without allocation or mutation.
    #[must_use]
    pub fn gain_at_sample(&self, sample: i64) -> f32 {
        value_at_slice(self.default_gain, &self.keyframes, sample)
    }
}

fn require_idle(lane: &AutomationLane, operation: &'static str) -> Result<()> {
    if lane.active_pass.is_some() {
        return Err(conflict(
            operation,
            "audio automation lane cannot be edited directly during an active pass",
        ));
    }
    Ok(())
}

fn require_clock(lane: &AutomationLane, at: SampleTime, operation: &'static str) -> Result<()> {
    if at.sample_rate() != lane.sample_rate {
        return Err(invalid(
            operation,
            "audio automation mutation does not use the lane sample rate",
        ));
    }
    Ok(())
}

fn require_recordable_coordinate(at: SampleTime, operation: &'static str) -> Result<()> {
    if at.sample() == i64::MAX {
        return Err(invalid(
            operation,
            "audio automation write coordinate leaves no representable exclusive end",
        ));
    }
    Ok(())
}

fn splice_region(
    keyframes: &mut BTreeMap<i64, f32>,
    default_gain: f32,
    baseline: &BTreeMap<i64, f32>,
    region: &WriteRegion,
) -> Result<()> {
    let end = region.end.ok_or_else(|| {
        internal(
            "splice_region",
            "audio automation write region was not finalized",
        )
    })?;
    if end <= region.start || region.points.is_empty() {
        return Err(internal(
            "splice_region",
            "audio automation write region has invalid bounds",
        ));
    }
    let before = region
        .start
        .checked_sub(1)
        .map(|sample| (sample, value_at_map(default_gain, baseline, sample)));
    let restore = value_at_map(default_gain, baseline, end);
    keyframes.retain(|sample, _| *sample < region.start || *sample >= end);
    if let Some((sample, value)) = before {
        keyframes.insert(sample, value);
    }
    for (sample, value) in &region.points {
        if *sample < region.start || *sample >= end {
            return Err(conflict(
                "splice_region",
                "recorded audio automation point lies outside its write interval",
            ));
        }
        keyframes.insert(*sample, *value);
    }
    let last_value = *region
        .points
        .last_key_value()
        .expect("nonempty region was validated")
        .1;
    keyframes.insert(end - 1, last_value);
    keyframes.insert(end, restore);
    Ok(())
}

fn value_at_map(default_gain: f32, keyframes: &BTreeMap<i64, f32>, sample: i64) -> f32 {
    let previous = keyframes.range(..=sample).next_back();
    if let Some((at, value)) = previous {
        if *at == sample {
            return *value;
        }
    }
    let next = keyframes.range(sample..).next();
    match (previous, next) {
        (None, _) => default_gain,
        (Some((_, value)), None) => *value,
        (Some((left_at, left)), Some((right_at, right))) => {
            interpolate(*left_at, *left, *right_at, *right, sample)
        }
    }
}

fn value_at_slice(default_gain: f32, keyframes: &[(i64, f32)], sample: i64) -> f32 {
    match keyframes.binary_search_by_key(&sample, |(at, _)| *at) {
        Ok(index) => keyframes[index].1,
        Err(0) => default_gain,
        Err(index) if index == keyframes.len() => keyframes[index - 1].1,
        Err(index) => {
            let (left_at, left) = keyframes[index - 1];
            let (right_at, right) = keyframes[index];
            interpolate(left_at, left, right_at, right, sample)
        }
    }
}

fn interpolate(left_at: i64, left: f32, right_at: i64, right: f32, sample: i64) -> f32 {
    debug_assert!(left_at < sample && sample < right_at);
    let offset = (i128::from(sample) - i128::from(left_at)) as f64;
    let length = (i128::from(right_at) - i128::from(left_at)) as f64;
    (f64::from(left) + (f64::from(right) - f64::from(left)) * (offset / length)) as f32
}

fn validate_gain(value: f32, operation: &'static str) -> Result<()> {
    if !value.is_finite() || !(0.0..=MAX_GAIN).contains(&value) {
        return Err(invalid(
            operation,
            "audio automation gain must be finite and between 0 and 64",
        ));
    }
    Ok(())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    automation_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    automation_error(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}

fn not_found(operation: &'static str, message: &'static str) -> Error {
    automation_error(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}

fn resource_exhausted(operation: &'static str, message: &'static str) -> Error {
    automation_error(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    automation_error(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        operation,
        message,
    )
}

fn automation_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
