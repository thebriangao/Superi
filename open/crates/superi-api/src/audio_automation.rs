//! Stable transport-neutral authored audio automation adapter.

use std::str::FromStr;

use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::ClipId;
use superi_core::settings::SemanticVersion;
use superi_core::time::SampleTime;
use superi_engine::dispatcher::{
    AudioAutomationKeyframe as EngineKeyframe, AudioAutomationMode as EngineMode,
    AudioAutomationMutation as EngineMutation, AudioAutomationSnapshot as EngineSnapshot,
    AudioAutomationTarget as EngineTarget, AudioAutomationTransaction as EngineTransaction,
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineTransactionId,
};

use crate::commands::{
    ExecuteAudioAutomationTransaction, ExecuteAudioAutomationTransactionResult, GetAudioAutomation,
    GetAudioAutomationResult,
};
use crate::events::AudioAutomationChanged;
use crate::version::AUDIO_AUTOMATION_SCHEMA_VERSION;

const COMPONENT: &str = "superi-api.audio-automation";

/// One exact signed sample coordinate and its integral sample clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioAutomationSampleTime {
    sample: i64,
    sample_rate: u32,
}

impl AudioAutomationSampleTime {
    /// Creates one explicit public sample coordinate.
    #[must_use]
    pub const fn new(sample: i64, sample_rate: u32) -> Self {
        Self {
            sample,
            sample_rate,
        }
    }

    /// Returns the signed sample coordinate.
    #[must_use]
    pub const fn sample(self) -> i64 {
        self.sample
    }

    /// Returns the integral sample clock.
    #[must_use]
    pub const fn sample_rate(self) -> u32 {
        self.sample_rate
    }

    fn into_engine(self) -> Result<SampleTime> {
        SampleTime::new(self.sample, self.sample_rate)
    }

    fn from_engine(value: SampleTime) -> Self {
        Self::new(value.sample(), value.sample_rate())
    }
}

/// One strict typed audio automation parameter address.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum AudioAutomationTarget {
    /// Linear gain for one canonical editorial clip identity.
    ClipGain {
        /// Canonical `clip:` identifier text.
        #[serde(deserialize_with = "deserialize_clip_id")]
        clip_id: String,
    },
}

impl AudioAutomationTarget {
    fn into_engine(self) -> Result<EngineTarget> {
        match self {
            Self::ClipGain { clip_id } => Ok(EngineTarget::clip_gain(parse_clip_id(&clip_id)?)),
        }
    }

    fn from_engine(value: EngineTarget) -> Result<Self> {
        match value {
            EngineTarget::ClipGain { clip_id } => Ok(Self::ClipGain {
                clip_id: clip_id.to_string(),
            }),
            _ => Err(unsupported_engine_value("convert_target")),
        }
    }
}

/// Professional playback and recording behavior for one automation lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum AudioAutomationMode {
    /// Evaluate existing authored keyframes without recording.
    Read,
    /// Replace the complete played pass with recorded control values.
    Write,
    /// Replace only intervals while the control is physically touched.
    Touch,
    /// Hold the last touched value until the pass ends.
    Latch,
}

impl AudioAutomationMode {
    const fn into_engine(self) -> EngineMode {
        match self {
            Self::Read => EngineMode::Read,
            Self::Write => EngineMode::Write,
            Self::Touch => EngineMode::Touch,
            Self::Latch => EngineMode::Latch,
        }
    }

    fn from_engine(value: EngineMode) -> Result<Self> {
        match value {
            EngineMode::Read => Ok(Self::Read),
            EngineMode::Write => Ok(Self::Write),
            EngineMode::Touch => Ok(Self::Touch),
            EngineMode::Latch => Ok(Self::Latch),
            _ => Err(unsupported_engine_value("convert_mode")),
        }
    }
}

/// One finite clip-gain value at an exact sample coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioAutomationKeyframe {
    at: AudioAutomationSampleTime,
    value: f32,
}

