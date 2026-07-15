use std::collections::BTreeMap;

use superi_core::error::ErrorCategory;
use superi_core::ids::{
    CaptionId, ClipId, GapId, GeneratorId, MarkerId, MediaId, ProjectId, TimelineId, TrackId,
    TransitionId,
};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::edit_ops::{
    apply_edit_batch, EditKind, EditOperation, EditSide, ExtendMode, RippleSyncAdjustment,
    ThreePointPlacement,
};
use superi_timeline::edit_state::{SelectionExpansion, SelectionUpdate};
use superi_timeline::markers::{Marker, MarkerLabel, MarkerOwner};
use superi_timeline::model::{
    Caption, Clip, ClipSource, EditorialObjectId, EditorialProject, Gap, Generator,
    LinkedMediaReference, Timeline, Track, TrackItem, TrackSemantics, Transition, VideoCompositing,
    VideoTrackSemantics,
};

const MEDIA: MediaId = MediaId::from_raw(1);
const TIMELINE: TimelineId = TimelineId::from_raw(2);
const TRACK: TrackId = TrackId::from_raw(3);
const TRACK_2: TrackId = TrackId::from_raw(4);
const A: ClipId = ClipId::from_raw(10);
const B: ClipId = ClipId::from_raw(11);
const C: ClipId = ClipId::from_raw(12);
const D: ClipId = ClipId::from_raw(13);
const E: ClipId = ClipId::from_raw(14);
const INTENT_MARKER: MarkerId = MarkerId::from_raw(15);

fn rate() -> Timebase {
    Timebase::integer(24).unwrap()
}

fn range(start: i64, duration: u64) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, rate()),
        Duration::new(duration, rate()).unwrap(),
    )
    .unwrap()
}

