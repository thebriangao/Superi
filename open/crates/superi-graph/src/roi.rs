//! Deterministic region-of-interest propagation over immutable editable graph snapshots.
//!
//! Requested output work travels toward graph sources through each node's declared ROI behavior.
//! The result contains only connected dependencies, retains exact typed endpoint identity, reuses
//! invalidation's exact region-set algebra, and records the graph revision from which it was
//! derived. This module plans work only. It owns no project mutation, cache state, scheduling, or
//! payload execution.

use std::collections::{BTreeMap, BTreeSet};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;

use crate::dag::GraphEndpoint;
use crate::ids::{GraphId, NodeId, PortId};
use crate::invalidation::{DirtyRegion, DirtyRegionSet, InvalidationPlan};
use crate::mutate::{EditableNode, GraphSnapshot};
use crate::node::RoiBehavior;

const COMPONENT: &str = "superi-graph.roi";

/// Regions of definition for graph output endpoints in one evaluation context.
///
/// Domains are supplied separately from editable graph state because they can depend on frame,
/// source media, and node parameters. Every output reached by propagation must have one exact
/// finite domain. Duplicate endpoint declarations are rejected instead of silently replacing one
/// meaning with another.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RoiDomains {
    outputs: BTreeMap<GraphEndpoint, PixelBounds>,
}

impl RoiDomains {
    /// Creates a deterministic domain map from exact output endpoints.
    pub fn new(outputs: impl IntoIterator<Item = (GraphEndpoint, PixelBounds)>) -> Result<Self> {
        let mut values = BTreeMap::new();
        for (endpoint, bounds) in outputs {
            if values.insert(endpoint, bounds).is_some() {
                return Err(roi_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "create_domains",
                    "region of definition is declared more than once for one endpoint",
                )
                .with_context(endpoint_context(endpoint)));
            }
        }
        Ok(Self { outputs: values })
    }

    /// Returns the finite region of definition for one output endpoint.
    #[must_use]
    pub fn get(&self, endpoint: GraphEndpoint) -> Option<PixelBounds> {
        self.outputs.get(&endpoint).copied()
    }

    /// Returns every declared output domain in stable endpoint order.
    #[must_use]
    pub const fn outputs(&self) -> &BTreeMap<GraphEndpoint, PixelBounds> {
        &self.outputs
    }
}

/// One requested output endpoint and its exact output work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoiRequest {
    endpoint: GraphEndpoint,
    regions: DirtyRegionSet,
}

impl RoiRequest {
    /// Creates a request from one endpoint and an exact normalized region set.
    #[must_use]
    pub const fn new(endpoint: GraphEndpoint, regions: DirtyRegionSet) -> Self {
        Self { endpoint, regions }
    }

    /// Creates a request from one finite half-open pixel region.
    #[must_use]
    pub fn from_bounds(endpoint: GraphEndpoint, bounds: PixelBounds) -> Self {
        Self::new(
            endpoint,
            DirtyRegionSet::from_region(DirtyRegion::Bounds(bounds)),
        )
    }

    /// Returns the requested graph output endpoint.
    #[must_use]
    pub const fn endpoint(&self) -> GraphEndpoint {
        self.endpoint
    }

    /// Returns the exact requested output work.
    #[must_use]
    pub const fn regions(&self) -> &DirtyRegionSet {
        &self.regions
    }
}

/// Stable context presented to a custom node ROI implementation.
pub struct CustomRoiRequest<'a, T> {
    node_id: NodeId,
    node: &'a EditableNode<T>,
    requested_outputs: &'a BTreeMap<PortId, DirtyRegionSet>,
}

impl<'a, T> CustomRoiRequest<'a, T> {
    /// Returns the exact node instance being mapped.
    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Returns the immutable schema-bound node state from the evaluated snapshot.
    #[must_use]
    pub const fn node(&self) -> &'a EditableNode<T> {
        self.node
    }

    /// Returns requested output work by exact output port identity.
    #[must_use]
    pub const fn requested_outputs(&self) -> &'a BTreeMap<PortId, DirtyRegionSet> {
        self.requested_outputs
    }
}

/// Node implementation seam for [`RoiBehavior::Custom`].
///
/// Returned keys must identify input ports on the provided node. Missing inputs request no work.
/// The mapper must be deterministic for equal snapshot state and requested outputs.
pub trait CustomRoiMapper<T> {
    /// Maps requested node outputs to exact input-port work.
    fn map_inputs(
        &mut self,
        request: CustomRoiRequest<'_, T>,
    ) -> Result<BTreeMap<PortId, DirtyRegionSet>>;
}

