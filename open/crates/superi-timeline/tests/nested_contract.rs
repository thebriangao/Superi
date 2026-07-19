use superi_core::error::ErrorCategory;
use superi_core::ids::{
    ClipId, GapId, MarkerId, MediaId, ProjectId, TimelineId, TrackId, TransitionId,
};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::edit_state::{SelectionExpansion, SelectionUpdate};
use superi_timeline::markers::{
    Marker, MarkerOwner, MetadataKey, MetadataOwner, MetadataValue, TimelineMetadata,
};
use superi_timeline::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting,
    AudioTrackSemantics, Clip, ClipSource, EditorialObjectId, EditorialProject, Gap,
    LinkedMediaReference, Timeline, Track, TrackItem, TrackSemantics, Transition, VideoCompositing,
    VideoTrackSemantics,
};
use superi_timeline::nested::{
    create_compound_clip, create_compound_clip_from_selection, edit_nested_sequence,
    nested_sequence_instances, nested_sequence_tree, place_nested_sequence, CompoundClipRequest,
    CompoundClipTrackRequest, NestedSequencePlacement, NestedSequenceRequest,
};

const MEDIA: MediaId = MediaId::from_raw(1);
const CHILD: TimelineId = TimelineId::from_raw(10);
const PARENT: TimelineId = TimelineId::from_raw(11);
const SECOND_PARENT: TimelineId = TimelineId::from_raw(12);
const GRANDCHILD: TimelineId = TimelineId::from_raw(13);
const CHILD_TRACK: TrackId = TrackId::from_raw(20);
const PARENT_TRACK: TrackId = TrackId::from_raw(21);
const SECOND_PARENT_TRACK: TrackId = TrackId::from_raw(22);
const GRANDCHILD_TRACK: TrackId = TrackId::from_raw(23);

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn video_semantics(frame_rate: FrameRate) -> TrackSemantics {
    TrackSemantics::Video(VideoTrackSemantics::new(frame_rate, VideoCompositing::Over))
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

fn media() -> LinkedMediaReference {
    LinkedMediaReference::new(
        MEDIA,
        "camera",
        "urn:superi:test:camera",
        Some(range(0, 960, Timebase::integer(48).unwrap())),
    )
}

fn media_clip(id: ClipId, record: TimeRange) -> TrackItem {
    let source_rate = Timebase::integer(48).unwrap();
    let source_duration = record
        .duration()
        .checked_rescale(source_rate, superi_core::time::TimeRounding::Exact)
        .unwrap();
    TrackItem::Clip(
        Clip::new(
            id,
            format!("clip {}", id.raw()),
            ClipSource::Media(MEDIA),
            TimeRange::new(RationalTime::zero(source_rate), source_duration).unwrap(),
            record,
        )
        .unwrap(),
    )
}

fn gap_track(
    timeline_id: TimelineId,
    track_id: TrackId,
    gap_id: GapId,
    duration: u64,
    frame_rate: FrameRate,
) -> Timeline {
    let rate = frame_rate.timebase();
    Timeline::new(
        timeline_id,
        format!("timeline {}", timeline_id.raw()),
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            track_id,
            "V1",
            video_semantics(frame_rate),
            vec![TrackItem::Gap(Gap::new(
                gap_id,
                "content",
                range(0, duration, rate),
            ))],
        )],
    )
}

fn nested_clip(
    id: ClipId,
    source: TimelineId,
    source_range: TimeRange,
    record_range: TimeRange,
) -> TrackItem {
    TrackItem::Clip(
        Clip::new(
            id,
            format!("nested {}", id.raw()),
            ClipSource::Timeline(source),
            source_range,
            record_range,
        )
        .unwrap(),
    )
}

