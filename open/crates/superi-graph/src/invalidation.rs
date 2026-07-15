//! Exact dirty-region sets and deterministic dependency invalidation plans.
//!
//! Invalidation is derived from an immutable DAG snapshot. This module owns region-set algebra and
//! dependency traversal, while node-specific region mapping remains an evaluator concern supplied
//! at the edge boundary.

use std::collections::{BTreeMap, BTreeSet};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;

use crate::dag::{DirectedAcyclicGraph, GraphEdge};
use crate::expr::ParameterAddress;
use crate::ids::{GraphId, NodeId};
use crate::mutate::{EditableNode, GraphSnapshot};

const COMPONENT: &str = "superi-graph.invalidation";

/// One dirty output extent.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DirtyRegion {
    /// Every pixel in the output is dirty.
    FullFrame,
    /// Only the exact half-open pixel bounds are dirty.
    Bounds(PixelBounds),
}

/// An exact normalized union of dirty output regions.
///
/// Finite regions are stored as deterministic, nonoverlapping rectangles. Normalization never
/// replaces an irregular union with its bounding box, so clean gaps remain clean work.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DirtyRegionSet {
    full_frame: bool,
    regions: Vec<PixelBounds>,
}

impl DirtyRegionSet {
    /// Creates an empty set with no dirty work.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            full_frame: false,
            regions: Vec::new(),
        }
    }

    /// Creates a set from one dirty extent.
    #[must_use]
    pub fn from_region(region: DirtyRegion) -> Self {
        let mut regions = Self::empty();
        regions.insert(region);
        regions
    }

    /// Creates a full-frame dirty set.
    #[must_use]
    pub const fn full_frame() -> Self {
        Self {
            full_frame: true,
            regions: Vec::new(),
        }
    }

    /// Returns true when the complete output is dirty.
    #[must_use]
    pub const fn is_full_frame(&self) -> bool {
        self.full_frame
    }

    /// Returns true when no output work is dirty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.full_frame && self.regions.is_empty()
    }

    /// Returns finite dirty rectangles in canonical coordinate order.
    ///
    /// A full-frame set returns an empty slice because no finite bounds can represent an unknown
    /// frame extent. Callers must inspect [`Self::is_full_frame`] first.
    #[must_use]
    pub fn regions(&self) -> &[PixelBounds] {
        &self.regions
    }

    /// Adds one dirty extent while preserving exact normalized coverage.
    pub fn insert(&mut self, region: DirtyRegion) {
        match region {
            DirtyRegion::FullFrame => {
                self.full_frame = true;
                self.regions.clear();
            }
            DirtyRegion::Bounds(bounds) if !self.full_frame && !bounds.is_empty() => {
                self.regions.push(bounds);
                self.regions = normalize_regions(std::mem::take(&mut self.regions));
            }
            DirtyRegion::Bounds(_) => {}
        }
    }

    /// Merges another set into this set without over-invalidating clean pixels.
    pub fn union_with(&mut self, other: &Self) {
        if self.full_frame || other.is_empty() {
            return;
        }
        if other.full_frame {
            self.full_frame = true;
            self.regions.clear();
            return;
        }

        self.regions.extend_from_slice(&other.regions);
        self.regions = normalize_regions(std::mem::take(&mut self.regions));
    }

    /// Returns only dirty work intersecting one requested output region.
    #[must_use]
    pub fn clip_to(&self, requested: PixelBounds) -> Self {
        if requested.is_empty() || self.is_empty() {
            return Self::empty();
        }
        if self.full_frame {
            return Self::from_region(DirtyRegion::Bounds(requested));
        }

        let clipped = self
            .regions
            .iter()
            .filter_map(|region| region.intersection(requested))
            .collect::<Vec<_>>();
        Self {
            full_frame: false,
            regions: normalize_regions(clipped),
        }
    }
}

/// One authored invalidation root on an immutable graph snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidationSeed {
    node_id: NodeId,
    dirty_regions: DirtyRegionSet,
}

