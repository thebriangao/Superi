use std::collections::BTreeMap;

use superi_core::error::ErrorCategory;
use superi_core::ids::{
    ClipId, GapId, GeneratorId, MediaId, ProjectId, TimelineId, TrackId, TransitionId,
};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::edit_ops::{apply_edit_batch, EditKind, EditOperation, TrackDurationChange};
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, Gap, Generator, LinkedMediaReference,
    Timeline, Track, TrackItem, TrackSemantics, Transition, VideoCompositing, VideoTrackSemantics,
};

const MEDIA: MediaId = MediaId::from_raw(1);
const SUB_TIMELINE: TimelineId = TimelineId::from_raw(2);
const MAIN_TIMELINE: TimelineId = TimelineId::from_raw(3);
const SUB_TRACK: TrackId = TrackId::from_raw(10);
const V1: TrackId = TrackId::from_raw(11);
const V2: TrackId = TrackId::from_raw(12);
const SUB_CLIP: ClipId = ClipId::from_raw(20);
const A: ClipId = ClipId::from_raw(21);
const B: ClipId = ClipId::from_raw(22);
const TRACK_GAP: GapId = GapId::from_raw(30);
const DISSOLVE: TransitionId = TransitionId::from_raw(40);

fn edit_rate() -> Timebase {
    Timebase::integer(24).unwrap()
}

fn source_rate() -> Timebase {
    Timebase::integer(48).unwrap()
}

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

fn media_clip(id: ClipId, name: &str, source_start: i64, record: TimeRange) -> TrackItem {
    TrackItem::Clip(Clip::new(
        id,
        name,
        ClipSource::Media(MEDIA),
        range(source_start, record.duration().value() * 2, source_rate()),
        record,
    ))
}

fn project_fixture() -> EditorialProject {
    let sub = Timeline::new(
        SUB_TIMELINE,
        "nested source",
        edit_rate(),
        RationalTime::zero(edit_rate()),
        vec![Track::new(
            SUB_TRACK,
            "nested v1",
            video_semantics(),
            vec![media_clip(
                SUB_CLIP,
                "nested shot",
                40,
                range(0, 12, edit_rate()),
            )],
        )],
    );
    let a = media_clip(A, "a", 0, range(0, 8, edit_rate()));
    let b = media_clip(B, "b", 16, range(8, 4, edit_rate()));
    let transition = TrackItem::Transition(Transition::new(
        DISSOLVE,
        "a to b",
        EditorialObjectId::Clip(A),
        EditorialObjectId::Clip(B),
        Duration::new(1, edit_rate()).unwrap(),
        Duration::new(1, edit_rate()).unwrap(),
    ));
    let main = Timeline::new(
        MAIN_TIMELINE,
        "main",
        edit_rate(),
        RationalTime::zero(edit_rate()),
        vec![
            Track::new(V1, "V1", video_semantics(), vec![a, transition, b]),
            Track::new(
                V2,
                "V2",
                video_semantics(),
                vec![TrackItem::Gap(Gap::new(
                    TRACK_GAP,
                    "empty V2",
                    range(0, 12, edit_rate()),
                ))],
            ),
        ],
    );

    EditorialProject::new(
        ProjectId::from_raw(100),
        "edit operations",
        [LinkedMediaReference::new(
            MEDIA,
            "camera",
            "urn:superi:test:camera",
            Some(range(0, 400, source_rate())),
        )],
        [sub, main],
    )
    .unwrap()
}

fn track(project: &EditorialProject, id: TrackId) -> &Track {
    project.timeline(MAIN_TIMELINE).unwrap().track(id).unwrap()
}

fn clip(project: &EditorialProject, track_id: TrackId, id: ClipId) -> &Clip {
    track(project, track_id)
        .item(EditorialObjectId::Clip(id))
        .unwrap()
        .as_clip()
        .unwrap()
}

fn timed_ids(track: &Track) -> Vec<EditorialObjectId> {
    track
        .items()
        .iter()
        .filter_map(|item| item.record_range().map(|_| item.id()))
        .collect()
}

