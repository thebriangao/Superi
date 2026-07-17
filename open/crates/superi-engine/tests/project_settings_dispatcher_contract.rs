use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition, PixelFormat};
use superi_core::settings::SettingValue;
use superi_core::time::{FrameRate, RationalTime, SampleTime, Timebase};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineTransactionId, MAX_PENDING_ENGINE_EVENTS,
};
use superi_engine::history::{ProjectHistoryCommand, ProjectMutationKind};
use superi_engine::project_settings::{CachePolicy, ColorSettings, ProxyMode};
use superi_project::document::ProjectDocument;
use superi_project::document::ProjectSnapshot;
use superi_project::settings::{
    ProjectSettingMutation, ProjectSettingsTransaction, AUDIO_OUTPUT_LAYOUT_KEY,
    AUDIO_SAMPLE_RATE_KEY, CACHE_MAX_BYTES_KEY, CACHE_MAX_FRAMES_KEY, CACHE_MODE_KEY,
    COLOR_CONFIG_FINGERPRINT_KEY, COLOR_CONFIG_ID_KEY, COLOR_MODE_KEY, COLOR_WORKING_SPACE_KEY,
    PROXY_MODE_KEY, PROXY_QUALITY_KEY, RENDER_PIXEL_FORMAT_KEY,
};
use superi_project::ProjectDatabase;
use superi_timeline::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting, AudioSpan,
    AudioTrackSemantics, EditorialProject, LinkedMediaReference, Timeline, Track, TrackSemantics,
};

const PROJECT: ProjectId = ProjectId::from_raw(10_001);
const ROOT: TimelineId = TimelineId::from_raw(10_002);
const AUDIO_TRACK: TrackId = TrackId::from_raw(10_003);
const CLIP: ClipId = ClipId::from_raw(10_004);

fn project_document() -> ProjectDocument {
    let layout = ChannelLayout::stereo();
    let routing = AudioRouting::new(
        AudioRouteDestination::Main,
        layout.clone(),
        [
            AudioChannelRoute::new(
                ChannelPosition::FrontLeft,
                AudioChannelTarget::Channel(ChannelPosition::FrontLeft),
            ),
            AudioChannelRoute::new(
                ChannelPosition::FrontRight,
                AudioChannelTarget::Channel(ChannelPosition::FrontRight),
            ),
        ],
    )
    .unwrap();
    let semantics = AudioTrackSemantics::new(48_000, layout, routing).unwrap();
    let timeline_rate = FrameRate::FPS_24.timebase();
    let timeline = Timeline::new(
        ROOT,
        "audio settings timeline",
        timeline_rate,
        RationalTime::zero(timeline_rate),
        vec![Track::new(
            AUDIO_TRACK,
            "A1",
            TrackSemantics::Audio(semantics),
            vec![],
        )],
    );
    let editorial = EditorialProject::new(
        PROJECT,
        "audio settings project",
        std::iter::empty::<LinkedMediaReference>(),
        [timeline],
    )
    .unwrap();
    ProjectDocument::new(editorial, ROOT).unwrap()
}

fn authored_audio(document: &ProjectDocument) -> &AudioTrackSemantics {
    let track = document
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .track(AUDIO_TRACK)
        .unwrap();
    let TrackSemantics::Audio(audio) = track.semantics() else {
        panic!("expected audio track semantics");
    };
    audio
}

fn authored_audio_snapshot(snapshot: &ProjectSnapshot) -> &AudioTrackSemantics {
    let track = snapshot
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .track(AUDIO_TRACK)
        .unwrap();
    let TrackSemantics::Audio(audio) = track.semantics() else {
        panic!("expected audio track semantics");
    };
    audio
}

