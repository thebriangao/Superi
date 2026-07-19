use superi_core::error::ErrorCategory;
use superi_core::ids::{GapId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::edit_ops::{apply_edit_batch, EditOperation};
use superi_timeline::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting,
    AudioTrackSemantics, CaptionPurpose, EditorialProject, Gap, Timeline, Track, TrackItem,
    TrackSemantics, VideoCompositing, VideoTrackSemantics,
};
use superi_timeline::track_ops::{
    apply_track_mutation_batch, TrackCreationKind, TrackMutation, TrackMutationKind,
    DEFAULT_TRACK_HEIGHT,
};

const TIMELINE: TimelineId = TimelineId::from_raw(10);
const V1: TrackId = TrackId::from_raw(20);
const A1: TrackId = TrackId::from_raw(21);
const V2: TrackId = TrackId::from_raw(22);
const A2: TrackId = TrackId::from_raw(23);
const C1: TrackId = TrackId::from_raw(24);
const D1: TrackId = TrackId::from_raw(25);

fn video_semantics() -> TrackSemantics {
    TrackSemantics::Video(VideoTrackSemantics::new(
        FrameRate::FPS_24,
        VideoCompositing::Over,
    ))
}

fn audio_semantics() -> TrackSemantics {
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
    )
    .unwrap();
    TrackSemantics::Audio(AudioTrackSemantics::new(48_000, layout, routing).unwrap())
}

fn project_fixture() -> EditorialProject {
    let rate = Timebase::integer(24).unwrap();
    EditorialProject::new(
        ProjectId::from_raw(100),
        "track controls",
        [],
        [Timeline::new(
            TIMELINE,
            "main edit",
            rate,
            RationalTime::zero(rate),
            vec![
                Track::new(V1, "V1", video_semantics(), vec![]),
                Track::new(A1, "A1", audio_semantics(), vec![]),
            ],
        )],
    )
    .unwrap()
}

#[test]
fn one_atomic_batch_preserves_identity_while_applying_every_track_control() {
    let mut project = project_fixture();
    let result = apply_track_mutation_batch(
        &mut project,
        0,
        &[
            TrackMutation::Create {
                timeline_id: TIMELINE,
                track_id: V2,
                name: "V2".to_owned(),
                kind: TrackCreationKind::Video,
                position: 1,
                height: 96,
            },
            TrackMutation::Rename {
                timeline_id: TIMELINE,
                track_id: V2,
                name: "Overlay".to_owned(),
            },
            TrackMutation::SetHeight {
                timeline_id: TIMELINE,
                track_id: V2,
                height: 110,
            },
            TrackMutation::SetTargeted {
                timeline_id: TIMELINE,
                track_id: V2,
                targeted: true,
            },
            TrackMutation::SetLocked {
                timeline_id: TIMELINE,
                track_id: V2,
                locked: true,
            },
            TrackMutation::SetSyncLocked {
                timeline_id: TIMELINE,
                track_id: V2,
                sync_locked: false,
            },
            TrackMutation::SetEnabled {
                timeline_id: TIMELINE,
                track_id: V2,
                enabled: false,
            },
            TrackMutation::SetMuted {
                timeline_id: TIMELINE,
                track_id: A1,
                muted: true,
            },
            TrackMutation::SetSolo {
                timeline_id: TIMELINE,
                track_id: A1,
                solo: true,
            },
            TrackMutation::Reorder {
                timeline_id: TIMELINE,
                track_id: V2,
                position: 2,
            },
        ],
    )
    .unwrap();

    assert_eq!(result.revision(), 1);
    assert_eq!(result.outcomes().len(), 10);
    let timeline = project.timeline(TIMELINE).unwrap();
    assert_eq!(
        timeline.tracks().iter().map(Track::id).collect::<Vec<_>>(),
        vec![V1, A1, V2]
    );
    assert_eq!(timeline.track(V2).unwrap().name(), "Overlay");
    let video_state = timeline.edit_state().track_state(V2).unwrap();
    assert_eq!(video_state.height(), 110);
    assert!(video_state.targeted());
    assert!(video_state.locked());
    assert!(!video_state.sync_locked());
    assert!(!video_state.enabled());
    assert!(!video_state.muted());
    assert!(!video_state.solo());
    let audio_state = timeline.edit_state().track_state(A1).unwrap();
    assert_eq!(audio_state.height(), DEFAULT_TRACK_HEIGHT);
    assert!(audio_state.muted());
    assert!(audio_state.solo());
}

