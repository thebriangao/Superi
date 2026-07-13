use std::sync::Arc;

use superi_core::error::ErrorCategory;
use superi_core::time::{Duration, RationalTime, TimeRange, Timebase};
use superi_core::timecode::{Timecode, TimecodeMode};
use superi_media_io::demux::StreamId;
use superi_media_io::timecode::{
    SourceTimecodeTrack, TimecodeDescription, TimecodeFlags, TimecodeSampleEncoding,
    TimecodeSegment, TimecodeValue,
};

fn drop_frame_description(encoding: TimecodeSampleEncoding) -> TimecodeDescription {
    TimecodeDescription::new(
        encoding,
        TimecodeFlags::new(
            TimecodeFlags::DROP_FRAME
                | TimecodeFlags::WRAPS_AT_24_HOURS
                | TimecodeFlags::NEGATIVE_TIMES_ALLOWED
                | 0x1000,
        ),
        2_997,
        100,
        30,
        Arc::from(&b"camera-a"[..]),
    )
    .expect("Apple's documented 29.97 drop-frame description")
}

#[test]
fn quicktime_drop_frame_description_preserves_raw_fields_and_uses_canonical_counting() {
    let description = drop_frame_description(TimecodeSampleEncoding::Timecode32);

    assert_eq!(description.time_scale(), 2_997);
    assert_eq!(description.frame_duration(), 100);
    assert_eq!(description.frame_quanta(), 30);
    assert_eq!(description.source_reference(), b"camera-a");
    assert_eq!(description.media_frame_rate().numerator(), 2_997);
    assert_eq!(description.media_frame_rate().denominator(), 100);
    assert_eq!(
        description.timecode_format().mode(),
        TimecodeMode::DropFrame
    );
    assert_eq!(description.timecode_format().rate().numerator(), 30_000);
    assert_eq!(description.timecode_format().rate().denominator(), 1_001);
    assert!(description.flags().wraps_at_24_hours());
    assert!(description.flags().negative_times_allowed());
    assert!(!description.flags().is_counter());
    assert_eq!(description.flags().unknown_bits(), 0x1000);
}

#[test]
fn timecode32_and_timecode64_samples_round_trip_without_losing_drop_frame_semantics() {
    let description32 = drop_frame_description(TimecodeSampleEncoding::Timecode32);
    let encoded32 = 1_800_i32.to_be_bytes();
    let value32 = description32.decode_sample(&encoded32).unwrap();
    let TimecodeValue::Timecode(timecode32) = value32 else {
        panic!("a non-counter description must decode timecode");
    };
    assert_eq!(timecode32.to_string(), "00:01:00;02");
    assert_eq!(description32.encode_sample(value32).unwrap(), encoded32);

    let description64 = drop_frame_description(TimecodeSampleEncoding::Timecode64);
    let encoded64 = 5_000_000_000_i64.to_be_bytes();
    let value64 = description64.decode_sample(&encoded64).unwrap();
    assert_eq!(description64.encode_sample(value64).unwrap(), encoded64);
}

#[test]
fn timecode_track_maps_media_time_across_a_drop_frame_minute_boundary() {
    let description = drop_frame_description(TimecodeSampleEncoding::Timecode32);
    let track_timebase = Timebase::integer(2_997).unwrap();
    let segment = TimecodeSegment::new(
        TimeRange::new(
            RationalTime::zero(track_timebase),
            Duration::new(2_997, track_timebase).unwrap(),
        )
        .unwrap(),
        TimecodeValue::Timecode(description.timecode_from_frames(1_799).unwrap()),
    );
    let track = SourceTimecodeTrack::new(
        StreamId::new(3),
        description,
        vec![StreamId::new(1), StreamId::new(2)],
        vec![segment],
    )
    .unwrap();

    assert_eq!(track.stream_id(), StreamId::new(3));
    assert_eq!(
        track.associated_stream_ids(),
        &[StreamId::new(1), StreamId::new(2)]
    );
    assert_eq!(track.segments(), &[segment]);

    assert_eq!(
        track
            .timecode_at(RationalTime::new(0, track_timebase))
            .unwrap()
            .unwrap()
            .to_string(),
        "00:00:59;29"
    );
    assert_eq!(
        track
            .timecode_at(RationalTime::new(100, track_timebase))
            .unwrap()
            .unwrap()
            .to_string(),
        "00:01:00;02"
    );
    assert_eq!(
        track
            .timecode_at(RationalTime::new(2_997, track_timebase))
            .unwrap(),
        None
    );
}

