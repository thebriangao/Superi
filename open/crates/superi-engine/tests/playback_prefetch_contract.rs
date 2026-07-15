use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use superi_cache::prefetch::{PlaybackPrefetchConfig, PlaybackPrefetchInput};
use superi_concurrency::jobs::{BoundedWorkerPool, JobTerminalState, WorkerPoolConfig};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::ids::JobId;
use superi_core::time::{Duration, RationalTime, TimeRange, Timebase};
use superi_engine::playback::{PlaybackPrefetchEvaluator, PlaybackPrefetcher};

fn plan(
    playhead: i64,
    critical: usize,
    predictive: usize,
) -> superi_cache::prefetch::PlaybackPrefetchPlan {
    let timebase = Timebase::integer(24).unwrap();
    PlaybackPrefetchConfig::new(critical, predictive, 0)
        .unwrap()
        .plan(
            PlaybackPrefetchInput::new(
                RationalTime::new(playhead, timebase),
                TimeRange::new(
                    RationalTime::new(0, timebase),
                    Duration::new(20, timebase).unwrap(),
                )
                .unwrap(),
                1,
            )
            .unwrap(),
        )
}

#[derive(Default)]
struct RecordingEvaluator {
    frames: Mutex<Vec<RationalTime>>,
    started: AtomicBool,
    hold_first: AtomicBool,
    fail: AtomicBool,
}

impl PlaybackPrefetchEvaluator for RecordingEvaluator {
    fn prefetch(&self, frame: RationalTime) -> Result<()> {
        self.frames.lock().unwrap().push(frame);
        self.started.store(true, Ordering::Release);
        while self.hold_first.load(Ordering::Acquire) {
            thread::yield_now();
        }
        if self.fail.load(Ordering::Acquire) {
            return Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "test prefetch evaluation is unavailable",
            ));
        }
        Ok(())
    }
}

fn poll_until(
    prefetcher: &mut PlaybackPrefetcher,
    expected: usize,
) -> Vec<superi_engine::playback::PlaybackPrefetchCompletion> {
    let mut completed = Vec::new();
    for _ in 0..50_000 {
        completed.extend(prefetcher.poll().unwrap());
        if completed.len() == expected {
            return completed;
        }
        thread::yield_now();
    }
    panic!("playback prefetch jobs did not complete within the bounded poll loop");
}

#[test]
fn playback_submission_is_domain_owned_immediate_and_nonblocking() {
    let pool = Arc::new(BoundedWorkerPool::new(WorkerPoolConfig::new(1, 8).unwrap()).unwrap());
    let evaluator = Arc::new(RecordingEvaluator::default());
    let mut prefetcher = PlaybackPrefetcher::new(&pool, evaluator.clone());

    assert!(prefetcher
        .submit(JobId::from_raw(1), plan(2, 2, 1))
        .is_err());
    assert!(prefetcher.poll().is_err());

    let _playback = ExecutionDomain::Playback.enter_current().unwrap();
    let submitted = prefetcher
        .submit(JobId::from_raw(1), plan(2, 2, 1))
        .unwrap();
    assert_eq!(submitted.generation(), 1);
    assert_eq!(submitted.planned_frames(), 3);
    assert!(submitted.is_queued());
    let initially_ready = prefetcher.poll().unwrap();
    assert!(initially_ready.len() <= 1);
    let completed = if initially_ready.is_empty() {
        poll_until(&mut prefetcher, 1)
    } else {
        initially_ready
    };
    assert_eq!(completed[0].status(), JobTerminalState::Succeeded);
    assert_eq!(completed[0].completed_frames(), 3);
    assert_eq!(completed[0].planned_frames(), 3);
    assert!(completed[0].error().is_none());
    assert_eq!(
        evaluator
            .frames
            .lock()
            .unwrap()
            .iter()
            .map(|frame| frame.value())
            .collect::<Vec<_>>(),
        vec![3, 4, 5]
    );

    drop(prefetcher);
    drop(_playback);
    Arc::try_unwrap(pool).ok().unwrap().shutdown().unwrap();
}