#[test]
fn insert_splits_exact_source_ranges_and_preserves_nested_material() {
    let mut project = project_fixture();
    let nested = TrackItem::Clip(Clip::new(
        ClipId::from_raw(50),
        "nested insert",
        ClipSource::Timeline(SUB_TIMELINE),
        range(2, 4, edit_rate()),
        range(99, 4, edit_rate()),
    ));
    let operation = EditOperation::insert(
        MAIN_TIMELINE,
        V1,
        RationalTime::new(4, edit_rate()),
        nested,
        [EditorialObjectId::Clip(ClipId::from_raw(51))],
    );

    let result = apply_edit_batch(&mut project, 0, &[operation]).unwrap();

    assert_eq!(project.revision(), 1);
    assert_eq!(result.revision(), 1);
    assert_eq!(result.outcomes().len(), 1);
    let outcome = &result.outcomes()[0];
    assert_eq!(outcome.kind(), EditKind::Insert);
    assert_eq!(outcome.timeline_id(), MAIN_TIMELINE);
    assert_eq!(outcome.track_id(), V1);
    assert_eq!(outcome.affected_range(), range(4, 4, edit_rate()));
    assert_eq!(
        outcome.duration_change(),
        TrackDurationChange::Extended(Duration::new(4, edit_rate()).unwrap())
    );
    assert_eq!(
        outcome.inserted_ids(),
        &[EditorialObjectId::Clip(ClipId::from_raw(50))]
    );
    assert_eq!(outcome.fragments().len(), 1);
    assert_eq!(
        outcome.fragments()[0].original(),
        EditorialObjectId::Clip(A)
    );
    assert_eq!(
        outcome.fragments()[0].created(),
        EditorialObjectId::Clip(ClipId::from_raw(51))
    );
    assert_eq!(outcome.removed_transitions(), &[DISSOLVE]);
    assert_eq!(
        outcome.modified_ids(),
        &[
            EditorialObjectId::Clip(A),
            EditorialObjectId::Clip(ClipId::from_raw(51)),
            EditorialObjectId::Clip(B),
        ]
    );

    assert_eq!(
        timed_ids(track(&project, V1)),
        vec![
            EditorialObjectId::Clip(A),
            EditorialObjectId::Clip(ClipId::from_raw(50)),
            EditorialObjectId::Clip(ClipId::from_raw(51)),
            EditorialObjectId::Clip(B),
        ]
    );
    assert!(track(&project, V1)
        .item(EditorialObjectId::Transition(DISSOLVE))
        .is_none());
    assert_eq!(
        clip(&project, V1, A).record_range(),
        range(0, 4, edit_rate())
    );
    assert_eq!(
        clip(&project, V1, A).source_range(),
        range(0, 8, source_rate())
    );
    assert_eq!(
        clip(&project, V1, ClipId::from_raw(51)).record_range(),
        range(8, 4, edit_rate())
    );
    assert_eq!(
        clip(&project, V1, ClipId::from_raw(51)).source_range(),
        range(8, 8, source_rate())
    );
    assert_eq!(
        clip(&project, V1, B).record_range(),
        range(12, 4, edit_rate())
    );
    let inserted = clip(&project, V1, ClipId::from_raw(50));
    assert_eq!(inserted.source(), ClipSource::Timeline(SUB_TIMELINE));
    assert_eq!(inserted.source_range(), range(2, 4, edit_rate()));
    assert_eq!(inserted.record_range(), range(4, 4, edit_rate()));
}

