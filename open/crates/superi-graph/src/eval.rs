//! Lazy, per-frame, per-region graph evaluation.
//!
//! Evaluation is a request-scoped pull through the stored DAG. Node payloads declare the incoming
//! edge, frame, and region work needed for one output, then receive only those resolved values.
//! Identical request keys execute once within the pull. The evaluator retains no cross-request
//! state, scheduler, dirty-region policy, or domain catalog knowledge.

use std::cmp::Ordering;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::geometry::PixelBounds;
use superi_core::time::RationalTime;

use crate::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use crate::ids::EdgeId;

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

        let mut resolver = Resolver::new(graph);
        let target_index = resolver.resolve(request.key())?;
        Ok(EvaluationResult {
            request,
            target_index,
            evaluated: resolver.evaluated,
        })
    }
}

struct Resolver<'a, N, V> {
    graph: &'a DirectedAcyclicGraph<N>,
    evaluated: Vec<EvaluatedValue<V>>,
    active: Vec<EvaluationKey>,
}

impl<'a, N, V> Resolver<'a, N, V>
where
    N: EvaluateNode<V>,
{
    const fn new(graph: &'a DirectedAcyclicGraph<N>) -> Self {
        Self {
            graph,
            evaluated: Vec::new(),
            active: Vec::new(),
        }
    }

    fn resolve(&mut self, key: EvaluationKey) -> Result<usize> {
        if let Some(index) = self.evaluated.iter().position(|entry| entry.key == key) {
            return Ok(index);
        }
        if self.active.contains(&key) {
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

        self.active.push(key);
        let result = self.resolve_uncached(key);
        self.active.pop();
        result
    }

    fn resolve_uncached(&mut self, key: EvaluationKey) -> Result<usize> {
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
            resolved.push((
                dependency,
                edge,
                EvaluationKey::new(edge.source(), dependency.frame(), dependency.region()),
            ));
        }

        let mut input_indices = Vec::with_capacity(resolved.len());
        for (_, _, source) in &resolved {
            input_indices.push(self.resolve(*source)?);
        }

        let value = {
            let inputs = resolved
                .iter()
                .zip(&input_indices)
                .map(|((dependency, edge, source), index)| EvaluationInput {
                    dependency: *dependency,
                    edge: *edge,
                    source: *source,
                    value: &self.evaluated[*index].value,
                })
                .collect::<Vec<_>>();
            let context = EvaluationContext {
                request,
                inputs: &inputs,
            };
            self.graph
                .node(node_id)
                .expect("evaluation node existence checked")
                .evaluate(&context)
                .with_error_context(request_context(self.graph, key, "evaluate_node"))?
        };

        let index = self.evaluated.len();
        self.evaluated.push(EvaluatedValue { key, value });
        Ok(index)
    }
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
