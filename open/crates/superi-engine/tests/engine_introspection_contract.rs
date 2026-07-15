use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, ErrorContext, Recoverability};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineReportedFailure, EngineTransactionId,
};
use superi_engine::error::{EngineFailureDisposition, EngineRecoveryRequest};
use superi_engine::introspection::{
    EngineIntrospectionSnapshot, EngineLifecyclePhase, MediaCapabilities,
};
use superi_engine::lifecycle::{EngineHealth, EngineSubsystem, EngineWorkKind};
use superi_engine::media::media_backend_registry;
use superi_engine::resource_arbitration::{
    EngineResourceArbiter, ResourceAdmission, ResourceArbitrationConfig, ResourceClass,
    ResourceClassBudget, ResourceConsumer, ResourceRequest,
};

#[test]
fn one_read_only_snapshot_keeps_capability_health_and_workflow_state_coherent() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = EngineCommandDispatcher::new().expect("dispatcher owns lifecycle state");
    let capabilities =
        MediaCapabilities::from_registry(&media_backend_registry().unwrap()).unwrap();
    let arbiter = EngineResourceArbiter::new(resource_config());

    let initial_resources = arbiter.snapshot().unwrap();
    let initial = dispatcher
        .introspection_snapshot(&capabilities, Some(&initial_resources))
        .unwrap();
    assert_eq!(initial.phase(), EngineLifecyclePhase::Starting);
    assert_eq!(initial.health(), EngineHealth::Healthy);
    assert_eq!(initial.media_capabilities(), &capabilities);
    assert!(!initial.media_capabilities().backends().is_empty());
    assert_eq!(
        initial
            .subsystems()
            .iter()
            .map(|status| status.subsystem())
            .collect::<Vec<_>>(),
        EngineSubsystem::ALL
    );
    assert_eq!(
        initial
            .workflows()
            .iter()
            .map(|status| status.work())
            .collect::<Vec<_>>(),
        [
            EngineWorkKind::Playback,
            EngineWorkKind::Rendering,
            EngineWorkKind::Export,
        ]
    );
    assert!(initial
        .workflows()
        .iter()
        .all(|status| !status.available() && status.blocking_subsystem().is_none()));
    assert_resource_state(&initial, 0, 0, 0);
    assert!(dispatcher.drain_events().unwrap().is_empty());
    assert_eq!(dispatcher.scenario_snapshot().unwrap().revision(), 0);

    drive_running(&mut dispatcher);
    dispatcher.drain_events().unwrap();
    let running = dispatcher
        .introspection_snapshot(&capabilities, Some(&arbiter.snapshot().unwrap()))
        .unwrap();
    assert_eq!(running.phase(), EngineLifecyclePhase::Running);
    assert_eq!(running.health(), EngineHealth::Healthy);
    assert!(running.workflows().iter().all(|status| status.available()));
    assert!(running.active_failures().is_empty());

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
    let resource_snapshot = arbiter.snapshot().unwrap();
    let with_resources = dispatcher
        .introspection_snapshot(&capabilities, Some(&resource_snapshot))
        .unwrap();
    assert_resource_state(&with_resources, 1, 128, 128);
    assert_eq!(arbiter.snapshot().unwrap(), resource_snapshot);
    assert!(dispatcher.drain_events().unwrap().is_empty());

    let failure = EngineReportedFailure::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "private device path /Users/example/secret is unavailable",
    )
    .unwrap()
    .with_context(
        ErrorContext::new("engine.introspection-test", "open_private_device")
            .with_field("path", "/Users/example/secret"),
    )
    .unwrap();
    dispatch(
        &mut dispatcher,
        "degrade-playback",
        EngineCommand::ReportRuntimeFailure {
            subsystem: EngineSubsystem::Playback,
            failure,
        },
    )
    .unwrap();
    dispatcher.drain_events().unwrap();

    let degraded = dispatcher
        .introspection_snapshot(&capabilities, Some(&resource_snapshot))
        .unwrap();
    assert_eq!(degraded.health(), EngineHealth::Degraded);
    assert!(!degraded
        .workflow(EngineWorkKind::Playback)
        .unwrap()
        .available());
    assert_eq!(
        degraded
            .workflow(EngineWorkKind::Playback)
            .unwrap()
            .blocking_subsystem(),
        Some(EngineSubsystem::Playback)
    );
    assert!(degraded
        .workflow(EngineWorkKind::Rendering)
        .unwrap()
        .available());
    assert!(degraded
        .workflow(EngineWorkKind::Export)
        .unwrap()
        .available());
    let active = degraded
        .active_failure(EngineSubsystem::Playback)
        .expect("playback failure remains visible");
    assert_eq!(
        active.disposition(),
        EngineFailureDisposition::ContinueDegraded
    );
    assert_eq!(
        active.recovery_request(),
        Some(EngineRecoveryRequest::RestoreSubsystem)
    );
    assert!(!active.recovery_in_progress());
    assert_eq!(
        active.user_safe_error().code(),
        "error.unavailable.degraded"
    );
    assert!(!active
        .user_safe_error()
        .to_string()
        .contains("/Users/example/secret"));

    let recovering = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "recover-playback",
            EngineCommand::BeginRecovery(EngineSubsystem::Playback),
        )
        .unwrap()
        .result(),
    )
    .clone();
    dispatcher.drain_events().unwrap();
    let recovering_snapshot = dispatcher
        .introspection_snapshot(&capabilities, Some(&resource_snapshot))
        .unwrap();
    assert!(recovering_snapshot
        .active_failure(EngineSubsystem::Playback)
        .unwrap()
        .recovery_in_progress());
    assert!(!recovering_snapshot
        .workflow(EngineWorkKind::Playback)
        .unwrap()
        .available());
    assert!(recovering_snapshot
        .workflow(EngineWorkKind::Rendering)
        .unwrap()
        .available());
    assert!(recovering_snapshot
        .workflow(EngineWorkKind::Export)
        .unwrap()
        .available());

    dispatch(
        &mut dispatcher,
        "complete-playback-recovery",
        EngineCommand::CompleteLifecycleAction(
            recovering
                .pending_action()
                .expect("recovery owns one exact lifecycle action"),
        ),
    )
    .unwrap();
    dispatcher.drain_events().unwrap();
    let restored = dispatcher
        .introspection_snapshot(&capabilities, Some(&resource_snapshot))
        .unwrap();
    assert_eq!(restored.health(), EngineHealth::Healthy);
    assert!(restored.active_failures().is_empty());
    assert!(restored.workflows().iter().all(|status| status.available()));

    assert_degradation_cycle(
        &mut dispatcher,
        &capabilities,
        &resource_snapshot,
        EngineSubsystem::Rendering,
        [true, false, false],
    );
    assert_degradation_cycle(
        &mut dispatcher,
        &capabilities,
        &resource_snapshot,
        EngineSubsystem::Export,
        [true, true, false],
    );
    assert_eq!(dispatcher.scenario_snapshot().unwrap().revision(), 0);
    assert!(dispatcher.drain_events().unwrap().is_empty());

    drop(reservation);
}

