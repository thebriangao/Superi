use superi_core::error::ErrorCategory;
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_media_io::demux::PacketTiming;
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::vfr::{
    PresentationFrame, PresentationSample, VariableFrameRateMap, MAX_PRESENTATION_FRAMES,
};

fn milliseconds() -> Timebase {
    Timebase::MILLISECONDS
}

fn packet(presentation: Option<i64>, decode: Option<i64>, duration: Option<u64>) -> PacketTiming {
    PacketTiming::new(milliseconds(), presentation, decode, duration).unwrap()
}

#[test]
fn decode_order_packet_timing_maps_to_exact_presentation_order() {
    let mapping = VariableFrameRateMap::from_packet_timings([
        packet(Some(80), Some(0), Some(80)),
        packet(Some(0), Some(40), Some(40)),
        packet(Some(40), Some(80), Some(40)),
    ])
    .unwrap();

    assert_eq!(mapping.timebase(), milliseconds());
    assert_eq!(mapping.frame_count(), 3);
    assert!(mapping.is_variable_frame_rate());
    assert_eq!(mapping.presentation_start().value(), 0);
    assert_eq!(mapping.presentation_end().value(), 160);
    assert_eq!(mapping.duration().value(), 160);
    assert_eq!(mapping.presentation_range().duration().value(), 160);

    let first = mapping.frame(0).unwrap();
    assert_eq!(first.presentation_time().value(), 0);
    assert_eq!(first.duration().value(), 40);
    let last = mapping.frame(2).unwrap();
    assert_eq!(last.presentation_time().value(), 80);
    assert_eq!(last.duration().value(), 80);
    assert_eq!(last.end_exclusive().value(), 160);
    assert!(mapping.frame(3).is_none());
}

#[test]
fn presentation_lookup_uses_half_open_ranges_across_timebases() {
    let mapping = VariableFrameRateMap::from_packet_timings([
        packet(Some(0), Some(0), Some(40)),
        packet(Some(40), Some(40), Some(40)),
        packet(Some(80), Some(80), Some(80)),
    ])
    .unwrap();

    assert_eq!(
        mapping.frame_index_at(RationalTime::new(-1, milliseconds())),
        None
    );
    assert_eq!(
        mapping.frame_index_at(RationalTime::new(0, milliseconds())),
        Some(0)
    );
    assert_eq!(
        mapping.frame_index_at(RationalTime::new(39, milliseconds())),
        Some(0)
    );
    assert_eq!(
        mapping.frame_index_at(RationalTime::new(40, milliseconds())),
        Some(1)
    );
    assert_eq!(
        mapping.frame_index_at(RationalTime::new(79, milliseconds())),
        Some(1)
    );
    assert_eq!(
        mapping.frame_index_at(RationalTime::new(80, milliseconds())),
        Some(2)
    );
    assert_eq!(
        mapping.frame_index_at(RationalTime::new(159, milliseconds())),
        Some(2)
    );
    assert_eq!(
        mapping.frame_index_at(RationalTime::new(160, milliseconds())),
        None
    );

    let twenty_five_hz = Timebase::integer(25).unwrap();
    assert_eq!(
        mapping.frame_index_at(RationalTime::new(1, twenty_five_hz)),
        Some(1)
    );
}

#[test]
fn final_presentation_boundary_completes_unknown_packet_durations() {
    let mapping = VariableFrameRateMap::from_packet_timings_with_end(
        [
            packet(Some(100), Some(0), None),
            packet(Some(200), Some(1), None),
            packet(Some(230), Some(2), None),
        ],
        RationalTime::new(300, milliseconds()),
    )
    .unwrap();

    assert_eq!(mapping.frame(0).unwrap().duration().value(), 100);
    assert_eq!(mapping.frame(1).unwrap().duration().value(), 30);
    assert_eq!(mapping.frame(2).unwrap().duration().value(), 70);
    assert_eq!(mapping.presentation_start().value(), 100);
    assert_eq!(mapping.presentation_end().value(), 300);
    assert_eq!(mapping.duration().value(), 200);
    assert!(mapping.is_variable_frame_rate());
}

