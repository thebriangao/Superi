use std::path::Path;

use serde_json::{json, Value};
use superi_dependency_check::{check_workspace, validate_metadata};

fn metadata(packages: Vec<Value>) -> String {
    json!({
        "packages": packages,
        "workspace_root": "/workspace/open"
    })
    .to_string()
}

fn package(name: &str, dependencies: Vec<Value>) -> Value {
    json!({
        "name": name,
        "manifest_path": format!("/workspace/open/crates/{name}/Cargo.toml"),
        "dependencies": dependencies
    })
}

fn dependency(name: &str, kind: Option<&str>) -> Value {
    json!({
        "name": name,
        "kind": kind,
        "path": format!("/workspace/open/crates/{name}")
    })
}

#[test]
fn current_workspace_obeys_the_documented_direction() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report = check_workspace(&workspace).expect("the checked-in workspace must obey the DAG");

    assert!(report.checked_packages >= 22);
    assert!(report.checked_internal_edges >= 1);
}

#[test]
fn unauthorized_runtime_and_build_edges_are_rejected() {
    let runtime = metadata(vec![
        package("superi-graph", vec![dependency("superi-color", None)]),
        package("superi-color", vec![]),
    ]);
    let build = metadata(vec![
        package(
            "superi-core",
            vec![dependency("superi-engine", Some("build"))],
        ),
        package("superi-engine", vec![]),
    ]);

    let runtime_error = validate_metadata(&runtime).expect_err("upward runtime edge must fail");
    let build_error = validate_metadata(&build).expect_err("upward build edge must fail");

    assert!(runtime_error
        .to_string()
        .contains("superi-graph -> superi-color"));
    assert!(build_error
        .to_string()
        .contains("superi-core -> superi-engine"));
}

#[test]
fn reviewed_dev_edges_do_not_authorize_production_edges() {
    let dev = metadata(vec![
        package(
            "superi-api",
            vec![
                dependency("superi-media-io", Some("dev")),
                dependency("superi-concurrency", Some("dev")),
            ],
        ),
        package("superi-media-io", vec![]),
        package("superi-concurrency", vec![]),
    ]);
    let runtime = metadata(vec![
        package(
            "superi-api",
            vec![
                dependency("superi-media-io", None),
                dependency("superi-concurrency", None),
            ],
        ),
        package("superi-media-io", vec![]),
        package("superi-concurrency", vec![]),
    ]);

    validate_metadata(&dev).expect("the reviewed API contract-test edges are valid");
    let runtime_error = validate_metadata(&runtime).expect_err("dev edges must not widen runtime");
    assert!(runtime_error
        .to_string()
        .contains("superi-api -> superi-media-io"));
    assert!(runtime_error
        .to_string()
        .contains("superi-api -> superi-concurrency"));
}

#[test]
fn reviewed_project_audio_edge_preserves_downward_direction() {
    let project_to_audio = metadata(vec![
        package("superi-project", vec![dependency("superi-audio", None)]),
        package("superi-audio", vec![]),
    ]);
    let audio_to_project = metadata(vec![
        package("superi-audio", vec![dependency("superi-project", None)]),
        package("superi-project", vec![]),
    ]);

    validate_metadata(&project_to_audio).expect("project may persist authored audio state");
    let error = validate_metadata(&audio_to_project).expect_err("audio must not depend on project");
    assert!(error.to_string().contains("superi-audio -> superi-project"));
}

#[test]
fn native_presentation_tiers_preserve_session_and_engine_ownership() {
    let reviewed = metadata(vec![
        package(
            "superi-desktop",
            vec![
                dependency("superi-gpu", None),
                dependency("superi-ui", None),
                dependency("superi-session", None),
            ],
        ),
        package("superi-ui", vec![dependency("superi-gpu", None)]),
        package(
            "superi-session",
            vec![
                dependency("superi-api", None),
                dependency("superi-engine", None),
            ],
        ),
        package("superi-gpu", vec![]),
        package("superi-api", vec![]),
        package("superi-engine", vec![]),
    ]);
    validate_metadata(&reviewed).expect("the reviewed native presentation tiers are valid");

    let upward = metadata(vec![
        package("superi-ui", vec![dependency("superi-session", None)]),
        package("superi-session", vec![]),
    ]);
    let error = validate_metadata(&upward).expect_err("retained UI must not own session services");
    assert!(error.to_string().contains("superi-ui -> superi-session"));
}

#[test]
fn unknown_runtime_crates_require_an_explicit_policy_decision() {
    let input = metadata(vec![package("superi-surprise", vec![])]);

    let error = validate_metadata(&input).expect_err("new runtime crate must fail closed");

    assert!(error
        .to_string()
        .contains("unknown runtime crate superi-surprise"));
}
