use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::edit_ops::{apply_edit_batch, EditKind, EditOperation, TrackDurationChange};
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};
use superi_timeline::retime::{ClipTimeMap, PlaybackRate, RetimeSegment};

const MEDIA: MediaId = MediaId::from_raw(1);
const TIMELINE: TimelineId = TimelineId::from_raw(2);
const TRACK: TrackId = TrackId::from_raw(3);
const CLIP: ClipId = ClipId::from_raw(4);

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

fn project() -> EditorialProject {
    EditorialProject::new(
        ProjectId::from_raw(5),
        "retime edit",
        [LinkedMediaReference::new(
            MEDIA,
            "camera",
            "urn:superi:test:retime-edit",
            Some(range(0, 240)),
        )],
        [Timeline::new(
            TIMELINE,
            "main",
            rate(),
            RationalTime::zero(rate()),
            vec![Track::new(
                TRACK,
                "V1",
                TrackSemantics::Video(VideoTrackSemantics::new(
                    FrameRate::FPS_24,
                    VideoCompositing::Over,
                )),
                vec![TrackItem::Clip(
                    Clip::new(
                        CLIP,
                        "shot",
                        ClipSource::Media(MEDIA),
                        range(40, 8),
                        range(0, 8),
                    )
                    .unwrap(),
                )],
            )],
        )],
    )
    .unwrap()
}

fn project_clip(project: &EditorialProject) -> &Clip {
    project
        .timeline(TIMELINE)
        .unwrap()
        .track(TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(CLIP))
        .unwrap()
        .as_clip()
        .unwrap()
}

#[test]
fn retime_edit_applies_exact_map_and_reports_stable_identity() {
    let mut project = project();
    let time_map = ClipTimeMap::speed(
        Duration::new(8, rate()).unwrap(),
        RationalTime::new(40, rate()),
        PlaybackRate::new(2, 1).unwrap(),
    )
    .unwrap();

    let result = apply_edit_batch(
        &mut project,
        0,
        &[EditOperation::retime(
            TIMELINE,
            TRACK,
            CLIP,
            time_map.clone(),
        )],
    )
    .unwrap();

    let outcome = &result.outcomes()[0];
    assert_eq!(outcome.kind(), EditKind::Retime);
    assert_eq!(outcome.affected_range(), range(0, 8));
    assert_eq!(outcome.duration_change(), TrackDurationChange::Unchanged);
    assert_eq!(outcome.modified_ids(), &[EditorialObjectId::Clip(CLIP)]);
    assert!(outcome.inserted_ids().is_empty());
    assert!(outcome.removed_ids().is_empty());
    assert_eq!(project_clip(&project).record_range(), range(0, 8));
    assert_eq!(project_clip(&project).time_map(), &time_map);
}

#[test]
fn every_authored_retime_mode_uses_the_same_exact_edit_path() {
    let duration = Duration::new(8, rate()).unwrap();
    let maps = [
        ClipTimeMap::speed(
            duration,
            RationalTime::new(40, rate()),
            PlaybackRate::new(2, 1).unwrap(),
        )
        .unwrap(),
        ClipTimeMap::reverse(duration, RationalTime::new(47, rate())).unwrap(),
        ClipTimeMap::freeze(duration, RationalTime::new(43, rate())).unwrap(),
        ClipTimeMap::new(
            duration,
            rate(),
            [
                RetimeSegment::new(
                    range(0, 4),
                    RationalTime::new(40, rate()),
                    PlaybackRate::new(2, 1).unwrap(),
                )
                .unwrap(),
                RetimeSegment::new(
                    range(4, 4),
                    RationalTime::new(48, rate()),
                    PlaybackRate::new(1, 2).unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap(),
    ];

    for time_map in maps {
        let mut project = project();
        apply_edit_batch(
            &mut project,
            0,
            &[EditOperation::retime(
                TIMELINE,
                TRACK,
                CLIP,
                time_map.clone(),
            )],
        )
        .unwrap();
        assert_eq!(project_clip(&project).time_map(), &time_map);
        assert_eq!(project_clip(&project).record_range(), range(0, 8));
    }
}

#[test]
fn retime_rejects_noops_locked_tracks_and_wrong_targets_atomically() {
    let replacement = ClipTimeMap::freeze(
        Duration::new(8, rate()).unwrap(),
        RationalTime::new(43, rate()),
    )
    .unwrap();

    let mut no_op = project();
    let before = no_op.clone();
    let current = project_clip(&no_op).time_map().clone();
    let error = apply_edit_batch(
        &mut no_op,
        0,
        &[EditOperation::retime(TIMELINE, TRACK, CLIP, current)],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(no_op, before);

    let mut locked = project();
    locked
        .edit(0, |draft| {
            draft.timeline_mut(TIMELINE)?.set_track_locked(TRACK, true)
        })
        .unwrap();
    let before = locked.clone();
    let error = apply_edit_batch(
        &mut locked,
        1,
        &[EditOperation::retime(
            TIMELINE,
            TRACK,
            CLIP,
            replacement.clone(),
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(locked, before);

    for operation in [
        EditOperation::retime(TIMELINE, TRACK, ClipId::from_raw(99), replacement.clone()),
        EditOperation::retime(TIMELINE, TrackId::from_raw(99), CLIP, replacement.clone()),
    ] {
        let mut project = project();
        let before = project.clone();
        let error = apply_edit_batch(&mut project, 0, &[operation]).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::NotFound);
        assert_eq!(project, before);
    }
}
