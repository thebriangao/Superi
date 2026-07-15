use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Recoverability, Result};
use superi_core::time::FrameRate;
use superi_engine::command::{
    GraphEffect, ScenarioAction, ScenarioPhase, CANONICAL_FIXTURE_ID, CANONICAL_FIXTURE_VERSION,
    CANONICAL_HEIGHT, CANONICAL_SOURCE_FRAMES, CANONICAL_WIDTH,
};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineReportedFailure, EngineTransactionId, ScenarioTransaction,
};
use superi_engine::export_dispatch::EngineExportJobCommand;
use superi_engine::export_jobs::ExportJobQueueConfig;
use superi_engine::introspection::{EngineLifecyclePhase, MediaCapabilities};
use superi_engine::lifecycle::{
    EngineLifecycleActionKind, EngineSubsystem, EngineSubsystemState, EngineWorkKind,
};
use superi_engine::media::media_backend_registry;
use superi_engine::validation::{EngineIntegrationCondition, EngineIntegrationValidationSnapshot};

#[test]
fn validation_exposes_coherent_normal_degraded_recovery_and_recovered_state() -> Result<()> {
    let _domain = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;

    let starting = inspect(&dispatcher)?;
    assert_eq!(starting.condition(), EngineIntegrationCondition::Starting);
    assert!(starting.is_coherent());
    assert!(starting.findings().is_empty());
    assert_eq!(starting.lifecycle().phase(), LifecyclePhase::Starting);
    assert_eq!(
        starting.introspection().phase(),
        EngineLifecyclePhase::Starting
    );
    assert!(!starting.introspection().media_capabilities().is_empty());
    assert!(starting.lifecycle().pending_action().is_some());
    assert!(starting.scenario().undo_depth() == 0 && starting.scenario().redo_depth() == 0);
    for work in [
        EngineWorkKind::Playback,
        EngineWorkKind::Rendering,
        EngineWorkKind::Export,
    ] {
        assert!(starting
            .workflow(work)
            .unwrap()
            .admission()
            .permit()
            .is_none());
    }
    assert!(!starting.playback().is_attached());
    assert!(!starting.playback().command_pending());
    assert!(!starting.export().is_attached());
    assert!(dispatcher.drain_events()?.is_empty());

    start_engine(&mut dispatcher)?;
    dispatcher.drain_events()?;
    let normal = inspect(&dispatcher)?;
    assert_eq!(normal.condition(), EngineIntegrationCondition::Normal);
    assert!(normal.is_coherent());
    for work in [
        EngineWorkKind::Playback,
        EngineWorkKind::Rendering,
        EngineWorkKind::Export,
    ] {
        let permit = normal
            .workflow(work)
            .unwrap()
            .admission()
            .permit()
            .expect("healthy running work has an exact permit");
        assert_eq!(permit.work(), work);
        assert_eq!(permit.lifetime(), normal.lifecycle().lifetime());
        assert_eq!(permit.state_revision(), normal.lifecycle().state_revision());
    }
    assert!(dispatcher.drain_events()?.is_empty());

    let mut power = match dispatch(
        &mut dispatcher,
        "begin-validation-sleep",
        EngineCommand::BeginSleep,
    )?
    .result()
    {
        EngineCommandResult::Lifecycle(state) => state.as_ref().clone(),
        other => panic!("unexpected sleep result: {other:?}"),
    };
    let preparing_sleep = inspect(&dispatcher)?;
    assert_eq!(
        preparing_sleep.condition(),
        EngineIntegrationCondition::PreparingSleep
    );
    assert_eq!(
        preparing_sleep.lifecycle().pending_action().unwrap().kind(),
        EngineLifecycleActionKind::PrepareSleep
    );
    assert!(preparing_sleep
        .workflows()
        .iter()
        .all(|workflow| workflow.admission().permit().is_none()));
    while power.phase() == LifecyclePhase::PreparingSleep {
        power = match dispatch(
            &mut dispatcher,
            "complete-validation-sleep",
            EngineCommand::CompleteLifecycleAction(power.pending_action().unwrap()),
        )?
        .result()
        {
            EngineCommandResult::Lifecycle(state) => state.as_ref().clone(),
            other => panic!("unexpected sleep completion: {other:?}"),
        };
    }
    let sleeping = inspect(&dispatcher)?;
    assert_eq!(sleeping.condition(), EngineIntegrationCondition::Sleeping);
    assert_eq!(
        sleeping
            .introspection()
            .subsystem(EngineSubsystem::Projects)
            .unwrap()
            .state(),
        EngineSubsystemState::Sleeping
    );

    power = match dispatch(
        &mut dispatcher,
        "begin-validation-wake",
        EngineCommand::BeginWake,
    )?
    .result()
    {
        EngineCommandResult::Lifecycle(state) => state.as_ref().clone(),
        other => panic!("unexpected wake result: {other:?}"),
    };
    let waking = inspect(&dispatcher)?;
    assert_eq!(waking.condition(), EngineIntegrationCondition::Waking);
    assert_eq!(
        waking.lifecycle().pending_action().unwrap().kind(),
        EngineLifecycleActionKind::Wake
    );
    while power.phase() == LifecyclePhase::Waking {
        power = match dispatch(
            &mut dispatcher,
            "complete-validation-wake",
            EngineCommand::CompleteLifecycleAction(power.pending_action().unwrap()),
        )?
        .result()
        {
            EngineCommandResult::Lifecycle(state) => state.as_ref().clone(),
            other => panic!("unexpected wake completion: {other:?}"),
        };
    }
    assert_eq!(power.phase(), LifecyclePhase::Running);
    assert_eq!(
        inspect(&dispatcher)?.condition(),
        EngineIntegrationCondition::Normal
    );
    dispatcher.drain_events()?;

    dispatch(
        &mut dispatcher,
        "degrade-playback",
        EngineCommand::ReportRuntimeFailure {
            subsystem: EngineSubsystem::Playback,
            failure: EngineReportedFailure::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "playback device is rebuilding",
            )?,
        },
    )?;
    dispatcher.drain_events()?;
    let degraded = inspect(&dispatcher)?;
    assert_eq!(degraded.condition(), EngineIntegrationCondition::Degraded);
    assert!(degraded.is_coherent());
    assert!(degraded
        .workflow(EngineWorkKind::Playback)
        .unwrap()
        .admission()
        .permit()
        .is_none());
    assert!(degraded
        .workflow(EngineWorkKind::Rendering)
        .unwrap()
        .admission()
        .permit()
        .is_some());
    assert!(degraded
        .workflow(EngineWorkKind::Export)
        .unwrap()
        .admission()
        .permit()
        .is_some());
    assert_eq!(degraded.recovery().active_failures().len(), 1);

    let recovering = dispatch(
        &mut dispatcher,
        "begin-playback-recovery",
        EngineCommand::BeginRecovery(EngineSubsystem::Playback),
    )?;
    let action = match recovering.result() {
        EngineCommandResult::Lifecycle(state) => state.pending_action().unwrap(),
        other => panic!("unexpected recovery result: {other:?}"),
    };
    dispatcher.drain_events()?;
    let recovery = inspect(&dispatcher)?;
    assert_eq!(recovery.condition(), EngineIntegrationCondition::Recovering);
    assert!(recovery.is_coherent());
    assert_eq!(recovery.lifecycle().pending_action(), Some(action));
    assert!(recovery.recovery().pending_recovery().is_some());

    dispatch(
        &mut dispatcher,
        "complete-playback-recovery",
        EngineCommand::CompleteLifecycleAction(action),
    )?;
    dispatcher.drain_events()?;
    let recovered = inspect(&dispatcher)?;
    assert_eq!(recovered.condition(), EngineIntegrationCondition::Normal);
    assert!(recovered.recovery().active_failures().is_empty());
    assert!(recovered.recovery().pending_recovery().is_none());

    dispatch(
        &mut dispatcher,
        "degrade-rendering",
        EngineCommand::ReportRuntimeFailure {
            subsystem: EngineSubsystem::Rendering,
            failure: EngineReportedFailure::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Degraded,
                "render device memory is constrained",
            )?,
        },
    )?;
    dispatcher.drain_events()?;
    let rendering_degraded = inspect(&dispatcher)?;
    assert_eq!(
        rendering_degraded.condition(),
        EngineIntegrationCondition::Degraded
    );
    assert!(rendering_degraded
        .workflow(EngineWorkKind::Playback)
        .unwrap()
        .admission()
        .permit()
        .is_some());
    for work in [EngineWorkKind::Rendering, EngineWorkKind::Export] {
        let denial = rendering_degraded
            .workflow(work)
            .unwrap()
            .admission()
            .denial()
            .expect("rendering degradation blocks dependent work");
        assert_eq!(
            denial.blocking_subsystem(),
            Some(EngineSubsystem::Rendering)
        );
    }
    assert!(dispatcher.drain_events()?.is_empty());
    Ok(())
}

