use superi_cache::prefetch::{
    PlaybackPrefetchConfig, PlaybackPrefetchDirection, PlaybackPrefetchInput,
    PlaybackPrefetchUrgency, MAX_PREFETCH_FRAMES,
};
use superi_core::time::{Duration, RationalTime, TimeRange, Timebase};

fn range(start: i64, end: i64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(u64::try_from(end - start).unwrap(), timebase).unwrap(),
    )
    .unwrap()
}

#[test]
fn forward_prediction_orders_nearest_playback_frames_before_extended_and_trailing_work() {
    let frames = Timebase::integer(24).unwrap();
    let config = PlaybackPrefetchConfig::new(2, 2, 1).unwrap();
    let input =
        PlaybackPrefetchInput::new(RationalTime::new(4, frames), range(0, 10, frames), 1).unwrap();

    let plan = config.plan(input);

    assert_eq!(plan.direction(), PlaybackPrefetchDirection::Forward);
    assert_eq!(plan.playhead(), RationalTime::new(4, frames));
    assert_eq!(
        plan.requests()
            .iter()
            .map(|request| request.frame().value())
            .collect::<Vec<_>>(),
        [5, 6, 7, 8, 3]
    );
    assert_eq!(
        plan.requests()
            .iter()
            .map(|request| request.offset_steps())
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, -1]
    );
    assert_eq!(
        plan.requests()
            .iter()
            .map(|request| request.urgency())
            .collect::<Vec<_>>(),
        [
            PlaybackPrefetchUrgency::PlaybackCritical,
            PlaybackPrefetchUrgency::PlaybackCritical,
            PlaybackPrefetchUrgency::Predictive,
            PlaybackPrefetchUrgency::Predictive,
            PlaybackPrefetchUrgency::Predictive,
        ]
    );
}

#[test]
fn reverse_and_faster_transport_predict_exact_signed_steps_and_clip_half_open_bounds() {
    let frames = Timebase::integer(30).unwrap();
    let config = PlaybackPrefetchConfig::new(2, 2, 1).unwrap();
    let input =
        PlaybackPrefetchInput::new(RationalTime::new(5, frames), range(0, 10, frames), -2).unwrap();

    let plan = config.plan(input);

    assert_eq!(plan.direction(), PlaybackPrefetchDirection::Reverse);
    assert_eq!(
        plan.requests()
            .iter()
            .map(|request| request.frame().value())
            .collect::<Vec<_>>(),
        [3, 1, 7]
    );
    assert_eq!(
        plan.requests()
            .iter()
            .map(|request| request.offset_steps())
            .collect::<Vec<_>>(),
        [-1, -2, 1]
    );
    assert!(plan
        .requests()
        .iter()
        .all(|request| request.frame() >= RationalTime::new(0, frames)
            && request.frame() < RationalTime::new(10, frames)));
}

#[test]
fn a_boundary_plan_can_be_empty_without_repeating_the_current_frame() {
    let frames = Timebase::integer(24).unwrap();
    let config = PlaybackPrefetchConfig::new(1, 0, 0).unwrap();
    let input =
        PlaybackPrefetchInput::new(RationalTime::new(0, frames), range(0, 1, frames), 1).unwrap();

    let plan = config.plan(input);

    assert!(plan.is_empty());
    assert_eq!(plan.len(), 0);
}

#[test]
fn prediction_configuration_and_observation_reject_ambiguous_or_unbounded_inputs() {
    let frames = Timebase::integer(24).unwrap();
    let milliseconds = Timebase::MILLISECONDS;

    assert!(PlaybackPrefetchConfig::new(0, 1, 0).is_err());
    assert!(PlaybackPrefetchConfig::new(MAX_PREFETCH_FRAMES, 1, 0).is_err());
    assert!(
        PlaybackPrefetchInput::new(RationalTime::new(2, frames), range(0, 10, frames), 0,).is_err()
    );
    assert!(PlaybackPrefetchInput::new(
        RationalTime::new(2, milliseconds),
        range(0, 10, frames),
        1,
    )
    .is_err());
    assert!(
        PlaybackPrefetchInput::new(RationalTime::new(10, frames), range(0, 10, frames), 1,)
            .is_err()
    );
}

#[test]
fn equal_inputs_produce_one_deterministic_prediction_order() {
    let frames = Timebase::integer(60).unwrap();
    let config = PlaybackPrefetchConfig::new(4, 3, 2).unwrap();
    let input = PlaybackPrefetchInput::new(RationalTime::new(20, frames), range(0, 100, frames), 3)
        .unwrap();

    assert_eq!(config.plan(input), config.plan(input));
}
