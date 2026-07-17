use std::thread;
use std::time::{Duration, Instant};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    sync::Arc,
};

use serde_json::json;
use superi_api::commands::{
    ApiCommand, CancelAllAsyncJobs, CancelAsyncJob, GetAsyncJobs, PauseAsyncJob, RemoveAsyncJob,
    ResumeAsyncJob, RetryAsyncJob,
};
use superi_api::events::{ApiEvent, AsyncJobsChanged};
use superi_api::jobs::{
    AsyncJobHandle, AsyncJobKind, AsyncJobPriority, AsyncJobStatus, AsyncJobsApi,
};
use superi_api::version::{
    ASYNC_JOBS_CHANGED_EVENT, ASYNC_JOBS_SCHEMA_VERSION, CANCEL_ALL_ASYNC_JOBS_METHOD,
    CANCEL_ASYNC_JOB_METHOD, GET_ASYNC_JOBS_METHOD, PAUSE_ASYNC_JOB_METHOD,
    REMOVE_ASYNC_JOB_METHOD, RESUME_ASYNC_JOB_METHOD, RETRY_ASYNC_JOB_METHOD,
};
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::ids::JobId;
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineTransactionId,
};
use superi_engine::export_dispatch::EngineExportJobCommand;
use superi_engine::export_jobs::{ExportJobExecutionContext, ExportJobQueueConfig};
use superi_engine::lifecycle::EngineLifecycleSnapshot;

#[test]
fn public_job_wire_contract_is_strict_versioned_and_namespaced() {
    let handle = AsyncJobHandle::new("job:0000000000000000000000000000002a").unwrap();
    assert_eq!(
        serde_json::to_value(&handle).unwrap(),
        json!("job:0000000000000000000000000000002a")
    );
    assert!(AsyncJobHandle::new("job:0000000000000000000000000000002A").is_err());
    assert!(AsyncJobHandle::new("media:0000000000000000000000000000002a").is_err());
    assert!(AsyncJobHandle::new("job:2a").is_err());

    let command = CancelAsyncJob::new("cancel-one", handle.clone());
    let mut wire = serde_json::to_value(&command).unwrap();
    wire["guessed"] = json!(true);
    assert!(serde_json::from_value::<CancelAsyncJob>(wire).is_err());

    assert_eq!(GetAsyncJobs::METHOD, GET_ASYNC_JOBS_METHOD);
    assert_eq!(PauseAsyncJob::METHOD, PAUSE_ASYNC_JOB_METHOD);
    assert_eq!(ResumeAsyncJob::METHOD, RESUME_ASYNC_JOB_METHOD);
    assert_eq!(RetryAsyncJob::METHOD, RETRY_ASYNC_JOB_METHOD);
    assert_eq!(CancelAsyncJob::METHOD, CANCEL_ASYNC_JOB_METHOD);
    assert_eq!(CancelAllAsyncJobs::METHOD, CANCEL_ALL_ASYNC_JOBS_METHOD);
    assert_eq!(RemoveAsyncJob::METHOD, REMOVE_ASYNC_JOB_METHOD);
    assert_eq!(AsyncJobsChanged::NAME, ASYNC_JOBS_CHANGED_EVENT);
    assert_eq!(GetAsyncJobs::SCHEMA_VERSION, ASYNC_JOBS_SCHEMA_VERSION);
    assert_eq!(AsyncJobKind::Export.code(), "export");
    assert_eq!(AsyncJobPriority::Export.code(), "export");
}