#[test]
fn existing_nested_sequence_placement_reuses_exact_insert_semantics() {
    let child_rate = Timebase::integer(48).unwrap();
    let parent_rate = FrameRate::FPS_24.timebase();
    let child = gap_track(
        CHILD,
        CHILD_TRACK,
        GapId::from_raw(30),
        96,
        FrameRate::FPS_48,
    );
    let parent_clip = ClipId::from_raw(40);
    let parent = Timeline::new(
        PARENT,
        "parent",
        parent_rate,
        RationalTime::zero(parent_rate),
        vec![Track::new(
            PARENT_TRACK,
            "V1",
            video_semantics(FrameRate::FPS_24),
            vec![media_clip(parent_clip, range(0, 48, parent_rate))],
        )],
    );
    let mut project = EditorialProject::new(
        ProjectId::from_raw(100),
        "nested",
        [media()],
        [child, parent],
    )
    .unwrap();
    let nested_id = ClipId::from_raw(41);
    let right_id = ClipId::from_raw(42);
    let request = NestedSequenceRequest::new(
        PARENT,
        PARENT_TRACK,
        nested_id,
        "child instance",
        range(24, 48, child_rate),
        NestedSequencePlacement::insert(
            RationalTime::new(12, parent_rate),
            [EditorialObjectId::Clip(right_id)],
        ),
    );

    let result = place_nested_sequence(&mut project, 0, CHILD, request).unwrap();

    assert_eq!(project.revision(), 1);
    assert_eq!(result.revision(), 1);
    assert_eq!(result.source_timeline_id(), CHILD);
    assert_eq!(
        result.outcome().affected_range(),
        range(12, 24, parent_rate)
    );
    assert_eq!(
        result.outcome().inserted_ids(),
        &[EditorialObjectId::Clip(nested_id)]
    );
    assert_eq!(
        result.outcome().fragments()[0].created(),
        EditorialObjectId::Clip(right_id)
    );

    let placed = project
        .timeline(PARENT)
        .unwrap()
        .track(PARENT_TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(nested_id))
        .unwrap()
        .as_clip()
        .unwrap();
    assert_eq!(placed.source(), ClipSource::Timeline(CHILD));
    assert_eq!(placed.source_range(), range(24, 48, child_rate));
    assert_eq!(placed.record_range(), range(12, 24, parent_rate));
    assert_eq!(nested_sequence_instances(&project, CHILD).len(), 1);
}

#[test]
fn compound_creation_preserves_child_objects_relationships_and_command_state() {
    let rate = FrameRate::FPS_24.timebase();
    let left = ClipId::from_raw(50);
    let right = ClipId::from_raw(51);
    let mut child = Timeline::new(
        CHILD,
        "compound source",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            CHILD_TRACK,
            "compound V1",
            video_semantics(FrameRate::FPS_24),
            vec![
                media_clip(left, range(0, 12, rate)),
                media_clip(right, range(12, 12, rate)),
            ],
        )],
    );
    child.link_clips([left, right]).unwrap();
    child.group_clips([left, right]).unwrap();
    child.set_track_targeted(CHILD_TRACK, true).unwrap();
    child.set_track_sync_locked(CHILD_TRACK, false).unwrap();
    child
        .update_selection(
            [EditorialObjectId::Clip(left)],
            SelectionUpdate::Replace,
            SelectionExpansion::Related,
        )
        .unwrap();

    let target_gap = GapId::from_raw(52);
    let parent = gap_track(PARENT, PARENT_TRACK, target_gap, 24, FrameRate::FPS_24);
    let mut project =
        EditorialProject::new(ProjectId::from_raw(101), "compound", [media()], [parent]).unwrap();
    let compound_id = ClipId::from_raw(53);
    let request = NestedSequenceRequest::new(
        PARENT,
        PARENT_TRACK,
        compound_id,
        "compound clip",
        range(0, 24, rate),
        NestedSequencePlacement::replace(EditorialObjectId::Gap(target_gap)),
    );

    let result = create_compound_clip(&mut project, 0, child, request).unwrap();

    assert_eq!(result.revision(), 1);
    assert_eq!(result.source_timeline_id(), CHILD);
    assert_eq!(
        result.outcome().removed_ids(),
        &[EditorialObjectId::Gap(target_gap)]
    );
    let child = project.timeline(CHILD).unwrap();
    let state = child.edit_state();
    assert_eq!(
        state.selected_objects().collect::<Vec<_>>(),
        vec![
            EditorialObjectId::Clip(left),
            EditorialObjectId::Clip(right)
        ]
    );
    assert_eq!(
        state.link_for(left).unwrap().members().collect::<Vec<_>>(),
        vec![left, right]
    );
    assert_eq!(
        state
            .group_for(right)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        vec![left, right]
    );
    assert!(state.track_state(CHILD_TRACK).unwrap().targeted());
    assert!(!state.track_state(CHILD_TRACK).unwrap().sync_locked());
    assert!(child
        .track(CHILD_TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(left))
        .is_some());

    let instance = &nested_sequence_instances(&project, CHILD)[0];
    assert_eq!(instance.parent_timeline_id(), PARENT);
    assert_eq!(instance.parent_track_id(), PARENT_TRACK);
    assert_eq!(instance.clip_id(), compound_id);
    assert_eq!(instance.source_range(), range(0, 24, rate));
    assert_eq!(instance.record_range(), range(0, 24, rate));
}

