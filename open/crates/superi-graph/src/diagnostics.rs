//! Deterministic node introspection, cache identity, and run-local evaluation timing.
//!
//! Semantic inspection is deliberately separate from measured timing. An inspection can be built
//! before node values execute and is stable for equal graph state and requests. Timings describe
//! only one run and never participate in evaluator result or cache-key equality.

use std::fmt;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::dag::{GraphEdge, GraphEndpoint};
use crate::eval::{EvaluationKey, EvaluationResult, EvaluationSchedule};
use crate::ids::{EdgeId, GraphId};
use crate::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, RoiBehavior,
    TimeBehavior,
};

const NODE_STATE_DOMAIN: &[u8] = b"superi.graph.node-state.v1\0";
const CACHE_KEY_DOMAIN: &[u8] = b"superi.graph.evaluation-cache-key.v1\0";

/// A digest of every canonical editable state byte that can affect one node's result.
///
/// Deterministic nodes must include all result-affecting state. Seeded nodes must also include the
/// exact seed. The digest is an identity input, not proof that arbitrary source bytes were
/// canonicalized correctly by the node provider.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeStateFingerprint([u8; 32]);

impl NodeStateFingerprint {
    /// Hashes one caller-owned canonical state encoding with a versioned domain separator.
    #[must_use]
    pub fn from_canonical_bytes(bytes: impl AsRef<[u8]>) -> Self {
        let bytes = bytes.as_ref();
        let mut hasher = Sha256::new();
        hasher.update(NODE_STATE_DOMAIN);
        update_bytes(&mut hasher, bytes);
        Self(hasher.finalize().into())
    }

    /// Wraps a previously computed SHA-256 digest of the same canonical state contract.
    #[must_use]
    pub const fn from_sha256(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for NodeStateFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_digest(formatter, &self.0)
    }
}

/// Deterministic schema, behavior, and editable state identity for one evaluator node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeIntrospection {
    schema_id: NodeSchemaId,
    behavior: NodeBehavior,
    state_fingerprint: NodeStateFingerprint,
}

impl NodeIntrospection {
    /// Creates complete introspection for one exact node implementation state.
    #[must_use]
    pub const fn new(
        schema_id: NodeSchemaId,
        behavior: NodeBehavior,
        state_fingerprint: NodeStateFingerprint,
    ) -> Self {
        Self {
            schema_id,
            behavior,
            state_fingerprint,
        }
    }

    /// Returns the exact schema identity.
    #[must_use]
    pub const fn schema_id(&self) -> &NodeSchemaId {
        &self.schema_id
    }

    /// Returns the complete declared evaluation behavior.
    #[must_use]
    pub const fn behavior(&self) -> NodeBehavior {
        self.behavior
    }

    /// Returns the canonical editable-state fingerprint.
    #[must_use]
    pub const fn state_fingerprint(&self) -> NodeStateFingerprint {
        self.state_fingerprint
    }
}

/// Supplies deterministic metadata for evaluator inspection and cache-key construction.
///
/// This is separate from value evaluation so the existing node-neutral evaluator remains useful
/// to lightweight callers. Diagnostic evaluation requires both contracts and executes the same
/// private plan and value path as ordinary evaluation.
pub trait IntrospectNode {
    /// Captures the exact schema, behavior, and current editable-state identity.
    fn introspection(&self) -> NodeIntrospection;
}

/// One versioned SHA-256 identity for reusable evaluator output.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvaluationCacheKey([u8; 32]);

impl EvaluationCacheKey {
    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for EvaluationCacheKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_digest(formatter, &self.0)
    }
}

/// Why one work unit has or does not have a reusable cache identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CacheKeyStatus {
    /// A complete reusable identity is available.
    Available(EvaluationCacheKey),
    /// The node schema explicitly disables result retention.
    DisabledByPolicy,
    /// The node declares that equal inputs and state may still produce different results.
    NonDeterministic,
    /// One incoming edge resolves to work without a reusable identity.
    BlockedByDependency {
        /// The first blocking edge in canonical dependency order.
        edge_id: EdgeId,
    },
}

