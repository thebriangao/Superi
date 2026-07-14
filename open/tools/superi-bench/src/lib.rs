//! Stable, dependency-free benchmark harnesses for Superi engine stages.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::hint::black_box;
use std::time::Instant;

use superi_core::error::Result as SuperiResult;
use superi_core::geometry::PixelBounds;
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::eval::{EvaluateNode, EvaluationContext, EvaluationRequest, LazyEvaluator};
use superi_graph::ids::{EdgeId, GraphId, NodeId, PortId};

const MAX_ITERATIONS: u32 = 1_000_000;
const GAP_REASON: &str = "no real workload is registered for this stage";

/// Engine stages that require a permanent benchmark identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BenchmarkStage {
    /// Packet-to-frame decode work.
    Decode,
    /// Lazy node-graph evaluation work.
    GraphEvaluation,
    /// Decoded-frame upload work.
    Upload,
    /// Real-time playback orchestration work.
    Playback,
    /// Frame and intermediate cache work.
    Cache,
    /// Headless render or export work.
    Render,
    /// Project serialization and deserialization work.
    SaveLoad,
}

impl BenchmarkStage {
    /// Every required stage in canonical report order.
    pub const ALL: &'static [Self] = &[
        Self::Decode,
        Self::GraphEvaluation,
        Self::Upload,
        Self::Playback,
        Self::Cache,
        Self::Render,
        Self::SaveLoad,
    ];

    /// Returns the stable machine-readable stage code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Decode => "decode",
            Self::GraphEvaluation => "graph_evaluation",
            Self::Upload => "upload",
            Self::Playback => "playback",
            Self::Cache => "cache",
            Self::Render => "render",
            Self::SaveLoad => "save_load",
        }
    }

    /// Looks up a stage by its stable code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|stage| stage.code() == code)
    }
}

/// A benchmark configuration error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenchmarkError(String);

impl BenchmarkError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for BenchmarkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BenchmarkError {}

/// Bounded warmup, sampling, and stage-selection policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenchmarkConfig {
    warmup_iterations: u32,
    measured_iterations: u32,
    stages: Vec<BenchmarkStage>,
}

impl BenchmarkConfig {
    /// Creates a bounded configuration that runs every required stage.
    pub fn new(warmup_iterations: u32, measured_iterations: u32) -> Result<Self, BenchmarkError> {
        validate_iterations("warmup", warmup_iterations)?;
        validate_iterations("measured", measured_iterations)?;
        Ok(Self {
            warmup_iterations,
            measured_iterations,
            stages: BenchmarkStage::ALL.to_vec(),
        })
    }

    /// Restricts the run to a nonempty set, retaining canonical stage order.
    pub fn with_stages(
        mut self,
        stages: impl IntoIterator<Item = BenchmarkStage>,
    ) -> Result<Self, BenchmarkError> {
        let requested = stages.into_iter().collect::<Vec<_>>();
        if requested.is_empty() {
            return Err(BenchmarkError::new(
                "at least one benchmark stage is required",
            ));
        }
        let unique = requested.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != requested.len() {
            return Err(BenchmarkError::new(
                "benchmark stage filter contains duplicates",
            ));
        }
        self.stages = BenchmarkStage::ALL
            .iter()
            .copied()
            .filter(|stage| unique.contains(stage))
            .collect();
        Ok(self)
    }

    /// Returns the number of unmeasured warmup invocations.
    #[must_use]
    pub const fn warmup_iterations(&self) -> u32 {
        self.warmup_iterations
    }

    /// Returns the number of measured samples.
    #[must_use]
    pub const fn measured_iterations(&self) -> u32 {
        self.measured_iterations
    }

    /// Returns selected stages in canonical order.
    #[must_use]
    pub fn stages(&self) -> &[BenchmarkStage] {
        &self.stages
    }
}

