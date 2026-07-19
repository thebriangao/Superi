//! Atomic multicam source, switching, refinement, and audio-intent mutations.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::ids::{ClipId, TimelineId};
use superi_core::time::{RationalTime, TimeRange, TimeRounding};

use crate::ids::MulticamAngleId;
use crate::model::{Clip, ClipSource, EditorialProject, ProjectDraft};
use crate::multicam::{MulticamAudioPolicy, MulticamClip, MulticamSource, MulticamSyncMethod};

/// One canonical authored multicam mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MulticamMutation {
    /// Creates or replaces complete synchronized angle state on one source timeline.
    SetSource {
        timeline_id: TimelineId,
        source: MulticamSource,
    },
    /// Replaces synchronization provenance without changing the angle catalog.
    SetSyncMethod {
        timeline_id: TimelineId,
        sync_method: MulticamSyncMethod,
    },
    /// Attaches an initial switch program to one ordinary nested timeline clip.
    AttachClip {
        timeline_id: TimelineId,
        clip_id: ClipId,
        initial_angle_id: MulticamAngleId,
        audio_policy: MulticamAudioPolicy,
    },
    /// Switches one angle from an exact target record point through its current interval end.
    SwitchAt {
        timeline_id: TimelineId,
        clip_id: ClipId,
        record_time: RationalTime,
        angle_id: MulticamAngleId,
    },
    /// Moves one existing switch cut using exact target record coordinates.
    MoveCut {
        timeline_id: TimelineId,
        clip_id: ClipId,
        at_record_time: RationalTime,
        to_record_time: RationalTime,
    },
    /// Replaces explicit multicam audio-selection intent.
    SetAudioPolicy {
        timeline_id: TimelineId,
        clip_id: ClipId,
        audio_policy: MulticamAudioPolicy,
    },
    /// Removes authored multicam state from one ordinary nested clip.
    DetachClip {
        timeline_id: TimelineId,
        clip_id: ClipId,
    },
}

impl MulticamMutation {
    /// Returns the timeline directly addressed by this mutation.
    #[must_use]
    pub const fn timeline_id(&self) -> TimelineId {
        match self {
            Self::SetSource { timeline_id, .. }
            | Self::SetSyncMethod { timeline_id, .. }
            | Self::AttachClip { timeline_id, .. }
            | Self::SwitchAt { timeline_id, .. }
            | Self::MoveCut { timeline_id, .. }
            | Self::SetAudioPolicy { timeline_id, .. }
            | Self::DetachClip { timeline_id, .. } => *timeline_id,
        }
    }

    /// Returns the target clip identity when this is a clip-local mutation.
    #[must_use]
    pub const fn clip_id(&self) -> Option<ClipId> {
        match self {
            Self::SetSource { .. } | Self::SetSyncMethod { .. } => None,
            Self::AttachClip { clip_id, .. }
            | Self::SwitchAt { clip_id, .. }
            | Self::MoveCut { clip_id, .. }
            | Self::SetAudioPolicy { clip_id, .. }
            | Self::DetachClip { clip_id, .. } => Some(*clip_id),
        }
    }

    const fn kind(&self) -> MulticamMutationKind {
        match self {
            Self::SetSource { .. } => MulticamMutationKind::SetSource,
            Self::SetSyncMethod { .. } => MulticamMutationKind::SetSyncMethod,
            Self::AttachClip { .. } => MulticamMutationKind::AttachClip,
            Self::SwitchAt { .. } => MulticamMutationKind::SwitchAt,
            Self::MoveCut { .. } => MulticamMutationKind::MoveCut,
            Self::SetAudioPolicy { .. } => MulticamMutationKind::SetAudioPolicy,
            Self::DetachClip { .. } => MulticamMutationKind::DetachClip,
        }
    }
}

/// Stable category for one applied multicam mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MulticamMutationKind {
    SetSource,
    SetSyncMethod,
    AttachClip,
    SwitchAt,
    MoveCut,
    SetAudioPolicy,
    DetachClip,
}

/// Semantic result for one mutation in an atomic batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MulticamMutationOutcome {
    timeline_id: TimelineId,
    clip_id: Option<ClipId>,
    kind: MulticamMutationKind,
}

impl MulticamMutationOutcome {
    #[must_use]
    pub const fn timeline_id(self) -> TimelineId {
        self.timeline_id
    }

    #[must_use]
    pub const fn clip_id(self) -> Option<ClipId> {
        self.clip_id
    }

    #[must_use]
    pub const fn kind(self) -> MulticamMutationKind {
        self.kind
    }
}

/// Results published together at one editorial project revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MulticamMutationBatchResult {
    revision: u64,
    outcomes: Vec<MulticamMutationOutcome>,
}

impl MulticamMutationBatchResult {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn outcomes(&self) -> &[MulticamMutationOutcome] {
        &self.outcomes
    }
}

/// Applies a nonempty ordered multicam mutation batch atomically.
pub fn apply_multicam_mutation_batch(
    project: &mut EditorialProject,
    expected_revision: u64,
    mutations: &[MulticamMutation],
) -> Result<MulticamMutationBatchResult> {
    if mutations.is_empty() {
        return Err(multicam_error(
            ErrorCategory::InvalidInput,
            "apply_multicam_mutation_batch",
            "a multicam mutation batch must contain at least one mutation",
        ));
    }
    let mut outcomes = Vec::with_capacity(mutations.len());
    project.edit(expected_revision, |draft| {
        for (index, mutation) in mutations.iter().enumerate() {
            apply_mutation(draft, mutation).with_error_context(
                ErrorContext::new("superi-timeline.multicam-ops", "apply_multicam_mutation")
                    .with_field("index", index.to_string())
                    .with_field("timeline", mutation.timeline_id().to_string())
                    .with_field(
                        "clip",
                        mutation
                            .clip_id()
                            .map_or_else(|| "none".to_owned(), |id| id.to_string()),
                    ),
            )?;
            outcomes.push(MulticamMutationOutcome {
                timeline_id: mutation.timeline_id(),
                clip_id: mutation.clip_id(),
                kind: mutation.kind(),
            });
        }
        Ok(())
    })?;
    Ok(MulticamMutationBatchResult {
        revision: project.revision(),
        outcomes,
    })
}

