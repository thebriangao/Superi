use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};
use superi_test_report::{generate_report, ReportError};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Self {
        let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "superi-test-report-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn base_input() -> Value {
    json!({
        "schema_version": 1,
        "matrix_revision": 1,
        "lane_id": "hw-linux-amd",
        "suites": ["performance", "fixtures", "codecs"],
        "blocking": true,
        "source": {
            "commit_sha": "0123456789abcdef0123456789abcdef01234567",
            "dirty": false,
            "build_profile": "release",
            "rust_version": "1.80.0",
            "cargo_lock_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "enabled_features": ["os-codecs"]
        },
        "fixtures": {
            "manifest_revision": "policy/utf8/v1",
            "reference_project_ids": ["project/reference-v1"],
            "reference_media_ids": ["media/reference-v1"],
            "expected_output_revision": "golden-v3"
        },
        "platform": {
            "os_name": "ubuntu",
            "os_edition": "lts",
            "os_version": "26.04",
            "os_build": "202604",
            "kernel": "6.14.0",
            "architecture": "x86_64",
            "cpu_model": "amd reference",
            "logical_cores": 16,
            "memory_bytes": 34359738368_u64,
            "hardware_tier": "recommended",
            "gpu": {
                "vendor": "amd",
                "model": "reference gpu",
                "device_id": "1002:0001",
                "backend": "vulkan",
                "driver_version": "1.0",
                "adapter_limits": "wgpu-default",
                "display_path": "physical"
            },
            "audio": null,
            "codec_backends": [{
                "identity": "vaapi",
                "operations": ["decode"],
                "acceleration": "hardware",
                "version": "1.22"
            }],
            "cache": {
                "state": "warm",
                "size_bytes": 1048576,
                "storage_medium": "nvme"
            }
        },
        "timing": {
            "started_at": "2026-07-14T00:00:00Z",
            "ended_at": "2026-07-14T00:01:00Z",
            "duration_ms": 60000
        },
        "artifacts": [{"id": "raw-log", "uri": "artifacts/raw.log"}],
        "tests": []
    })
}

fn attempt(number: u32, status: &str, reason: Option<&str>) -> Value {
    json!({
        "number": number,
        "status": status,
        "duration_ms": 10,
        "seed": "seed-7",
        "reason": reason,
        "artifacts": []
    })
}

