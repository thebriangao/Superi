use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use sha2::{Digest, Sha256};
use superi_api::commands::{ApiCommand, ExecuteScenarioAction};
use superi_api::scenario::{
    ScenarioApi, ScenarioDocument, ScenarioPhase, SliceActionKind, SliceImplementation,
    MAX_SCENARIO_ACTIONS,
};
use superi_api::version::{EXECUTE_SCENARIO_ACTION_METHOD, SLICE_SCENARIO_SCHEMA_VERSION};

#[test]
fn canonical_schema_is_strict_versioned_and_uses_one_typed_api_method() {
    let directory = test_directory("schema");
    let source = directory.join("input.webm");
    fs::write(&source, b"canonical api fixture").unwrap();
    let value = canonical_scenario_value(&source);
    let scenario: ScenarioDocument = serde_json::from_value(value.clone()).unwrap();

    assert_eq!(scenario.schema_version(), &SLICE_SCENARIO_SCHEMA_VERSION);
    assert_eq!(scenario.actions().len(), 9);
    assert_eq!(scenario.actions()[0].kind(), SliceActionKind::ImportClip);
    assert_eq!(scenario.actions()[8].kind(), SliceActionKind::Inspect);
    assert_eq!(
        ExecuteScenarioAction::METHOD,
        EXECUTE_SCENARIO_ACTION_METHOD
    );
    scenario.validate().unwrap();
    assert_eq!(serde_json::to_value(&scenario).unwrap(), value);

    let mut unknown = value.clone();
    unknown["actions"][0]["guessed"] = json!(true);
    assert!(serde_json::from_value::<ScenarioDocument>(unknown).is_err());

    let mut unknown_history_field = value;
    unknown_history_field["actions"][4]["count"] = json!(1);
    assert!(serde_json::from_value::<ScenarioDocument>(unknown_history_field).is_err());
}