impl<T, F> CustomRoiMapper<T> for F
where
    F: for<'a> FnMut(CustomRoiRequest<'a, T>) -> Result<BTreeMap<PortId, DirtyRegionSet>>,
{
    fn map_inputs(
        &mut self,
        request: CustomRoiRequest<'_, T>,
    ) -> Result<BTreeMap<PortId, DirtyRegionSet>> {
        self(request)
    }
}

/// Required graph work derived from one immutable editable snapshot.
///
/// Output and input maps contain only connected, nonempty work. Evaluation order contains every
/// required node exactly once in dependency-first topological order. The plan is derived state and
/// does not authorize mutation or cache reuse across another graph revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoiPlan {
    graph_id: GraphId,
    graph_revision: u64,
    evaluation_order: Vec<NodeId>,
    output_regions: BTreeMap<GraphEndpoint, DirtyRegionSet>,
    input_regions: BTreeMap<GraphEndpoint, DirtyRegionSet>,
}

impl RoiPlan {
    /// Returns the graph identity from the evaluated snapshot.
    #[must_use]
    pub const fn graph_id(&self) -> GraphId {
        self.graph_id
    }

    /// Returns the immutable editable graph revision used to derive this plan.
    #[must_use]
    pub const fn graph_revision(&self) -> u64 {
        self.graph_revision
    }

    /// Returns required nodes in stable dependency-first order.
    #[must_use]
    pub fn evaluation_order(&self) -> &[NodeId] {
        &self.evaluation_order
    }

    /// Returns true when propagation resolved no graph work.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.evaluation_order.is_empty()
    }

    /// Returns exact required work for one output endpoint.
    #[must_use]
    pub fn output_regions(&self, endpoint: GraphEndpoint) -> Option<&DirtyRegionSet> {
        self.output_regions.get(&endpoint)
    }

    /// Returns exact required work for one connected input endpoint.
    #[must_use]
    pub fn input_regions(&self, endpoint: GraphEndpoint) -> Option<&DirtyRegionSet> {
        self.input_regions.get(&endpoint)
    }

    /// Returns every required output in stable endpoint order.
    #[must_use]
    pub const fn outputs(&self) -> &BTreeMap<GraphEndpoint, DirtyRegionSet> {
        &self.output_regions
    }

    /// Returns every required connected input in stable endpoint order.
    #[must_use]
    pub const fn inputs(&self) -> &BTreeMap<GraphEndpoint, DirtyRegionSet> {
        &self.input_regions
    }

    /// Intersects required output work with an existing node invalidation plan.
    ///
    /// Clean nodes and empty intersections are absent. Exact dirty-region normalization preserves
    /// clean gaps rather than replacing the result with a bounding rectangle.
    #[must_use]
    pub fn invalidated_output_work(
        &self,
        invalidation: &InvalidationPlan,
    ) -> BTreeMap<GraphEndpoint, DirtyRegionSet> {
        self.output_regions
            .iter()
            .filter_map(|(endpoint, requested)| {
                let dirty = invalidation.dirty_regions(endpoint.node_id())?;
                let work = intersect_regions(requested, dirty);
                (!work.is_empty()).then_some((*endpoint, work))
            })
            .collect()
    }
}

/// Propagates built-in ROI behavior through one immutable editable graph snapshot.
///
/// A reached [`RoiBehavior::Custom`] node fails with an unsupported error. Use
/// [`propagate_roi_with`] when the graph can contain custom node behavior.
pub fn propagate_roi<T, I>(
    snapshot: &GraphSnapshot<T>,
    domains: &RoiDomains,
    requests: I,
) -> Result<RoiPlan>
where
    I: IntoIterator<Item = RoiRequest>,
{
    propagate_roi_internal(snapshot, domains, requests, None)
}

/// Propagates built-in and custom ROI behavior through one immutable editable snapshot.
///
/// The custom mapper is invoked only for reached custom nodes with connected inputs. Nodes and
/// edges are visited in stable graph order, and equal deterministic inputs produce equal plans for
/// editor, script, playback, and headless callers.
pub fn propagate_roi_with<T, I, M>(
    snapshot: &GraphSnapshot<T>,
    domains: &RoiDomains,
    requests: I,
    mut custom_mapper: M,
) -> Result<RoiPlan>
where
    I: IntoIterator<Item = RoiRequest>,
    M: CustomRoiMapper<T>,
{
    propagate_roi_internal(snapshot, domains, requests, Some(&mut custom_mapper))
}

