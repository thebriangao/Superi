use std::time::Duration;

use superi_concurrency::clock::{
    measure_av_drift, AvDriftDirection, AvFrameAction, AvFrameCorrection, AvFrameObservation,
    AvPresentationReason, AvSyncPolicy, AvSyncScheduler,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::ErrorCategory;
use superi_core::time::{Duration as MediaDuration, FrameRate, RationalTime, SampleTime, Timebase};

fn milliseconds(value: i64) -> RationalTime {
    RationalTime::new(value, Timebase::MILLISECONDS)
}

fn observation(video_ms: i64, master_ms: i64) -> AvFrameObservation {
    AvFrameObservation::new(
        milliseconds(video_ms),
        milliseconds(master_ms),
        MediaDuration::new(40, Timebase::MILLISECONDS).unwrap(),
    )
}

#[test]
fn drift_measurement_is_signed_exact_and_crosses_frame_and_sample_timebases() {
    let video = RationalTime::from_frames(24, FrameRate::FPS_24);
    let audio = RationalTime::from_sample_time(SampleTime::new(47_520, 48_000).unwrap());
    let ahead = measure_av_drift(video, audio).unwrap();
    assert_eq!(ahead.nanoseconds(), 10_000_000);
    assert_eq!(ahead.direction(), AvDriftDirection::VideoAhead);
    assert_eq!(ahead.absolute_duration(), Duration::from_millis(10));

    let behind = measure_av_drift(audio, video).unwrap();
    assert_eq!(behind.nanoseconds(), -10_000_000);
    assert_eq!(behind.direction(), AvDriftDirection::VideoBehind);
    assert_eq!(behind.absolute_duration(), Duration::from_millis(10));

    let aligned = measure_av_drift(video, video).unwrap();
    assert_eq!(aligned.direction(), AvDriftDirection::Aligned);
}

#[test]
fn scheduling_requires_the_playback_owner_and_never_claims_ui_or_audio() {
    let mut scheduler = AvSyncScheduler::new(AvSyncPolicy::default()).unwrap();
    let error = scheduler.schedule(observation(1_000, 1_000)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.contexts()[0].field("expected"), Some("playback"));
    assert_eq!(scheduler.statistics().observations(), 0);

    let ui = ExecutionDomain::Ui.enter_current().unwrap();
    let error = scheduler.schedule(observation(1_000, 1_000)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.contexts()[0].field("actual"), Some("ui"));
    drop(ui);
}

#[test]
fn early_frames_return_a_bounded_wait_instead_of_blocking_the_playback_thread() {
    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let mut scheduler = AvSyncScheduler::new(AvSyncPolicy::default()).unwrap();

    let decision = scheduler.schedule(observation(1_000, 950)).unwrap();
    assert_eq!(decision.drift().nanoseconds(), 50_000_000);
    assert_eq!(
        decision.action(),
        AvFrameAction::Wait {
            duration: Duration::from_millis(30),
        }
    );
    assert_eq!(scheduler.statistics().waited(), 1);
    assert_eq!(scheduler.statistics().presented(), 0);
    drop(playback);
}

#[test]
fn frames_inside_tolerance_present_at_nominal_duration_without_correction() {
    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let mut scheduler = AvSyncScheduler::new(AvSyncPolicy::default()).unwrap();

    let decision = scheduler.schedule(observation(1_000, 1_010)).unwrap();
    assert_eq!(
        decision.action(),
        AvFrameAction::Present {
            display_for: Duration::from_millis(40),
            correction: AvFrameCorrection::None,
            reason: AvPresentationReason::InTolerance,
        }
    );
    assert_eq!(scheduler.statistics().presented(), 1);
    drop(playback);
}

#[test]
fn moderately_late_video_shortens_its_next_interval_without_touching_audio() {
    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let mut scheduler = AvSyncScheduler::new(AvSyncPolicy::default()).unwrap();

    let decision = scheduler.schedule(observation(1_000, 1_030)).unwrap();
    assert_eq!(
        decision.action(),
        AvFrameAction::Present {
            display_for: Duration::from_millis(30),
            correction: AvFrameCorrection::ShortenBy(Duration::from_millis(10)),
            reason: AvPresentationReason::CorrectedLate,
        }
    );
    assert_eq!(scheduler.statistics().corrected(), 1);
    drop(playback);
}

#[test]
fn late_video_drops_only_when_a_successor_is_ready_and_the_frame_is_eligible() {
    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let mut scheduler = AvSyncScheduler::new(AvSyncPolicy::default()).unwrap();

    let dropped = scheduler
        .schedule(
            observation(1_000, 1_050)
                .with_following_frame_ready(true)
                .with_drop_eligible(true),
        )
        .unwrap();
    assert_eq!(dropped.action(), AvFrameAction::DropLate);

    let protected = scheduler
        .schedule(
            observation(1_040, 1_090)
                .with_following_frame_ready(true)
                .with_drop_eligible(false),
        )
        .unwrap();
    assert_eq!(
        protected.action(),
        AvFrameAction::Present {
            display_for: Duration::from_millis(20),
            correction: AvFrameCorrection::ShortenBy(Duration::from_millis(20)),
            reason: AvPresentationReason::ProtectedFromDrop,
        }
    );

    let last_ready = scheduler
        .schedule(
            observation(1_080, 1_130)
                .with_following_frame_ready(false)
                .with_drop_eligible(true),
        )
        .unwrap();
    assert_eq!(
        last_ready.action(),
        AvFrameAction::Present {
            display_for: Duration::from_millis(20),
            correction: AvFrameCorrection::ShortenBy(Duration::from_millis(20)),
            reason: AvPresentationReason::NoReadySuccessor,
        }
    );

    let statistics = scheduler.statistics();
    assert_eq!(statistics.dropped(), 1);
    assert_eq!(statistics.presented(), 2);
    assert_eq!(statistics.protected_presentations(), 2);
    drop(playback);
}

#[test]
fn consecutive_drop_limit_forces_visible_progress_and_resets_the_streak() {
    let policy = AvSyncPolicy::new(
        Duration::from_millis(20),
        Duration::from_millis(40),
        Duration::from_secs(10),
        Duration::from_millis(20),
        2,
    )
    .unwrap();
    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let mut scheduler = AvSyncScheduler::new(policy).unwrap();
    let late = |video, master| {
        observation(video, master)
            .with_following_frame_ready(true)
            .with_drop_eligible(true)
    };

    assert_eq!(
        scheduler.schedule(late(1_000, 1_080)).unwrap().action(),
        AvFrameAction::DropLate
    );
    assert_eq!(
        scheduler.schedule(late(1_040, 1_120)).unwrap().action(),
        AvFrameAction::DropLate
    );
    let forced = scheduler.schedule(late(1_080, 1_160)).unwrap();
    assert_eq!(
        forced.action(),
        AvFrameAction::Present {
            display_for: Duration::from_millis(20),
            correction: AvFrameCorrection::ShortenBy(Duration::from_millis(20)),
            reason: AvPresentationReason::StarvationGuard,
        }
    );
    assert_eq!(scheduler.statistics().consecutive_drops(), 0);
    assert_eq!(scheduler.statistics().starvation_presentations(), 1);
    drop(playback);
}

#[test]
fn discontinuity_requests_one_explicit_rebase_instead_of_a_drop_storm() {
    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let mut scheduler = AvSyncScheduler::new(AvSyncPolicy::default()).unwrap();

    let decision = scheduler
        .schedule(
            observation(1_000, 12_000)
                .with_following_frame_ready(true)
                .with_drop_eligible(true),
        )
        .unwrap();
    assert_eq!(
        decision.action(),
        AvFrameAction::Rebase {
            target_master_time: milliseconds(1_000),
        }
    );
    assert_eq!(scheduler.statistics().rebases(), 1);
    assert_eq!(scheduler.statistics().dropped(), 0);
    assert_eq!(scheduler.statistics().consecutive_drops(), 0);
    drop(playback);
}

#[test]
fn policy_and_observation_validation_fail_before_mutating_scheduler_state() {
    let invalid = AvSyncPolicy::new(
        Duration::from_millis(40),
        Duration::from_millis(20),
        Duration::from_secs(10),
        Duration::from_millis(20),
        2,
    )
    .unwrap_err();
    assert_eq!(invalid.category(), ErrorCategory::InvalidInput);

    let out_of_range = AvSyncPolicy::new(
        Duration::from_nanos(i64::MAX as u64 + 1),
        Duration::from_nanos(i64::MAX as u64 + 2),
        Duration::from_nanos(i64::MAX as u64 + 3),
        Duration::from_millis(20),
        2,
    )
    .unwrap_err();
    assert_eq!(out_of_range.category(), ErrorCategory::InvalidInput);

    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let mut scheduler = AvSyncScheduler::new(AvSyncPolicy::default()).unwrap();
    let invalid_frame = AvFrameObservation::new(
        milliseconds(0),
        milliseconds(0),
        MediaDuration::zero(Timebase::MILLISECONDS),
    );
    let error = scheduler.schedule(invalid_frame).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(scheduler.statistics().observations(), 0);
    drop(playback);
}

#[test]
fn statistics_are_deterministic_resettable_and_safe_to_transfer() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AvSyncScheduler>();

    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let mut scheduler = AvSyncScheduler::new(AvSyncPolicy::default()).unwrap();
    scheduler.schedule(observation(1_000, 950)).unwrap();
    scheduler.schedule(observation(1_000, 1_010)).unwrap();
    scheduler
        .schedule(
            observation(1_040, 1_090)
                .with_following_frame_ready(true)
                .with_drop_eligible(true),
        )
        .unwrap();

    let statistics = scheduler.statistics();
    assert_eq!(statistics.observations(), 3);
    assert_eq!(
        statistics.maximum_absolute_drift(),
        Duration::from_millis(50)
    );
    assert_eq!(statistics.last_drift().unwrap().nanoseconds(), -50_000_000);

    scheduler.reset();
    assert_eq!(scheduler.statistics().observations(), 0);
    assert_eq!(scheduler.statistics().last_drift(), None);
    drop(playback);
}
