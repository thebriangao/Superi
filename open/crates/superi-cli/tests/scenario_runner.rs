use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

const SCENARIO_ID: &str = "superi.slice.canonical.v1";

#[test]
fn runner_executes_the_normalized_slice_and_writes_reproducible_contract_reports() {
    let first = test_directory("first");
    let second = test_directory("second");
    let first_artifacts = first.join("artifacts");
    let second_artifacts = second.join("artifacts");
    let first_report = first.join("report.json");
    let second_report = second.join("report.json");

    let first_output = run_slice(&first_artifacts, &first_report, SCENARIO_ID);
    let second_output = run_slice(&second_artifacts, &second_report, SCENARIO_ID);
    assert_success(&first_output);
    assert_success(&second_output);
    assert!(first_output.stderr.is_empty());
    assert!(second_output.stderr.is_empty());

    let first_value = read_json(&first_report);
    let second_value = read_json(&second_report);
    assert_eq!(first_value["schema_version"], "1.1.0");
    assert_eq!(first_value["scenario_id"], SCENARIO_ID);
    assert_eq!(first_value["scenario_revision"], 1);
    assert_eq!(first_value["success"], true);
    assert_eq!(first_value["conformance"], "contract");
    assert_eq!(first_value["fixture"]["fixture_id"], "slice/video-cfr");
    assert_eq!(first_value["fixture"]["fixture_version"], 1);
    assert_eq!(first_value["fixture"]["payload"]["frame_count"], 96);
    assert_eq!(first_value["fixture"]["payload"]["width"], 96);
    assert_eq!(first_value["fixture"]["payload"]["height"], 54);

    let stages = first_value["stages"].as_array().unwrap();
    assert_eq!(stages.len(), 8);
    assert_eq!(stages[0]["stage_id"], "fixture.resolve");
    assert_eq!(stages[1]["stage_id"], "media.import");
    assert_eq!(stages[2]["stage_id"], "timeline.edit");
    assert_eq!(stages[3]["stage_id"], "timeline.compile");
    assert_eq!(stages[4]["stage_id"], "graph.evaluate");
    assert_eq!(stages[5]["stage_id"], "color.deliver");
    assert_eq!(stages[6]["stage_id"], "media.export");
    assert_eq!(stages[7]["stage_id"], "slice.verify");
    assert_eq!(stages[0]["implementation"], "runtime");
    assert_eq!(stages[1]["implementation"], "stub");
    assert_eq!(stages[7]["implementation"], "runtime");
    assert!(stages.iter().all(|stage| stage["success"] == true));
    assert_instrumentation(&first_value);
    assert_instrumentation(&second_value);

    assert_eq!(
        first_value["state"]["timeline"]["timeline_name"],
        "canonical"
    );
    assert_eq!(first_value["state"]["timeline"]["source_start_frame"], 24);
    assert_eq!(first_value["state"]["timeline"]["source_end_frame"], 72);
    assert_eq!(first_value["state"]["graph"]["matrix"][0], -1.0);
    assert_eq!(first_value["state"]["graph"]["matrix"][2], 95.0);
    assert_eq!(
        first_value["state"]["operation_log"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(first_value["verification"]["undo_redo_recovered"], true);
    let expectations = &first_value["expectations"];
    assert_eq!(expectations["status"], "contract_passed");
    assert_eq!(expectations["identity"]["fixture_id"], "slice/expectations");
    assert_eq!(expectations["identity"]["fixture_version"], 1);
    assert_eq!(
        expectations["identity"]["manifest_sha256"],
        "2566fae77cff603adb686bf9939e6b09bf48d332603e967a0d2794b5c1482652"
    );
    assert_eq!(
        expectations["identity"]["record_sha256"],
        "6d82626024a5b58b9bd91f8763bc05cc568bba71a123c8ff526eab59382a8646"
    );
    assert_eq!(expectations["reference_frames"]["frame_count"], 48);
    assert_eq!(expectations["reference_frames"]["pixel_format"], "rgba8");
    assert_eq!(
        expectations["tolerances"]["pixel"]["maximum_absolute_error"],
        0.001
    );
    assert_eq!(
        expectations["tolerances"]["audio"]["maximum_absolute_error_pcm16"],
        0
    );
    assert_eq!(expectations["audio_samples"]["case_count"], 3);
    assert_eq!(
        expectations["audio_samples"]["maximum_adjacent_delta_pcm16"],
        600
    );
    let expectation_results = expectations["results"].as_array().unwrap();
    assert_eq!(expectation_results.len(), 8);
    assert_result(expectation_results, "record_integrity", "passed");
    assert_result(expectation_results, "reference_frames", "passed");
    assert_result(expectation_results, "audio_samples", "passed");
    assert_result(expectation_results, "timestamps", "passed");
    assert_result(expectation_results, "project_state", "passed");
    assert_result(expectation_results, "export_metadata", "passed");
    assert_result(expectation_results, "rendered_pixels", "not_evaluated");
    assert_result(expectation_results, "rendered_audio", "not_applicable");

    let export = &first_value["export"];
    assert_eq!(export["artifact_kind"], "contract_stub");
    assert_eq!(export["implementation"], "stub");
    assert_eq!(export["playable"], false);
    assert_eq!(export["target_stream"]["codec"], "av1");
    assert_eq!(export["target_stream"]["container"], "webm");
    assert_eq!(export["target_stream"]["frame_count"], 48);
    assert_eq!(
        export["target_stream"]["timestamps"]
            .as_array()
            .unwrap()
            .len(),
        48
    );
    assert!(export["bytes"].as_u64().unwrap() > 0);
    assert_eq!(export["sha256"].as_str().unwrap().len(), 64);

    let first_stub = fs::read(first_artifacts.join("canonical.webm.contract-stub")).unwrap();
    let second_stub = fs::read(second_artifacts.join("canonical.webm.contract-stub")).unwrap();
    assert_eq!(first_stub, second_stub);
    let stub: Value = serde_json::from_slice(&first_stub).unwrap();
    assert_eq!(stub["artifact_kind"], "contract_stub");
    assert_eq!(stub["playable"], false);
    assert_eq!(stub["missing_runtime_owners"].as_array().unwrap().len(), 6);

    assert_eq!(
        normalized_report(first_value),
        normalized_report(second_value)
    );
}

#[test]
fn runner_rejects_unknown_scenarios_and_output_collisions_before_execution() {
    let unknown = test_directory("unknown");
    let output = run_slice(
        &unknown.join("artifacts"),
        &unknown.join("report.json"),
        "guessed",
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(error_kind(&output), "invalid_input");

    let nonempty = test_directory("nonempty");
    let artifacts = nonempty.join("artifacts");
    fs::create_dir(&artifacts).unwrap();
    fs::write(artifacts.join("owned.txt"), b"preserve").unwrap();
    let output = run_slice(&artifacts, &nonempty.join("report.json"), SCENARIO_ID);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(error_kind(&output), "invalid_input");
    assert_eq!(fs::read(artifacts.join("owned.txt")).unwrap(), b"preserve");

    let collision = test_directory("collision");
    let report = collision.join("report.json");
    fs::write(&report, b"preserve").unwrap();
    let output = run_slice(&collision.join("artifacts"), &report, SCENARIO_ID);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(error_kind(&output), "invalid_input");
    assert_eq!(fs::read(report).unwrap(), b"preserve");
}

#[test]
fn runner_has_precise_help_version_and_usage_status() {
    let help = run(&["--help"]);
    assert_success(&help);
    let help_text = String::from_utf8(help.stdout).unwrap();
    assert!(help_text.contains("superi-cli slice run --scenario superi.slice.canonical.v1"));
    assert!(help_text.contains("--artifact-dir <EMPTY_DIRECTORY> --report <REPORT_JSON>"));

    let version = run(&["--version"]);
    assert_success(&version);
    assert_eq!(String::from_utf8(version.stdout).unwrap(), "superi 0.0.0\n");

    let invalid = run(&["slice"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert_eq!(error_kind(&invalid), "invalid_input");
}

fn run_slice(artifact_dir: &Path, report: &Path, scenario: &str) -> Output {
    run(&[
        "slice",
        "run",
        "--scenario",
        scenario,
        "--artifact-dir",
        artifact_dir.to_str().unwrap(),
        "--report",
        report.to_str().unwrap(),
    ])
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_superi-cli"))
        .args(arguments)
        .current_dir(repo_root())
        .output()
        .unwrap()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn normalized_report(mut value: Value) -> Value {
    value["export"].as_object_mut().unwrap().remove("path");
    value["instrumentation"]
        .as_object_mut()
        .unwrap()
        .remove("observed_resident_bytes_max");
    for stage in value["stages"].as_array_mut().unwrap() {
        stage.as_object_mut().unwrap().remove("duration_us");
        stage.as_object_mut().unwrap().remove("memory");
    }
    value
}

fn assert_instrumentation(report: &Value) {
    let stages = report["stages"].as_array().unwrap();
    let instrumentation = &report["instrumentation"];
    assert_eq!(instrumentation["clock"], "monotonic");
    assert_eq!(instrumentation["duration_unit"], "microseconds");
    assert_eq!(instrumentation["memory_metric"], "process_resident_set");
    assert_eq!(instrumentation["memory_unit"], "bytes");
    assert_eq!(instrumentation["sampling"], "stage_boundaries");
    assert_eq!(instrumentation["stage_count"], 8);

    let mut observed_resident_bytes_max = 0;
    for stage in stages {
        assert!(stage["duration_us"].as_u64().is_some());
        let before = stage["memory"]["resident_bytes_before"].as_u64().unwrap();
        let after = stage["memory"]["resident_bytes_after"].as_u64().unwrap();
        assert!(before > 0);
        assert!(after > 0);
        observed_resident_bytes_max = observed_resident_bytes_max.max(before).max(after);
    }

    assert_eq!(
        instrumentation["observed_resident_bytes_max"],
        observed_resident_bytes_max
    );
}

fn error_kind(output: &Output) -> String {
    let failure: Value = serde_json::from_slice(&output.stderr).unwrap();
    failure["category"].as_str().unwrap().to_owned()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_result(results: &[Value], expectation: &str, status: &str) {
    let result = results
        .iter()
        .find(|result| result["expectation"] == expectation)
        .unwrap_or_else(|| panic!("missing expectation result {expectation}"));
    assert_eq!(result["status"], status);
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

fn test_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "superi-cli-p1-w07-c017-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
