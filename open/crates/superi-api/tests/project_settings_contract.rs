use superi_api::commands::{ApiCommand, ExecuteProjectSettingsTransaction, GetProjectSettings};
use superi_api::events::{ApiEvent, ProjectSettingsChanged};
use superi_api::project::{ProjectSettingMutation, ProjectSettingValue, ProjectSettingsApi};
use superi_api::version::{
    EXECUTE_PROJECT_SETTINGS_TRANSACTION_METHOD, GET_PROJECT_SETTINGS_METHOD,
    PROJECT_SETTINGS_CHANGED_EVENT,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::time::Timebase;
use superi_engine::dispatcher::EngineCommandDispatcher;
use superi_engine::test_support::empty_project_document;

const AUDIO_SAMPLE_RATE_KEY: &str = "superi.project.audio.sample_rate_hz";
const AUDIO_OUTPUT_LAYOUT_KEY: &str = "superi.project.audio.output_layout";

const PROJECT: ProjectId = ProjectId::from_raw(11_001);
const ROOT: TimelineId = TimelineId::from_raw(11_002);

#[test]
fn one_strict_public_surface_controls_and_observes_project_settings() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher
        .attach_project(
            empty_project_document(PROJECT, ROOT, Timebase::integer(24).unwrap()).unwrap(),
        )
        .unwrap();
    let mut api = ProjectSettingsApi::new(dispatcher).unwrap();

    let initial = api.execute(GetProjectSettings::new()).unwrap();
    assert_eq!(initial.snapshot().schema_version().to_string(), "1.0.0");
    assert_eq!(initial.snapshot().project_id(), PROJECT.to_string());
    assert_eq!(initial.snapshot().project_revision(), 0);
    assert_eq!(
        initial.snapshot().value(AUDIO_SAMPLE_RATE_KEY),
        Some(&ProjectSettingValue::Integer(48_000))
    );

    let command = ExecuteProjectSettingsTransaction::new(
        "public-project-settings",
        0,
        vec![
            ProjectSettingMutation::Set {
                key: AUDIO_SAMPLE_RATE_KEY.to_owned(),
                value: ProjectSettingValue::Integer(96_000),
            },
            ProjectSettingMutation::Set {
                key: AUDIO_OUTPUT_LAYOUT_KEY.to_owned(),
                value: ProjectSettingValue::Text("surround_7_1".into()),
            },
        ],
    );
    let wire = serde_json::to_value(&command).unwrap();
    assert_eq!(wire["transaction_id"], "public-project-settings");
    assert_eq!(wire["expected_revision"], 0);
    assert_eq!(wire["mutations"].as_array().unwrap().len(), 2);
    let mut unknown = wire.clone();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ExecuteProjectSettingsTransaction>(unknown).is_err());
    let command = serde_json::from_value(wire).unwrap();

    let result = api.execute_transaction(command).unwrap();
    assert_eq!(result.transaction_id(), "public-project-settings");
    assert_eq!(result.command_sequence(), 2);
    assert_eq!(result.snapshot().project_revision(), 1);
    assert_eq!(
        result.snapshot().value(AUDIO_SAMPLE_RATE_KEY),
        Some(&ProjectSettingValue::Integer(96_000))
    );
    assert_eq!(
        result.snapshot().value(AUDIO_OUTPUT_LAYOUT_KEY),
        Some(&ProjectSettingValue::Text("surround_7_1".into()))
    );

    let events = api.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sequence(), 1);
    assert_eq!(events[0].command_sequence(), result.command_sequence());
    assert_eq!(events[0].transaction_id(), "public-project-settings");
    assert_eq!(events[0].project_revision(), 1);
    assert_eq!(events[0].snapshot(), result.snapshot());
    let wire = serde_json::to_value(&events[0]).unwrap();
    let round_trip: ProjectSettingsChanged = serde_json::from_value(wire).unwrap();
    assert_eq!(round_trip, events[0]);
}

#[test]
fn project_settings_names_are_permanent_namespaced_contracts() {
    assert_eq!(GetProjectSettings::METHOD, GET_PROJECT_SETTINGS_METHOD);
    assert_eq!(
        ExecuteProjectSettingsTransaction::METHOD,
        EXECUTE_PROJECT_SETTINGS_TRANSACTION_METHOD
    );
    assert_eq!(ProjectSettingsChanged::NAME, PROJECT_SETTINGS_CHANGED_EVENT);
    assert_eq!(GET_PROJECT_SETTINGS_METHOD, "superi.project.settings.get");
    assert_eq!(
        EXECUTE_PROJECT_SETTINGS_TRANSACTION_METHOD,
        "superi.project.settings.transaction.execute"
    );
    assert_eq!(
        PROJECT_SETTINGS_CHANGED_EVENT,
        "superi.project.settings.changed"
    );
}