#[test]
fn overwrite_and_replace_preserve_track_duration_and_exact_source_mapping() {
    let mut overwritten = project_fixture();
    let material = media_clip(
        ClipId::from_raw(60),
        "overwrite",
        100,
        range(70, 4, edit_rate()),
    );
    let operation = EditOperation::overwrite(
        MAIN_TIMELINE,
        V1,
        RationalTime::new(2, edit_rate()),
        material,
        [EditorialObjectId::Clip(ClipId::from_raw(61))],
    );

    let result = apply_edit_batch(&mut overwritten, 0, &[operation]).unwrap();
    assert_eq!(
        result.outcomes()[0].duration_change(),
        TrackDurationChange::Unchanged
    );
    assert_eq!(
        timed_ids(track(&overwritten, V1)),
        vec![
            EditorialObjectId::Clip(A),
            EditorialObjectId::Clip(ClipId::from_raw(60)),
            EditorialObjectId::Clip(ClipId::from_raw(61)),
            EditorialObjectId::Clip(B),
        ]
    );
    assert_eq!(
        clip(&overwritten, V1, A).record_range(),
        range(0, 2, edit_rate())
    );
    assert_eq!(
        clip(&overwritten, V1, A).source_range(),
        range(0, 4, source_rate())
    );
    assert_eq!(
        clip(&overwritten, V1, ClipId::from_raw(61)).record_range(),
        range(6, 2, edit_rate())
    );
    assert_eq!(
        clip(&overwritten, V1, ClipId::from_raw(61)).source_range(),
        range(12, 4, source_rate())
    );
    assert_eq!(
        clip(&overwritten, V1, ClipId::from_raw(60)).source_range(),
        range(100, 8, source_rate())
    );
    assert_eq!(
        overwritten
            .timeline(MAIN_TIMELINE)
            .unwrap()
            .duration()
            .unwrap(),
        Duration::new(12, edit_rate()).unwrap()
    );

    let mut replaced = project_fixture();
    let generator = TrackItem::Generator(Generator::new(
        GeneratorId::from_raw(62),
        "replacement",
        "solid_color",
        BTreeMap::from([("rgba".to_owned(), "0,0,0,1".to_owned())]),
        range(30, 4, edit_rate()),
    ));
    let result = apply_edit_batch(
        &mut replaced,
        0,
        &[EditOperation::replace(
            MAIN_TIMELINE,
            V1,
            EditorialObjectId::Clip(B),
            generator,
        )],
    )
    .unwrap();
    let outcome = &result.outcomes()[0];
    assert_eq!(outcome.duration_change(), TrackDurationChange::Unchanged);
    assert_eq!(outcome.removed_ids(), &[EditorialObjectId::Clip(B)]);
    assert_eq!(
        outcome.inserted_ids(),
        &[EditorialObjectId::Generator(GeneratorId::from_raw(62))]
    );
    assert_eq!(outcome.removed_transitions(), &[DISSOLVE]);
    let item = track(&replaced, V1)
        .item(EditorialObjectId::Generator(GeneratorId::from_raw(62)))
        .unwrap();
    assert_eq!(item.record_range().unwrap(), range(8, 4, edit_rate()));
    assert_eq!(
        replaced
            .timeline(MAIN_TIMELINE)
            .unwrap()
            .duration()
            .unwrap(),
        Duration::new(12, edit_rate()).unwrap()
    );

    let mut identity_preserving = project_fixture();
    let replacement = media_clip(A, "alternate take", 60, range(44, 8, edit_rate()));
    let result = apply_edit_batch(
        &mut identity_preserving,
        0,
        &[EditOperation::replace(
            MAIN_TIMELINE,
            V1,
            EditorialObjectId::Clip(A),
            replacement,
        )],
    )
    .unwrap();
    let outcome = &result.outcomes()[0];
    assert!(outcome.inserted_ids().is_empty());
    assert!(outcome.removed_ids().is_empty());
    assert_eq!(outcome.modified_ids(), &[EditorialObjectId::Clip(A)]);
    assert!(outcome.removed_transitions().is_empty());
    assert!(track(&identity_preserving, V1)
        .item(EditorialObjectId::Transition(DISSOLVE))
        .is_some());
    assert_eq!(
        clip(&identity_preserving, V1, A).source_range(),
        range(60, 16, source_rate())
    );
}

#[test]
fn lift_append_and_extract_keep_every_result_directly_inspectable() {
    let mut project = project_fixture();
    let lift_gap = Gap::new(
        GapId::from_raw(70),
        "lifted range",
        range(90, 8, edit_rate()),
    );
    let lifted = apply_edit_batch(
        &mut project,
        0,
        &[EditOperation::lift(
            MAIN_TIMELINE,
            V1,
            range(2, 8, edit_rate()),
            lift_gap,
            [],
        )],
    )
    .unwrap();
    assert_eq!(
        lifted.outcomes()[0].duration_change(),
        TrackDurationChange::Unchanged
    );
    assert_eq!(
        timed_ids(track(&project, V1)),
        vec![
            EditorialObjectId::Clip(A),
            EditorialObjectId::Gap(GapId::from_raw(70)),
            EditorialObjectId::Clip(B),
        ]
    );
    assert_eq!(
        clip(&project, V1, A).record_range(),
        range(0, 2, edit_rate())
    );
    assert_eq!(
        clip(&project, V1, B).record_range(),
        range(10, 2, edit_rate())
    );
    assert_eq!(
        clip(&project, V1, B).source_range(),
        range(20, 4, source_rate())
    );
    assert_eq!(
        track(&project, V1)
            .item(EditorialObjectId::Gap(GapId::from_raw(70)))
            .unwrap()
            .record_range()
            .unwrap(),
        range(2, 8, edit_rate())
    );

    let appended = apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::append(
            MAIN_TIMELINE,
            V1,
            media_clip(ClipId::from_raw(71), "tail", 200, range(0, 2, edit_rate())),
        )],
    )
    .unwrap();
    assert_eq!(
        appended.outcomes()[0].duration_change(),
        TrackDurationChange::Extended(Duration::new(2, edit_rate()).unwrap())
    );
    assert_eq!(
        clip(&project, V1, ClipId::from_raw(71)).record_range(),
        range(12, 2, edit_rate())
    );

    let extracted = apply_edit_batch(
        &mut project,
        2,
        &[EditOperation::extract(
            MAIN_TIMELINE,
            V1,
            range(1, 2, edit_rate()),
            [],
        )],
    )
    .unwrap();
    assert_eq!(
        extracted.outcomes()[0].duration_change(),
        TrackDurationChange::Shortened(Duration::new(2, edit_rate()).unwrap())
    );
    assert_eq!(
        clip(&project, V1, A).record_range(),
        range(0, 1, edit_rate())
    );
    assert_eq!(
        track(&project, V1)
            .item(EditorialObjectId::Gap(GapId::from_raw(70)))
            .unwrap()
            .record_range()
            .unwrap(),
        range(1, 7, edit_rate())
    );
    assert_eq!(
        clip(&project, V1, ClipId::from_raw(71)).record_range(),
        range(10, 2, edit_rate())
    );
    assert_eq!(
        project.timeline(MAIN_TIMELINE).unwrap().duration().unwrap(),
        Duration::new(12, edit_rate()).unwrap()
    );
}