impl CacheKeyStatus {
    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Available(_) => "available",
            Self::DisabledByPolicy => "disabled_by_policy",
            Self::NonDeterministic => "non_deterministic",
            Self::BlockedByDependency { .. } => "blocked_by_dependency",
        }
    }

    /// Returns the reusable key when one is available.
    #[must_use]
    pub const fn key(self) -> Option<EvaluationCacheKey> {
        match self {
            Self::Available(key) => Some(key),
            Self::DisabledByPolicy | Self::NonDeterministic | Self::BlockedByDependency { .. } => {
                None
            }
        }
    }

    /// Returns the exact edge preventing a dependent key.
    #[must_use]
    pub const fn blocking_edge(self) -> Option<EdgeId> {
        match self {
            Self::BlockedByDependency { edge_id } => Some(edge_id),
            Self::Available(_) | Self::DisabledByPolicy | Self::NonDeterministic => None,
        }
    }
}

/// Deterministic metadata and cache identity for one planned work unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeInspection {
    key: EvaluationKey,
    introspection: NodeIntrospection,
    cache_status: CacheKeyStatus,
}

impl NodeInspection {
    pub(crate) const fn new(
        key: EvaluationKey,
        introspection: NodeIntrospection,
        cache_status: CacheKeyStatus,
    ) -> Self {
        Self {
            key,
            introspection,
            cache_status,
        }
    }

    /// Returns the exact planned work key.
    #[must_use]
    pub const fn key(&self) -> EvaluationKey {
        self.key
    }

    /// Returns deterministic node metadata captured before value execution.
    #[must_use]
    pub const fn introspection(&self) -> &NodeIntrospection {
        &self.introspection
    }

    /// Returns the cache-key decision for this exact work unit.
    #[must_use]
    pub const fn cache_status(&self) -> &CacheKeyStatus {
        &self.cache_status
    }
}

/// A deterministic pre-execution view of one exact evaluator plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationInspection {
    schedule: EvaluationSchedule,
    nodes: Vec<NodeInspection>,
}

impl EvaluationInspection {
    pub(crate) const fn new(schedule: EvaluationSchedule, nodes: Vec<NodeInspection>) -> Self {
        Self { schedule, nodes }
    }

    /// Returns the exact schedule inspected before value execution.
    #[must_use]
    pub const fn schedule(&self) -> &EvaluationSchedule {
        &self.schedule
    }

    /// Returns reached nodes in deterministic dependency-completion order.
    #[must_use]
    pub fn nodes(&self) -> &[NodeInspection] {
        &self.nodes
    }

    /// Looks up one reached work unit by exact key.
    #[must_use]
    pub fn node(&self, key: EvaluationKey) -> Option<&NodeInspection> {
        self.nodes.iter().find(|node| node.key == key)
    }
}

/// Monotonic elapsed time for one completed node implementation call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeTiming {
    key: EvaluationKey,
    elapsed: Duration,
}

impl NodeTiming {
    pub(crate) const fn new(key: EvaluationKey, elapsed: Duration) -> Self {
        Self { key, elapsed }
    }

    /// Returns the exact completed work key.
    #[must_use]
    pub const fn key(self) -> EvaluationKey {
        self.key
    }

    /// Returns time spent inside the node implementation call.
    #[must_use]
    pub const fn elapsed(self) -> Duration {
        self.elapsed
    }
}

/// Deterministic inspection plus run-local planning, execution, and node timings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationDiagnostics {
    inspection: EvaluationInspection,
    planning_elapsed: Duration,
    execution_elapsed: Duration,
    node_timings: Vec<NodeTiming>,
}

