use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::ids::JobId;
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineReportedFailure, EngineTransactionId,
};
use superi_engine::export_dispatch::{EngineExportJobCommand, EngineExportJobState};
use superi_engine::export_jobs::{
    ExportJobExecutionContext, ExportJobQueueConfig, ExportJobStatus,
};
use superi_engine::lifecycle::{
    EngineHealth, EngineLifecycleActionKind, EngineLifecycleSnapshot, EngineSubsystem,
    EngineWorkKind, EngineWorkPermit,
};

#[test]
fn export_commands_publish_ordered_full_state_and_retain_typed_results() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let runtime = dispatcher.attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 2, 4)?)?;
    start_engine(&mut dispatcher)?;
    dispatcher.drain_events()?;

    let job_id = JobId::from_raw(700);
    runtime.prepare_executor(
        job_id,
        |context: &ExportJobExecutionContext, permit: EngineWorkPermit| {
            assert_eq!(permit.work(), EngineWorkKind::Export);
            context.progress().increment(3)?;
            Ok(42)
        },
    )?;

    let submitted = dispatch(
        &mut dispatcher,
        "export-submit",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(job_id, [])?),
    )?;
    let submitted_state = export_state(submitted.result());
    assert_eq!(submitted_state.revision(), 1);
    assert_eq!(submitted_state.jobs().len(), 1);
    assert_eq!(submitted_state.job(job_id).unwrap().attempt(), 1);

    let events = dispatcher.drain_events()?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].transaction_id().as_str(), "export-submit");
    assert_eq!(events[0].export_state_revision(), Some(1));
    match events[0].event() {
        EngineEvent::ExportJobsStateChanged(state) => assert_eq!(state.as_ref(), submitted_state),
        _ => panic!("expected export replacement state"),
    }

    drop(guard);
    assert_eq!(
        runtime.shutdown().unwrap_err().category(),
        ErrorCategory::Conflict
    );
    let guard = ExecutionDomain::EngineControl.enter_current()?;

    let completed = poll_until(
        &mut dispatcher,
        job_id,
        ExportJobStatus::Completed,
        "export-poll",
    )?;
    assert!(completed.revision() > 1);
    let snapshot = completed.job(job_id).unwrap();
    assert_eq!(snapshot.progress().completed_units(), 3);
    assert!(snapshot.has_result());
    assert_eq!(*runtime.result(job_id)?.unwrap(), 42);
    let completion_events = dispatcher.drain_events()?;
    assert!(!completion_events.is_empty());
    assert_eq!(
        completion_events.last().unwrap().export_state_revision(),
        Some(completed.revision())
    );

    let inspected = dispatch(
        &mut dispatcher,
        "export-inspect",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::Inspect(job_id)),
    )?;
    assert_eq!(export_state(inspected.result()), &completed);
    assert!(dispatcher.drain_events()?.is_empty());

    let removed = dispatch(
        &mut dispatcher,
        "export-remove",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::Remove(job_id)),
    )?;
    assert!(export_state(removed.result()).jobs().is_empty());
    let removal_events = dispatcher.drain_events()?;
    assert_eq!(removal_events.len(), 1);
    assert!(matches!(
        removal_events[0].event(),
        EngineEvent::ExportJobsStateChanged(state) if state.jobs().is_empty()
    ));

    drop(guard);
    runtime.shutdown()?;
    Ok(())
}

