//! Structured offline test reports for Superi's platform matrix.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub enum ReportError {
    Json(String),
    Validation(Vec<String>),
    Io { path: PathBuf, message: String },
}

impl fmt::Display for ReportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(message) => write!(formatter, "invalid report input JSON: {message}"),
            Self::Validation(errors) => {
                for error in errors {
                    writeln!(formatter, "report.validation: {error}")?;
                }
                Ok(())
            }
            Self::Io { path, message } => write!(formatter, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for ReportError {}

#[derive(Clone, Debug)]
pub struct GeneratedReport {
    report: TestReport,
}

impl GeneratedReport {
    pub fn canonical_json(&self) -> Result<Vec<u8>, ReportError> {
        let mut bytes = serde_json::to_vec_pretty(&self.report)
            .map_err(|error| ReportError::Json(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    #[must_use]
    pub const fn has_blocking_findings(&self) -> bool {
        self.report.blocking
            && (!matches!(self.report.status, LaneStatus::Pass)
                || self.report.summary.blocking_findings > 0)
    }
}

pub fn generate_report(input: &[u8]) -> Result<GeneratedReport, ReportError> {
    let mut input = serde_json::from_slice::<ReportInput>(input)
        .map_err(|error| ReportError::Json(error.to_string()))?;
    let errors = validate(&input);
    if !errors.is_empty() {
        return Err(ReportError::Validation(errors));
    }
    canonicalize(&mut input);
    Ok(GeneratedReport {
        report: derive_report(input),
    })
}

pub fn build_report_file(input: &Path, output: &Path) -> Result<GeneratedReport, ReportError> {
    let bytes = fs::read(input).map_err(|error| io_error(input, error))?;
    let report = generate_report(&bytes)?;
    write_new(output, &report.canonical_json()?)?;
    Ok(report)
}

fn write_new(output: &Path, bytes: &[u8]) -> Result<(), ReportError> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(ReportError::Io {
            path: parent.to_path_buf(),
            message: "output parent is not a directory".to_owned(),
        });
    }
    let file_name = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ReportError::Io {
            path: output.to_path_buf(),
            message: "output file name must be valid UTF-8".to_owned(),
        })?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| io_error(&temporary, error))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(io_error(&temporary, error));
    }
    if let Err(error) = fs::hard_link(&temporary, output) {
        let _ = fs::remove_file(&temporary);
        return Err(ReportError::Io {
            path: output.to_path_buf(),
            message: if error.kind() == std::io::ErrorKind::AlreadyExists {
                "refusing to overwrite an existing report".to_owned()
            } else {
                error.to_string()
            },
        });
    }
    fs::remove_file(&temporary).map_err(|error| io_error(&temporary, error))?;
    Ok(())
}