#[test]
fn every_creation_kind_uses_explicit_canonical_semantics_and_position() {
    let mut project = project_fixture();
    apply_track_mutation_batch(
        &mut project,
        0,
        &[
            TrackMutation::Create {
                timeline_id: TIMELINE,
                track_id: V2,
                name: "V2".to_owned(),
                kind: TrackCreationKind::Video,
                position: 2,
                height: 72,
            },
            TrackMutation::Create {
                timeline_id: TIMELINE,
                track_id: A2,
                name: "A2".to_owned(),
                kind: TrackCreationKind::Audio,
                position: 3,
                height: 80,
            },
            TrackMutation::Create {
                timeline_id: TIMELINE,
                track_id: C1,
                name: "C1".to_owned(),
                kind: TrackCreationKind::Caption,
                position: 4,
                height: 88,
            },
            TrackMutation::SetCaptionSemantics {
                timeline_id: TIMELINE,
                track_id: C1,
                language: "fr-FR".to_owned(),
                purpose: CaptionPurpose::Subtitles,
            },
            TrackMutation::Create {
                timeline_id: TIMELINE,
                track_id: D1,
                name: "D1".to_owned(),
                kind: TrackCreationKind::Data,
                position: 5,
                height: 96,
            },
        ],
    )
    .unwrap();

    let timeline = project.timeline(TIMELINE).unwrap();
    assert_eq!(
        timeline.tracks().iter().map(Track::id).collect::<Vec<_>>(),
        vec![V1, A1, V2, A2, C1, D1]
    );
    assert!(matches!(
        timeline.track(V2).unwrap().semantics(),
        TrackSemantics::Video(_)
    ));
    assert!(matches!(
        timeline.track(A2).unwrap().semantics(),
        TrackSemantics::Audio(_)
    ));
    let TrackSemantics::Caption(caption) = timeline.track(C1).unwrap().semantics() else {
        panic!("caption creation must retain caption semantics");
    };
    assert_eq!(caption.language().as_str(), "fr-fr");
    assert_eq!(caption.purpose(), CaptionPurpose::Subtitles);
    assert_eq!(
        timeline.track(C1).unwrap().semantics().timebase(),
        Timebase::MILLISECONDS
    );
    assert!(matches!(
        timeline.track(D1).unwrap().semantics(),
        TrackSemantics::Data(_)
    ));
    assert_eq!(timeline.edit_state().track_state(V2).unwrap().height(), 72);
    assert_eq!(timeline.edit_state().track_state(A2).unwrap().height(), 80);
    assert_eq!(timeline.edit_state().track_state(C1).unwrap().height(), 88);
    assert_eq!(timeline.edit_state().track_state(D1).unwrap().height(), 96);
}

