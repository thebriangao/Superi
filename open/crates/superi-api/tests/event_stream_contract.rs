use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use superi_api::api::EngineIntrospectionApi;
use superi_api::commands::ApiCommand;
use superi_api::editor::{
    EditorMediaMutation, EditorMediaPath, ExecuteProjectCommand, ProjectAction, ProjectCommand,
};
use superi_api::event_stream::{
    replacement_resource_manifest, CloseEventSubscription, EventGapReason, EventStreamApi,
    EventStreamConfig, EventStreamId, EventStreamSnapshot, OpenEventSubscription, PollEvents,
    PublicApiEvent, PublicEventCorrelation, SubscriptionId, SubscriptionStart,
    MAX_EVENT_STREAM_BOUND,
};
use superi_api::events::{ApiEvent, AsyncJobsChanged, ProjectStateChanged};
use superi_api::jobs::{AsyncJobHandle, AsyncJobStatus, AsyncJobsApi};
use superi_api::schema::{ApiResource, PublicApiSchemaApi, PublicMethodKind};
use superi_api::version::{
    CLOSE_EVENT_SUBSCRIPTION_METHOD, EVENT_STREAM_SCHEMA_VERSION, OPEN_EVENT_SUBSCRIPTION_METHOD,
    POLL_EVENT_SUBSCRIPTION_METHOD,
};
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::ids::{JobId, ProjectId, TimelineId};
use superi_core::time::Timebase;
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineTransactionId,
};
use superi_engine::editor as engine;
use superi_engine::export_dispatch::EngineExportJobCommand;
use superi_engine::export_jobs::{ExportJobExecutionContext, ExportJobQueueConfig};
use superi_engine::introspection::MediaCapabilities;
use superi_engine::lifecycle::EngineLifecycleSnapshot;
use superi_engine::media::media_backend_registry;

#[test]
fn stream_configuration_and_registration_are_strict_and_bounded() {
    assert!(EventStreamId::new("").is_err());
    assert!(EventStreamId::new("Stream Uppercase").is_err());
    assert!(SubscriptionId::new("subscription\ncontrol").is_err());
    assert!(EventStreamConfig::new(0, 1, 1).is_err());
    assert!(EventStreamConfig::new(1, 0, 1).is_err());
    assert!(EventStreamConfig::new(1, 1, 0).is_err());
    assert!(EventStreamConfig::new(MAX_EVENT_STREAM_BOUND + 1, 1, 1).is_err());

    let config = EventStreamConfig::new(4, 2, 2).unwrap();
    let mut stream = EventStreamApi::new(EventStreamId::new("stream-main").unwrap(), config);
    let opened = stream
        .open(OpenEventSubscription::new(
            SubscriptionId::new("subscriber-a").unwrap(),
            SubscriptionStart::EarliestAvailable,
        ))
        .unwrap();

    assert_eq!(opened.stream_id().as_str(), "stream-main");
    assert_eq!(opened.subscription_id().as_str(), "subscriber-a");
    assert_eq!(opened.initial_after_sequence(), 0);

    assert_eq!(
        OpenEventSubscription::METHOD,
        OPEN_EVENT_SUBSCRIPTION_METHOD
    );
    assert_eq!(OpenEventSubscription::KIND, PublicMethodKind::Command);
    assert_eq!(
        CloseEventSubscription::METHOD,
        CLOSE_EVENT_SUBSCRIPTION_METHOD
    );
    assert_eq!(CloseEventSubscription::KIND, PublicMethodKind::Command);
    assert_eq!(PollEvents::METHOD, POLL_EVENT_SUBSCRIPTION_METHOD);
    assert_eq!(PollEvents::KIND, PublicMethodKind::Query);
    assert_eq!(PollEvents::SCHEMA_VERSION, EVENT_STREAM_SCHEMA_VERSION);
    assert_eq!(EventStreamSnapshot::RESOURCE, "superi.events.subscription");

    let duplicate = stream
        .open(OpenEventSubscription::new(
            SubscriptionId::new("subscriber-a").unwrap(),
            SubscriptionStart::Latest,
        ))
        .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::Conflict);
    stream
        .open(OpenEventSubscription::new(
            SubscriptionId::new("subscriber-b").unwrap(),
            SubscriptionStart::Latest,
        ))
        .unwrap();
    let exhausted = stream
        .open(OpenEventSubscription::new(
            SubscriptionId::new("subscriber-c").unwrap(),
            SubscriptionStart::Latest,
        ))
        .unwrap_err();
    assert_eq!(exhausted.category(), ErrorCategory::ResourceExhausted);

    let mut config_wire = serde_json::to_value(config).unwrap();
    config_wire["guessed"] = json!(true);
    assert!(serde_json::from_value::<EventStreamConfig>(config_wire).is_err());
    assert!(serde_json::from_value::<PollEvents>(json!({
        "stream_id": "stream-main",
        "subscription_id": "subscriber-a",
        "after_sequence": 0,
        "requested_limit": 0
    }))
    .is_err());
}

