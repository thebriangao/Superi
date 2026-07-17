use superi_audio::graph::{AudioProcessBlock, AudioProcessor};
use superi_audio::mixing::{ChannelMap, ClipMixControls, ClipMixMutation, ClipMixState};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::ErrorCategory;
use superi_core::ids::ClipId;
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::SampleTime;
use superi_engine::dispatcher::{
    AudioAutomationKeyframe, AudioAutomationMode, AudioAutomationMutation, AudioAutomationSnapshot,
    AudioAutomationState, AudioAutomationTarget, AudioAutomationTransaction, EngineCommand,
    EngineCommandDispatcher, EngineCommandOutcome, EngineCommandRequest, EngineCommandResult,
    EngineEvent, EngineTransactionId, MAX_PENDING_ENGINE_EVENTS,
};

const RATE: u32 = 48_000;
const CLIP: ClipId = ClipId::from_raw(0xe011);

fn at(sample: i64) -> SampleTime {
    SampleTime::new(sample, RATE).unwrap()
}

fn target() -> AudioAutomationTarget {
    AudioAutomationTarget::clip_gain(CLIP)
}

fn keyframe(sample: i64, value: f32) -> AudioAutomationKeyframe {
    AudioAutomationKeyframe::new(at(sample), value).unwrap()
}

fn transaction(
    expected_revision: u64,
    mutations: Vec<AudioAutomationMutation>,
) -> AudioAutomationTransaction {
    AudioAutomationTransaction::new(expected_revision, mutations).unwrap()
}

fn dispatch(
    dispatcher: &mut EngineCommandDispatcher,
    transaction_id: &str,
    command: EngineCommand,
) -> superi_core::error::Result<EngineCommandOutcome> {
    dispatcher.dispatch(EngineCommandRequest::new(
        EngineTransactionId::new(transaction_id).unwrap(),
        command,
    ))
}

fn automation_result(result: &EngineCommandResult) -> &AudioAutomationSnapshot {
    match result {
        EngineCommandResult::AudioAutomation(snapshot) => snapshot,
        _ => panic!("expected audio automation result"),
    }
}

#[test]
fn owner_inspection_and_transactions_are_serialized_and_fully_correlated() {
    let mut scenario_only = EngineCommandDispatcher::scenario_only();
    assert_eq!(
        scenario_only
            .attach_audio_automation(AudioAutomationState::new())
            .unwrap_err()
            .category(),
        ErrorCategory::Unavailable
    );

    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    let missing = dispatch(
        &mut dispatcher,
        "automation-missing",
        EngineCommand::InspectAudioAutomation,
    )
    .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::Unavailable);
    assert!(dispatcher.drain_events().unwrap().is_empty());

    dispatcher
        .attach_audio_automation(AudioAutomationState::new())
        .unwrap();
    assert_eq!(
        dispatcher
            .attach_audio_automation(AudioAutomationState::new())
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );

    let inspected = dispatch(
        &mut dispatcher,
        "automation-inspect",
        EngineCommand::InspectAudioAutomation,
    )
    .unwrap();
    assert_eq!(inspected.command_sequence(), 1);
    assert_eq!(automation_result(inspected.result()).revision(), 0);
    assert!(dispatcher.drain_events().unwrap().is_empty());

    let changed = dispatch(
        &mut dispatcher,
        "automation-create",
        EngineCommand::ExecuteAudioAutomation(transaction(
            0,
            vec![
                AudioAutomationMutation::CreateLane {
                    target: target(),
                    sample_rate: RATE,
                    default_gain: 0.5,
                },
                AudioAutomationMutation::SetKeyframe {
                    target: target(),
                    keyframe: keyframe(0, 0.5),
                },
                AudioAutomationMutation::SetKeyframe {
                    target: target(),
                    keyframe: keyframe(1, 1.0),
                },
            ],
        )),
    )
    .unwrap();
    assert_eq!(changed.command_sequence(), 2);
    let changed_snapshot = automation_result(changed.result()).clone();
    assert_eq!(changed_snapshot.revision(), 1);

    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sequence(), 1);
    assert_eq!(events[0].command_sequence(), 2);
    assert_eq!(events[0].transaction_id().as_str(), "automation-create");
    assert_eq!(events[0].audio_automation_revision(), Some(1));
    assert_eq!(events[0].project_revision(), None);
    assert_eq!(events[0].lifecycle_state_revision(), None);
    assert_eq!(events[0].playback_epoch(), None);
    assert_eq!(events[0].export_state_revision(), None);
    assert_eq!(events[0].recovery_state_revision(), None);
    match events[0].event() {
        EngineEvent::AudioAutomationStateChanged(snapshot) => {
            assert_eq!(snapshot.as_ref(), &changed_snapshot);
        }
        _ => panic!("expected audio automation state event"),
    }

    let no_op = dispatch(
        &mut dispatcher,
        "automation-no-op",
        EngineCommand::ExecuteAudioAutomation(transaction(
            1,
            vec![AudioAutomationMutation::SetKeyframe {
                target: target(),
                keyframe: keyframe(1, 1.0),
            }],
        )),
    )
    .unwrap();
    assert_eq!(no_op.command_sequence(), 3);
    assert_eq!(automation_result(no_op.result()), &changed_snapshot);
    assert!(dispatcher.drain_events().unwrap().is_empty());

    let stale = dispatch(
        &mut dispatcher,
        "automation-stale",
        EngineCommand::ExecuteAudioAutomation(transaction(
            0,
            vec![AudioAutomationMutation::SetMode {
                target: target(),
                mode: AudioAutomationMode::Write,
            }],
        )),
    )
    .unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(
        dispatcher.audio_automation_snapshot().unwrap(),
        changed_snapshot
    );
    assert!(dispatcher.drain_events().unwrap().is_empty());

    let invalid = dispatch(
        &mut dispatcher,
        "automation-invalid",
        EngineCommand::ExecuteAudioAutomation(transaction(
            1,
            vec![
                AudioAutomationMutation::SetMode {
                    target: target(),
                    mode: AudioAutomationMode::Write,
                },
                AudioAutomationMutation::SetControlValue {
                    target: target(),
                    at: at(2),
                    value: 2.0,
                },
            ],
        )),
    )
    .unwrap_err();
    assert_eq!(invalid.category(), ErrorCategory::Conflict);
    assert_eq!(
        dispatcher.audio_automation_snapshot().unwrap(),
        changed_snapshot
    );
    assert!(dispatcher.drain_events().unwrap().is_empty());

    let after_failures = dispatch(
        &mut dispatcher,
        "automation-after-failures",
        EngineCommand::InspectAudioAutomation,
    )
    .unwrap();
    assert_eq!(after_failures.command_sequence(), 4);
    assert_eq!(
        automation_result(after_failures.result()),
        &changed_snapshot
    );

    render_result_snapshot(&changed_snapshot);
}

