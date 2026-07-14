//! Deterministic directed acyclic graph storage.
//!
//! The store is generic over node payloads so node schemas and instances can
//! evolve above the graph algorithm. Connections retain official typed graph,
//! node, port, and edge identity while validation prevents cycles before any
//! collection changes.

use std::collections::{BTreeMap, BTreeSet};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::ids::{EdgeId, GraphId, NodeId, PortId};

const COMPONENT: &str = "superi-graph.dag";

/// One typed endpoint of a directed graph connection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GraphEndpoint {
    node_id: NodeId,
    port_id: PortId,
}

impl GraphEndpoint {
    /// Creates an endpoint from its owning node and node-local port identities.
    #[must_use]
    pub const fn new(node_id: NodeId, port_id: PortId) -> Self {
        Self { node_id, port_id }
    }

    /// Returns the node that owns this endpoint.
    #[must_use]
    pub const fn node_id(self) -> NodeId {
        self.node_id
    }

    /// Returns the typed port identity.
    #[must_use]
    pub const fn port_id(self) -> PortId {
        self.port_id
    }
}

/// One typed directed edge from an output endpoint to an input endpoint.
///
/// Port direction and data compatibility belong to the node-schema validator.
/// This value owns only stable connection identity and routing.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GraphEdge {
    id: EdgeId,
    source: GraphEndpoint,
    destination: GraphEndpoint,
}

impl GraphEdge {
    /// Creates a directed connection.
    #[must_use]
    pub const fn new(id: EdgeId, source: GraphEndpoint, destination: GraphEndpoint) -> Self {
        Self {
            id,
            source,
            destination,
        }
    }

    /// Returns the stable edge identity.
    #[must_use]
    pub const fn id(self) -> EdgeId {
        self.id
    }

    /// Returns the source endpoint.
    #[must_use]
    pub const fn source(self) -> GraphEndpoint {
        self.source
    }

    /// Returns the destination endpoint.
    #[must_use]
    pub const fn destination(self) -> GraphEndpoint {
        self.destination
    }
}

/// Authoritative deterministic storage for one directed acyclic graph.
///
/// Primary and adjacency collections are key ordered, so inspection does not
/// depend on insertion history or randomized hashing. Node payloads remain
/// generic and are never interpreted by cycle prevention.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectedAcyclicGraph<N> {
    id: GraphId,
    nodes: BTreeMap<NodeId, N>,
    edges: BTreeMap<EdgeId, GraphEdge>,
    incoming: BTreeMap<NodeId, BTreeSet<EdgeId>>,
    outgoing: BTreeMap<NodeId, BTreeSet<EdgeId>>,
}

impl<N> DirectedAcyclicGraph<N> {
    /// Creates an empty graph with stable identity.
    #[must_use]
    pub const fn new(id: GraphId) -> Self {
        Self {
            id,
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            incoming: BTreeMap::new(),
            outgoing: BTreeMap::new(),
        }
    }

    /// Returns the graph identity.
    #[must_use]
    pub const fn id(&self) -> GraphId {
        self.id
    }

    /// Returns the number of stored nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of stored edges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Returns all nodes in stable identity order.
    #[must_use]
    pub const fn nodes(&self) -> &BTreeMap<NodeId, N> {
        &self.nodes
    }