#[test]
fn export_retry_is_denied_during_degradation_and_uses_a_fresh_recovery_permit() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let runtime = dispatcher.attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 1, 3)?)?;
    start_engine(&mut dispatcher)?;
    dispatcher.drain_events()?;

    let job_id = JobId::from_raw(710);
    let attempts = Arc::new(AtomicUsize::new(0));
    let permits = Arc::new(Mutex::new(Vec::new()));
    let attempts_in_job = Arc::clone(&attempts);
    let permits_in_job = Arc::clone(&permits);
    runtime.prepare_executor(
        job_id,
        move |_context: &ExportJobExecutionContext, permit: EngineWorkPermit| {
            permits_in_job.lock().unwrap().push(permit);
            if attempts_in_job.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(Error::new(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "temporary export failure",
                ));
            }
            Ok(71)
        },
    )?;
    dispatch(
        &mut dispatcher,
        "submit-retryable-export",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(job_id, [])?),
    )?;
    poll_until(
        &mut dispatcher,
        job_id,
        ExportJobStatus::Failed,
        "poll-failed-export",
    )?;
    dispatcher.drain_events()?;

    let degraded = lifecycle_state(
        dispatch(
            &mut dispatcher,
            "degrade-export",
            EngineCommand::ReportRuntimeFailure {
                subsystem: EngineSubsystem::Export,
                failure: EngineReportedFailure::new(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "export service is recovering",
                )?,
            },
        )?
        .result(),
    );
    assert_eq!(degraded.health(), EngineHealth::Degraded);
    let denied = dispatch(
        &mut dispatcher,
        "retry-while-degraded",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::Retry(job_id)),
    )
    .unwrap_err();
    assert_eq!(denied.category(), ErrorCategory::Conflict);

    let observed = dispatch(
        &mut dispatcher,
        "inspect-while-degraded",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::Inspect(job_id)),
    )?;
    assert_eq!(
        export_state(observed.result())
            .job(job_id)
            .unwrap()
            .status(),
        ExportJobStatus::Failed
    );

    let recovering = lifecycle_state(
        dispatch(
            &mut dispatcher,
            "begin-export-recovery",
            EngineCommand::BeginRecovery(EngineSubsystem::Export),
        )?
        .result(),
    );
    let recovery = recovering.pending_action().unwrap();
    let recovered = lifecycle_state(
        dispatch(
            &mut dispatcher,
            "complete-export-recovery",
            EngineCommand::CompleteLifecycleAction(recovery),
        )?
        .result(),
    );
    assert_eq!(recovered.health(), EngineHealth::Healthy);

    dispatch(
        &mut dispatcher,
        "retry-after-recovery",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::Retry(job_id)),
    )?;
    poll_until(
        &mut dispatcher,
        job_id,
        ExportJobStatus::Completed,
        "poll-retried-export",
    )?;
    assert_eq!(*runtime.result(job_id)?.unwrap(), 71);
    let permits = permits.lock().unwrap();
    assert_eq!(permits.len(), 2);
    assert_ne!(permits[0].state_revision(), permits[1].state_revision());

    drop(permits);
    drop(guard);
    runtime.shutdown()?;
    Ok(())
}

