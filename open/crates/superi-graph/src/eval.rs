//! Lazy, per-frame, per-region graph evaluation.
//!
//! Evaluation is a request-scoped pull through the stored DAG. Node payloads declare the incoming
//! edge, frame, and region work needed for one output, then receive only those resolved values.
//! Identical request keys execute once within the pull. The evaluator publishes deterministic
//! dependency-ready batches while retaining no cross-request state, outer job policy, dirty-region
//! policy, or domain catalog knowledge.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::geometry::PixelBounds;
use superi_core::time::RationalTime;

use crate::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use crate::diagnostics::{
    derive_cache_status, CacheKeyStatus, EvaluationCacheKey, EvaluationDiagnostics,
    EvaluationInspection, EvaluationReport, IntrospectNode, NodeInspection, NodeTiming,
};
use crate::ids::{EdgeId, GraphId};
use crate::node::{CachePolicy, Determinism};

const COMPONENT: &str = "superi-graph.eval";

/// One exact unit of evaluator work.
///
/// Physical time equality follows [`RationalTime`], so equivalent coordinates in different
/// timebases identify the same request-local work. Pixel bounds retain their exact signed,
/// half-open meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationKey {
    output: GraphEndpoint,
    frame: RationalTime,
    region: PixelBounds,
}

impl Ord for EvaluationKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.output
            .cmp(&other.output)
            .then_with(|| {
                self.frame
                    .partial_cmp(&other.frame)
                    .expect("rational time provides a total physical order")
            })
            .then_with(|| region_key(self.region).cmp(&region_key(other.region)))
    }
}

impl PartialOrd for EvaluationKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl EvaluationKey {
    /// Creates an exact endpoint, frame, and region work key.
    #[must_use]
    pub const fn new(output: GraphEndpoint, frame: RationalTime, region: PixelBounds) -> Self {
        Self {
            output,
            frame,
            region,
        }
    }

    /// Returns the requested output endpoint.
    #[must_use]
    pub const fn output(self) -> GraphEndpoint {
        self.output
    }

    /// Returns the exact requested frame.
    #[must_use]
    pub const fn frame(self) -> RationalTime {
        self.frame
    }

    /// Returns the exact requested pixel region.
    #[must_use]
    pub const fn region(self) -> PixelBounds {
        self.region
    }
}

/// The top-level work requested by an evaluator caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationRequest {
    key: EvaluationKey,
}

impl EvaluationRequest {
    /// Creates an evaluator request for one output endpoint, frame, and region.
    #[must_use]
    pub const fn new(output: GraphEndpoint, frame: RationalTime, region: PixelBounds) -> Self {
        Self {
            key: EvaluationKey::new(output, frame, region),
        }
    }

    /// Returns the complete work key.
    #[must_use]
    pub const fn key(self) -> EvaluationKey {
        self.key
    }

    /// Returns the requested output endpoint.
    #[must_use]
    pub const fn output(self) -> GraphEndpoint {
        self.key.output()
    }

    /// Returns the exact requested frame.
    #[must_use]
    pub const fn frame(self) -> RationalTime {
        self.key.frame()
    }

    /// Returns the exact requested pixel region.
    #[must_use]
    pub const fn region(self) -> PixelBounds {
        self.key.region()
    }
}

impl From<EvaluationKey> for EvaluationRequest {
    fn from(key: EvaluationKey) -> Self {
        Self { key }
    }
}

/// The retention tier used for one reusable evaluator value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvaluationCacheEntryKind {
    /// The exact top-level output requested by an evaluator caller.
    FinalFrame,
    /// One prerequisite node output reached while resolving a final frame.
    IntermediateNode,
}

/// The graph-owned identity inputs for one retained evaluator value.
///
/// `graph_id` and `graph_revision` identify editable lineage when evaluation comes from a published
/// snapshot. `graph_key` covers graph topology, node state, request scope, behavior, and upstream
/// lineage. `evaluation_key` exposes the exact endpoint, physical time, and region so an outer
/// cache owner can compose graph identity with media, parameter, color, and render-setting identity
/// without moving those concerns into the graph crate. Direct immutable DAG evaluation has no
/// editable revision and reports `None` conservatively.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EvaluationCacheIdentity {
    graph_id: GraphId,
    graph_revision: Option<u64>,
    graph_key: EvaluationCacheKey,
    evaluation_key: EvaluationKey,
}

