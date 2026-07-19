//! Atomic track creation, deletion, naming, ordering, and control-state mutations.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::ids::{TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::FrameRate;

pub use crate::edit_state::{DEFAULT_TRACK_HEIGHT, MAX_TRACK_HEIGHT, MIN_TRACK_HEIGHT};
use crate::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting,
    AudioTrackSemantics, CaptionPurpose, CaptionTrackSemantics, DataSchema, DataTrackSemantics,
    EditorialProject, LanguageTag, ProjectDraft, Track, TrackSemantics, VideoCompositing,
    VideoTrackSemantics,
};

/// User-facing semantic template for one empty track.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TrackCreationKind {
    /// Visual content at the timeline edit rate.
    Video,
    /// Stereo audio at 48 kHz with direct main-output routing.
    Audio,
    /// English caption content on a millisecond exchange clock.
    Caption,
    /// Superi generic timed data at the timeline edit rate.
    Data,
}

/// One canonical authored track mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TrackMutation {
    /// Creates one empty track at a canonical bottom-to-top position.
    Create {
        timeline_id: TimelineId,
        track_id: TrackId,
        name: String,
        kind: TrackCreationKind,
        position: usize,
        height: u16,
    },
    /// Deletes one unlocked track and its contained objects.
    Delete {
        timeline_id: TimelineId,
        track_id: TrackId,
    },
    /// Replaces one track's editor-facing name.
    Rename {
        timeline_id: TimelineId,
        track_id: TrackId,
        name: String,
    },
    /// Replaces one track's bounded editor lane height.
    SetHeight {
        timeline_id: TimelineId,
        track_id: TrackId,
        height: u16,
    },
    /// Replaces the language and purpose of one caption track without changing its clock.
    SetCaptionSemantics {
        timeline_id: TimelineId,
        track_id: TrackId,
        language: String,
        purpose: CaptionPurpose,
    },
    /// Moves one track to a canonical bottom-to-top final position.
    Reorder {
        timeline_id: TimelineId,
        track_id: TrackId,
        position: usize,
    },
    /// Changes command targeting intent.
    SetTargeted {
        timeline_id: TimelineId,
        track_id: TrackId,
        targeted: bool,
    },
    /// Changes authored-item lock intent.
    SetLocked {
        timeline_id: TimelineId,
        track_id: TrackId,
        locked: bool,
    },
    /// Changes synchronization lock intent.
    SetSyncLocked {
        timeline_id: TimelineId,
        track_id: TrackId,
        sync_locked: bool,
    },
    /// Changes audio mute intent.
    SetMuted {
        timeline_id: TimelineId,
        track_id: TrackId,
        muted: bool,
    },
    /// Changes audio solo intent.
    SetSolo {
        timeline_id: TimelineId,
        track_id: TrackId,
        solo: bool,
    },
    /// Replaces complete source-channel routing intent for one audio track.
    SetAudioRouting {
        timeline_id: TimelineId,
        track_id: TrackId,
        routing: AudioRouting,
    },
    /// Changes output enable intent.
    SetEnabled {
        timeline_id: TimelineId,
        track_id: TrackId,
        enabled: bool,
    },
}

impl TrackMutation {
    /// Returns the containing timeline identity.
    #[must_use]
    pub const fn timeline_id(&self) -> TimelineId {
        match self {
            Self::Create { timeline_id, .. }
            | Self::Delete { timeline_id, .. }
            | Self::Rename { timeline_id, .. }
            | Self::SetHeight { timeline_id, .. }
            | Self::SetCaptionSemantics { timeline_id, .. }
            | Self::Reorder { timeline_id, .. }
            | Self::SetTargeted { timeline_id, .. }
            | Self::SetLocked { timeline_id, .. }
            | Self::SetSyncLocked { timeline_id, .. }
            | Self::SetMuted { timeline_id, .. }
            | Self::SetSolo { timeline_id, .. }
            | Self::SetAudioRouting { timeline_id, .. }
            | Self::SetEnabled { timeline_id, .. } => *timeline_id,
        }
    }

    /// Returns the stable track identity addressed by this mutation.
    #[must_use]
    pub const fn track_id(&self) -> TrackId {
        match self {
            Self::Create { track_id, .. }
            | Self::Delete { track_id, .. }
            | Self::Rename { track_id, .. }
            | Self::SetHeight { track_id, .. }
            | Self::SetCaptionSemantics { track_id, .. }
            | Self::Reorder { track_id, .. }
            | Self::SetTargeted { track_id, .. }
            | Self::SetLocked { track_id, .. }
            | Self::SetSyncLocked { track_id, .. }
            | Self::SetMuted { track_id, .. }
            | Self::SetSolo { track_id, .. }
            | Self::SetAudioRouting { track_id, .. }
            | Self::SetEnabled { track_id, .. } => *track_id,
        }
    }