impl EvaluationDiagnostics {
    pub(crate) const fn new(
        inspection: EvaluationInspection,
        planning_elapsed: Duration,
        execution_elapsed: Duration,
        node_timings: Vec<NodeTiming>,
    ) -> Self {
        Self {
            inspection,
            planning_elapsed,
            execution_elapsed,
            node_timings,
        }
    }

    /// Returns deterministic semantic inspection for this run.
    #[must_use]
    pub const fn inspection(&self) -> &EvaluationInspection {
        &self.inspection
    }

    /// Returns time spent discovering work and building semantic inspection.
    #[must_use]
    pub const fn planning_elapsed(&self) -> Duration {
        self.planning_elapsed
    }

    /// Returns time spent executing the complete private plan.
    #[must_use]
    pub const fn execution_elapsed(&self) -> Duration {
        self.execution_elapsed
    }

    /// Returns per-node implementation timings in result completion order.
    #[must_use]
    pub fn node_timings(&self) -> &[NodeTiming] {
        &self.node_timings
    }

    /// Looks up timing for one completed work key.
    #[must_use]
    pub fn timing(&self, key: EvaluationKey) -> Option<NodeTiming> {
        self.node_timings
            .iter()
            .find(|timing| timing.key == key)
            .copied()
    }
}

/// One unchanged evaluator result paired with graph-specific diagnostics for the same private plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationReport<V> {
    result: EvaluationResult<V>,
    diagnostics: EvaluationDiagnostics,
}

impl<V> EvaluationReport<V> {
    pub(crate) const fn new(
        result: EvaluationResult<V>,
        diagnostics: EvaluationDiagnostics,
    ) -> Self {
        Self {
            result,
            diagnostics,
        }
    }

    /// Returns the ordinary semantic evaluator result.
    #[must_use]
    pub const fn result(&self) -> &EvaluationResult<V> {
        &self.result
    }

    /// Returns deterministic inspection and run-local timings.
    #[must_use]
    pub const fn diagnostics(&self) -> &EvaluationDiagnostics {
        &self.diagnostics
    }

    /// Consumes the report and returns the ordinary semantic result.
    #[must_use]
    pub fn into_result(self) -> EvaluationResult<V> {
        self.result
    }
}

pub(crate) fn derive_cache_status(
    graph_id: GraphId,
    key: EvaluationKey,
    introspection: &NodeIntrospection,
    dependencies: &[(GraphEdge, EvaluationCacheKey)],
) -> CacheKeyStatus {
    let behavior = introspection.behavior();
    if behavior.cache_policy() == CachePolicy::Disabled {
        return CacheKeyStatus::DisabledByPolicy;
    }
    if behavior.determinism() == Determinism::NonDeterministic {
        return CacheKeyStatus::NonDeterministic;
    }

    let mut hasher = Sha256::new();
    hasher.update(CACHE_KEY_DOMAIN);
    update_text(&mut hasher, &graph_id.to_string());
    update_text(&mut hasher, &introspection.schema_id().to_string());
    hasher.update(introspection.state_fingerprint().as_bytes());
    update_endpoint(&mut hasher, key.output());
    update_behavior(&mut hasher, behavior);

    match behavior.cache_policy() {
        CachePolicy::Disabled => unreachable!("disabled policy returned before hashing"),
        CachePolicy::Static => {}
        CachePolicy::PerFrame => update_time(&mut hasher, key.frame()),
        CachePolicy::PerRegion => {
            update_time(&mut hasher, key.frame());
            update_region(&mut hasher, key.region());
        }
    }

    update_u64(&mut hasher, dependencies.len() as u64);
    for (edge, dependency_key) in dependencies {
        update_text(&mut hasher, &edge.id().to_string());
        update_endpoint(&mut hasher, edge.source());
        update_endpoint(&mut hasher, edge.destination());
        hasher.update(dependency_key.as_bytes());
    }

    CacheKeyStatus::Available(EvaluationCacheKey(hasher.finalize().into()))
}