#[test]
fn counter_metadata_remains_distinct_from_editorial_timecode() {
    let description = TimecodeDescription::new(
        TimecodeSampleEncoding::Timecode32,
        TimecodeFlags::new(TimecodeFlags::COUNTER),
        1_000,
        1,
        100,
        Arc::from(&b"deck-counter"[..]),
    )
    .unwrap();
    let encoded = 4_200_u32.to_be_bytes();
    let value = description.decode_sample(&encoded).unwrap();
    assert_eq!(value, TimecodeValue::Counter(42));
    assert_eq!(description.encode_sample(value).unwrap(), encoded);
    assert_eq!(
        description
            .decode_sample(&4_201_u32.to_be_bytes())
            .unwrap_err()
            .category(),
        ErrorCategory::CorruptData
    );
    assert_eq!(
        description
            .encode_sample(TimecodeValue::Counter(u64::from(u32::MAX) / 100 + 1))
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    let timebase = Timebase::integer(1_000).unwrap();
    let track = SourceTimecodeTrack::new(
        StreamId::new(9),
        description,
        Vec::new(),
        vec![TimecodeSegment::new(
            TimeRange::new(
                RationalTime::zero(timebase),
                Duration::new(1_000, timebase).unwrap(),
            )
            .unwrap(),
            value,
        )],
    )
    .unwrap();
    let error = track.timecode_at(RationalTime::zero(timebase)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
}

#[test]
fn source_projection_wraps_at_24_hours_without_changing_core_arithmetic() {
    let description = drop_frame_description(TimecodeSampleEncoding::Timecode32);
    let core = Timecode::parse("23:59:59;29", description.timecode_format()).unwrap();
    assert_eq!(
        core.checked_add_frames(1).unwrap().to_string(),
        "24:00:00;00"
    );
    let start = description.timecode_from_label("23:59:59;29").unwrap();

    let track_timebase = Timebase::integer(2_997).unwrap();
    let track = SourceTimecodeTrack::new(
        StreamId::new(3),
        description,
        vec![StreamId::new(1)],
        vec![TimecodeSegment::new(
            TimeRange::new(
                RationalTime::zero(track_timebase),
                Duration::new(200, track_timebase).unwrap(),
            )
            .unwrap(),
            TimecodeValue::Timecode(start),
        )],
    )
    .unwrap();

    assert_eq!(
        track
            .timecode_at(RationalTime::new(100, track_timebase))
            .unwrap()
            .unwrap()
            .to_string(),
        "00:00:00;00"
    );
}

#[test]
fn source_timecode_keeps_physical_time_distinct_from_label_counting() {
    let description = drop_frame_description(TimecodeSampleEncoding::Timecode32);
    let source = description.timecode_from_frames(10_000_000).unwrap();
    let physical = RationalTime::from_frames(10_000_000, description.media_frame_rate());
    let counting = RationalTime::from_frames(10_000_000, description.timecode_format().rate());

    assert_eq!(source.physical_frame_rate(), description.media_frame_rate());
    assert_eq!(source.to_rational_time(), physical);
    assert_ne!(source.to_rational_time(), counting);
}

#[test]
fn validated_tracks_require_every_segment_to_round_trip_through_its_sample_width() {
    let description32 = TimecodeDescription::new(
        TimecodeSampleEncoding::Timecode32,
        TimecodeFlags::new(0),
        30,
        1,
        30,
        Arc::from(&b""[..]),
    )
    .unwrap();
    let timebase = Timebase::integer(30).unwrap();
    let range = TimeRange::new(
        RationalTime::zero(timebase),
        Duration::new(1, timebase).unwrap(),
    )
    .unwrap();
    let too_wide = TimecodeValue::Timecode(
        description32
            .timecode_from_frames(i64::from(i32::MAX) + 1)
            .unwrap(),
    );
    let error = SourceTimecodeTrack::new(
        StreamId::new(3),
        description32,
        vec![StreamId::new(1)],
        vec![TimecodeSegment::new(range, too_wide)],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let counter64 = TimecodeDescription::new(
        TimecodeSampleEncoding::Timecode64,
        TimecodeFlags::new(TimecodeFlags::COUNTER),
        1_000,
        1,
        100,
        Arc::from(&b""[..]),
    )
    .unwrap();
    let error = SourceTimecodeTrack::new(
        StreamId::new(4),
        counter64,
        Vec::new(),
        vec![TimecodeSegment::new(
            range,
            TimecodeValue::Counter(u64::MAX),
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn malformed_or_inconsistent_timecode_metadata_fails_explicitly() {
    let unsupported_drop_rate = TimecodeDescription::new(
        TimecodeSampleEncoding::Timecode32,
        TimecodeFlags::new(TimecodeFlags::DROP_FRAME),
        25,
        1,
        25,
        Arc::from(&b""[..]),
    )
    .unwrap_err();
    assert_eq!(
        unsupported_drop_rate.category(),
        ErrorCategory::InvalidInput
    );

    let wrong_frame_quanta = TimecodeDescription::new(
        TimecodeSampleEncoding::Timecode32,
        TimecodeFlags::new(0),
        24,
        1,
        25,
        Arc::from(&b""[..]),
    )
    .unwrap_err();
    assert_eq!(wrong_frame_quanta.category(), ErrorCategory::InvalidInput);

    let no_negative = TimecodeDescription::new(
        TimecodeSampleEncoding::Timecode32,
        TimecodeFlags::new(0),
        30,
        1,
        30,
        Arc::from(&b""[..]),
    )
    .unwrap();
    let error = no_negative
        .decode_sample(&(-1_i32).to_be_bytes())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert_eq!(
        no_negative
            .decode_sample(&[0, 0, 0])
            .unwrap_err()
            .category(),
        ErrorCategory::CorruptData
    );

    let timebase = Timebase::integer(30).unwrap();
    let first = TimecodeSegment::new(
        TimeRange::new(
            RationalTime::zero(timebase),
            Duration::new(30, timebase).unwrap(),
        )
        .unwrap(),
        TimecodeValue::Timecode(no_negative.timecode_from_frames(0).unwrap()),
    );
    let overlapping = TimecodeSegment::new(
        TimeRange::new(
            RationalTime::new(29, timebase),
            Duration::new(30, timebase).unwrap(),
        )
        .unwrap(),
        TimecodeValue::Timecode(no_negative.timecode_from_frames(29).unwrap()),
    );
    let error = SourceTimecodeTrack::new(
        StreamId::new(3),
        no_negative,
        vec![StreamId::new(1)],
        vec![first, overlapping],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}
