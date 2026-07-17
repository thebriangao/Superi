use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::JobId;
use superi_engine::export_jobs::{
    ExportJobExecutionContext, ExportJobExecutor, ExportJobQueue, ExportJobQueueConfig,
    ExportJobSnapshot, ExportJobStatus,
};

struct ConstantExport(u64);

impl ExportJobExecutor<u64> for ConstantExport {
    fn execute(&self, context: &ExportJobExecutionContext) -> Result<u64> {
        context.check("before_export")?;
        context.progress().increment(3)?;
        Ok(self.0)
    }
}

fn poll_until<R>(
    queue: &mut ExportJobQueue<R>,
    job_id: JobId,
    expected: ExportJobStatus,
) -> Result<ExportJobSnapshot>
where
    R: Send + Sync + 'static,
{
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        queue.poll()?;
        let snapshot = queue.snapshot(job_id)?;
        if snapshot.status() == expected {
            return Ok(snapshot);
        }
        assert!(
            Instant::now() < deadline,
            "export job did not reach {} from {}",
            expected.code(),
            snapshot.status().code()
        );
        thread::yield_now();
    }
}

#[test]
fn completed_export_jobs_publish_progress_and_typed_results_without_blocking() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut queue = ExportJobQueue::new(ExportJobQueueConfig::new(1, 2, 4)?)?;
    let job_id = JobId::from_raw(1);

    queue.submit_executor(job_id, [], ConstantExport(42))?;

    let snapshot = poll_until(&mut queue, job_id, ExportJobStatus::Completed)?;
    assert_eq!(snapshot.attempt(), 1);
    assert_eq!(snapshot.progress().completed_units(), 3);
    assert!(snapshot.has_result());
    assert_eq!(*queue.result(job_id)?.unwrap(), 42);

    drop(guard);
    queue.shutdown()?;
    Ok(())
}

#[test]
fn recoverable_failures_wait_for_retry_before_releasing_dependencies() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut queue: ExportJobQueue<u64> = ExportJobQueue::new(ExportJobQueueConfig::new(2, 2, 6)?)?;
    let prerequisite = JobId::from_raw(10);
    let dependent = JobId::from_raw(11);
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_in_job = Arc::clone(&attempts);
    let dependent_runs = Arc::new(AtomicUsize::new(0));
    let dependent_runs_in_job = Arc::clone(&dependent_runs);

    queue.submit(prerequisite, [], move |context| {
        let attempt = attempts_in_job.fetch_add(1, Ordering::SeqCst) + 1;
        context.progress().increment(1)?;
        if attempt == 1 {
            return Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "temporary encoder outage",
            )
            .with_context(ErrorContext::new("test.export", "encode")));
        }
        Ok(5)
    })?;
    queue.submit(dependent, [prerequisite], move |context| {
        dependent_runs_in_job.fetch_add(1, Ordering::SeqCst);
        context.progress().increment(1)?;
        Ok(10)
    })?;

    let failed = poll_until(&mut queue, prerequisite, ExportJobStatus::Failed)?;
    assert!(failed.retry_allowed());
    assert_eq!(
        failed.failure().unwrap().recoverability(),
        Recoverability::Retryable
    );
    assert_eq!(
        queue.snapshot(dependent)?.status(),
        ExportJobStatus::Waiting
    );
    assert_eq!(dependent_runs.load(Ordering::SeqCst), 0);

    queue.retry(prerequisite)?;
    assert_eq!(
        poll_until(&mut queue, prerequisite, ExportJobStatus::Completed)?.attempt(),
        2
    );
    assert_eq!(
        poll_until(&mut queue, dependent, ExportJobStatus::Completed)?.attempt(),
        1
    );
    assert_eq!(*queue.result(prerequisite)?.unwrap(), 5);
    assert_eq!(*queue.result(dependent)?.unwrap(), 10);
    assert_eq!(dependent_runs.load(Ordering::SeqCst), 1);

    drop(guard);
    queue.shutdown()?;
    Ok(())
}

#[test]
fn failure_snapshots_distinguish_all_recoverability_classes_and_terminal_dependencies() -> Result<()>
{
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut queue: ExportJobQueue<u64> = ExportJobQueue::new(ExportJobQueueConfig::new(2, 4, 8)?)?;

    for (raw, recoverability) in [
        (20, Recoverability::Retryable),
        (21, Recoverability::Degraded),
        (22, Recoverability::UserCorrectable),
        (23, Recoverability::Terminal),
    ] {
        queue.submit(JobId::from_raw(raw), [], move |_context| {
            Err(Error::new(
                ErrorCategory::Unavailable,
                recoverability,
                "classified export failure",
            )
            .with_context(ErrorContext::new("test.export", "classified_failure")))
        })?;
    }
    let dependent_runs = Arc::new(AtomicUsize::new(0));
    let dependent_runs_in_job = Arc::clone(&dependent_runs);
    queue.submit(
        JobId::from_raw(24),
        [JobId::from_raw(23)],
        move |_context| {
            dependent_runs_in_job.fetch_add(1, Ordering::SeqCst);
            Ok(24)
        },
    )?;

    for (raw, recoverability) in [
        (20, Recoverability::Retryable),
        (21, Recoverability::Degraded),
        (22, Recoverability::UserCorrectable),
        (23, Recoverability::Terminal),
    ] {
        let snapshot = poll_until(&mut queue, JobId::from_raw(raw), ExportJobStatus::Failed)?;
        let failure = snapshot.failure().unwrap();
        assert_eq!(failure.recoverability(), recoverability);
        assert_eq!(failure.message(), "classified export failure");
        assert!(failure
            .contexts()
            .iter()
            .any(|context| context.component() == "superi-engine.export_jobs"));
        assert_eq!(
            snapshot.retry_allowed(),
            recoverability != Recoverability::Terminal
        );
    }

    let blocked = poll_until(
        &mut queue,
        JobId::from_raw(24),
        ExportJobStatus::DependencyFailed,
    )?;
    assert_eq!(blocked.dependency_failures().len(), 1);
    assert_eq!(
        blocked.dependency_failures()[0].job_id(),
        JobId::from_raw(23)
    );
    assert_eq!(blocked.dependency_failures()[0].state().code(), "failed");
    assert_eq!(dependent_runs.load(Ordering::SeqCst), 0);
    assert_eq!(
        queue.retry(JobId::from_raw(23)).unwrap_err().category(),
        ErrorCategory::Conflict
    );

    drop(guard);
    queue.shutdown()?;
    Ok(())
}

