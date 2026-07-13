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
        TimecodeValue::Timecode(Timecode::from_frames(1_799, description.timecode_format())),
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
    let encoded = u32::MAX.to_be_bytes();
    let value = description.decode_sample(&encoded).unwrap();
    assert_eq!(value, TimecodeValue::Counter(u64::from(u32::MAX)));
    assert_eq!(description.encode_sample(value).unwrap(), encoded);

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
    let format = no_negative.timecode_format();
    let first = TimecodeSegment::new(
        TimeRange::new(
            RationalTime::zero(timebase),
            Duration::new(30, timebase).unwrap(),
        )
        .unwrap(),
        TimecodeValue::Timecode(Timecode::from_frames(0, format)),
    );
    let overlapping = TimecodeSegment::new(
        TimeRange::new(
            RationalTime::new(29, timebase),
            Duration::new(30, timebase).unwrap(),
        )
        .unwrap(),
        TimecodeValue::Timecode(Timecode::from_frames(29, format)),
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