#[test]
fn multi_track_batches_publish_once_and_fail_atomically() {
    let mut project = project_fixture();
    let before = project.clone();
    let invalid = [
        EditOperation::append(
            MAIN_TIMELINE,
            V2,
            media_clip(
                ClipId::from_raw(80),
                "would append",
                220,
                range(0, 2, edit_rate()),
            ),
        ),
        EditOperation::insert(
            MAIN_TIMELINE,
            V1,
            RationalTime::new(6, edit_rate()),
            media_clip(
                ClipId::from_raw(81),
                "missing split identity",
                240,
                range(0, 2, edit_rate()),
            ),
            [],
        ),
    ];

    let error = apply_edit_batch(&mut project, 0, &invalid).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert!(error.message().contains("fragment identity"));
    assert_eq!(project, before);

    let operations = [
        EditOperation::insert(
            MAIN_TIMELINE,
            V1,
            RationalTime::new(6, edit_rate()),
            media_clip(
                ClipId::from_raw(82),
                "V1 insert",
                260,
                range(0, 2, edit_rate()),
            ),
            [EditorialObjectId::Clip(ClipId::from_raw(83))],
        ),
        EditOperation::insert(
            MAIN_TIMELINE,
            V2,
            RationalTime::new(6, edit_rate()),
            media_clip(
                ClipId::from_raw(84),
                "V2 insert",
                280,
                range(0, 2, edit_rate()),
            ),
            [EditorialObjectId::Gap(GapId::from_raw(85))],
        ),
    ];

    let result = apply_edit_batch(&mut project, 0, &operations).unwrap();
    assert_eq!(project.revision(), 1);
    assert_eq!(result.revision(), 1);
    assert_eq!(result.outcomes().len(), 2);
    assert_eq!(
        track(&project, V1)
            .items()
            .iter()
            .filter_map(TrackItem::record_range)
            .next_back()
            .unwrap()
            .end_exclusive()
            .unwrap(),
        RationalTime::new(14, edit_rate())
    );
    assert_eq!(
        track(&project, V2)
            .items()
            .iter()
            .filter_map(TrackItem::record_range)
            .next_back()
            .unwrap()
            .end_exclusive()
            .unwrap(),
        RationalTime::new(14, edit_rate())
    );

    let stale = apply_edit_batch(&mut project, 0, &operations).unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(project.revision(), 1);
}

#[test]
fn invalid_clocks_transitions_and_overwrite_extent_are_rejected_without_publication() {
    let mut project = project_fixture();
    let before = project.clone();
    let transition_material = TrackItem::Transition(Transition::new(
        TransitionId::from_raw(90),
        "invalid material",
        EditorialObjectId::Clip(A),
        EditorialObjectId::Clip(B),
        Duration::new(1, edit_rate()).unwrap(),
        Duration::new(1, edit_rate()).unwrap(),
    ));
    let error = apply_edit_batch(
        &mut project,
        0,
        &[EditOperation::append(
            MAIN_TIMELINE,
            V1,
            transition_material,
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);

    let wrong_clock = EditOperation::extract(
        MAIN_TIMELINE,
        V1,
        range(0, 1, Timebase::integer(30).unwrap()),
        [],
    );
    let error = apply_edit_batch(&mut project, 0, &[wrong_clock]).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);

    let overlong = EditOperation::overwrite(
        MAIN_TIMELINE,
        V1,
        RationalTime::new(10, edit_rate()),
        media_clip(
            ClipId::from_raw(91),
            "overlong",
            300,
            range(0, 4, edit_rate()),
        ),
        [],
    );
    let error = apply_edit_batch(&mut project, 0, &[overlong]).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);
}