#[test]
fn selection_compound_moves_authored_identity_into_one_exact_nested_instance() {
    let rate = FrameRate::FPS_24.timebase();
    let left = ClipId::from_raw(54);
    let right = ClipId::from_raw(55);
    let compound_instance = ClipId::from_raw(56);
    let compound_timeline = TimelineId::from_raw(14);
    let compound_track = TrackId::from_raw(24);
    let mut parent = Timeline::new(
        PARENT,
        "selection parent",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            PARENT_TRACK,
            "V1",
            video_semantics(FrameRate::FPS_24),
            vec![
                media_clip(left, range(0, 12, rate)),
                media_clip(right, range(12, 12, rate)),
            ],
        )],
    );
    parent.link_clips([left, right]).unwrap();
    parent.group_clips([left, right]).unwrap();
    parent
        .update_selection(
            [
                EditorialObjectId::Clip(left),
                EditorialObjectId::Clip(right),
            ],
            SelectionUpdate::Replace,
            SelectionExpansion::Related,
        )
        .unwrap();
    let mut project = EditorialProject::new(
        ProjectId::from_raw(106),
        "selection compound",
        [media()],
        [parent],
    )
    .unwrap();
    let request = CompoundClipRequest::new(
        PARENT,
        compound_timeline,
        "compound source",
        [
            EditorialObjectId::Clip(left),
            EditorialObjectId::Clip(right),
        ],
        [CompoundClipTrackRequest::new(
            PARENT_TRACK,
            compound_track,
            compound_instance,
        )],
    );

    let result = create_compound_clip_from_selection(&mut project, 0, request).unwrap();

    assert_eq!(result.revision(), 1);
    assert_eq!(result.compound_timeline_id(), compound_timeline);
    assert_eq!(result.clip_ids(), &[compound_instance]);
    assert_eq!(result.outcomes().len(), 1);
    let child = project.timeline(compound_timeline).unwrap();
    let child_track = child.track(compound_track).unwrap();
    for (clip_id, start) in [(left, 0), (right, 12)] {
        let clip = child_track
            .item(EditorialObjectId::Clip(clip_id))
            .unwrap()
            .as_clip()
            .unwrap();
        assert_eq!(clip.source(), ClipSource::Media(MEDIA));
        assert_eq!(clip.record_range(), range(start, 12, rate));
    }
    assert_eq!(
        child
            .edit_state()
            .link_for(left)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        vec![left, right]
    );
    assert_eq!(
        child
            .edit_state()
            .group_for(right)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        vec![left, right]
    );
    let parent = project.timeline(PARENT).unwrap();
    let instance = parent
        .track(PARENT_TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(compound_instance))
        .unwrap()
        .as_clip()
        .unwrap();
    assert_eq!(instance.source(), ClipSource::Timeline(compound_timeline));
    assert_eq!(instance.source_range(), range(0, 24, rate));
    assert_eq!(instance.record_range(), range(0, 24, rate));
    assert_eq!(
        parent.edit_state().selected_objects().collect::<Vec<_>>(),
        vec![EditorialObjectId::Clip(compound_instance)]
    );
}