fn validate_iterations(name: &str, value: u32) -> Result<(), BenchmarkError> {
    if value == 0 || value > MAX_ITERATIONS {
        return Err(BenchmarkError::new(format!(
            "{name} iterations must be between 1 and {MAX_ITERATIONS}"
        )));
    }
    Ok(())
}

/// Complete environment and reference-input fields attached to every run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenchmarkContextFields {
    pub build: String,
    pub operating_system: String,
    pub architecture: String,
    pub cpu: String,
    pub memory_mib: u64,
    pub gpu_backend: String,
    pub gpu_driver: String,
    pub cache_state: String,
    pub hardware_tier: String,
    pub fixture_revision: String,
    pub project_revision: String,
}

/// Validated context required to interpret a benchmark report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenchmarkContext(BenchmarkContextFields);

impl BenchmarkContext {
    /// Validates that every context dimension is present.
    pub fn new(fields: BenchmarkContextFields) -> Result<Self, BenchmarkError> {
        for (name, value) in [
            ("build", fields.build.as_str()),
            ("operating_system", fields.operating_system.as_str()),
            ("architecture", fields.architecture.as_str()),
            ("cpu", fields.cpu.as_str()),
            ("gpu_backend", fields.gpu_backend.as_str()),
            ("gpu_driver", fields.gpu_driver.as_str()),
            ("cache_state", fields.cache_state.as_str()),
            ("hardware_tier", fields.hardware_tier.as_str()),
            ("fixture_revision", fields.fixture_revision.as_str()),
            ("project_revision", fields.project_revision.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(BenchmarkError::new(format!(
                    "benchmark context field {name} must not be empty"
                )));
            }
        }
        if fields.memory_mib == 0 {
            return Err(BenchmarkError::new(
                "benchmark context memory_mib must be greater than zero",
            ));
        }
        Ok(Self(fields))
    }

    /// Returns all validated context fields.
    #[must_use]
    pub const fn fields(&self) -> &BenchmarkContextFields {
        &self.0
    }

    /// Reports whether context is suitable as performance evidence.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.0.memory_mib > 1
            && ![
                self.0.build.as_str(),
                self.0.operating_system.as_str(),
                self.0.architecture.as_str(),
                self.0.cpu.as_str(),
                self.0.gpu_backend.as_str(),
                self.0.gpu_driver.as_str(),
                self.0.cache_state.as_str(),
                self.0.hardware_tier.as_str(),
                self.0.fixture_revision.as_str(),
                self.0.project_revision.as_str(),
            ]
            .contains(&"unreported")
    }
}

struct Workload<'a> {
    name: String,
    fixture_id: String,
    run: Box<dyn FnMut() -> Result<(), String> + 'a>,
}

/// Registry and runner for real engine workloads.
#[derive(Default)]
pub struct BenchmarkSuite<'a> {
    workloads: BTreeMap<BenchmarkStage, Workload<'a>>,
}

#[derive(Clone, Copy)]
struct GraphBenchmarkNode {
    source_value: Option<i64>,
}

impl EvaluateNode<i64> for GraphBenchmarkNode {
    fn evaluate(&self, context: &EvaluationContext<'_, i64>) -> SuperiResult<i64> {
        Ok(self
            .source_value
            .unwrap_or_else(|| context.inputs().iter().map(|input| *input.value()).sum()))
    }
}

