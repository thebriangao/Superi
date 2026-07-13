use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use superi_concurrency::jobs::{
    JobCancellationToken, JobCompletion, JobControl, JobDependencies, JobDependencyFailure,
    JobDependencyStatus, JobDependencyTracker, JobInterruption, JobOutcome, JobProgress,
    JobTerminalState,
};
use superi_core::error::{
    Error, ErrorCategory, ErrorContext, Recoverability, Result as SuperiResult,
};
use superi_core::ids::JobId;

#[test]
fn cancellation_and_deadlines_are_cooperative_and_actionable() {
    let job_id = JobId::from_raw(17);
    let cancellation = JobCancellationToken::new();
    let control = JobControl::new().with_cancellation(cancellation.clone());
    assert_eq!(control.interruption(), None);
    assert!(control.check(job_id, "render_frame").is_ok());

    let worker_token = cancellation.clone();
    thread::spawn(move || worker_token.cancel()).join().unwrap();

    assert_eq!(control.interruption(), Some(JobInterruption::Cancelled));
    let cancelled = control.check(job_id, "render_frame").unwrap_err();
    assert_eq!(cancelled.category(), ErrorCategory::Cancelled);
    assert_eq!(cancelled.recoverability(), Recoverability::Degraded);
    let context = cancelled.contexts().last().unwrap();
    assert_eq!(context.component(), "superi-concurrency.job");
    assert_eq!(context.operation(), "render_frame");
    assert_eq!(context.field("job_id"), Some(job_id.to_string().as_str()));
    assert_eq!(JobInterruption::Cancelled.code(), "cancelled");
    assert_eq!(
        JobInterruption::DeadlineExceeded.code(),
        "deadline_exceeded"
    );

    let expired = JobControl::new().with_deadline(Instant::now());
    assert_eq!(
        expired.interruption(),
        Some(JobInterruption::DeadlineExceeded)
    );
    assert_eq!(
        expired.check(job_id, "decode_tile").unwrap_err().category(),
        ErrorCategory::Timeout
    );

    let cancelled_and_expired = JobControl::new().with_deadline(Instant::now());
    cancelled_and_expired.cancellation_token().cancel();
    assert_eq!(
        cancelled_and_expired.interruption(),
        Some(JobInterruption::Cancelled)
    );

    let timed = JobControl::new()
        .with_timeout(Duration::from_secs(1))
        .unwrap();
    assert!(timed
        .remaining()
        .is_some_and(|remaining| !remaining.is_zero()));
    assert_eq!(
        JobControl::new()
            .with_timeout(Duration::MAX)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn dependencies_are_validated_resolved_and_reported_deterministically() {
    let owner = JobId::from_raw(10);
    let first = JobId::from_raw(1);
    let second = JobId::from_raw(2);
    let third = JobId::from_raw(3);
    let dependencies = JobDependencies::new(owner, [third, first, second]).unwrap();
    assert_eq!(
        dependencies.iter().copied().collect::<Vec<_>>(),
        vec![first, second, third]
    );

    let tracker = JobDependencyTracker::new(dependencies);
    assert_eq!(
        tracker.status(),
        JobDependencyStatus::Waiting(vec![first, second, third])
    );
    assert_eq!(
        tracker.record(second, JobTerminalState::Succeeded).unwrap(),
        JobDependencyStatus::Waiting(vec![first, third])
    );

    let blocked = tracker.record(first, JobTerminalState::Cancelled).unwrap();
    let failure = JobDependencyFailure::new(first, JobTerminalState::Cancelled).unwrap();
    assert_eq!(blocked, JobDependencyStatus::Blocked(vec![failure.clone()]));
    assert_eq!(
        tracker.record(first, JobTerminalState::Cancelled).unwrap(),
        blocked
    );
    assert_eq!(
        tracker
            .record(first, JobTerminalState::Failed)
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    assert_eq!(
        tracker
            .record(JobId::from_raw(99), JobTerminalState::Succeeded)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    let ready =
        JobDependencyTracker::new(JobDependencies::new(owner, [first, second, third]).unwrap());
    for dependency in [third, first, second] {
        ready
            .record(dependency, JobTerminalState::Succeeded)
            .unwrap();
    }
    assert_eq!(ready.status(), JobDependencyStatus::Ready);

    let contended_dependencies = (20..52).map(JobId::from_raw).collect::<Vec<_>>();
    let contended = Arc::new(JobDependencyTracker::new(
        JobDependencies::new(owner, contended_dependencies.iter().copied()).unwrap(),
    ));
    let recorders = contended_dependencies
        .into_iter()
        .map(|dependency| {
            let contended = Arc::clone(&contended);
            thread::spawn(move || {
                contended
                    .record(dependency, JobTerminalState::Succeeded)
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();
    for recorder in recorders {
        recorder.join().unwrap();
    }
    assert_eq!(contended.status(), JobDependencyStatus::Ready);
    assert_eq!(
        JobDependencyTracker::new(JobDependencies::none(owner)).status(),
        JobDependencyStatus::Ready
    );

    assert_eq!(
        JobDependencies::new(owner, [owner]).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        JobDependencies::new(owner, [first, first])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        JobDependencyFailure::new(first, JobTerminalState::Succeeded)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn progress_is_monotonic_and_contention_safe() {
    let progress = Arc::new(JobProgress::with_total(8_000).unwrap());
    let workers = (0..8)
        .map(|_| {
            let progress = Arc::clone(&progress);
            thread::spawn(move || {
                for _ in 0..1_000 {
                    progress.increment(1).unwrap();
                }
            })
        })
        .collect::<Vec<_>>();

    for worker in workers {
        worker.join().unwrap();
    }

    let snapshot = progress.snapshot();
    assert_eq!(snapshot.completed_units(), 8_000);
    assert_eq!(snapshot.total_units(), Some(8_000));
    assert_eq!(snapshot.revision(), 8_000);
    assert!(snapshot.is_complete());
    assert_eq!(
        progress.advance_to(7_999).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(
        progress.increment(1).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );

    let indeterminate = JobProgress::indeterminate();
    assert_eq!(indeterminate.increment(5).unwrap().completed_units(), 5);
    assert_eq!(indeterminate.snapshot().total_units(), None);
    assert!(!indeterminate.snapshot().is_complete());
    assert_eq!(
        JobProgress::with_total(0).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );

    let finishing = JobProgress::with_total(4).unwrap();
    finishing.advance_to(2).unwrap();
    let finished = finishing.finish().unwrap();
    assert_eq!(finished.completed_units(), 4);
    assert_eq!(finished.revision(), 2);
    assert_eq!(finishing.finish().unwrap(), finished);
}

#[test]
fn structured_completions_preserve_identity_progress_and_failure_detail() {
    let job_id = JobId::from_raw(42);
    let progress = JobProgress::with_total(4).unwrap();
    progress.increment(3).unwrap();
    let error = Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "render device is temporarily unavailable",
    )
    .with_context(ErrorContext::new("test", "render"));
    let completion = JobCompletion::new(
        job_id,
        JobOutcome::<()>::Failed(error),
        progress.snapshot(),
        Duration::from_millis(9),
    );

    assert_eq!(completion.job_id(), job_id);
    assert_eq!(completion.status(), JobTerminalState::Failed);
    assert_eq!(completion.progress().completed_units(), 3);
    assert_eq!(completion.elapsed(), Duration::from_millis(9));
    assert_eq!(
        completion.outcome().error().unwrap().category(),
        ErrorCategory::Unavailable
    );
    assert_eq!(
        completion.outcome().error().unwrap().contexts()[0].operation(),
        "render"
    );

    let dependency =
        JobDependencyFailure::new(JobId::from_raw(7), JobTerminalState::DeadlineExceeded).unwrap();
    let blocked = JobCompletion::new(
        job_id,
        JobOutcome::<()>::DependencyFailed(dependency.clone()),
        JobProgress::indeterminate().snapshot(),
        Duration::ZERO,
    );
    assert_eq!(blocked.status(), JobTerminalState::DependencyFailed);
    assert_eq!(blocked.outcome().dependency_failure(), Some(&dependency));

    assert_eq!(
        JobOutcome::<()>::Cancelled.status(),
        JobTerminalState::Cancelled
    );
    assert_eq!(
        JobOutcome::<()>::DeadlineExceeded.status(),
        JobTerminalState::DeadlineExceeded
    );
    assert_eq!(JobOutcome::Succeeded(9).into_success(), Some(9));
    assert_eq!(JobTerminalState::Succeeded.code(), "succeeded");
    assert_eq!(JobTerminalState::Failed.code(), "failed");
    assert_eq!(JobTerminalState::Cancelled.code(), "cancelled");
    assert_eq!(
        JobTerminalState::DeadlineExceeded.code(),
        "deadline_exceeded"
    );
    assert_eq!(
        JobTerminalState::DependencyFailed.code(),
        "dependency_failed"
    );
}

#[test]
fn shared_job_controls_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<JobCancellationToken>();
    assert_send_sync::<JobControl>();
    assert_send_sync::<JobProgress>();
    assert_send_sync::<JobDependencyTracker>();
    assert_send_sync::<JobOutcome<()>>();
    assert_send_sync::<JobCompletion<()>>();
}

#[test]
fn public_job_results_use_the_shared_error_contract() -> SuperiResult<()> {
    let outcome = JobOutcome::<()>::Failed(Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "job failed",
    ));
    assert_eq!(outcome.status(), JobTerminalState::Failed);
    Ok(())
}
