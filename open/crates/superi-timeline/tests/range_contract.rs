use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, GapId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::model::{
    Clip, ClipRangeMap, ClipSource, EditorialProject, Gap, LinkedMediaReference, RangeAvailability,
    Timeline, Track, TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
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

fn clip_track(track: u128, clip: Clip) -> Track {
    Track::new(
        TrackId::from_raw(track),
        format!("V{track}"),
        video_semantics(),
        vec![TrackItem::Clip(clip)],
    )
}

#[test]
fn clip_range_map_translates_points_and_subranges_across_exact_clocks() {
    let source_rate = Timebase::integer(48).unwrap();
    let record_rate = Timebase::integer(24).unwrap();
    let ranges = ClipRangeMap::new(range(96, 96, source_rate), range(12, 48, record_rate)).unwrap();

    assert_eq!(
        ranges
            .source_time_to_record(RationalTime::new(120, source_rate))
            .unwrap(),
        RationalTime::new(24, record_rate)
    );
    assert_eq!(
        ranges
            .record_time_to_source(RationalTime::new(36, record_rate))
            .unwrap(),
        RationalTime::new(144, source_rate)
    );
    assert_eq!(
        ranges
            .source_range_to_record(range(120, 24, source_rate))
            .unwrap(),
        range(24, 12, record_rate)
    );
    assert_eq!(
        ranges
            .record_range_to_source(range(24, 12, record_rate))
            .unwrap(),
        range(120, 24, source_rate)
    );

    for error in [
        ranges
            .source_time_to_record(RationalTime::new(192, source_rate))
            .unwrap_err(),
        ranges
            .source_time_to_record(RationalTime::new(95, source_rate))
            .unwrap_err(),
    ] {
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert!(error.message().contains("inside"));
    }

    let inexact = ranges
        .source_time_to_record(RationalTime::new(97, source_rate))
        .unwrap_err();
    assert_eq!(inexact.category(), ErrorCategory::InvalidInput);
    assert!(inexact.message().contains("exact"));
}

#[test]
fn clip_range_construction_and_direct_replacement_cannot_publish_desynchronization() {
    let source_rate = Timebase::integer(48).unwrap();
    let record_rate = Timebase::integer(24).unwrap();
    let mut clip = Clip::new(
        ClipId::from_raw(10),
        "shot",
        ClipSource::Media(MediaId::from_raw(20)),
        range(0, 48, source_rate),
        range(0, 24, record_rate),
    )
    .unwrap();
    let original = clip.clone();

    let error = clip
        .set_source_range(range(1, 47, source_rate))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(clip, original);

    let error = clip
        .set_record_range(range(0, 23, record_rate))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(clip, original);

    clip.set_ranges(range(96, 48, source_rate), range(24, 24, record_rate))
        .unwrap();
    assert_eq!(clip.source(), ClipSource::Media(MediaId::from_raw(20)));
    assert_eq!(clip.source_range(), range(96, 48, source_rate));
    assert_eq!(clip.record_range(), range(24, 24, record_rate));

    for result in [
        ClipRangeMap::new(range(0, 0, source_rate), range(0, 0, record_rate)),
        ClipRangeMap::new(range(0, 48, source_rate), range(0, 23, record_rate)),
    ] {
        let error = result.unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
    }
}

#[test]
fn media_availability_is_resolved_without_destroying_overscan_or_unknown_state() {
    let rate = Timebase::integer(24).unwrap();
    let availability_rate = Timebase::integer(48).unwrap();
    let known_media = MediaId::from_raw(100);
    let unknown_media = MediaId::from_raw(101);
    let fully_available = ClipId::from_raw(110);
    let partly_available = ClipId::from_raw(111);
    let unavailable = ClipId::from_raw(112);
    let unknown = ClipId::from_raw(113);

    let clips = [
        (
            200,
            Clip::new(
                fully_available,
                "available",
                ClipSource::Media(known_media),
                range(10, 10, rate),
                range(0, 10, rate),
            )
            .unwrap(),
        ),
        (
            201,
            Clip::new(
                partly_available,
                "partial overscan",
                ClipSource::Media(known_media),
                range(-5, 10, rate),
                range(0, 10, rate),
            )
            .unwrap(),
        ),
        (
            202,
            Clip::new(
                unavailable,
                "offline overscan",
                ClipSource::Media(known_media),
                range(200, 10, rate),
                range(0, 10, rate),
            )
            .unwrap(),
        ),
        (
            203,
            Clip::new(
                unknown,
                "unknown media extent",
                ClipSource::Media(unknown_media),
                range(0, 10, rate),
                range(0, 10, rate),
            )
            .unwrap(),
        ),
    ];
    let timeline = Timeline::new(
        TimelineId::from_raw(300),
        "availability",
        rate,
        RationalTime::zero(rate),
        clips
            .into_iter()
            .map(|(track, clip)| clip_track(track, clip))
            .collect(),
    );
    let project = EditorialProject::new(
        ProjectId::from_raw(400),
        "availability project",
        [
            LinkedMediaReference::new(
                known_media,
                "known",
                "urn:known",
                Some(range(0, 200, availability_rate)),
            ),
            LinkedMediaReference::new(unknown_media, "unknown", "urn:unknown", None),
        ],
        [timeline],
    )
    .unwrap();

    let full = project.clip_range_context(fully_available).unwrap();
    assert_eq!(full.source(), ClipSource::Media(known_media));
    assert_eq!(full.source_range(), range(10, 10, rate));
    assert_eq!(full.record_range(), range(0, 10, rate));
    assert_eq!(
        full.available_range(),
        Some(range(0, 200, availability_rate))
    );
    assert_eq!(
        full.availability().unwrap(),
        RangeAvailability::FullyAvailable
    );

    assert_eq!(
        project
            .clip_range_context(partly_available)
            .unwrap()
            .availability()
            .unwrap(),
        RangeAvailability::PartiallyAvailable
    );
    assert_eq!(
        project
            .clip_range_context(unavailable)
            .unwrap()
            .availability()
            .unwrap(),
        RangeAvailability::Unavailable
    );
    let unknown_context = project.clip_range_context(unknown).unwrap();
    assert_eq!(unknown_context.available_range(), None);
    assert_eq!(
        unknown_context.availability().unwrap(),
        RangeAvailability::Unknown
    );
}

#[test]
fn nested_timeline_availability_is_derived_without_losing_the_typed_link() {
    let rate = Timebase::integer(24).unwrap();
    let nested_timeline = TimelineId::from_raw(500);
    let main_timeline = TimelineId::from_raw(501);
    let nested_clip = ClipId::from_raw(510);
    let nested = Timeline::new(
        nested_timeline,
        "nested",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            TrackId::from_raw(520),
            "V1",
            video_semantics(),
            vec![TrackItem::Gap(Gap::new(
                GapId::from_raw(530),
                "nested content",
                range(0, 24, rate),
            ))],
        )],
    );
    let main = Timeline::new(
        main_timeline,
        "main",
        rate,
        RationalTime::zero(rate),
        vec![clip_track(
            521,
            Clip::new(
                nested_clip,
                "nested clip",
                ClipSource::Timeline(nested_timeline),
                range(4, 16, rate),
                range(0, 16, rate),
            )
            .unwrap(),
        )],
    );
    let project = EditorialProject::new(
        ProjectId::from_raw(540),
        "nested ranges",
        [],
        [nested, main],
    )
    .unwrap();

    let context = project.clip_range_context(nested_clip).unwrap();
    assert_eq!(context.source(), ClipSource::Timeline(nested_timeline));
    assert_eq!(context.source_range(), range(4, 16, rate));
    assert_eq!(context.record_range(), range(0, 16, rate));
    assert_eq!(context.available_range(), Some(range(0, 24, rate)));
    assert_eq!(
        context.availability().unwrap(),
        RangeAvailability::FullyAvailable
    );
}