/// Registers a deterministic three-node pull through the public lazy graph evaluator.
pub fn register_graph_evaluation_workload<'a>(
    suite: &mut BenchmarkSuite<'a>,
) -> Result<(), BenchmarkError> {
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(1));
    graph
        .insert_node(
            NodeId::from_raw(1),
            GraphBenchmarkNode {
                source_value: Some(3),
            },
        )
        .map_err(|error| BenchmarkError::new(error.to_string()))?;
    graph
        .insert_node(
            NodeId::from_raw(2),
            GraphBenchmarkNode {
                source_value: Some(5),
            },
        )
        .map_err(|error| BenchmarkError::new(error.to_string()))?;
    graph
        .insert_node(
            NodeId::from_raw(3),
            GraphBenchmarkNode { source_value: None },
        )
        .map_err(|error| BenchmarkError::new(error.to_string()))?;
    for (edge_id, source_node, destination_port) in [(1, 1, 31), (2, 2, 32)] {
        graph
            .insert_edge(GraphEdge::new(
                EdgeId::from_raw(edge_id),
                GraphEndpoint::new(NodeId::from_raw(source_node), PortId::from_raw(10)),
                GraphEndpoint::new(NodeId::from_raw(3), PortId::from_raw(destination_port)),
            ))
            .map_err(|error| BenchmarkError::new(error.to_string()))?;
    }
    let request = EvaluationRequest::new(
        GraphEndpoint::new(NodeId::from_raw(3), PortId::from_raw(30)),
        RationalTime::new(
            12,
            Timebase::integer(24).expect("24 fps is a valid timebase"),
        ),
        PixelBounds::new(0, 0, 1920, 1080).expect("benchmark region is nonempty"),
    );

    suite.register(
        BenchmarkStage::GraphEvaluation,
        "lazy three-node graph pull",
        "graph/evaluation/three-node/v1",
        move || {
            let result =
                LazyEvaluator::evaluate(&graph, request).map_err(|error| error.to_string())?;
            if *result.value() != 8 || result.evaluated_keys().len() != 3 {
                return Err("graph evaluation returned an unexpected result".into());
            }
            black_box(result.value());
            Ok(())
        },
    )
}

impl BenchmarkSuite<'_> {
    /// Creates an empty suite. Empty stages are reported as gaps.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a> BenchmarkSuite<'a> {
    /// Registers one real workload for a stage.
    pub fn register(
        &mut self,
        stage: BenchmarkStage,
        name: impl Into<String>,
        fixture_id: impl Into<String>,
        run: impl FnMut() -> Result<(), String> + 'a,
    ) -> Result<(), BenchmarkError> {
        let name = name.into();
        let fixture_id = fixture_id.into();
        if name.trim().is_empty() || fixture_id.trim().is_empty() {
            return Err(BenchmarkError::new(
                "benchmark workload name and fixture identity must not be empty",
            ));
        }
        if self.workloads.contains_key(&stage) {
            return Err(BenchmarkError::new(format!(
                "benchmark stage {} already has a workload",
                stage.code()
            )));
        }
        self.workloads.insert(
            stage,
            Workload {
                name,
                fixture_id,
                run: Box::new(run),
            },
        );
        Ok(())
    }

    /// Runs with the process monotonic clock.
    pub fn run(&mut self, config: &BenchmarkConfig, context: &BenchmarkContext) -> BenchmarkReport {
        let origin = Instant::now();
        self.run_with_clock(config, context, || {
            u64::try_from(origin.elapsed().as_nanos()).unwrap_or(u64::MAX)
        })
    }

    /// Runs with an injected monotonic nanosecond clock for contract testing.
    pub fn run_with_clock(
        &mut self,
        config: &BenchmarkConfig,
        context: &BenchmarkContext,
        mut clock: impl FnMut() -> u64,
    ) -> BenchmarkReport {
        let mut results = Vec::with_capacity(config.stages.len());
        for stage in &config.stages {
            let Some(workload) = self.workloads.get_mut(stage) else {
                results.push(BenchmarkResult::gap(*stage));
                continue;
            };

            let mut failure = None;
            for _ in 0..config.warmup_iterations {
                if let Err(message) = run_once(workload) {
                    failure = Some(message);
                    break;
                }
            }
            if let Some(message) = failure {
                results.push(BenchmarkResult::failed(stage, workload, message));
                continue;
            }

            let mut samples = Vec::with_capacity(config.measured_iterations as usize);
            for _ in 0..config.measured_iterations {
                let start = clock();
                if let Err(message) = run_once(workload) {
                    failure = Some(message);
                    break;
                }
                let finish = clock();
                match finish.checked_sub(start) {
                    Some(duration) => samples.push(duration),
                    None => {
                        failure = Some("benchmark clock moved backwards".into());
                        break;
                    }
                }
            }
            results.push(match failure {
                Some(message) => BenchmarkResult::failed(stage, workload, message),
                None => BenchmarkResult::measured(*stage, workload, Statistics::new(samples)),
            });
        }
        BenchmarkReport {
            context: context.clone(),
            config: config.clone(),
            results,
        }
    }
}

