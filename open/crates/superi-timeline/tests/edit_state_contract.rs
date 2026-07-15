use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::edit_state::{SelectionExpansion, SelectionUpdate};
use superi_timeline::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting,
    AudioTrackSemantics, Clip, ClipSource, EditorialObjectId, EditorialProject,
    LinkedMediaReference, Timeline, Track, TrackItem, TrackKind, TrackSemantics, VideoCompositing,
    VideoTrackSemantics,
};

const MEDIA: MediaId = MediaId::from_raw(1);
const TIMELINE: TimelineId = TimelineId::from_raw(10);
const V1: TrackId = TrackId::from_raw(20);
const A1: TrackId = TrackId::from_raw(21);
const V2: TrackId = TrackId::from_raw(22);
const PICTURE: ClipId = ClipId::from_raw(30);
const SOUND: ClipId = ClipId::from_raw(31);
const BROLL: ClipId = ClipId::from_raw(32);

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

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
    let video_rate = Timebase::integer(24).unwrap();
    let source_rate = Timebase::integer(48).unwrap();
    let audio_rate = Timebase::integer(48_000).unwrap();
    let media = LinkedMediaReference::new(
        MEDIA,
        "camera original",
        "urn:superi:test:camera-original",
        None,
    );
    let picture = Clip::new(
        PICTURE,
        "picture",
        ClipSource::Media(MEDIA),
        range(48, 48, source_rate),
        range(0, 24, video_rate),
    );
    let sound = Clip::new(
        SOUND,
        "production sound",
        ClipSource::Media(MEDIA),
        range(48_000, 48_000, audio_rate),
        range(0, 48_000, audio_rate),
    );
    let broll = Clip::new(
        BROLL,
        "b roll",
        ClipSource::Media(MEDIA),
        range(96, 48, source_rate),
        range(0, 24, video_rate),
    );
    let timeline = Timeline::new(
        TIMELINE,
        "main edit",
        video_rate,
        RationalTime::zero(video_rate),
        vec![
            Track::new(V1, "V1", video_semantics(), vec![TrackItem::Clip(picture)]),
            Track::new(A1, "A1", audio_semantics(), vec![TrackItem::Clip(sound)]),
            Track::new(V2, "V2", video_semantics(), vec![TrackItem::Clip(broll)]),
        ],
    );
    EditorialProject::new(
        ProjectId::from_raw(100),
        "selection project",
        [media],
        [timeline],
    )
    .unwrap()
}

fn selected(project: &EditorialProject) -> Vec<EditorialObjectId> {
    project
        .timeline(TIMELINE)
        .unwrap()
        .edit_state()
        .selected_objects()
        .collect()
}