#[test]
fn api_projects_complete_canonical_state_and_reversible_operation_evidence() {
    let directory = test_directory("projection");
    let source = directory.join("input.webm");
    fs::write(&source, b"canonical api fixture").unwrap();
    let scenario: ScenarioDocument =
        serde_json::from_value(canonical_scenario_value(&source)).unwrap();
    let mut api = ScenarioApi::new();
    let mut results = Vec::new();

    for action in scenario.actions() {
        results.push(
            api.execute(ExecuteScenarioAction::new(action.clone()))
                .unwrap(),
        );
    }

    assert_eq!(results[3].state().phase(), ScenarioPhase::Effected);
    assert_eq!(results[5].state().phase(), ScenarioPhase::Placed);
    assert_eq!(results[7].state().phase(), ScenarioPhase::Effected);
    assert_eq!(results[8].action(), SliceActionKind::Inspect);
    assert_eq!(results[8].state().revision(), 8);

    let state = results[8].state();
    assert_eq!(state.schema_version(), &SLICE_SCENARIO_SCHEMA_VERSION);
    assert_eq!(
        state.project_id(),
        "project:00000000000000000000000000000001"
    );
    let media = state.media().unwrap();
    assert_eq!(media.media_id(), "media:00000000000000000000000000000001");
    assert_eq!(media.fixture_id(), "slice/video-cfr");
    assert_eq!(media.fixture_version(), 1);
    assert_eq!(media.path(), source.to_str().unwrap());
    assert_eq!(media.frame_rate().numerator(), 24);
    assert_eq!(media.frame_rate().denominator(), 1);
    assert_eq!(media.frame_count(), 96);
    assert_eq!(media.extent(), (96, 54));

    let timeline = state.timeline().unwrap();
    assert_eq!(timeline.timeline_name(), "canonical");
    assert_eq!(timeline.track_name(), "V1");
    assert_eq!(timeline.clip_name(), "clip-1");
    assert_eq!(timeline.source_range(), (24, 72));
    assert_eq!(timeline.timeline_range(), (0, 48));
    assert_eq!(timeline.canvas(), (96, 54));

    let graph = state.graph().unwrap();
    assert_eq!(graph.nodes().len(), 3);
    assert_eq!(graph.edges().len(), 2);
    assert_eq!(graph.nodes()[1].instance_id(), "slice.node.effect");
    assert_eq!(graph.nodes()[1].node_type(), "superi.effect.transform");
    assert_eq!(graph.nodes()[1].input_ports(), ["image"]);
    assert_eq!(graph.nodes()[1].output_ports(), ["image"]);
    assert_eq!(graph.edges()[0].from_node(), "slice.node.source");
    assert_eq!(graph.edges()[0].to_node(), "slice.node.effect");
    assert_eq!(graph.effect().code(), "horizontal_mirror");
    assert_eq!(
        graph.matrix(),
        [-1.0, 0.0, 95.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(graph.sampling(), "nearest");
    assert_eq!(graph.edge_mode(), "transparent_black");
    assert_eq!(graph.output_extent(), (96, 54));
    assert_eq!(graph.derived_timeline_identity(), "canonical/V1/clip-1");
    assert_eq!(graph.implementation(), SliceImplementation::Reference);

    assert_eq!(state.operation_log().len(), 4);
    assert_eq!(state.operation_log()[0].operation_id(), "slice.op.import");
    assert_eq!(state.operation_log()[3].operation_id(), "slice.op.effect");
    assert_eq!(state.operation_log()[3].resulting_revision(), 4);
    assert_eq!(state.undo_depth(), 4);
    assert_eq!(state.redo_depth(), 0);

    let encoded = serde_json::to_value(&results[8]).unwrap();
    assert_eq!(encoded["action"], "inspect");
    assert_eq!(encoded["state"]["phase"], "effected");
    assert_eq!(encoded["state"]["graph"]["matrix"][2], 95.0);
    assert_eq!(
        encoded["state"]["operation_log"][2]["operation_id"],
        "slice.op.trim"
    );
    assert_eq!(encoded["state"]["schema_version"], "1.0.0");
    assert!(encoded["state"].get("export").is_none());
}

#[test]
fn api_failures_are_structured_and_retain_the_last_valid_state() {
    let mut api = ScenarioApi::new();
    let action = serde_json::from_value(json!({
        "schema_version": "1.0.0",
        "actions": [{
            "action": "trim_clip",
            "source_start_frame": 24,
            "source_end_frame": 72
        }]
    }))
    .map(|scenario: ScenarioDocument| scenario.actions()[0].clone())
    .unwrap();

    let error = api.execute(ExecuteScenarioAction::new(action)).unwrap_err();
    assert_eq!(error.category(), "conflict");
    assert_eq!(error.recoverability(), "user_correctable");
    assert_eq!(error.state().phase(), ScenarioPhase::Empty);
    assert_eq!(error.state().revision(), 0);
    assert!(error
        .contexts()
        .iter()
        .any(|context| context.component() == "superi-engine.command"));

    let encoded = serde_json::to_value(error).unwrap();
    assert_eq!(encoded["category"], "conflict");
    assert_eq!(encoded["recoverability"], "user_correctable");
    assert_eq!(encoded["state"]["phase"], "empty");
}

#[test]
fn scenario_validation_bounds_actions_and_requires_the_current_schema() {
    let actions = (0..=MAX_SCENARIO_ACTIONS)
        .map(|_| json!({"action": "inspect"}))
        .collect::<Vec<_>>();
    let oversized: ScenarioDocument = serde_json::from_value(json!({
        "schema_version": "1.0.0",
        "actions": actions
    }))
    .unwrap();
    assert_eq!(
        oversized.validate().unwrap_err().category(),
        "resource_exhausted"
    );

    let wrong_version: ScenarioDocument = serde_json::from_value(json!({
        "schema_version": "2.0.0",
        "actions": [{"action": "inspect"}]
    }))
    .unwrap();
    assert_eq!(
        wrong_version.validate().unwrap_err().category(),
        "unsupported"
    );
}

fn canonical_scenario_value(source: &Path) -> serde_json::Value {
    let bytes = fs::read(source).unwrap();
    json!({
        "schema_version": "1.0.0",
        "actions": [
            {
                "action": "import_clip",
                "path": source.to_str().unwrap(),
                "fixture_id": "slice/video-cfr",
                "fixture_version": 1,
                "manifest_sha256": "1d2b28b5f44c7f86dce50d67b718b0fad967d267d9016961e3d71bb9dab94419",
                "payload_sha256": format!("{:x}", Sha256::digest(bytes)),
                "frame_rate": {"numerator": 24, "denominator": 1},
                "frame_count": 96,
                "width": 96,
                "height": 54
            },
            {"action": "place_clip", "timeline_start_frame": 0},
            {"action": "trim_clip", "source_start_frame": 24, "source_end_frame": 72},
            {"action": "apply_graph_effect", "effect": "horizontal_mirror"},
            {"action": "undo"},
            {"action": "undo"},
            {"action": "redo"},
            {"action": "redo"},
            {"action": "inspect"}
        ]
    })
}

fn test_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "superi-api-p1-w07-c017-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