    const fn kind(&self) -> TrackMutationKind {
        match self {
            Self::Create { .. } => TrackMutationKind::Create,
            Self::Delete { .. } => TrackMutationKind::Delete,
            Self::Rename { .. } => TrackMutationKind::Rename,
            Self::SetHeight { .. } => TrackMutationKind::SetHeight,
            Self::SetCaptionSemantics { .. } => TrackMutationKind::SetCaptionSemantics,
            Self::Reorder { .. } => TrackMutationKind::Reorder,
            Self::SetTargeted { .. } => TrackMutationKind::SetTargeted,
            Self::SetLocked { .. } => TrackMutationKind::SetLocked,
            Self::SetSyncLocked { .. } => TrackMutationKind::SetSyncLocked,
            Self::SetMuted { .. } => TrackMutationKind::SetMuted,
            Self::SetSolo { .. } => TrackMutationKind::SetSolo,
            Self::SetAudioRouting { .. } => TrackMutationKind::SetAudioRouting,
            Self::SetEnabled { .. } => TrackMutationKind::SetEnabled,
        }
    }
}

/// Stable category for one applied track mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TrackMutationKind {
    Create,
    Delete,
    Rename,
    SetHeight,
    SetCaptionSemantics,
    Reorder,
    SetTargeted,
    SetLocked,
    SetSyncLocked,
    SetMuted,
    SetSolo,
    SetAudioRouting,
    SetEnabled,
}

/// Semantic result for one mutation in an atomic batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrackMutationOutcome {
    timeline_id: TimelineId,
    track_id: TrackId,
    kind: TrackMutationKind,
}

impl TrackMutationOutcome {
    #[must_use]
    pub const fn timeline_id(self) -> TimelineId {
        self.timeline_id
    }

    #[must_use]
    pub const fn track_id(self) -> TrackId {
        self.track_id
    }

    #[must_use]
    pub const fn kind(self) -> TrackMutationKind {
        self.kind
    }
}

/// Results published together at one editorial project revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackMutationBatchResult {
    revision: u64,
    outcomes: Vec<TrackMutationOutcome>,
}

impl TrackMutationBatchResult {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn outcomes(&self) -> &[TrackMutationOutcome] {
        &self.outcomes
    }
}

/// Applies a nonempty ordered track mutation batch atomically.
pub fn apply_track_mutation_batch(
    project: &mut EditorialProject,
    expected_revision: u64,
    mutations: &[TrackMutation],
) -> Result<TrackMutationBatchResult> {
    if mutations.is_empty() {
        return Err(track_error(
            ErrorCategory::InvalidInput,
            "apply_track_mutation_batch",
            "a track mutation batch must contain at least one mutation",
        ));
    }
    let mut outcomes = Vec::with_capacity(mutations.len());
    project.edit(expected_revision, |draft| {
        for (index, mutation) in mutations.iter().enumerate() {
            apply_mutation(draft, mutation).with_error_context(
                ErrorContext::new("superi-timeline.track-ops", "apply_track_mutation")
                    .with_field("index", index.to_string())
                    .with_field("track", mutation.track_id().to_string()),
            )?;
            outcomes.push(TrackMutationOutcome {
                timeline_id: mutation.timeline_id(),
                track_id: mutation.track_id(),
                kind: mutation.kind(),
            });
        }
        Ok(())
    })?;
    Ok(TrackMutationBatchResult {
        revision: project.revision(),
        outcomes,
    })
}