#[test]
fn selection_compound_preserves_mixed_clocks_transitions_annotations_and_track_intent() {
    let video_rate = FrameRate::FPS_24.timebase();
    let audio_rate = Timebase::integer(48_000).unwrap();
    let video_left = ClipId::from_raw(90);
    let video_right = ClipId::from_raw(91);
    let audio_left = ClipId::from_raw(92);
    let audio_right = ClipId::from_raw(93);
    let transition = TransitionId::from_raw(94);
    let audio_track = TrackId::from_raw(25);
    let compound_timeline = TimelineId::from_raw(15);
    let child_video = TrackId::from_raw(26);
    let child_audio = TrackId::from_raw(27);
    let parent_video_instance = ClipId::from_raw(95);
    let parent_audio_instance = ClipId::from_raw(96);
    let selected = [
        EditorialObjectId::Clip(video_left),
        EditorialObjectId::Clip(video_right),
        EditorialObjectId::Clip(audio_left),
        EditorialObjectId::Clip(audio_right),
    ];
    let mut parent = Timeline::new(
        PARENT,
        "mixed-clock parent",
        video_rate,
        RationalTime::zero(video_rate),
        vec![
            Track::new(
                PARENT_TRACK,
                "V1",
                video_semantics(FrameRate::FPS_24),
                vec![
                    media_clip(video_left, range(0, 12, video_rate)),
                    TrackItem::Transition(Transition::new(
                        transition,
                        "Dissolve",
                        EditorialObjectId::Clip(video_left),
                        EditorialObjectId::Clip(video_right),
                        Duration::new(3, video_rate).unwrap(),
                        Duration::new(3, video_rate).unwrap(),
                    )),
                    media_clip(video_right, range(12, 12, video_rate)),
                ],
            ),
            Track::new(
                audio_track,
                "A1",
                audio_semantics(),
                vec![
                    media_clip(audio_left, range(0, 24_000, audio_rate)),
                    media_clip(audio_right, range(24_000, 24_000, audio_rate)),
                ],
            ),
        ],
    );
    parent
        .link_clips([video_left, video_right, audio_left, audio_right])
        .unwrap();
    parent
        .group_clips([video_left, video_right, audio_left, audio_right])
        .unwrap();
    parent.set_track_targeted(audio_track, true).unwrap();
    parent.set_track_height(audio_track, 96).unwrap();
    parent.set_track_sync_locked(audio_track, false).unwrap();
    parent.set_track_muted(audio_track, true).unwrap();
    parent.set_track_solo(audio_track, true).unwrap();
    parent.set_track_enabled(audio_track, false).unwrap();
    parent.set_snapping_enabled(false);
    parent.set_linked_selection_enabled(false);
    parent
        .update_selection(
            selected,
            SelectionUpdate::Replace,
            SelectionExpansion::Direct,
        )
        .unwrap();
    let marker_id = MarkerId::from_raw(97);
    parent
        .upsert_marker(
            Marker::new(
                marker_id,
                MarkerOwner::Object(EditorialObjectId::Clip(video_left)),
                range(1, 1, video_rate),
            )
            .unwrap(),
        )
        .unwrap();
    let object_metadata = TimelineMetadata::from_entries([(
        MetadataKey::new("edit.intent").unwrap(),
        MetadataValue::Text("keep exact".into()),
    )]);
    parent
        .set_metadata(
            MetadataOwner::Object(EditorialObjectId::Clip(video_left)),
            object_metadata.clone(),
        )
        .unwrap();
    let track_metadata = TimelineMetadata::from_entries([(
        MetadataKey::new("mix.role").unwrap(),
        MetadataValue::Text("dialog".into()),
    )]);
    parent
        .set_metadata(MetadataOwner::Track(audio_track), track_metadata.clone())
        .unwrap();
    let mut project = EditorialProject::new(
        ProjectId::from_raw(107),
        "mixed compound",
        [media()],
        [parent],
    )
    .unwrap();

    create_compound_clip_from_selection(
        &mut project,
        0,
        CompoundClipRequest::new(
            PARENT,
            compound_timeline,
            "mixed compound",
            selected,
            [
                CompoundClipTrackRequest::new(PARENT_TRACK, child_video, parent_video_instance),
                CompoundClipTrackRequest::new(audio_track, child_audio, parent_audio_instance),
            ],
        ),
    )
    .unwrap();

    let child = project.timeline(compound_timeline).unwrap();
    assert!(!child.snapping_enabled());
    assert!(!child.edit_state().linked_selection_enabled());
    assert!(child
        .track(child_video)
        .unwrap()
        .item(EditorialObjectId::Transition(transition))
        .is_some());
    let audio_state = child.edit_state().track_state(child_audio).unwrap();
    assert_eq!(audio_state.height(), 96);
    assert!(audio_state.targeted());
    assert!(!audio_state.sync_locked());
    assert!(audio_state.muted());
    assert!(audio_state.solo());
    assert!(!audio_state.enabled());
    assert_eq!(child.track(child_audio).unwrap().items().len(), 2);
    assert_eq!(
        child
            .track(child_audio)
            .unwrap()
            .items()
            .last()
            .unwrap()
            .record_range()
            .unwrap()
            .end_exclusive()
            .unwrap(),
        RationalTime::new(48_000, audio_rate)
    );
    assert!(child.marker(marker_id).is_some());
    assert_eq!(
        child.metadata(MetadataOwner::Object(EditorialObjectId::Clip(video_left))),
        Some(&object_metadata)
    );
    assert_eq!(
        child.metadata(MetadataOwner::Track(child_audio)),
        Some(&track_metadata)
    );
    let parent = project.timeline(PARENT).unwrap();
    assert!(parent.marker(marker_id).is_none());
    assert_eq!(
        parent
            .edit_state()
            .link_for(parent_video_instance)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        vec![parent_video_instance, parent_audio_instance]
    );
    assert_eq!(
        parent
            .edit_state()
            .group_for(parent_audio_instance)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        vec![parent_video_instance, parent_audio_instance]
    );
}

