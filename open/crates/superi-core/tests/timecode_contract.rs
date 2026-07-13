use superi_core::time::{FrameRate, TimeRounding};
use superi_core::timecode::{
    Timecode, TimecodeComponent, TimecodeError, TimecodeFormat, TimecodeMode,
};

#[test]
fn non_drop_labels_are_strict_and_canonical() {
    let format = TimecodeFormat::non_drop(FrameRate::FPS_24);
    let value = Timecode::parse("01:02:03:04", format).expect("valid non-drop label");

    assert_eq!(value.frames(), 89_356);
    assert_eq!(value.format(), format);
    assert_eq!(value.to_string(), "01:02:03:04");
    assert_eq!(
        Timecode::parse("-00:00:00:01", format)
            .expect("negative label")
            .frames(),
        -1
    );
    assert_eq!(
        Timecode::from_frames(-1, format).to_string(),
        "-00:00:00:01"
    );
    assert_eq!(
        Timecode::parse("27:00:00:00", format)
            .expect("long editorial label")
            .to_string(),
        "27:00:00:00"
    );

    assert!(matches!(
        Timecode::parse("00:00:00;00", format),
        Err(TimecodeError::SeparatorMismatch { .. })
    ));
    assert!(matches!(
        Timecode::parse("00:60:00:00", format),
        Err(TimecodeError::ComponentOutOfRange {
            component: TimecodeComponent::Minutes,
            ..
        })
    ));
    assert!(matches!(
        Timecode::parse("00:00:00:24", format),
        Err(TimecodeError::ComponentOutOfRange {
            component: TimecodeComponent::Frames,
            ..
        })
    ));
    assert!(matches!(
        Timecode::parse("00:00:00", format),
        Err(TimecodeError::InvalidSyntax)
    ));
    assert!(matches!(
        Timecode::parse("00:00:aa:00", format),
        Err(TimecodeError::InvalidSyntax)
    ));
    assert!(matches!(
        Timecode::parse("+00:00:00:00", format),
        Err(TimecodeError::InvalidSyntax)
    ));
    assert!(matches!(
        Timecode::parse("-00:00:00:00", format),
        Err(TimecodeError::NegativeZero)
    ));

    let sub_frame_rate = FrameRate::new(1, 3).expect("one frame every three seconds");
    let sub_frame_format = TimecodeFormat::non_drop(sub_frame_rate);
    assert_eq!(
        Timecode::parse("00:00:00:00", sub_frame_format)
            .expect("positive rational rates remain total")
            .to_string(),
        "00:00:00:00"
    );
}

#[test]
fn drop_frame_skips_labels_without_skipping_frames() {
    let format = TimecodeFormat::drop_frame(FrameRate::FPS_30000_1001).expect("29.97 drop-frame");
    assert_eq!(format.mode(), TimecodeMode::DropFrame);
    assert_eq!(format.drop_frames_per_minute(), 2);

    let before = Timecode::parse("00:00:59;29", format).expect("last label before drop");
    let after = before.checked_add_frames(1).expect("next physical frame");
    assert_eq!(before.frames(), 1_799);
    assert_eq!(after.frames(), 1_800);
    assert_eq!(after.to_string(), "00:01:00;02");

    assert_eq!(
        Timecode::parse("00:10:00;00", format)
            .expect("ten-minute label")
            .frames(),
        17_982
    );
    assert_eq!(
        Timecode::parse("24:00:00;00", format)
            .expect("24-hour editorial position")
            .frames(),
        2_589_408
    );
    assert!(matches!(
        Timecode::parse("00:01:00;00", format),
        Err(TimecodeError::DroppedFrameLabel { .. })
    ));
    assert!(matches!(
        Timecode::parse("00:01:00;01", format),
        Err(TimecodeError::DroppedFrameLabel { .. })
    ));
    assert_eq!(
        Timecode::parse("00:10:00;00", format)
            .expect("tenth minute does not skip")
            .to_string(),
        "00:10:00;00"
    );
}

#[test]
fn high_frame_rate_drop_counting_is_supported_exactly() {
    let format = TimecodeFormat::drop_frame(FrameRate::FPS_60000_1001).expect("59.94 drop-frame");
    assert_eq!(format.drop_frames_per_minute(), 4);

    let before = Timecode::parse("00:00:59;59", format).expect("last label before drop");
    let after = before.checked_add_frames(1).expect("next physical frame");
    assert_eq!(before.frames(), 3_599);
    assert_eq!(after.frames(), 3_600);
    assert_eq!(after.to_string(), "00:01:00;04");

    assert!(matches!(
        Timecode::parse("00:01:00;03", format),
        Err(TimecodeError::DroppedFrameLabel { .. })
    ));
}

