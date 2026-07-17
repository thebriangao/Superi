use superi_api::audio_automation::{
    AudioAutomationApi, AudioAutomationKeyframe, AudioAutomationMode, AudioAutomationMutation,
    AudioAutomationSampleTime, AudioAutomationTarget,
};
use superi_api::commands::{ApiCommand, ExecuteAudioAutomationTransaction, GetAudioAutomation};
use superi_api::events::{ApiEvent, AudioAutomationChanged};
use superi_api::version::{
    AUDIO_AUTOMATION_CHANGED_EVENT, AUDIO_AUTOMATION_SCHEMA_VERSION,
    EXECUTE_AUDIO_AUTOMATION_TRANSACTION_METHOD, GET_AUDIO_AUTOMATION_METHOD,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::ErrorCategory;
use superi_core::ids::ClipId;
use superi_engine::dispatcher::{AudioAutomationState, EngineCommandDispatcher};

const RATE: u32 = 48_000;
const CLIP: ClipId = ClipId::from_raw(0xa011);

fn at(sample: i64) -> AudioAutomationSampleTime {
    AudioAutomationSampleTime::new(sample, RATE)
}

fn target() -> AudioAutomationTarget {
    AudioAutomationTarget::ClipGain {
        clip_id: CLIP.to_string(),
    }
}

fn keyframe(sample: i64, value: f32) -> AudioAutomationKeyframe {
    AudioAutomationKeyframe::new(at(sample), value)
}

#[test]
fn denied_audio_removal_preserves_replacement_state_and_events() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher
        .attach_audio_automation(AudioAutomationState::new())
        .unwrap();
    let mut api = AudioAutomationApi::new(dispatcher).unwrap();
    let before = api
        .execute_transaction(ExecuteAudioAutomationTransaction::new(
            "permission-create",
            0,
            vec![AudioAutomationMutation::CreateLane {
                target: target(),
                sample_rate: RATE,
                default_gain: 1.0,
            }],
        ))
        .unwrap()
        .snapshot()
        .clone();
    api.drain_events().unwrap();

    let failure = api
        .execute_transaction(ExecuteAudioAutomationTransaction::new(
            "permission-remove-denied",
            before.revision(),
            vec![AudioAutomationMutation::RemoveLane { target: target() }],
        ))
        .unwrap_err();
    assert_eq!(failure.category(), ErrorCategory::PermissionDenied);
    assert!(api.drain_events().unwrap().is_empty());
    let after = api
        .execute(GetAudioAutomation::new())
        .unwrap()
        .snapshot()
        .clone();
    assert_eq!(after, before);
}