impl InvalidationSeed {
    /// Creates a seed from one node and its complete dirty-region set.
    #[must_use]
    pub const fn new(node_id: NodeId, dirty_regions: DirtyRegionSet) -> Self {
        Self {
            node_id,
            dirty_regions,
        }
    }

    /// Creates a seed from one node and one dirty extent.
    #[must_use]
    pub fn from_region(node_id: NodeId, dirty_region: DirtyRegion) -> Self {
        Self::new(node_id, DirtyRegionSet::from_region(dirty_region))
    }

    /// Returns the graph node whose output changed.
    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Returns the exact changed output regions.
    #[must_use]
    pub const fn dirty_regions(&self) -> &DirtyRegionSet {
        &self.dirty_regions
    }
}

/// One affected node in stable dependency order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidatedNode {
    node_id: NodeId,
    dirty_regions: DirtyRegionSet,
}

impl InvalidatedNode {
    /// Returns the affected graph node.
    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Returns the exact dirty output regions for this node.
    #[must_use]
    pub const fn dirty_regions(&self) -> &DirtyRegionSet {
        &self.dirty_regions
    }
}

/// Derived invalidation work for one immutable graph snapshot.
///
/// Entries include only dirty nodes and appear once in stable topological order. The plan contains
/// no mutable project, cache, or scheduler state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InvalidationPlan {
    nodes: Vec<InvalidatedNode>,
}

impl InvalidationPlan {
    /// Returns affected nodes in stable topological order.
    #[must_use]
    pub fn nodes(&self) -> &[InvalidatedNode] {
        &self.nodes
    }

    /// Returns true when the plan contains no dirty work.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns all dirty output regions for one affected node.
    #[must_use]
    pub fn dirty_regions(&self, node_id: NodeId) -> Option<&DirtyRegionSet> {
        self.nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .map(InvalidatedNode::dirty_regions)
    }

    /// Returns only the dirty work intersecting one requested output region.
    #[must_use]
    pub fn requested_work(&self, node_id: NodeId, requested: PixelBounds) -> DirtyRegionSet {
        self.dirty_regions(node_id)
            .map_or_else(DirtyRegionSet::empty, |dirty| dirty.clip_to(requested))
    }
}

/// Deterministic cache lineage affected by one published graph edit interval.
///
/// Roots identify direct semantic changes. Affected nodes include those roots plus every result
/// descendant reached in either the prior or current topology. Node identities are sorted so a
/// coalesced edit interval remains inspectable even when the two revisions have different
/// topological orders.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphEditInvalidation {
    graph_id: GraphId,
    from_revision: u64,
    to_revision: u64,
    roots: Vec<NodeId>,
    affected_nodes: Vec<NodeId>,
}

impl GraphEditInvalidation {
    /// Returns the graph whose retained values are affected.
    #[must_use]
    pub const fn graph_id(&self) -> GraphId {
        self.graph_id
    }

    /// Returns the oldest compared editable revision.
    #[must_use]
    pub const fn from_revision(&self) -> u64 {
        self.from_revision
    }

    /// Returns the newly published editable revision.
    #[must_use]
    pub const fn to_revision(&self) -> u64 {
        self.to_revision
    }

    /// Returns direct semantic edit roots in stable node identity order.
    #[must_use]
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// Returns all affected cache-lineage nodes in stable identity order.
    #[must_use]
    pub fn affected_nodes(&self) -> &[NodeId] {
        &self.affected_nodes
    }

    /// Returns whether one node has affected retained output.
    #[must_use]
    pub fn affects(&self, node_id: NodeId) -> bool {
        self.affected_nodes.binary_search(&node_id).is_ok()
    }

    /// Returns whether the revisions differ only in presentation or not at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.affected_nodes.is_empty()
    }
}