fn run_once(workload: &mut Workload<'_>) -> Result<(), String> {
    let result = (workload.run)();
    black_box(&result);
    result
}

/// Deterministic integer statistics over one measured stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Statistics {
    sample_count: u32,
    min_ns: u64,
    max_ns: u64,
    mean_ns: u64,
    p50_ns: u64,
    p95_ns: u64,
}

impl Statistics {
    fn new(mut samples: Vec<u64>) -> Self {
        samples.sort_unstable();
        let sample_count = samples.len() as u32;
        let total = samples.iter().map(|value| u128::from(*value)).sum::<u128>();
        Self {
            sample_count,
            min_ns: samples[0],
            max_ns: samples[samples.len() - 1],
            mean_ns: u64::try_from(total / u128::from(sample_count)).unwrap_or(u64::MAX),
            p50_ns: percentile(&samples, 50),
            p95_ns: percentile(&samples, 95),
        }
    }

    pub const fn sample_count(&self) -> u32 {
        self.sample_count
    }
    pub const fn min_ns(&self) -> u64 {
        self.min_ns
    }
    pub const fn max_ns(&self) -> u64 {
        self.max_ns
    }
    pub const fn mean_ns(&self) -> u64 {
        self.mean_ns
    }
    pub const fn p50_ns(&self) -> u64 {
        self.p50_ns
    }
    pub const fn p95_ns(&self) -> u64 {
        self.p95_ns
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    let rank = (percentile * sorted.len()).div_ceil(100).saturating_sub(1);
    sorted[rank]
}

/// Honest outcome for one required stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BenchmarkStatus {
    Measured(Statistics),
    Gap(String),
    Failed(String),
}

/// One stage result with workload provenance when registered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenchmarkResult {
    stage: BenchmarkStage,
    workload_name: Option<String>,
    fixture_id: Option<String>,
    status: BenchmarkStatus,
}

impl BenchmarkResult {
    fn gap(stage: BenchmarkStage) -> Self {
        Self {
            stage,
            workload_name: None,
            fixture_id: None,
            status: BenchmarkStatus::Gap(GAP_REASON.into()),
        }
    }

    fn failed(stage: &BenchmarkStage, workload: &Workload<'_>, message: String) -> Self {
        Self {
            stage: *stage,
            workload_name: Some(workload.name.clone()),
            fixture_id: Some(workload.fixture_id.clone()),
            status: BenchmarkStatus::Failed(message),
        }
    }

    fn measured(stage: BenchmarkStage, workload: &Workload<'_>, statistics: Statistics) -> Self {
        Self {
            stage,
            workload_name: Some(workload.name.clone()),
            fixture_id: Some(workload.fixture_id.clone()),
            status: BenchmarkStatus::Measured(statistics),
        }
    }

    pub const fn stage(&self) -> BenchmarkStage {
        self.stage
    }
    pub fn workload_name(&self) -> Option<&str> {
        self.workload_name.as_deref()
    }
    pub fn fixture_id(&self) -> Option<&str> {
        self.fixture_id.as_deref()
    }
    pub const fn status(&self) -> &BenchmarkStatus {
        &self.status
    }
}

