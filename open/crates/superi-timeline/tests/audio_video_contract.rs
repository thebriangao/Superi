use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, GapId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::edit_ops::{apply_edit_batch, EditKind, EditOperation};
use superi_timeline::edit_state::{SelectionExpansion, SelectionUpdate};
use superi_timeline::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting,
    AudioTrackSemantics, Clip, ClipSource, EditorialObjectId, EditorialProject, Gap,
    LinkedMediaReference, Timeline, Track, TrackItem, TrackSemantics, VideoCompositing,
    VideoTrackSemantics,
};

const PROJECT: ProjectId = ProjectId::from_raw(0xa000);
const MEDIA: MediaId = MediaId::from_raw(0xa001);
const TIMELINE: TimelineId = TimelineId::from_raw(0xa002);
const VIDEO_TRACK: TrackId = TrackId::from_raw(0xa003);
const AUDIO_TRACK: TrackId = TrackId::from_raw(0xa004);
const VIDEO_CLIP: ClipId = ClipId::from_raw(0xa005);
const AUDIO_CLIP: ClipId = ClipId::from_raw(0xa006);
const REPLACEMENT_AUDIO: ClipId = ClipId::from_raw(0xa007);

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
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
    let video_rate = FrameRate::FPS_24.timebase();
    let audio_rate = Timebase::integer(48_000).unwrap();
    let video = Clip::new(
        VIDEO_CLIP,
        "picture",
        ClipSource::Media(MEDIA),
        range(48, 24, video_rate),
        range(24, 24, video_rate),
    )
    .unwrap();
    let audio = Clip::new(
        AUDIO_CLIP,
        "production audio",
        ClipSource::Media(MEDIA),
        range(0, 48_000, audio_rate),
        range(96_000, 48_000, audio_rate),
    )
    .unwrap();
    let timeline = Timeline::new(
        TIMELINE,
        "audio video edit",
        video_rate,
        RationalTime::zero(video_rate),
        vec![
            Track::new(
                VIDEO_TRACK,
                "V1",
                TrackSemantics::Video(VideoTrackSemantics::new(
                    FrameRate::FPS_24,
                    VideoCompositing::Over,
                )),
                vec![
                    TrackItem::Gap(Gap::new(
                        GapId::from_raw(0xa010),
                        "picture lead",
                        range(0, 24, video_rate),
                    )),
                    TrackItem::Clip(video),
                ],
            ),
            Track::new(
                AUDIO_TRACK,
                "A1",
                audio_semantics(),
                vec![
                    TrackItem::Gap(Gap::new(
                        GapId::from_raw(0xa011),
                        "audio lead",
                        range(0, 96_000, audio_rate),
                    )),
                    TrackItem::Clip(audio),
                ],
            ),
        ],
    );
    EditorialProject::new(
        PROJECT,
        "audio video contract",
        [LinkedMediaReference::new(
            MEDIA,
            "camera source",
            "urn:superi:test:audio-video",
            None,
        )],
        [timeline],
    )
    .unwrap()
}

fn audio_clip(project: &EditorialProject, id: ClipId) -> &Clip {
    project
        .timeline(TIMELINE)
        .unwrap()
        .track(AUDIO_TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(id))
        .unwrap()
        .as_clip()
        .unwrap()
}

#[test]
fn link_and_detach_publish_exact_relationship_intent() {
    let mut project = project_fixture();
    let linked = apply_edit_batch(
        &mut project,
        0,
        &[EditOperation::link_audio_video(
            TIMELINE,
            VIDEO_TRACK,
            VIDEO_CLIP,
            AUDIO_TRACK,
            AUDIO_CLIP,
        )],
    )
    .unwrap();

    assert_eq!(linked.outcomes()[0].kind(), EditKind::LinkAudioVideo);
    assert_eq!(linked.outcomes()[0].track_id(), AUDIO_TRACK);
    assert_eq!(linked.outcomes()[0].synchronized_tracks(), &[VIDEO_TRACK]);
    let relation = project
        .timeline(TIMELINE)
        .unwrap()
        .edit_state()
        .link_for(AUDIO_CLIP)
        .unwrap();
    assert_eq!(
        relation.members().collect::<Vec<_>>(),
        vec![VIDEO_CLIP, AUDIO_CLIP]
    );

    let detached = apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::detach_audio(
            TIMELINE,
            VIDEO_TRACK,
            VIDEO_CLIP,
            AUDIO_TRACK,
            AUDIO_CLIP,
        )],
    )
    .unwrap();
    assert_eq!(detached.outcomes()[0].kind(), EditKind::DetachAudio);
    assert!(project
        .timeline(TIMELINE)
        .unwrap()
        .edit_state()
        .link_for(AUDIO_CLIP)
        .is_none());
}

