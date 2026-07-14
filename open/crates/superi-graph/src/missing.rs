//! Derived missing-node placeholders for unavailable plugin schemas.
//!
//! Availability is resolved from one immutable editable graph snapshot and one
//! immutable registry snapshot. The derived view never rewrites authored node
//! schemas, ports, parameters, drivers, edges, order, or document bytes.

use std::collections::BTreeMap;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::ids::{GraphId, NodeId};
use crate::mutate::{EditableNode, GraphSnapshot};
use crate::node::{NodeRegistrySnapshot, NodeSchemaId};

const COMPONENT: &str = "superi-graph.missing-node";

/// Why one saved node cannot be bound to a currently registered schema.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum MissingNodeReason {
    /// No registered schema has the saved node's exact identity.
    UnregisteredSchema,
    /// The exact identity is registered with fields that differ from saved state.
    IncompatibleSchema,
}

impl MissingNodeReason {
    /// Returns the stable diagnostic code for this reason.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnregisteredSchema => "unregistered_schema",
            Self::IncompatibleSchema => "incompatible_schema",
        }
    }
}

/// Inspectable derived state for one saved node whose plugin is unavailable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissingNodePlaceholder {
    node_id: NodeId,
    schema_id: NodeSchemaId,
    reason: MissingNodeReason,
}

impl MissingNodePlaceholder {
    fn new(node_id: NodeId, schema_id: NodeSchemaId, reason: MissingNodeReason) -> Self {
        Self {
            node_id,
            schema_id,
            reason,
        }
    }

    /// Returns the stable graph-local node identity.
    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Returns the exact schema identity retained by the saved node.
    #[must_use]
    pub const fn schema_id(&self) -> &NodeSchemaId {
        &self.schema_id
    }

    /// Returns why the saved node is currently unavailable.
    #[must_use]
    pub const fn reason(&self) -> MissingNodeReason {
        self.reason
    }
}

/// Current derived availability of one authored node.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum NodeAvailability {
    /// The registry contains the exact saved schema and definition.
    Available,
    /// The node remains authored and editable through a missing-node placeholder.
    Missing(MissingNodePlaceholder),
}

impl NodeAvailability {
    /// Returns whether the node can be bound to current plugin discovery.
    #[must_use]
    pub const fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    /// Returns the placeholder when current plugin discovery cannot bind the node.
    #[must_use]
    pub const fn placeholder(&self) -> Option<&MissingNodePlaceholder> {
        match self {
            Self::Available => None,
            Self::Missing(placeholder) => Some(placeholder),
        }
    }
}

/// One original editable node paired with its derived availability.
#[derive(Clone, Debug)]
pub struct ResolvedNode<'a, T> {
    node_id: NodeId,
    node: &'a EditableNode<T>,
    availability: &'a NodeAvailability,
}

impl<'a, T> ResolvedNode<'a, T> {
    /// Returns the stable graph-local node identity.
    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Returns the exact original typed editable node.
    #[must_use]
    pub const fn node(&self) -> &'a EditableNode<T> {
        self.node
    }

    /// Returns current derived availability without changing authored state.
    #[must_use]
    pub const fn availability(&self) -> &'a NodeAvailability {
        self.availability
    }

    /// Returns the current missing-node placeholder, if any.
    #[must_use]
    pub const fn placeholder(&self) -> Option<&'a MissingNodePlaceholder> {
        self.availability.placeholder()
    }
}

/// One immutable authored graph plus deterministic current plugin availability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphResolution<T> {
    graph: GraphSnapshot<T>,
    registry_revision: u64,
    availability: BTreeMap<NodeId, NodeAvailability>,
}

impl<T> GraphResolution<T> {
    /// Returns the exact authored immutable graph snapshot.
    #[must_use]
    pub const fn graph(&self) -> &GraphSnapshot<T> {
        &self.graph
    }

    /// Returns the authored graph identity.
    #[must_use]
    pub fn graph_id(&self) -> GraphId {
        self.graph.graph_id()
    }