#[test]
fn event_backpressure_is_reserved_before_mutation_but_not_before_a_no_op() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher
        .attach_audio_automation(AudioAutomationState::new())
        .unwrap();

    dispatch(
        &mut dispatcher,
        "automation-capacity-0",
        EngineCommand::ExecuteAudioAutomation(transaction(
            0,
            vec![AudioAutomationMutation::CreateLane {
                target: target(),
                sample_rate: RATE,
                default_gain: 1.0,
            }],
        )),
    )
    .unwrap();
    for index in 1..MAX_PENDING_ENGINE_EVENTS {
        let mode = if index % 2 == 1 {
            AudioAutomationMode::Write
        } else {
            AudioAutomationMode::Read
        };
        dispatch(
            &mut dispatcher,
            &format!("automation-capacity-{index}"),
            EngineCommand::ExecuteAudioAutomation(transaction(
                u64::try_from(index).unwrap(),
                vec![AudioAutomationMutation::SetMode {
                    target: target(),
                    mode,
                }],
            )),
        )
        .unwrap();
    }
    assert_eq!(
        dispatcher.audio_automation_snapshot().unwrap().revision(),
        u64::try_from(MAX_PENDING_ENGINE_EVENTS).unwrap()
    );

    let no_op = dispatch(
        &mut dispatcher,
        "automation-full-no-op",
        EngineCommand::ExecuteAudioAutomation(transaction(
            u64::try_from(MAX_PENDING_ENGINE_EVENTS).unwrap(),
            vec![AudioAutomationMutation::SetMode {
                target: target(),
                mode: AudioAutomationMode::Write,
            }],
        )),
    )
    .unwrap();
    assert_eq!(
        no_op.command_sequence(),
        u64::try_from(MAX_PENDING_ENGINE_EVENTS + 1).unwrap()
    );

    let before = dispatcher.audio_automation_snapshot().unwrap();
    let full = dispatch(
        &mut dispatcher,
        "automation-full-change",
        EngineCommand::ExecuteAudioAutomation(transaction(
            before.revision(),
            vec![AudioAutomationMutation::SetMode {
                target: target(),
                mode: AudioAutomationMode::Read,
            }],
        )),
    )
    .unwrap_err();
    assert_eq!(full.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(dispatcher.audio_automation_snapshot().unwrap(), before);

    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), MAX_PENDING_ENGINE_EVENTS);
    assert_eq!(events.last().unwrap().sequence(), 64);
    let retried = dispatch(
        &mut dispatcher,
        "automation-retried",
        EngineCommand::ExecuteAudioAutomation(transaction(
            before.revision(),
            vec![AudioAutomationMutation::SetMode {
                target: target(),
                mode: AudioAutomationMode::Read,
            }],
        )),
    )
    .unwrap();
    assert_eq!(retried.command_sequence(), 66);
    assert_eq!(automation_result(retried.result()).revision(), 65);
    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sequence(), 65);
    assert_eq!(events[0].audio_automation_revision(), Some(65));
}

fn render_result_snapshot(automation: &AudioAutomationSnapshot) {
    let stereo = ChannelLayout::stereo();
    let controls = ClipMixControls::new(
        stereo.clone(),
        stereo.clone(),
        [
            ChannelMap::new(ChannelPosition::FrontLeft, ChannelPosition::FrontLeft, 1.0).unwrap(),
            ChannelMap::new(
                ChannelPosition::FrontRight,
                ChannelPosition::FrontRight,
                1.0,
            )
            .unwrap(),
        ],
    )
    .unwrap()
    .with_gain(0.25)
    .unwrap();
    let mut mix = ClipMixState::new();
    mix.apply(0, &[ClipMixMutation::set(CLIP, controls)])
        .unwrap();
    let mut processor = mix
        .snapshot()
        .prepare_processor_with_automation(CLIP, at(0), 2, automation)
        .unwrap();
    let input = [1.0_f32; 4];
    let mut output = [0.0_f32; 4];
    processor
        .process(AudioProcessBlock {
            start_time: at(0),
            frame_count: 2,
            input: Some(&input),
            input_layout: Some(&stereo),
            output: &mut output,
            output_layout: &stereo,
        })
        .unwrap();
    assert_eq!(output, [0.5, 0.5, 1.0, 1.0]);
}