#[test]
fn real_project_events_replay_idempotently_for_independent_subscribers() -> Result<()> {
    let source_event = project_events(1)?.pop().unwrap();
    let source_sequence = source_event.sequence();
    let command_sequence = source_event.command_sequence();
    let mut stream = EventStreamApi::new(
        EventStreamId::new("stream-project")?,
        EventStreamConfig::new(8, 2, 4)?,
    );
    for subscriber in ["subscriber-a", "subscriber-b"] {
        stream.open(OpenEventSubscription::new(
            SubscriptionId::new(subscriber)?,
            SubscriptionStart::EarliestAvailable,
        ))?;
    }

    let record = stream.publish_typed(source_event.clone())?;
    assert_eq!(record.sequence().get(), 1);
    assert_eq!(record.event_name(), "superi.project.state.changed");
    assert_eq!(
        record.correlation(),
        &PublicEventCorrelation::Command {
            source_event_sequence: source_sequence,
            command_sequence,
            transaction_id: "event-stream-project-0".to_owned(),
        }
    );
    assert_eq!(
        record.replacement_resource().descriptor().resource(),
        "superi.project.history"
    );
    assert_eq!(
        record.replacement_resource().descriptor().method_kind(),
        PublicMethodKind::Command
    );

    let first = stream.poll(PollEvents::new(
        EventStreamId::new("stream-project")?,
        SubscriptionId::new("subscriber-a")?,
        0,
        4,
    )?)?;
    let retried = stream.poll(PollEvents::new(
        EventStreamId::new("stream-project")?,
        SubscriptionId::new("subscriber-a")?,
        0,
        4,
    )?)?;
    assert_eq!(first, retried);
    assert_eq!(
        first.batch().unwrap().records(),
        std::slice::from_ref(&record)
    );

    let independent = stream.poll(PollEvents::new(
        EventStreamId::new("stream-project")?,
        SubscriptionId::new("subscriber-b")?,
        0,
        4,
    )?)?;
    assert_eq!(
        independent.batch().unwrap().records(),
        std::slice::from_ref(&record)
    );

    let encoded = serde_json::to_value(&record).unwrap();
    let decoded: superi_api::event_stream::PublicEventRecord =
        serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(decoded, record);
    let mut unknown = encoded;
    unknown["guessed"] = json!(true);
    assert!(
        serde_json::from_value::<superi_api::event_stream::PublicEventRecord>(unknown).is_err()
    );

    let event = PublicApiEvent::try_from(source_event)?;
    let mut inconsistent = serde_json::to_value(&event).unwrap();
    inconsistent["payload"]["project_revision"] = json!(99);
    assert!(serde_json::from_value::<PublicApiEvent>(inconsistent).is_err());
    assert!(serde_json::from_value::<PublicApiEvent>(json!({
        "event": "superi.guessed.changed",
        "payload": {}
    }))
    .is_err());

    stream.close(CloseEventSubscription::new(
        EventStreamId::new("stream-project")?,
        SubscriptionId::new("subscriber-a")?,
    ))?;
    let closed = stream
        .poll(PollEvents::new(
            EventStreamId::new("stream-project")?,
            SubscriptionId::new("subscriber-a")?,
            0,
            4,
        )?)
        .unwrap_err();
    assert_eq!(closed.category(), ErrorCategory::NotFound);
    assert_eq!(
        stream
            .poll(PollEvents::new(
                EventStreamId::new("stream-project")?,
                SubscriptionId::new("subscriber-b")?,
                0,
                4,
            )?)?
            .batch()
            .unwrap()
            .records(),
        [record]
    );
    Ok(())
}