impl EvaluationCacheIdentity {
    const fn new(
        graph_id: GraphId,
        graph_revision: Option<u64>,
        graph_key: EvaluationCacheKey,
        evaluation_key: EvaluationKey,
    ) -> Self {
        Self {
            graph_id,
            graph_revision,
            graph_key,
            evaluation_key,
        }
    }

    /// Returns the stable graph identity that owns this retained work.
    #[must_use]
    pub const fn graph_id(self) -> GraphId {
        self.graph_id
    }

    /// Returns the published editable revision, when evaluation came from one.
    #[must_use]
    pub const fn graph_revision(self) -> Option<u64> {
        self.graph_revision
    }

    /// Returns the deterministic graph-lineage component.
    #[must_use]
    pub const fn graph_key(self) -> EvaluationCacheKey {
        self.graph_key
    }

    /// Returns the exact evaluator work identity.
    #[must_use]
    pub const fn evaluation_key(self) -> EvaluationKey {
        self.evaluation_key
    }
}

/// Node-neutral retained value storage consumed by cached graph evaluation.
///
/// Implementations own synchronization, memory placement, and retention policy. A lookup miss or
/// insertion replacement is ordinary cache behavior and cannot change graph meaning. The evaluator
/// calls this interface only for work with complete graph-owned identity. Outer implementations may
/// compose [`EvaluationCacheIdentity`] with additional authoritative identity categories.
pub trait EvaluationValueCache<V> {
    /// Returns an owned reusable value from the requested retention tier.
    fn get(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity) -> Option<V>;

    /// Retains one exact evaluator value in the requested tier.
    fn insert(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity, value: V);
}

/// One incoming edge request declared by a node implementation.
///
/// The evaluator validates that `edge_id` names an edge entering the node being evaluated. The
/// source endpoint comes only from that stored edge, so a node cannot invent routing outside the
/// authoritative DAG.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationDependency {
    edge_id: EdgeId,
    frame: RationalTime,
    region: PixelBounds,
}

impl EvaluationDependency {
    /// Creates an incoming edge request at an explicit frame and region.
    #[must_use]
    pub const fn new(edge_id: EdgeId, frame: RationalTime, region: PixelBounds) -> Self {
        Self {
            edge_id,
            frame,
            region,
        }
    }

    /// Creates an incoming edge request at the current output frame and region.
    #[must_use]
    pub const fn same_request(edge_id: EdgeId, request: EvaluationRequest) -> Self {
        Self::new(edge_id, request.frame(), request.region())
    }

    /// Returns the stored incoming edge identity.
    #[must_use]
    pub const fn edge_id(self) -> EdgeId {
        self.edge_id
    }

    /// Returns the source frame requested through the edge.
    #[must_use]
    pub const fn frame(self) -> RationalTime {
        self.frame
    }

    /// Returns the source pixel region requested through the edge.
    #[must_use]
    pub const fn region(self) -> PixelBounds {
        self.region
    }
}

/// One resolved input supplied to a node implementation.
pub struct EvaluationInput<'a, V> {
    dependency: EvaluationDependency,
    edge: GraphEdge,
    source: EvaluationKey,
    value: &'a V,
}

impl<'a, V> EvaluationInput<'a, V> {
    /// Returns the node-declared dependency request.
    #[must_use]
    pub const fn dependency(&self) -> EvaluationDependency {
        self.dependency
    }

    /// Returns the authoritative stored edge used for this input.
    #[must_use]
    pub const fn edge(&self) -> GraphEdge {
        self.edge
    }

    /// Returns the exact source work key that produced the value.
    #[must_use]
    pub const fn source(&self) -> EvaluationKey {
        self.source
    }

    /// Borrows the resolved evaluator-owned value.
    #[must_use]
    pub const fn value(&self) -> &'a V {
        self.value
    }
}

/// Immutable inputs for one node output evaluation.
pub struct EvaluationContext<'a, V> {
    request: EvaluationRequest,
    inputs: &'a [EvaluationInput<'a, V>],
}

impl<'a, V> EvaluationContext<'a, V> {
    /// Returns the node output request being evaluated.
    #[must_use]
    pub const fn request(&self) -> EvaluationRequest {
        self.request
    }

    /// Returns resolved inputs in evaluator-owned canonical dependency order.
    #[must_use]
    pub const fn inputs(&self) -> &'a [EvaluationInput<'a, V>] {
        self.inputs
    }
}