#[test]
fn malformed_or_ambiguous_timing_is_rejected() {
    assert!(VariableFrameRateMap::from_packet_timings([
        packet(None, Some(0), Some(40)),
        packet(Some(40), Some(40), Some(40)),
    ])
    .is_err());
    assert!(VariableFrameRateMap::from_packet_timings([
        packet(Some(0), Some(0), Some(40)),
        packet(Some(0), Some(40), Some(40)),
    ])
    .is_err());
    assert!(VariableFrameRateMap::from_packet_timings([
        packet(Some(0), Some(0), Some(30)),
        packet(Some(40), Some(40), Some(40)),
    ])
    .is_err());
    assert!(VariableFrameRateMap::from_packet_timings([
        packet(Some(0), Some(0), Some(40)),
        packet(Some(40), Some(40), None),
    ])
    .is_err());
    assert!(VariableFrameRateMap::from_packet_timings_with_end(
        [
            packet(Some(0), Some(0), Some(40)),
            packet(Some(40), Some(40), Some(30)),
        ],
        RationalTime::new(80, milliseconds()),
    )
    .is_err());

    let other_timebase = Timebase::integer(90_000).unwrap();
    let mixed_packet =
        PacketTiming::new(other_timebase, Some(3_600), Some(3_600), Some(3_600)).unwrap();
    assert!(VariableFrameRateMap::from_packet_timings([
        packet(Some(0), Some(0), Some(40)),
        mixed_packet,
    ])
    .is_err());
}

#[test]
fn explicit_frame_maps_require_one_contiguous_positive_timeline() {
    let frame = |start, duration| {
        PresentationFrame::new(
            RationalTime::new(start, milliseconds()),
            Duration::new(duration, milliseconds()).unwrap(),
        )
        .unwrap()
    };

    let constant = VariableFrameRateMap::new(vec![frame(10, 40), frame(50, 40)]).unwrap();
    assert!(!constant.is_variable_frame_rate());
    assert_eq!(constant.duration().value(), 80);

    assert!(VariableFrameRateMap::new(Vec::new()).is_err());
    assert!(VariableFrameRateMap::new(vec![frame(10, 40), frame(60, 40)]).is_err());
    assert!(VariableFrameRateMap::new(vec![frame(10, 50), frame(50, 40)]).is_err());
    assert!(VariableFrameRateMap::new(vec![frame(50, 40), frame(10, 40)]).is_err());
    assert!(PresentationFrame::new(
        RationalTime::new(0, milliseconds()),
        Duration::zero(milliseconds()),
    )
    .is_err());
    assert!(PresentationFrame::new(
        RationalTime::new(0, milliseconds()),
        Duration::new(1, Timebase::SECONDS).unwrap(),
    )
    .is_err());
}

#[test]
fn negative_timestamps_and_coordinate_boundaries_are_checked_exactly() {
    let mapping = VariableFrameRateMap::from_packet_timings([
        packet(Some(-100), Some(0), Some(60)),
        packet(Some(-40), Some(1), Some(40)),
        packet(Some(0), Some(2), Some(20)),
    ])
    .unwrap();
    assert_eq!(mapping.presentation_start().value(), -100);
    assert_eq!(mapping.presentation_end().value(), 20);
    assert_eq!(mapping.duration().value(), 120);

    assert!(VariableFrameRateMap::from_packet_timings_with_end(
        [
            packet(Some(-10), Some(0), None),
            packet(Some(10), Some(1), None)
        ],
        RationalTime::new(5, milliseconds()),
    )
    .is_err());
    assert!(PresentationFrame::new(
        RationalTime::new(i64::MAX, milliseconds()),
        Duration::new(1, milliseconds()).unwrap(),
    )
    .is_err());
    assert!(VariableFrameRateMap::from_packet_timings_with_end(
        [
            packet(Some(i64::MIN), Some(0), None),
            packet(Some(i64::MAX), Some(1), None),
        ],
        RationalTime::new(i64::MAX, milliseconds()),
    )
    .is_err());
}

#[test]
fn gaps_and_overlaps_have_distinct_failures() {
    let frame = |start, duration| {
        PresentationFrame::new(
            RationalTime::new(start, milliseconds()),
            Duration::new(duration, milliseconds()).unwrap(),
        )
        .unwrap()
    };
    let gap = VariableFrameRateMap::new(vec![frame(0, 40), frame(50, 40)]).unwrap_err();
    let overlap = VariableFrameRateMap::new(vec![frame(0, 50), frame(40, 40)]).unwrap_err();
    assert!(gap.message().contains("gap"));
    assert!(overlap.message().contains("overlap"));
}

#[test]
fn construction_is_resource_bounded_and_interruptible() {
    let sample = PresentationSample::new(
        Some(RationalTime::zero(milliseconds())),
        Some(Duration::new(1, milliseconds()).unwrap()),
    );
    let error = VariableFrameRateMap::from_samples(
        std::iter::repeat(sample).take(MAX_PRESENTATION_FRAMES + 1),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);

    let operation = OperationContext::new(MediaPriority::Interactive);
    let cancellation = operation.cancellation_token().clone();
    let samples = (0_i64..4_096).map(|value| {
        if value == 1_024 {
            cancellation.cancel();
        }
        PresentationSample::new(
            Some(RationalTime::new(value, milliseconds())),
            Some(Duration::new(1, milliseconds()).unwrap()),
        )
    });
    let error = VariableFrameRateMap::from_samples_with_operation(samples, &operation).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
}