impl Eq for AudioAutomationKeyframe {}

impl AudioAutomationKeyframe {
    /// Creates one public keyframe whose value is validated at engine conversion.
    #[must_use]
    pub const fn new(at: AudioAutomationSampleTime, value: f32) -> Self {
        Self { at, value }
    }

    /// Returns the exact sample coordinate.
    #[must_use]
    pub const fn at(self) -> AudioAutomationSampleTime {
        self.at
    }

    /// Returns the linear gain value.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.value
    }

    fn into_engine(self) -> Result<EngineKeyframe> {
        EngineKeyframe::new(self.at.into_engine()?, self.value)
    }

    fn from_engine(value: EngineKeyframe) -> Self {
        Self::new(
            AudioAutomationSampleTime::from_engine(value.at()),
            value.value(),
        )
    }
}

/// One strict ordered mutation in the permanent public automation vocabulary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum AudioAutomationMutation {
    /// Creates one new authored lane in Read mode.
    CreateLane {
        target: AudioAutomationTarget,
        sample_rate: u32,
        default_gain: f32,
    },
    /// Removes one idle authored lane.
    RemoveLane { target: AudioAutomationTarget },
    /// Changes professional automation mode on one idle lane.
    SetMode {
        target: AudioAutomationTarget,
        mode: AudioAutomationMode,
    },
    /// Inserts or replaces one exact keyframe.
    SetKeyframe {
        target: AudioAutomationTarget,
        keyframe: AudioAutomationKeyframe,
    },
    /// Removes one exact keyframe.
    RemoveKeyframe {
        target: AudioAutomationTarget,
        at: AudioAutomationSampleTime,
    },
    /// Begins one Write, Touch, or Latch pass.
    BeginPass {
        target: AudioAutomationTarget,
        at: AudioAutomationSampleTime,
        current_value: f32,
    },
    /// Begins physical manipulation in Touch or Latch mode.
    BeginTouch {
        target: AudioAutomationTarget,
        at: AudioAutomationSampleTime,
        value: f32,
    },
    /// Records one ordered control value during a writable interval.
    SetControlValue {
        target: AudioAutomationTarget,
        at: AudioAutomationSampleTime,
        value: f32,
    },
    /// Releases physical manipulation in Touch or Latch mode.
    EndTouch {
        target: AudioAutomationTarget,
        at: AudioAutomationSampleTime,
    },
    /// Ends one active write pass.
    EndPass {
        target: AudioAutomationTarget,
        at: AudioAutomationSampleTime,
    },
}

impl Eq for AudioAutomationMutation {}

impl AudioAutomationMutation {
    fn into_engine(self) -> Result<EngineMutation> {
        match self {
            Self::CreateLane {
                target,
                sample_rate,
                default_gain,
            } => Ok(EngineMutation::CreateLane {
                target: target.into_engine()?,
                sample_rate,
                default_gain,
            }),
            Self::RemoveLane { target } => Ok(EngineMutation::RemoveLane {
                target: target.into_engine()?,
            }),
            Self::SetMode { target, mode } => Ok(EngineMutation::SetMode {
                target: target.into_engine()?,
                mode: mode.into_engine(),
            }),
            Self::SetKeyframe { target, keyframe } => Ok(EngineMutation::SetKeyframe {
                target: target.into_engine()?,
                keyframe: keyframe.into_engine()?,
            }),
            Self::RemoveKeyframe { target, at } => Ok(EngineMutation::RemoveKeyframe {
                target: target.into_engine()?,
                at: at.into_engine()?,
            }),
            Self::BeginPass {
                target,
                at,
                current_value,
            } => Ok(EngineMutation::BeginPass {
                target: target.into_engine()?,
                at: at.into_engine()?,
                current_value,
            }),
            Self::BeginTouch { target, at, value } => Ok(EngineMutation::BeginTouch {
                target: target.into_engine()?,
                at: at.into_engine()?,
                value,
            }),
            Self::SetControlValue { target, at, value } => Ok(EngineMutation::SetControlValue {
                target: target.into_engine()?,
                at: at.into_engine()?,
                value,
            }),
            Self::EndTouch { target, at } => Ok(EngineMutation::EndTouch {
                target: target.into_engine()?,
                at: at.into_engine()?,
            }),
            Self::EndPass { target, at } => Ok(EngineMutation::EndPass {
                target: target.into_engine()?,
                at: at.into_engine()?,
            }),
        }
    }
}

