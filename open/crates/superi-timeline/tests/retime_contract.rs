use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, GapId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, TimeRounding, Timebase};
use superi_timeline::edit_ops::{apply_edit_batch, EditOperation};
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, Gap, LinkedMediaReference,
    SampleAvailability, Timeline, Track, TrackItem, TrackSemantics, VideoCompositing,
    VideoTrackSemantics,
};
use superi_timeline::retime::{
    ClipTimeMap, PlaybackRate, RetimeMode, RetimeResolution, RetimeSegment,
};

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

#[test]
fn speed_reverse_and_freeze_resolve_exact_source_samples() {
    let record_rate = Timebase::integer(24).unwrap();
    let source_rate = Timebase::integer(48).unwrap();
    let duration = Duration::new(8, record_rate).unwrap();

    let speed = ClipTimeMap::speed(
        duration,
        RationalTime::new(40, source_rate),
        PlaybackRate::new(2, 1).unwrap(),
    )
    .unwrap();
    assert_eq!(speed.mode(), RetimeMode::SpeedChange);
    let sample = speed
        .source_time_at(RationalTime::new(3, record_rate), TimeRounding::Exact)
        .unwrap();
    assert_eq!(sample.time(), RationalTime::new(52, source_rate));
    assert_eq!(sample.resolution(), RetimeResolution::Exact);
    assert_eq!(sample.segment_index(), 0);

    let reverse = ClipTimeMap::reverse(duration, RationalTime::new(100, source_rate)).unwrap();
    assert_eq!(reverse.mode(), RetimeMode::Reverse);
    assert_eq!(
        reverse
            .source_time_at(RationalTime::new(3, record_rate), TimeRounding::Exact)
            .unwrap()
            .time(),
        RationalTime::new(94, source_rate)
    );

    let freeze = ClipTimeMap::freeze(duration, RationalTime::new(77, source_rate)).unwrap();
    assert_eq!(freeze.mode(), RetimeMode::Freeze);
    for record in [0, 3, 7] {
        let sample = freeze
            .source_time_at(RationalTime::new(record, record_rate), TimeRounding::Exact)
            .unwrap();
        assert_eq!(sample.time(), RationalTime::new(77, source_rate));
        assert_eq!(sample.resolution(), RetimeResolution::Held);
    }
}

