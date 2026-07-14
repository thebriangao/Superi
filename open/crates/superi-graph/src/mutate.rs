//! Atomic editable graph mutation transactions.
//!
//! This module binds stable instance port and parameter identifiers to one
//! exact node schema, applies ordered mutations to a private candidate, and
//! publishes one immutable revision only after every mutation succeeds.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::dag::{DirectedAcyclicGraph, GraphEdge};
use crate::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use crate::node::{NodeSchema, ParameterName, PortCardinality, PortName, PortSchema, ValueTypeId};
use crate::validation::validate_connection;

const COMPONENT: &str = "superi-graph.mutation";

/// One opaque editable parameter payload tagged with its graph value type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedParameterValue<T> {
    value_type: ValueTypeId,
    payload: T,
}

impl<T> TypedParameterValue<T> {
    /// Associates an opaque parameter payload with its exact graph value type.
    #[must_use]
    pub const fn new(value_type: ValueTypeId, payload: T) -> Self {
        Self {
            value_type,
            payload,
        }
    }

    /// Returns the exact graph value type.
    #[must_use]
    pub const fn value_type(&self) -> &ValueTypeId {
        &self.value_type
    }

    /// Borrows the caller-owned payload.
    #[must_use]
    pub const fn payload(&self) -> &T {
        &self.payload
    }

    /// Consumes the wrapper and returns the caller-owned payload.
    #[must_use]
    pub fn into_payload(self) -> T {
        self.payload
    }
}

/// A stable instance port identity bound to one schema-local port name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstancePort {
    id: PortId,
    name: PortName,
}

impl InstancePort {
    /// Creates one instance-to-schema port binding.
    #[must_use]
    pub const fn new(id: PortId, name: PortName) -> Self {
        Self { id, name }
    }

    /// Returns the stable instance port identity.
    #[must_use]
    pub const fn id(&self) -> PortId {
        self.id
    }

    /// Returns the exact schema-local port name.
    #[must_use]
    pub const fn name(&self) -> &PortName {
        &self.name
    }
}

/// One editable parameter instance with stable identity and typed state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditableParameter<T> {
    id: ParameterId,
    name: ParameterName,
    value: TypedParameterValue<T>,
}

impl<T> EditableParameter<T> {
    /// Creates one parameter instance binding and its initial value.
    #[must_use]
    pub const fn new(id: ParameterId, name: ParameterName, value: TypedParameterValue<T>) -> Self {
        Self { id, name, value }
    }

    /// Returns the stable parameter identity.
    #[must_use]
    pub const fn id(&self) -> ParameterId {
        self.id
    }

    /// Returns the exact schema-local parameter name.
    #[must_use]
    pub const fn name(&self) -> &ParameterName {
        &self.name
    }

    /// Returns the current typed value.
    #[must_use]
    pub const fn value(&self) -> &TypedParameterValue<T> {
        &self.value
    }
}

/// A complete editable node instance bound to one immutable schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditableNode<T> {
    schema: Arc<NodeSchema>,
    inputs: BTreeMap<PortId, PortName>,
    outputs: BTreeMap<PortId, PortName>,
    parameters: BTreeMap<ParameterId, EditableParameter<T>>,
}

impl<T> EditableNode<T> {
    /// Creates a complete instance and validates every schema binding.
    ///
    /// Inputs, outputs, and parameters must each bind every declaration exactly
    /// once. Parameter values must have the declaration's exact value type.
    ///
    /// # Errors
    ///
    /// Returns user-correctable invalid input for missing, unknown, duplicate,
    /// cross-direction, or mistyped bindings.
    pub fn new(
        schema: Arc<NodeSchema>,
        inputs: impl IntoIterator<Item = InstancePort>,
        outputs: impl IntoIterator<Item = InstancePort>,
        parameters: impl IntoIterator<Item = EditableParameter<T>>,
    ) -> Result<Self> {
        let input_map = collect_ports(&schema, "inputs", inputs, schema.inputs())?;
        let output_map = collect_ports(&schema, "outputs", outputs, schema.outputs())?;
        if let Some(duplicate) = input_map.keys().find(|id| output_map.contains_key(id)) {
            return Err(binding_error(
                &schema,
                "ports",
                "duplicate_instance_id",
                "one instance port identity is bound in both directions",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_node")
                    .with_field("port_id", duplicate.to_string()),
            ));
        }
        let parameter_map = collect_parameters(&schema, parameters)?;
        Ok(Self {
            schema,
            inputs: input_map,
            outputs: output_map,
            parameters: parameter_map,
        })
    }