/// Bounded state for one active automation write pass.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioAutomationActivePassSnapshot {
    start: AudioAutomationSampleTime,
    current_value: f32,
    touch_active: bool,
    latch_active: bool,
}

impl Eq for AudioAutomationActivePassSnapshot {}

impl AudioAutomationActivePassSnapshot {
    /// Returns the first pass sample.
    #[must_use]
    pub const fn start(self) -> AudioAutomationSampleTime {
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

    /// Returns whether Latch is holding a touched value.
    #[must_use]
    pub const fn latch_active(self) -> bool {
        self.latch_active
    }
}

/// Complete strict replacement state for one authored automation lane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    pub const fn target(&self) -> &AudioAutomationTarget {
        &self.target
    }

    /// Returns the fixed lane sample clock.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the value before the first keyframe.
    #[must_use]
    pub const fn default_gain(&self) -> f32 {
        self.default_gain
    }

    /// Returns the professional playback and recording mode.
    #[must_use]
    pub const fn mode(&self) -> AudioAutomationMode {
        self.mode
    }

    /// Returns effective keyframes in exact signed sample order.
    #[must_use]
    pub fn keyframes(&self) -> &[AudioAutomationKeyframe] {
        &self.keyframes
    }

    /// Returns bounded active pass state when one is in progress.
    #[must_use]
    pub const fn active_pass(&self) -> Option<AudioAutomationActivePassSnapshot> {
        self.active_pass
    }
}

/// Complete strict public replacement snapshot for authored audio automation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioAutomationSnapshot {
    schema_version: SemanticVersion,
    revision: u64,
    lanes: Vec<AudioAutomationLaneSnapshot>,
}

impl AudioAutomationSnapshot {
    /// Returns the permanent public schema version.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns the authored automation revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns complete lanes in deterministic typed target order.
    #[must_use]
    pub fn lanes(&self) -> &[AudioAutomationLaneSnapshot] {
        &self.lanes
    }

    /// Returns one complete lane by its typed public target.
    #[must_use]
    pub fn lane(&self, target: &AudioAutomationTarget) -> Option<&AudioAutomationLaneSnapshot> {
        self.lanes.iter().find(|lane| lane.target() == target)
    }
}

/// Mutable public facade around one full engine dispatcher with attached automation state.
pub struct AudioAutomationApi {
    dispatcher: EngineCommandDispatcher,
}

impl AudioAutomationApi {
    /// Takes ownership of a dispatcher with one attached automation owner.
    pub fn new(dispatcher: EngineCommandDispatcher) -> Result<Self> {
        let snapshot = dispatcher.audio_automation_snapshot()?;
        let _ = public_snapshot(&snapshot)?;
        Ok(Self { dispatcher })
    }

