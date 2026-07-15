//! Atomic editable graph mutation transactions.
//!
//! This module binds stable instance port and parameter identifiers to one
//! exact node schema, applies ordered mutations to a private candidate, and
//! publishes one immutable revision only after every mutation succeeds.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::dag::{DirectedAcyclicGraph, GraphEdge};
use crate::expr::{
    ExpressionParameterValue, ParameterAddress, ParameterDriver, ParameterReference,
};
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
    /// Creates or replaces one typed parameter driver.
    SetParameterDriver {
        /// Stable graph-local target parameter.
        target: ParameterAddress,
        /// Complete direct-link or expression state.
        driver: ParameterDriver,
    },
    /// Removes one parameter driver and exposes the stored literal again.
    ClearParameterDriver {
        /// Stable graph-local target parameter.
        target: ParameterAddress,
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
            Self::SetParameterDriver { .. } => "set_parameter_driver",
            Self::ClearParameterDriver { .. } => "clear_parameter_driver",
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
    parameter_drivers: BTreeMap<ParameterAddress, ParameterDriver>,
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

    /// Returns every authored driver in canonical target-address order.
    #[must_use]
    pub fn parameter_drivers(&self) -> &BTreeMap<ParameterAddress, ParameterDriver> {
        &self.state.parameter_drivers
    }

    /// Returns the driver attached to one target parameter.
    #[must_use]
    pub fn parameter_driver(&self, target: ParameterAddress) -> Option<&ParameterDriver> {
        self.state.parameter_drivers.get(&target)
    }
}

/// One deterministic typed parameter result from an immutable graph snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParameterEvaluation<T> {
    target: ParameterAddress,
    value: TypedParameterValue<T>,
    evaluated_parameters: Vec<ParameterAddress>,
}

impl<T> ParameterEvaluation<T> {
    /// Returns the requested target parameter.
    #[must_use]
    pub const fn target(&self) -> ParameterAddress {
        self.target
    }

    /// Returns the resolved typed value.
    #[must_use]
    pub const fn value(&self) -> &TypedParameterValue<T> {
        &self.value
    }

    /// Returns unique parameters in deterministic dependency-completion order.
    #[must_use]
    pub fn evaluated_parameters(&self) -> &[ParameterAddress] {
        &self.evaluated_parameters
    }
}

impl<T> GraphSnapshot<T> {
    /// Resolves one parameter after projecting stored literals into an expression-capable domain.
    ///
    /// The graph remains the sole owner of driver traversal, exact dependency typing, direct-link
    /// behavior, expression evaluation, cycle invariants, and deterministic completion order. The
    /// caller projection runs only for reached parameters without drivers, allowing a higher domain
    /// to sample or otherwise adapt its literal payload without copying graph topology or formulas.
    /// The projected payload keeps the literal's exact [`ValueTypeId`].
    ///
    /// # Errors
    ///
    /// Returns user-correctable errors for a missing target, projection failure, or projected
    /// expression conversion failure. A cycle or dangling reference in already published state is
    /// classified as an internal invariant failure because mutation validation prevents either
    /// condition.
    pub fn evaluate_parameter_with<U: ExpressionParameterValue>(
        &self,
        target: ParameterAddress,
        mut project: impl FnMut(ParameterAddress, &TypedParameterValue<T>) -> Result<U>,
    ) -> Result<ParameterEvaluation<U>> {
        if self
            .state
            .dag
            .node(target.node_id())
            .and_then(|node| node.parameter(target.parameter_id()))
            .is_none()
        {
            return Err(parameter_evaluation_error(
                self.graph_id(),
                target,
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "missing_parameter_target",
                "requested graph parameter does not exist",
            ));
        }

        let mut evaluated = BTreeMap::new();
        let mut completion = Vec::new();
        let mut active = BTreeSet::new();
        let value = resolve_parameter(
            self.state.as_ref(),
            target,
            &mut evaluated,
            &mut completion,
            &mut active,
            &mut project,
        )?;
        Ok(ParameterEvaluation {
            target,
            value,
            evaluated_parameters: completion,
        })
    }
}

impl<T: ExpressionParameterValue> GraphSnapshot<T> {
    /// Resolves one parameter from this exact immutable editable snapshot.
    ///
    /// Direct links clone the source payload without conversion. Expressions resolve their
    /// explicit dependencies first and use [`ExpressionParameterValue`] only at the numeric
    /// expression boundary. Every call starts with an empty request-local result set.
    ///
    /// # Errors
    ///
    /// Returns user-correctable errors for a missing target or payload conversion failure. A cycle
    /// or dangling reference in already published state is classified as an internal invariant
    /// failure because mutation validation prevents either condition.
    pub fn evaluate_parameter(&self, target: ParameterAddress) -> Result<ParameterEvaluation<T>> {
        self.evaluate_parameter_with(target, |_, literal| Ok(literal.payload().clone()))
    }
}