    /// Returns the exact immutable node schema.
    #[must_use]
    pub fn schema(&self) -> &NodeSchema {
        self.schema.as_ref()
    }

    /// Returns all input bindings in stable instance identity order.
    #[must_use]
    pub const fn inputs(&self) -> &BTreeMap<PortId, PortName> {
        &self.inputs
    }

    /// Returns all output bindings in stable instance identity order.
    #[must_use]
    pub const fn outputs(&self) -> &BTreeMap<PortId, PortName> {
        &self.outputs
    }

    /// Resolves one input instance identity to its schema-local name.
    #[must_use]
    pub fn input_name(&self, id: PortId) -> Option<&PortName> {
        self.inputs.get(&id)
    }

    /// Resolves one output instance identity to its schema-local name.
    #[must_use]
    pub fn output_name(&self, id: PortId) -> Option<&PortName> {
        self.outputs.get(&id)
    }

    /// Returns all parameters in stable instance identity order.
    #[must_use]
    pub const fn parameters(&self) -> &BTreeMap<ParameterId, EditableParameter<T>> {
        &self.parameters
    }

    /// Returns one editable parameter by stable identity.
    #[must_use]
    pub fn parameter(&self, id: ParameterId) -> Option<&EditableParameter<T>> {
        self.parameters.get(&id)
    }

    fn set_parameter(&mut self, id: ParameterId, value: TypedParameterValue<T>) -> Result<()> {
        let parameter = self.parameters.get(&id).ok_or_else(|| {
            Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "graph parameter does not exist on the node instance",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "set_parameter")
                    .with_field("schema_id", self.schema.id().to_string())
                    .with_field("parameter_id", id.to_string()),
            )
        })?;
        let declaration = self
            .schema
            .parameter(parameter.name())
            .expect("validated parameter binding remains declared");
        if declaration.value_type() != value.value_type() {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "parameter value type does not match the node schema",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "set_parameter")
                    .with_field("schema_id", self.schema.id().to_string())
                    .with_field("parameter_id", id.to_string())
                    .with_field("parameter", parameter.name().as_str())
                    .with_field("expected_type", declaration.value_type().as_str())
                    .with_field("actual_type", value.value_type().as_str()),
            ));
        }
        self.parameters
            .get_mut(&id)
            .expect("validated parameter remains present")
            .value = value;
        Ok(())
    }
}

/// One mutation in an ordered graph transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GraphMutation<T> {
    /// Inserts a complete node at one explicit presentation position.
    Add {
        /// Stable graph-local node identity.
        node_id: NodeId,
        /// Complete typed node state.
        node: EditableNode<T>,
        /// Zero-based position in the resulting presentation order.
        position: usize,
    },
    /// Removes one disconnected node.
    Remove {
        /// Stable graph-local node identity.
        node_id: NodeId,
    },
    /// Inserts one typed directed edge.
    Connect {
        /// Complete stable edge route.
        edge: GraphEdge,
    },
    /// Removes one edge by stable identity.
    Disconnect {
        /// Stable edge identity.
        edge_id: EdgeId,
    },
    /// Moves one node in presentation order without changing topology.
    Reorder {
        /// Stable graph-local node identity.
        node_id: NodeId,
        /// Zero-based position in the resulting presentation order.
        position: usize,
    },
    /// Replaces one typed editable parameter value.
    SetParameter {
        /// Stable graph-local node identity.
        node_id: NodeId,
        /// Stable node-local parameter identity.
        parameter_id: ParameterId,
        /// Complete replacement value.
        value: TypedParameterValue<T>,
    },
}

impl<T> GraphMutation<T> {
    /// Returns the stable operation code used in diagnostics.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Add { .. } => "add",
            Self::Remove { .. } => "remove",
            Self::Connect { .. } => "connect",
            Self::Disconnect { .. } => "disconnect",
            Self::Reorder { .. } => "reorder",
            Self::SetParameter { .. } => "set_parameter",
        }
    }
}

/// An ordered optimistic transaction against one exact graph revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphTransaction<T> {
    expected_revision: u64,
    mutations: Vec<GraphMutation<T>>,
}

impl<T> GraphTransaction<T> {
    /// Creates an empty transaction against one exact revision.
    #[must_use]
    pub const fn new(expected_revision: u64) -> Self {
        Self {
            expected_revision,
            mutations: Vec::new(),
        }
    }

    /// Creates a transaction from an ordered mutation sequence.
    #[must_use]
    pub fn with_mutations(
        expected_revision: u64,
        mutations: impl IntoIterator<Item = GraphMutation<T>>,
    ) -> Self {
        Self {
            expected_revision,
            mutations: mutations.into_iter().collect(),
        }
    }