/// One complete, machine-readable benchmark run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenchmarkReport {
    context: BenchmarkContext,
    config: BenchmarkConfig,
    results: Vec<BenchmarkResult>,
}

impl BenchmarkReport {
    pub fn results(&self) -> &[BenchmarkResult] {
        &self.results
    }
    pub fn result(&self, stage: BenchmarkStage) -> Option<&BenchmarkResult> {
        self.results.iter().find(|result| result.stage == stage)
    }
    pub fn has_failures(&self) -> bool {
        self.results
            .iter()
            .any(|result| matches!(result.status, BenchmarkStatus::Failed(_)))
    }

    /// Serializes a stable schema without adding a runtime dependency.
    #[must_use]
    pub fn to_json(&self) -> String {
        let fields = self.context.fields();
        let mut output = String::from("{\"schema_version\":1,\"context_complete\":");
        output.push_str(if self.context.is_complete() {
            "true"
        } else {
            "false"
        });
        output.push_str(",\"context\":{");
        let text_fields = [
            ("build", fields.build.as_str()),
            ("operating_system", fields.operating_system.as_str()),
            ("architecture", fields.architecture.as_str()),
            ("cpu", fields.cpu.as_str()),
        ];
        append_text_fields(&mut output, &text_fields, false);
        output.push_str(",\"memory_mib\":");
        output.push_str(&fields.memory_mib.to_string());
        append_text_fields(
            &mut output,
            &[
                ("gpu_backend", fields.gpu_backend.as_str()),
                ("gpu_driver", fields.gpu_driver.as_str()),
                ("cache_state", fields.cache_state.as_str()),
                ("hardware_tier", fields.hardware_tier.as_str()),
                ("fixture_revision", fields.fixture_revision.as_str()),
                ("project_revision", fields.project_revision.as_str()),
            ],
            true,
        );
        output.push_str("},\"config\":{");
        output.push_str(&format!(
            "\"warmup_iterations\":{},\"measured_iterations\":{}",
            self.config.warmup_iterations, self.config.measured_iterations
        ));
        output.push_str("},\"results\":[");
        for (index, result) in self.results.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            output.push_str("{\"stage\":");
            append_json_string(&mut output, result.stage.code());
            if let Some(name) = &result.workload_name {
                output.push_str(",\"workload\":");
                append_json_string(&mut output, name);
            }
            if let Some(fixture) = &result.fixture_id {
                output.push_str(",\"fixture_id\":");
                append_json_string(&mut output, fixture);
            }
            match &result.status {
                BenchmarkStatus::Measured(statistics) => {
                    output.push_str(",\"status\":\"measured\",\"statistics\":{");
                    output.push_str(&format!("\"sample_count\":{},\"min_ns\":{},\"max_ns\":{},\"mean_ns\":{},\"p50_ns\":{},\"p95_ns\":{}", statistics.sample_count, statistics.min_ns, statistics.max_ns, statistics.mean_ns, statistics.p50_ns, statistics.p95_ns));
                    output.push('}');
                }
                BenchmarkStatus::Gap(reason) => {
                    output.push_str(",\"status\":\"gap\",\"reason\":");
                    append_json_string(&mut output, reason);
                }
                BenchmarkStatus::Failed(message) => {
                    output.push_str(",\"status\":\"failed\",\"message\":");
                    append_json_string(&mut output, message);
                }
            }
            output.push('}');
        }
        output.push_str("]}");
        output
    }
}

fn append_text_fields(output: &mut String, fields: &[(&str, &str)], leading_comma: bool) {
    for (index, (name, value)) in fields.iter().enumerate() {
        if leading_comma || index > 0 {
            output.push(',');
        }
        append_json_string(output, name);
        output.push(':');
        append_json_string(output, value);
    }
}

fn append_json_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value if value.is_control() => output.push_str(&format!("\\u{:04x}", u32::from(value))),
            value => output.push(value),
        }
    }
    output.push('"');
}
