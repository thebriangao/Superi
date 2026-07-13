use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Barrier};

use superi_concurrency::jobs::{
    BoundedWorkerPool, DispatchOrigin, JobControl, JobExecutionContext, JobKind, JobPriority,
    JobProgress, ScheduledJob, WorkerPoolConfig,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result as SuperiResult};
use superi_core::ids::JobId;

fn job<T, F>(raw: u128, priority: JobPriority, task: F) -> ScheduledJob<F>
where
    F: FnOnce(JobExecutionContext) -> SuperiResult<T>,
{
    ScheduledJob::new(JobId::from_raw(raw), JobKind::Frame, priority, task)
}

#[test]
fn configuration_is_explicit_bounded_and_resource_aware() {
    let config = WorkerPoolConfig::new(3, 7).unwrap();
    assert_eq!(config.worker_count(), 3);
    assert_eq!(config.queue_capacity(), 7);

    assert_eq!(
        WorkerPoolConfig::new(0, 1).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        WorkerPoolConfig::new(1, 0).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        WorkerPoolConfig::new(WorkerPoolConfig::MAX_WORKERS + 1, 1)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    let recommended = WorkerPoolConfig::recommended(9).unwrap();
    assert!((1..=WorkerPoolConfig::MAX_WORKERS).contains(&recommended.worker_count()));
    assert_eq!(recommended.queue_capacity(), 9);
}

#[test]
fn queue_capacity_and_active_parallelism_are_hard_bounds() -> SuperiResult<()> {
    let pool = BoundedWorkerPool::new(WorkerPoolConfig::new(2, 1)?)?;
    let (started_tx, started_rx) = mpsc::channel();
    let release = Arc::new(Barrier::new(3));

    let mut handles = Vec::new();
    for raw in 1..=2 {
        let started_tx = started_tx.clone();
        let release = Arc::clone(&release);
        handles.push(pool.submit(
            job(raw, JobPriority::Playback, move |context| {
                context.check("start_bounded_job")?;
                started_tx.send(()).unwrap();
                release.wait();
                Ok(raw)
            }),
            JobControl::new(),
            JobProgress::with_total(1)?,
        )?);
        started_rx.recv().unwrap();
    }
    drop(started_tx);

    handles.push(pool.submit(
        job(3, JobPriority::Background, |_context| Ok(3)),
        JobControl::new(),
        JobProgress::indeterminate(),
    )?);
    let full = pool
        .submit(
            job(4, JobPriority::Interactive, |_context| Ok(4)),
            JobControl::new(),
            JobProgress::indeterminate(),
        )
        .unwrap_err();
    assert_eq!(full.category(), ErrorCategory::ResourceExhausted);

    let saturated = pool.snapshot();
    assert_eq!(saturated.worker_count(), 2);
    assert_eq!(saturated.active_workers(), 2);
    assert_eq!(saturated.queued_jobs(), 1);
    assert_eq!(saturated.queue_capacity(), 1);
    assert_eq!(saturated.maximum_active_workers(), 2);

    release.wait();
    for handle in handles {
        assert!(handle.wait()?.completion().outcome().error().is_none());
    }
    let report = pool.shutdown()?;
    assert_eq!(report.maximum_active_workers(), 2);
    assert_eq!(report.completed_jobs(), 3);
    Ok(())
}

#[test]
fn idle_workers_steal_from_busy_owners_and_keep_domain_ownership_explicit() -> SuperiResult<()> {
    let pool = BoundedWorkerPool::new(WorkerPoolConfig::new(2, 8)?)?;
    let (started_tx, started_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let blocking = pool.submit(
        job(10, JobPriority::Interactive, move |context| {
            ExecutionDomain::BackgroundJob.require_current()?;
            started_tx
                .send((context.worker_index(), context.dispatch_origin()))
                .unwrap();
            release_rx.recv().unwrap();
            context.check("finish_owner_job")?;
            Ok(context.worker_index())
        }),
        JobControl::new(),
        JobProgress::indeterminate(),
    )?;
    let (busy_worker, blocking_origin) = started_rx.recv().unwrap();
    let idle_worker = 1 - busy_worker;

    let local = pool.submit(
        job(11, JobPriority::Playback, |context| {
            ExecutionDomain::BackgroundJob.require_current()?;
            Ok(context.worker_index())
        }),
        JobControl::new(),
        JobProgress::indeterminate(),
    )?;
    let stolen = pool.submit(
        job(12, JobPriority::Playback, |context| {
            ExecutionDomain::BackgroundJob.require_current()?;
            Ok(context.worker_index())
        }),
        JobControl::new(),
        JobProgress::indeterminate(),
    )?;

    let first = local.wait()?;
    let second = stolen.wait()?;
    let (local, stolen) = if first.dispatch_origin() == DispatchOrigin::Owned {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(local.preferred_worker(), idle_worker);
    assert_eq!(local.worker_index(), idle_worker);
    assert_eq!(local.dispatch_origin(), DispatchOrigin::Owned);
    assert_eq!(stolen.preferred_worker(), busy_worker);
    assert_eq!(stolen.worker_index(), idle_worker);
    assert_eq!(
        stolen.dispatch_origin(),
        DispatchOrigin::Stolen {
            from_worker: busy_worker
        }
    );

    release_tx.send(()).unwrap();
    assert_eq!(blocking.wait()?.worker_index(), busy_worker);
    let report = pool.shutdown()?;
    let expected_steals = if blocking_origin == DispatchOrigin::Owned {
        1
    } else {
        2
    };
    assert_eq!(report.stolen_dispatches(), expected_steals);
    Ok(())
}

#[test]
fn global_priority_service_protects_interaction_across_local_queues() -> SuperiResult<()> {
    let pool = BoundedWorkerPool::new(WorkerPoolConfig::new(2, 8)?)?;
    let (first_started_tx, first_started_rx) = mpsc::sync_channel(0);
    let (first_release_tx, first_release_rx) = mpsc::sync_channel(0);
    let (second_started_tx, second_started_rx) = mpsc::sync_channel(0);
    let (second_release_tx, second_release_rx) = mpsc::sync_channel(0);
    let (order_tx, order_rx) = mpsc::channel();

    let first_blocker = pool.submit(
        job(20, JobPriority::Background, move |_context| {
            first_started_tx.send(()).unwrap();
            first_release_rx.recv().unwrap();
            Ok(())
        }),
        JobControl::new(),
        JobProgress::indeterminate(),
    )?;
    first_started_rx.recv().unwrap();
    let second_blocker = pool.submit(
        job(25, JobPriority::Background, move |_context| {
            second_started_tx.send(()).unwrap();
            second_release_rx.recv().unwrap();
            Ok(())
        }),
        JobControl::new(),
        JobProgress::indeterminate(),
    )?;
    second_started_rx.recv().unwrap();

    let mut handles = Vec::new();
    for (raw, priority, label) in [
        (21, JobPriority::Background, "background"),
        (22, JobPriority::Export, "export"),
        (23, JobPriority::Playback, "playback"),
        (24, JobPriority::Interactive, "interactive"),
    ] {
        let order_tx = order_tx.clone();
        handles.push(pool.submit(
            job(raw, priority, move |_context| {
                order_tx.send(label).unwrap();
                Ok(())
            }),
            JobControl::new(),
            JobProgress::indeterminate(),
        )?);
    }
    drop(order_tx);

    first_release_tx.send(()).unwrap();
    first_blocker.wait()?;
    let order = order_rx.iter().collect::<Vec<_>>();
    assert_eq!(order, ["interactive", "playback", "export", "background"]);
    second_release_tx.send(()).unwrap();
    second_blocker.wait()?;
    for handle in handles {
        handle.wait()?;
    }
    pool.shutdown()?;
    Ok(())
}

#[test]
fn lifecycle_progress_panics_and_nonblocking_observation_use_shared_contracts() -> SuperiResult<()>
{
    let pool = BoundedWorkerPool::new(WorkerPoolConfig::new(1, 8)?)?;
    let cancelled = JobControl::new();
    cancelled.cancellation_token().cancel();
    let ran = Arc::new(AtomicUsize::new(0));
    let ran_in_task = Arc::clone(&ran);
    let cancelled_handle = pool.submit(
        job(30, JobPriority::Interactive, move |_context| {
            ran_in_task.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
        cancelled,
        JobProgress::indeterminate(),
    )?;
    let cancelled = cancelled_handle.wait()?;
    assert_eq!(cancelled.completion().status().code(), "cancelled");
    assert_eq!(ran.load(Ordering::SeqCst), 0);

    let progress = JobProgress::with_total(2)?;
    let progress_in_task = progress.clone();
    let (progress_started_tx, progress_started_rx) = mpsc::sync_channel(0);
    let (progress_release_tx, progress_release_rx) = mpsc::sync_channel(0);
    let success = pool.submit(
        job(31, JobPriority::Playback, move |context| {
            assert_eq!(context.progress().increment(1)?.completed_units(), 1);
            progress_started_tx.send(()).unwrap();
            progress_release_rx.recv().unwrap();
            context.check("midpoint")?;
            assert_eq!(context.progress().finish()?.completed_units(), 2);
            Ok(7)
        }),
        JobControl::new(),
        progress_in_task,
    )?;
    progress_started_rx.recv().unwrap();
    assert!(success.try_completion()?.is_none());
    progress_release_tx.send(()).unwrap();
    let success = success.wait()?;
    assert_eq!(success.completion().progress().completed_units(), 2);
    assert_eq!(
        success.into_completion().into_outcome().into_success(),
        Some(7)
    );

    let panicked = pool.submit(
        job(
            32,
            JobPriority::Background,
            |_context| -> SuperiResult<()> { panic!("contained worker panic") },
        ),
        JobControl::new(),
        JobProgress::indeterminate(),
    )?;
    let panicked = panicked.wait()?;
    assert_eq!(panicked.completion().status().code(), "failed");
    assert_eq!(
        panicked.completion().outcome().error().unwrap().category(),
        ErrorCategory::Internal
    );

    let after_panic = pool.submit(
        job(33, JobPriority::Interactive, |_context| Ok(9)),
        JobControl::new(),
        JobProgress::indeterminate(),
    )?;
    assert_eq!(
        after_panic
            .wait()?
            .into_completion()
            .into_outcome()
            .into_success(),
        Some(9)
    );
    let report = pool.shutdown()?;
    assert_eq!(report.completed_jobs(), 4);
    Ok(())
}

#[test]
fn waiting_is_rejected_from_nonblocking_execution_domains() -> SuperiResult<()> {
    let pool = BoundedWorkerPool::new(WorkerPoolConfig::new(1, 2)?)?;
    let handle = pool.submit(
        job(40, JobPriority::Interactive, |_context| Ok(())),
        JobControl::new(),
        JobProgress::indeterminate(),
    )?;
    let guard = ExecutionDomain::Ui.enter_current()?;
    let error = handle.wait().unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    drop(guard);
    assert_eq!(handle.wait()?.completion().status().code(), "succeeded");
    pool.shutdown()?;
    Ok(())
}

#[test]
fn pool_contract_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BoundedWorkerPool>();
}
