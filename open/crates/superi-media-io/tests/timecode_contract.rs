use superi_core::time::{Duration, RationalTime, TimeRounding, Timebase};
use superi_media_io::demux::StreamEdit;
use superi_media_io::timecode::{EditMapping, EditTimeline, TimestampNormalizer};

#[test]
fn timestamp_normalization_keeps_decode_and_edit_coordinates_coherent() {
    let media_timebase = Timebase::integer(1_000).unwrap();
    let normalizer = TimestampNormalizer::new(RationalTime::new(100, media_timebase));

    assert_eq!(
        normalizer
            .normalize(RationalTime::new(100, media_timebase))
            .unwrap()
            .value(),
        0
    );
    assert_eq!(
        normalizer
            .normalize(RationalTime::new(0, media_timebase))
            .unwrap()
            .value(),
        -100
    );
    assert_eq!(
        normalizer
            .restore(RationalTime::new(900, media_timebase))
            .unwrap()
            .value(),
        1_000
    );
    assert!(normalizer
        .normalize(RationalTime::new(1, Timebase::integer(48_000).unwrap()))
        .is_err());
}

#[test]
fn edit_timeline_maps_empty_edits_and_media_coordinates_exactly() {
    let movie_timebase = Timebase::integer(1_000).unwrap();
    let media_timebase = Timebase::integer(1_000).unwrap();
    let timeline = EditTimeline::new(
        movie_timebase,
        media_timebase,
        vec![
            StreamEdit::new(Duration::new(500, movie_timebase).unwrap(), None, 1, 0),
            StreamEdit::new(
                Duration::new(1_500, movie_timebase).unwrap(),
                Some(RationalTime::zero(media_timebase)),
                1,
                0,
            ),
        ],
    )
    .unwrap();

    assert_eq!(
        timeline
            .presentation_to_media(RationalTime::new(250, movie_timebase), TimeRounding::Exact,)
            .unwrap(),
        EditMapping::Empty { edit_index: 0 }
    );
    assert_eq!(
        timeline
            .presentation_to_media(RationalTime::new(950, movie_timebase), TimeRounding::Exact,)
            .unwrap(),
        EditMapping::Media {
            edit_index: 1,
            media_time: RationalTime::new(450, media_timebase),
        }
    );
    assert_eq!(
        timeline
            .media_to_presentations(RationalTime::new(900, media_timebase), TimeRounding::Exact,)
            .unwrap(),
        [RationalTime::new(1_400, movie_timebase)]
    );
    assert_eq!(
        timeline
            .presentation_to_media(
                RationalTime::new(2_000, movie_timebase),
                TimeRounding::Exact,
            )
            .unwrap(),
        EditMapping::Outside
    );
}

#[test]
fn edit_timeline_supports_fixed_point_rates_and_repeated_media() {
    let movie_timebase = Timebase::integer(1_000).unwrap();
    let media_timebase = Timebase::integer(100).unwrap();
    let timeline = EditTimeline::new(
        movie_timebase,
        media_timebase,
        vec![
            StreamEdit::new(
                Duration::new(500, movie_timebase).unwrap(),
                Some(RationalTime::zero(media_timebase)),
                2,
                0,
            ),
            StreamEdit::new(
                Duration::new(1_000, movie_timebase).unwrap(),
                Some(RationalTime::zero(media_timebase)),
                1,
                0,
            ),
        ],
    )
    .unwrap();

    assert_eq!(
        timeline
            .presentation_to_media(RationalTime::new(250, movie_timebase), TimeRounding::Exact,)
            .unwrap(),
        EditMapping::Media {
            edit_index: 0,
            media_time: RationalTime::new(50, media_timebase),
        }
    );
    assert_eq!(
        timeline
            .media_to_presentations(RationalTime::new(50, media_timebase), TimeRounding::Exact,)
            .unwrap(),
        [
            RationalTime::new(250, movie_timebase),
            RationalTime::new(1_000, movie_timebase),
        ]
    );

    let dwell = EditTimeline::new(
        movie_timebase,
        media_timebase,
        vec![StreamEdit::new(
            Duration::new(1_000, movie_timebase).unwrap(),
            Some(RationalTime::new(25, media_timebase)),
            0,
            0,
        )],
    )
    .unwrap();
    assert_eq!(
        dwell
            .presentation_to_media(RationalTime::new(750, movie_timebase), TimeRounding::Exact)
            .unwrap(),
        EditMapping::Media {
            edit_index: 0,
            media_time: RationalTime::new(25, media_timebase),
        }
    );
    assert_eq!(
        dwell
            .media_to_presentations(RationalTime::new(25, media_timebase), TimeRounding::Exact)
            .unwrap(),
        [RationalTime::zero(movie_timebase)]
    );

    let half_rate = EditTimeline::new(
        movie_timebase,
        media_timebase,
        vec![StreamEdit::new(
            Duration::new(1_000, movie_timebase).unwrap(),
            Some(RationalTime::zero(media_timebase)),
            0,
            i16::MIN,
        )],
    )
    .unwrap();
    assert_eq!(
        half_rate
            .presentation_to_media(
                RationalTime::new(1_000, movie_timebase),
                TimeRounding::Exact,
            )
            .unwrap(),
        EditMapping::Outside
    );
    assert_eq!(
        half_rate
            .presentation_to_media(RationalTime::new(500, movie_timebase), TimeRounding::Exact,)
            .unwrap(),
        EditMapping::Media {
            edit_index: 0,
            media_time: RationalTime::new(25, media_timebase),
        }
    );
}

#[test]
fn reverse_mapping_never_rounds_onto_the_exclusive_edit_end() {
    let movie_timebase = Timebase::integer(10).unwrap();
    let media_timebase = Timebase::integer(3).unwrap();
    let timeline = EditTimeline::new(
        movie_timebase,
        media_timebase,
        vec![StreamEdit::new(
            Duration::new(10, movie_timebase).unwrap(),
            Some(RationalTime::zero(media_timebase)),
            1,
            0,
        )],
    )
    .unwrap();

    assert!(timeline
        .media_to_presentations(RationalTime::new(3, media_timebase), TimeRounding::Ceil)
        .unwrap()
        .is_empty());
}