    /// Returns one node payload by identity.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&N> {
        self.nodes.get(&id)
    }

    /// Returns all edges in stable identity order.
    #[must_use]
    pub const fn edges(&self) -> &BTreeMap<EdgeId, GraphEdge> {
        &self.edges
    }

    /// Returns one edge by identity.
    #[must_use]
    pub fn edge(&self, id: EdgeId) -> Option<&GraphEdge> {
        self.edges.get(&id)
    }

    /// Returns the incoming edge identities for a stored node.
    #[must_use]
    pub fn incoming_edge_ids(&self, id: NodeId) -> Option<&BTreeSet<EdgeId>> {
        self.incoming.get(&id)
    }

    /// Returns the outgoing edge identities for a stored node.
    #[must_use]
    pub fn outgoing_edge_ids(&self, id: NodeId) -> Option<&BTreeSet<EdgeId>> {
        self.outgoing.get(&id)
    }

    /// Inserts one uniquely identified node payload.
    ///
    /// # Errors
    ///
    /// Returns a user-correctable conflict without changing the graph when the
    /// node identity is already present.
    pub fn insert_node(&mut self, id: NodeId, node: N) -> Result<()> {
        if self.nodes.contains_key(&id) {
            return Err(self.error(
                ErrorCategory::Conflict,
                "insert_node",
                "graph node identity already exists",
                [("node_id", id.to_string())],
            ));
        }

        self.nodes.insert(id, node);
        self.incoming.insert(id, BTreeSet::new());
        self.outgoing.insert(id, BTreeSet::new());
        Ok(())
    }

    /// Removes one disconnected node and returns its payload.
    ///
    /// # Errors
    ///
    /// Returns not found for an unknown node or conflict for a node that still
    /// has incident edges. Either failure leaves the graph unchanged.
    pub fn remove_node(&mut self, id: NodeId) -> Result<N> {
        if !self.nodes.contains_key(&id) {
            return Err(self.error(
                ErrorCategory::NotFound,
                "remove_node",
                "graph node does not exist",
                [("node_id", id.to_string())],
            ));
        }

        let has_incoming = self
            .incoming
            .get(&id)
            .is_some_and(|edges| !edges.is_empty());
        let has_outgoing = self
            .outgoing
            .get(&id)
            .is_some_and(|edges| !edges.is_empty());
        if has_incoming || has_outgoing {
            return Err(self.error(
                ErrorCategory::Conflict,
                "remove_node",
                "graph node must be disconnected before removal",
                [("node_id", id.to_string())],
            ));
        }

        self.incoming.remove(&id);
        self.outgoing.remove(&id);
        Ok(self
            .nodes
            .remove(&id)
            .expect("validated node remains present until removal"))
    }

    /// Inserts one edge after complete endpoint and cycle validation.
    ///
    /// Parallel edges with distinct identities are retained. Port direction and
    /// compatibility are intentionally deferred to node-schema validation.
    ///
    /// # Errors
    ///
    /// Returns conflict for a duplicate edge identity or any direct or
    /// transitive cycle, and not found for a missing endpoint node. Validation
    /// completes before any collection changes, so every rejection is atomic.
    pub fn insert_edge(&mut self, edge: GraphEdge) -> Result<()> {
        if self.edges.contains_key(&edge.id) {
            return Err(self.edge_error(
                ErrorCategory::Conflict,
                "graph edge identity already exists",
                edge,
            ));
        }
        if !self.nodes.contains_key(&edge.source.node_id) {
            return Err(self.edge_error(
                ErrorCategory::NotFound,
                "graph edge source node does not exist",
                edge,
            ));
        }
        if !self.nodes.contains_key(&edge.destination.node_id) {
            return Err(self.edge_error(
                ErrorCategory::NotFound,
                "graph edge destination node does not exist",
                edge,
            ));
        }
        if edge.source.node_id == edge.destination.node_id
            || self.reaches(edge.destination.node_id, edge.source.node_id)
        {
            return Err(self.edge_error(
                ErrorCategory::Conflict,
                "graph edge would create a directed cycle",
                edge,
            ));
        }

        self.edges.insert(edge.id, edge);
        self.outgoing
            .get_mut(&edge.source.node_id)
            .expect("validated source owns adjacency")
            .insert(edge.id);
        self.incoming
            .get_mut(&edge.destination.node_id)
            .expect("validated destination owns adjacency")
            .insert(edge.id);
        Ok(())
    }

    /// Removes one edge and returns its complete typed route.
    ///
    /// # Errors
    ///
    /// Returns not found without changing the graph when the edge is absent.
    pub fn remove_edge(&mut self, id: EdgeId) -> Result<GraphEdge> {
        let edge = self.edges.get(&id).copied().ok_or_else(|| {
            self.error(
                ErrorCategory::NotFound,
                "remove_edge",
                "graph edge does not exist",
                [("edge_id", id.to_string())],
            )
        })?;

        self.outgoing
            .get_mut(&edge.source.node_id)
            .expect("stored edge source owns adjacency")
            .remove(&id);
        self.incoming
            .get_mut(&edge.destination.node_id)
            .expect("stored edge destination owns adjacency")
            .remove(&id);
        self.edges.remove(&id);
        Ok(edge)
    }

    /// Computes the stable topological node order.
    ///
    /// When several nodes are ready, the smallest typed node identity is chosen.
    /// The graph's checked mutation boundary guarantees a complete order.
    #[must_use]
    pub fn topological_order(&self) -> Vec<NodeId> {
        let mut indegree = self
            .incoming
            .iter()
            .map(|(node_id, edges)| (*node_id, edges.len()))
            .collect::<BTreeMap<_, _>>();
        let mut ready = indegree
            .iter()
            .filter_map(|(node_id, degree)| (*degree == 0).then_some(*node_id))
            .collect::<BTreeSet<_>>();
        let mut order = Vec::with_capacity(self.nodes.len());

        while let Some(node_id) = ready.iter().next().copied() {
            ready.remove(&node_id);
            order.push(node_id);
            for edge_id in &self.outgoing[&node_id] {
                let destination = self.edges[edge_id].destination.node_id;
                let degree = indegree
                    .get_mut(&destination)
                    .expect("stored edge destination has an indegree");
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(destination);
                }
            }
        }

        debug_assert_eq!(order.len(), self.nodes.len());
        order
    }

    fn reaches(&self, start: NodeId, target: NodeId) -> bool {
        let mut pending = vec![start];
        let mut visited = BTreeSet::new();
        while let Some(node_id) = pending.pop() {
            if node_id == target {
                return true;
            }
            if !visited.insert(node_id) {
                continue;
            }
            for edge_id in &self.outgoing[&node_id] {
                pending.push(self.edges[edge_id].destination.node_id);
            }
        }
        false
    }

    fn edge_error(&self, category: ErrorCategory, message: &'static str, edge: GraphEdge) -> Error {
        self.error(
            category,
            "insert_edge",
            message,
            [
                ("edge_id", edge.id.to_string()),
                ("source_node_id", edge.source.node_id.to_string()),
                ("destination_node_id", edge.destination.node_id.to_string()),
            ],
        )
    }

    fn error<const N_FIELDS: usize>(
        &self,
        category: ErrorCategory,
        operation: &'static str,
        message: &'static str,
        fields: [(&'static str, String); N_FIELDS],
    ) -> Error {
        let mut context =
            ErrorContext::new(COMPONENT, operation).with_field("graph_id", self.id.to_string());
        for (name, value) in fields {
            context.insert_field(name, value);
        }
        Error::new(category, Recoverability::UserCorrectable, message).with_context(context)
    }
}