/// Derives precise cache invalidation from two immutable editable graph revisions.
///
/// Equal snapshots are valid and produce no work. A later revision may skip intermediate
/// publications, allowing an orchestration owner to coalesce rapid edits into one semantic diff.
/// Presentation order is excluded because it does not affect processing meaning. Parameter edits
/// expand through both revisions' driver dependency graphs, and node roots propagate through both
/// pixel-flow topologies so connection removal and node removal cannot hide old descendants.
pub fn derive_edit_invalidation<T: PartialEq>(
    before: &GraphSnapshot<T>,
    after: &GraphSnapshot<T>,
) -> Result<GraphEditInvalidation> {
    if before.graph_id() != after.graph_id() {
        return Err(edit_pair_error(
            before,
            after,
            ErrorCategory::InvalidInput,
            "graph_identity_mismatch",
            "graph edit invalidation requires one stable graph identity",
        ));
    }
    if after.revision() < before.revision() {
        return Err(edit_pair_error(
            before,
            after,
            ErrorCategory::Conflict,
            "revision_reversed",
            "graph edit invalidation revisions are reversed",
        ));
    }
    if after.revision() == before.revision() {
        if before == after {
            return Ok(GraphEditInvalidation {
                graph_id: before.graph_id(),
                from_revision: before.revision(),
                to_revision: after.revision(),
                roots: Vec::new(),
                affected_nodes: Vec::new(),
            });
        }
        return Err(edit_pair_error(
            before,
            after,
            ErrorCategory::Conflict,
            "revision_not_advanced",
            "different graph states cannot share one editable revision",
        ));
    }

    let mut roots = BTreeSet::new();
    let mut changed_parameters = BTreeSet::new();
    let node_ids = before
        .dag()
        .nodes()
        .keys()
        .chain(after.dag().nodes().keys())
        .copied()
        .collect::<BTreeSet<_>>();

    for node_id in node_ids {
        match (before.node(node_id), after.node(node_id)) {
            (Some(previous), Some(current)) => {
                collect_changed_node_state(
                    node_id,
                    previous,
                    current,
                    &mut roots,
                    &mut changed_parameters,
                );
            }
            (Some(node), None) | (None, Some(node)) => {
                roots.insert(node_id);
                changed_parameters.extend(
                    node.parameters()
                        .keys()
                        .map(|parameter_id| ParameterAddress::new(node_id, *parameter_id)),
                );
            }
            (None, None) => unreachable!("node identity came from one compared graph"),
        }
    }

    let edge_ids = before
        .dag()
        .edges()
        .keys()
        .chain(after.dag().edges().keys())
        .copied()
        .collect::<BTreeSet<_>>();
    for edge_id in edge_ids {
        let previous = before.dag().edge(edge_id);
        let current = after.dag().edge(edge_id);
        if previous == current {
            continue;
        }
        if let Some(edge) = previous {
            roots.insert(edge.destination().node_id());
        }
        if let Some(edge) = current {
            roots.insert(edge.destination().node_id());
        }
    }

    let driver_targets = before
        .parameter_drivers()
        .keys()
        .chain(after.parameter_drivers().keys())
        .copied()
        .collect::<BTreeSet<_>>();
    for target in driver_targets {
        if before.parameter_driver(target) != after.parameter_driver(target) {
            roots.insert(target.node_id());
            changed_parameters.insert(target);
        }
    }

    let mut driver_dependents: BTreeMap<ParameterAddress, BTreeSet<ParameterAddress>> =
        BTreeMap::new();
    for snapshot in [before, after] {
        for (target, driver) in snapshot.parameter_drivers() {
            for dependency in driver.dependencies() {
                driver_dependents
                    .entry(dependency.address())
                    .or_default()
                    .insert(*target);
            }
        }
    }

    let mut pending_parameters = changed_parameters;
    let mut visited_parameters = BTreeSet::new();
    while let Some(address) = pending_parameters.pop_first() {
        if !visited_parameters.insert(address) {
            continue;
        }
        roots.insert(address.node_id());
        if let Some(dependents) = driver_dependents.get(&address) {
            pending_parameters.extend(dependents.iter().copied());
        }
    }

    let mut affected_nodes = roots.clone();
    for graph in [before.dag(), after.dag()] {
        let seeds = roots
            .iter()
            .filter(|node_id| graph.node(**node_id).is_some())
            .map(|node_id| InvalidationSeed::from_region(*node_id, DirtyRegion::FullFrame));
        let propagated = propagate_dependency_invalidation(graph, seeds)?;
        affected_nodes.extend(propagated.nodes().iter().map(InvalidatedNode::node_id));
    }

    Ok(GraphEditInvalidation {
        graph_id: before.graph_id(),
        from_revision: before.revision(),
        to_revision: after.revision(),
        roots: roots.into_iter().collect(),
        affected_nodes: affected_nodes.into_iter().collect(),
    })
}

