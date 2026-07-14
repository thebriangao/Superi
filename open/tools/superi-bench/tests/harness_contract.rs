use std::cell::Cell;
use std::collections::VecDeque;

use superi_bench::{
    register_graph_evaluation_workload, BenchmarkConfig, BenchmarkContext, BenchmarkContextFields,
    BenchmarkStage, BenchmarkStatus, BenchmarkSuite,
};

fn context() -> BenchmarkContext {
    BenchmarkContext::new(BenchmarkContextFields {
        build: "a11cecd-release".into(),
        operating_system: "test-os 1".into(),
        architecture: "test-arch".into(),
        cpu: "test-cpu".into(),
        memory_mib: 16_384,
        gpu_backend: "test-gpu-backend".into(),
        gpu_driver: "test-gpu-driver".into(),
        cache_state: "cold".into(),
        hardware_tier: "contract".into(),
        fixture_revision: "fixtures-v1".into(),
        project_revision: "project-v1".into(),
    })
    .unwrap()
}

#[test]
fn exposes_every_required_stage_in_stable_order() {
    assert_eq!(
        BenchmarkStage::ALL,
        &[
            BenchmarkStage::Decode,
            BenchmarkStage::GraphEvaluation,
            BenchmarkStage::Upload,
            BenchmarkStage::Playback,
            BenchmarkStage::Cache,
            BenchmarkStage::Render,
            BenchmarkStage::SaveLoad,
        ]
    );
    assert_eq!(BenchmarkStage::GraphEvaluation.code(), "graph_evaluation");
    assert_eq!(BenchmarkStage::SaveLoad.code(), "save_load");
    assert_eq!(
        BenchmarkStage::from_code("upload"),
        Some(BenchmarkStage::Upload)
    );
    assert_eq!(BenchmarkStage::from_code("unknown"), None);
}

#[test]
fn rejects_unbounded_or_incomplete_configuration_and_context() {
    assert!(BenchmarkConfig::new(0, 1).is_err());
    assert!(BenchmarkConfig::new(1, 0).is_err());
    assert!(BenchmarkConfig::new(1_000_001, 1).is_err());
    assert!(BenchmarkConfig::new(1, 1_000_001).is_err());

    let mut fields = context().fields().clone();
    fields.gpu_driver.clear();
    assert!(BenchmarkContext::new(fields).is_err());

    let mut fields = context().fields().clone();
    fields.memory_mib = 0;
    assert!(BenchmarkContext::new(fields).is_err());

    let mut fields = context().fields().clone();
    fields.gpu_backend = "unreported".into();
    assert!(!BenchmarkContext::new(fields).unwrap().is_complete());

    let mut fields = context().fields().clone();
    fields.memory_mib = 1;
    assert!(!BenchmarkContext::new(fields).unwrap().is_complete());
}