#[test]
fn selection_compound_rejects_relationship_and_transition_boundary_crossing_atomically() {
    let rate = FrameRate::FPS_24.timebase();
    let left = ClipId::from_raw(100);
    let right = ClipId::from_raw(101);
    let transition = TransitionId::from_raw(102);
    let mut parent = Timeline::new(
        PARENT,
        "boundary parent",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            PARENT_TRACK,
            "V1",
            video_semantics(FrameRate::FPS_24),
            vec![
                media_clip(left, range(0, 12, rate)),
                TrackItem::Transition(Transition::new(
                    transition,
                    "Boundary dissolve",
                    EditorialObjectId::Clip(left),
                    EditorialObjectId::Clip(right),
                    Duration::new(3, rate).unwrap(),
                    Duration::new(3, rate).unwrap(),
                )),
                media_clip(right, range(12, 12, rate)),
            ],
        )],
    );
    parent.link_clips([left, right]).unwrap();
    let mut project = EditorialProject::new(
        ProjectId::from_raw(108),
        "boundary rollback",
        [media()],
        [parent],
    )
    .unwrap();
    let before = project.clone();

    let error = create_compound_clip_from_selection(
        &mut project,
        0,
        CompoundClipRequest::new(
            PARENT,
            TimelineId::from_raw(16),
            "invalid boundary",
            [EditorialObjectId::Clip(left)],
            [CompoundClipTrackRequest::new(
                PARENT_TRACK,
                TrackId::from_raw(28),
                ClipId::from_raw(103),
            )],
        ),
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);
}

#[test]
fn nested_edits_report_shared_instances_and_reject_invalid_source_shrink() {
    let rate = FrameRate::FPS_24.timebase();
    let child = gap_track(
        CHILD,
        CHILD_TRACK,
        GapId::from_raw(60),
        24,
        FrameRate::FPS_24,
    );
    let first_instance = ClipId::from_raw(61);
    let second_instance = ClipId::from_raw(62);
    let first_parent = Timeline::new(
        PARENT,
        "first parent",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            PARENT_TRACK,
            "V1",
            video_semantics(FrameRate::FPS_24),
            vec![nested_clip(
                first_instance,
                CHILD,
                range(0, 24, rate),
                range(0, 24, rate),
            )],
        )],
    );
    let second_parent = Timeline::new(
        SECOND_PARENT,
        "second parent",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            SECOND_PARENT_TRACK,
            "V1",
            video_semantics(FrameRate::FPS_24),
            vec![nested_clip(
                second_instance,
                CHILD,
                range(0, 24, rate),
                range(0, 24, rate),
            )],
        )],
    );
    let mut project = EditorialProject::new(
        ProjectId::from_raw(102),
        "shared nested source",
        [],
        [child, first_parent, second_parent],
    )
    .unwrap();

    let result = edit_nested_sequence(&mut project, 0, first_instance, |timeline| {
        timeline.set_name("renamed shared child");
        Ok(())
    })
    .unwrap();

    assert_eq!(result.revision(), 1);
    assert_eq!(result.source_timeline_id(), CHILD);
    assert_eq!(
        result
            .instances()
            .iter()
            .map(|value| value.clip_id())
            .collect::<Vec<_>>(),
        vec![first_instance, second_instance]
    );
    assert_eq!(
        project.timeline(CHILD).unwrap().name(),
        "renamed shared child"
    );

    let before = project.clone();
    let error = edit_nested_sequence(&mut project, 1, second_instance, |timeline| {
        timeline
            .track_mut(CHILD_TRACK)?
            .replace_items(vec![TrackItem::Gap(Gap::new(
                GapId::from_raw(63),
                "too short",
                range(0, 12, rate),
            ))]);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);

    let stale = edit_nested_sequence(&mut project, 0, first_instance, |_| Ok(())).unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(project, before);
}