#[test]
fn pause_acknowledges_one_attempt_and_resume_starts_fresh_progress() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut queue: ExportJobQueue<u64> = ExportJobQueue::new(ExportJobQueueConfig::new(1, 1, 3)?)?;
    let job_id = JobId::from_raw(30);
    let entered = Arc::new(AtomicBool::new(false));
    let entered_in_job = Arc::clone(&entered);

    queue.submit(job_id, [], move |context| {
        context.progress().increment(1)?;
        if context.attempt() == 1 {
            entered_in_job.store(true, Ordering::SeqCst);
            loop {
                context.check("wait_for_pause")?;
                thread::yield_now();
            }
        }
        context.progress().increment(2)?;
        Ok(u64::from(context.attempt()))
    })?;

    poll_until(&mut queue, job_id, ExportJobStatus::Running)?;
    let entry_deadline = Instant::now() + Duration::from_secs(2);
    while !entered.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < entry_deadline,
            "export executor did not enter its first attempt"
        );
        thread::yield_now();
    }
    assert_eq!(queue.pause(job_id)?.status(), ExportJobStatus::Pausing);
    let paused = poll_until(&mut queue, job_id, ExportJobStatus::Paused)?;
    assert_eq!(paused.attempt(), 1);
    assert_eq!(paused.progress().completed_units(), 1);
    assert!(!paused.has_result());

    queue.resume(job_id)?;
    let completed = poll_until(&mut queue, job_id, ExportJobStatus::Completed)?;
    assert_eq!(completed.attempt(), 2);
    assert_eq!(completed.progress().completed_units(), 3);
    assert_eq!(*queue.result(job_id)?.unwrap(), 2);

    drop(guard);
    queue.shutdown()?;
    Ok(())
}

#[test]
fn cancellation_wins_over_active_work_and_publishes_no_result() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut queue: ExportJobQueue<u64> = ExportJobQueue::new(ExportJobQueueConfig::new(1, 1, 3)?)?;
    let job_id = JobId::from_raw(40);

    queue.submit(job_id, [], |context| loop {
        context.check("wait_for_cancel")?;
        thread::yield_now();
    })?;
    poll_until(&mut queue, job_id, ExportJobStatus::Running)?;
    assert_eq!(queue.cancel(job_id)?.status(), ExportJobStatus::Cancelling);
    let cancelled = poll_until(&mut queue, job_id, ExportJobStatus::Cancelled)?;
    assert!(cancelled.is_final());
    assert!(!cancelled.has_result());
    assert!(queue.result(job_id)?.is_none());

    drop(guard);
    queue.shutdown()?;
    Ok(())
}

#[test]
fn retained_bounds_identity_dependency_order_and_removal_are_explicit() -> Result<()> {
    assert_eq!(
        ExportJobQueueConfig::new(1, 2, 2).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );

    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut queue: ExportJobQueue<u64> = ExportJobQueue::new(ExportJobQueueConfig::new(1, 1, 2)?)?;
    let first = JobId::from_raw(50);
    let second = JobId::from_raw(51);
    queue.submit(first, [], |_context| Ok(1))?;
    assert_eq!(
        queue
            .submit(first, [], |_context| Ok(2))
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    assert_eq!(
        queue
            .submit(second, [JobId::from_raw(99)], |_context| Ok(2))
            .unwrap_err()
            .category(),
        ErrorCategory::NotFound
    );
    queue.submit(second, [first], |_context| Ok(2))?;
    assert_eq!(
        queue
            .submit(JobId::from_raw(52), [], |_context| Ok(3))
            .unwrap_err()
            .category(),
        ErrorCategory::ResourceExhausted
    );

    poll_until(&mut queue, first, ExportJobStatus::Completed)?;
    poll_until(&mut queue, second, ExportJobStatus::Completed)?;
    assert_eq!(
        queue.remove(first).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(queue.remove(second)?.status(), ExportJobStatus::Completed);
    assert_eq!(queue.remove(first)?.status(), ExportJobStatus::Completed);
    assert!(queue.is_empty()?);

    drop(guard);
    queue.shutdown()?;
    Ok(())
}

#[test]
fn control_and_shutdown_respect_execution_domain_policy() -> Result<()> {
    assert_eq!(
        ExportJobQueue::<u64>::new(ExportJobQueueConfig::new(1, 1, 2)?)
            .err()
            .unwrap()
            .category(),
        ErrorCategory::Conflict
    );

    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut queue: ExportJobQueue<u64> = ExportJobQueue::new(ExportJobQueueConfig::new(1, 1, 2)?)?;
    assert_eq!(queue.len()?, 0);
    assert_eq!(queue.worker_snapshot()?.worker_count(), 1);
    assert_eq!(
        queue.shutdown().unwrap_err().category(),
        ErrorCategory::Conflict
    );
    drop(guard);

    assert_eq!(
        queue.snapshot(JobId::from_raw(1)).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    queue.shutdown()?;
    Ok(())
}