    /// Executes the stable complete automation query through the engine dispatcher.
    pub fn execute(&mut self, _command: GetAudioAutomation) -> Result<GetAudioAutomationResult> {
        let outcome = self.dispatcher.dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("audio-automation-get")?,
            EngineCommand::InspectAudioAutomation,
        ))?;
        let EngineCommandResult::AudioAutomation(snapshot) = outcome.result() else {
            return Err(unexpected_result("get_audio_automation"));
        };
        Ok(GetAudioAutomationResult::new(public_snapshot(snapshot)?))
    }

    /// Executes one strict optimistic automation transaction through the engine owner.
    pub fn execute_transaction(
        &mut self,
        command: ExecuteAudioAutomationTransaction,
    ) -> Result<ExecuteAudioAutomationTransactionResult> {
        let (transaction_id, expected_revision, mutations) = command.into_parts();
        let mutations = mutations
            .into_iter()
            .map(AudioAutomationMutation::into_engine)
            .collect::<Result<Vec<_>>>()?;
        let transaction = EngineTransaction::new(expected_revision, mutations)?;
        let transaction_id = EngineTransactionId::new(transaction_id)?;
        let outcome = self.dispatcher.dispatch(EngineCommandRequest::new(
            transaction_id,
            EngineCommand::ExecuteAudioAutomation(transaction),
        ))?;
        let EngineCommandResult::AudioAutomation(snapshot) = outcome.result() else {
            return Err(unexpected_result("execute_audio_automation_transaction"));
        };
        Ok(ExecuteAudioAutomationTransactionResult::new(
            outcome.transaction_id().as_str().to_owned(),
            outcome.command_sequence(),
            public_snapshot(snapshot)?,
        ))
    }

    /// Drains ordered full replacement authored automation events.
    pub fn drain_events(&mut self) -> Result<Vec<AudioAutomationChanged>> {
        self.dispatcher
            .drain_events()?
            .into_iter()
            .map(|envelope| match envelope.event() {
                EngineEvent::AudioAutomationStateChanged(snapshot) => {
                    let revision = envelope.audio_automation_revision().ok_or_else(|| {
                        Error::new(
                            ErrorCategory::Internal,
                            Recoverability::Terminal,
                            "audio automation event omitted its automation revision",
                        )
                        .with_context(ErrorContext::new(COMPONENT, "drain_events"))
                    })?;
                    Ok(AudioAutomationChanged::new(
                        envelope.sequence(),
                        envelope.command_sequence(),
                        envelope.transaction_id().as_str().to_owned(),
                        revision,
                        public_snapshot(snapshot)?,
                    ))
                }
                _ => Err(Error::new(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "audio automation API received an unrelated engine event",
                )
                .with_context(ErrorContext::new(COMPONENT, "drain_events"))),
            })
            .collect()
    }
}

impl std::fmt::Debug for AudioAutomationApi {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AudioAutomationApi")
            .finish_non_exhaustive()
    }
}

pub(crate) fn public_snapshot(snapshot: &EngineSnapshot) -> Result<AudioAutomationSnapshot> {
    let lanes = snapshot
        .lanes()
        .map(|lane| {
            Ok(AudioAutomationLaneSnapshot {
                target: AudioAutomationTarget::from_engine(lane.target())?,
                sample_rate: lane.sample_rate(),
                default_gain: lane.default_gain(),
                mode: AudioAutomationMode::from_engine(lane.mode())?,
                keyframes: lane
                    .keyframes()
                    .iter()
                    .copied()
                    .map(AudioAutomationKeyframe::from_engine)
                    .collect(),
                active_pass: lane
                    .active_pass()
                    .map(|pass| AudioAutomationActivePassSnapshot {
                        start: AudioAutomationSampleTime::from_engine(pass.start()),
                        current_value: pass.current_value(),
                        touch_active: pass.touch_active(),
                        latch_active: pass.latch_active(),
                    }),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AudioAutomationSnapshot {
        schema_version: AUDIO_AUTOMATION_SCHEMA_VERSION.clone(),
        revision: snapshot.revision(),
        lanes,
    })
}

fn deserialize_clip_id<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    parse_clip_id(&value).map_err(D::Error::custom)?;
    Ok(value)
}

fn parse_clip_id(value: &str) -> Result<ClipId> {
    ClipId::from_str(value).map_err(|error| {
        Error::new(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "audio automation target requires a canonical clip identifier",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "parse_clip_id").with_field("reason", error.to_string()),
        )
    })
}

fn unsupported_engine_value(operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "engine audio automation value is unsupported by this public schema",
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unexpected_result(operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "audio automation dispatcher returned an unrelated result",
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