#[test]
fn newer_prediction_cooperatively_supersedes_older_work_between_frames() {
    let pool = Arc::new(BoundedWorkerPool::new(WorkerPoolConfig::new(1, 8).unwrap()).unwrap());
    let evaluator = Arc::new(RecordingEvaluator::default());
    evaluator.hold_first.store(true, Ordering::Release);
    let mut prefetcher = PlaybackPrefetcher::new(&pool, evaluator.clone());
    let _playback = ExecutionDomain::Playback.enter_current().unwrap();

    prefetcher
        .submit(JobId::from_raw(10), plan(0, 2, 2))
        .unwrap();
    while !evaluator.started.load(Ordering::Acquire) {
        thread::yield_now();
    }
    prefetcher
        .submit(JobId::from_raw(11), plan(10, 1, 0))
        .unwrap();
    evaluator.hold_first.store(false, Ordering::Release);

    let completed = poll_until(&mut prefetcher, 2);
    assert_eq!(completed[0].generation(), 1);
    assert_eq!(completed[0].status(), JobTerminalState::Cancelled);
    assert!(completed[0].completed_frames() <= 1);
    assert_eq!(completed[1].generation(), 2);
    assert_eq!(completed[1].status(), JobTerminalState::Succeeded);
    assert_eq!(completed[1].completed_frames(), 1);
    assert_eq!(
        evaluator
            .frames
            .lock()
            .unwrap()
            .iter()
            .map(|frame| frame.value())
            .collect::<Vec<_>>(),
        vec![1, 11]
    );

    drop(prefetcher);
    drop(_playback);
    Arc::try_unwrap(pool).ok().unwrap().shutdown().unwrap();
}

#[test]
fn failure_is_structured_degraded_state_and_later_playback_work_can_continue() {
    let pool = Arc::new(BoundedWorkerPool::new(WorkerPoolConfig::new(1, 8).unwrap()).unwrap());
    let evaluator = Arc::new(RecordingEvaluator::default());
    evaluator.fail.store(true, Ordering::Release);
    let mut prefetcher = PlaybackPrefetcher::new(&pool, evaluator.clone());
    let _playback = ExecutionDomain::Playback.enter_current().unwrap();

    prefetcher
        .submit(JobId::from_raw(20), plan(4, 1, 0))
        .unwrap();
    let failed = poll_until(&mut prefetcher, 1).remove(0);
    assert_eq!(failed.status(), JobTerminalState::Failed);
    assert_eq!(failed.completed_frames(), 0);
    assert_eq!(
        failed.error().unwrap().recoverability(),
        Recoverability::Degraded
    );

    evaluator.fail.store(false, Ordering::Release);
    prefetcher
        .submit(JobId::from_raw(21), plan(5, 1, 0))
        .unwrap();
    let recovered = poll_until(&mut prefetcher, 1).remove(0);
    assert_eq!(recovered.status(), JobTerminalState::Succeeded);
    assert_eq!(recovered.completed_frames(), 1);

    drop(prefetcher);
    drop(_playback);
    Arc::try_unwrap(pool).ok().unwrap().shutdown().unwrap();
}

#[test]
fn empty_boundary_plan_cancels_prior_work_without_queueing_a_replacement() {
    let pool = Arc::new(BoundedWorkerPool::new(WorkerPoolConfig::new(1, 8).unwrap()).unwrap());
    let evaluator = Arc::new(RecordingEvaluator::default());
    evaluator.hold_first.store(true, Ordering::Release);
    let mut prefetcher = PlaybackPrefetcher::new(&pool, evaluator.clone());
    let _playback = ExecutionDomain::Playback.enter_current().unwrap();

    prefetcher
        .submit(JobId::from_raw(30), plan(0, 2, 0))
        .unwrap();
    while !evaluator.started.load(Ordering::Acquire) {
        thread::yield_now();
    }
    let timebase = Timebase::integer(24).unwrap();
    let empty = PlaybackPrefetchConfig::new(1, 0, 0).unwrap().plan(
        PlaybackPrefetchInput::new(
            RationalTime::new(0, timebase),
            TimeRange::new(
                RationalTime::new(0, timebase),
                Duration::new(1, timebase).unwrap(),
            )
            .unwrap(),
            1,
        )
        .unwrap(),
    );
    let submitted = prefetcher.submit(JobId::from_raw(31), empty).unwrap();
    assert!(!submitted.is_queued());
    evaluator.hold_first.store(false, Ordering::Release);

    let completed = poll_until(&mut prefetcher, 2);
    assert_eq!(completed[0].status(), JobTerminalState::Succeeded);
    assert_eq!(completed[0].planned_frames(), 0);
    assert_eq!(completed[1].status(), JobTerminalState::Cancelled);

    drop(prefetcher);
    drop(_playback);
    Arc::try_unwrap(pool).ok().unwrap().shutdown().unwrap();
}
