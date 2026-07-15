use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use superi_concurrency::clock::{
    AudioMasterClock, AvFrameCorrection, AvPresentationReason, PlaybackClockMode,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::ErrorCategory;
use superi_core::time::{Duration as MediaDuration, RationalTime, SampleTime, Timebase};
use superi_engine::av_sync::{
    interactive_av_sync_policy, AvSyncCoordinator, AvSyncFrameReadiness, AvSyncFrameTiming,
    AvSyncOutcome,
};

const SAMPLE_RATE: u32 = 48_000;

fn sample_time(sample: i64) -> RationalTime {
    RationalTime::from_sample_time(SampleTime::new(sample, SAMPLE_RATE).unwrap())
}

fn frame_duration() -> MediaDuration {
    MediaDuration::new(1_920, Timebase::new(SAMPLE_RATE, 1).unwrap()).unwrap()
}

fn timing(sample: i64) -> AvSyncFrameTiming {
    AvSyncFrameTiming::new(sample_time(sample), frame_duration())
}

fn coordinator(initial_sample: i64) -> (Arc<AudioMasterClock>, AvSyncCoordinator) {
    let audio = Arc::new(AudioMasterClock::new(
        SampleTime::new(initial_sample, SAMPLE_RATE).unwrap(),
    ));
    let coordinator =
        AvSyncCoordinator::audio_master(sample_time(initial_sample), audio.clone()).unwrap();
    (audio, coordinator)
}

#[test]
fn interactive_policy_stays_inside_the_product_sync_target() {
    let policy = interactive_av_sync_policy().unwrap();
    assert_eq!(policy.presentation_tolerance(), StdDuration::from_millis(8));
    assert_eq!(policy.drop_lateness(), StdDuration::from_millis(40));
    assert_eq!(policy.discontinuity_threshold(), StdDuration::from_secs(10));
    assert_eq!(policy.maximum_correction(), StdDuration::from_millis(20));
    assert_eq!(policy.max_consecutive_drops(), 4);
}

#[test]
fn coordination_is_nonblocking_and_owned_only_by_the_playback_domain() {
    let (_, mut coordinator) = coordinator(48_000);
    let error = coordinator
        .coordinate_at(
            timing(48_000),
            AvSyncFrameReadiness::last_ready(),
            Instant::now(),
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(coordinator.statistics().observations(), 0);

    let ui = ExecutionDomain::Ui.enter_current().unwrap();
    let error = coordinator
        .coordinate_at(
            timing(48_000),
            AvSyncFrameReadiness::last_ready(),
            Instant::now(),
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(coordinator.statistics().observations(), 0);
    drop(ui);
}

#[test]
fn normal_and_stressed_frames_return_explicit_bounded_actions() {
    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let (audio, mut coordinator) = coordinator(48_000);
    let now = Instant::now();

    let early_timing = timing(48_960);
    let held = coordinator
        .coordinate_at(early_timing, AvSyncFrameReadiness::last_ready(), now)
        .unwrap();
    assert_eq!(held.timing(), early_timing);
    assert_eq!(held.master_time(), sample_time(48_000));
    assert_eq!(held.drift().nanoseconds(), 20_000_000);
    assert_eq!(
        held,
        AvSyncOutcome::Hold {
            timing: early_timing,
            master_time: sample_time(48_000),
            drift: held.drift(),
            wait_for: StdDuration::from_millis(12),
        }
    );

    let on_time_timing = timing(48_000);
    let on_time = coordinator
        .coordinate_at(on_time_timing, AvSyncFrameReadiness::last_ready(), now)
        .unwrap();
    assert_eq!(
        on_time,
        AvSyncOutcome::Present {
            timing: on_time_timing,
            master_time: sample_time(48_000),
            drift: on_time.drift(),
            display_for: StdDuration::from_millis(40),
            correction: AvFrameCorrection::None,
            reason: AvPresentationReason::InTolerance,
            recovery: None,
        }
    );

    audio.publish_sample(49_440).unwrap();
    let late_timing = timing(48_000);
    let corrected = coordinator
        .coordinate_at(late_timing, AvSyncFrameReadiness::last_ready(), now)
        .unwrap();
    assert_eq!(corrected.timing(), late_timing);
    assert_eq!(corrected.drift().nanoseconds(), -30_000_000);
    assert_eq!(
        corrected,
        AvSyncOutcome::Present {
            timing: late_timing,
            master_time: sample_time(49_440),
            drift: corrected.drift(),
            display_for: StdDuration::from_millis(20),
            correction: AvFrameCorrection::ShortenBy(StdDuration::from_millis(20)),
            reason: AvPresentationReason::CorrectedLate,
            recovery: None,
        }
    );

    audio.publish_sample(51_840).unwrap();
    let protected = coordinator
        .coordinate_at(timing(48_000), AvSyncFrameReadiness::last_ready(), now)
        .unwrap();
    assert!(matches!(
        protected,
        AvSyncOutcome::Present {
            reason: AvPresentationReason::ProtectedFromDrop,
            ..
        }
    ));

    let dropped = coordinator
        .coordinate_at(timing(49_920), AvSyncFrameReadiness::new(true, true), now)
        .unwrap();
    assert!(matches!(dropped, AvSyncOutcome::Drop { .. }));
    assert_eq!(dropped.timing(), timing(49_920));
    assert_eq!(coordinator.statistics().dropped(), 1);
    assert_eq!(coordinator.statistics().corrected(), 2);
    drop(playback);
}

#[test]
fn discontinuity_rebases_once_then_presents_without_rewriting_frame_time() {
    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let (_, mut coordinator) = coordinator(0);
    let now = Instant::now();
    let exact_timing = timing(480_000);

    let recovered = coordinator
        .coordinate_at(exact_timing, AvSyncFrameReadiness::last_ready(), now)
        .unwrap();
    let AvSyncOutcome::Present {
        timing,
        master_time,
        drift,
        display_for,
        correction,
        reason,
        recovery: Some(recovery),
    } = recovered
    else {
        panic!("a discontinuity must rebase and present in one nonblocking step");
    };
    assert_eq!(timing, exact_timing);
    assert_eq!(master_time, sample_time(480_000));
    assert_eq!(drift.nanoseconds(), 0);
    assert_eq!(display_for, StdDuration::from_millis(40));
    assert_eq!(correction, AvFrameCorrection::None);
    assert_eq!(reason, AvPresentationReason::InTolerance);
    assert_eq!(recovery.previous_master_time(), sample_time(0));
    assert_eq!(recovery.target_master_time(), sample_time(480_000));
    assert_eq!(recovery.discontinuity().nanoseconds(), 10_000_000_000);
    assert_eq!(
        coordinator.clock_position_at(now).unwrap(),
        sample_time(480_000)
    );
    assert_eq!(coordinator.statistics().rebases(), 1);
    assert_eq!(coordinator.statistics().presented(), 1);
    assert_eq!(coordinator.statistics().observations(), 2);
    drop(playback);
}

#[test]
fn clock_fallback_and_audio_recovery_preserve_one_continuous_timeline() {
    let playback = ExecutionDomain::Playback.enter_current().unwrap();
    let (audio, mut coordinator) = coordinator(0);
    let start = Instant::now();
    audio.publish_sample(480).unwrap();
    assert_eq!(
        coordinator.clock_position_at(start).unwrap(),
        sample_time(480)
    );

    assert_eq!(
        coordinator.fallback_to_playback_at(start).unwrap(),
        sample_time(480)
    );
    assert_eq!(coordinator.clock_mode(), PlaybackClockMode::Playback);
    let later = start + StdDuration::from_millis(25);
    assert_eq!(
        coordinator.clock_position_at(later).unwrap(),
        sample_time(1_680)
    );

    assert_eq!(
        coordinator
            .recover_audio_master_at(audio.clone(), later)
            .unwrap(),
        sample_time(1_680)
    );
    assert_eq!(coordinator.clock_mode(), PlaybackClockMode::AudioMaster);
    audio.publish_sample(960).unwrap();
    assert_eq!(
        coordinator.clock_position_at(later).unwrap(),
        sample_time(2_160)
    );
    drop(playback);
}