#[test]
fn recursive_tree_exposes_every_nested_link_in_parent_order() {
    let rate = FrameRate::FPS_24.timebase();
    let grandchild = gap_track(
        GRANDCHILD,
        GRANDCHILD_TRACK,
        GapId::from_raw(70),
        24,
        FrameRate::FPS_24,
    );
    let child_instance = ClipId::from_raw(71);
    let child = Timeline::new(
        CHILD,
        "child",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            CHILD_TRACK,
            "V1",
            video_semantics(FrameRate::FPS_24),
            vec![nested_clip(
                child_instance,
                GRANDCHILD,
                range(0, 24, rate),
                range(0, 24, rate),
            )],
        )],
    );
    let parent_instance = ClipId::from_raw(72);
    let parent = Timeline::new(
        PARENT,
        "parent",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            PARENT_TRACK,
            "V1",
            video_semantics(FrameRate::FPS_24),
            vec![nested_clip(
                parent_instance,
                CHILD,
                range(0, 24, rate),
                range(0, 24, rate),
            )],
        )],
    );
    let project = EditorialProject::new(
        ProjectId::from_raw(103),
        "tree",
        [],
        [grandchild, child, parent],
    )
    .unwrap();

    let tree = nested_sequence_tree(&project, PARENT).unwrap();
    assert_eq!(tree.len(), 2);
    assert_eq!(tree[0].depth(), 0);
    assert_eq!(tree[0].parent_timeline_id(), PARENT);
    assert_eq!(tree[0].clip_id(), parent_instance);
    assert_eq!(tree[0].source_timeline_id(), CHILD);
    assert_eq!(tree[1].depth(), 1);
    assert_eq!(tree[1].parent_timeline_id(), CHILD);
    assert_eq!(tree[1].clip_id(), child_instance);
    assert_eq!(tree[1].source_timeline_id(), GRANDCHILD);
}

#[test]
fn invalid_compounds_and_inexact_clocks_roll_back_atomically() {
    let rate = FrameRate::FPS_24.timebase();
    let target_gap = GapId::from_raw(80);
    let parent = gap_track(PARENT, PARENT_TRACK, target_gap, 24, FrameRate::FPS_24);
    let mut project =
        EditorialProject::new(ProjectId::from_raw(104), "cycle", [], [parent.clone()]).unwrap();
    let cyclic_child = Timeline::new(
        CHILD,
        "cyclic child",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            CHILD_TRACK,
            "V1",
            video_semantics(FrameRate::FPS_24),
            vec![nested_clip(
                ClipId::from_raw(81),
                PARENT,
                range(0, 24, rate),
                range(0, 24, rate),
            )],
        )],
    );
    let request = NestedSequenceRequest::new(
        PARENT,
        PARENT_TRACK,
        ClipId::from_raw(82),
        "cycle",
        range(0, 24, rate),
        NestedSequencePlacement::replace(EditorialObjectId::Gap(target_gap)),
    );
    let before = project.clone();
    let error = create_compound_clip(&mut project, 0, cyclic_child, request).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(project, before);

    let duplicate = NestedSequenceRequest::new(
        PARENT,
        PARENT_TRACK,
        ClipId::from_raw(86),
        "duplicate timeline",
        range(0, 24, rate),
        NestedSequencePlacement::replace(EditorialObjectId::Gap(target_gap)),
    );
    let error = create_compound_clip(&mut project, 0, parent, duplicate).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(project, before);

    let missing = NestedSequenceRequest::new(
        PARENT,
        PARENT_TRACK,
        ClipId::from_raw(87),
        "missing timeline",
        range(0, 24, rate),
        NestedSequencePlacement::replace(EditorialObjectId::Gap(target_gap)),
    );
    let error =
        place_nested_sequence(&mut project, 0, TimelineId::from_raw(999), missing).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(project, before);

    let child = gap_track(
        CHILD,
        CHILD_TRACK,
        GapId::from_raw(83),
        24,
        FrameRate::FPS_24,
    );
    let parent_25 = gap_track(
        SECOND_PARENT,
        SECOND_PARENT_TRACK,
        GapId::from_raw(84),
        25,
        FrameRate::FPS_25,
    );
    let mut inexact =
        EditorialProject::new(ProjectId::from_raw(105), "inexact", [], [child, parent_25]).unwrap();
    let request = NestedSequenceRequest::new(
        SECOND_PARENT,
        SECOND_PARENT_TRACK,
        ClipId::from_raw(85),
        "inexact",
        range(0, 1, rate),
        NestedSequencePlacement::append(),
    );
    let before = inexact.clone();
    let error = place_nested_sequence(&mut inexact, 0, CHILD, request).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(inexact, before);
}