fn resolve_parameter<T, U, F>(
    state: &GraphState<T>,
    address: ParameterAddress,
    evaluated: &mut BTreeMap<ParameterAddress, TypedParameterValue<U>>,
    completion: &mut Vec<ParameterAddress>,
    active: &mut BTreeSet<ParameterAddress>,
    project: &mut F,
) -> Result<TypedParameterValue<U>>
where
    U: ExpressionParameterValue,
    F: FnMut(ParameterAddress, &TypedParameterValue<T>) -> Result<U>,
{
    if let Some(value) = evaluated.get(&address) {
        return Ok(value.clone());
    }
    if !active.insert(address) {
        return Err(parameter_evaluation_error(
            state.dag.id(),
            address,
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "parameter_dependency_cycle",
            "published parameter state contains a dependency cycle",
        ));
    }

    let result = resolve_parameter_uncached(state, address, evaluated, completion, active, project);
    active.remove(&address);
    let value = result?;
    evaluated.insert(address, value.clone());
    completion.push(address);
    Ok(value)
}

fn resolve_parameter_uncached<T, U, F>(
    state: &GraphState<T>,
    address: ParameterAddress,
    evaluated: &mut BTreeMap<ParameterAddress, TypedParameterValue<U>>,
    completion: &mut Vec<ParameterAddress>,
    active: &mut BTreeSet<ParameterAddress>,
    project: &mut F,
) -> Result<TypedParameterValue<U>>
where
    U: ExpressionParameterValue,
    F: FnMut(ParameterAddress, &TypedParameterValue<T>) -> Result<U>,
{
    let literal = state
        .dag
        .node(address.node_id())
        .and_then(|node| node.parameter(address.parameter_id()))
        .map(EditableParameter::value)
        .ok_or_else(|| {
            parameter_evaluation_error(
                state.dag.id(),
                address,
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "dangling_parameter_dependency",
                "published parameter driver references a missing parameter",
            )
        })?;
    let Some(driver) = state.parameter_drivers.get(&address) else {
        let payload = project(address, literal).map_err(|mut error| {
            error.push_context(
                ErrorContext::new(COMPONENT, "evaluate_parameter")
                    .with_field("graph_id", state.dag.id().to_string())
                    .with_field("parameter", address.to_string())
                    .with_field("reason", "parameter_projection_failed"),
            );
            error
        })?;
        return Ok(TypedParameterValue::new(
            literal.value_type().clone(),
            payload,
        ));
    };

    for dependency in driver.dependencies() {
        let _ = resolve_parameter(
            state,
            dependency.address(),
            evaluated,
            completion,
            active,
            project,
        )?;
    }

    if let Some(source) = driver.as_link() {
        return evaluated.get(&source.address()).cloned().ok_or_else(|| {
            parameter_evaluation_error(
                state.dag.id(),
                address,
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "missing_parameter_result",
                "parameter link dependency did not produce a value",
            )
        });
    }

    let expression = driver
        .as_expression()
        .expect("parameter driver kind is link or expression");
    let scalar = expression.evaluate_with(|name, reference| {
        let value = evaluated.get(&reference.address()).ok_or_else(|| {
            parameter_evaluation_error(
                state.dag.id(),
                address,
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "missing_parameter_result",
                "expression dependency did not produce a value",
            )
        })?;
        value
            .payload()
            .to_expression_scalar(reference.value_type())
            .map_err(|mut error| {
                error.push_context(
                    ErrorContext::new(COMPONENT, "evaluate_parameter")
                        .with_field("graph_id", state.dag.id().to_string())
                        .with_field("target", address.to_string())
                        .with_field("variable", name.as_str())
                        .with_field("dependency", reference.address().to_string()),
                );
                error
            })
    })?;
    let payload = U::from_expression_scalar(driver.value_type(), scalar).map_err(|mut error| {
        error.push_context(
            ErrorContext::new(COMPONENT, "evaluate_parameter")
                .with_field("graph_id", state.dag.id().to_string())
                .with_field("target", address.to_string())
                .with_field("result_type", driver.value_type().as_str()),
        );
        error
    })?;
    Ok(TypedParameterValue::new(
        driver.value_type().clone(),
        payload,
    ))
}