    /// Appends one mutation to the ordered transaction.
    pub fn push(&mut self, mutation: GraphMutation<T>) {
        self.mutations.push(mutation);
    }

    /// Returns the revision this transaction expects to replace.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Returns the ordered mutations.
    #[must_use]
    pub fn mutations(&self) -> &[GraphMutation<T>] {
        &self.mutations
    }

    /// Returns whether the transaction contains no mutations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GraphState<T> {
    dag: DirectedAcyclicGraph<EditableNode<T>>,
    node_order: Vec<NodeId>,
}

/// An immutable revisioned view shared by editor, script, and headless readers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphSnapshot<T> {
    revision: u64,
    state: Arc<GraphState<T>>,
}

impl<T> GraphSnapshot<T> {
    /// Returns the successful mutation revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the checked typed DAG state.
    #[must_use]
    pub fn dag(&self) -> &DirectedAcyclicGraph<EditableNode<T>> {
        &self.state.dag
    }

    /// Returns the graph identity.
    #[must_use]
    pub fn graph_id(&self) -> GraphId {
        self.state.dag.id()
    }

    /// Returns presentation order without changing processing topology.
    #[must_use]
    pub fn node_order(&self) -> &[NodeId] {
        &self.state.node_order
    }

    /// Returns one complete editable node by identity.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&EditableNode<T>> {
        self.state.dag.node(id)
    }
}

/// The mutable owner that atomically publishes immutable graph snapshots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditableGraph<T> {
    revision: u64,
    state: Arc<GraphState<T>>,
}

impl<T> EditableGraph<T> {
    /// Creates one empty editable graph at revision zero.
    #[must_use]
    pub fn new(id: GraphId) -> Self {
        Self {
            revision: 0,
            state: Arc::new(GraphState {
                dag: DirectedAcyclicGraph::new(id),
                node_order: Vec::new(),
            }),
        }
    }

    /// Returns the latest successful transaction revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Captures an immutable view unaffected by later transactions.
    #[must_use]
    pub fn snapshot(&self) -> GraphSnapshot<T> {
        GraphSnapshot {
            revision: self.revision,
            state: Arc::clone(&self.state),
        }
    }

    pub(crate) fn restore_document_revision(&mut self, revision: u64) -> Result<()> {
        if revision == 0 && self.state.dag.node_count() != 0 {
            return Err(Error::new(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "graph document revision zero cannot contain editable nodes",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "restore_document_revision")
                    .with_field("graph_id", self.state.dag.id().to_string())
                    .with_field("node_count", self.state.dag.node_count().to_string()),
            ));
        }
        self.revision = revision;
        Ok(())
    }
}

impl<T: Clone> EditableGraph<T> {
    /// Applies every ordered mutation or publishes none of them.
    ///
    /// Empty transactions validate their expected revision and do not advance
    /// it. Every successful nonempty transaction advances exactly once.
    ///
    /// # Errors
    ///
    /// Returns conflict for a stale revision and preserves the current state.
    /// Any mutation failure retains its original classification, adds its
    /// ordered transaction position, and discards the complete candidate.
    pub fn apply(&mut self, transaction: GraphTransaction<T>) -> Result<GraphSnapshot<T>> {
        if transaction.expected_revision != self.revision {
            return Err(stale_revision_error(
                self.state.dag.id(),
                transaction.expected_revision,
                self.revision,
            ));
        }
        if transaction.mutations.is_empty() {
            return Ok(self.snapshot());
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| revision_error(self.state.dag.id(), self.revision))?;
        let mut candidate = self.state.as_ref().clone();
        for (index, mutation) in transaction.mutations.into_iter().enumerate() {
            let code = mutation.code();
            if let Err(mut error) = apply_mutation(&mut candidate, mutation) {
                error.push_context(
                    ErrorContext::new(COMPONENT, "apply")
                        .with_field("graph_id", candidate.dag.id().to_string())
                        .with_field("expected_revision", self.revision.to_string())
                        .with_field("mutation_index", index.to_string())
                        .with_field("mutation", code),
                );
                return Err(error);
            }
        }

        self.revision = next_revision;
        self.state = Arc::new(candidate);
        Ok(self.snapshot())
    }
}