/// Node payload behavior consumed by the lazy evaluator.
///
/// The default dependency declaration requests every stored incoming edge at the output request's
/// exact frame and region. Nodes with editable branch, temporal, or spatial behavior may return a
/// smaller or different set of incoming requests. Evaluation remains node-type-neutral because
/// the payload owns that policy.
pub trait EvaluateNode<V> {
    /// Declares the incoming work required for one output request.
    fn dependencies(
        &self,
        request: EvaluationRequest,
        incoming: &[GraphEdge],
    ) -> Result<Vec<EvaluationDependency>> {
        Ok(incoming
            .iter()
            .map(|edge| EvaluationDependency::same_request(edge.id(), request))
            .collect())
    }

    /// Evaluates one output from the resolved declared inputs.
    fn evaluate(&self, context: &EvaluationContext<'_, V>) -> Result<V>;
}

/// One deterministic batch of independent evaluator work.
///
/// Every prerequisite of every key in this batch appears in an earlier batch. Keys within a batch
/// are independent and use the evaluator's stable work-key order, so a later render coordinator
/// may dispatch them concurrently without changing dependency or result meaning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationBatch {
    keys: Vec<EvaluationKey>,
}

impl EvaluationBatch {
    /// Returns ready work in deterministic key order.
    #[must_use]
    pub fn keys(&self) -> &[EvaluationKey] {
        &self.keys
    }
}

/// An immutable request-local evaluation schedule.
///
/// The schedule contains only work reached through node-declared stored edges. It owns no values,
/// cache state, invalidation state, worker priority, cancellation, or caller mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationSchedule {
    request: EvaluationRequest,
    batches: Vec<EvaluationBatch>,
    prerequisites: BTreeMap<EvaluationKey, Vec<EvaluationKey>>,
}

impl EvaluationSchedule {
    /// Returns the top-level request that owns this schedule.
    #[must_use]
    pub const fn request(&self) -> EvaluationRequest {
        self.request
    }

    /// Returns deterministic dependency-ready batches.
    #[must_use]
    pub fn batches(&self) -> &[EvaluationBatch] {
        &self.batches
    }

    /// Returns the unique prerequisite work keys for one scheduled key.
    ///
    /// Keys are ordered independently of declaration and insertion history. Distinct input edges
    /// that reuse one value remain distinct node inputs even though this dependency set contains
    /// the source work only once.
    #[must_use]
    pub fn prerequisites(&self, key: EvaluationKey) -> Option<&[EvaluationKey]> {
        self.prerequisites.get(&key).map(Vec::as_slice)
    }

    /// Returns the number of unique work keys in this schedule.
    #[must_use]
    pub fn len(&self) -> usize {
        self.prerequisites.len()
    }

    /// Returns whether no evaluator work is scheduled.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.prerequisites.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EvaluatedValue<V> {
    key: EvaluationKey,
    value: V,
}

/// One completed request and all request-local values that were actually needed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationResult<V> {
    request: EvaluationRequest,
    target_index: usize,
    evaluated: Vec<EvaluatedValue<V>>,
    schedule: EvaluationSchedule,
}

impl<V> EvaluationResult<V> {
    /// Returns the original top-level request.
    #[must_use]
    pub const fn request(&self) -> EvaluationRequest {
        self.request
    }

    /// Borrows the requested output value.
    #[must_use]
    pub fn value(&self) -> &V {
        &self.evaluated[self.target_index].value
    }

    /// Returns the exact schedule used to produce this result.
    #[must_use]
    pub const fn schedule(&self) -> &EvaluationSchedule {
        &self.schedule
    }

    /// Returns work keys in stable dependency-completion order.
    pub fn evaluated_keys(&self) -> impl ExactSizeIterator<Item = EvaluationKey> + '_ {
        self.evaluated.iter().map(|entry| entry.key)
    }

    /// Borrows one request-local value by exact work key.
    #[must_use]
    pub fn value_for(&self, key: EvaluationKey) -> Option<&V> {
        self.evaluated
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| &entry.value)
    }
}

/// Stateless entry point for one lazy graph pull.
pub struct LazyEvaluator;

impl LazyEvaluator {
    /// Builds an inspectable deterministic schedule without evaluating node values.
    ///
    /// Dependency declarations are read from the immutable graph for this call. The returned
    /// schedule is diagnostic state, not a reusable cache. [`Self::evaluate`] always builds and
    /// executes one private schedule so current editable payload state is observed atomically with
    /// respect to the caller's graph borrow.
    pub fn schedule<N, V>(
        graph: &DirectedAcyclicGraph<N>,
        request: EvaluationRequest,
    ) -> Result<EvaluationSchedule>
    where
        N: EvaluateNode<V>,
    {
        PlanBuilder::build::<V>(graph, request).map(|plan| plan.schedule)
    }