fn continuity(audio: &AudioTrackSemantics) -> superi_timeline::model::AudioContinuityReport {
    let rate = Timebase::integer(48_000).unwrap();
    let first = AudioSpan::new(
        CLIP,
        RationalTime::zero(rate),
        SampleTime::new(1_000, 48_000).unwrap(),
        48_000,
    )
    .unwrap();
    let second = AudioSpan::new(
        CLIP,
        RationalTime::new(48_000, rate),
        SampleTime::new(49_000, 48_000).unwrap(),
        48_000,
    )
    .unwrap();
    audio.audit_continuity(&[first, second]).unwrap()
}

fn dispatch_settings(
    dispatcher: &mut EngineCommandDispatcher,
    transaction_id: &str,
    transaction: ProjectSettingsTransaction,
) -> superi_core::error::Result<superi_engine::dispatcher::EngineCommandOutcome> {
    dispatcher.dispatch(EngineCommandRequest::new(
        EngineTransactionId::new(transaction_id).unwrap(),
        EngineCommand::ExecuteProjectSettings(transaction),
    ))
}

#[test]
fn project_settings_dispatch_through_the_real_engine_without_rewriting_authored_audio() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let project = project_document();
    let authored_before = authored_audio(&project).clone();
    let continuity_before = continuity(&authored_before);
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(project).unwrap();

    let initial = dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("inspect-project-settings").unwrap(),
            EngineCommand::InspectProjectSettings,
        ))
        .unwrap();
    let EngineCommandResult::ProjectSettings(initial) = initial.result() else {
        panic!("expected project settings state");
    };
    assert_eq!(initial.project_revision(), 0);
    assert_eq!(
        initial.resolved().timeline().frame_rate(),
        FrameRate::FPS_24
    );
    assert_eq!(initial.resolved().audio().sample_rate().numerator(), 48_000);
    assert_eq!(
        initial.resolved().audio().channel_layout(),
        &ChannelLayout::stereo()
    );
    assert_eq!(initial.resolved().cache(), &CachePolicy::Automatic);
    let initial_render_fingerprint = initial.resolved().render().fingerprint();

    let transaction = ProjectSettingsTransaction::new(
        0,
        vec![
            ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Integer(96_000))
                .unwrap(),
            ProjectSettingMutation::set(
                AUDIO_OUTPUT_LAYOUT_KEY,
                SettingValue::Text("surround_5_1".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(CACHE_MODE_KEY, SettingValue::Text("bounded".into()))
                .unwrap(),
            ProjectSettingMutation::set(
                CACHE_MAX_BYTES_KEY,
                SettingValue::Integer(256 * 1024 * 1024),
            )
            .unwrap(),
            ProjectSettingMutation::set(CACHE_MAX_FRAMES_KEY, SettingValue::Integer(64)).unwrap(),
            ProjectSettingMutation::set(PROXY_MODE_KEY, SettingValue::Text("prefer".into()))
                .unwrap(),
            ProjectSettingMutation::set(PROXY_QUALITY_KEY, SettingValue::Text("quarter".into()))
                .unwrap(),
            ProjectSettingMutation::set(
                RENDER_PIXEL_FORMAT_KEY,
                SettingValue::Text("rgba32_float".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(COLOR_MODE_KEY, SettingValue::Text("pinned_config".into()))
                .unwrap(),
            ProjectSettingMutation::set(
                COLOR_WORKING_SPACE_KEY,
                SettingValue::Text("acescg".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(
                COLOR_CONFIG_ID_KEY,
                SettingValue::Text("studio.config".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(
                COLOR_CONFIG_FINGERPRINT_KEY,
                SettingValue::Text("a".repeat(64)),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let outcome = dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("change-project-settings").unwrap(),
            EngineCommand::ExecuteProjectSettings(transaction),
        ))
        .unwrap();
    let EngineCommandResult::ProjectSettings(state) = outcome.result() else {
        panic!("expected project settings state");
    };
    assert_eq!(state.project_revision(), 1);
    assert_eq!(state.resolved().audio().sample_rate().numerator(), 96_000);
    assert_eq!(
        state.resolved().audio().channel_layout(),
        &ChannelLayout::surround_5_1()
    );
    let CachePolicy::Bounded(limit) = state.resolved().cache() else {
        panic!("expected bounded cache policy");
    };
    assert_eq!(limit.max_bytes(), 256 * 1024 * 1024);
    assert_eq!(limit.max_frames(), 64);
    assert_eq!(state.resolved().proxy().mode(), ProxyMode::Prefer);
    assert_eq!(
        state.resolved().render().pixel_format(),
        PixelFormat::Rgba32Float
    );
    assert_ne!(
        state.resolved().render().fingerprint(),
        initial_render_fingerprint
    );
    match state.resolved().color() {
        ColorSettings::PinnedConfig {
            config_id,
            config_fingerprint,
            working_space,
        } => {
            assert_eq!(config_id, "studio.config");
            assert_eq!(config_fingerprint, &"a".repeat(64));
            assert_eq!(working_space, "acescg");
        }
        _ => panic!("expected explicit pinned color settings"),
    }

    let retained = dispatcher.project_snapshot().unwrap();
    let authored_after = authored_audio_snapshot(&retained);
    assert_eq!(authored_after, &authored_before);
    assert_eq!(authored_after.sample_rate(), 48_000);
    assert_eq!(authored_after.channel_layout(), &ChannelLayout::stereo());
    assert_eq!(authored_after.routing(), authored_before.routing());
    assert_eq!(continuity(authored_after), continuity_before);

    let mut database = ProjectDatabase::memory().unwrap();
    database.replace(&retained).unwrap();
    let reopened = database.load().unwrap();
    let reopened_audio = authored_audio(&reopened);
    assert_eq!(reopened_audio, &authored_before);
    assert_eq!(continuity(reopened_audio), continuity_before);

    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].project_revision(), Some(1));
    match events[0].event() {
        EngineEvent::ProjectSettingsChanged(event_state) => assert_eq!(event_state, state),
        _ => panic!("expected project settings change event"),
    }

    let history = dispatcher.project_history_state().unwrap();
    assert_eq!(history.undo_depth(), 1);
    assert_eq!(
        history.next_undo(),
        Some(ProjectMutationKind::ProjectSettings)
    );
    let undo = dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("undo-project-settings").unwrap(),
            EngineCommand::ExecuteProjectHistory(ProjectHistoryCommand::undo(1)),
        ))
        .unwrap();
    let EngineCommandResult::ProjectHistory(undo) = undo.result() else {
        panic!("expected project history state");
    };
    assert_eq!(undo.state().snapshot().revision(), 2);
    assert_eq!(
        undo.state()
            .snapshot()
            .settings()
            .integer(AUDIO_SAMPLE_RATE_KEY),
        Some(48_000)
    );
    assert_eq!(
        authored_audio_snapshot(undo.state().snapshot()),
        &authored_before
    );
    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    match events[0].event() {
        EngineEvent::ProjectStateChanged(history) => {
            assert_eq!(history.snapshot().revision(), 2);
            assert_eq!(
                history.next_redo(),
                Some(ProjectMutationKind::ProjectSettings)
            );
        }
        _ => panic!("expected project history change event"),
    }

    dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("redo-project-settings").unwrap(),
            EngineCommand::ExecuteProjectHistory(ProjectHistoryCommand::redo(2)),
        ))
        .unwrap();
    let restored = dispatcher.project_snapshot().unwrap();
    assert_eq!(restored.revision(), 3);
    assert_eq!(
        restored.settings().integer(AUDIO_SAMPLE_RATE_KEY),
        Some(96_000)
    );
    assert_eq!(authored_audio_snapshot(&restored), &authored_before);
}

#[test]
fn project_settings_failures_and_event_capacity_publish_no_partial_state() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut missing = EngineCommandDispatcher::new().unwrap();
    let error = missing
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("missing-project").unwrap(),
            EngineCommand::InspectProjectSettings,
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unavailable);

    let mut compatibility = EngineCommandDispatcher::scenario_only();
    assert_eq!(
        compatibility
            .attach_project(project_document())
            .unwrap_err()
            .category(),
        ErrorCategory::Unavailable
    );
    compatibility
        .attach_project_history(project_document(), 1)
        .unwrap();
    assert_eq!(
        compatibility
            .dispatch(EngineCommandRequest::new(
                EngineTransactionId::new("compatibility-settings").unwrap(),
                EngineCommand::InspectProjectSettings,
            ))
            .unwrap_err()
            .category(),
        ErrorCategory::Unavailable
    );

    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(project_document()).unwrap();
    let invalid = ProjectSettingsTransaction::new(
        0,
        vec![
            ProjectSettingMutation::set(CACHE_MODE_KEY, SettingValue::Text("bounded".into()))
                .unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        dispatch_settings(&mut dispatcher, "invalid-settings", invalid)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(dispatcher.project_snapshot().unwrap().revision(), 0);
    assert!(dispatcher.drain_events().unwrap().is_empty());

    for revision in 0..MAX_PENDING_ENGINE_EVENTS as u64 {
        let sample_rate = if revision % 2 == 0 { 96_000 } else { 48_000 };
        let transaction = ProjectSettingsTransaction::new(
            revision,
            vec![ProjectSettingMutation::set(
                AUDIO_SAMPLE_RATE_KEY,
                SettingValue::Integer(sample_rate),
            )
            .unwrap()],
        )
        .unwrap();
        dispatch_settings(
            &mut dispatcher,
            &format!("fill-project-settings-{revision}"),
            transaction,
        )
        .unwrap();
    }
    assert_eq!(
        dispatcher.project_snapshot().unwrap().revision(),
        MAX_PENDING_ENGINE_EVENTS as u64
    );

    let noop = ProjectSettingsTransaction::new(
        MAX_PENDING_ENGINE_EVENTS as u64,
        vec![
            ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Integer(48_000))
                .unwrap(),
        ],
    )
    .unwrap();
    dispatch_settings(&mut dispatcher, "full-queue-noop", noop).unwrap();
    assert_eq!(
        dispatcher.project_snapshot().unwrap().revision(),
        MAX_PENDING_ENGINE_EVENTS as u64
    );

    let blocked = ProjectSettingsTransaction::new(
        MAX_PENDING_ENGINE_EVENTS as u64,
        vec![
            ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Integer(96_000))
                .unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        dispatch_settings(&mut dispatcher, "full-queue-change", blocked)
            .unwrap_err()
            .category(),
        ErrorCategory::ResourceExhausted
    );
    assert_eq!(
        dispatcher.project_snapshot().unwrap().revision(),
        MAX_PENDING_ENGINE_EVENTS as u64
    );
    assert_eq!(
        dispatcher.drain_events().unwrap().len(),
        MAX_PENDING_ENGINE_EVENTS
    );

    let stale = ProjectSettingsTransaction::new(
        MAX_PENDING_ENGINE_EVENTS as u64 - 1,
        vec![
            ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Integer(96_000))
                .unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        dispatch_settings(&mut dispatcher, "stale-project-settings", stale)
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    assert!(dispatcher.drain_events().unwrap().is_empty());
}

#[test]
fn attached_project_settings_require_engine_control() {
    let mut dispatcher = {
        let _domain = ExecutionDomain::EngineControl
            .enter_current()
            .expect("construction owns engine control");
        let mut dispatcher = EngineCommandDispatcher::new().unwrap();
        dispatcher.attach_project(project_document()).unwrap();
        dispatcher
    };
    assert_eq!(
        dispatcher.project_snapshot().unwrap_err().category(),
        ErrorCategory::Conflict
    );
    let error = dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("off-domain-project-settings").unwrap(),
            EngineCommand::InspectProjectSettings,
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
}
