use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::ids::{ClipId, GapId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::{Duration, RationalTime, TimeRange, Timebase};
use superi_engine::dispatcher::{
    AudioAutomationState, EngineCommand, EngineCommandDispatcher, EngineCommandRequest,
    EngineCommandResult, EngineTransactionId,
};
use superi_engine::editor_state::{
    EditorAudioChannelTarget, EditorAudioContinuity, EditorAudioRecordContinuity,
    EditorAudioRouteDestination, EditorAudioSourceContinuity, EditorStateAvailability,
};
use superi_engine::export_dispatch::EngineExportJobCommand;
use superi_engine::export_jobs::ExportJobQueueConfig;
use superi_engine::transport::PlaybackTransportCommand;
use superi_project::document::ProjectDocument;
use superi_timeline::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting,
    AudioTrackSemantics, Clip, ClipSource, EditorialProject, Gap, Timeline, Track, TrackItem,
    TrackSemantics,
};

const PROJECT: ProjectId = ProjectId::from_raw(0xe017);
const ROOT: TimelineId = TimelineId::from_raw(0xe018);
const SOURCE: TimelineId = TimelineId::from_raw(0xe019);
const AUDIO_TRACK: TrackId = TrackId::from_raw(0xe01a);
const SOURCE_TRACK: TrackId = TrackId::from_raw(0xe01b);
const LEFT_CLIP: ClipId = ClipId::from_raw(0xe01c);
const RIGHT_CLIP: ClipId = ClipId::from_raw(0xe01d);

fn document() -> ProjectDocument {
    let rate = Timebase::integer(24).unwrap();
    let timeline = Timeline::new(
        ROOT,
        "editor state timeline",
        rate,
        RationalTime::zero(rate),
        vec![],
    );
    let project = EditorialProject::new(PROJECT, "editor state project", [], [timeline]).unwrap();
    ProjectDocument::new(project, ROOT).unwrap()
}

fn audio_semantics() -> TrackSemantics {
    let source = ChannelLayout::new([
        ChannelPosition::FrontLeft,
        ChannelPosition::FrontRight,
        ChannelPosition::FrontCenter,
    ])
    .unwrap();
    let routing = AudioRouting::new(
        AudioRouteDestination::Main,
        ChannelLayout::stereo(),
        [
            AudioChannelRoute::new(
                ChannelPosition::FrontLeft,
                AudioChannelTarget::Channel(ChannelPosition::FrontLeft),
            ),
            AudioChannelRoute::new(
                ChannelPosition::FrontRight,
                AudioChannelTarget::Channel(ChannelPosition::FrontRight),
            ),
            AudioChannelRoute::new(ChannelPosition::FrontCenter, AudioChannelTarget::Muted),
        ],
    )
    .unwrap();
    TrackSemantics::Audio(AudioTrackSemantics::new(48_000, source, routing).unwrap())
}

fn sample_range(start: i64, sample_count: u64) -> TimeRange {
    let clock = Timebase::integer(48_000).unwrap();
    TimeRange::new(
        RationalTime::new(start, clock),
        Duration::new(sample_count, clock).unwrap(),
    )
    .unwrap()
}

fn audio_document() -> ProjectDocument {
    let clock = Timebase::integer(48_000).unwrap();
    let source = Timeline::new(
        SOURCE,
        "source audio",
        clock,
        RationalTime::zero(clock),
        vec![Track::new(
            SOURCE_TRACK,
            "source audio track",
            audio_semantics(),
            vec![TrackItem::Gap(Gap::new(
                GapId::from_raw(0xe01e),
                "source extent",
                sample_range(0, 2_000),
            ))],
        )],
    );
    let root = Timeline::new(
        ROOT,
        "editor audio",
        clock,
        RationalTime::zero(clock),
        vec![Track::new(
            AUDIO_TRACK,
            "routed audio",
            audio_semantics(),
            vec![
                TrackItem::Clip(
                    Clip::new(
                        LEFT_CLIP,
                        "left",
                        ClipSource::Timeline(SOURCE),
                        sample_range(0, 500),
                        sample_range(0, 500),
                    )
                    .unwrap(),
                ),
                TrackItem::Gap(Gap::new(
                    GapId::from_raw(0xe01f),
                    "intentional silence",
                    sample_range(500, 100),
                )),
                TrackItem::Clip(
                    Clip::new(
                        RIGHT_CLIP,
                        "right",
                        ClipSource::Timeline(SOURCE),
                        sample_range(500, 500),
                        sample_range(600, 500),
                    )
                    .unwrap(),
                ),
            ],
        )],
    );
    let project = EditorialProject::new(PROJECT, "audio editor state", [], [root, source]).unwrap();
    ProjectDocument::new(project, ROOT).unwrap()
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

#[test]
fn editor_state_requires_one_project_and_captures_one_eventless_revision() -> Result<()> {
    let _domain = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    let missing = dispatch(
        &mut dispatcher,
        "editor-state-missing-project",
        EngineCommand::InspectEditorState,
    )
    .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::Unavailable);
    assert!(dispatcher.drain_events()?.is_empty());

    dispatcher.attach_project(document())?;
    let before = dispatcher.project_history_state()?;
    let outcome = dispatch(
        &mut dispatcher,
        "editor-state-inspect",
        EngineCommand::InspectEditorState,
    )?;
    let EngineCommandResult::EditorState(state) = outcome.result() else {
        panic!("expected editor state result");
    };

    assert_eq!(state.project(), &before);
    assert_eq!(state.diagnostics().project_id(), PROJECT);
    assert_eq!(state.diagnostics().observed_document_revision(), 0);
    assert_eq!(state.settings().project_revision(), 0);
    assert!(matches!(
        state.audio_automation(),
        EditorStateAvailability::Detached
    ));
    assert!(matches!(
        state.project_recovery(),
        EditorStateAvailability::Detached
    ));
    assert!(!state.playback().attached());
    assert!(!state.playback().pending_command());
    assert!(state.playback().latest().is_none());
    assert!(!state.export().attached());
    assert!(state.export().latest().is_none());
    assert_eq!(dispatcher.project_history_state()?, before);
    assert!(dispatcher.drain_events()?.is_empty());
    Ok(())
}

