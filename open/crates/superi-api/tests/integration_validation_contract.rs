use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Recoverability, Result};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineReportedFailure, EngineTransactionId,
};
use superi_engine::introspection::MediaCapabilities;
use superi_engine::lifecycle::EngineSubsystem as InternalSubsystem;
use superi_engine::media::media_backend_registry;

use superi_api::api::{EngineLifecyclePhase, EngineSubsystem, EngineWorkflow};

use superi_api::commands::{
    ApiCommand, GetEngineIntegrationValidation, GetEngineIntegrationValidationResult,
};
use superi_api::validation::{
    IntegrationCondition, IntegrationLifecycleActionKind, IntegrationValidationApi,
};
use superi_api::version::{
    ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION, GET_ENGINE_INTEGRATION_VALIDATION_METHOD,
};

#[test]
fn public_validation_query_is_strict_versioned_and_projects_exact_action_state() -> Result<()> {
    let _domain = ExecutionDomain::EngineControl.enter_current()?;
    let dispatcher = EngineCommandDispatcher::new()?;
    let api = validation_api(&dispatcher)?;

    assert_eq!(
        GetEngineIntegrationValidation::METHOD,
        "superi.engine.integration.validation.get"
    );
    assert_eq!(
        GetEngineIntegrationValidation::METHOD,
        GET_ENGINE_INTEGRATION_VALIDATION_METHOD
    );
    assert_eq!(
        ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION.to_string(),
        "1.0.0"
    );

    let result = api.execute(GetEngineIntegrationValidation::new());
    let snapshot = result.snapshot();
    assert_eq!(snapshot.condition(), IntegrationCondition::Starting);
    assert!(snapshot.is_coherent());
    assert_eq!(snapshot.schema_version().to_string(), "1.0.0");
    assert_eq!(snapshot.engine().phase(), EngineLifecyclePhase::Starting);
    assert_eq!(snapshot.scenario().revision(), 0);
    assert_eq!(snapshot.scenario().phase(), "empty");
    assert_eq!(snapshot.scenario().operation_count(), 0);
    assert!(!snapshot.scenario().can_undo());
    assert!(!snapshot.scenario().can_redo());
    assert_eq!(
        snapshot
            .engine()
            .subsystems()
            .iter()
            .map(|state| state.subsystem())
            .collect::<Vec<_>>(),
        [
            EngineSubsystem::SharedState,
            EngineSubsystem::Projects,
            EngineSubsystem::Devices,
            EngineSubsystem::Playback,
            EngineSubsystem::Rendering,
            EngineSubsystem::Export,
        ]
    );
    let action = snapshot
        .pending_action()
        .expect("startup exposes its exact pending action");
    assert_eq!(action.subsystem(), EngineSubsystem::SharedState);
    assert_eq!(action.kind().code(), "initialize");
    assert_eq!(action.lifetime(), snapshot.engine().lifetime());
    assert!(snapshot
        .workflows()
        .iter()
        .all(|workflow| !workflow.is_available()));
    assert!(!snapshot.playback().is_attached());
    assert!(snapshot.playback().latest().is_none());
    assert!(snapshot.playback().latest_failure().is_none());
    assert!(!snapshot.export().is_attached());
    assert!(snapshot.export().latest().is_none());
    assert!(snapshot.findings().is_empty());

    let wire = serde_json::to_value(&result).unwrap();
    assert_eq!(wire["snapshot"]["condition"], "starting");
    assert_eq!(wire["snapshot"]["pending_action"]["kind"], "initialize");
    let decoded: GetEngineIntegrationValidationResult =
        serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(decoded, result);

    let mut unknown_result = wire;
    unknown_result["snapshot"]["unexpected"] = serde_json::json!(true);
    assert!(
        serde_json::from_value::<GetEngineIntegrationValidationResult>(unknown_result).is_err()
    );
    assert!(
        serde_json::from_value::<GetEngineIntegrationValidation>(serde_json::json!({
            "unexpected": true
        }))
        .is_err()
    );
    Ok(())
}

