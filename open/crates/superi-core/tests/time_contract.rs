use std::cmp::Ordering;

use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::time::{
    Duration, FrameRate, RationalTime, SampleTime, TimeRange, TimeRounding, Timebase,
};

#[test]
fn timebases_are_positive_reduced_values() {
    let reduced = Timebase::new(48_000, 2).unwrap();
    assert_eq!(reduced, Timebase::integer(24_000).unwrap());
    assert_eq!(reduced.numerator(), 24_000);
    assert_eq!(reduced.denominator(), 1);
    assert!(reduced.is_integral());

    let fractional = Timebase::new(30_000, 1_001).unwrap();
    assert_eq!(fractional.numerator(), 30_000);
    assert_eq!(fractional.denominator(), 1_001);
    assert!(!fractional.is_integral());
}

#[test]
fn invalid_rates_use_the_shared_actionable_error_contract() {
    for error in [
        Timebase::new(0, 1).unwrap_err(),
        Timebase::new(1, 0).unwrap_err(),
        FrameRate::new(0, 1).unwrap_err(),
        SampleTime::new(0, 0).unwrap_err(),
    ] {
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert_eq!(error.contexts()[0].component(), "superi-core.time");
    }
}

#[test]
fn professional_frame_rates_retain_exact_and_nominal_values() {
    let expected = [
        (FrameRate::FPS_24, 24, 1, 24),
        (FrameRate::FPS_25, 25, 1, 25),
        (FrameRate::FPS_30, 30, 1, 30),
        (FrameRate::FPS_48, 48, 1, 48),
        (FrameRate::FPS_50, 50, 1, 50),
        (FrameRate::FPS_60, 60, 1, 60),
        (FrameRate::FPS_24000_1001, 24_000, 1_001, 24),
        (FrameRate::FPS_30000_1001, 30_000, 1_001, 30),
        (FrameRate::FPS_60000_1001, 60_000, 1_001, 60),
    ];

    for (rate, numerator, denominator, nominal) in expected {
        assert_eq!(rate.numerator(), numerator);
        assert_eq!(rate.denominator(), denominator);
        assert_eq!(rate.nominal_fps(), nominal);
    }
}

#[test]
fn rational_time_equality_and_ordering_compare_physical_time() {
    let second_at_24 = RationalTime::from_frames(24, FrameRate::FPS_24);
    let second_at_25 = RationalTime::from_frames(25, FrameRate::FPS_25);
    let second_at_48k = SampleTime::new(48_000, 48_000).unwrap().rational_time();

    assert_eq!(second_at_24, second_at_25);
    assert_eq!(second_at_24, second_at_48k);
    assert_eq!(
        RationalTime::from_frames(23, FrameRate::FPS_24).partial_cmp(&second_at_25),
        Some(Ordering::Less)
    );
    assert!(
        RationalTime::from_frames(-1, FrameRate::FPS_24) < RationalTime::zero(Timebase::SECONDS)
    );
}

#[test]
fn exact_rescaling_rejects_fractional_target_coordinates() {
    let half_second = RationalTime::new(1, Timebase::integer(2).unwrap());
    let error = half_second
        .checked_rescale(Timebase::SECONDS, TimeRounding::Exact)
        .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(error.message().contains("not exact"));
}