#[test]
fn editor_state_distinguishes_attached_unobserved_pending_and_observed_owners() -> Result<()> {
    let _domain = ExecutionDomain::EngineControl.enter_current()?;
    let (mut dispatcher, _playback_executor) = EngineCommandDispatcher::new_with_playback_bridge()?;
    dispatcher.attach_project(document())?;
    dispatcher.attach_audio_automation(AudioAutomationState::new())?;
    let _runtime = dispatcher.attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 1, 2)?)?;

    let unobserved = dispatch(
        &mut dispatcher,
        "editor-state-unobserved",
        EngineCommand::InspectEditorState,
    )?;
    let EngineCommandResult::EditorState(unobserved) = unobserved.result() else {
        panic!("expected editor state result");
    };
    assert!(matches!(
        unobserved.audio_automation(),
        EditorStateAvailability::Attached(snapshot) if snapshot.revision() == 0
    ));
    assert!(unobserved.playback().attached());
    assert!(!unobserved.playback().pending_command());
    assert!(unobserved.playback().latest().is_none());
    assert!(unobserved.export().attached());
    assert!(unobserved.export().latest().is_none());

    dispatch(
        &mut dispatcher,
        "playback-inspect-pending",
        EngineCommand::ExecutePlayback(PlaybackTransportCommand::Inspect),
    )?;
    let pending = dispatch(
        &mut dispatcher,
        "editor-state-pending",
        EngineCommand::InspectEditorState,
    )?;
    let EngineCommandResult::EditorState(pending) = pending.result() else {
        panic!("expected editor state result");
    };
    assert!(pending.playback().attached());
    assert!(pending.playback().pending_command());
    assert!(pending.playback().latest().is_none());

    dispatch(
        &mut dispatcher,
        "export-inspect-all",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::InspectAll),
    )?;
    let observed = dispatch(
        &mut dispatcher,
        "editor-state-observed",
        EngineCommand::InspectEditorState,
    )?;
    let EngineCommandResult::EditorState(observed) = observed.result() else {
        panic!("expected editor state result");
    };
    assert!(observed.export().attached());
    assert_eq!(observed.export().latest().unwrap().revision(), 0);
    assert!(observed.export().latest().unwrap().jobs().is_empty());
    assert!(dispatcher.drain_events()?.is_empty());
    Ok(())
}

#[test]
fn editor_state_preserves_channel_routing_and_audits_sample_exact_continuity() -> Result<()> {
    let _domain = ExecutionDomain::EngineControl.enter_current()?;
    let mut dispatcher = EngineCommandDispatcher::new()?;
    dispatcher.attach_project(audio_document())?;

    let outcome = dispatch(
        &mut dispatcher,
        "editor-state-audio",
        EngineCommand::InspectEditorState,
    )?;
    let EngineCommandResult::EditorState(state) = outcome.result() else {
        panic!("expected editor state result");
    };
    let track = state
        .audio_tracks()
        .iter()
        .find(|track| track.track_id() == AUDIO_TRACK.to_string())
        .unwrap();
    assert_eq!(track.timeline_id(), ROOT.to_string());
    assert_eq!(track.sample_rate(), 48_000);
    assert_eq!(
        track.source_channels(),
        ["front_left", "front_right", "front_center"]
    );
    assert_eq!(track.destination(), &EditorAudioRouteDestination::Main);
    assert_eq!(track.destination_channels(), ["front_left", "front_right"]);
    assert_eq!(track.routes().len(), 3);
    assert_eq!(track.routes()[2].source(), "front_center");
    assert_eq!(track.routes()[2].target(), &EditorAudioChannelTarget::Muted);
    assert_eq!(track.clip_count(), 2);
    let EditorAudioContinuity::Audited {
        uninterrupted_record_coverage,
        seams,
    } = track.continuity()
    else {
        panic!("expected exact audio continuity audit");
    };
    assert!(!uninterrupted_record_coverage);
    assert_eq!(seams.len(), 1);
    assert_eq!(
        seams[0].record(),
        EditorAudioRecordContinuity::Gap { sample_count: 100 }
    );
    assert_eq!(
        seams[0].source(),
        &EditorAudioSourceContinuity::DifferentClip {
            left: LEFT_CLIP.to_string(),
            right: RIGHT_CLIP.to_string(),
        }
    );
    let timeline_json = std::str::from_utf8(state.authored_documents().timeline().bytes()).unwrap();
    assert!(timeline_json.contains("front_center"));
    assert!(timeline_json.contains("muted"));
    assert!(dispatcher.drain_events()?.is_empty());
    Ok(())
}