#[test]
fn public_validation_exposes_precise_sleep_and_wake_actions() -> Result<()> {
    let _domain = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = running_dispatcher()?;
    dispatcher.drain_events()?;

    let mut lifecycle = match dispatch(
        &mut dispatcher,
        "begin-public-sleep",
        EngineCommand::BeginSleep,
    )?
    .result()
    {
        EngineCommandResult::Lifecycle(state) => state.as_ref().clone(),
        other => panic!("unexpected sleep result: {other:?}"),
    };
    let preparing = validation_api(&dispatcher)?.execute(GetEngineIntegrationValidation::new());
    assert_eq!(
        preparing.snapshot().condition(),
        IntegrationCondition::PreparingSleep
    );
    assert_eq!(
        preparing.snapshot().pending_action().unwrap().kind(),
        IntegrationLifecycleActionKind::PrepareSleep
    );
    assert!(preparing
        .snapshot()
        .workflows()
        .iter()
        .all(|workflow| !workflow.is_available()));

    while lifecycle.phase() == LifecyclePhase::PreparingSleep {
        lifecycle = match dispatch(
            &mut dispatcher,
            "complete-public-sleep",
            EngineCommand::CompleteLifecycleAction(lifecycle.pending_action().unwrap()),
        )?
        .result()
        {
            EngineCommandResult::Lifecycle(state) => state.as_ref().clone(),
            other => panic!("unexpected sleep completion: {other:?}"),
        };
    }
    let sleeping = validation_api(&dispatcher)?.execute(GetEngineIntegrationValidation::new());
    assert_eq!(
        sleeping.snapshot().condition(),
        IntegrationCondition::Sleeping
    );
    assert_eq!(
        sleeping.snapshot().engine().phase(),
        EngineLifecyclePhase::Sleeping
    );

    lifecycle = match dispatch(
        &mut dispatcher,
        "begin-public-wake",
        EngineCommand::BeginWake,
    )?
    .result()
    {
        EngineCommandResult::Lifecycle(state) => state.as_ref().clone(),
        other => panic!("unexpected wake result: {other:?}"),
    };
    let waking = validation_api(&dispatcher)?.execute(GetEngineIntegrationValidation::new());
    assert_eq!(waking.snapshot().condition(), IntegrationCondition::Waking);
    assert_eq!(
        waking.snapshot().pending_action().unwrap().kind(),
        IntegrationLifecycleActionKind::Wake
    );
    while lifecycle.phase() == LifecyclePhase::Waking {
        lifecycle = match dispatch(
            &mut dispatcher,
            "complete-public-wake",
            EngineCommand::CompleteLifecycleAction(lifecycle.pending_action().unwrap()),
        )?
        .result()
        {
            EngineCommandResult::Lifecycle(state) => state.as_ref().clone(),
            other => panic!("unexpected wake completion: {other:?}"),
        };
    }
    assert_eq!(lifecycle.phase(), LifecyclePhase::Running);
    assert_eq!(
        validation_api(&dispatcher)?
            .execute(GetEngineIntegrationValidation::new())
            .snapshot()
            .condition(),
        IntegrationCondition::Normal
    );
    Ok(())
}

#[test]
fn public_validation_preserves_degraded_and_recovery_evidence() -> Result<()> {
    let _domain = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = running_dispatcher()?;
    dispatcher.drain_events()?;

    dispatch(
        &mut dispatcher,
        "degrade-rendering",
        EngineCommand::ReportRuntimeFailure {
            subsystem: InternalSubsystem::Rendering,
            failure: EngineReportedFailure::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Degraded,
                "render memory is constrained",
            )?,
        },
    )?;
    dispatcher.drain_events()?;
    let degraded = validation_api(&dispatcher)?.execute(GetEngineIntegrationValidation::new());
    assert_eq!(
        degraded.snapshot().condition(),
        IntegrationCondition::Degraded
    );
    assert_eq!(degraded.snapshot().engine().active_failures().len(), 1);
    assert_eq!(
        degraded.snapshot().engine().active_failures()[0].subsystem(),
        EngineSubsystem::Rendering
    );
    assert!(degraded
        .snapshot()
        .workflow(EngineWorkflow::Playback)
        .unwrap()
        .is_available());
    assert!(!degraded
        .snapshot()
        .workflow(EngineWorkflow::Rendering)
        .unwrap()
        .is_available());
    assert!(!degraded
        .snapshot()
        .workflow(EngineWorkflow::Export)
        .unwrap()
        .is_available());

    let started = dispatch(
        &mut dispatcher,
        "begin-rendering-recovery",
        EngineCommand::BeginRecovery(InternalSubsystem::Rendering),
    )?;
    let lifecycle_action = match started.result() {
        EngineCommandResult::Lifecycle(state) => state.pending_action().unwrap(),
        other => panic!("unexpected recovery result: {other:?}"),
    };
    dispatcher.drain_events()?;
    let recovering = validation_api(&dispatcher)?.execute(GetEngineIntegrationValidation::new());
    assert_eq!(
        recovering.snapshot().condition(),
        IntegrationCondition::Recovering
    );
    let recovery = recovering
        .snapshot()
        .recovery()
        .pending_recovery()
        .expect("recovery exposes its exact token");
    assert_eq!(recovery.subsystem(), EngineSubsystem::Rendering);
    assert!(recovery.sequence() > 0);
    assert_eq!(recovery.attempt(), 1);
    assert!(recovering.snapshot().recovery().diagnostic_event_count() > 0);
    let denial = recovering
        .snapshot()
        .workflow(EngineWorkflow::Rendering)
        .unwrap()
        .denial()
        .expect("recovery keeps exact workflow denial evidence");
    let failure = denial
        .failure()
        .expect("recovery denial keeps classified failure evidence");
    assert_eq!(failure.category(), "resource_exhausted");
    assert_eq!(failure.recoverability(), "degraded");
    assert_eq!(
        recovering.snapshot().pending_action().unwrap().revision(),
        lifecycle_action.revision()
    );
    Ok(())
}

fn validation_api(dispatcher: &EngineCommandDispatcher) -> Result<IntegrationValidationApi> {
    let capabilities = MediaCapabilities::from_registry(&media_backend_registry()?)?;
    let snapshot = dispatcher.integration_validation_snapshot(&capabilities, None)?;
    Ok(IntegrationValidationApi::new(&snapshot))
}

fn running_dispatcher() -> Result<EngineCommandDispatcher> {
    let mut dispatcher = EngineCommandDispatcher::new()?;
    loop {
        let state = match dispatch(
            &mut dispatcher,
            "inspect-startup",
            EngineCommand::InspectLifecycle,
        )?
        .result()
        {
            EngineCommandResult::Lifecycle(state) => state.clone(),
            other => panic!("unexpected startup result: {other:?}"),
        };
        if state.phase() == LifecyclePhase::Running {
            return Ok(dispatcher);
        }
        dispatch(
            &mut dispatcher,
            "complete-startup",
            EngineCommand::CompleteLifecycleAction(state.pending_action().unwrap()),
        )?;
    }
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