#[test]
fn public_api_projects_nonblocking_progress_and_ordered_completion_events() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let runtime = dispatcher.attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 2, 4)?)?;
    start_engine(&mut dispatcher)?;
    dispatcher.drain_events()?;

    let job_id = JobId::from_raw(42);
    runtime.prepare_executor(job_id, |context: &ExportJobExecutionContext, _permit| {
        context.progress().increment(3)?;
        Ok(42)
    })?;
    dispatch(
        &mut dispatcher,
        "internal-export-submit",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(job_id, [])?),
    )?;
    dispatcher.drain_events()?;

    let handle = AsyncJobHandle::from(job_id);
    let mut api = AsyncJobsApi::new(dispatcher);
    let initial = api.execute(GetAsyncJobs::new("jobs-get"))?;
    assert_eq!(initial.transaction_id(), "jobs-get");
    assert_eq!(
        initial.snapshot().schema_version(),
        &ASYNC_JOBS_SCHEMA_VERSION
    );
    let initial_job = initial.snapshot().job(&handle).unwrap();
    assert_eq!(initial_job.kind(), AsyncJobKind::Export);
    assert_eq!(initial_job.priority(), AsyncJobPriority::Export);
    assert_eq!(initial_job.handle(), &handle);

    let deadline = Instant::now() + Duration::from_secs(2);
    let completed = loop {
        let before = Instant::now();
        let result = api.poll_runtime("host-poll")?;
        assert!(before.elapsed() < Duration::from_millis(100));
        let job = result.snapshot().job(&handle).unwrap();
        if job.status() == AsyncJobStatus::Completed {
            break result;
        }
        assert!(Instant::now() < deadline, "public job did not complete");
        thread::yield_now();
    };

    let completed_job = completed.snapshot().job(&handle).unwrap();
    assert_eq!(completed_job.progress().completed_units(), 3);
    assert!(completed_job.has_result());
    assert!(completed_job.is_final());
    assert!(serde_json::to_value(completed_job)
        .unwrap()
        .get("result")
        .is_none());
    assert_eq!(*runtime.result(job_id)?.unwrap(), 42);

    let events = api.drain_events()?;
    assert!(events
        .windows(2)
        .all(|pair| pair[0].sequence() < pair[1].sequence()));
    let event = events
        .last()
        .expect("completion publishes replacement state");
    assert_eq!(event.transaction_id(), "host-poll");
    assert_eq!(event.jobs_revision(), completed.snapshot().revision());
    assert_eq!(event.snapshot(), completed.snapshot());
    assert_eq!(
        event.snapshot().job(&handle).unwrap().status(),
        AsyncJobStatus::Completed
    );

    drop(api);
    drop(guard);
    runtime.shutdown()?;
    Ok(())
}

#[test]
fn public_pause_resume_retry_and_remove_preserve_engine_transition_policy() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let runtime = dispatcher.attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 2, 4)?)?;
    start_engine(&mut dispatcher)?;
    dispatcher.drain_events()?;

    let pause_id = JobId::from_raw(60);
    runtime.prepare_executor(pause_id, |context: &ExportJobExecutionContext, _permit| {
        if context.attempt() == 1 {
            loop {
                context.check("wait_for_public_pause")?;
                thread::yield_now();
            }
        }
        context.progress().increment(2)?;
        Ok(60)
    })?;
    dispatch(
        &mut dispatcher,
        "internal-pause-submit",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(pause_id, [])?),
    )?;

    let retry_id = JobId::from_raw(61);
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_in_job = Arc::clone(&attempts);
    runtime.prepare_executor(
        retry_id,
        move |context: &ExportJobExecutionContext, _permit| {
            let attempt = attempts_in_job.fetch_add(1, Ordering::SeqCst) + 1;
            context.progress().increment(1)?;
            if attempt == 1 {
                return Err(Error::new(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "temporary public retry fixture",
                ));
            }
            Ok(61)
        },
    )?;
    dispatch(
        &mut dispatcher,
        "internal-retry-submit",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(retry_id, [])?),
    )?;
    dispatcher.drain_events()?;

    let paused = AsyncJobHandle::from(pause_id);
    let retried = AsyncJobHandle::from(retry_id);
    let mut api = AsyncJobsApi::new(dispatcher);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let state = api.poll_runtime("host-wait-pause-running")?;
        if state.snapshot().job(&paused).unwrap().status() == AsyncJobStatus::Running {
            break;
        }
        assert!(Instant::now() < deadline, "pausable job did not start");
        thread::yield_now();
    }

    let pausing = api.pause(PauseAsyncJob::new("public-pause", paused.clone()))?;
    assert!(matches!(
        pausing.snapshot().job(&paused).unwrap().status(),
        AsyncJobStatus::Pausing | AsyncJobStatus::Paused
    ));
    loop {
        let state = api.poll_runtime("host-wait-paused-and-failed")?;
        if state.snapshot().job(&paused).unwrap().status() == AsyncJobStatus::Paused
            && state.snapshot().job(&retried).unwrap().status() == AsyncJobStatus::Failed
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "paused and failed states did not settle"
        );
        thread::yield_now();
    }

    let resumed = api.resume(ResumeAsyncJob::new("public-resume", paused.clone()))?;
    assert_eq!(resumed.snapshot().job(&paused).unwrap().attempt(), 2);
    let retrying = api.retry(RetryAsyncJob::new("public-retry", retried.clone()))?;
    assert_eq!(retrying.snapshot().job(&retried).unwrap().attempt(), 2);

    let completed = loop {
        let state = api.poll_runtime("host-wait-recovered")?;
        if state.snapshot().job(&paused).unwrap().status() == AsyncJobStatus::Completed
            && state.snapshot().job(&retried).unwrap().status() == AsyncJobStatus::Completed
        {
            break state;
        }
        assert!(Instant::now() < deadline, "resumed jobs did not complete");
        thread::yield_now();
    };
    assert_eq!(completed.snapshot().jobs().len(), 2);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    let one_left = api.remove(RemoveAsyncJob::new("public-remove-retry", retried))?;
    assert_eq!(one_left.snapshot().jobs().len(), 1);
    let empty = api.remove(RemoveAsyncJob::new("public-remove-pause", paused))?;
    assert!(empty.snapshot().jobs().is_empty());

    drop(api);
    drop(guard);
    runtime.shutdown()?;
    Ok(())
}