#[test]
fn one_strict_public_surface_controls_observes_and_emits_audio_automation() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher
        .attach_audio_automation(AudioAutomationState::new())
        .unwrap();
    let mut api = AudioAutomationApi::new(dispatcher).unwrap();

    let initial = api.execute(GetAudioAutomation::new()).unwrap();
    assert_eq!(
        initial.snapshot().schema_version(),
        &AUDIO_AUTOMATION_SCHEMA_VERSION
    );
    assert_eq!(initial.snapshot().revision(), 0);
    assert!(initial.snapshot().lanes().is_empty());

    let command = ExecuteAudioAutomationTransaction::new(
        "public-audio-automation",
        0,
        vec![
            AudioAutomationMutation::CreateLane {
                target: target(),
                sample_rate: RATE,
                default_gain: 0.0,
            },
            AudioAutomationMutation::SetKeyframe {
                target: target(),
                keyframe: keyframe(0, 0.0),
            },
            AudioAutomationMutation::SetKeyframe {
                target: target(),
                keyframe: keyframe(10, 1.0),
            },
            AudioAutomationMutation::SetMode {
                target: target(),
                mode: AudioAutomationMode::Touch,
            },
        ],
    );
    let wire = serde_json::to_value(&command).unwrap();
    assert_eq!(wire["transaction_id"], "public-audio-automation");
    assert_eq!(wire["expected_revision"], 0);
    assert_eq!(wire["mutations"][0]["operation"], "create_lane");
    assert_eq!(wire["mutations"][0]["target"]["kind"], "clip_gain");
    assert_eq!(wire["mutations"][0]["target"]["clip_id"], CLIP.to_string());
    let mut unknown = wire.clone();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ExecuteAudioAutomationTransaction>(unknown).is_err());
    let command = serde_json::from_value(wire).unwrap();

    let result = api.execute_transaction(command).unwrap();
    assert_eq!(result.transaction_id(), "public-audio-automation");
    assert_eq!(result.command_sequence(), 2);
    assert_eq!(result.snapshot().revision(), 1);
    let lane = result.snapshot().lane(&target()).unwrap();
    assert_eq!(lane.sample_rate(), RATE);
    assert_eq!(lane.default_gain(), 0.0);
    assert_eq!(lane.mode(), AudioAutomationMode::Touch);
    assert_eq!(lane.keyframes(), [keyframe(0, 0.0), keyframe(10, 1.0)]);
    assert!(lane.active_pass().is_none());

    let events = api.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sequence(), 1);
    assert_eq!(events[0].command_sequence(), result.command_sequence());
    assert_eq!(events[0].transaction_id(), "public-audio-automation");
    assert_eq!(events[0].audio_automation_revision(), 1);
    assert_eq!(events[0].snapshot(), result.snapshot());
    let event_wire = serde_json::to_value(&events[0]).unwrap();
    let round_trip: AudioAutomationChanged = serde_json::from_value(event_wire).unwrap();
    assert_eq!(round_trip, events[0]);

    let pass = ExecuteAudioAutomationTransaction::new(
        "public-touch-pass",
        1,
        vec![
            AudioAutomationMutation::BeginPass {
                target: target(),
                at: at(2),
                current_value: 0.2,
            },
            AudioAutomationMutation::BeginTouch {
                target: target(),
                at: at(4),
                value: 2.0,
            },
            AudioAutomationMutation::SetControlValue {
                target: target(),
                at: at(6),
                value: 4.0,
            },
            AudioAutomationMutation::EndTouch {
                target: target(),
                at: at(8),
            },
            AudioAutomationMutation::EndPass {
                target: target(),
                at: at(9),
            },
        ],
    );
    let pass = api.execute_transaction(pass).unwrap();
    assert_eq!(pass.snapshot().revision(), 2);
    assert_eq!(
        pass.snapshot().lane(&target()).unwrap().mode(),
        AudioAutomationMode::Touch
    );
    assert!(pass
        .snapshot()
        .lane(&target())
        .unwrap()
        .active_pass()
        .is_none());
    assert_eq!(api.drain_events().unwrap().len(), 1);

    let no_op = api
        .execute_transaction(ExecuteAudioAutomationTransaction::new(
            "public-audio-no-op",
            2,
            vec![AudioAutomationMutation::SetMode {
                target: target(),
                mode: AudioAutomationMode::Touch,
            }],
        ))
        .unwrap();
    assert_eq!(no_op.command_sequence(), 4);
    assert_eq!(no_op.snapshot().revision(), 2);
    assert!(api.drain_events().unwrap().is_empty());
}