fn propagate_roi_internal<T, I>(
    snapshot: &GraphSnapshot<T>,
    domains: &RoiDomains,
    requests: I,
    mut custom_mapper: Option<&mut dyn CustomRoiMapper<T>>,
) -> Result<RoiPlan>
where
    I: IntoIterator<Item = RoiRequest>,
{
    let dag = snapshot.dag();
    let requests = requests.into_iter().collect::<Vec<_>>();
    for request in &requests {
        validate_output_endpoint(snapshot, request.endpoint, "validate_request")?;
    }

    let mut output_regions = BTreeMap::new();
    for request in requests {
        let domain = required_domain(domains, snapshot.graph_id(), request.endpoint)?;
        let clipped = request.regions.clip_to(domain);
        merge_regions(&mut output_regions, request.endpoint, &clipped);
    }

    let mut input_regions = BTreeMap::new();
    let topological_order = dag.topological_order();
    for node_id in topological_order.iter().rev().copied() {
        let requested_outputs = output_regions
            .iter()
            .filter(|(endpoint, _)| endpoint.node_id() == node_id)
            .map(|(endpoint, regions)| (endpoint.port_id(), regions.clone()))
            .collect::<BTreeMap<_, _>>();
        if requested_outputs.is_empty() {
            continue;
        }

        let incoming_edges = dag
            .incoming_edge_ids(node_id)
            .expect("topological nodes retain incoming adjacency")
            .iter()
            .map(|edge_id| {
                *dag.edge(*edge_id)
                    .expect("incoming adjacency retains stored edge identity")
            })
            .collect::<Vec<_>>();
        if incoming_edges.is_empty() {
            continue;
        }

        let node = snapshot
            .node(node_id)
            .expect("topological node remains present in graph snapshot");
        let mapped_inputs = map_node_inputs(
            snapshot.graph_id(),
            node_id,
            node,
            &requested_outputs,
            &mut custom_mapper,
        )?;

        for graph_edge in incoming_edges {
            let Some(mapped) = mapped_inputs.get(&graph_edge.destination().port_id()) else {
                continue;
            };
            let source = graph_edge.source();
            let domain = required_domain(domains, snapshot.graph_id(), source)?;
            let clipped = mapped.clip_to(domain);
            if clipped.is_empty() {
                continue;
            }
            merge_regions(&mut input_regions, graph_edge.destination(), &clipped);
            merge_regions(&mut output_regions, source, &clipped);
        }
    }

    let required_nodes = output_regions
        .keys()
        .map(|endpoint| endpoint.node_id())
        .collect::<BTreeSet<_>>();
    let evaluation_order = topological_order
        .into_iter()
        .filter(|node_id| required_nodes.contains(node_id))
        .collect();

    Ok(RoiPlan {
        graph_id: snapshot.graph_id(),
        graph_revision: snapshot.revision(),
        evaluation_order,
        output_regions,
        input_regions,
    })
}

fn map_node_inputs<T>(
    graph_id: GraphId,
    node_id: NodeId,
    node: &EditableNode<T>,
    requested_outputs: &BTreeMap<PortId, DirtyRegionSet>,
    custom_mapper: &mut Option<&mut dyn CustomRoiMapper<T>>,
) -> Result<BTreeMap<PortId, DirtyRegionSet>> {
    let aggregate =
        requested_outputs
            .values()
            .fold(DirtyRegionSet::empty(), |mut combined, regions| {
                combined.union_with(regions);
                combined
            });

    let mapped = match node.schema().behavior().roi() {
        RoiBehavior::FullFrame => node
            .inputs()
            .keys()
            .copied()
            .map(|port_id| (port_id, DirtyRegionSet::full_frame()))
            .collect(),
        RoiBehavior::InputBounds => node
            .inputs()
            .keys()
            .copied()
            .map(|port_id| (port_id, aggregate.clone()))
            .collect(),
        RoiBehavior::Expanded { pixels } => {
            let expanded = expand_regions(&aggregate, pixels, graph_id, node_id)?;
            node.inputs()
                .keys()
                .copied()
                .map(|port_id| (port_id, expanded.clone()))
                .collect()
        }
        RoiBehavior::Custom => {
            let mapper = custom_mapper.as_mut().ok_or_else(|| {
                roi_error(
                    ErrorCategory::Unsupported,
                    Recoverability::Degraded,
                    "map_custom_inputs",
                    "custom ROI behavior requires a node implementation mapper",
                )
                .with_context(graph_node_context(graph_id, node_id))
            })?;
            (*mapper)
                .map_inputs(CustomRoiRequest {
                    node_id,
                    node,
                    requested_outputs,
                })
                .map_err(|mut error| {
                    error.push_context(
                        ErrorContext::new(COMPONENT, "map_custom_inputs")
                            .with_field("graph_id", graph_id.to_string())
                            .with_field("node_id", node_id.to_string()),
                    );
                    error
                })?
        }
    };

    for port_id in mapped.keys().copied() {
        if node.input_name(port_id).is_none() {
            return Err(roi_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "validate_custom_inputs",
                "ROI mapping returned a port that is not an input on this node",
            )
            .with_context(
                graph_node_context(graph_id, node_id).with_field("port_id", port_id.to_string()),
            ));
        }
    }
    Ok(mapped)
}