#[test]
fn piecewise_remaps_require_exact_coverage_and_continuous_source_seams() {
    let record_rate = Timebase::integer(24).unwrap();
    let source_rate = Timebase::integer(48).unwrap();
    let duration = Duration::new(8, record_rate).unwrap();
    let segments = [
        RetimeSegment::new(
            range(0, 4, record_rate),
            RationalTime::new(0, source_rate),
            PlaybackRate::new(2, 1).unwrap(),
        )
        .unwrap(),
        RetimeSegment::new(
            range(4, 4, record_rate),
            RationalTime::new(16, source_rate),
            PlaybackRate::new(1, 1).unwrap(),
        )
        .unwrap(),
    ];
    let remap = ClipTimeMap::new(duration, source_rate, segments).unwrap();
    assert_eq!(remap.mode(), RetimeMode::TimeRemap);
    assert_eq!(remap.segments().len(), 2);
    let seam = remap
        .source_time_at(RationalTime::new(4, record_rate), TimeRounding::Exact)
        .unwrap();
    assert_eq!(seam.segment_index(), 1);
    assert_eq!(seam.time(), RationalTime::new(16, source_rate));

    let discontinuous = ClipTimeMap::new(
        duration,
        source_rate,
        [
            remap.segments()[0].clone(),
            RetimeSegment::new(
                range(4, 4, record_rate),
                RationalTime::new(17, source_rate),
                PlaybackRate::new(1, 1).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap_err();
    assert_eq!(discontinuous.category(), ErrorCategory::InvalidInput);
    assert!(discontinuous.message().contains("continuous"));

    let gap = ClipTimeMap::new(
        duration,
        source_rate,
        [
            RetimeSegment::new(
                range(0, 3, record_rate),
                RationalTime::new(0, source_rate),
                PlaybackRate::new(2, 1).unwrap(),
            )
            .unwrap(),
            RetimeSegment::new(
                range(4, 4, record_rate),
                RationalTime::new(12, source_rate),
                PlaybackRate::NORMAL,
            )
            .unwrap(),
        ],
    )
    .unwrap_err();
    assert!(gap.message().contains("gapless"));

    let quantized_source_rate = Timebase::integer(24).unwrap();
    let quantized = ClipTimeMap::speed(
        duration,
        RationalTime::zero(quantized_source_rate),
        PlaybackRate::new(1, 2).unwrap(),
    )
    .unwrap();
    let exact = quantized
        .source_time_at(RationalTime::new(1, record_rate), TimeRounding::Exact)
        .unwrap_err();
    assert!(exact.message().contains("exact"));
    let rounded = quantized
        .source_time_at(
            RationalTime::new(1, record_rate),
            TimeRounding::NearestTiesEven,
        )
        .unwrap();
    assert_eq!(rounded.time(), RationalTime::new(0, quantized_source_rate));
    assert_eq!(
        rounded.resolution(),
        RetimeResolution::Rounded(TimeRounding::NearestTiesEven)
    );
}

#[test]
fn project_queries_report_availability_without_losing_linked_editorial_intent() {
    let record_rate = Timebase::integer(24).unwrap();
    let source_rate = Timebase::integer(48).unwrap();
    let media = MediaId::from_raw(1);
    let timeline_id = TimelineId::from_raw(2);
    let track_id = TrackId::from_raw(3);
    let clip_id = ClipId::from_raw(4);
    let linked_id = ClipId::from_raw(5);
    let clip = Clip::new(
        clip_id,
        "retimed",
        ClipSource::Media(media),
        range(40, 16, source_rate),
        range(0, 8, record_rate),
    )
    .unwrap();
    let linked = Clip::new(
        linked_id,
        "linked",
        ClipSource::Media(media),
        range(56, 16, source_rate),
        range(8, 8, record_rate),
    )
    .unwrap();
    let timeline = Timeline::new(
        timeline_id,
        "main",
        record_rate,
        RationalTime::zero(record_rate),
        vec![Track::new(
            track_id,
            "V1",
            video_semantics(),
            vec![TrackItem::Clip(clip), TrackItem::Clip(linked)],
        )],
    );
    let mut project = EditorialProject::new(
        ProjectId::from_raw(6),
        "retime",
        [LinkedMediaReference::new(
            media,
            "camera",
            "urn:camera",
            Some(range(0, 50, source_rate)),
        )],
        [timeline],
    )
    .unwrap();

    project
        .edit(0, |draft| {
            let timeline = draft.timeline_mut(timeline_id)?;
            timeline.link_clips([clip_id, linked_id])?;
            let clip = timeline
                .track_mut(track_id)?
                .item_mut(EditorialObjectId::Clip(clip_id))?
                .as_clip_mut()
                .unwrap();
            clip.set_time_map(ClipTimeMap::speed(
                clip.record_range().duration(),
                RationalTime::new(40, source_rate),
                PlaybackRate::new(2, 1)?,
            )?)?;
            Ok(())
        })
        .unwrap();

    let context = project.clip_range_context(clip_id).unwrap();
    let available = context
        .playback_sample(RationalTime::new(2, record_rate), TimeRounding::Exact)
        .unwrap();
    assert_eq!(available.source_time(), RationalTime::new(48, source_rate));
    assert_eq!(available.availability(), SampleAvailability::Available);
    let unavailable = context
        .playback_sample(RationalTime::new(4, record_rate), TimeRounding::Exact)
        .unwrap();
    assert_eq!(
        unavailable.source_time(),
        RationalTime::new(56, source_rate)
    );
    assert_eq!(unavailable.availability(), SampleAvailability::Unavailable);

    let state = project.timeline(timeline_id).unwrap().edit_state();
    assert_eq!(
        state
            .link_for(clip_id)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        vec![clip_id, linked_id]
    );
}

#[test]
fn clip_time_map_binding_is_atomic_and_rejects_implicit_retime_loss() {
    let record_rate = Timebase::integer(24).unwrap();
    let source_rate = Timebase::integer(48).unwrap();
    let mut clip = Clip::new(
        ClipId::from_raw(20),
        "atomic",
        ClipSource::Media(MediaId::from_raw(21)),
        range(0, 16, source_rate),
        range(0, 8, record_rate),
    )
    .unwrap();
    clip.set_time_map(
        ClipTimeMap::speed(
            clip.record_range().duration(),
            RationalTime::new(4, source_rate),
            PlaybackRate::new(2, 1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let retimed = clip.clone();

    let wrong_duration = ClipTimeMap::speed(
        Duration::new(7, record_rate).unwrap(),
        RationalTime::new(4, source_rate),
        PlaybackRate::NORMAL,
    )
    .unwrap();
    let error = clip.set_time_map(wrong_duration).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(clip, retimed);

    let error = clip
        .set_ranges(range(0, 8, source_rate), range(0, 4, record_rate))
        .unwrap_err();
    assert!(error.message().contains("preserve the clip duration"));
    assert_eq!(clip, retimed);

    let end_error = clip
        .source_time_at(
            clip.record_range().end_exclusive().unwrap(),
            TimeRounding::Exact,
        )
        .unwrap_err();
    assert!(end_error.message().contains("inside"));
}

#[test]
fn identity_range_replacement_retains_existing_resize_behavior() {
    let record_rate = Timebase::integer(24).unwrap();
    let source_rate = Timebase::integer(48).unwrap();
    let mut clip = Clip::new(
        ClipId::from_raw(22),
        "identity resize",
        ClipSource::Media(MediaId::from_raw(23)),
        range(0, 16, source_rate),
        range(0, 8, record_rate),
    )
    .unwrap();

    clip.set_ranges(range(20, 8, source_rate), range(10, 4, record_rate))
        .unwrap();

    assert_eq!(clip.time_map().mode(), RetimeMode::Identity);
    assert_eq!(
        clip.source_time_at(RationalTime::new(13, record_rate), TimeRounding::Exact)
            .unwrap()
            .time(),
        RationalTime::new(26, source_rate)
    );
}

#[test]
fn insert_split_and_record_shift_preserve_the_complete_retime_mapping() {
    let record_rate = Timebase::integer(24).unwrap();
    let source_rate = Timebase::integer(48).unwrap();
    let media = MediaId::from_raw(30);
    let timeline_id = TimelineId::from_raw(31);
    let track_id = TrackId::from_raw(32);
    let clip_id = ClipId::from_raw(33);
    let fragment_id = ClipId::from_raw(34);
    let mut clip = Clip::new(
        clip_id,
        "split retime",
        ClipSource::Media(media),
        range(0, 16, source_rate),
        range(0, 8, record_rate),
    )
    .unwrap();
    clip.set_time_map(
        ClipTimeMap::new(
            clip.record_range().duration(),
            source_rate,
            [
                RetimeSegment::new(
                    range(0, 4, record_rate),
                    RationalTime::zero(source_rate),
                    PlaybackRate::new(2, 1).unwrap(),
                )
                .unwrap(),
                RetimeSegment::new(
                    range(4, 4, record_rate),
                    RationalTime::new(16, source_rate),
                    PlaybackRate::NORMAL,
                )
                .unwrap(),
            ],
        )
        .unwrap(),
    )
    .unwrap();
    let timeline = Timeline::new(
        timeline_id,
        "edit",
        record_rate,
        RationalTime::zero(record_rate),
        vec![Track::new(
            track_id,
            "V1",
            video_semantics(),
            vec![TrackItem::Clip(clip)],
        )],
    );
    let mut project = EditorialProject::new(
        ProjectId::from_raw(35),
        "edit retime",
        [LinkedMediaReference::new(
            media,
            "camera",
            "urn:camera",
            Some(range(0, 200, source_rate)),
        )],
        [timeline],
    )
    .unwrap();

    apply_edit_batch(
        &mut project,
        0,
        &[EditOperation::insert(
            timeline_id,
            track_id,
            RationalTime::new(2, record_rate),
            TrackItem::Gap(Gap::new(
                GapId::from_raw(36),
                "inserted gap",
                range(0, 2, record_rate),
            )),
            [EditorialObjectId::Clip(fragment_id)],
        )],
    )
    .unwrap();

    let track = project
        .timeline(timeline_id)
        .unwrap()
        .track(track_id)
        .unwrap();
    let left = track
        .item(EditorialObjectId::Clip(clip_id))
        .unwrap()
        .as_clip()
        .unwrap();
    let right = track
        .item(EditorialObjectId::Clip(fragment_id))
        .unwrap()
        .as_clip()
        .unwrap();
    assert_eq!(left.time_map().mode(), RetimeMode::SpeedChange);
    assert_eq!(right.time_map().mode(), RetimeMode::TimeRemap);
    assert_eq!(
        left.source_time_at(RationalTime::new(1, record_rate), TimeRounding::Exact)
            .unwrap()
            .time(),
        RationalTime::new(4, source_rate)
    );
    assert_eq!(right.record_range(), range(4, 6, record_rate));
    assert_eq!(
        right
            .source_time_at(RationalTime::new(4, record_rate), TimeRounding::Exact)
            .unwrap()
            .time(),
        RationalTime::new(8, source_rate)
    );
    assert_eq!(
        right
            .source_time_at(RationalTime::new(6, record_rate), TimeRounding::Exact)
            .unwrap()
            .time(),
        RationalTime::new(16, source_rate)
    );
    assert_eq!(
        right
            .source_time_at(RationalTime::new(9, record_rate), TimeRounding::Exact)
            .unwrap()
            .time(),
        RationalTime::new(22, source_rate)
    );
}