#[test]
fn public_json_is_strict_and_covers_every_supported_mutation() {
    let mutations = vec![
        AudioAutomationMutation::CreateLane {
            target: target(),
            sample_rate: RATE,
            default_gain: 1.0,
        },
        AudioAutomationMutation::RemoveLane { target: target() },
        AudioAutomationMutation::SetMode {
            target: target(),
            mode: AudioAutomationMode::Read,
        },
        AudioAutomationMutation::SetMode {
            target: target(),
            mode: AudioAutomationMode::Write,
        },
        AudioAutomationMutation::SetMode {
            target: target(),
            mode: AudioAutomationMode::Touch,
        },
        AudioAutomationMutation::SetMode {
            target: target(),
            mode: AudioAutomationMode::Latch,
        },
        AudioAutomationMutation::SetKeyframe {
            target: target(),
            keyframe: keyframe(-1, 0.5),
        },
        AudioAutomationMutation::RemoveKeyframe {
            target: target(),
            at: at(-1),
        },
        AudioAutomationMutation::BeginPass {
            target: target(),
            at: at(0),
            current_value: 1.0,
        },
        AudioAutomationMutation::BeginTouch {
            target: target(),
            at: at(1),
            value: 2.0,
        },
        AudioAutomationMutation::SetControlValue {
            target: target(),
            at: at(2),
            value: 3.0,
        },
        AudioAutomationMutation::EndTouch {
            target: target(),
            at: at(3),
        },
        AudioAutomationMutation::EndPass {
            target: target(),
            at: at(4),
        },
    ];
    let wire = serde_json::to_value(&mutations).unwrap();
    let round_trip: Vec<AudioAutomationMutation> = serde_json::from_value(wire).unwrap();
    assert_eq!(round_trip, mutations);

    let malformed_id = serde_json::json!({
        "transaction_id": "malformed-id",
        "expected_revision": 0,
        "mutations": [{
            "operation": "remove_lane",
            "target": {"kind": "clip_gain", "clip_id": "clip:ABC"}
        }]
    });
    assert!(serde_json::from_value::<ExecuteAudioAutomationTransaction>(malformed_id).is_err());
    let unknown_target_field = serde_json::json!({
        "kind": "clip_gain",
        "clip_id": CLIP.to_string(),
        "extra": true
    });
    assert!(serde_json::from_value::<AudioAutomationTarget>(unknown_target_field).is_err());
    let unknown_mutation_field = serde_json::json!({
        "operation": "remove_lane",
        "target": {"kind": "clip_gain", "clip_id": CLIP.to_string()},
        "extra": true
    });
    assert!(serde_json::from_value::<AudioAutomationMutation>(unknown_mutation_field).is_err());
}

#[test]
fn audio_automation_names_are_permanent_namespaced_contracts() {
    assert_eq!(GetAudioAutomation::METHOD, GET_AUDIO_AUTOMATION_METHOD);
    assert_eq!(
        ExecuteAudioAutomationTransaction::METHOD,
        EXECUTE_AUDIO_AUTOMATION_TRANSACTION_METHOD
    );
    assert_eq!(AudioAutomationChanged::NAME, AUDIO_AUTOMATION_CHANGED_EVENT);
    assert_eq!(GET_AUDIO_AUTOMATION_METHOD, "superi.audio.automation.get");
    assert_eq!(
        EXECUTE_AUDIO_AUTOMATION_TRANSACTION_METHOD,
        "superi.audio.automation.transaction.execute"
    );
    assert_eq!(
        AUDIO_AUTOMATION_CHANGED_EVENT,
        "superi.audio.automation.changed"
    );
    assert_eq!(AUDIO_AUTOMATION_SCHEMA_VERSION.to_string(), "1.0.0");
}

#[test]
fn public_conversion_failures_emit_nothing_and_leave_state_unchanged() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher
        .attach_audio_automation(AudioAutomationState::new())
        .unwrap();
    let mut api = AudioAutomationApi::new(dispatcher).unwrap();
    let invalid = api
        .execute_transaction(ExecuteAudioAutomationTransaction::new(
            "bad-clock",
            0,
            vec![AudioAutomationMutation::CreateLane {
                target: target(),
                sample_rate: 0,
                default_gain: 1.0,
            }],
        ))
        .unwrap_err();
    assert_eq!(invalid.category(), ErrorCategory::InvalidInput);
    assert!(api.drain_events().unwrap().is_empty());
    assert_eq!(
        api.execute(GetAudioAutomation::new())
            .unwrap()
            .snapshot()
            .revision(),
        0
    );
}