fn io_error(path: &Path, error: std::io::Error) -> ReportError {
    ReportError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportInput {
    schema_version: u32,
    matrix_revision: u32,
    lane_id: String,
    suites: Vec<String>,
    blocking: bool,
    source: SourceEvidence,
    fixtures: FixtureEvidence,
    platform: PlatformEvidence,
    timing: RunTiming,
    artifacts: Vec<Artifact>,
    tests: Vec<TestEvidence>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceEvidence {
    commit_sha: String,
    dirty: bool,
    build_profile: String,
    rust_version: String,
    cargo_lock_sha256: String,
    enabled_features: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FixtureEvidence {
    manifest_revision: String,
    reference_project_ids: Vec<String>,
    reference_media_ids: Vec<String>,
    expected_output_revision: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PlatformEvidence {
    os_name: String,
    os_edition: String,
    os_version: String,
    os_build: String,
    kernel: String,
    architecture: String,
    cpu_model: String,
    logical_cores: u32,
    memory_bytes: u64,
    hardware_tier: String,
    gpu: Option<GpuEvidence>,
    audio: Option<AudioEvidence>,
    codec_backends: Vec<CodecBackendEvidence>,
    cache: CacheEvidence,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GpuEvidence {
    vendor: String,
    model: String,
    device_id: String,
    backend: String,
    driver_version: String,
    adapter_limits: String,
    display_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AudioEvidence {
    device: String,
    driver: String,
    sample_rate: u32,
    channel_layout: String,
    buffer_frames: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CodecBackendEvidence {
    identity: String,
    operations: Vec<String>,
    acceleration: String,
    version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CacheEvidence {
    state: String,
    size_bytes: u64,
    storage_medium: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RunTiming {
    started_at: String,
    ended_at: String,
    duration_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(deny_unknown_fields)]
struct Artifact {
    id: String,
    uri: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TestEvidence {
    id: String,
    suite: String,
    command: String,
    attempts: Vec<TestAttempt>,
    evidence: Vec<AnalysisEvidence>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TestAttempt {
    number: u32,
    status: AttemptStatus,
    duration_ms: u64,
    seed: String,
    reason: Option<String>,
    artifacts: Vec<Artifact>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum AttemptStatus {
    Pass,
    Fail,
    Skip,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum AnalysisEvidence {
    Performance {
        metric: String,
        unit: String,
        baseline: f64,
        observed: f64,
        allowed_regression_fraction: f64,
        direction: MetricDirection,
    },
    Golden {
        expected_revision: String,
        actual_revision: String,
        tolerance: f64,
        compared_samples: u64,
        mismatched_samples: u64,
        maximum_absolute_error: f64,
        first_mismatch: Option<String>,
        artifacts: Vec<Artifact>,
    },
    PlatformGap {
        capability: String,
        expected: String,
        observed: String,
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MetricDirection {
    LowerIsBetter,
    HigherIsBetter,
}

#[derive(Clone, Debug, Serialize)]
struct TestReport {
    schema_version: u32,
    matrix_revision: u32,
    lane_id: String,
    status: LaneStatus,
    blocking: bool,
    suites: Vec<String>,
    summary: ReportSummary,
    source: SourceEvidence,
    fixtures: FixtureEvidence,
    platform: PlatformEvidence,
    timing: RunTiming,
    artifacts: Vec<Artifact>,
    tests: Vec<TestEvidence>,
    findings: Vec<Finding>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum LaneStatus {
    Pass,
    Fail,
    Gap,
}

#[derive(Clone, Debug, Default, Serialize)]
struct ReportSummary {
    passed: u64,
    failed: u64,
    skipped: u64,
    gaps: u64,
    performance_regressions: u64,
    golden_mismatches: u64,
    flaky_tests: u64,
    platform_gaps: u64,
    blocking_findings: u64,
}

#[derive(Clone, Debug, Serialize)]
struct Finding {
    category: FindingCategory,
    test_id: String,
    suite: String,
    blocking: bool,
    detail: FindingDetail,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum FindingCategory {
    PerformanceRegression,
    GoldenMismatch,
    FlakyTest,
    PlatformGap,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
enum FindingDetail {
    Performance(PerformanceFinding),
    Golden(GoldenFinding),
    Flaky(FlakyFinding),
    Gap(PlatformGapFinding),
}

#[derive(Clone, Debug, Serialize)]
struct PerformanceFinding {
    metric: String,
    unit: String,
    baseline: f64,
    observed: f64,
    allowed_regression_fraction: f64,
    direction: MetricDirection,
    failure_threshold: f64,
    observed_delta_fraction: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
struct GoldenFinding {
    expected_revision: String,
    actual_revision: String,
    tolerance: f64,
    compared_samples: u64,
    mismatched_samples: u64,
    maximum_absolute_error: f64,
    first_mismatch: Option<String>,
    artifacts: Vec<Artifact>,
}

#[derive(Clone, Debug, Serialize)]
struct FlakyFinding {
    attempts: u32,
    first_status: AttemptStatus,
    final_status: AttemptStatus,
    reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct PlatformGapFinding {
    capability: String,
    expected: String,
    observed: String,
    reason: String,
}

fn validate(input: &ReportInput) -> Vec<String> {
    let mut errors = Vec::new();
    if input.schema_version != SUPPORTED_SCHEMA_VERSION {
        errors.push(format!(
            "schema_version must be {SUPPORTED_SCHEMA_VERSION}, got {}",
            input.schema_version
        ));
    }
    if input.matrix_revision == 0 {
        errors.push("matrix_revision must be positive".to_owned());
    }
    require_code(&mut errors, "lane_id", &input.lane_id);
    require_unique_codes(&mut errors, "suites", &input.suites);
    validate_source(&mut errors, &input.source);
    validate_fixtures(&mut errors, &input.fixtures);
    validate_platform(&mut errors, &input.platform);
    require_text(&mut errors, "timing.started_at", &input.timing.started_at);
    require_text(&mut errors, "timing.ended_at", &input.timing.ended_at);
    validate_artifacts(&mut errors, "artifacts", &input.artifacts);

    let suites = input
        .suites
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut test_ids = BTreeSet::new();
    for test in &input.tests {
        require_code(&mut errors, "test.id", &test.id);
        require_code(&mut errors, "test.suite", &test.suite);
        require_text(&mut errors, "test.command", &test.command);
        if !test_ids.insert(test.id.as_str()) {
            errors.push(format!("duplicate test id {:?}", test.id));
        }
        if !suites.contains(test.suite.as_str()) {
            errors.push(format!(
                "test {:?} references undeclared suite {:?}",
                test.id, test.suite
            ));
        }
        if test.attempts.is_empty() {
            errors.push(format!(
                "test {:?} must contain at least one attempt",
                test.id
            ));
        }
        for (index, attempt) in test.attempts.iter().enumerate() {
            let expected = u32::try_from(index + 1).unwrap_or(u32::MAX);
            if attempt.number != expected {
                errors.push(format!(
                    "test {:?} attempt numbers must start at 1 and remain contiguous",
                    test.id
                ));
                break;
            }
            require_text(&mut errors, "test.attempt.seed", &attempt.seed);
            if !matches!(attempt.status, AttemptStatus::Pass)
                && attempt
                    .reason
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
            {
                errors.push(format!(
                    "test {:?} non-pass attempt {} requires an exact reason",
                    test.id, attempt.number
                ));
            }
            validate_artifacts(&mut errors, "test.attempt.artifacts", &attempt.artifacts);
        }
        for evidence in &test.evidence {
            validate_analysis(&mut errors, &test.id, evidence);
        }
    }
    errors.sort();
    errors.dedup();
    errors
}

fn validate_source(errors: &mut Vec<String>, source: &SourceEvidence) {
    if source.commit_sha.len() != 40 || !is_lower_hex(&source.commit_sha) {
        errors
            .push("source.commit_sha must be a 40-character lowercase hexadecimal SHA".to_owned());
    }
    if source.cargo_lock_sha256.len() != 64 || !is_lower_hex(&source.cargo_lock_sha256) {
        errors.push("source.cargo_lock_sha256 must be a 64-character lowercase SHA-256".to_owned());
    }
    require_text(errors, "source.build_profile", &source.build_profile);
    require_text(errors, "source.rust_version", &source.rust_version);
    require_unique_codes(errors, "source.enabled_features", &source.enabled_features);
}

fn validate_fixtures(errors: &mut Vec<String>, fixtures: &FixtureEvidence) {
    require_text(
        errors,
        "fixtures.manifest_revision",
        &fixtures.manifest_revision,
    );
    require_text(
        errors,
        "fixtures.expected_output_revision",
        &fixtures.expected_output_revision,
    );
    require_unique_text(
        errors,
        "fixtures.reference_project_ids",
        &fixtures.reference_project_ids,
    );
    require_unique_text(
        errors,
        "fixtures.reference_media_ids",
        &fixtures.reference_media_ids,
    );
}

fn validate_platform(errors: &mut Vec<String>, platform: &PlatformEvidence) {
    for (name, value) in [
        ("platform.os_name", platform.os_name.as_str()),
        ("platform.os_edition", platform.os_edition.as_str()),
        ("platform.os_version", platform.os_version.as_str()),
        ("platform.os_build", platform.os_build.as_str()),
        ("platform.kernel", platform.kernel.as_str()),
        ("platform.architecture", platform.architecture.as_str()),
        ("platform.cpu_model", platform.cpu_model.as_str()),
        ("platform.hardware_tier", platform.hardware_tier.as_str()),
        ("platform.cache.state", platform.cache.state.as_str()),
        (
            "platform.cache.storage_medium",
            platform.cache.storage_medium.as_str(),
        ),
    ] {
        require_text(errors, name, value);
    }
    if platform.logical_cores == 0 {
        errors.push("platform.logical_cores must be positive".to_owned());
    }
    if platform.memory_bytes == 0 {
        errors.push("platform.memory_bytes must be positive".to_owned());
    }
    if let Some(gpu) = &platform.gpu {
        for (name, value) in [
            ("platform.gpu.vendor", gpu.vendor.as_str()),
            ("platform.gpu.model", gpu.model.as_str()),
            ("platform.gpu.device_id", gpu.device_id.as_str()),
            ("platform.gpu.backend", gpu.backend.as_str()),
            ("platform.gpu.driver_version", gpu.driver_version.as_str()),
            ("platform.gpu.adapter_limits", gpu.adapter_limits.as_str()),
            ("platform.gpu.display_path", gpu.display_path.as_str()),
        ] {
            require_text(errors, name, value);
        }
    }
    if let Some(audio) = &platform.audio {
        for (name, value) in [
            ("platform.audio.device", audio.device.as_str()),
            ("platform.audio.driver", audio.driver.as_str()),
            (
                "platform.audio.channel_layout",
                audio.channel_layout.as_str(),
            ),
        ] {
            require_text(errors, name, value);
        }
        if audio.sample_rate == 0 || audio.buffer_frames == 0 {
            errors.push("platform audio sample rate and buffer frames must be positive".to_owned());
        }
    }
    let mut backend_ids = BTreeSet::new();
    for backend in &platform.codec_backends {
        require_code(errors, "platform.codec_backend.identity", &backend.identity);
        if !backend_ids.insert(backend.identity.as_str()) {
            errors.push(format!("duplicate codec backend {:?}", backend.identity));
        }
        require_unique_codes(
            errors,
            "platform.codec_backend.operations",
            &backend.operations,
        );
        require_text(
            errors,
            "platform.codec_backend.acceleration",
            &backend.acceleration,
        );
        require_text(errors, "platform.codec_backend.version", &backend.version);
    }
}

fn validate_analysis(errors: &mut Vec<String>, test_id: &str, evidence: &AnalysisEvidence) {
    match evidence {
        AnalysisEvidence::Performance {
            metric,
            unit,
            baseline,
            observed,
            allowed_regression_fraction,
            ..
        } => {
            require_code(errors, "performance.metric", metric);
            require_code(errors, "performance.unit", unit);
            for (name, value) in [
                ("baseline", *baseline),
                ("observed", *observed),
                ("allowed_regression_fraction", *allowed_regression_fraction),
            ] {
                if !value.is_finite() || value < 0.0 {
                    errors.push(format!(
                        "test {test_id:?} performance {name} must be finite and nonnegative"
                    ));
                }
            }
            if *allowed_regression_fraction > 1.0 {
                errors.push(format!(
                    "test {test_id:?} allowed regression fraction cannot exceed 1"
                ));
            }
        }
        AnalysisEvidence::Golden {
            expected_revision,
            actual_revision,
            tolerance,
            compared_samples,
            mismatched_samples,
            maximum_absolute_error,
            artifacts,
            ..
        } => {
            require_text(errors, "golden.expected_revision", expected_revision);
            require_text(errors, "golden.actual_revision", actual_revision);
            if !tolerance.is_finite() || *tolerance < 0.0 {
                errors.push(format!(
                    "test {test_id:?} golden tolerance must be finite and nonnegative"
                ));
            }
            if !maximum_absolute_error.is_finite() || *maximum_absolute_error < 0.0 {
                errors.push(format!(
                    "test {test_id:?} golden maximum error must be finite and nonnegative"
                ));
            }
            if mismatched_samples > compared_samples {
                errors.push(format!(
                    "test {test_id:?} golden mismatches cannot exceed compared samples"
                ));
            }
            validate_artifacts(errors, "golden.artifacts", artifacts);
        }
        AnalysisEvidence::PlatformGap {
            capability,
            expected,
            observed,
            reason,
        } => {
            require_code(errors, "platform_gap.capability", capability);
            require_text(errors, "platform_gap.expected", expected);
            require_text(errors, "platform_gap.observed", observed);
            require_text(errors, "platform_gap.reason", reason);
        }
    }
}

fn canonicalize(input: &mut ReportInput) {
    input.suites.sort();
    input.source.enabled_features.sort();
    input.fixtures.reference_project_ids.sort();
    input.fixtures.reference_media_ids.sort();
    input.artifacts.sort();
    input
        .platform
        .codec_backends
        .sort_by(|left, right| left.identity.cmp(&right.identity));
    for backend in &mut input.platform.codec_backends {
        backend.operations.sort();
    }
    input.tests.sort_by(|left, right| {
        left.suite
            .cmp(&right.suite)
            .then_with(|| left.id.cmp(&right.id))
    });
    for test in &mut input.tests {
        for attempt in &mut test.attempts {
            attempt.artifacts.sort();
        }
        for evidence in &mut test.evidence {
            if let AnalysisEvidence::Golden { artifacts, .. } = evidence {
                artifacts.sort();
            }
        }
    }
}

fn derive_report(input: ReportInput) -> TestReport {
    let mut summary = ReportSummary::default();
    let mut findings = Vec::new();
    let mut suites_with_evidence = BTreeSet::new();

    for test in &input.tests {
        suites_with_evidence.insert(test.suite.as_str());
        let final_status = test
            .attempts
            .last()
            .map(|attempt| attempt.status)
            .unwrap_or(AttemptStatus::Skip);
        match final_status {
            AttemptStatus::Pass => summary.passed += 1,
            AttemptStatus::Fail => summary.failed += 1,
            AttemptStatus::Skip => summary.skipped += 1,
        }

        if test.attempts.len() > 1
            && test
                .attempts
                .iter()
                .any(|attempt| attempt.status != final_status)
        {
            let reasons = test
                .attempts
                .iter()
                .filter_map(|attempt| attempt.reason.clone())
                .collect();
            findings.push(Finding {
                category: FindingCategory::FlakyTest,
                test_id: test.id.clone(),
                suite: test.suite.clone(),
                blocking: input.blocking,
                detail: FindingDetail::Flaky(FlakyFinding {
                    attempts: u32::try_from(test.attempts.len()).unwrap_or(u32::MAX),
                    first_status: test.attempts[0].status,
                    final_status,
                    reasons,
                }),
            });
        }

        let mut explicit_gap = false;
        for evidence in &test.evidence {
            match evidence {
                AnalysisEvidence::Performance {
                    metric,
                    unit,
                    baseline,
                    observed,
                    allowed_regression_fraction,
                    direction,
                } => {
                    let threshold = match direction {
                        MetricDirection::LowerIsBetter => {
                            baseline * (1.0 + allowed_regression_fraction)
                        }
                        MetricDirection::HigherIsBetter => {
                            baseline * (1.0 - allowed_regression_fraction).max(0.0)
                        }
                    };
                    let regressed = match direction {
                        MetricDirection::LowerIsBetter => observed > &threshold,
                        MetricDirection::HigherIsBetter => observed < &threshold,
                    };
                    if regressed {
                        findings.push(Finding {
                            category: FindingCategory::PerformanceRegression,
                            test_id: test.id.clone(),
                            suite: test.suite.clone(),
                            blocking: input.blocking,
                            detail: FindingDetail::Performance(PerformanceFinding {
                                metric: metric.clone(),
                                unit: unit.clone(),
                                baseline: *baseline,
                                observed: *observed,
                                allowed_regression_fraction: *allowed_regression_fraction,
                                direction: *direction,
                                failure_threshold: threshold,
                                observed_delta_fraction: (*baseline != 0.0)
                                    .then_some((observed - baseline) / baseline),
                            }),
                        });
                    }
                }
                AnalysisEvidence::Golden {
                    expected_revision,
                    actual_revision,
                    tolerance,
                    compared_samples,
                    mismatched_samples,
                    maximum_absolute_error,
                    first_mismatch,
                    artifacts,
                } if *mismatched_samples > 0 => findings.push(Finding {
                    category: FindingCategory::GoldenMismatch,
                    test_id: test.id.clone(),
                    suite: test.suite.clone(),
                    blocking: input.blocking,
                    detail: FindingDetail::Golden(GoldenFinding {
                        expected_revision: expected_revision.clone(),
                        actual_revision: actual_revision.clone(),
                        tolerance: *tolerance,
                        compared_samples: *compared_samples,
                        mismatched_samples: *mismatched_samples,
                        maximum_absolute_error: *maximum_absolute_error,
                        first_mismatch: first_mismatch.clone(),
                        artifacts: artifacts.clone(),
                    }),
                }),
                AnalysisEvidence::Golden { .. } => {}
                AnalysisEvidence::PlatformGap {
                    capability,
                    expected,
                    observed,
                    reason,
                } => {
                    explicit_gap = true;
                    findings.push(Finding {
                        category: FindingCategory::PlatformGap,
                        test_id: test.id.clone(),
                        suite: test.suite.clone(),
                        blocking: input.blocking,
                        detail: FindingDetail::Gap(PlatformGapFinding {
                            capability: capability.clone(),
                            expected: expected.clone(),
                            observed: observed.clone(),
                            reason: reason.clone(),
                        }),
                    });
                }
            }
        }
        if final_status == AttemptStatus::Skip && !explicit_gap {
            findings.push(Finding {
                category: FindingCategory::PlatformGap,
                test_id: test.id.clone(),
                suite: test.suite.clone(),
                blocking: input.blocking,
                detail: FindingDetail::Gap(PlatformGapFinding {
                    capability: test.id.clone(),
                    expected: "test executed".to_owned(),
                    observed: "skipped".to_owned(),
                    reason: test
                        .attempts
                        .last()
                        .and_then(|attempt| attempt.reason.clone())
                        .unwrap_or_else(|| "skip did not include a reason".to_owned()),
                }),
            });
        }
    }

    for suite in &input.suites {
        if !suites_with_evidence.contains(suite.as_str()) {
            findings.push(Finding {
                category: FindingCategory::PlatformGap,
                test_id: format!("suite:{suite}"),
                suite: suite.clone(),
                blocking: input.blocking,
                detail: FindingDetail::Gap(PlatformGapFinding {
                    capability: suite.clone(),
                    expected: "suite evidence".to_owned(),
                    observed: "missing".to_owned(),
                    reason: "declared suite produced no test evidence".to_owned(),
                }),
            });
        }
    }

    findings.sort_by(|left, right| {
        left.test_id
            .cmp(&right.test_id)
            .then_with(|| finding_rank(left.category).cmp(&finding_rank(right.category)))
    });
    for finding in &findings {
        match finding.category {
            FindingCategory::PerformanceRegression => summary.performance_regressions += 1,
            FindingCategory::GoldenMismatch => summary.golden_mismatches += 1,
            FindingCategory::FlakyTest => summary.flaky_tests += 1,
            FindingCategory::PlatformGap => summary.platform_gaps += 1,
        }
        if finding.blocking {
            summary.blocking_findings += 1;
        }
    }
    summary.gaps = summary.platform_gaps;
    let status = if summary.failed > 0
        || summary.performance_regressions > 0
        || summary.golden_mismatches > 0
        || summary.flaky_tests > 0
    {
        LaneStatus::Fail
    } else if summary.platform_gaps > 0 {
        LaneStatus::Gap
    } else {
        LaneStatus::Pass
    };

    TestReport {
        schema_version: input.schema_version,
        matrix_revision: input.matrix_revision,
        lane_id: input.lane_id,
        status,
        blocking: input.blocking,
        suites: input.suites,
        summary,
        source: input.source,
        fixtures: input.fixtures,
        platform: input.platform,
        timing: input.timing,
        artifacts: input.artifacts,
        tests: input.tests,
        findings,
    }
}

const fn finding_rank(category: FindingCategory) -> u8 {
    match category {
        FindingCategory::PerformanceRegression => 0,
        FindingCategory::GoldenMismatch => 1,
        FindingCategory::FlakyTest => 2,
        FindingCategory::PlatformGap => 3,
    }
}

fn validate_artifacts(errors: &mut Vec<String>, field: &str, artifacts: &[Artifact]) {
    let mut ids = BTreeSet::new();
    for artifact in artifacts {
        require_code(errors, &format!("{field}.id"), &artifact.id);
        require_text(errors, &format!("{field}.uri"), &artifact.uri);
        if !ids.insert(artifact.id.as_str()) {
            errors.push(format!(
                "{field} contains duplicate artifact id {:?}",
                artifact.id
            ));
        }
    }
}

fn require_unique_codes(errors: &mut Vec<String>, field: &str, values: &[String]) {
    let mut unique = BTreeSet::new();
    for value in values {
        require_code(errors, field, value);
        if !unique.insert(value.as_str()) {
            errors.push(format!("{field} contains duplicate value {value:?}"));
        }
    }
    if values.is_empty() && field == "suites" {
        errors.push("suites must not be empty".to_owned());
    }
}

fn require_unique_text(errors: &mut Vec<String>, field: &str, values: &[String]) {
    let mut unique = BTreeSet::new();
    for value in values {
        require_text(errors, field, value);
        if !unique.insert(value.as_str()) {
            errors.push(format!("{field} contains duplicate value {value:?}"));
        }
    }
    if values.is_empty() {
        errors.push(format!("{field} must not be empty"));
    }
}

fn require_code(errors: &mut Vec<String>, field: &str, value: &str) {
    if !is_code(value) {
        errors.push(format!(
            "{field} must use lowercase ASCII letters and digits separated by '.', '_', or '-'"
        ));
    }
}

fn require_text(errors: &mut Vec<String>, field: &str, value: &str) {
    if value.trim().is_empty() {
        errors.push(format!("{field} must not be empty"));
    }
}

fn is_code(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    let mut separator = false;
    for byte in bytes {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            separator = false;
        } else if matches!(byte, b'.' | b'_' | b'-') && !separator {
            separator = true;
        } else {
            return false;
        }
    }
    !separator
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