#[test]
fn creates_deterministic_reports_with_all_required_finding_categories() {
    let mut input = base_input();
    input["tests"] = json!([
        {
            "id": "performance.playback-start",
            "suite": "performance",
            "command": "cargo test playback_start",
            "attempts": [attempt(1, "pass", None)],
            "evidence": [{
                "kind": "performance",
                "metric": "warm_playback_start_ms",
                "unit": "milliseconds",
                "baseline": 100.0,
                "observed": 125.0,
                "allowed_regression_fraction": 0.10,
                "direction": "lower_is_better"
            }]
        },
        {
            "id": "fixtures.graph-golden",
            "suite": "fixtures",
            "command": "cargo test graph_golden",
            "attempts": [attempt(1, "fail", Some("image mismatch"))],
            "evidence": [{
                "kind": "golden",
                "expected_revision": "golden-v3",
                "actual_revision": "sha256:bbbb",
                "tolerance": 0.001,
                "compared_samples": 4096,
                "mismatched_samples": 3,
                "maximum_absolute_error": 0.125,
                "first_mismatch": "x=4,y=2,channel=red",
                "artifacts": [{"id": "diff", "uri": "artifacts/diff.exr"}]
            }]
        },
        {
            "id": "codecs.vaapi-probe",
            "suite": "codecs",
            "command": "cargo test vaapi_probe",
            "attempts": [
                attempt(1, "fail", Some("driver initialization failed")),
                attempt(2, "pass", None)
            ],
            "evidence": []
        },
        {
            "id": "codecs.vvc-encode",
            "suite": "codecs",
            "command": "cargo test vvc_encode",
            "attempts": [attempt(1, "skip", Some("driver does not advertise VVC encode"))],
            "evidence": [{
                "kind": "platform_gap",
                "capability": "vvc_encode",
                "expected": "available",
                "observed": "unavailable",
                "reason": "driver does not advertise VVC encode"
            }]
        }
    ]);

    let first = generate_report(&serde_json::to_vec(&input).unwrap()).unwrap();
    let second = generate_report(&serde_json::to_vec(&input).unwrap()).unwrap();
    assert_eq!(
        first.canonical_json().unwrap(),
        second.canonical_json().unwrap()
    );

    let report: Value = serde_json::from_slice(&first.canonical_json().unwrap()).unwrap();
    assert_eq!(report["status"], "fail");
    assert_eq!(report["summary"]["passed"], 2);
    assert_eq!(report["summary"]["failed"], 1);
    assert_eq!(report["summary"]["skipped"], 1);
    assert_eq!(report["summary"]["gaps"], 1);
    assert_eq!(report["summary"]["performance_regressions"], 1);
    assert_eq!(report["summary"]["golden_mismatches"], 1);
    assert_eq!(report["summary"]["flaky_tests"], 1);
    assert_eq!(report["summary"]["platform_gaps"], 1);
    let flaky = report["tests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|test| test["id"] == "codecs.vaapi-probe")
        .unwrap();
    assert_eq!(flaky["attempts"].as_array().unwrap().len(), 2);
    let categories = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding["category"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        categories,
        [
            "flaky_test",
            "platform_gap",
            "golden_mismatch",
            "performance_regression"
        ]
    );
}

#[test]
fn missing_suites_and_unexplained_skips_become_platform_gaps() {
    let mut input = base_input();
    input["tests"] = json!([{
        "id": "performance.unavailable",
        "suite": "performance",
        "command": "cargo test performance",
        "attempts": [attempt(1, "skip", Some("benchmark is not implemented"))],
        "evidence": []
    }]);

    let report = generate_report(&serde_json::to_vec(&input).unwrap()).unwrap();
    let value: Value = serde_json::from_slice(&report.canonical_json().unwrap()).unwrap();
    assert_eq!(value["summary"]["platform_gaps"], 3);
    assert!(value["findings"].as_array().unwrap().iter().any(|finding| {
        finding["test_id"] == "suite:fixtures"
            && finding["detail"]["reason"] == "declared suite produced no test evidence"
    }));
}

#[test]
fn rejects_noncanonical_or_contradictory_evidence() {
    let mut input = base_input();
    input["tests"] = json!([{
        "id": "performance.bad",
        "suite": "performance",
        "command": "cargo test performance",
        "attempts": [attempt(2, "pass", None)],
        "evidence": []
    }]);

    let error = generate_report(&serde_json::to_vec(&input).unwrap()).unwrap_err();
    assert!(matches!(error, ReportError::Validation(_)));
    assert!(error
        .to_string()
        .contains("attempt numbers must start at 1"));
}

#[test]
fn rejects_unbounded_performance_thresholds() {
    let mut input = base_input();
    input["tests"] = json!([{
        "id": "performance.bad-threshold",
        "suite": "performance",
        "command": "cargo test performance",
        "attempts": [attempt(1, "pass", None)],
        "evidence": [{
            "kind": "performance",
            "metric": "throughput",
            "unit": "frames_per_second",
            "baseline": 60.0,
            "observed": 30.0,
            "allowed_regression_fraction": 1.5,
            "direction": "higher_is_better"
        }]
    }]);

    let error = generate_report(&serde_json::to_vec(&input).unwrap()).unwrap_err();
    assert!(error
        .to_string()
        .contains("allowed regression fraction cannot exceed 1"));
}

#[test]
fn canonical_example_builds_a_clean_report() {
    let report = generate_report(include_bytes!("fixtures/passing-lane.json")).unwrap();
    assert!(!report.has_blocking_findings());
    let value: Value = serde_json::from_slice(&report.canonical_json().unwrap()).unwrap();
    assert_eq!(value["status"], "pass");
    assert_eq!(value["summary"]["passed"], 1);
    assert!(value["findings"].as_array().unwrap().is_empty());
}

#[test]
fn cli_writes_blocking_report_before_returning_failure_and_never_overwrites_it() {
    let mut input = base_input();
    input["tests"] = json!([{
        "id": "performance.regression",
        "suite": "performance",
        "command": "cargo test performance",
        "attempts": [attempt(1, "pass", None)],
        "evidence": [{
            "kind": "performance",
            "metric": "latency_ms",
            "unit": "milliseconds",
            "baseline": 10.0,
            "observed": 20.0,
            "allowed_regression_fraction": 0.05,
            "direction": "lower_is_better"
        }]
    }, {
        "id": "fixtures.pass",
        "suite": "fixtures",
        "command": "cargo test fixtures",
        "attempts": [attempt(1, "pass", None)],
        "evidence": []
    }, {
        "id": "codecs.pass",
        "suite": "codecs",
        "command": "cargo test codecs",
        "attempts": [attempt(1, "pass", None)],
        "evidence": []
    }]);
    let root = TempDir::new();
    let input_path = root.0.join("input.json");
    let output_path = root.0.join("report.json");
    fs::write(&input_path, serde_json::to_vec(&input).unwrap()).unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_superi-test-report"))
        .arg("build")
        .arg(&input_path)
        .arg(&output_path)
        .output()
        .unwrap();
    assert_eq!(first.status.code(), Some(1));
    assert!(output_path.is_file());
    let report: Value = serde_json::from_slice(&fs::read(&output_path).unwrap()).unwrap();
    assert_eq!(report["status"], "fail");
    assert_eq!(report["summary"]["performance_regressions"], 1);

    let original = fs::read(&output_path).unwrap();
    let second = Command::new(env!("CARGO_BIN_EXE_superi-test-report"))
        .arg("build")
        .arg(&input_path)
        .arg(&output_path)
        .output()
        .unwrap();
    assert_eq!(second.status.code(), Some(2));
    assert_eq!(fs::read(&output_path).unwrap(), original);
    assert!(String::from_utf8(second.stderr)
        .unwrap()
        .contains("refusing to overwrite"));
}