fn apply_mutation(draft: &mut ProjectDraft, mutation: &MulticamMutation) -> Result<()> {
    match mutation {
        MulticamMutation::SetSource {
            timeline_id,
            source,
        } => draft
            .timeline_mut(*timeline_id)?
            .set_multicam_source(source.clone())
            .map(drop),
        MulticamMutation::SetSyncMethod {
            timeline_id,
            sync_method,
        } => draft
            .timeline_mut(*timeline_id)?
            .multicam_source_mut()?
            .set_sync_method(sync_method.clone()),
        MulticamMutation::AttachClip {
            timeline_id,
            clip_id,
            initial_angle_id,
            audio_policy,
        } => {
            let target = target_clip(draft, *timeline_id, *clip_id)?;
            let ClipSource::Timeline(source_timeline_id) = target.source() else {
                return Err(multicam_error(
                    ErrorCategory::InvalidInput,
                    "attach_multicam_clip",
                    "multicam target must be an ordinary nested timeline clip",
                ));
            };
            let source = draft.timeline(source_timeline_id).ok_or_else(|| {
                multicam_error(
                    ErrorCategory::NotFound,
                    "attach_multicam_clip",
                    "multicam source timeline was not found",
                )
            })?;
            let source_range =
                TimeRange::new(RationalTime::zero(source.edit_rate()), source.duration()?)?;
            let clip = MulticamClip::new(
                *clip_id,
                source_range,
                *initial_angle_id,
                audio_policy.clone(),
            )?;
            draft
                .timeline_mut(*timeline_id)?
                .upsert_multicam_clip(clip)?;
            Ok(())
        }
        MulticamMutation::SwitchAt {
            timeline_id,
            clip_id,
            record_time,
            angle_id,
        } => {
            let target = target_clip(draft, *timeline_id, *clip_id)?;
            let source_time = target
                .source_time_at(*record_time, TimeRounding::Exact)?
                .time();
            let multicam = draft
                .timeline(*timeline_id)
                .and_then(|timeline| timeline.multicam_clip(*clip_id))
                .ok_or_else(|| {
                    multicam_error(
                        ErrorCategory::NotFound,
                        "switch_multicam_angle",
                        "multicam clip state was not found",
                    )
                })?;
            if !multicam.source_range().contains(source_time)? {
                return Err(multicam_error(
                    ErrorCategory::InvalidInput,
                    "switch_multicam_angle",
                    "target record point lies outside the multicam switch program",
                ));
            }
            let mut interval_end = None;
            for authored_switch in multicam.switches() {
                if authored_switch.source_range().contains(source_time)? {
                    interval_end = Some(authored_switch.source_range().end_exclusive()?);
                    break;
                }
            }
            let interval_end = interval_end.ok_or_else(|| {
                multicam_error(
                    ErrorCategory::InvalidInput,
                    "switch_multicam_angle",
                    "multicam switch program does not completely cover its source range",
                )
            })?;
            let source_range = TimeRange::from_start_end(source_time, interval_end)?;
            draft
                .timeline_mut(*timeline_id)?
                .multicam_clip_mut(*clip_id)?
                .switch_range(source_range, *angle_id)
        }
        MulticamMutation::MoveCut {
            timeline_id,
            clip_id,
            at_record_time,
            to_record_time,
        } => {
            let target = target_clip(draft, *timeline_id, *clip_id)?;
            let at = target
                .source_time_at(*at_record_time, TimeRounding::Exact)?
                .time();
            let to = target
                .source_time_at(*to_record_time, TimeRounding::Exact)?
                .time();
            draft
                .timeline_mut(*timeline_id)?
                .multicam_clip_mut(*clip_id)?
                .move_cut(at, to)
        }
        MulticamMutation::SetAudioPolicy {
            timeline_id,
            clip_id,
            audio_policy,
        } => {
            draft
                .timeline_mut(*timeline_id)?
                .multicam_clip_mut(*clip_id)?
                .set_audio_policy(audio_policy.clone());
            Ok(())
        }
        MulticamMutation::DetachClip {
            timeline_id,
            clip_id,
        } => draft
            .timeline_mut(*timeline_id)?
            .remove_multicam_clip(*clip_id)
            .map(drop)
            .ok_or_else(|| {
                multicam_error(
                    ErrorCategory::NotFound,
                    "detach_multicam_clip",
                    "multicam clip state was not found",
                )
            }),
    }
}

fn target_clip(draft: &ProjectDraft, timeline_id: TimelineId, clip_id: ClipId) -> Result<Clip> {
    draft
        .timeline(timeline_id)
        .ok_or_else(|| {
            multicam_error(
                ErrorCategory::NotFound,
                "find_multicam_target",
                "multicam target timeline was not found",
            )
        })?
        .tracks()
        .iter()
        .flat_map(|track| track.items())
        .find_map(|item| item.as_clip().filter(|clip| clip.id() == clip_id))
        .cloned()
        .ok_or_else(|| {
            multicam_error(
                ErrorCategory::NotFound,
                "find_multicam_target",
                "multicam target clip was not found",
            )
        })
}

fn multicam_error(
    category: ErrorCategory,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new("superi-timeline.multicam-ops", operation))
}