fn parameter_evaluation_error(
    graph_id: GraphId,
    target: ParameterAddress,
    category: ErrorCategory,
    recoverability: Recoverability,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message).with_context(
        ErrorContext::new(COMPONENT, "evaluate_parameter")
            .with_field("graph_id", graph_id.to_string())
            .with_field("target", target.to_string())
            .with_field("reason", reason),
    )
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
                parameter_drivers: BTreeMap::new(),
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
            validate_node_driver_removal(state, node_id)?;
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
        GraphMutation::SetParameterDriver { target, driver } => {
            validate_parameter_driver(state, target, &driver)?;
            state.parameter_drivers.insert(target, driver);
            if parameter_dependency_reaches(state, target, target, &mut BTreeSet::new()) {
                return Err(parameter_driver_error(
                    state.dag.id(),
                    "set_parameter_driver",
                    target,
                    ErrorCategory::Conflict,
                    "parameter_dependency_cycle",
                    "parameter driver would create a dependency cycle",
                ));
            }
            Ok(())
        }
        GraphMutation::ClearParameterDriver { target } => {
            let _ = parameter_value_type(
                state,
                target,
                "clear_parameter_driver",
                "missing_parameter_target",
            )?;
            if state.parameter_drivers.remove(&target).is_none() {
                return Err(parameter_driver_error(
                    state.dag.id(),
                    "clear_parameter_driver",
                    target,
                    ErrorCategory::NotFound,
                    "missing_parameter_driver",
                    "parameter does not have an authored driver",
                ));
            }
            Ok(())
        }
    }
}

fn validate_parameter_driver<T>(
    state: &GraphState<T>,
    target: ParameterAddress,
    driver: &ParameterDriver,
) -> Result<()> {
    let target_type = parameter_value_type(
        state,
        target,
        "set_parameter_driver",
        "missing_parameter_target",
    )?;
    if target_type != driver.value_type() {
        return Err(parameter_driver_type_error(
            state.dag.id(),
            target,
            None,
            "parameter_driver_type_mismatch",
            target_type,
            driver.value_type(),
            "parameter driver result type does not match its target",
        ));
    }

    for dependency in driver.dependencies() {
        let source_type = parameter_value_type(
            state,
            dependency.address(),
            "set_parameter_driver",
            "missing_parameter_dependency",
        )?;
        if source_type != dependency.value_type() {
            return Err(parameter_driver_type_error(
                state.dag.id(),
                target,
                Some(dependency),
                "parameter_dependency_type_mismatch",
                source_type,
                dependency.value_type(),
                "parameter dependency type does not match its source",
            ));
        }
        if driver.as_link().is_some() && source_type != target_type {
            return Err(parameter_driver_type_error(
                state.dag.id(),
                target,
                Some(dependency),
                "parameter_link_type_mismatch",
                target_type,
                source_type,
                "direct parameter link requires equal source and target types",
            ));
        }
    }
    Ok(())
}

fn validate_node_driver_removal<T>(state: &GraphState<T>, node_id: NodeId) -> Result<()> {
    let reference = state.parameter_drivers.iter().find(|(target, driver)| {
        target.node_id() == node_id
            || driver
                .dependencies()
                .iter()
                .any(|dependency| dependency.address().node_id() == node_id)
    });
    if let Some((target, _)) = reference {
        return Err(parameter_driver_error(
            state.dag.id(),
            "remove",
            *target,
            ErrorCategory::Conflict,
            "parameter_driver_references_node",
            "node remains referenced by an authored parameter driver",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "remove").with_field("node_id", node_id.to_string()),
        ));
    }
    Ok(())
}

fn parameter_dependency_reaches<T>(
    state: &GraphState<T>,
    current: ParameterAddress,
    target: ParameterAddress,
    visited: &mut BTreeSet<ParameterAddress>,
) -> bool {
    let Some(driver) = state.parameter_drivers.get(&current) else {
        return false;
    };
    for dependency in driver.dependencies() {
        let address = dependency.address();
        if address == target {
            return true;
        }
        if visited.insert(address) && parameter_dependency_reaches(state, address, target, visited)
        {
            return true;
        }
    }
    false
}

fn parameter_value_type<'a, T>(
    state: &'a GraphState<T>,
    address: ParameterAddress,
    operation: &'static str,
    reason: &'static str,
) -> Result<&'a ValueTypeId> {
    state
        .dag
        .node(address.node_id())
        .and_then(|node| node.parameter(address.parameter_id()))
        .map(|parameter| parameter.value().value_type())
        .ok_or_else(|| {
            parameter_driver_error(
                state.dag.id(),
                operation,
                address,
                ErrorCategory::NotFound,
                reason,
                "graph parameter dependency does not exist",
            )
        })
}

fn parameter_driver_type_error(
    graph_id: GraphId,
    target: ParameterAddress,
    dependency: Option<&ParameterReference>,
    reason: &'static str,
    expected: &ValueTypeId,
    actual: &ValueTypeId,
    message: &'static str,
) -> Error {
    let mut context = ErrorContext::new(COMPONENT, "set_parameter_driver")
        .with_field("graph_id", graph_id.to_string())
        .with_field("target", target.to_string())
        .with_field("reason", reason)
        .with_field("expected_type", expected.as_str())
        .with_field("actual_type", actual.as_str());
    if let Some(dependency) = dependency {
        context.insert_field("dependency", dependency.address().to_string());
    }
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context)
}

fn parameter_driver_error(
    graph_id: GraphId,
    operation: &'static str,
    target: ParameterAddress,
    category: ErrorCategory,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message).with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("graph_id", graph_id.to_string())
            .with_field("target", target.to_string())
            .with_field("reason", reason),
    )
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