#[test]
fn every_rounding_policy_is_explicit_for_positive_and_negative_time() {
    let positive_half = RationalTime::new(1, Timebase::integer(2).unwrap());
    let negative_half = RationalTime::new(-1, Timebase::integer(2).unwrap());
    let seconds = Timebase::SECONDS;

    assert_eq!(
        positive_half
            .checked_rescale(seconds, TimeRounding::Floor)
            .unwrap()
            .value(),
        0
    );
    assert_eq!(
        positive_half
            .checked_rescale(seconds, TimeRounding::Ceil)
            .unwrap()
            .value(),
        1
    );
    assert_eq!(
        negative_half
            .checked_rescale(seconds, TimeRounding::Floor)
            .unwrap()
            .value(),
        -1
    );
    assert_eq!(
        negative_half
            .checked_rescale(seconds, TimeRounding::Ceil)
            .unwrap()
            .value(),
        0
    );
    assert_eq!(
        negative_half
            .checked_rescale(seconds, TimeRounding::TowardZero)
            .unwrap()
            .value(),
        0
    );
    assert_eq!(
        positive_half
            .checked_rescale(seconds, TimeRounding::NearestTiesEven)
            .unwrap()
            .value(),
        0
    );
    assert_eq!(
        RationalTime::new(3, Timebase::integer(2).unwrap())
            .checked_rescale(seconds, TimeRounding::NearestTiesEven)
            .unwrap()
            .value(),
        2
    );
    assert_eq!(
        RationalTime::new(-3, Timebase::integer(2).unwrap())
            .checked_rescale(seconds, TimeRounding::NearestTiesEven)
            .unwrap()
            .value(),
        -2
    );
}

#[test]
fn frame_conversion_preserves_fractional_broadcast_rate() {
    let frame = RationalTime::from_frames(12_345, FrameRate::FPS_24000_1001);
    assert_eq!(
        frame
            .to_frames(FrameRate::FPS_24000_1001, TimeRounding::Exact)
            .unwrap(),
        12_345
    );

    let one_thousand_and_one_seconds = RationalTime::new(1_001, Timebase::SECONDS);
    assert_eq!(
        one_thousand_and_one_seconds
            .to_frames(FrameRate::FPS_24000_1001, TimeRounding::Exact)
            .unwrap(),
        24_000
    );
}

#[test]
fn checked_arithmetic_requires_a_declared_result_clock() {
    let half = RationalTime::new(1, Timebase::integer(2).unwrap());
    let quarter = RationalTime::new(1, Timebase::integer(4).unwrap());
    let quarters = Timebase::integer(4).unwrap();

    assert_eq!(
        half.checked_add_at(quarter, quarters, TimeRounding::Exact)
            .unwrap(),
        RationalTime::new(3, quarters)
    );
    assert_eq!(
        half.checked_sub_at(quarter, quarters, TimeRounding::Exact)
            .unwrap(),
        RationalTime::new(1, quarters)
    );
    assert_eq!(
        half.checked_add_at(half, Timebase::SECONDS, TimeRounding::NearestTiesEven)
            .unwrap(),
        RationalTime::new(1, Timebase::SECONDS)
    );
}