    /// Returns the authored graph revision used for this resolution.
    #[must_use]
    pub const fn graph_revision(&self) -> u64 {
        self.graph.revision()
    }

    /// Returns the registry revision used for this resolution.
    #[must_use]
    pub const fn registry_revision(&self) -> u64 {
        self.registry_revision
    }

    /// Returns every node in stable identity order with original editable state.
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = ResolvedNode<'_, T>> {
        self.availability.iter().map(|(node_id, availability)| {
            let node = self
                .graph
                .node(*node_id)
                .expect("availability derives from every authored node");
            ResolvedNode {
                node_id: *node_id,
                node,
                availability,
            }
        })
    }

    /// Returns one original editable node and its derived availability.
    #[must_use]
    pub fn node(&self, node_id: NodeId) -> Option<ResolvedNode<'_, T>> {
        let availability = self.availability.get(&node_id)?;
        let node = self
            .graph
            .node(node_id)
            .expect("availability derives from an authored node");
        Some(ResolvedNode {
            node_id,
            node,
            availability,
        })
    }

    /// Returns every missing-node placeholder in stable node identity order.
    pub fn missing_nodes(&self) -> impl DoubleEndedIterator<Item = &MissingNodePlaceholder> {
        self.availability
            .values()
            .filter_map(NodeAvailability::placeholder)
    }

    /// Returns the current number of unavailable authored nodes.
    #[must_use]
    pub fn missing_node_count(&self) -> usize {
        self.missing_nodes().count()
    }

    /// Returns whether every authored node has an exact current schema binding.
    #[must_use]
    pub fn is_evaluable(&self) -> bool {
        self.missing_node_count() == 0
    }

    /// Returns the authored graph only when every node can bind current discovery.
    ///
    /// # Errors
    ///
    /// Returns a degraded unavailable result containing all blockers in stable
    /// node identity order while preserving the editable graph for inspection.
    pub fn require_evaluable(&self) -> Result<&GraphSnapshot<T>> {
        if self.is_evaluable() {
            return Ok(&self.graph);
        }

        let missing_nodes = self
            .missing_nodes()
            .map(|placeholder| {
                format!(
                    "{}={}:{}",
                    placeholder.node_id(),
                    placeholder.schema_id(),
                    placeholder.reason().code()
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        Err(Error::new(
            ErrorCategory::Unavailable,
            Recoverability::Degraded,
            "graph evaluation requires unavailable node plugins",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "require_evaluable")
                .with_field("graph_id", self.graph_id().to_string())
                .with_field("graph_revision", self.graph_revision().to_string())
                .with_field("registry_revision", self.registry_revision.to_string())
                .with_field("missing_node_count", self.missing_node_count().to_string())
                .with_field("missing_nodes", missing_nodes),
        ))
    }
}

/// Resolves current plugin availability without changing authored graph state.
///
/// An exact schema identity with different registered fields fails closed. A
/// later registry snapshot containing the exact saved definition restores
/// availability without a graph transaction or document migration.
#[must_use]
pub fn resolve_graph<T: Clone>(
    graph: &GraphSnapshot<T>,
    registry: &NodeRegistrySnapshot,
) -> GraphResolution<T> {
    let availability = graph
        .dag()
        .nodes()
        .iter()
        .map(|(node_id, node)| {
            let availability = match registry.get(node.schema().id()) {
                Some(schema) if schema == node.schema() => NodeAvailability::Available,
                Some(_) => NodeAvailability::Missing(MissingNodePlaceholder::new(
                    *node_id,
                    node.schema().id().clone(),
                    MissingNodeReason::IncompatibleSchema,
                )),
                None => NodeAvailability::Missing(MissingNodePlaceholder::new(
                    *node_id,
                    node.schema().id().clone(),
                    MissingNodeReason::UnregisteredSchema,
                )),
            };
            (*node_id, availability)
        })
        .collect();
    GraphResolution {
        graph: graph.clone(),
        registry_revision: registry.revision(),
        availability,
    }
}