#[test]
fn public_cancel_all_finalizes_every_unfinished_job_in_handle_order() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let runtime = dispatcher.attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 2, 4)?)?;
    start_engine(&mut dispatcher)?;
    dispatcher.drain_events()?;

    let first_id = JobId::from_raw(70);
    let second_id = JobId::from_raw(71);
    for job_id in [first_id, second_id] {
        runtime.prepare_executor(
            job_id,
            |context: &ExportJobExecutionContext, _permit| loop {
                context.check("wait_for_public_cancel_all")?;
                thread::yield_now();
            },
        )?;
        dispatch(
            &mut dispatcher,
            &format!("internal-cancel-all-submit-{job_id}"),
            EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(
                job_id,
                if job_id == second_id {
                    vec![first_id]
                } else {
                    Vec::new()
                },
            )?),
        )?;
    }
    dispatcher.drain_events()?;

    let first = AsyncJobHandle::from(first_id);
    let second = AsyncJobHandle::from(second_id);
    let mut api = AsyncJobsApi::new(dispatcher);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let state = api.poll_runtime("host-wait-cancel-all-running")?;
        if state.snapshot().job(&first).unwrap().status() == AsyncJobStatus::Running {
            let dependent = state.snapshot().job(&second).unwrap();
            assert_eq!(dependent.status(), AsyncJobStatus::Waiting);
            assert_eq!(dependent.dependencies(), std::slice::from_ref(&first));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "first cancel-all job did not start"
        );
        thread::yield_now();
    }

    api.cancel_all(CancelAllAsyncJobs::new("public-cancel-all"))?;
    let cancelled = loop {
        let state = api.poll_runtime("host-wait-cancel-all-terminal")?;
        if state
            .snapshot()
            .jobs()
            .iter()
            .all(|job| job.status() == AsyncJobStatus::Cancelled)
        {
            break state;
        }
        assert!(Instant::now() < deadline, "cancel-all jobs did not settle");
        thread::yield_now();
    };
    assert_eq!(
        cancelled
            .snapshot()
            .jobs()
            .iter()
            .map(|job| job.handle())
            .collect::<Vec<_>>(),
        vec![&first, &second]
    );
    assert!(cancelled
        .snapshot()
        .jobs()
        .iter()
        .all(|job| job.is_final() && !job.has_result()));

    drop(api);
    drop(guard);
    runtime.shutdown()?;
    Ok(())
}