#[test]
fn invalid_drop_frame_rates_are_rejected() {
    for rate in [
        FrameRate::FPS_24,
        FrameRate::FPS_30,
        FrameRate::FPS_24000_1001,
    ] {
        assert!(matches!(
            TimecodeFormat::drop_frame(rate),
            Err(TimecodeError::UnsupportedDropFrameRate { .. })
        ));
    }
}

#[test]
fn arithmetic_is_checked_and_format_preserving() {
    let fps24 = TimecodeFormat::non_drop(FrameRate::FPS_24);
    let ten_seconds = Timecode::parse("00:00:10:00", fps24).expect("timecode");
    let eleven_seconds = ten_seconds.checked_add_frames(24).expect("checked add");

    assert_eq!(eleven_seconds.to_string(), "00:00:11:00");
    assert_eq!(
        eleven_seconds
            .checked_sub_frames(24)
            .expect("checked subtract"),
        ten_seconds
    );
    assert_eq!(
        eleven_seconds
            .checked_duration_since(ten_seconds)
            .expect("same format"),
        24
    );

    let fps25 = TimecodeFormat::non_drop(FrameRate::FPS_25);
    assert!(matches!(
        eleven_seconds.checked_duration_since(Timecode::from_frames(0, fps25)),
        Err(TimecodeError::FormatMismatch { .. })
    ));
    assert!(matches!(
        Timecode::from_frames(i64::MAX, fps24).checked_add_frames(1),
        Err(TimecodeError::ArithmeticOverflow)
    ));
    assert!(matches!(
        Timecode::from_frames(i64::MIN, fps24).checked_sub_frames(1),
        Err(TimecodeError::ArithmeticOverflow)
    ));
}

#[test]
fn conversions_use_exact_rational_time_and_explicit_rounding() {
    let fps24 = TimecodeFormat::non_drop(FrameRate::FPS_24);
    let fps48 = TimecodeFormat::non_drop(FrameRate::FPS_48);
    let one_second = Timecode::from_frames(24, fps24);

    let doubled = one_second
        .converted_to(fps48, TimeRounding::Exact)
        .expect("exact conversion");
    assert_eq!(doubled.frames(), 48);
    assert_eq!(doubled.to_string(), "00:00:01:00");
    assert_eq!(
        one_second
            .to_rational_time()
            .to_frames(FrameRate::FPS_48, TimeRounding::Exact)
            .expect("same rational conversion"),
        48
    );

    let one_frame = Timecode::from_frames(1, fps24);
    let fps30 = TimecodeFormat::non_drop(FrameRate::FPS_30);
    assert!(matches!(
        one_frame.converted_to(fps30, TimeRounding::Exact),
        Err(TimecodeError::Time(_))
    ));
    assert_eq!(
        one_frame
            .converted_to(fps30, TimeRounding::Floor)
            .expect("explicit floor")
            .frames(),
        1
    );
    assert_eq!(
        one_frame
            .converted_to(fps30, TimeRounding::Ceil)
            .expect("explicit ceil")
            .frames(),
        2
    );

    let df = TimecodeFormat::drop_frame(FrameRate::FPS_30000_1001).expect("29.97 drop-frame");
    let ndf = TimecodeFormat::non_drop(FrameRate::FPS_30000_1001);
    let source = Timecode::parse("00:01:00;02", df).expect("drop label");
    let converted = source
        .converted_to(ndf, TimeRounding::Exact)
        .expect("same exact frame rate");
    assert_eq!(converted.frames(), source.frames());
    assert_eq!(converted.to_string(), "00:01:00:00");
}

#[test]
fn representative_values_round_trip_through_labels() {
    let formats = [
        TimecodeFormat::non_drop(FrameRate::FPS_24),
        TimecodeFormat::non_drop(FrameRate::FPS_24000_1001),
        TimecodeFormat::drop_frame(FrameRate::FPS_30000_1001).expect("29.97 drop-frame"),
        TimecodeFormat::drop_frame(FrameRate::FPS_60000_1001).expect("59.94 drop-frame"),
    ];
    let frames = [
        -2_589_409,
        -18_000,
        -1,
        0,
        1,
        1_799,
        1_800,
        17_981,
        17_982,
        2_589_408,
        9_000_000,
        i64::MAX,
        i64::MIN,
    ];

    for format in formats {
        for frame in frames {
            let value = Timecode::from_frames(frame, format);
            let label = value.to_string();
            let reparsed = Timecode::parse(&label, format).expect("formatted label must parse");
            assert_eq!(reparsed, value, "{label} at {format:?}");
        }
    }
}