#[test]
fn synchronization_translates_only_audio_source_time_and_links_atomically() {
    let mut project = project_fixture();
    let original_record = audio_clip(&project, AUDIO_CLIP).record_range();
    let original_duration = audio_clip(&project, AUDIO_CLIP).source_range().duration();

    let synchronized = apply_edit_batch(
        &mut project,
        0,
        &[EditOperation::synchronize_audio_video(
            TIMELINE,
            VIDEO_TRACK,
            VIDEO_CLIP,
            AUDIO_TRACK,
            AUDIO_CLIP,
        )],
    )
    .unwrap();

    assert_eq!(
        synchronized.outcomes()[0].kind(),
        EditKind::SynchronizeAudioVideo
    );
    let audio = audio_clip(&project, AUDIO_CLIP);
    assert_eq!(audio.record_range(), original_record);
    assert_eq!(audio.source_range().duration(), original_duration);
    assert_eq!(audio.source_range().start().value(), 144_000);
    assert_eq!(
        audio.time_map().segments()[0].source_start().value(),
        144_000
    );
    assert!(project
        .timeline(TIMELINE)
        .unwrap()
        .edit_state()
        .link_for(AUDIO_CLIP)
        .is_some());
}

#[test]
fn audio_replacement_transfers_selection_link_group_and_multisystem_identity() {
    let mut project = project_fixture();
    project
        .edit(0, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.link_clips([VIDEO_CLIP, AUDIO_CLIP])?;
            timeline.group_clips([VIDEO_CLIP, AUDIO_CLIP])?;
            timeline.update_selection(
                [EditorialObjectId::Clip(AUDIO_CLIP)],
                SelectionUpdate::Replace,
                SelectionExpansion::Direct,
            )?;
            Ok(())
        })
        .unwrap();
    let prior = audio_clip(&project, AUDIO_CLIP).clone();
    let replacement = Clip::new(
        REPLACEMENT_AUDIO,
        "replacement audio",
        prior.source(),
        prior.source_range(),
        prior.record_range(),
    )
    .unwrap();

    apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::replace(
            TIMELINE,
            AUDIO_TRACK,
            EditorialObjectId::Clip(AUDIO_CLIP),
            TrackItem::Clip(replacement),
        )],
    )
    .unwrap();

    let timeline = project.timeline(TIMELINE).unwrap();
    assert!(timeline
        .edit_state()
        .selected_objects()
        .any(|id| id == EditorialObjectId::Clip(REPLACEMENT_AUDIO)));
    assert!(!timeline
        .edit_state()
        .selected_objects()
        .any(|id| id == EditorialObjectId::Clip(AUDIO_CLIP)));
    assert!(timeline
        .edit_state()
        .link_for(REPLACEMENT_AUDIO)
        .unwrap()
        .contains(VIDEO_CLIP));
    assert!(timeline
        .edit_state()
        .group_for(REPLACEMENT_AUDIO)
        .unwrap()
        .contains(VIDEO_CLIP));
}

#[test]
fn wrong_roles_and_locked_tracks_reject_without_partial_publication() {
    let mut project = project_fixture();
    let before = project.clone();
    let wrong_roles =
        EditOperation::link_audio_video(TIMELINE, AUDIO_TRACK, AUDIO_CLIP, VIDEO_TRACK, VIDEO_CLIP);
    assert_eq!(
        apply_edit_batch(&mut project, 0, &[wrong_roles])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(project, before);

    project
        .edit(0, |draft| {
            draft
                .timeline_mut(TIMELINE)?
                .set_track_locked(AUDIO_TRACK, true)
        })
        .unwrap();
    let locked = project.clone();
    let error = apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::synchronize_audio_video(
            TIMELINE,
            VIDEO_TRACK,
            VIDEO_CLIP,
            AUDIO_TRACK,
            AUDIO_CLIP,
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(project, locked);
}