fn update_behavior(hasher: &mut Sha256, behavior: NodeBehavior) {
    match behavior.time() {
        TimeBehavior::Invariant => hasher.update(b"time:invariant\0"),
        TimeBehavior::CurrentFrame => hasher.update(b"time:current-frame\0"),
        TimeBehavior::FrameWindow {
            frames_before,
            frames_after,
        } => {
            hasher.update(b"time:frame-window\0");
            hasher.update(frames_before.to_be_bytes());
            hasher.update(frames_after.to_be_bytes());
        }
        TimeBehavior::Unbounded => hasher.update(b"time:unbounded\0"),
    }
    match behavior.roi() {
        RoiBehavior::FullFrame => hasher.update(b"roi:full-frame\0"),
        RoiBehavior::InputBounds => hasher.update(b"roi:input-bounds\0"),
        RoiBehavior::Expanded { pixels } => {
            hasher.update(b"roi:expanded\0");
            hasher.update(pixels.to_be_bytes());
        }
        RoiBehavior::Custom => hasher.update(b"roi:custom\0"),
    }
    match behavior.color() {
        ColorRequirements::NotApplicable => hasher.update(b"color:not-applicable\0"),
        ColorRequirements::Tagged => hasher.update(b"color:tagged\0"),
        ColorRequirements::Exact(color) => {
            hasher.update(b"color:exact\0");
            update_text(hasher, color.primaries().code());
            update_text(hasher, color.transfer().code());
            update_text(hasher, color.matrix().code());
            update_text(hasher, color.range().code());
        }
    }
    hasher.update(match behavior.determinism() {
        Determinism::Deterministic => b"determinism:deterministic\0".as_slice(),
        Determinism::Seeded => b"determinism:seeded\0".as_slice(),
        Determinism::NonDeterministic => b"determinism:non-deterministic\0".as_slice(),
    });
    hasher.update(match behavior.cache_policy() {
        CachePolicy::Disabled => b"cache:disabled\0".as_slice(),
        CachePolicy::Static => b"cache:static\0".as_slice(),
        CachePolicy::PerFrame => b"cache:per-frame\0".as_slice(),
        CachePolicy::PerRegion => b"cache:per-region\0".as_slice(),
    });
}

fn update_endpoint(hasher: &mut Sha256, endpoint: GraphEndpoint) {
    update_text(hasher, &endpoint.node_id().to_string());
    update_text(hasher, &endpoint.port_id().to_string());
}

fn update_time(hasher: &mut Sha256, time: superi_core::time::RationalTime) {
    let numerator = i128::from(time.value()) * i128::from(time.timebase().denominator());
    let denominator = i128::from(time.timebase().numerator());
    let divisor = greatest_common_divisor_i128(numerator.abs(), denominator);
    hasher.update((numerator / divisor).to_be_bytes());
    hasher.update((denominator / divisor).to_be_bytes());
}

fn update_region(hasher: &mut Sha256, region: superi_core::geometry::PixelBounds) {
    hasher.update(region.min_x().to_be_bytes());
    hasher.update(region.min_y().to_be_bytes());
    hasher.update(region.max_x().to_be_bytes());
    hasher.update(region.max_y().to_be_bytes());
}

fn update_text(hasher: &mut Sha256, value: &str) {
    update_bytes(hasher, value.as_bytes());
}

fn update_bytes(hasher: &mut Sha256, value: &[u8]) {
    update_u64(hasher, value.len() as u64);
    hasher.update(value);
}

fn update_u64(hasher: &mut Sha256, value: u64) {
    hasher.update(value.to_be_bytes());
}

fn greatest_common_divisor_i128(mut left: i128, mut right: i128) -> i128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn write_digest(formatter: &mut fmt::Formatter<'_>, digest: &[u8; 32]) -> fmt::Result {
    formatter.write_str("sha256:")?;
    for byte in digest {
        write!(formatter, "{byte:02x}")?;
    }
    Ok(())
}