fn collect_changed_node_state<T: PartialEq>(
    node_id: NodeId,
    previous: &EditableNode<T>,
    current: &EditableNode<T>,
    roots: &mut BTreeSet<NodeId>,
    changed_parameters: &mut BTreeSet<ParameterAddress>,
) {
    let structure_changed = previous.schema() != current.schema()
        || previous.inputs() != current.inputs()
        || previous.outputs() != current.outputs();
    if structure_changed {
        roots.insert(node_id);
    }

    let parameter_ids = previous
        .parameters()
        .keys()
        .chain(current.parameters().keys())
        .copied()
        .collect::<BTreeSet<_>>();
    for parameter_id in parameter_ids {
        if structure_changed || previous.parameter(parameter_id) != current.parameter(parameter_id)
        {
            roots.insert(node_id);
            changed_parameters.insert(ParameterAddress::new(node_id, parameter_id));
        }
    }
}

fn edit_pair_error<T>(
    before: &GraphSnapshot<T>,
    after: &GraphSnapshot<T>,
    category: ErrorCategory,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message).with_context(
        ErrorContext::new(COMPONENT, "derive_edit_invalidation")
            .with_field("before_graph_id", before.graph_id().to_string())
            .with_field("after_graph_id", after.graph_id().to_string())
            .with_field("before_revision", before.revision().to_string())
            .with_field("after_revision", after.revision().to_string())
            .with_field("reason", reason),
    )
}

/// Propagates dirty regions through dependencies that share one coordinate space.
///
/// Use [`propagate_invalidation_with`] when an edge crosses a node-specific transform or can stop
/// propagation. The identity convenience deliberately performs no ROI interpretation.
pub fn propagate_dependency_invalidation<N>(
    graph: &DirectedAcyclicGraph<N>,
    seeds: impl IntoIterator<Item = InvalidationSeed>,
) -> Result<InvalidationPlan> {
    propagate_invalidation_with(graph, seeds, |_, _, dirty| Ok(dirty.clone()))
}