fn range_at(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn clip(id: ClipId, source_start: i64, record_start: i64, duration: u64) -> TrackItem {
    TrackItem::Clip(
        Clip::new(
            id,
            format!("clip {id}"),
            ClipSource::Media(MEDIA),
            range(source_start, duration),
            range(record_start, duration),
        )
        .unwrap(),
    )
}

fn project() -> EditorialProject {
    let semantics = TrackSemantics::Video(VideoTrackSemantics::new(
        FrameRate::FPS_24,
        VideoCompositing::Over,
    ));
    EditorialProject::new(
        ProjectId::from_raw(20),
        "advanced edits",
        [LinkedMediaReference::new(
            MEDIA,
            "media",
            "urn:superi:test:advanced",
            Some(range(0, 200)),
        )],
        [Timeline::new(
            TIMELINE,
            "main",
            rate(),
            RationalTime::zero(rate()),
            vec![Track::new(
                TRACK,
                "V1",
                semantics,
                vec![clip(A, 0, 0, 8), clip(B, 20, 8, 8), clip(C, 40, 16, 8)],
            )],
        )],
    )
    .unwrap()
}

fn project_clip(project: &EditorialProject, id: ClipId) -> &Clip {
    project
        .timeline(TIMELINE)
        .unwrap()
        .track(TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(id))
        .unwrap()
        .as_clip()
        .unwrap()
}

fn track_clip(project: &EditorialProject, track_id: TrackId, id: ClipId) -> &Clip {
    project
        .timeline(TIMELINE)
        .unwrap()
        .track(track_id)
        .unwrap()
        .item(EditorialObjectId::Clip(id))
        .unwrap()
        .as_clip()
        .unwrap()
}

#[test]
fn ripple_moves_the_cut_and_downstream_material_atomically() {
    let mut project = project();
    project
        .edit(0, |draft| {
            let mut marker = Marker::new(
                INTENT_MARKER,
                MarkerOwner::Object(EditorialObjectId::Clip(B)),
                range(1, 1),
            )?;
            marker.set_label(Some(MarkerLabel::new("preserve cut intent")?));
            draft.timeline_mut(TIMELINE)?.upsert_marker(marker)?;
            Ok(())
        })
        .unwrap();
    let operation = EditOperation::ripple(
        TIMELINE,
        TRACK,
        EditorialObjectId::Clip(A),
        EditSide::End,
        RationalTime::new(6, rate()),
    );

    let result = apply_edit_batch(&mut project, 1, &[operation]).unwrap();

    assert_eq!(project.revision(), 2);
    assert_eq!(result.outcomes()[0].kind(), EditKind::Ripple);
    assert_eq!(project_clip(&project, A).record_range(), range(0, 6));
    assert_eq!(project_clip(&project, A).source_range(), range(0, 6));
    assert_eq!(project_clip(&project, B).record_range(), range(6, 8));
    assert_eq!(project_clip(&project, B).source_range(), range(20, 8));
    assert_eq!(project_clip(&project, C).record_range(), range(14, 8));
    let timeline = project.timeline(TIMELINE).unwrap();
    assert_eq!(
        timeline.resolved_marker_range(INTENT_MARKER).unwrap(),
        Some(range(7, 1))
    );
    assert_eq!(
        timeline
            .marker(INTENT_MARKER)
            .unwrap()
            .label()
            .unwrap()
            .as_str(),
        "preserve cut intent"
    );
}

#[test]
fn trim_roll_slip_and_slide_preserve_their_distinct_timing_intent() {
    let mut trimmed = project();
    apply_edit_batch(
        &mut trimmed,
        0,
        &[EditOperation::trim(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(A),
            EditSide::End,
            RationalTime::new(6, rate()),
            Some(GapId::from_raw(30)),
        )],
    )
    .unwrap();
    assert_eq!(project_clip(&trimmed, A).record_range(), range(0, 6));
    assert_eq!(project_clip(&trimmed, B).record_range(), range(8, 8));
    assert_eq!(
        trimmed
            .timeline(TIMELINE)
            .unwrap()
            .track(TRACK)
            .unwrap()
            .item(EditorialObjectId::Gap(GapId::from_raw(30)))
            .unwrap()
            .record_range()
            .unwrap(),
        range(6, 2)
    );
    apply_edit_batch(
        &mut trimmed,
        1,
        &[EditOperation::trim(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(A),
            EditSide::End,
            RationalTime::new(7, rate()),
            None,
        )],
    )
    .unwrap();
    assert_eq!(project_clip(&trimmed, A).record_range(), range(0, 7));
    assert_eq!(
        trimmed
            .timeline(TIMELINE)
            .unwrap()
            .track(TRACK)
            .unwrap()
            .item(EditorialObjectId::Gap(GapId::from_raw(30)))
            .unwrap()
            .record_range()
            .unwrap(),
        range(7, 1)
    );

    let mut rolled = project();
    let roll = apply_edit_batch(
        &mut rolled,
        0,
        &[EditOperation::roll(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(A),
            EditorialObjectId::Clip(B),
            RationalTime::new(6, rate()),
        )],
    )
    .unwrap();
    assert_eq!(roll.outcomes()[0].kind(), EditKind::Roll);
    assert_eq!(project_clip(&rolled, A).record_range(), range(0, 6));
    assert_eq!(project_clip(&rolled, B).record_range(), range(6, 10));
    assert_eq!(project_clip(&rolled, B).source_range(), range(18, 10));
    assert_eq!(project_clip(&rolled, C).record_range(), range(16, 8));

    let mut slipped = project();
    apply_edit_batch(
        &mut slipped,
        0,
        &[EditOperation::slip(
            TIMELINE,
            TRACK,
            A,
            RationalTime::new(30, rate()),
        )],
    )
    .unwrap();
    assert_eq!(project_clip(&slipped, A).source_range(), range(30, 8));
    assert_eq!(project_clip(&slipped, A).record_range(), range(0, 8));

    let mut slid = project();
    apply_edit_batch(
        &mut slid,
        0,
        &[EditOperation::slide(
            TIMELINE,
            TRACK,
            B,
            RationalTime::new(6, rate()),
        )],
    )
    .unwrap();
    assert_eq!(project_clip(&slid, A).record_range(), range(0, 6));
    assert_eq!(project_clip(&slid, A).source_range(), range(0, 6));
    assert_eq!(project_clip(&slid, B).record_range(), range(6, 8));
    assert_eq!(project_clip(&slid, B).source_range(), range(20, 8));
    assert_eq!(project_clip(&slid, C).record_range(), range(14, 10));
    assert_eq!(project_clip(&slid, C).source_range(), range(38, 10));
}

#[test]
fn razor_and_extend_reuse_exact_trim_semantics() {
    let mut razored = project();
    let result = apply_edit_batch(
        &mut razored,
        0,
        &[EditOperation::razor(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(B),
            RationalTime::new(12, rate()),
            EditorialObjectId::Clip(ClipId::from_raw(31)),
        )],
    )
    .unwrap();
    assert_eq!(result.outcomes()[0].kind(), EditKind::Razor);
    assert_eq!(project_clip(&razored, B).record_range(), range(8, 4));
    assert_eq!(project_clip(&razored, B).source_range(), range(20, 4));
    assert_eq!(
        project_clip(&razored, ClipId::from_raw(31)).record_range(),
        range(12, 4)
    );
    assert_eq!(
        project_clip(&razored, ClipId::from_raw(31)).source_range(),
        range(24, 4)
    );

    let mut extended = project();
    let result = apply_edit_batch(
        &mut extended,
        0,
        &[EditOperation::extend(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(A),
            EditSide::End,
            RationalTime::new(10, rate()),
            ExtendMode::Ripple,
        )],
    )
    .unwrap();
    assert_eq!(result.outcomes()[0].kind(), EditKind::Extend);
    assert_eq!(project_clip(&extended, A).record_range(), range(0, 10));
    assert_eq!(project_clip(&extended, B).record_range(), range(10, 8));
}

fn point_clip(id: ClipId) -> Clip {
    Clip::new(
        id,
        format!("point {id}"),
        ClipSource::Media(MEDIA),
        range(0, 1),
        range(0, 1),
    )
    .unwrap()
}

#[test]
fn three_and_four_point_edits_derive_exact_missing_boundaries() {
    let placements = [
        ThreePointPlacement::SourceRangeAtRecordStart {
            source_range: range(30, 4),
            record_start: RationalTime::new(4, rate()),
        },
        ThreePointPlacement::SourceStartOverRecordRange {
            source_start: RationalTime::new(30, rate()),
            record_range: range(4, 4),
        },
        ThreePointPlacement::SourceRangeBacktimedToRecordEnd {
            source_range: range(30, 4),
            record_end: RationalTime::new(8, rate()),
        },
        ThreePointPlacement::SourceEndBacktimedOverRecordRange {
            source_end: RationalTime::new(34, rate()),
            record_range: range(4, 4),
        },
    ];
    for (index, placement) in placements.into_iter().enumerate() {
        let mut project = project();
        let id = ClipId::from_raw(40 + index as u128);
        let result = apply_edit_batch(
            &mut project,
            0,
            &[EditOperation::three_point(
                TIMELINE,
                TRACK,
                point_clip(id),
                placement,
                [],
            )],
        )
        .unwrap();
        assert_eq!(result.outcomes()[0].kind(), EditKind::ThreePoint);
        assert_eq!(project_clip(&project, id).source_range(), range(30, 4));
        assert_eq!(project_clip(&project, id).record_range(), range(4, 4));
    }

    let mut exact = project();
    apply_edit_batch(
        &mut exact,
        0,
        &[EditOperation::four_point(
            TIMELINE,
            TRACK,
            point_clip(ClipId::from_raw(60)),
            range(50, 4),
            range(4, 4),
            [],
        )],
    )
    .unwrap();
    assert_eq!(
        project_clip(&exact, ClipId::from_raw(60)).source_range(),
        range(50, 4)
    );

    let mut mismatch = project();
    let before = mismatch.clone();
    let error = apply_edit_batch(
        &mut mismatch,
        0,
        &[EditOperation::four_point(
            TIMELINE,
            TRACK,
            point_clip(ClipId::from_raw(62)),
            range(50, 4),
            range(4, 6),
            [],
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(mismatch, before);
}

#[test]
fn synchronized_ripple_is_explicit_and_atomic_across_locked_tracks() {
    let semantics = TrackSemantics::Video(VideoTrackSemantics::new(
        FrameRate::FPS_24,
        VideoCompositing::Over,
    ));
    let mut project = EditorialProject::new(
        ProjectId::from_raw(70),
        "synchronized ripple",
        [LinkedMediaReference::new(
            MEDIA,
            "media",
            "urn:superi:test:sync",
            Some(range(0, 200)),
        )],
        [Timeline::new(
            TIMELINE,
            "main",
            rate(),
            RationalTime::zero(rate()),
            vec![
                Track::new(
                    TRACK,
                    "V1",
                    semantics.clone(),
                    vec![clip(A, 0, 0, 8), clip(B, 20, 8, 8)],
                ),
                Track::new(
                    TRACK_2,
                    "V2",
                    semantics,
                    vec![clip(D, 40, 0, 8), clip(E, 60, 8, 8)],
                ),
            ],
        )],
    )
    .unwrap();
    let before = project.clone();
    let error = apply_edit_batch(
        &mut project,
        0,
        &[EditOperation::ripple(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(A),
            EditSide::End,
            RationalTime::new(6, rate()),
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);

    let result = apply_edit_batch(
        &mut project,
        0,
        &[EditOperation::ripple_synchronized(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(A),
            EditSide::End,
            RationalTime::new(6, rate()),
            [RippleSyncAdjustment::new(TRACK_2, GapId::from_raw(71), [])],
        )],
    )
    .unwrap();
    assert_eq!(result.outcomes()[0].synchronized_tracks(), &[TRACK_2]);
    assert_eq!(track_clip(&project, TRACK, A).record_range(), range(0, 6));
    assert_eq!(track_clip(&project, TRACK, B).record_range(), range(6, 8));
    assert_eq!(track_clip(&project, TRACK_2, D).record_range(), range(0, 6));
    assert_eq!(track_clip(&project, TRACK_2, E).record_range(), range(6, 8));

    let mut extended = before;
    let error = apply_edit_batch(
        &mut extended,
        0,
        &[EditOperation::extend(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(A),
            EditSide::End,
            RationalTime::new(6, rate()),
            ExtendMode::Ripple,
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    let result = apply_edit_batch(
        &mut extended,
        0,
        &[EditOperation::extend_synchronized(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(A),
            EditSide::End,
            RationalTime::new(6, rate()),
            [RippleSyncAdjustment::new(TRACK_2, GapId::from_raw(73), [])],
        )],
    )
    .unwrap();
    assert_eq!(result.outcomes()[0].kind(), EditKind::Extend);
    assert_eq!(result.outcomes()[0].synchronized_tracks(), &[TRACK_2]);
    assert_eq!(track_clip(&extended, TRACK, B).record_range(), range(6, 8));
    assert_eq!(
        track_clip(&extended, TRACK_2, E).record_range(),
        range(6, 8)
    );
}

#[test]
fn split_clip_fragments_inherit_selection_links_and_groups() {
    let mut project = project();
    project
        .edit(0, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.link_clips([A, B])?;
            timeline.group_clips([A, C])?;
            timeline.set_track_targeted(TRACK, true)?;
            timeline.set_track_sync_locked(TRACK, false)?;
            timeline.update_selection(
                [EditorialObjectId::Clip(A)],
                SelectionUpdate::Replace,
                SelectionExpansion::Direct,
            )
        })
        .unwrap();
    let fragment = ClipId::from_raw(72);
    apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::razor(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(A),
            RationalTime::new(4, rate()),
            EditorialObjectId::Clip(fragment),
        )],
    )
    .unwrap();

    let state = project.timeline(TIMELINE).unwrap().edit_state();
    let selected: Vec<_> = state.selected_objects().collect();
    assert!(selected.contains(&EditorialObjectId::Clip(A)));
    assert!(selected.contains(&EditorialObjectId::Clip(fragment)));
    let linked: Vec<_> = state.link_for(A).unwrap().members().collect();
    assert_eq!(linked, vec![A, B, fragment]);
    let grouped: Vec<_> = state.group_for(A).unwrap().members().collect();
    assert_eq!(grouped, vec![A, B, C, fragment]);
    let track_state = state.track_state(TRACK).unwrap();
    assert!(track_state.targeted());
    assert!(!track_state.sync_locked());
}

#[test]
fn razor_supports_every_timed_object_domain_and_rejects_boundaries() {
    let gap_id = GapId::from_raw(80);
    let generator_id = GeneratorId::from_raw(81);
    let caption_id = CaptionId::from_raw(82);
    let track = Track::new(
        TRACK,
        "mixed",
        TrackSemantics::Video(VideoTrackSemantics::new(
            FrameRate::FPS_24,
            VideoCompositing::Over,
        )),
        vec![
            TrackItem::Gap(Gap::new(gap_id, "gap", range(0, 4))),
            TrackItem::Generator(Generator::new(
                generator_id,
                "generator",
                "solid",
                BTreeMap::from([("color".to_owned(), "black".to_owned())]),
                range(4, 4),
            )),
            TrackItem::Caption(Caption::new(
                caption_id,
                "caption",
                "hello",
                Some("en".to_owned()),
                range(8, 4),
            )),
        ],
    );
    let mut project = EditorialProject::new(
        ProjectId::from_raw(83),
        "razor domains",
        [],
        [Timeline::new(
            TIMELINE,
            "main",
            rate(),
            RationalTime::zero(rate()),
            vec![track],
        )],
    )
    .unwrap();
    let operations = [
        EditOperation::razor(
            TIMELINE,
            TRACK,
            EditorialObjectId::Gap(gap_id),
            RationalTime::new(2, rate()),
            EditorialObjectId::Gap(GapId::from_raw(84)),
        ),
        EditOperation::razor(
            TIMELINE,
            TRACK,
            EditorialObjectId::Generator(generator_id),
            RationalTime::new(6, rate()),
            EditorialObjectId::Generator(GeneratorId::from_raw(85)),
        ),
        EditOperation::razor(
            TIMELINE,
            TRACK,
            EditorialObjectId::Caption(caption_id),
            RationalTime::new(10, rate()),
            EditorialObjectId::Caption(CaptionId::from_raw(86)),
        ),
    ];
    apply_edit_batch(&mut project, 0, &operations).unwrap();
    let track = project.timeline(TIMELINE).unwrap().track(TRACK).unwrap();
    for id in [
        EditorialObjectId::Gap(GapId::from_raw(84)),
        EditorialObjectId::Generator(GeneratorId::from_raw(85)),
        EditorialObjectId::Caption(CaptionId::from_raw(86)),
    ] {
        assert!(track.item(id).is_some());
    }

    let before = project.clone();
    let error = apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::razor(
            TIMELINE,
            TRACK,
            EditorialObjectId::Gap(gap_id),
            RationalTime::new(0, rate()),
            EditorialObjectId::Gap(GapId::from_raw(87)),
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);
}

#[test]
fn advanced_failures_roll_back_complete_batches_and_preserve_transition_truth() {
    let mut project = project();
    let before = project.clone();
    let error = apply_edit_batch(
        &mut project,
        0,
        &[
            EditOperation::slip(TIMELINE, TRACK, A, RationalTime::new(30, rate())),
            EditOperation::roll(
                TIMELINE,
                TRACK,
                EditorialObjectId::Clip(A),
                EditorialObjectId::Clip(C),
                RationalTime::new(6, rate()),
            ),
        ],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);

    let wrong_clock = Timebase::integer(30).unwrap();
    let error = apply_edit_batch(
        &mut project,
        0,
        &[EditOperation::slip(
            TIMELINE,
            TRACK,
            A,
            RationalTime::new(1, wrong_clock),
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);

    let transition_id = TransitionId::from_raw(88);
    project
        .edit(0, |draft| {
            let track = draft.timeline_mut(TIMELINE)?.track_mut(TRACK)?;
            let mut items = track.items().to_vec();
            items.insert(
                1,
                TrackItem::Transition(Transition::new(
                    transition_id,
                    "wide dissolve",
                    EditorialObjectId::Clip(A),
                    EditorialObjectId::Clip(B),
                    Duration::new(2, rate())?,
                    Duration::new(2, rate())?,
                )),
            );
            track.replace_items(items);
            Ok(())
        })
        .unwrap();
    let result = apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::roll(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(A),
            EditorialObjectId::Clip(B),
            RationalTime::new(1, rate()),
        )],
    )
    .unwrap();
    assert_eq!(result.outcomes()[0].removed_transitions(), &[transition_id]);
    assert!(project
        .timeline(TIMELINE)
        .unwrap()
        .track(TRACK)
        .unwrap()
        .item(EditorialObjectId::Transition(transition_id))
        .is_none());
}

#[test]
fn exact_cross_rate_point_edits_and_role_neutral_commands_are_deterministic() {
    let source_rate = Timebase::integer(48).unwrap();
    let mut cross_rate = project();
    apply_edit_batch(
        &mut cross_rate,
        0,
        &[EditOperation::three_point(
            TIMELINE,
            TRACK,
            point_clip(ClipId::from_raw(90)),
            ThreePointPlacement::SourceRangeAtRecordStart {
                source_range: range_at(60, 8, source_rate),
                record_start: RationalTime::new(4, rate()),
            },
            [],
        )],
    )
    .unwrap();
    assert_eq!(
        project_clip(&cross_rate, ClipId::from_raw(90)).source_range(),
        range_at(60, 8, source_rate)
    );
    assert_eq!(
        project_clip(&cross_rate, ClipId::from_raw(90)).record_range(),
        range(4, 4)
    );

    let operation = EditOperation::slip(TIMELINE, TRACK, A, RationalTime::new(32, rate()));
    let mut editor = project();
    let mut script = project();
    let mut headless = project();
    for candidate in [&mut editor, &mut script, &mut headless] {
        apply_edit_batch(candidate, 0, std::slice::from_ref(&operation)).unwrap();
    }
    assert_eq!(editor, script);
    assert_eq!(script, headless);
}