#[test]
fn excludes_warmup_and_records_deterministic_statistics() {
    let calls = Cell::new(0_u32);
    let mut suite = BenchmarkSuite::new();
    suite
        .register(
            BenchmarkStage::Decode,
            "decode synthetic rgba frame",
            "video/rgba/v1",
            || {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap();
    let config = BenchmarkConfig::new(2, 4).unwrap();
    let mut ticks = VecDeque::from([10_u64, 20, 100, 120, 200, 230, 300, 340]);

    let report = suite.run_with_clock(&config, &context(), || ticks.pop_front().unwrap());

    assert_eq!(calls.get(), 6, "two warmups plus four measured samples");
    assert!(ticks.is_empty());
    let result = report.result(BenchmarkStage::Decode).unwrap();
    assert_eq!(result.workload_name(), Some("decode synthetic rgba frame"));
    assert_eq!(result.fixture_id(), Some("video/rgba/v1"));
    let BenchmarkStatus::Measured(statistics) = result.status() else {
        panic!("registered workload must be measured");
    };
    assert_eq!(statistics.sample_count(), 4);
    assert_eq!(statistics.min_ns(), 10);
    assert_eq!(statistics.max_ns(), 40);
    assert_eq!(statistics.mean_ns(), 25);
    assert_eq!(statistics.p50_ns(), 20);
    assert_eq!(statistics.p95_ns(), 40);
}

#[test]
fn reports_missing_and_failed_workloads_without_mocking_success() {
    let mut suite = BenchmarkSuite::new();
    suite
        .register(
            BenchmarkStage::Upload,
            "real upload",
            "video/rgba/v1",
            || Err("adapter was lost".into()),
        )
        .unwrap();

    let report = suite.run_with_clock(&BenchmarkConfig::new(1, 2).unwrap(), &context(), || 0);

    assert!(report.has_failures());
    assert_eq!(report.results().len(), BenchmarkStage::ALL.len());
    assert!(matches!(
        report.result(BenchmarkStage::GraphEvaluation).unwrap().status(),
        BenchmarkStatus::Gap(reason) if reason.contains("no real workload")
    ));
    assert!(matches!(
        report.result(BenchmarkStage::Upload).unwrap().status(),
        BenchmarkStatus::Failed(message) if message == "adapter was lost"
    ));
}

#[test]
fn filters_stages_but_keeps_requested_gaps_visible() {
    let config = BenchmarkConfig::new(1, 1)
        .unwrap()
        .with_stages([BenchmarkStage::Cache, BenchmarkStage::SaveLoad])
        .unwrap();
    let mut suite = BenchmarkSuite::new();

    let report = suite.run_with_clock(&config, &context(), || 0);

    assert_eq!(report.results().len(), 2);
    assert_eq!(report.results()[0].stage(), BenchmarkStage::Cache);
    assert_eq!(report.results()[1].stage(), BenchmarkStage::SaveLoad);
    assert!(!report.has_failures());
}

#[test]
fn serializes_complete_context_statuses_and_escaped_workload_data() {
    let mut suite = BenchmarkSuite::new();
    suite
        .register(
            BenchmarkStage::Render,
            "render \"reference\"\nframe",
            "render/reference/v1",
            || Ok(()),
        )
        .unwrap();
    let mut ticks = VecDeque::from([5_u64, 10]);
    let report = suite.run_with_clock(&BenchmarkConfig::new(1, 1).unwrap(), &context(), || {
        ticks.pop_front().unwrap()
    });

    let json = report.to_json();
    assert!(json.contains("\"schema_version\":1"));
    assert!(json.contains("\"build\":\"a11cecd-release\""));
    assert!(json.contains("\"memory_mib\":16384"));
    assert!(json.contains("\"gpu_backend\":\"test-gpu-backend\""));
    assert!(json.contains("\"cache_state\":\"cold\""));
    assert!(json.contains("\"fixture_revision\":\"fixtures-v1\""));
    assert!(json.contains("\"status\":\"measured\""));
    assert!(json.contains("render \\\"reference\\\"\\nframe"));
    assert!(!json.contains("NaN"));
}

#[test]
fn rejects_duplicate_or_invalid_workload_registration() {
    let mut suite = BenchmarkSuite::new();
    suite
        .register(BenchmarkStage::Playback, "playback", "playback/v1", || {
            Ok(())
        })
        .unwrap();
    assert!(suite
        .register(BenchmarkStage::Playback, "again", "playback/v1", || Ok(()))
        .is_err());

    let mut suite = BenchmarkSuite::new();
    assert!(suite
        .register(BenchmarkStage::Decode, "", "decode/v1", || Ok(()))
        .is_err());
    assert!(suite
        .register(BenchmarkStage::Decode, "decode", "", || Ok(()))
        .is_err());
}

#[test]
fn shipped_graph_workload_exercises_the_public_lazy_evaluator() {
    let mut suite = BenchmarkSuite::new();
    register_graph_evaluation_workload(&mut suite).unwrap();
    let config = BenchmarkConfig::new(1, 2)
        .unwrap()
        .with_stages([BenchmarkStage::GraphEvaluation])
        .unwrap();
    let mut ticks = VecDeque::from([10_u64, 25, 30, 50]);

    let report = suite.run_with_clock(&config, &context(), || ticks.pop_front().unwrap());

    let result = report.result(BenchmarkStage::GraphEvaluation).unwrap();
    assert_eq!(result.workload_name(), Some("lazy three-node graph pull"));
    assert_eq!(result.fixture_id(), Some("graph/evaluation/three-node/v1"));
    assert!(matches!(result.status(), BenchmarkStatus::Measured(_)));
}