#[test]
fn checked_arithmetic_reports_coordinate_overflow() {
    let error = RationalTime::new(i64::MAX, Timebase::SECONDS)
        .checked_add_at(
            RationalTime::new(1, Timebase::SECONDS),
            Timebase::SECONDS,
            TimeRounding::Exact,
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert!(error.message().contains("coordinate range"));
}

#[test]
fn sample_positions_convert_without_drift() {
    let one_second = SampleTime::new(48_000, 48_000).unwrap();
    assert_eq!(one_second.sample(), 48_000);
    assert_eq!(one_second.sample_rate(), 48_000);
    assert_eq!(
        one_second.rational_time(),
        RationalTime::new(1, Timebase::SECONDS)
    );

    let resampled = SampleTime::new(44_100, 44_100)
        .unwrap()
        .checked_resample(48_000, TimeRounding::Exact)
        .unwrap();
    assert_eq!(resampled, one_second);
    assert_eq!(SampleTime::new(96_000, 96_000).unwrap(), one_second);
}

#[test]
fn durations_are_nonnegative_and_share_checked_coordinate_bounds() {
    let duration = Duration::from_frames(120, FrameRate::FPS_24).unwrap();
    assert_eq!(duration.value(), 120);
    assert_eq!(duration.timebase(), FrameRate::FPS_24.timebase());
    assert!(!duration.is_zero());
    assert!(Duration::zero(Timebase::SECONDS).is_zero());
    assert_eq!(
        duration,
        Duration::from_frames(240, FrameRate::FPS_48).unwrap()
    );

    let error = Duration::new(i64::MAX as u64 + 1, Timebase::SECONDS).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn time_ranges_are_half_open_and_accept_cross_rate_queries() {
    let range = TimeRange::new(
        RationalTime::from_frames(10, FrameRate::FPS_24),
        Duration::from_frames(5, FrameRate::FPS_24).unwrap(),
    )
    .unwrap();

    assert_eq!(
        range.end_exclusive().unwrap(),
        RationalTime::from_frames(15, FrameRate::FPS_24)
    );
    assert!(range
        .contains(RationalTime::from_frames(10, FrameRate::FPS_24))
        .unwrap());
    assert!(range
        .contains(RationalTime::from_frames(29, FrameRate::FPS_48))
        .unwrap());
    assert!(!range
        .contains(RationalTime::from_frames(30, FrameRate::FPS_48))
        .unwrap());
}

#[test]
fn time_ranges_reject_implicit_timebase_conversion() {
    let error = TimeRange::new(
        RationalTime::from_frames(0, FrameRate::FPS_24),
        Duration::from_frames(10, FrameRate::FPS_25).unwrap(),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert!(error.message().contains("same timebase"));
}

#[test]
fn time_range_rescaling_preserves_exclusive_endpoints() {
    let source = TimeRange::new(
        RationalTime::new(1, Timebase::integer(2).unwrap()),
        Duration::new(2, Timebase::integer(2).unwrap()).unwrap(),
    )
    .unwrap();
    let target = source
        .checked_rescale(Timebase::integer(4).unwrap(), TimeRounding::Exact)
        .unwrap();

    assert_eq!(target.start().value(), 2);
    assert_eq!(target.duration().value(), 4);
    assert_eq!(target.end_exclusive().unwrap().value(), 6);
}

#[test]
fn adjacent_and_overlapping_ranges_are_distinguished() {
    let first = TimeRange::from_start_end(
        RationalTime::new(0, Timebase::SECONDS),
        RationalTime::new(10, Timebase::SECONDS),
    )
    .unwrap();
    let adjacent = TimeRange::from_start_end(
        RationalTime::new(10, Timebase::SECONDS),
        RationalTime::new(20, Timebase::SECONDS),
    )
    .unwrap();
    let overlapping = TimeRange::from_start_end(
        RationalTime::new(9, Timebase::SECONDS),
        RationalTime::new(11, Timebase::SECONDS),
    )
    .unwrap();
    let empty = TimeRange::new(
        RationalTime::new(5, Timebase::SECONDS),
        Duration::zero(Timebase::SECONDS),
    )
    .unwrap();

    assert!(!first.intersects(adjacent).unwrap());
    assert!(first.intersects(overlapping).unwrap());
    assert!(!first.intersects(empty).unwrap());
}

#[test]
fn one_hour_audio_and_video_clocks_are_exactly_aligned() {
    let audio = SampleTime::new(48_000 * 3_600, 48_000)
        .unwrap()
        .rational_time();
    let video = RationalTime::from_frames(24 * 3_600, FrameRate::FPS_24);

    assert_eq!(audio, video);
    assert_eq!(
        audio
            .to_frames(FrameRate::FPS_24, TimeRounding::Exact)
            .unwrap(),
        86_400
    );
}

#[test]
fn shared_time_values_are_copy_send_and_sync() {
    fn assert_copy_send_sync<T: Copy + Send + Sync>() {}

    assert_copy_send_sync::<Timebase>();
    assert_copy_send_sync::<FrameRate>();
    assert_copy_send_sync::<RationalTime>();
    assert_copy_send_sync::<SampleTime>();
    assert_copy_send_sync::<Duration>();
    assert_copy_send_sync::<TimeRange>();
}