    /// Builds deterministic node introspection and cache-key decisions without evaluating values.
    ///
    /// The inspection is derived from the same private plan consumed by diagnostic evaluation. It
    /// contains no timing, cache contents, cache hits, invalidation generations, or caller mode.
    pub fn inspect<N, V>(
        graph: &DirectedAcyclicGraph<N>,
        request: EvaluationRequest,
    ) -> Result<EvaluationInspection>
    where
        N: EvaluateNode<V> + IntrospectNode,
    {
        let plan = PlanBuilder::build::<V>(graph, request)?;
        Ok(build_inspection(graph, &plan))
    }

    /// Evaluates one output endpoint, frame, and region from an immutable graph.
    ///
    /// Every call owns a fresh request-local value set, so no prior graph state or result can be
    /// reused after an edit. Identical keys reached more than once during this call execute once.
    pub fn evaluate<N, V>(
        graph: &DirectedAcyclicGraph<N>,
        request: EvaluationRequest,
    ) -> Result<EvaluationResult<V>>
    where
        N: EvaluateNode<V>,
    {
        let plan = PlanBuilder::build::<V>(graph, request)?;
        execute_plan(graph, plan, None).map(|(result, _)| result)
    }

    /// Evaluates one output while reusing exact final and intermediate values when available.
    ///
    /// Cache identity comes only from deterministic graph inspection. A final-frame hit returns
    /// immediately, while an intermediate-node hit prunes that node's complete prerequisite
    /// subtree. Misses execute through the ordinary immutable node contract and retain a clone of
    /// the exact result, leaving the returned value and authored graph unchanged.
    pub fn evaluate_with_cache<N, V, C>(
        graph: &DirectedAcyclicGraph<N>,
        request: EvaluationRequest,
        cache: &C,
    ) -> Result<EvaluationResult<V>>
    where
        N: EvaluateNode<V> + IntrospectNode,
        V: Clone,
        C: EvaluationValueCache<V>,
    {
        Self::evaluate_with_cache_at_revision(graph, request, cache, None)
    }

    pub(crate) fn evaluate_with_cache_at_revision<N, V, C>(
        graph: &DirectedAcyclicGraph<N>,
        request: EvaluationRequest,
        cache: &C,
        graph_revision: Option<u64>,
    ) -> Result<EvaluationResult<V>>
    where
        N: EvaluateNode<V> + IntrospectNode,
        V: Clone,
        C: EvaluationValueCache<V>,
    {
        let plan = PlanBuilder::build::<V>(graph, request)?;
        let inspection = build_inspection(graph, &plan);
        execute_plan_with_cache(graph, plan, &inspection, cache, graph_revision)
    }

