//! Shared interactive and headless evaluation over immutable editable graph state.
//!
//! Editable graph nodes contain ordinary serializable state, while executable node catalogs live
//! in higher dependency tiers. This module binds those layers without introducing a second graph
//! or evaluator: a caller-owned compiler translates every node from one [`GraphSnapshot`], and the
//! resulting immutable evaluation snapshot delegates scheduling and execution to [`LazyEvaluator`].
//! Editor, script, playback, and headless callers therefore observe the same authored revision and
//! use the same request-scoped evaluation path.

use superi_core::error::{ErrorContext, Result, ResultExt};

use crate::dag::DirectedAcyclicGraph;
use crate::diagnostics::{EvaluationInspection, EvaluationReport, IntrospectNode};
use crate::eval::{
    EvaluateNode, EvaluationRequest, EvaluationResult, EvaluationSchedule, EvaluationValueCache,
    LazyEvaluator,
};
use crate::ids::{GraphId, NodeId};
use crate::mutate::{EditableNode, GraphSnapshot};

const COMPONENT: &str = "superi-graph.headless";

/// Compiles one complete editable node into its caller-owned runtime implementation.
///
/// Catalogs above `superi-graph` implement this seam so the node-neutral graph crate never depends
/// on effects, color, media, cache, or engine types. The compiler receives the complete immutable
/// graph snapshot beside the schema-bound node, so snapshot-owned parameter drivers and other
/// evaluation-affecting authored state cannot be hidden from runtime translation. Equal editable
/// state must compile to equivalent runtime behavior when deterministic interactive and headless
/// parity is required.
pub trait NodeCompiler<T, N> {
    /// Compiles one node from the exact graph snapshot being prepared for evaluation.
    fn compile_node(
        &mut self,
        snapshot: &GraphSnapshot<T>,
        node_id: NodeId,
        node: &EditableNode<T>,
    ) -> Result<N>;
}

impl<T, N, F> NodeCompiler<T, N> for F
where
    F: FnMut(&GraphSnapshot<T>, NodeId, &EditableNode<T>) -> Result<N>,
{
    fn compile_node(
        &mut self,
        snapshot: &GraphSnapshot<T>,
        node_id: NodeId,
        node: &EditableNode<T>,
    ) -> Result<N> {
        self(snapshot, node_id, node)
    }
}

/// One immutable editable graph revision and its executable node projection.
///
/// Compilation replaces only node payloads. Graph identity, node identity, exact edge routes, and
/// deterministic topology come directly from the checked editable snapshot. The source snapshot is
/// retained for inspection, ROI, invalidation, and revision checks, while every schedule and result
/// uses the same stateless [`LazyEvaluator`] as any direct interactive pull.
///
/// A later edit requires compiling a new evaluation snapshot. Existing values remain bound to the
/// old immutable revision, and evaluation retains no cross-request cache that could reuse stale
/// work after the edit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphEvaluationSnapshot<T, N> {
    editable: GraphSnapshot<T>,
    executable: DirectedAcyclicGraph<N>,
}

impl<T, N> GraphEvaluationSnapshot<T, N> {
    /// Compiles every editable node and preserves the snapshot's exact checked topology.
    ///
    /// Nodes and edges are visited in stable identity order. A compiler failure aborts the complete
    /// local projection, preserves its classification, and gains the exact graph revision, node,
    /// and schema context. No partially compiled graph is published.
    pub fn compile<C>(editable: GraphSnapshot<T>, mut compiler: C) -> Result<Self>
    where
        C: NodeCompiler<T, N>,
    {
        let mut executable = DirectedAcyclicGraph::new(editable.graph_id());
        for (node_id, node) in editable.dag().nodes() {
            let compiled = compiler
                .compile_node(&editable, *node_id, node)
                .with_error_context(compile_context(&editable, *node_id, node))?;
            executable
                .insert_node(*node_id, compiled)
                .expect("editable node identities are unique in an empty runtime projection");
        }
        for edge in editable.dag().edges().values() {
            executable
                .insert_edge(*edge)
                .expect("checked editable topology remains valid in the runtime projection");
        }

        Ok(Self {
            editable,
            executable,
        })
    }

    /// Returns the exact immutable editable state used for runtime compilation.
    #[must_use]
    pub const fn editable_snapshot(&self) -> &GraphSnapshot<T> {
        &self.editable
    }

    /// Returns the stable graph identity shared by editable and executable state.
    #[must_use]
    pub fn graph_id(&self) -> GraphId {
        self.editable.graph_id()
    }

    /// Returns the editable revision from which runtime nodes were compiled.
    #[must_use]
    pub const fn graph_revision(&self) -> u64 {
        self.editable.revision()
    }

    /// Builds the shared deterministic request-local schedule without evaluating values.
    pub fn schedule<V>(&self, request: EvaluationRequest) -> Result<EvaluationSchedule>
    where
        N: EvaluateNode<V>,
    {
        LazyEvaluator::schedule(&self.executable, request)
    }

    /// Evaluates one request through the shared stateless lazy evaluator.
    ///
    /// Each call starts with an empty request-local value set. Exact frame and region work supplied
    /// by ROI or invalidation callers passes through unchanged, and only node-declared stored
    /// dependencies are reached.
    pub fn evaluate<V>(&self, request: EvaluationRequest) -> Result<EvaluationResult<V>>
    where
        N: EvaluateNode<V>,
    {
        LazyEvaluator::evaluate(&self.executable, request)
    }

    /// Evaluates one request through the shared path with retained final and intermediate values.
    ///
    /// The concrete cache remains caller-owned. Cache identity is derived from this snapshot's
    /// immutable executable projection, so every role observes the same authored revision and
    /// semantic result whether a value is retained or freshly evaluated.
    pub fn evaluate_with_cache<V, C>(
        &self,
        request: EvaluationRequest,
        cache: &C,
    ) -> Result<EvaluationResult<V>>
    where
        N: EvaluateNode<V> + IntrospectNode,
        V: Clone,
        C: EvaluationValueCache<V>,
    {
        LazyEvaluator::evaluate_with_cache(&self.executable, request, cache)
    }

    /// Inspects deterministic node identity and cache decisions through the shared evaluator.
    pub fn inspect<V>(&self, request: EvaluationRequest) -> Result<EvaluationInspection>
    where
        N: EvaluateNode<V> + IntrospectNode,
    {
        LazyEvaluator::inspect(&self.executable, request)
    }

    /// Evaluates through the shared path and returns its unchanged result with run-local timing.
    pub fn evaluate_with_diagnostics<V>(
        &self,
        request: EvaluationRequest,
    ) -> Result<EvaluationReport<V>>
    where
        N: EvaluateNode<V> + IntrospectNode,
    {
        LazyEvaluator::evaluate_with_diagnostics(&self.executable, request)
    }
}

fn compile_context<T>(
    snapshot: &GraphSnapshot<T>,
    node_id: NodeId,
    node: &EditableNode<T>,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, "compile_node")
        .with_field("graph_id", snapshot.graph_id().to_string())
        .with_field("graph_revision", snapshot.revision().to_string())
        .with_field("node_id", node_id.to_string())
        .with_field("schema_id", node.schema().id().to_string())
}
