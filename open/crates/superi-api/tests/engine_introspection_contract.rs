use superi_api::api::{
    EngineFailureDisposition, EngineHealth, EngineIntrospectionApi, EngineLifecyclePhase,
    EngineRecoveryRequest, EngineResourceClass, EngineSubsystem, EngineWorkflow,
};
use superi_api::commands::{ApiCommand, GetEngineIntrospection, GetEngineIntrospectionResult};
use superi_api::events::{ApiEvent, EngineIntrospectionChanged};
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, ErrorContext, Recoverability};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineReportedFailure, EngineTransactionId,
};
use superi_engine::introspection::MediaCapabilities;
use superi_engine::lifecycle::EngineSubsystem as InternalSubsystem;
use superi_engine::media::media_backend_registry;
use superi_engine::resource_arbitration::{
    EngineResourceArbiter, ResourceAdmission, ResourceArbitrationConfig, ResourceClass,
    ResourceClassBudget, ResourceConsumer, ResourceRequest,
};
use superi_media_io::backend::BackendRegistry;

#[test]
fn public_query_and_event_track_capabilities_resources_health_and_recovery() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    let capabilities =
        MediaCapabilities::from_registry(&media_backend_registry().unwrap()).unwrap();
    let arbiter = EngineResourceArbiter::new(resource_config());
    let initial = dispatcher
        .introspection_snapshot(&capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    let mut api = EngineIntrospectionApi::new(&initial);

    assert_eq!(
        GetEngineIntrospection::METHOD,
        "superi.engine.introspection.get"
    );
    assert_eq!(
        EngineIntrospectionChanged::NAME,
        "superi.engine.introspection.changed"
    );
    let initial_result = api.execute(GetEngineIntrospection::new());
    assert_eq!(
        initial_result.snapshot().schema_version().to_string(),
        "1.0.0"
    );
    assert_eq!(initial_result.snapshot().revision(), 0);
    assert_eq!(
        initial_result.snapshot().phase(),
        EngineLifecyclePhase::Starting
    );
    assert_eq!(initial_result.snapshot().health(), EngineHealth::Healthy);
    assert!(initial_result
        .snapshot()
        .workflows()
        .iter()
        .all(|workflow| !workflow.available()));
    assert!(!initial_result
        .snapshot()
        .media_capabilities()
        .backends()
        .is_empty());
    assert_eq!(
        initial_result
            .snapshot()
            .resources()
            .unwrap()
            .classes()
            .iter()
            .map(|class| class.class())
            .collect::<Vec<_>>(),
        [
            EngineResourceClass::DecodeBuffer,
            EngineResourceClass::GpuMemory,
            EngineResourceClass::Cache,
            EngineResourceClass::Audio,
            EngineResourceClass::Ai,
            EngineResourceClass::Export,
        ]
    );
    assert!(api.synchronize(&initial).unwrap().is_none());

    drive_running(&mut dispatcher);
    dispatcher.drain_events().unwrap();
    let running = dispatcher
        .introspection_snapshot(&capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    let running_event = api.synchronize(&running).unwrap().unwrap();
    assert_eq!(running_event.snapshot().revision(), 1);
    assert_eq!(
        running_event.snapshot().phase(),
        EngineLifecyclePhase::Running
    );
    assert!(running_event
        .snapshot()
        .workflows()
        .iter()
        .all(|workflow| workflow.available()));
    assert_eq!(running_event.snapshot().media_capabilities().revision(), 0);
    assert_eq!(
        api.execute(GetEngineIntrospection::new()).snapshot(),
        running_event.snapshot()
    );

    let reservation = match arbiter
        .reserve(
            ResourceRequest::new(ResourceClass::Cache, ResourceConsumer::Background, 128).unwrap(),
        )
        .unwrap()
    {
        ResourceAdmission::Granted(grant) => grant.into_reservation(),
        ResourceAdmission::Degraded(degradation) => {
            panic!("empty resource envelope must admit cache state: {degradation:?}")
        }
    };
    let resource_changed = dispatcher
        .introspection_snapshot(&capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    let resource_event = api.synchronize(&resource_changed).unwrap().unwrap();
    assert_eq!(resource_event.snapshot().revision(), 2);
    assert_eq!(resource_event.snapshot().media_capabilities().revision(), 0);
    let resource_state = resource_event.snapshot().resources().unwrap();
    assert_eq!(resource_state.revision(), 1);
    assert_eq!(resource_state.total_used_bytes(), 128);
    assert_eq!(
        resource_state
            .class(EngineResourceClass::Cache)
            .unwrap()
            .used_bytes(),
        128
    );

    let failure = EngineReportedFailure::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "private device /Users/example/secret failed",
    )
    .unwrap()
    .with_context(
        ErrorContext::new("api.introspection-test", "open_private_device")
            .with_field("path", "/Users/example/secret"),
    )
    .unwrap();
    dispatch(
        &mut dispatcher,
        "degrade-playback",
        EngineCommand::ReportRuntimeFailure {
            subsystem: InternalSubsystem::Playback,
            failure,
        },
    )
    .unwrap();
    dispatcher.drain_events().unwrap();
    let degraded = dispatcher
        .introspection_snapshot(&capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    let degraded_event = api.synchronize(&degraded).unwrap().unwrap();
    assert_eq!(degraded_event.snapshot().revision(), 3);
    assert_eq!(degraded_event.snapshot().health(), EngineHealth::Degraded);
    assert!(!degraded_event
        .snapshot()
        .workflow(EngineWorkflow::Playback)
        .unwrap()
        .available());
    assert!(degraded_event
        .snapshot()
        .workflow(EngineWorkflow::Rendering)
        .unwrap()
        .available());
    assert!(degraded_event
        .snapshot()
        .workflow(EngineWorkflow::Export)
        .unwrap()
        .available());
    let public_failure = degraded_event
        .snapshot()
        .active_failure(EngineSubsystem::Playback)
        .unwrap();
    assert_eq!(
        public_failure.disposition(),
        EngineFailureDisposition::ContinueDegraded
    );
    assert_eq!(
        public_failure.recovery_request(),
        Some(EngineRecoveryRequest::RestoreSubsystem)
    );
    assert_eq!(public_failure.error().category(), "unavailable");
    assert_eq!(public_failure.error().recoverability(), "degraded");
    assert_eq!(public_failure.error().code(), "error.unavailable.degraded");
    assert!(!public_failure.recovery_in_progress());
    let degraded_json = serde_json::to_string(&degraded_event).unwrap();
    assert!(!degraded_json.contains("/Users/example/secret"));
    assert!(!degraded_json.contains("open_private_device"));

    let recovering = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "recover-playback",
            EngineCommand::BeginRecovery(InternalSubsystem::Playback),
        )
        .unwrap()
        .result(),
    )
    .clone();
    dispatcher.drain_events().unwrap();
    let recovering_state = dispatcher
        .introspection_snapshot(&capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    let recovering_event = api.synchronize(&recovering_state).unwrap().unwrap();
    assert_eq!(recovering_event.snapshot().revision(), 4);
    assert!(recovering_event
        .snapshot()
        .active_failure(EngineSubsystem::Playback)
        .unwrap()
        .recovery_in_progress());

    dispatch(
        &mut dispatcher,
        "complete-playback-recovery",
        EngineCommand::CompleteLifecycleAction(recovering.pending_action().unwrap()),
    )
    .unwrap();
    dispatcher.drain_events().unwrap();
    let restored = dispatcher
        .introspection_snapshot(&capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    let restored_event = api.synchronize(&restored).unwrap().unwrap();
    assert_eq!(restored_event.snapshot().revision(), 5);
    assert_eq!(restored_event.snapshot().health(), EngineHealth::Healthy);
    assert!(restored_event.snapshot().active_failures().is_empty());
    assert!(restored_event
        .snapshot()
        .workflows()
        .iter()
        .all(|workflow| workflow.available()));

    dispatch(
        &mut dispatcher,
        "degrade-rendering",
        EngineCommand::ReportRuntimeFailure {
            subsystem: InternalSubsystem::Rendering,
            failure: EngineReportedFailure::new(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "render device is temporarily unavailable",
            )
            .unwrap(),
        },
    )
    .unwrap();
    dispatcher.drain_events().unwrap();
    let rendering_degraded = dispatcher
        .introspection_snapshot(&capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    let rendering_event = api.synchronize(&rendering_degraded).unwrap().unwrap();
    assert_eq!(rendering_event.snapshot().revision(), 6);
    assert!(rendering_event
        .snapshot()
        .workflow(EngineWorkflow::Playback)
        .unwrap()
        .available());
    assert!(!rendering_event
        .snapshot()
        .workflow(EngineWorkflow::Rendering)
        .unwrap()
        .available());
    assert!(!rendering_event
        .snapshot()
        .workflow(EngineWorkflow::Export)
        .unwrap()
        .available());

    let rendering_recovering = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "recover-rendering",
            EngineCommand::BeginRecovery(InternalSubsystem::Rendering),
        )
        .unwrap()
        .result(),
    )
    .clone();
    dispatcher.drain_events().unwrap();
    let rendering_recovery = dispatcher
        .introspection_snapshot(&capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    assert_eq!(
        api.synchronize(&rendering_recovery)
            .unwrap()
            .unwrap()
            .snapshot()
            .revision(),
        7
    );
    dispatch(
        &mut dispatcher,
        "complete-rendering-recovery",
        EngineCommand::CompleteLifecycleAction(rendering_recovering.pending_action().unwrap()),
    )
    .unwrap();
    dispatcher.drain_events().unwrap();
    let rendering_restored = dispatcher
        .introspection_snapshot(&capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    assert_eq!(
        api.synchronize(&rendering_restored)
            .unwrap()
            .unwrap()
            .snapshot()
            .revision(),
        8
    );

    let empty_capabilities = MediaCapabilities::from_registry(&BackendRegistry::new()).unwrap();
    let media_changed = dispatcher
        .introspection_snapshot(&empty_capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    let media_event = api.synchronize(&media_changed).unwrap().unwrap();
    assert_eq!(media_event.snapshot().revision(), 9);
    assert_eq!(media_event.snapshot().media_capabilities().revision(), 1);
    assert!(media_event
        .snapshot()
        .media_capabilities()
        .backends()
        .is_empty());
    assert!(api.synchronize(&media_changed).unwrap().is_none());

    let result = api.execute(GetEngineIntrospection::new());
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["snapshot"]["schema_version"], "1.0.0");
    assert_eq!(json["snapshot"]["phase"], "running");
    assert_eq!(json["snapshot"]["health"], "healthy");
    let decoded: GetEngineIntrospectionResult = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(decoded, result);
    let mut unknown = json;
    unknown["snapshot"]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<GetEngineIntrospectionResult>(unknown).is_err());
    assert_eq!(dispatcher.scenario_snapshot().unwrap().revision(), 0);
    assert!(dispatcher.drain_events().unwrap().is_empty());

    drop(reservation);
}

fn resource_config() -> ResourceArbitrationConfig {
    ResourceArbitrationConfig::new(
        1_024,
        [
            ResourceClassBudget::new(ResourceClass::DecodeBuffer, 64, 1_024).unwrap(),
            ResourceClassBudget::new(ResourceClass::GpuMemory, 64, 1_024).unwrap(),
            ResourceClassBudget::new(ResourceClass::Cache, 0, 1_024).unwrap(),
            ResourceClassBudget::new(ResourceClass::Audio, 64, 1_024).unwrap(),
            ResourceClassBudget::new(ResourceClass::Ai, 0, 1_024).unwrap(),
            ResourceClassBudget::new(ResourceClass::Export, 0, 1_024).unwrap(),
        ],
    )
    .unwrap()
}

fn drive_running(dispatcher: &mut EngineCommandDispatcher) {
    let mut lifecycle = lifecycle_result(
        dispatch(
            dispatcher,
            "inspect-startup",
            EngineCommand::InspectLifecycle,
        )
        .unwrap()
        .result(),
    )
    .clone();
    while lifecycle.phase() == LifecyclePhase::Starting {
        lifecycle = lifecycle_result(
            dispatch(
                dispatcher,
                &format!(
                    "initialize-{}",
                    lifecycle.pending_action().unwrap().subsystem()
                ),
                EngineCommand::CompleteLifecycleAction(lifecycle.pending_action().unwrap()),
            )
            .unwrap()
            .result(),
        )
        .clone();
    }
    assert_eq!(lifecycle.phase(), LifecyclePhase::Running);
}

fn dispatch(
    dispatcher: &mut EngineCommandDispatcher,
    transaction_id: &str,
    command: EngineCommand,
) -> superi_core::error::Result<superi_engine::dispatcher::EngineCommandOutcome> {
    dispatcher.dispatch(EngineCommandRequest::new(
        EngineTransactionId::new(transaction_id).unwrap(),
        command,
    ))
}

fn lifecycle_result(
    result: &EngineCommandResult,
) -> &superi_engine::lifecycle::EngineLifecycleSnapshot {
    match result {
        EngineCommandResult::Lifecycle(snapshot) => snapshot,
        _ => panic!("expected lifecycle result"),
    }
}
