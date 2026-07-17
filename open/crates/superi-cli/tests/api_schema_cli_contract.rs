use std::process::{Command, Output};

use serde_json::Value;

#[test]
fn api_schema_is_a_deterministic_complete_public_api_consumer() {
    let first = run(&["api", "schema"]);
    let second = run(&["api", "schema"]);
    assert_success(&first);
    assert_success(&second);
    assert!(first.stderr.is_empty());
    assert!(second.stderr.is_empty());

    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    let second: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(first, second);
    assert_eq!(first["schema_version"], "1.0.0");
    assert_eq!(first["primitive_schema_revision"], 1);
    assert_eq!(first["json_rpc_version"], "2.0");
    assert_eq!(first["commands"].as_array().unwrap().len(), 6);
    assert_eq!(first["queries"].as_array().unwrap().len(), 8);
    assert_eq!(first["events"].as_array().unwrap().len(), 6);
    assert_eq!(first["resources"].as_array().unwrap().len(), 7);
    assert_eq!(first["error"]["schema"]["version"], "1.0.0");
    assert_eq!(
        first["capability"]["availability"]
            .as_array()
            .unwrap()
            .len(),
        4
    );

    assert_eq!(
        names(&first, "commands", "method"),
        vec![
            "superi.audio.automation.transaction.execute",
            "superi.project.recovery.dismiss",
            "superi.project.recovery.restore",
            "superi.project.settings.transaction.execute",
            "superi.slice.scenario.action.execute",
            "superi.slice.scenario.transaction.execute",
        ]
    );
    assert_eq!(
        names(&first, "queries", "method"),
        vec![
            "superi.api.schema.get",
            "superi.audio.automation.get",
            "superi.engine.integration.validation.get",
            "superi.engine.introspection.get",
            "superi.media.capabilities.get",
            "superi.project.recovery.compare",
            "superi.project.recovery.get",
            "superi.project.settings.get",
        ]
    );
}

#[test]
fn api_schema_help_and_invalid_usage_remain_precise() {
    let help = run(&["--help"]);
    assert_success(&help);
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("superi-cli api schema"));

    let invalid = run(&["api"]);
    assert_eq!(invalid.status.code(), Some(2));
    let failure: Value = serde_json::from_slice(&invalid.stderr).unwrap();
    assert_eq!(failure["category"], "invalid_input");
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_superi-cli"))
        .args(arguments)
        .output()
        .unwrap()
}

fn names<'a>(value: &'a Value, section: &str, field: &str) -> Vec<&'a str> {
    value[section]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry[field].as_str().unwrap())
        .collect()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
