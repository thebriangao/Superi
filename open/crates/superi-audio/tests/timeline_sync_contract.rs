use std::sync::Arc;

use superi_audio::sync::{
    AudioScheduleEpoch, AudioTimelinePlacement, AudioTimelineSchedule, AudioTimelineScheduler,
};
use superi_concurrency::clock::{measure_av_drift, AudioMasterClock, PlaybackClock};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, TimelineId, TrackId};
use superi_core::time::{SampleTime, Timebase};

fn sample(value: i64) -> SampleTime {
    SampleTime::new(value, 48_000).unwrap()
}

fn placement(
    track: u128,
    clip: u128,
    track_order: u32,
    record_start: i64,
    source_start: i64,
    frame_count: u64,
) -> AudioTimelinePlacement {
    AudioTimelinePlacement::new(
        TrackId::from_raw(track),
        ClipId::from_raw(clip),
        track_order,
        sample(record_start),
        sample(source_start),
        frame_count,
    )
    .unwrap()
}

fn schedule() -> AudioTimelineSchedule {
    AudioTimelineSchedule::new(
        TimelineId::from_raw(1),
        7,
        48_000,
        [
            placement(20, 200, 1, 125, 2_000, 100),
            placement(10, 101, 0, 250, 3_000, 50),
            placement(10, 100, 0, 100, 1_000, 100),
        ],
    )
    .unwrap()
}

#[test]
fn schedule_construction_preserves_identity_and_canonical_track_order() {
    let schedule = schedule();
    assert_eq!(schedule.timeline_id(), TimelineId::from_raw(1));
    assert_eq!(schedule.timeline_revision(), 7);
    assert_eq!(schedule.sample_rate(), 48_000);
    assert_eq!(schedule.placements().len(), 3);
    assert_eq!(schedule.placements()[0].track_id(), TrackId::from_raw(10));
    assert_eq!(schedule.placements()[0].clip_id(), ClipId::from_raw(100));
    assert_eq!(schedule.placements()[1].clip_id(), ClipId::from_raw(101));
    assert_eq!(schedule.placements()[2].track_id(), TrackId::from_raw(20));
    assert_eq!(schedule.placements()[2].track_order(), 1);
}