fn expand_regions(
    regions: &DirtyRegionSet,
    pixels: u32,
    graph_id: GraphId,
    node_id: NodeId,
) -> Result<DirtyRegionSet> {
    if regions.is_empty() || pixels == 0 {
        return Ok(regions.clone());
    }
    if regions.is_full_frame() {
        return Ok(DirtyRegionSet::full_frame());
    }
    let pixels = i32::try_from(pixels).map_err(|_| {
        expansion_error(
            graph_id,
            node_id,
            "ROI expansion exceeds the supported coordinate range",
        )
    })?;
    let mut expanded = DirtyRegionSet::empty();
    for region in regions.regions() {
        let min_x = region.min_x().checked_sub(pixels);
        let min_y = region.min_y().checked_sub(pixels);
        let max_x = region.max_x().checked_add(pixels);
        let max_y = region.max_y().checked_add(pixels);
        let (Some(min_x), Some(min_y), Some(max_x), Some(max_y)) = (min_x, min_y, max_x, max_y)
        else {
            return Err(expansion_error(
                graph_id,
                node_id,
                "ROI expansion exceeds the supported coordinate range",
            ));
        };
        expanded.insert(DirtyRegion::Bounds(
            PixelBounds::new(min_x, min_y, max_x, max_y).expect("checked expansion stays ordered"),
        ));
    }
    Ok(expanded)
}

fn validate_output_endpoint<T>(
    snapshot: &GraphSnapshot<T>,
    endpoint: GraphEndpoint,
    operation: &'static str,
) -> Result<()> {
    let Some(node) = snapshot.node(endpoint.node_id()) else {
        return Err(roi_error(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            operation,
            "ROI request node does not exist in the graph snapshot",
        )
        .with_context(
            graph_node_context(snapshot.graph_id(), endpoint.node_id())
                .with_field("port_id", endpoint.port_id().to_string()),
        ));
    };
    if node.output_name(endpoint.port_id()).is_some() {
        return Ok(());
    }
    let category = if node.input_name(endpoint.port_id()).is_some() {
        ErrorCategory::InvalidInput
    } else {
        ErrorCategory::NotFound
    };
    Err(roi_error(
        category,
        Recoverability::UserCorrectable,
        operation,
        "ROI request endpoint is not an output port on this node",
    )
    .with_context(
        graph_node_context(snapshot.graph_id(), endpoint.node_id())
            .with_field("port_id", endpoint.port_id().to_string()),
    ))
}

fn required_domain(
    domains: &RoiDomains,
    graph_id: GraphId,
    endpoint: GraphEndpoint,
) -> Result<PixelBounds> {
    domains.get(endpoint).ok_or_else(|| {
        roi_error(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "resolve_domain",
            "required graph output has no region of definition",
        )
        .with_context(
            graph_node_context(graph_id, endpoint.node_id())
                .with_field("port_id", endpoint.port_id().to_string()),
        )
    })
}

fn merge_regions(
    values: &mut BTreeMap<GraphEndpoint, DirtyRegionSet>,
    endpoint: GraphEndpoint,
    regions: &DirtyRegionSet,
) {
    if regions.is_empty() {
        return;
    }
    values.entry(endpoint).or_default().union_with(regions);
}

fn intersect_regions(left: &DirtyRegionSet, right: &DirtyRegionSet) -> DirtyRegionSet {
    if left.is_empty() || right.is_empty() {
        return DirtyRegionSet::empty();
    }
    if left.is_full_frame() {
        return right.clone();
    }
    if right.is_full_frame() {
        return left.clone();
    }
    let mut intersection = DirtyRegionSet::empty();
    for region in left.regions() {
        intersection.union_with(&right.clip_to(*region));
    }
    intersection
}

fn expansion_error(graph_id: GraphId, node_id: NodeId, message: &'static str) -> Error {
    roi_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "expand_regions",
        message,
    )
    .with_context(graph_node_context(graph_id, node_id))
}

fn roi_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn graph_node_context(graph_id: GraphId, node_id: NodeId) -> ErrorContext {
    ErrorContext::new(COMPONENT, "propagate_roi")
        .with_field("graph_id", graph_id.to_string())
        .with_field("node_id", node_id.to_string())
}

fn endpoint_context(endpoint: GraphEndpoint) -> ErrorContext {
    ErrorContext::new(COMPONENT, "endpoint")
        .with_field("node_id", endpoint.node_id().to_string())
        .with_field("port_id", endpoint.port_id().to_string())
}