#[test]
fn public_cancellation_is_cooperative_and_failure_snapshots_are_user_safe() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let runtime = dispatcher.attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 2, 4)?)?;
    start_engine(&mut dispatcher)?;
    dispatcher.drain_events()?;

    let cancelled_id = JobId::from_raw(50);
    runtime.prepare_executor(
        cancelled_id,
        |context: &ExportJobExecutionContext, _permit| loop {
            context.check("wait_for_public_cancel")?;
            thread::yield_now();
        },
    )?;
    dispatch(
        &mut dispatcher,
        "internal-cancel-submit",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(cancelled_id, [])?),
    )?;

    let failed_id = JobId::from_raw(51);
    runtime.prepare_executor(
        failed_id,
        |_context: &ExportJobExecutionContext, _permit| {
            Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "private /secret/export.mov encoder token",
            ))
        },
    )?;
    dispatch(
        &mut dispatcher,
        "internal-failure-submit",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(failed_id, [])?),
    )?;
    dispatcher.drain_events()?;

    let cancelled = AsyncJobHandle::from(cancelled_id);
    let failed = AsyncJobHandle::from(failed_id);
    let mut api = AsyncJobsApi::new(dispatcher);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let state = api.poll_runtime("host-wait-running")?;
        if state.snapshot().job(&cancelled).unwrap().status() == AsyncJobStatus::Running {
            break;
        }
        assert!(Instant::now() < deadline, "cancellable job did not start");
        thread::yield_now();
    }

    let accepted = api.cancel(CancelAsyncJob::new("public-cancel", cancelled.clone()))?;
    assert!(matches!(
        accepted.snapshot().job(&cancelled).unwrap().status(),
        AsyncJobStatus::Cancelling | AsyncJobStatus::Cancelled
    ));

    let terminal = loop {
        let state = api.poll_runtime("host-wait-terminal")?;
        let cancelled_status = state.snapshot().job(&cancelled).unwrap().status();
        let failed_status = state.snapshot().job(&failed).unwrap().status();
        if cancelled_status == AsyncJobStatus::Cancelled && failed_status == AsyncJobStatus::Failed
        {
            break state;
        }
        assert!(Instant::now() < deadline, "public jobs did not settle");
        thread::yield_now();
    };

    let cancelled_job = terminal.snapshot().job(&cancelled).unwrap();
    assert!(cancelled_job.is_final());
    assert!(!cancelled_job.has_result());
    let failed_job = terminal.snapshot().job(&failed).unwrap();
    assert!(failed_job.retry_allowed());
    let failure = failed_job.failure().unwrap();
    assert_eq!(failure.category(), "unavailable");
    assert_eq!(failure.recoverability(), "retryable");
    let wire = serde_json::to_string(failed_job).unwrap();
    assert!(!wire.contains("secret"));
    assert!(!wire.contains("export.mov"));
    assert!(!wire.contains("encoder token"));

    let resolved = api.cancel(CancelAsyncJob::new("cancel-failed", failed.clone()))?;
    assert_eq!(
        resolved.snapshot().job(&failed).unwrap().status(),
        AsyncJobStatus::Cancelled
    );

    drop(api);
    drop(guard);
    runtime.shutdown()?;
    Ok(())
}

fn start_engine(dispatcher: &mut EngineCommandDispatcher) -> Result<()> {
    let mut state = lifecycle_state(
        dispatch(
            dispatcher,
            "inspect-startup",
            EngineCommand::InspectLifecycle,
        )?
        .result(),
    );
    while state.phase() == LifecyclePhase::Starting {
        state = lifecycle_state(
            dispatch(
                dispatcher,
                &format!(
                    "initialize-{}",
                    state.pending_action().unwrap().subsystem().code()
                ),
                EngineCommand::CompleteLifecycleAction(state.pending_action().unwrap()),
            )?
            .result(),
        );
    }
    assert_eq!(state.phase(), LifecyclePhase::Running);
    Ok(())
}

fn dispatch(
    dispatcher: &mut EngineCommandDispatcher,
    transaction_id: &str,
    command: EngineCommand,
) -> Result<superi_engine::dispatcher::EngineCommandOutcome> {
    dispatcher.dispatch(EngineCommandRequest::new(
        EngineTransactionId::new(transaction_id)?,
        command,
    ))
}

fn lifecycle_state(result: &EngineCommandResult) -> EngineLifecycleSnapshot {
    match result {
        EngineCommandResult::Lifecycle(state) => state.as_ref().clone(),
        _ => panic!("expected lifecycle result"),
    }
}