fn apply_mutation(draft: &mut ProjectDraft, mutation: &TrackMutation) -> Result<()> {
    let timeline = draft.timeline_mut(mutation.timeline_id())?;
    match mutation {
        TrackMutation::Create {
            track_id,
            name,
            kind,
            position,
            height,
            ..
        } => {
            let semantics = default_semantics(*kind, timeline.edit_rate())?;
            timeline.insert_track(
                *position,
                Track::new(*track_id, name.clone(), semantics, Vec::new()),
                *height,
            )
        }
        TrackMutation::Delete { track_id, .. } => timeline.remove_track(*track_id).map(drop),
        TrackMutation::Rename { track_id, name, .. } => {
            timeline.track_mut(*track_id)?.set_name(name.clone());
            Ok(())
        }
        TrackMutation::SetHeight {
            track_id, height, ..
        } => timeline.set_track_height(*track_id, *height),
        TrackMutation::SetCaptionSemantics {
            track_id,
            language,
            purpose,
            ..
        } => {
            let track = timeline.track_mut(*track_id)?;
            let TrackSemantics::Caption(current) = track.semantics() else {
                return Err(track_error(
                    ErrorCategory::InvalidInput,
                    "set_caption_semantics",
                    "caption semantics can be set only on a caption track",
                ));
            };
            let timebase = current.timebase();
            track.set_semantics(TrackSemantics::Caption(CaptionTrackSemantics::new(
                timebase,
                LanguageTag::new(language.clone())?,
                *purpose,
            )));
            Ok(())
        }
        TrackMutation::Reorder {
            track_id, position, ..
        } => timeline.reorder_track(*track_id, *position),
        TrackMutation::SetTargeted {
            track_id, targeted, ..
        } => timeline.set_track_targeted(*track_id, *targeted),
        TrackMutation::SetLocked {
            track_id, locked, ..
        } => timeline.set_track_locked(*track_id, *locked),
        TrackMutation::SetSyncLocked {
            track_id,
            sync_locked,
            ..
        } => timeline.set_track_sync_locked(*track_id, *sync_locked),
        TrackMutation::SetMuted {
            track_id, muted, ..
        } => timeline.set_track_muted(*track_id, *muted),
        TrackMutation::SetSolo { track_id, solo, .. } => timeline.set_track_solo(*track_id, *solo),
        TrackMutation::SetAudioRouting {
            track_id, routing, ..
        } => {
            if let AudioRouteDestination::Track(destination) = routing.destination() {
                if destination == *track_id {
                    return Err(track_error(
                        ErrorCategory::InvalidInput,
                        "set_audio_routing",
                        "an audio track cannot route into itself",
                    ));
                }
                let destination_track = timeline.track(destination).ok_or_else(|| {
                    track_error(
                        ErrorCategory::NotFound,
                        "set_audio_routing",
                        "the routed destination track was not found",
                    )
                })?;
                if destination_track.kind() != crate::model::TrackKind::Audio {
                    return Err(track_error(
                        ErrorCategory::InvalidInput,
                        "set_audio_routing",
                        "audio routing destinations must be audio tracks",
                    ));
                }
            }
            let source_track = timeline.track(*track_id).ok_or_else(|| {
                track_error(
                    ErrorCategory::NotFound,
                    "set_audio_routing",
                    "the audio track was not found",
                )
            })?;
            let replacement = match source_track.semantics() {
                TrackSemantics::Audio(audio) => {
                    if audio.routing() == routing {
                        return Err(track_error(
                            ErrorCategory::InvalidInput,
                            "set_audio_routing",
                            "audio routing replacement must change routing intent",
                        ));
                    }
                    TrackSemantics::Audio(audio.with_routing(routing.clone())?)
                }
                _ => {
                    return Err(track_error(
                        ErrorCategory::InvalidInput,
                        "set_audio_routing",
                        "audio routing is supported only for audio tracks",
                    ));
                }
            };
            timeline.track_mut(*track_id)?.set_semantics(replacement);
            Ok(())
        }
        TrackMutation::SetEnabled {
            track_id, enabled, ..
        } => timeline.set_track_enabled(*track_id, *enabled),
    }
}

fn default_semantics(
    kind: TrackCreationKind,
    edit_rate: superi_core::time::Timebase,
) -> Result<TrackSemantics> {
    match kind {
        TrackCreationKind::Video => Ok(TrackSemantics::Video(VideoTrackSemantics::new(
            FrameRate::from_timebase(edit_rate),
            VideoCompositing::Over,
        ))),
        TrackCreationKind::Audio => {
            let layout = ChannelLayout::stereo();
            let routing = AudioRouting::new(
                AudioRouteDestination::Main,
                layout.clone(),
                [
                    AudioChannelRoute::new(
                        ChannelPosition::FrontLeft,
                        AudioChannelTarget::Channel(ChannelPosition::FrontLeft),
                    ),
                    AudioChannelRoute::new(
                        ChannelPosition::FrontRight,
                        AudioChannelTarget::Channel(ChannelPosition::FrontRight),
                    ),
                ],
            )?;
            Ok(TrackSemantics::Audio(AudioTrackSemantics::new(
                48_000, layout, routing,
            )?))
        }
        TrackCreationKind::Caption => Ok(TrackSemantics::Caption(CaptionTrackSemantics::new(
            superi_core::time::Timebase::MILLISECONDS,
            LanguageTag::new("en-US")?,
            CaptionPurpose::Captions,
        ))),
        TrackCreationKind::Data => Ok(TrackSemantics::Data(DataTrackSemantics::new(
            edit_rate,
            DataSchema::new("urn:superi:track:data", None)?,
        ))),
    }
}

fn track_error(category: ErrorCategory, operation: &'static str, message: &'static str) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new("superi-timeline.track-ops", operation))
}
