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

    assert!(report.checked_packages >= 19);
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
            vec![dependency("superi-media-io", Some("dev"))],
        ),
        package("superi-media-io", vec![]),
    ]);
    let runtime = metadata(vec![
        package("superi-api", vec![dependency("superi-media-io", None)]),
        package("superi-media-io", vec![]),
    ]);

    validate_metadata(&dev).expect("the reviewed API contract-test edge is valid");
    assert!(validate_metadata(&runtime).is_err());
}

#[test]
fn unknown_runtime_crates_require_an_explicit_policy_decision() {
    let input = metadata(vec![package("superi-surprise", vec![])]);

    let error = validate_metadata(&input).expect_err("new runtime crate must fail closed");

    assert!(error
        .to_string()
        .contains("unknown runtime crate superi-surprise"));
}