#[test]
fn retention_batch_limits_and_reconnects_never_hide_a_gap() -> Result<()> {
    let events = project_events(5)?;
    let mut stream = EventStreamApi::new(
        EventStreamId::new("stream-bounded")?,
        EventStreamConfig::new(2, 1, 1)?,
    );
    stream.open(OpenEventSubscription::new(
        SubscriptionId::new("subscriber-main")?,
        SubscriptionStart::EarliestAvailable,
    ))?;
    for event in events.iter().take(3).cloned() {
        stream.publish_typed(event)?;
    }

    let gap_result = stream.poll(PollEvents::new(
        EventStreamId::new("stream-bounded")?,
        SubscriptionId::new("subscriber-main")?,
        0,
        MAX_EVENT_STREAM_BOUND,
    )?)?;
    let gap = gap_result.gap().unwrap();
    assert_eq!(gap.reason(), EventGapReason::CursorEvicted);
    assert_eq!(gap.oldest_available_sequence(), Some(2));
    assert_eq!(gap.latest_sequence(), 3);
    assert_eq!(gap.reset_barrier(), 3);

    let schema = PublicApiSchemaApi::new()?.execute(superi_api::schema::GetPublicApiSchema::new());
    let expected_resources = schema
        .snapshot()
        .resources()
        .iter()
        .map(|resource| resource.resource())
        .filter(|resource| *resource != EventStreamSnapshot::RESOURCE)
        .collect::<Vec<_>>();
    assert_eq!(
        gap.replacement_resources()
            .iter()
            .map(|resource| resource.resource())
            .collect::<Vec<_>>(),
        expected_resources
    );
    assert_eq!(gap.replacement_resources(), replacement_resource_manifest());
    assert_eq!(
        gap.replacement_resources()
            .iter()
            .find(|resource| resource.resource() == "superi.project.history")
            .unwrap()
            .method_kind(),
        PublicMethodKind::Command
    );
    assert_eq!(
        gap.replacement_resources()
            .iter()
            .find(|resource| resource.resource() == "superi.slice.scenario.state")
            .unwrap()
            .method_kind(),
        PublicMethodKind::Command
    );

    stream.publish_typed(events[3].clone())?;
    stream.publish_typed(events[4].clone())?;
    let capped = stream.poll(PollEvents::new(
        EventStreamId::new("stream-bounded")?,
        SubscriptionId::new("subscriber-main")?,
        gap.reset_barrier(),
        MAX_EVENT_STREAM_BOUND,
    )?)?;
    assert_eq!(capped.batch().unwrap().records().len(), 1);
    assert_eq!(capped.batch().unwrap().records()[0].sequence().get(), 4);
    assert_eq!(capped.batch().unwrap().through_sequence(), 4);
    let second = stream.poll(PollEvents::new(
        EventStreamId::new("stream-bounded")?,
        SubscriptionId::new("subscriber-main")?,
        4,
        MAX_EVENT_STREAM_BOUND,
    )?)?;
    assert_eq!(second.batch().unwrap().records()[0].sequence().get(), 5);

    let future = stream
        .poll(PollEvents::new(
            EventStreamId::new("stream-bounded")?,
            SubscriptionId::new("subscriber-main")?,
            6,
            1,
        )?)
        .unwrap_err();
    assert_eq!(future.category(), ErrorCategory::InvalidInput);

    let restarted = EventStreamApi::new(
        EventStreamId::new("stream-restarted")?,
        EventStreamConfig::default(),
    );
    let restart = restarted.poll(PollEvents::new(
        EventStreamId::new("stream-bounded")?,
        SubscriptionId::new("subscriber-main")?,
        5,
        1,
    )?)?;
    let restart_gap = restart.gap().unwrap();
    assert_eq!(restart_gap.reason(), EventGapReason::StreamRestarted);
    assert_eq!(restart_gap.current_stream_id().as_str(), "stream-restarted");
    assert_eq!(restart_gap.reset_barrier(), 0);
    Ok(())
}

#[test]
fn real_observation_events_keep_revision_correlation() -> Result<()> {
    let _domain = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let capabilities = MediaCapabilities::from_registry(&media_backend_registry()?)?;
    let initial = dispatcher.introspection_snapshot(&capabilities, None)?;
    let mut producer = EngineIntrospectionApi::new(&initial);
    drive_running(&mut dispatcher)?;
    dispatcher.drain_events()?;
    let running = dispatcher.introspection_snapshot(&capabilities, None)?;
    let event = producer.synchronize(&running)?.unwrap();
    let revision = event.snapshot().revision();

    let mut stream = EventStreamApi::new(
        EventStreamId::new("stream-observation")?,
        EventStreamConfig::default(),
    );
    let record = stream.publish_typed(event)?;
    assert_eq!(
        record.correlation(),
        &PublicEventCorrelation::Observation { revision }
    );
    assert_eq!(
        record.replacement_resource().descriptor().resource(),
        "superi.engine.introspection"
    );
    Ok(())
}