#[test]
fn full_introspection_requires_the_dispatcher_owner() {
    let (dispatcher, capabilities) = {
        let _domain = ExecutionDomain::EngineControl
            .enter_current()
            .expect("construction owns engine control");
        let dispatcher = EngineCommandDispatcher::new().unwrap();
        let capabilities =
            MediaCapabilities::from_registry(&media_backend_registry().unwrap()).unwrap();
        (dispatcher, capabilities)
    };

    let error = dispatcher
        .introspection_snapshot(&capabilities, None)
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
}

fn assert_resource_state(
    snapshot: &EngineIntrospectionSnapshot,
    revision: u64,
    total_used_bytes: u64,
    cache_used_bytes: u64,
) {
    let resources = snapshot
        .resources()
        .expect("resource state was supplied to introspection");
    assert_eq!(resources.revision(), revision);
    assert_eq!(resources.total_limit_bytes(), 1_024);
    assert_eq!(resources.total_used_bytes(), total_used_bytes);
    assert_eq!(resources.available_bytes(), 1_024 - total_used_bytes);
    assert_eq!(
        resources
            .classes()
            .iter()
            .map(|status| status.class())
            .collect::<Vec<_>>(),
        ResourceClass::ALL
    );
    assert_eq!(
        resources.class(ResourceClass::Cache).used_bytes(),
        cache_used_bytes
    );
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

fn assert_degradation_cycle(
    dispatcher: &mut EngineCommandDispatcher,
    capabilities: &MediaCapabilities,
    resources: &superi_engine::resource_arbitration::ResourceArbitrationSnapshot,
    subsystem: EngineSubsystem,
    expected_available: [bool; 3],
) {
    dispatch(
        dispatcher,
        &format!("degrade-{}", subsystem.code()),
        EngineCommand::ReportRuntimeFailure {
            subsystem,
            failure: EngineReportedFailure::new(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "subsystem is temporarily unavailable",
            )
            .unwrap(),
        },
    )
    .unwrap();
    dispatcher.drain_events().unwrap();
    let degraded = dispatcher
        .introspection_snapshot(capabilities, Some(resources))
        .unwrap();
    assert_eq!(degraded.health(), EngineHealth::Degraded);
    for (work, available) in [
        EngineWorkKind::Playback,
        EngineWorkKind::Rendering,
        EngineWorkKind::Export,
    ]
    .into_iter()
    .zip(expected_available)
    {
        let status = degraded.workflow(work).unwrap();
        assert_eq!(status.available(), available);
        assert_eq!(
            status.blocking_subsystem(),
            (!available).then_some(subsystem)
        );
    }

    let recovering = lifecycle_result(
        dispatch(
            dispatcher,
            &format!("recover-{}", subsystem.code()),
            EngineCommand::BeginRecovery(subsystem),
        )
        .unwrap()
        .result(),
    )
    .clone();
    dispatcher.drain_events().unwrap();
    let recovery_snapshot = dispatcher
        .introspection_snapshot(capabilities, Some(resources))
        .unwrap();
    assert!(recovery_snapshot
        .active_failure(subsystem)
        .unwrap()
        .recovery_in_progress());

    dispatch(
        dispatcher,
        &format!("complete-{}-recovery", subsystem.code()),
        EngineCommand::CompleteLifecycleAction(recovering.pending_action().unwrap()),
    )
    .unwrap();
    dispatcher.drain_events().unwrap();
    let restored = dispatcher
        .introspection_snapshot(capabilities, Some(resources))
        .unwrap();
    assert_eq!(restored.health(), EngineHealth::Healthy);
    assert!(restored.active_failures().is_empty());
    assert!(restored.workflows().iter().all(|status| status.available()));
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