/// Builds a deterministic invalidation plan with caller-owned edge region mapping.
///
/// The mapper receives the immutable graph, exact edge identity, and the source node's merged dirty
/// output. Returning an empty set stops that dependency branch. Errors abort the derived plan and
/// gain exact graph and edge context without changing the graph snapshot. The mapper is invoked in
/// stable edge order and must itself be deterministic to preserve parity.
pub fn propagate_invalidation_with<N, I, F>(
    graph: &DirectedAcyclicGraph<N>,
    seeds: I,
    mut map_edge: F,
) -> Result<InvalidationPlan>
where
    I: IntoIterator<Item = InvalidationSeed>,
    F: FnMut(&DirectedAcyclicGraph<N>, GraphEdge, &DirtyRegionSet) -> Result<DirtyRegionSet>,
{
    let seeds = seeds.into_iter().collect::<Vec<_>>();
    for seed in &seeds {
        if graph.node(seed.node_id).is_none() {
            return Err(unknown_seed_error(graph, seed.node_id));
        }
    }

    let mut dirty_by_node: BTreeMap<NodeId, DirtyRegionSet> = BTreeMap::new();
    for seed in seeds {
        dirty_by_node
            .entry(seed.node_id)
            .or_default()
            .union_with(&seed.dirty_regions);
    }

    let mut invalidated = Vec::new();
    for node_id in graph.topological_order() {
        let Some(dirty_regions) = dirty_by_node.get(&node_id).cloned() else {
            continue;
        };
        if dirty_regions.is_empty() {
            continue;
        }

        for edge_id in graph
            .outgoing_edge_ids(node_id)
            .expect("topological nodes retain outgoing adjacency")
        {
            let graph_edge = *graph
                .edge(*edge_id)
                .expect("outgoing adjacency retains stored edge identity");
            let mapped = map_edge(graph, graph_edge, &dirty_regions).map_err(|mut error| {
                error.push_context(
                    ErrorContext::new(COMPONENT, "map_dirty_edge")
                        .with_field("graph_id", graph.id().to_string())
                        .with_field("edge_id", graph_edge.id().to_string())
                        .with_field("source_node_id", graph_edge.source().node_id().to_string())
                        .with_field(
                            "destination_node_id",
                            graph_edge.destination().node_id().to_string(),
                        ),
                );
                error
            })?;
            if mapped.is_empty() {
                continue;
            }
            dirty_by_node
                .entry(graph_edge.destination().node_id())
                .or_default()
                .union_with(&mapped);
        }

        invalidated.push(InvalidatedNode {
            node_id,
            dirty_regions,
        });
    }

    Ok(InvalidationPlan { nodes: invalidated })
}

fn unknown_seed_error<N>(graph: &DirectedAcyclicGraph<N>, node_id: NodeId) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        "invalidation seed node does not exist",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "propagate_invalidation")
            .with_field("graph_id", graph.id().to_string())
            .with_field("node_id", node_id.to_string()),
    )
}

#[derive(Debug, Eq, PartialEq)]
struct RegionStrip {
    min_x: i32,
    max_x: i32,
    y_intervals: Vec<(i32, i32)>,
}

fn normalize_regions(regions: Vec<PixelBounds>) -> Vec<PixelBounds> {
    let regions = regions
        .into_iter()
        .filter(|region| !region.is_empty())
        .collect::<Vec<_>>();
    if regions.is_empty() {
        return Vec::new();
    }

    let x_edges = regions
        .iter()
        .flat_map(|region| [region.min_x(), region.max_x()])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut strips: Vec<RegionStrip> = Vec::new();

    for edges in x_edges.windows(2) {
        let min_x = edges[0];
        let max_x = edges[1];
        let mut intervals = regions
            .iter()
            .filter(|region| region.min_x() <= min_x && region.max_x() >= max_x)
            .map(|region| (region.min_y(), region.max_y()))
            .collect::<Vec<_>>();
        intervals.sort_unstable();

        let mut merged: Vec<(i32, i32)> = Vec::new();
        for (min_y, max_y) in intervals {
            if let Some((_, current_max_y)) = merged.last_mut() {
                if min_y <= *current_max_y {
                    *current_max_y = (*current_max_y).max(max_y);
                    continue;
                }
            }
            merged.push((min_y, max_y));
        }
        if merged.is_empty() {
            continue;
        }

        if let Some(previous) = strips.last_mut() {
            if previous.max_x == min_x && previous.y_intervals == merged {
                previous.max_x = max_x;
                continue;
            }
        }
        strips.push(RegionStrip {
            min_x,
            max_x,
            y_intervals: merged,
        });
    }

    strips
        .into_iter()
        .flat_map(|strip| {
            strip.y_intervals.into_iter().map(move |(min_y, max_y)| {
                PixelBounds::new(strip.min_x, min_y, strip.max_x, max_y)
                    .expect("normalized region edges preserve valid pixel bounds")
            })
        })
        .collect()
}