#[test]
fn export_pause_resume_dependencies_and_cancel_remain_dispatcher_actions() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let runtime = dispatcher.attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 2, 6)?)?;
    start_engine(&mut dispatcher)?;
    dispatcher.drain_events()?;

    let prerequisite = JobId::from_raw(720);
    let dependent = JobId::from_raw(721);
    runtime.prepare_executor(
        prerequisite,
        |context: &ExportJobExecutionContext, _permit: EngineWorkPermit| {
            if context.attempt() == 1 {
                loop {
                    context.check("wait_for_dispatched_pause")?;
                    thread::yield_now();
                }
            }
            context.progress().increment(2)?;
            Ok(72)
        },
    )?;
    runtime.prepare_executor(
        dependent,
        |context: &ExportJobExecutionContext, _permit: EngineWorkPermit| {
            context.progress().increment(1)?;
            Ok(73)
        },
    )?;
    dispatch(
        &mut dispatcher,
        "submit-pausable-export",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(prerequisite, [])?),
    )?;
    dispatch(
        &mut dispatcher,
        "submit-dependent-export",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(dependent, [prerequisite])?),
    )?;
    poll_until(
        &mut dispatcher,
        prerequisite,
        ExportJobStatus::Running,
        "poll-running-export",
    )?;
    dispatch(
        &mut dispatcher,
        "pause-export",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::Pause(prerequisite)),
    )?;
    let paused = poll_until(
        &mut dispatcher,
        prerequisite,
        ExportJobStatus::Paused,
        "poll-paused-export",
    )?;
    assert_eq!(
        paused.job(dependent).unwrap().status(),
        ExportJobStatus::Waiting
    );
    dispatch(
        &mut dispatcher,
        "resume-export",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::Resume(prerequisite)),
    )?;
    let completed = poll_until(
        &mut dispatcher,
        dependent,
        ExportJobStatus::Completed,
        "poll-dependent-export",
    )?;
    assert_eq!(
        completed.job(prerequisite).unwrap().status(),
        ExportJobStatus::Completed
    );
    assert_eq!(*runtime.result(prerequisite)?.unwrap(), 72);
    assert_eq!(*runtime.result(dependent)?.unwrap(), 73);

    let cancelled = JobId::from_raw(722);
    let cancelled_all = JobId::from_raw(723);
    runtime.prepare_executor(
        cancelled,
        |context: &ExportJobExecutionContext, _permit: EngineWorkPermit| loop {
            context.check("wait_for_dispatched_cancel")?;
            thread::yield_now();
        },
    )?;
    runtime.prepare_executor(
        cancelled_all,
        |context: &ExportJobExecutionContext, _permit: EngineWorkPermit| loop {
            context.check("wait_for_dispatched_cancel_all")?;
            thread::yield_now();
        },
    )?;
    dispatch(
        &mut dispatcher,
        "submit-cancelled-export",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(cancelled, [])?),
    )?;
    dispatch(
        &mut dispatcher,
        "submit-cancelled-all-export",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(cancelled_all, [])?),
    )?;
    poll_until(
        &mut dispatcher,
        cancelled,
        ExportJobStatus::Running,
        "poll-cancellable-export",
    )?;
    let shutdown = lifecycle_state(
        dispatch(
            &mut dispatcher,
            "begin-shutdown-with-active-export",
            EngineCommand::BeginShutdown,
        )?
        .result(),
    );
    let export_teardown = shutdown.pending_action().unwrap();
    assert_eq!(export_teardown.subsystem(), EngineSubsystem::Export);
    assert_eq!(export_teardown.kind(), EngineLifecycleActionKind::Teardown);
    let premature_teardown = dispatch(
        &mut dispatcher,
        "complete-active-export-teardown",
        EngineCommand::CompleteLifecycleAction(export_teardown),
    )
    .unwrap_err();
    assert_eq!(premature_teardown.category(), ErrorCategory::Conflict);
    dispatch(
        &mut dispatcher,
        "cancel-export",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::Cancel(cancelled)),
    )?;
    dispatch(
        &mut dispatcher,
        "cancel-all-exports",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::CancelAll),
    )?;
    let cancelled_state = poll_until(
        &mut dispatcher,
        cancelled,
        ExportJobStatus::Cancelled,
        "poll-cancelled-export",
    )?;
    assert!(cancelled_state.job(cancelled).unwrap().is_final());
    let cancelled_state = poll_until(
        &mut dispatcher,
        cancelled_all,
        ExportJobStatus::Cancelled,
        "poll-cancelled-all-export",
    )?;
    assert_eq!(
        cancelled_state.job(cancelled_all).unwrap().status(),
        ExportJobStatus::Cancelled
    );
    dispatch(
        &mut dispatcher,
        "complete-final-export-teardown",
        EngineCommand::CompleteLifecycleAction(export_teardown),
    )?;

    let inspected = dispatch(
        &mut dispatcher,
        "inspect-all-exports",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::InspectAll),
    )?;
    assert_eq!(export_state(inspected.result()).jobs().len(), 4);

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

fn poll_until(
    dispatcher: &mut EngineCommandDispatcher,
    job_id: JobId,
    expected: ExportJobStatus,
    transaction_prefix: &str,
) -> Result<EngineExportJobState> {
    let deadline = Instant::now() + Duration::from_secs(2);
    for poll in 0_u64.. {
        let outcome = dispatch(
            dispatcher,
            &format!("{transaction_prefix}-{poll}"),
            EngineCommand::ExecuteExportJob(EngineExportJobCommand::Poll),
        )?;
        let state = export_state(outcome.result()).clone();
        if state.job(job_id).unwrap().status() == expected {
            return Ok(state);
        }
        assert!(Instant::now() < deadline, "export job did not settle");
        thread::yield_now();
    }
    unreachable!()
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

fn export_state(result: &EngineCommandResult) -> &EngineExportJobState {
    match result {
        EngineCommandResult::ExportJobs(state) => state,
        _ => panic!("expected export state result"),
    }
}

fn lifecycle_state(result: &EngineCommandResult) -> EngineLifecycleSnapshot {
    match result {
        EngineCommandResult::Lifecycle(state) => state.as_ref().clone(),
        _ => panic!("expected lifecycle result"),
    }
}