    /// Evaluates one private plan and returns its ordinary result beside graph diagnostics.
    ///
    /// Planning and node timings use a monotonic clock and are run-local observations. The
    /// deterministic inspection is built before values execute, and the ordinary evaluator result
    /// is produced by the same execution loop used by [`Self::evaluate`].
    pub fn evaluate_with_diagnostics<N, V>(
        graph: &DirectedAcyclicGraph<N>,
        request: EvaluationRequest,
    ) -> Result<EvaluationReport<V>>
    where
        N: EvaluateNode<V> + IntrospectNode,
    {
        let planning_started = Instant::now();
        let plan = PlanBuilder::build::<V>(graph, request)?;
        let inspection = build_inspection(graph, &plan);
        let planning_elapsed = planning_started.elapsed();

        let execution_started = Instant::now();
        let (result, node_timings) = execute_plan(graph, plan, Some(&inspection))?;
        let execution_elapsed = execution_started.elapsed();
        let diagnostics = EvaluationDiagnostics::new(
            inspection,
            planning_elapsed,
            execution_elapsed,
            node_timings,
        );
        Ok(EvaluationReport::new(result, diagnostics))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlannedDependency {
    dependency: EvaluationDependency,
    edge: GraphEdge,
    source: EvaluationKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlannedWork {
    dependencies: Vec<PlannedDependency>,
}

struct EvaluationPlan {
    schedule: EvaluationSchedule,
    work: BTreeMap<EvaluationKey, PlannedWork>,
}

struct PlanBuilder<'a, N> {
    graph: &'a DirectedAcyclicGraph<N>,
    work: BTreeMap<EvaluationKey, PlannedWork>,
    active: BTreeSet<EvaluationKey>,
}

impl<'a, N> PlanBuilder<'a, N> {
    fn build<V>(
        graph: &'a DirectedAcyclicGraph<N>,
        request: EvaluationRequest,
    ) -> Result<EvaluationPlan>
    where
        N: EvaluateNode<V>,
    {
        if graph.node(request.output().node_id()).is_none() {
            return Err(request_error(
                graph,
                request.key(),
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "requested evaluation node does not exist",
                "evaluate",
                "missing_target_node",
            ));
        }

        let mut builder = Self {
            graph,
            work: BTreeMap::new(),
            active: BTreeSet::new(),
        };
        builder.discover::<V>(request.key())?;
        let schedule = build_schedule(graph, request, &builder.work)?;
        Ok(EvaluationPlan {
            schedule,
            work: builder.work,
        })
    }

    fn discover<V>(&mut self, key: EvaluationKey) -> Result<()>
    where
        N: EvaluateNode<V>,
    {
        if self.work.contains_key(&key) {
            return Ok(());
        }
        if !self.active.insert(key) {
            return Err(request_error(
                self.graph,
                key,
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "evaluation dependency recursion reached an active request",
                "resolve_dependency",
                "evaluation_cycle",
            ));
        }

        let result = self.discover_uncached::<V>(key);
        self.active.remove(&key);
        result
    }

    fn discover_uncached<V>(&mut self, key: EvaluationKey) -> Result<()>
    where
        N: EvaluateNode<V>,
    {
        if self.graph.node(key.output().node_id()).is_none() {
            return Err(request_error(
                self.graph,
                key,
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "stored evaluation edge references a missing source node",
                "resolve_dependency",
                "missing_source_node",
            ));
        }
        let request = EvaluationRequest::from(key);
        let node_id = key.output().node_id();
        let incoming_ids = self
            .graph
            .incoming_edge_ids(node_id)
            .expect("stored evaluation node owns incoming adjacency");
        let incoming = incoming_ids
            .iter()
            .map(|edge_id| {
                *self
                    .graph
                    .edge(*edge_id)
                    .expect("stored incoming identity owns an edge")
            })
            .collect::<Vec<_>>();

        let dependencies = self
            .graph
            .node(node_id)
            .expect("evaluation node existence checked")
            .dependencies(request, &incoming)
            .with_error_context(request_context(self.graph, key, "declare_dependencies"));
        let mut dependencies = dependencies?;
        canonicalize_dependencies(&mut dependencies);

        let mut resolved = Vec::with_capacity(dependencies.len());
        for dependency in dependencies {
            let edge = self
                .graph
                .edge(dependency.edge_id())
                .copied()
                .ok_or_else(|| {
                    dependency_error(
                        self.graph,
                        key,
                        dependency,
                        "missing_dependency_edge",
                        "node declared an evaluation edge that does not exist",
                    )
                })?;
            if edge.destination().node_id() != node_id || !incoming_ids.contains(&edge.id()) {
                return Err(dependency_error(
                    self.graph,
                    key,
                    dependency,
                    "dependency_edge_not_incoming",
                    "node declared an evaluation edge that does not enter it",
                ));
            }
            resolved.push(PlannedDependency {
                dependency,
                edge,
                source: EvaluationKey::new(edge.source(), dependency.frame(), dependency.region()),
            });
        }

        for planned in &resolved {
            self.discover::<V>(planned.source)?;
        }

        self.work.insert(
            key,
            PlannedWork {
                dependencies: resolved,
            },
        );
        Ok(())
    }
}

fn build_schedule<N>(
    graph: &DirectedAcyclicGraph<N>,
    request: EvaluationRequest,
    work: &BTreeMap<EvaluationKey, PlannedWork>,
) -> Result<EvaluationSchedule> {
    let prerequisites = work
        .iter()
        .map(|(key, planned)| {
            let sources = planned
                .dependencies
                .iter()
                .map(|dependency| dependency.source)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            (*key, sources)
        })
        .collect::<BTreeMap<_, _>>();
    let mut remaining = prerequisites
        .iter()
        .map(|(key, sources)| (*key, sources.len()))
        .collect::<BTreeMap<_, _>>();
    let mut dependents: BTreeMap<EvaluationKey, BTreeSet<EvaluationKey>> = BTreeMap::new();
    for (key, sources) in &prerequisites {
        for source in sources {
            dependents.entry(*source).or_default().insert(*key);
        }
    }

    let mut ready = remaining
        .iter()
        .filter_map(|(key, count)| (*count == 0).then_some(*key))
        .collect::<BTreeSet<_>>();
    let mut batches = Vec::new();
    let mut scheduled_count = 0;

    while !ready.is_empty() {
        let keys = ready.iter().copied().collect::<Vec<_>>();
        scheduled_count += keys.len();
        let mut next_ready = BTreeSet::new();
        for key in &keys {
            if let Some(consumers) = dependents.get(key) {
                for consumer in consumers {
                    let count = remaining
                        .get_mut(consumer)
                        .expect("planned consumer owns a prerequisite count");
                    *count -= 1;
                    if *count == 0 {
                        next_ready.insert(*consumer);
                    }
                }
            }
        }
        batches.push(EvaluationBatch { keys });
        ready = next_ready;
    }

    if scheduled_count != work.len() {
        return Err(request_error(
            graph,
            request.key(),
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "evaluation work graph could not produce a complete schedule",
            "schedule",
            "evaluation_schedule_cycle",
        ));
    }

    Ok(EvaluationSchedule {
        request,
        batches,
        prerequisites,
    })
}

fn build_inspection<N>(
    graph: &DirectedAcyclicGraph<N>,
    plan: &EvaluationPlan,
) -> EvaluationInspection
where
    N: IntrospectNode,
{
    let mut cache_statuses = BTreeMap::new();
    let mut nodes = Vec::with_capacity(plan.schedule.len());

    for batch in plan.schedule.batches() {
        for key in batch.keys() {
            let planned = plan
                .work
                .get(key)
                .expect("scheduled evaluator work remains planned");
            let introspection = graph
                .node(key.output().node_id())
                .expect("planned evaluation node remains present")
                .introspection();
            let behavior = introspection.behavior();

            let cache_status = if behavior.cache_policy() == CachePolicy::Disabled {
                CacheKeyStatus::DisabledByPolicy
            } else if behavior.determinism() == Determinism::NonDeterministic {
                CacheKeyStatus::NonDeterministic
            } else {
                let mut dependencies = Vec::with_capacity(planned.dependencies.len());
                let mut blocking_edge = None;
                for dependency in &planned.dependencies {
                    let status = cache_statuses
                        .get(&dependency.source)
                        .expect("inspection prerequisite appears in an earlier batch");
                    if let Some(cache_key) = CacheKeyStatus::key(*status) {
                        dependencies.push((dependency.edge, cache_key));
                    } else {
                        blocking_edge = Some(dependency.edge.id());
                        break;
                    }
                }
                match blocking_edge {
                    Some(edge_id) => CacheKeyStatus::BlockedByDependency { edge_id },
                    None => derive_cache_status(graph.id(), *key, &introspection, &dependencies),
                }
            };

            cache_statuses.insert(*key, cache_status);
            nodes.push(NodeInspection::new(*key, introspection, cache_status));
        }
    }

    EvaluationInspection::new(plan.schedule.clone(), nodes)
}

fn execute_plan<N, V>(
    graph: &DirectedAcyclicGraph<N>,
    plan: EvaluationPlan,
    inspection: Option<&EvaluationInspection>,
) -> Result<(EvaluationResult<V>, Vec<NodeTiming>)>
where
    N: EvaluateNode<V>,
{
    let EvaluationPlan { schedule, work } = plan;
    let mut evaluated: Vec<EvaluatedValue<V>> = Vec::with_capacity(schedule.len());
    let mut completed = BTreeMap::new();
    let mut node_timings = Vec::with_capacity(inspection.map_or(0, |_| schedule.len()));

    for batch in schedule.batches() {
        for key in batch.keys() {
            let planned = work
                .get(key)
                .expect("scheduled evaluator work remains planned");
            let input_indices = planned
                .dependencies
                .iter()
                .map(|dependency| {
                    completed
                        .get(&dependency.source)
                        .copied()
                        .expect("scheduled prerequisite completed in an earlier batch")
                })
                .collect::<Vec<usize>>();

            let value = {
                let inputs = planned
                    .dependencies
                    .iter()
                    .zip(&input_indices)
                    .map(|(dependency, index)| EvaluationInput {
                        dependency: dependency.dependency,
                        edge: dependency.edge,
                        source: dependency.source,
                        value: &evaluated[*index].value,
                    })
                    .collect::<Vec<_>>();
                let context = EvaluationContext {
                    request: EvaluationRequest::from(*key),
                    inputs: &inputs,
                };
                let node = graph
                    .node(key.output().node_id())
                    .expect("planned evaluation node remains present");
                let started = inspection.map(|_| Instant::now());
                match node.evaluate(&context) {
                    Ok(value) => {
                        if let Some(started) = started {
                            node_timings.push(NodeTiming::new(*key, started.elapsed()));
                        }
                        value
                    }
                    Err(mut error) => {
                        error.push_context(request_context(graph, *key, "evaluate_node"));
                        if let (Some(inspection), Some(started)) = (inspection, started) {
                            let node = inspection
                                .node(*key)
                                .expect("diagnostic inspection contains every scheduled key");
                            error.push_context(diagnostic_context(node, started.elapsed()));
                        }
                        return Err(error);
                    }
                }
            };

            let index = evaluated.len();
            evaluated.push(EvaluatedValue { key: *key, value });
            completed.insert(*key, index);
        }
    }

    let target_index = completed[&schedule.request().key()];
    Ok((
        EvaluationResult {
            request: schedule.request(),
            target_index,
            evaluated,
            schedule,
        },
        node_timings,
    ))
}

fn execute_plan_with_cache<N, V, C>(
    graph: &DirectedAcyclicGraph<N>,
    plan: EvaluationPlan,
    inspection: &EvaluationInspection,
    cache: &C,
    graph_revision: Option<u64>,
) -> Result<EvaluationResult<V>>
where
    N: EvaluateNode<V>,
    V: Clone,
    C: EvaluationValueCache<V>,
{
    let lineage = EvaluationCacheLineage {
        graph_id: graph.id(),
        graph_revision,
    };
    let target = plan.schedule.request().key();
    if let Some(cache_key) = inspection
        .node(target)
        .and_then(|node| node.cache_status().key())
    {
        let identity = lineage.identity(cache_key, target);
        if let Some(value) = cache.get(EvaluationCacheEntryKind::FinalFrame, identity) {
            return Ok(EvaluationResult {
                request: plan.schedule.request(),
                target_index: 0,
                evaluated: vec![EvaluatedValue { key: target, value }],
                schedule: plan.schedule,
            });
        }
    }

    let mut selection = RequiredWorkSelection {
        required: BTreeSet::new(),
        retained: BTreeMap::new(),
    };
    select_required_work(
        target,
        target,
        &plan.work,
        inspection,
        cache,
        lineage,
        &mut selection,
    );
    let RequiredWorkSelection {
        required,
        mut retained,
    } = selection;

    let EvaluationPlan { schedule, work } = plan;
    let mut evaluated: Vec<EvaluatedValue<V>> = Vec::with_capacity(required.len() + retained.len());
    let mut completed = BTreeMap::new();

    for batch in schedule.batches() {
        for key in batch.keys() {
            let value = if let Some(value) = retained.remove(key) {
                value
            } else if required.contains(key) {
                let planned = work
                    .get(key)
                    .expect("required evaluator work remains planned");
                let input_indices = planned
                    .dependencies
                    .iter()
                    .map(|dependency| {
                        completed
                            .get(&dependency.source)
                            .copied()
                            .expect("required prerequisite completed in an earlier batch")
                    })
                    .collect::<Vec<usize>>();
                let inputs = planned
                    .dependencies
                    .iter()
                    .zip(&input_indices)
                    .map(|(dependency, index)| EvaluationInput {
                        dependency: dependency.dependency,
                        edge: dependency.edge,
                        source: dependency.source,
                        value: &evaluated[*index].value,
                    })
                    .collect::<Vec<_>>();
                let context = EvaluationContext {
                    request: EvaluationRequest::from(*key),
                    inputs: &inputs,
                };
                let node = graph
                    .node(key.output().node_id())
                    .expect("planned evaluation node remains present");
                let value = node.evaluate(&context).map_err(|mut error| {
                    error.push_context(request_context(graph, *key, "evaluate_node"));
                    error
                })?;
                if let Some(cache_key) = inspection
                    .node(*key)
                    .and_then(|node| node.cache_status().key())
                {
                    let kind = if *key == target {
                        EvaluationCacheEntryKind::FinalFrame
                    } else {
                        EvaluationCacheEntryKind::IntermediateNode
                    };
                    cache.insert(kind, lineage.identity(cache_key, *key), value.clone());
                }
                value
            } else {
                continue;
            };

            let index = evaluated.len();
            evaluated.push(EvaluatedValue { key: *key, value });
            completed.insert(*key, index);
        }
    }

    let target_index = completed[&target];
    Ok(EvaluationResult {
        request: schedule.request(),
        target_index,
        evaluated,
        schedule,
    })
}

#[derive(Clone, Copy)]
struct EvaluationCacheLineage {
    graph_id: GraphId,
    graph_revision: Option<u64>,
}

impl EvaluationCacheLineage {
    const fn identity(
        self,
        graph_key: EvaluationCacheKey,
        evaluation_key: EvaluationKey,
    ) -> EvaluationCacheIdentity {
        EvaluationCacheIdentity::new(
            self.graph_id,
            self.graph_revision,
            graph_key,
            evaluation_key,
        )
    }
}

struct RequiredWorkSelection<V> {
    required: BTreeSet<EvaluationKey>,
    retained: BTreeMap<EvaluationKey, V>,
}

fn select_required_work<V, C>(
    key: EvaluationKey,
    target: EvaluationKey,
    work: &BTreeMap<EvaluationKey, PlannedWork>,
    inspection: &EvaluationInspection,
    cache: &C,
    lineage: EvaluationCacheLineage,
    selection: &mut RequiredWorkSelection<V>,
) where
    C: EvaluationValueCache<V>,
{
    if selection.required.contains(&key) || selection.retained.contains_key(&key) {
        return;
    }

    if key != target {
        if let Some(cache_key) = inspection
            .node(key)
            .and_then(|node| node.cache_status().key())
        {
            let identity = lineage.identity(cache_key, key);
            if let Some(value) = cache.get(EvaluationCacheEntryKind::IntermediateNode, identity) {
                selection.retained.insert(key, value);
                return;
            }
        }
    }

    let planned = work
        .get(&key)
        .expect("selected evaluator work remains planned");
    for dependency in &planned.dependencies {
        select_required_work(
            dependency.source,
            target,
            work,
            inspection,
            cache,
            lineage,
            selection,
        );
    }
    selection.required.insert(key);
}

fn diagnostic_context(node: &NodeInspection, elapsed: std::time::Duration) -> ErrorContext {
    let mut context = ErrorContext::new(COMPONENT, "diagnose_node")
        .with_field("schema_id", node.introspection().schema_id().to_string())
        .with_field(
            "state_fingerprint",
            node.introspection().state_fingerprint().to_string(),
        )
        .with_field("cache_status", node.cache_status().code())
        .with_field("elapsed_ns", elapsed.as_nanos().to_string());
    if let Some(cache_key) = node.cache_status().key() {
        context.insert_field("cache_key", cache_key.to_string());
    }
    if let Some(edge_id) = node.cache_status().blocking_edge() {
        context.insert_field("blocking_edge_id", edge_id.to_string());
    }
    context
}

fn canonicalize_dependencies(dependencies: &mut Vec<EvaluationDependency>) {
    dependencies.sort_by(compare_dependencies);
    dependencies.dedup();
}

fn compare_dependencies(left: &EvaluationDependency, right: &EvaluationDependency) -> Ordering {
    left.edge_id()
        .cmp(&right.edge_id())
        .then_with(|| {
            left.frame()
                .partial_cmp(&right.frame())
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| region_key(left.region()).cmp(&region_key(right.region())))
        .then_with(|| time_representation(left.frame()).cmp(&time_representation(right.frame())))
}

const fn region_key(region: PixelBounds) -> (i32, i32, i32, i32) {
    (
        region.min_x(),
        region.min_y(),
        region.max_x(),
        region.max_y(),
    )
}

const fn time_representation(time: RationalTime) -> (u32, u32, i64) {
    (
        time.timebase().numerator(),
        time.timebase().denominator(),
        time.value(),
    )
}

fn dependency_error<N>(
    graph: &DirectedAcyclicGraph<N>,
    key: EvaluationKey,
    dependency: EvaluationDependency,
    reason: &'static str,
    message: &'static str,
) -> Error {
    let mut context = request_context(graph, key, "resolve_dependency");
    context.insert_field("edge_id", dependency.edge_id().to_string());
    context.insert_field("reason", reason);
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message).with_context(context)
}

fn request_error<N>(
    graph: &DirectedAcyclicGraph<N>,
    key: EvaluationKey,
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
    reason: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(request_context(graph, key, operation).with_field("reason", reason))
}

fn request_context<N>(
    graph: &DirectedAcyclicGraph<N>,
    key: EvaluationKey,
    operation: &'static str,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
        .with_field("graph_id", graph.id().to_string())
        .with_field("node_id", key.output().node_id().to_string())
        .with_field("port_id", key.output().port_id().to_string())
        .with_field("frame", key.frame().to_string())
        .with_field("region", format_region(key.region()))
}

fn format_region(region: PixelBounds) -> String {
    format!(
        "[{},{},{},{})",
        region.min_x(),
        region.min_y(),
        region.max_x(),
        region.max_y()
    )
}