fn collect_ports<'a>(
    schema: &NodeSchema,
    collection: &'static str,
    ports: impl IntoIterator<Item = InstancePort>,
    declarations: impl Iterator<Item = &'a PortSchema>,
) -> Result<BTreeMap<PortId, PortName>> {
    let declared = declarations
        .map(|declaration| declaration.name().clone())
        .collect::<BTreeSet<_>>();
    let mut names = BTreeSet::new();
    let mut values = BTreeMap::new();
    for port in ports {
        if !declared.contains(&port.name) {
            return Err(binding_error(
                schema,
                collection,
                "unknown_binding",
                "instance port is absent from the node schema",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_node")
                    .with_field("port_id", port.id.to_string())
                    .with_field("port", port.name.as_str()),
            ));
        }
        if !names.insert(port.name.clone()) {
            return Err(binding_error(
                schema,
                collection,
                "duplicate_name",
                "schema-local port name is bound more than once",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_node").with_field("port", port.name.as_str()),
            ));
        }
        let id = port.id;
        if values.insert(id, port.name).is_some() {
            return Err(binding_error(
                schema,
                collection,
                "duplicate_instance_id",
                "instance port identity is bound more than once",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_node").with_field("port_id", id.to_string()),
            ));
        }
    }
    if let Some(missing) = declared.iter().find(|name| !names.contains(*name)) {
        return Err(binding_error(
            schema,
            collection,
            "missing_binding",
            "node instance does not bind every schema port",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "create_node").with_field("port", missing.as_str()),
        ));
    }
    Ok(values)
}

fn collect_parameters<T>(
    schema: &NodeSchema,
    parameters: impl IntoIterator<Item = EditableParameter<T>>,
) -> Result<BTreeMap<ParameterId, EditableParameter<T>>> {
    let declared = schema
        .parameters()
        .map(|declaration| declaration.name().clone())
        .collect::<BTreeSet<_>>();
    let mut names = BTreeSet::new();
    let mut values = BTreeMap::new();
    for parameter in parameters {
        let declaration = schema.parameter(&parameter.name).ok_or_else(|| {
            binding_error(
                schema,
                "parameters",
                "unknown_binding",
                "instance parameter is absent from the node schema",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_node")
                    .with_field("parameter_id", parameter.id.to_string())
                    .with_field("parameter", parameter.name.as_str()),
            )
        })?;
        if !names.insert(parameter.name.clone()) {
            return Err(binding_error(
                schema,
                "parameters",
                "duplicate_name",
                "schema-local parameter name is bound more than once",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_node")
                    .with_field("parameter", parameter.name.as_str()),
            ));
        }
        if declaration.value_type() != parameter.value.value_type() {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "parameter value type does not match the node schema",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_node")
                    .with_field("schema_id", schema.id().to_string())
                    .with_field("parameter_id", parameter.id.to_string())
                    .with_field("parameter", parameter.name.as_str())
                    .with_field("expected_type", declaration.value_type().as_str())
                    .with_field("actual_type", parameter.value.value_type().as_str()),
            ));
        }
        let id = parameter.id;
        if values.insert(id, parameter).is_some() {
            return Err(binding_error(
                schema,
                "parameters",
                "duplicate_instance_id",
                "parameter identity is bound more than once",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_node")
                    .with_field("parameter_id", id.to_string()),
            ));
        }
    }
    if let Some(missing) = declared.iter().find(|name| !names.contains(*name)) {
        return Err(binding_error(
            schema,
            "parameters",
            "missing_binding",
            "node instance does not bind every schema parameter",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "create_node").with_field("parameter", missing.as_str()),
        ));
    }
    Ok(values)
}

fn binding_error(
    schema: &NodeSchema,
    collection: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "create_node")
            .with_field("schema_id", schema.id().to_string())
            .with_field("collection", collection)
            .with_field("reason", reason),
    )
}

fn apply_mutation<T>(state: &mut GraphState<T>, mutation: GraphMutation<T>) -> Result<()> {
    match mutation {
        GraphMutation::Add {
            node_id,
            node,
            position,
        } => {
            if position > state.node_order.len() {
                return Err(position_error(
                    state.dag.id(),
                    "add",
                    node_id,
                    position,
                    state.node_order.len(),
                ));
            }
            state.dag.insert_node(node_id, node)?;
            state.node_order.insert(position, node_id);
            Ok(())
        }
        GraphMutation::Remove { node_id } => {
            let _ = state.dag.remove_node(node_id)?;
            let position = state
                .node_order
                .iter()
                .position(|stored| *stored == node_id)
                .expect("stored node remains present in presentation order");
            state.node_order.remove(position);
            Ok(())
        }
        GraphMutation::Connect { edge } => {
            validate_stored_connection(&state.dag, edge)?;
            state.dag.insert_edge(edge)
        }
        GraphMutation::Disconnect { edge_id } => {
            let _ = state.dag.remove_edge(edge_id)?;
            Ok(())
        }
        GraphMutation::Reorder { node_id, position } => {
            let current = state
                .node_order
                .iter()
                .position(|stored| *stored == node_id)
                .ok_or_else(|| missing_node_error(state.dag.id(), "reorder", node_id))?;
            if position >= state.node_order.len() {
                return Err(position_error(
                    state.dag.id(),
                    "reorder",
                    node_id,
                    position,
                    state.node_order.len(),
                ));
            }
            state.node_order.remove(current);
            state.node_order.insert(position, node_id);
            Ok(())
        }
        GraphMutation::SetParameter {
            node_id,
            parameter_id,
            value,
        } => {
            let graph_id = state.dag.id();
            state
                .dag
                .node_mut(node_id)
                .ok_or_else(|| missing_node_error(graph_id, "set_parameter", node_id))?
                .set_parameter(parameter_id, value)
        }
    }
}

