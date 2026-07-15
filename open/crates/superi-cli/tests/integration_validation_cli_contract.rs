use std::process::{Command, Output};

use serde_json::Value;

#[test]
fn engine_validate_is_a_strict_deterministic_public_api_consumer() {
    let first = run(&["engine", "validate"]);
    let second = run(&["engine", "validate"]);
    assert_success(&first);
    assert_success(&second);
    assert!(first.stderr.is_empty());
    assert!(second.stderr.is_empty());

    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    let second: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(first, second);
    assert_eq!(first["snapshot"]["schema_version"], "1.0.0");
    assert_eq!(first["snapshot"]["condition"], "starting");
    assert_eq!(first["snapshot"]["coherent"], true);
    assert_eq!(first["snapshot"]["engine"]["phase"], "starting");
    assert!(
        !first["snapshot"]["engine"]["media_capabilities"]["backends"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        first["snapshot"]["pending_action"]["subsystem"],
        "shared_state"
    );
    assert_eq!(first["snapshot"]["pending_action"]["kind"], "initialize");
    assert_eq!(first["snapshot"]["workflows"].as_array().unwrap().len(), 3);
    assert!(first["snapshot"]["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .all(|workflow| workflow["permit"].is_null() && !workflow["denial"].is_null()));
    assert_eq!(first["snapshot"]["playback"]["attached"], false);
    assert_eq!(first["snapshot"]["export"]["attached"], false);
    assert!(first["snapshot"]["findings"].as_array().unwrap().is_empty());
}

#[test]
fn engine_validate_help_and_invalid_usage_remain_precise() {
    let help = run(&["--help"]);
    assert_success(&help);
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("superi-cli engine validate"));

    let invalid = run(&["engine"]);
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

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