#[test]
fn callback_planning_is_audio_owned_exact_and_allocation_free_by_construction() {
    let mut scheduler = AudioTimelineScheduler::new(
        schedule(),
        AudioScheduleEpoch::new(sample(10_000), sample(150)).unwrap(),
    )
    .unwrap();

    let error = scheduler.plan_callback(sample(10_000), 120).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);

    let audio = ExecutionDomain::Audio.enter_current().unwrap();
    let plan = scheduler.plan_callback(sample(10_000), 120).unwrap();
    assert_eq!(plan.device_start(), sample(10_000));
    assert_eq!(plan.timeline_start(), sample(150));
    assert_eq!(plan.frame_count(), 120);
    assert_eq!(plan.device_end(), sample(10_120));
    assert_eq!(plan.timeline_end(), sample(270));
    let slices = plan
        .slices()
        .map(|slice| {
            (
                slice.track_id(),
                slice.clip_id(),
                slice.track_order(),
                slice.output_offset(),
                slice.source_start(),
                slice.frame_count(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        slices,
        vec![
            (
                TrackId::from_raw(10),
                ClipId::from_raw(100),
                0,
                0,
                sample(1_050),
                50,
            ),
            (
                TrackId::from_raw(10),
                ClipId::from_raw(101),
                0,
                100,
                sample(3_000),
                20,
            ),
            (
                TrackId::from_raw(20),
                ClipId::from_raw(200),
                1,
                0,
                sample(2_025),
                75,
            ),
        ]
    );
    drop(audio);

    scheduler
        .reanchor(AudioScheduleEpoch::new(sample(20_000), sample(1_000)).unwrap())
        .unwrap();
    let audio = ExecutionDomain::Audio.enter_current().unwrap();
    let seek_plan = scheduler.plan_callback(sample(20_240), 10).unwrap();
    assert_eq!(seek_plan.timeline_start(), sample(1_240));
    assert_eq!(seek_plan.slices().count(), 0);
    drop(audio);
}

#[test]
fn presented_audio_advances_the_existing_master_clock_and_aligns_video_exactly() {
    let source = Arc::new(AudioMasterClock::new(sample(10_000)));
    let playback =
        PlaybackClock::audio_master(sample(48_000).rational_time(), Arc::clone(&source)).unwrap();
    let scheduler = AudioTimelineScheduler::new(
        schedule(),
        AudioScheduleEpoch::new(sample(10_000), sample(48_000)).unwrap(),
    )
    .unwrap();

    let audio = ExecutionDomain::Audio.enter_current().unwrap();
    let plan = scheduler.plan_callback(sample(10_000), 480).unwrap();
    assert_eq!(plan.publish_presented(&source).unwrap().code(), "advanced");
    drop(audio);

    let master_time = playback.position().unwrap();
    assert_eq!(master_time, sample(48_480).rational_time());
    assert_eq!(
        measure_av_drift(sample(48_480).rational_time(), master_time)
            .unwrap()
            .nanoseconds(),
        0
    );

    let one_hour_device = sample(10_000 + 48_000 * 3_600);
    let audio = ExecutionDomain::Audio.enter_current().unwrap();
    let one_hour = scheduler.plan_callback(one_hour_device, 480).unwrap();
    assert_eq!(one_hour.timeline_start(), sample(48_000 + 48_000 * 3_600));
    one_hour.publish_presented(&source).unwrap();
    drop(audio);
    assert_eq!(
        playback.position().unwrap(),
        sample(48_480 + 48_000 * 3_600).rational_time()
    );
}

#[test]
fn invalid_placements_fail_before_publishing_a_schedule() {
    let overlap = AudioTimelineSchedule::new(
        TimelineId::from_raw(1),
        0,
        48_000,
        [
            placement(10, 1, 0, 0, 0, 100),
            placement(10, 2, 0, 99, 1_000, 100),
        ],
    )
    .unwrap_err();
    assert_eq!(overlap.category(), ErrorCategory::Conflict);

    let duplicate_order = AudioTimelineSchedule::new(
        TimelineId::from_raw(1),
        0,
        48_000,
        [placement(10, 1, 0, 0, 0, 10), placement(20, 2, 0, 0, 0, 10)],
    )
    .unwrap_err();
    assert_eq!(duplicate_order.category(), ErrorCategory::Conflict);

    let negative = AudioTimelinePlacement::new(
        TrackId::from_raw(1),
        ClipId::from_raw(1),
        0,
        sample(-1),
        sample(0),
        1,
    )
    .unwrap_err();
    assert_eq!(negative.category(), ErrorCategory::InvalidInput);

    let zero = AudioTimelinePlacement::new(
        TrackId::from_raw(1),
        ClipId::from_raw(1),
        0,
        sample(0),
        sample(0),
        0,
    )
    .unwrap_err();
    assert_eq!(zero.category(), ErrorCategory::InvalidInput);

    let overflow = AudioTimelinePlacement::new(
        TrackId::from_raw(1),
        ClipId::from_raw(1),
        0,
        sample(i64::MAX),
        sample(0),
        1,
    )
    .unwrap_err();
    assert_eq!(overflow.category(), ErrorCategory::InvalidInput);
}

#[test]
fn epochs_and_callback_windows_reject_clock_mismatches_and_overflow() {
    let wrong_rate = SampleTime::new(0, 44_100).unwrap();
    let error = AudioScheduleEpoch::new(sample(0), wrong_rate).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let scheduler = AudioTimelineScheduler::new(
        schedule(),
        AudioScheduleEpoch::new(sample(0), sample(0)).unwrap(),
    )
    .unwrap();
    let audio = ExecutionDomain::Audio.enter_current().unwrap();
    let error = scheduler.plan_callback(wrong_rate, 1).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    let error = scheduler.plan_callback(sample(-1), 1).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    let error = scheduler.plan_callback(sample(i64::MAX), 1).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    drop(audio);

    assert_eq!(
        Timebase::integer(scheduler.sample_rate()).unwrap(),
        sample(0).timebase()
    );
}