fn validate_stored_connection<T>(
    dag: &DirectedAcyclicGraph<EditableNode<T>>,
    edge: GraphEdge,
) -> Result<()> {
    let source_id = edge.source().node_id();
    let target_id = edge.destination().node_id();
    let source = dag
        .node(source_id)
        .ok_or_else(|| missing_node_error(dag.id(), "connect", source_id))?;
    let target = dag
        .node(target_id)
        .ok_or_else(|| missing_node_error(dag.id(), "connect", target_id))?;
    let source_port_id = edge.source().port_id();
    let target_port_id = edge.destination().port_id();
    let source_name = source
        .output_name(source_port_id)
        .ok_or_else(|| missing_port_error(dag.id(), edge, "source_output", source_port_id))?;
    let target_name = target
        .input_name(target_port_id)
        .ok_or_else(|| missing_port_error(dag.id(), edge, "target_input", target_port_id))?;
    validate_connection(source.schema(), source_name, target.schema(), target_name)?;

    let cardinality = target
        .schema()
        .input(target_name)
        .expect("validated input binding remains declared")
        .cardinality();
    let incoming_count = dag
        .incoming_edge_ids(target_id)
        .expect("stored target owns incoming adjacency")
        .iter()
        .filter(|edge_id| {
            dag.edge(**edge_id)
                .is_some_and(|stored| stored.destination().port_id() == target_port_id)
        })
        .count();
    if matches!(
        cardinality,
        PortCardinality::Single | PortCardinality::Optional
    ) && incoming_count >= 1
    {
        return Err(Error::new(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "target input already has its maximum connection count",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "connect")
                .with_field("graph_id", dag.id().to_string())
                .with_field("edge_id", edge.id().to_string())
                .with_field("target_node_id", target_id.to_string())
                .with_field("target_port_id", target_port_id.to_string())
                .with_field("target_port", target_name.as_str())
                .with_field("cardinality", cardinality_code(cardinality))
                .with_field("existing_connections", incoming_count.to_string()),
        ));
    }
    Ok(())
}

fn cardinality_code(cardinality: PortCardinality) -> &'static str {
    match cardinality {
        PortCardinality::Single => "single",
        PortCardinality::Optional => "optional",
        PortCardinality::Variadic => "variadic",
    }
}

fn stale_revision_error(graph_id: GraphId, expected: u64, actual: u64) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        "graph transaction revision is stale",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "apply")
            .with_field("graph_id", graph_id.to_string())
            .with_field("expected_revision", expected.to_string())
            .with_field("actual_revision", actual.to_string()),
    )
}

fn revision_error(graph_id: GraphId, revision: u64) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Terminal,
        "graph mutation revision space is exhausted",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "apply")
            .with_field("graph_id", graph_id.to_string())
            .with_field("revision", revision.to_string()),
    )
}

fn missing_node_error(graph_id: GraphId, operation: &'static str, node_id: NodeId) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        "graph node does not exist",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("graph_id", graph_id.to_string())
            .with_field("node_id", node_id.to_string()),
    )
}

fn missing_port_error(
    graph_id: GraphId,
    edge: GraphEdge,
    endpoint: &'static str,
    port_id: PortId,
) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        "graph endpoint port is not bound to the node schema",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "connect")
            .with_field("graph_id", graph_id.to_string())
            .with_field("edge_id", edge.id().to_string())
            .with_field("endpoint", endpoint)
            .with_field("port_id", port_id.to_string()),
    )
}

fn position_error(
    graph_id: GraphId,
    operation: &'static str,
    node_id: NodeId,
    position: usize,
    node_count: usize,
) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "graph presentation position is out of bounds",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("graph_id", graph_id.to_string())
            .with_field("node_id", node_id.to_string())
            .with_field("position", position.to_string())
            .with_field("node_count", node_count.to_string()),
    )
}