#[test]
fn validation_uses_cached_export_replacement_state_without_polling_the_runtime() -> Result<()> {
    let domain = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let runtime = dispatcher.attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 1, 2)?)?;

    let attached = inspect(&dispatcher)?;
    assert!(attached.export().is_attached());
    assert!(attached.export().latest_state().is_none());

    let observed = dispatch(
        &mut dispatcher,
        "inspect-empty-export-queue",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::InspectAll),
    )?;
    let state = match observed.result() {
        EngineCommandResult::ExportJobs(state) => state.clone(),
        other => panic!("unexpected export result: {other:?}"),
    };
    assert_eq!(state.revision(), 0);
    assert!(state.jobs().is_empty());

    let validation = inspect(&dispatcher)?;
    assert_eq!(validation.export().latest_state(), Some(state.as_ref()));
    assert!(dispatcher.drain_events()?.is_empty());

    drop(domain);
    runtime.shutdown()?;
    Ok(())
}

#[test]
fn validation_tracks_exact_scenario_reversal_capacity() -> Result<()> {
    let _domain = ExecutionDomain::EngineControl.enter_current()?;
    let directory = test_directory("reversal");
    let source = directory.join("input.webm");
    let source_bytes = b"integration validation reversal fixture";
    fs::write(&source, source_bytes).unwrap();
    let mut dispatcher = EngineCommandDispatcher::new()?;

    dispatch(
        &mut dispatcher,
        "validation-scenario-commit",
        EngineCommand::ExecuteScenario(ScenarioTransaction::new(
            0,
            vec![
                import_action(source, source_bytes),
                ScenarioAction::PlaceClip {
                    timeline_start_frame: 0,
                },
                ScenarioAction::TrimClip {
                    source_start_frame: 24,
                    source_end_frame: 72,
                },
                ScenarioAction::ApplyGraphEffect {
                    effect: GraphEffect::HorizontalMirror,
                },
            ],
        )?),
    )?;
    let committed = inspect(&dispatcher)?;
    assert_eq!(committed.scenario().revision(), 1);
    assert_eq!(committed.scenario().phase(), ScenarioPhase::Effected);
    assert_eq!(committed.scenario().operation_log().len(), 4);
    assert_eq!(committed.scenario().undo_depth(), 1);
    assert_eq!(committed.scenario().redo_depth(), 0);

    dispatch(
        &mut dispatcher,
        "validation-scenario-undo",
        EngineCommand::ExecuteScenario(ScenarioTransaction::new(1, vec![ScenarioAction::Undo])?),
    )?;
    let undone = inspect(&dispatcher)?;
    assert_eq!(undone.scenario().revision(), 2);
    assert_eq!(undone.scenario().phase(), ScenarioPhase::Empty);
    assert_eq!(undone.scenario().undo_depth(), 0);
    assert_eq!(undone.scenario().redo_depth(), 1);

    dispatch(
        &mut dispatcher,
        "validation-scenario-redo",
        EngineCommand::ExecuteScenario(ScenarioTransaction::new(2, vec![ScenarioAction::Redo])?),
    )?;
    let redone = inspect(&dispatcher)?;
    assert_eq!(redone.scenario().revision(), 3);
    assert_eq!(redone.scenario().phase(), ScenarioPhase::Effected);
    assert_eq!(redone.scenario().undo_depth(), 1);
    assert_eq!(redone.scenario().redo_depth(), 0);
    assert_eq!(dispatcher.drain_events()?.len(), 3);

    fs::remove_dir_all(directory).unwrap();
    Ok(())
}