#[test]
fn real_async_job_lifecycle_events_use_the_same_public_stream() -> Result<()> {
    let guard = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let runtime = dispatcher.attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 2, 4)?)?;
    drive_running(&mut dispatcher)?;
    dispatcher.drain_events()?;

    let job_id = JobId::from_raw(0xc019);
    runtime.prepare_executor(job_id, |context: &ExportJobExecutionContext, _permit| {
        context.progress().increment(3)?;
        Ok(19)
    })?;
    dispatch(
        &mut dispatcher,
        "event-stream-job-submit",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(job_id, [])?),
    )?;
    dispatcher.drain_events()?;

    let handle = AsyncJobHandle::from(job_id);
    let mut jobs = AsyncJobsApi::new(dispatcher);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let state = jobs.poll_runtime("event-stream-job-poll")?;
        if state.snapshot().job(&handle).unwrap().status() == AsyncJobStatus::Completed {
            break;
        }
        assert!(Instant::now() < deadline, "job fixture did not complete");
        thread::yield_now();
    }
    let lifecycle_events = jobs.drain_events()?;
    assert!(lifecycle_events
        .windows(2)
        .all(|pair| pair[0].sequence() < pair[1].sequence()));
    let completed = lifecycle_events.last().unwrap().clone();
    assert_eq!(
        completed.snapshot().job(&handle).unwrap().status(),
        AsyncJobStatus::Completed
    );

    let mut stream = EventStreamApi::new(
        EventStreamId::new("stream-jobs")?,
        EventStreamConfig::default(),
    );
    let record = stream.publish_typed(completed)?;
    assert_eq!(record.event_name(), AsyncJobsChanged::NAME);
    assert!(matches!(
        record.correlation(),
        PublicEventCorrelation::Command { transaction_id, .. }
            if transaction_id == "event-stream-job-poll"
    ));
    assert_eq!(
        record.replacement_resource().descriptor().resource(),
        "superi.jobs"
    );

    drop(jobs);
    drop(guard);
    runtime.shutdown()?;
    Ok(())
}

#[test]
fn closed_event_union_matches_the_live_catalog_exactly() -> Result<()> {
    let schema = PublicApiSchemaApi::new()?.execute(superi_api::schema::GetPublicApiSchema::new());
    let mut union_names = PublicApiEvent::NAMES.to_vec();
    union_names.sort_unstable();
    assert_eq!(
        union_names,
        schema
            .snapshot()
            .events()
            .iter()
            .map(|event| event.event())
            .collect::<Vec<_>>()
    );
    Ok(())
}

fn project_events(count: u64) -> Result<Vec<ProjectStateChanged>> {
    let _domain = ExecutionDomain::EngineControl.enter_current()?;
    let project_id = ProjectId::from_raw(0xc019);
    let root_timeline_id = TimelineId::from_raw(0xc020);
    let media_id = engine::MediaId::from_raw(0xc021);
    let edit_rate = Timebase::integer(24)?;
    let root = engine::Timeline::new(
        root_timeline_id,
        "event stream root",
        edit_rate,
        engine::RationalTime::zero(edit_rate),
        vec![],
    );
    let media = engine::LinkedMediaReference::new(
        media_id,
        "event stream source",
        "urn:event-stream:initial",
        None,
    );
    let editorial =
        engine::EditorialProject::new(project_id, "event stream project", [media], [root])?;
    let document = engine::ProjectDocument::new(editorial, root_timeline_id)?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    dispatcher.attach_project(document)?;
    let mut editor = superi_api::editor::ProjectEditorApi::new(dispatcher)?;
    let mut events = Vec::new();
    for expected_revision in 0..count {
        let transaction_id = format!("event-stream-project-{expected_revision}");
        let result = editor.execute(ExecuteProjectCommand::new(
            transaction_id,
            expected_revision,
            ProjectCommand::Apply {
                actions: vec![ProjectAction::MutateMedia {
                    mutation: EditorMediaMutation::SetPath {
                        media_id: media_id.to_string(),
                        path: EditorMediaPath::ProjectRelative {
                            path: format!("source-{expected_revision}.mov"),
                        },
                    },
                }],
            },
        ))?;
        assert!(result.authored_state_changed());
        events.extend(editor.drain_events()?);
    }
    Ok(events)
}

fn drive_running(dispatcher: &mut EngineCommandDispatcher) -> Result<()> {
    let mut lifecycle = lifecycle_result(
        dispatch(
            dispatcher,
            "event-stream-startup",
            EngineCommand::InspectLifecycle,
        )?
        .result(),
    )
    .clone();
    while lifecycle.phase() == LifecyclePhase::Starting {
        lifecycle = lifecycle_result(
            dispatch(
                dispatcher,
                &format!(
                    "event-stream-initialize-{}",
                    lifecycle.pending_action().unwrap().subsystem()
                ),
                EngineCommand::CompleteLifecycleAction(lifecycle.pending_action().unwrap()),
            )?
            .result(),
        )
        .clone();
    }
    assert_eq!(lifecycle.phase(), LifecyclePhase::Running);
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

fn lifecycle_result(result: &EngineCommandResult) -> &EngineLifecycleSnapshot {
    match result {
        EngineCommandResult::Lifecycle(snapshot) => snapshot,
        _ => panic!("expected lifecycle result"),
    }
}