#[test]
fn locked_deletion_is_atomic_and_unlock_then_delete_is_explicit() {
    let mut project = project_fixture();
    apply_track_mutation_batch(
        &mut project,
        0,
        &[TrackMutation::SetLocked {
            timeline_id: TIMELINE,
            track_id: V1,
            locked: true,
        }],
    )
    .unwrap();

    let before = project.clone();
    let error = apply_track_mutation_batch(
        &mut project,
        1,
        &[TrackMutation::Delete {
            timeline_id: TIMELINE,
            track_id: V1,
        }],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(project, before);

    apply_track_mutation_batch(
        &mut project,
        1,
        &[
            TrackMutation::SetLocked {
                timeline_id: TIMELINE,
                track_id: V1,
                locked: false,
            },
            TrackMutation::Delete {
                timeline_id: TIMELINE,
                track_id: V1,
            },
        ],
    )
    .unwrap();
    assert_eq!(project.revision(), 2);
    assert!(project.timeline(TIMELINE).unwrap().track(V1).is_none());
}

#[test]
fn audio_only_controls_and_height_bounds_reject_invalid_intent() {
    let mut project = project_fixture();
    let mute_error = apply_track_mutation_batch(
        &mut project,
        0,
        &[TrackMutation::SetMuted {
            timeline_id: TIMELINE,
            track_id: V1,
            muted: true,
        }],
    )
    .unwrap_err();
    assert_eq!(mute_error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project.revision(), 0);

    let height_error = apply_track_mutation_batch(
        &mut project,
        0,
        &[TrackMutation::SetHeight {
            timeline_id: TIMELINE,
            track_id: V1,
            height: 1,
        }],
    )
    .unwrap_err();
    assert_eq!(height_error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project.revision(), 0);

    let caption_semantics_error = apply_track_mutation_batch(
        &mut project,
        0,
        &[TrackMutation::SetCaptionSemantics {
            timeline_id: TIMELINE,
            track_id: V1,
            language: "fr-FR".to_owned(),
            purpose: CaptionPurpose::Subtitles,
        }],
    )
    .unwrap_err();
    assert_eq!(
        caption_semantics_error.category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(project.revision(), 0);
}

#[test]
fn authored_item_edits_reject_a_locked_target_without_publishing() {
    let mut project = project_fixture();
    apply_track_mutation_batch(
        &mut project,
        0,
        &[TrackMutation::SetLocked {
            timeline_id: TIMELINE,
            track_id: V1,
            locked: true,
        }],
    )
    .unwrap();
    let before = project.clone();
    let rate = Timebase::integer(24).unwrap();
    let error = apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::Append {
            timeline_id: TIMELINE,
            track_id: V1,
            material: TrackItem::Gap(Gap::new(
                GapId::from_raw(40),
                "locked gap",
                TimeRange::new(RationalTime::zero(rate), Duration::new(24, rate).unwrap()).unwrap(),
            )),
        }],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(project, before);
}

#[test]
fn complete_audio_routing_replacement_preserves_source_meaning_and_is_atomic() {
    let mut project = project_fixture();
    let swapped = AudioRouting::new(
        AudioRouteDestination::Main,
        ChannelLayout::stereo(),
        [
            AudioChannelRoute::new(
                ChannelPosition::FrontLeft,
                AudioChannelTarget::Channel(ChannelPosition::FrontRight),
            ),
            AudioChannelRoute::new(ChannelPosition::FrontRight, AudioChannelTarget::Muted),
        ],
    )
    .unwrap();
    let result = apply_track_mutation_batch(
        &mut project,
        0,
        &[TrackMutation::SetAudioRouting {
            timeline_id: TIMELINE,
            track_id: A1,
            routing: swapped.clone(),
        }],
    )
    .unwrap();

    assert_eq!(
        result.outcomes()[0].kind(),
        TrackMutationKind::SetAudioRouting
    );
    let TrackSemantics::Audio(audio) = project
        .timeline(TIMELINE)
        .unwrap()
        .track(A1)
        .unwrap()
        .semantics()
    else {
        panic!("A1 must remain an audio track");
    };
    assert_eq!(audio.sample_rate(), 48_000);
    assert_eq!(audio.channel_layout(), &ChannelLayout::stereo());
    assert_eq!(audio.routing(), &swapped);

    let before = project.clone();
    let incomplete = AudioRouting::new(
        AudioRouteDestination::Main,
        ChannelLayout::stereo(),
        [AudioChannelRoute::new(
            ChannelPosition::FrontLeft,
            AudioChannelTarget::Channel(ChannelPosition::FrontLeft),
        )],
    )
    .unwrap();
    assert_eq!(
        apply_track_mutation_batch(
            &mut project,
            1,
            &[TrackMutation::SetAudioRouting {
                timeline_id: TIMELINE,
                track_id: A1,
                routing: incomplete,
            }],
        )
        .unwrap_err()
        .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(project, before);

    for destination in [
        AudioRouteDestination::Track(A1),
        AudioRouteDestination::Track(V1),
    ] {
        let invalid_destination = AudioRouting::new(
            destination,
            ChannelLayout::stereo(),
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
        )
        .unwrap();
        assert_eq!(
            apply_track_mutation_batch(
                &mut project,
                1,
                &[TrackMutation::SetAudioRouting {
                    timeline_id: TIMELINE,
                    track_id: A1,
                    routing: invalid_destination,
                }],
            )
            .unwrap_err()
            .category(),
            ErrorCategory::InvalidInput
        );
        assert_eq!(project, before);
    }
}