#[test]
fn linked_and_grouped_selection_expands_but_each_clip_remains_directly_controllable() {
    let mut project = project_fixture();
    project
        .edit(0, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.link_clips([PICTURE, SOUND])?;
            timeline.update_selection(
                [EditorialObjectId::Clip(PICTURE)],
                SelectionUpdate::Replace,
                SelectionExpansion::Related,
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        selected(&project),
        vec![
            EditorialObjectId::Clip(PICTURE),
            EditorialObjectId::Clip(SOUND),
        ]
    );

    project
        .edit(1, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.set_linked_selection_enabled(false);
            timeline.update_selection(
                [EditorialObjectId::Clip(PICTURE)],
                SelectionUpdate::Replace,
                SelectionExpansion::Related,
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(selected(&project), vec![EditorialObjectId::Clip(PICTURE)]);

    project
        .edit(2, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.set_linked_selection_enabled(true);
            timeline.group_clips([PICTURE, BROLL])?;
            timeline.update_selection(
                [EditorialObjectId::Clip(PICTURE)],
                SelectionUpdate::Replace,
                SelectionExpansion::Related,
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        selected(&project),
        vec![
            EditorialObjectId::Clip(PICTURE),
            EditorialObjectId::Clip(SOUND),
            EditorialObjectId::Clip(BROLL),
        ]
    );

    let state = project.timeline(TIMELINE).unwrap().edit_state();
    assert_eq!(
        state
            .link_for(PICTURE)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        vec![PICTURE, SOUND]
    );
    assert_eq!(
        state
            .group_for(SOUND)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        vec![PICTURE, SOUND, BROLL]
    );

    project
        .edit(3, |draft| {
            draft.timeline_mut(TIMELINE)?.update_selection(
                [EditorialObjectId::Clip(SOUND)],
                SelectionUpdate::Replace,
                SelectionExpansion::Direct,
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(selected(&project), vec![EditorialObjectId::Clip(SOUND)]);
}

#[test]
fn track_targets_and_sync_locks_preserve_layer_order_and_explicit_edit_tracks() {
    let mut project = project_fixture();
    project
        .edit(0, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.set_track_targeted(V2, true)?;
            timeline.set_track_targeted(V1, true)?;
            timeline.set_track_sync_locked(V1, false)?;
            timeline.set_track_sync_locked(A1, false)?;
            Ok(())
        })
        .unwrap();

    let timeline = project.timeline(TIMELINE).unwrap();
    assert_eq!(
        timeline
            .targeted_tracks()
            .map(Track::id)
            .collect::<Vec<_>>(),
        vec![V1, V2]
    );
    assert_eq!(
        timeline
            .targeted_tracks_by_kind(TrackKind::Video)
            .map(Track::id)
            .collect::<Vec<_>>(),
        vec![V1, V2]
    );
    assert_eq!(
        timeline
            .targeted_tracks_by_kind(TrackKind::Audio)
            .map(Track::id)
            .collect::<Vec<_>>(),
        Vec::<TrackId>::new()
    );
    assert_eq!(
        timeline.tracks_affected_by_sync([V1]).unwrap(),
        vec![V1, V2]
    );
    assert_eq!(
        timeline.tracks_affected_by_sync([A1]).unwrap(),
        vec![A1, V2]
    );
}

#[test]
fn selection_updates_and_relationship_removal_are_exact_and_independent() {
    let mut project = project_fixture();
    project
        .edit(0, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.group_clips([PICTURE, BROLL])?;
            timeline.link_clips([PICTURE, SOUND])?;
            timeline.update_selection(
                [EditorialObjectId::Clip(PICTURE)],
                SelectionUpdate::Replace,
                SelectionExpansion::Direct,
            )?;
            timeline.update_selection(
                [EditorialObjectId::Clip(BROLL)],
                SelectionUpdate::Add,
                SelectionExpansion::Direct,
            )?;
            timeline.update_selection(
                [EditorialObjectId::Clip(PICTURE)],
                SelectionUpdate::Remove,
                SelectionExpansion::Direct,
            )?;
            timeline.unlink_clips([SOUND])?;
            Ok(())
        })
        .unwrap();

    let state = project.timeline(TIMELINE).unwrap().edit_state();
    assert_eq!(
        state.selected_objects().collect::<Vec<_>>(),
        vec![EditorialObjectId::Clip(BROLL)]
    );
    assert_eq!(state.links().count(), 0);
    assert_eq!(state.groups().count(), 1);
    assert!(state.group_for(SOUND).is_some());

    project
        .edit(1, |draft| {
            draft.timeline_mut(TIMELINE)?.ungroup_clips([BROLL])?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        project
            .timeline(TIMELINE)
            .unwrap()
            .edit_state()
            .groups()
            .count(),
        0
    );
}

#[test]
fn structural_edits_keep_surviving_intent_and_preserve_clip_source_record_and_identity() {
    let mut project = project_fixture();
    let picture_before = project
        .timeline(TIMELINE)
        .unwrap()
        .track(V1)
        .unwrap()
        .item(EditorialObjectId::Clip(PICTURE))
        .unwrap()
        .as_clip()
        .unwrap()
        .clone();

    project
        .edit(0, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.link_clips([PICTURE, SOUND])?;
            timeline.group_clips([PICTURE, BROLL])?;
            timeline.set_track_targeted(A1, true)?;
            timeline.set_track_sync_locked(A1, false)?;
            timeline.update_selection(
                [EditorialObjectId::Clip(PICTURE)],
                SelectionUpdate::Replace,
                SelectionExpansion::Related,
            )?;
            Ok(())
        })
        .unwrap();

    project
        .edit(1, |draft| {
            draft
                .timeline_mut(TIMELINE)?
                .track_mut(A1)?
                .replace_items(Vec::new());
            Ok(())
        })
        .unwrap();

    let timeline = project.timeline(TIMELINE).unwrap();
    let state = timeline.edit_state();
    assert!(state.link_for(PICTURE).is_none());
    assert_eq!(
        state
            .group_for(PICTURE)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        vec![PICTURE, BROLL]
    );
    assert_eq!(
        state.selected_objects().collect::<Vec<_>>(),
        vec![
            EditorialObjectId::Clip(PICTURE),
            EditorialObjectId::Clip(BROLL),
        ]
    );
    assert!(state.track_state(A1).unwrap().targeted());
    assert!(!state.track_state(A1).unwrap().sync_locked());

    let picture_after = timeline
        .track(V1)
        .unwrap()
        .item(EditorialObjectId::Clip(PICTURE))
        .unwrap()
        .as_clip()
        .unwrap();
    assert_eq!(picture_after, &picture_before);
    assert_eq!(picture_after.id(), PICTURE);
    assert_eq!(picture_after.source(), ClipSource::Media(MEDIA));
    assert_eq!(picture_after.source_range(), picture_before.source_range());
    assert_eq!(picture_after.record_range(), picture_before.record_range());
}

#[test]
fn invalid_relationships_and_stale_revisions_roll_back_the_complete_project() {
    let mut project = project_fixture();
    let original = project.clone();
    let missing = ClipId::from_raw(999);

    let error = project
        .edit(0, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.set_track_targeted(V1, true)?;
            timeline.link_clips([PICTURE, missing])?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(project, original);

    project
        .edit(0, |draft| {
            draft
                .timeline_mut(TIMELINE)?
                .group_clips([PICTURE, BROLL])?;
            Ok(())
        })
        .unwrap();
    let published = project.clone();
    let stale = project
        .edit(0, |draft| {
            draft.timeline_mut(TIMELINE)?.ungroup_clips([PICTURE])?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(project, published);
}