fn start_engine(dispatcher: &mut EngineCommandDispatcher) -> Result<()> {
    loop {
        let state = match dispatch(
            dispatcher,
            "inspect-startup-lifecycle",
            EngineCommand::InspectLifecycle,
        )?
        .result()
        {
            EngineCommandResult::Lifecycle(state) => state.clone(),
            other => panic!("unexpected startup result: {other:?}"),
        };
        if state.phase() == LifecyclePhase::Running {
            return Ok(());
        }
        dispatch(
            dispatcher,
            &format!(
                "complete-startup-{}",
                state.pending_action().unwrap().subsystem().code()
            ),
            EngineCommand::CompleteLifecycleAction(state.pending_action().unwrap()),
        )?;
    }
}

fn inspect(dispatcher: &EngineCommandDispatcher) -> Result<EngineIntegrationValidationSnapshot> {
    let capabilities = MediaCapabilities::from_registry(&media_backend_registry()?)?;
    dispatcher.integration_validation_snapshot(&capabilities, None)
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

fn import_action(path: PathBuf, payload: &[u8]) -> ScenarioAction {
    ScenarioAction::ImportClip {
        path,
        fixture_id: CANONICAL_FIXTURE_ID.to_owned(),
        fixture_version: CANONICAL_FIXTURE_VERSION,
        manifest_sha256: "1d2b28b5f44c7f86dce50d67b718b0fad967d267d9016961e3d71bb9dab94419"
            .to_owned(),
        payload_sha256: format!("{:x}", Sha256::digest(payload)),
        frame_rate: FrameRate::FPS_24,
        frame_count: CANONICAL_SOURCE_FRAMES,
        width: CANONICAL_WIDTH,
        height: CANONICAL_HEIGHT,
    }
}

fn test_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "superi-p2-w06-c014-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
