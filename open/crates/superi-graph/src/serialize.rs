//! Versioned deterministic graph documents and checked recovery.
//!
//! The codec owns the portable graph boundary, not project file I/O. It records
//! complete schema definitions and editable instance state, protects the
//! canonical payload with SHA-256, migrates supported older envelopes, and
//! reconstructs state exclusively through the graph's checked constructors and
//! atomic mutation transaction.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::{CapabilityId, CapabilitySet, SemanticVersion};

use crate::dag::{GraphEdge, GraphEndpoint};
use crate::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use crate::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphSnapshot, GraphTransaction,
    InstancePort, TypedParameterValue,
};
use crate::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};

const COMPONENT: &str = "superi-graph.serialization";
const GRAPH_DOCUMENT_FORMAT: &str = "superi.graph";
const LEGACY_GRAPH_DOCUMENT_FORMAT_REVISION: u32 = 0;

/// The current incompatible revision of the graph document contract.
pub const GRAPH_DOCUMENT_FORMAT_REVISION: u32 = 1;

/// One fully validated graph load and its canonical current-format document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphLoad<T> {
    graph: EditableGraph<T>,
    source_format_revision: u32,
    canonical_document: Vec<u8>,
}

impl<T> GraphLoad<T> {
    /// Returns the reconstructed editable graph.
    #[must_use]
    pub const fn graph(&self) -> &EditableGraph<T> {
        &self.graph
    }

    /// Consumes the load result and returns the reconstructed editable graph.
    #[must_use]
    pub fn into_graph(self) -> EditableGraph<T> {
        self.graph
    }

    /// Returns the format revision found in the supplied document.
    #[must_use]
    pub const fn source_format_revision(&self) -> u32 {
        self.source_format_revision
    }

    /// Returns whether a supported older document was migrated during loading.
    #[must_use]
    pub const fn was_migrated(&self) -> bool {
        self.source_format_revision != GRAPH_DOCUMENT_FORMAT_REVISION
    }

    /// Returns deterministic current-format bytes produced only after validation.
    #[must_use]
    pub fn canonical_document(&self) -> &[u8] {
        &self.canonical_document
    }
}

/// Serializes one immutable editable graph snapshot into canonical JSON bytes.
///
/// Parameter payloads remain caller-owned ordinary serializable state. JSON
/// object keys are canonicalized before schema hashing and document encoding.
///
/// # Errors
///
/// Returns a classified error when a payload cannot be represented as JSON or
/// when two nodes use conflicting schema definitions under one schema identity.
pub fn serialize_graph<T>(snapshot: &GraphSnapshot<T>) -> Result<Vec<u8>>
where
    T: Serialize,
{
    let payload = GraphPayloadWire::from_snapshot(snapshot)?;
    encode_current_document(&payload)
}

/// Loads, migrates, validates, and reconstructs one editable graph document.
///
/// Revision zero documents are upgraded in memory to the integrity-protected
/// current envelope. The original bytes are never changed. Callers may persist
/// [`GraphLoad::canonical_document`] only after their project transaction and
/// recovery policy accepts the successfully validated result.
///
/// # Errors
///
/// Returns corrupt data for malformed JSON, unknown fields, or checksum failure;
/// unsupported for unknown formats or incompatible revisions; and the checked
/// graph constructors' classifications for invalid editable state.
pub fn deserialize_graph<T>(document: &[u8]) -> Result<GraphLoad<T>>
where
    T: Clone + DeserializeOwned,
{
    let raw = serde_json::from_slice::<Value>(document).map_err(|source| {
        document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "decode_document",
            "graph document is not valid JSON",
            [("source", source.to_string())],
        )
    })?;
    let source_format_revision = inspect_header(&raw)?;

    let mut payload = match source_format_revision {
        GRAPH_DOCUMENT_FORMAT_REVISION => {
            let envelope =
                serde_json::from_value::<CurrentEnvelopeWire>(raw).map_err(|source| {
                    document_error(
                        ErrorCategory::CorruptData,
                        Recoverability::UserCorrectable,
                        "decode_document",
                        "current graph document does not match its strict schema",
                        [("source", source.to_string())],
                    )
                })?;
            validate_current_envelope(&envelope)?;
            envelope.payload
        }
        LEGACY_GRAPH_DOCUMENT_FORMAT_REVISION => {
            let envelope = serde_json::from_value::<LegacyEnvelopeWire>(raw).map_err(|source| {
                document_error(
                    ErrorCategory::CorruptData,
                    Recoverability::UserCorrectable,
                    "migrate_document",
                    "legacy graph document does not match its strict schema",
                    [("source", source.to_string())],
                )
            })?;
            if envelope.format != GRAPH_DOCUMENT_FORMAT
                || envelope.format_revision != LEGACY_GRAPH_DOCUMENT_FORMAT_REVISION
            {
                return Err(unsupported_format_error(
                    &envelope.format,
                    envelope.format_revision,
                ));
            }
            envelope.graph
        }
        revision => return Err(unsupported_format_error(GRAPH_DOCUMENT_FORMAT, revision)),
    };

    payload.canonicalize();
    let canonical_document = encode_current_document(&payload)?;
    let graph = payload.into_graph()?;
    Ok(GraphLoad {
        graph,
        source_format_revision,
        canonical_document,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentEnvelopeWire {
    format: String,
    format_revision: u32,
    primitive_schema_revision: u32,
    payload_sha256: String,
    payload: GraphPayloadWire,
}

#[derive(Serialize)]
struct CurrentEnvelopeRef<'a> {
    format: &'static str,
    format_revision: u32,
    primitive_schema_revision: u32,
    payload_sha256: String,
    payload: &'a GraphPayloadWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyEnvelopeWire {
    format: String,
    format_revision: u32,
    graph: GraphPayloadWire,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphPayloadWire {
    graph_id: GraphId,
    revision: String,
    schemas: Vec<NodeSchemaWire>,
    nodes: Vec<NodeWire>,
    edges: Vec<EdgeWire>,
    node_order: Vec<NodeId>,
}

impl GraphPayloadWire {
    fn from_snapshot<T>(snapshot: &GraphSnapshot<T>) -> Result<Self>
    where
        T: Serialize,
    {
        let mut schemas = BTreeMap::<NodeSchemaId, NodeSchemaWire>::new();
        let mut nodes = Vec::with_capacity(snapshot.dag().node_count());
        for (node_id, node) in snapshot.dag().nodes() {
            let schema = NodeSchemaWire::from_schema(node.schema());
            match schemas.get(node.schema().id()) {
                Some(existing) if existing != &schema => {
                    return Err(document_error(
                        ErrorCategory::Conflict,
                        Recoverability::UserCorrectable,
                        "encode_document",
                        "one graph schema identity has conflicting definitions",
                        [("schema_id", node.schema().id().to_string())],
                    ));
                }
                Some(_) => {}
                None => {
                    schemas.insert(node.schema().id().clone(), schema);
                }
            }
            nodes.push(NodeWire::from_node(*node_id, node)?);
        }

        Ok(Self {
            graph_id: snapshot.graph_id(),
            revision: snapshot.revision().to_string(),
            schemas: schemas.into_values().collect(),
            nodes,
            edges: snapshot
                .dag()
                .edges()
                .values()
                .copied()
                .map(EdgeWire::from)
                .collect(),
            node_order: snapshot.node_order().to_vec(),
        })
    }

    fn canonicalize(&mut self) {
        self.schemas.sort_by(|left, right| {
            left.id
                .node_type
                .cmp(&right.id.node_type)
                .then_with(|| {
                    left.id
                        .schema_version
                        .precedence_cmp(&right.id.schema_version)
                })
                .then_with(|| {
                    left.id
                        .schema_version
                        .to_string()
                        .cmp(&right.id.schema_version.to_string())
                })
        });
        for schema in &mut self.schemas {
            schema
                .inputs
                .sort_by(|left, right| left.name.cmp(&right.name));
            schema
                .outputs
                .sort_by(|left, right| left.name.cmp(&right.name));
            schema
                .parameters
                .sort_by(|left, right| left.name.cmp(&right.name));
            schema.required_capabilities.sort();
        }
        self.nodes.sort_by_key(|node| node.id);
        for node in &mut self.nodes {
            node.inputs.sort_by_key(|binding| binding.id);
            node.outputs.sort_by_key(|binding| binding.id);
            node.parameters.sort_by_key(|parameter| parameter.id);
            for parameter in &mut node.parameters {
                parameter.payload =
                    canonicalize_value(std::mem::replace(&mut parameter.payload, Value::Null));
            }
        }
        self.edges.sort_by_key(|edge| edge.id);
    }

    fn into_graph<T>(self) -> Result<EditableGraph<T>>
    where
        T: Clone + DeserializeOwned,
    {
        let revision = parse_revision(&self.revision)?;
        let mut schemas = BTreeMap::<NodeSchemaId, Arc<NodeSchema>>::new();
        for wire in self.schemas {
            let schema = wire.into_schema()?;
            let id = schema.id().clone();
            if schemas.insert(id.clone(), Arc::new(schema)).is_some() {
                return Err(document_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "validate_document",
                    "graph document contains a duplicate schema identity",
                    [("schema_id", id.to_string())],
                ));
            }
        }

        let mut nodes = BTreeMap::<NodeId, EditableNode<T>>::new();
        for wire in self.nodes {
            let node_id = wire.id;
            let node = wire.into_node(&schemas)?;
            if nodes.insert(node_id, node).is_some() {
                return Err(document_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "validate_document",
                    "graph document contains a duplicate node identity",
                    [("node_id", node_id.to_string())],
                ));
            }
        }

        validate_node_order(&self.node_order, &nodes)?;
        let mut mutations = Vec::with_capacity(nodes.len() + self.edges.len());
        for (position, node_id) in self.node_order.into_iter().enumerate() {
            let node = nodes
                .remove(&node_id)
                .expect("validated presentation identity owns one node");
            mutations.push(GraphMutation::Add {
                node_id,
                node,
                position,
            });
        }

        let mut edges = BTreeMap::<EdgeId, GraphEdge>::new();
        for wire in self.edges {
            let edge = GraphEdge::from(wire);
            if edges.insert(edge.id(), edge).is_some() {
                return Err(document_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "validate_document",
                    "graph document contains a duplicate edge identity",
                    [("edge_id", edge.id().to_string())],
                ));
            }
        }
        mutations.extend(
            edges
                .into_values()
                .map(|edge| GraphMutation::Connect { edge }),
        );

        let mut graph = EditableGraph::new(self.graph_id);
        if !mutations.is_empty() {
            graph.apply(GraphTransaction::with_mutations(0, mutations))?;
        }
        graph.restore_document_revision(revision)?;
        Ok(graph)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSchemaIdWire {
    node_type: String,
    schema_version: SemanticVersion,
}

impl NodeSchemaIdWire {
    fn from_id(id: &NodeSchemaId) -> Self {
        Self {
            node_type: id.node_type().as_str().to_owned(),
            schema_version: id.schema_version().clone(),
        }
    }

    fn into_id(self) -> Result<NodeSchemaId> {
        let node_type = NodeTypeId::from_str(&self.node_type).map_err(|source| {
            invalid_field_error("node_type", &self.node_type, source.to_string())
        })?;
        Ok(NodeSchemaId::new(node_type, self.schema_version))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSchemaWire {
    id: NodeSchemaIdWire,
    inputs: Vec<PortSchemaWire>,
    outputs: Vec<PortSchemaWire>,
    parameters: Vec<ParameterSchemaWire>,
    behavior: NodeBehaviorWire,
    required_capabilities: Vec<CapabilityId>,
}

impl NodeSchemaWire {
    fn from_schema(schema: &NodeSchema) -> Self {
        Self {
            id: NodeSchemaIdWire::from_id(schema.id()),
            inputs: schema.inputs().map(PortSchemaWire::from).collect(),
            outputs: schema.outputs().map(PortSchemaWire::from).collect(),
            parameters: schema.parameters().map(ParameterSchemaWire::from).collect(),
            behavior: NodeBehaviorWire::from(schema.behavior()),
            required_capabilities: schema.required_capabilities().iter().cloned().collect(),
        }
    }

    fn into_schema(self) -> Result<NodeSchema> {
        NodeSchema::new(
            self.id.into_id()?,
            self.inputs
                .into_iter()
                .map(PortSchemaWire::into_schema)
                .collect::<Result<Vec<_>>>()?,
            self.outputs
                .into_iter()
                .map(PortSchemaWire::into_schema)
                .collect::<Result<Vec<_>>>()?,
            self.parameters
                .into_iter()
                .map(ParameterSchemaWire::into_schema)
                .collect::<Result<Vec<_>>>()?,
            self.behavior.into_behavior(),
            CapabilitySet::new(self.required_capabilities),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortSchemaWire {
    name: String,
    value_type: String,
    cardinality: PortCardinalityWire,
}

impl From<&PortSchema> for PortSchemaWire {
    fn from(schema: &PortSchema) -> Self {
        Self {
            name: schema.name().as_str().to_owned(),
            value_type: schema.value_type().as_str().to_owned(),
            cardinality: PortCardinalityWire::from(schema.cardinality()),
        }
    }
}

impl PortSchemaWire {
    fn into_schema(self) -> Result<PortSchema> {
        Ok(PortSchema::new(
            parse_port_name(&self.name)?,
            parse_value_type(&self.value_type)?,
            self.cardinality.into_cardinality(),
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterSchemaWire {
    name: String,
    value_type: String,
    animatable: bool,
}

impl From<&ParameterSchema> for ParameterSchemaWire {
    fn from(schema: &ParameterSchema) -> Self {
        Self {
            name: schema.name().as_str().to_owned(),
            value_type: schema.value_type().as_str().to_owned(),
            animatable: schema.is_animatable(),
        }
    }
}

impl ParameterSchemaWire {
    fn into_schema(self) -> Result<ParameterSchema> {
        Ok(ParameterSchema::new(
            parse_parameter_name(&self.name)?,
            parse_value_type(&self.value_type)?,
            self.animatable,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PortCardinalityWire {
    Single,
    Optional,
    Variadic,
}

impl From<PortCardinality> for PortCardinalityWire {
    fn from(value: PortCardinality) -> Self {
        match value {
            PortCardinality::Single => Self::Single,
            PortCardinality::Optional => Self::Optional,
            PortCardinality::Variadic => Self::Variadic,
        }
    }
}

impl PortCardinalityWire {
    const fn into_cardinality(self) -> PortCardinality {
        match self {
            Self::Single => PortCardinality::Single,
            Self::Optional => PortCardinality::Optional,
            Self::Variadic => PortCardinality::Variadic,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeBehaviorWire {
    time: TimeBehaviorWire,
    roi: RoiBehaviorWire,
    color: ColorRequirementsWire,
    determinism: DeterminismWire,
    cache_policy: CachePolicyWire,
}

impl From<NodeBehavior> for NodeBehaviorWire {
    fn from(behavior: NodeBehavior) -> Self {
        Self {
            time: TimeBehaviorWire::from(behavior.time()),
            roi: RoiBehaviorWire::from(behavior.roi()),
            color: ColorRequirementsWire::from(behavior.color()),
            determinism: DeterminismWire::from(behavior.determinism()),
            cache_policy: CachePolicyWire::from(behavior.cache_policy()),
        }
    }
}

impl NodeBehaviorWire {
    const fn into_behavior(self) -> NodeBehavior {
        NodeBehavior::new(
            self.time.into_behavior(),
            self.roi.into_behavior(),
            self.color.into_requirements(),
            self.determinism.into_determinism(),
            self.cache_policy.into_policy(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TimeBehaviorWire {
    Invariant,
    CurrentFrame,
    FrameWindow {
        frames_before: u32,
        frames_after: u32,
    },
    Unbounded,
}

impl From<TimeBehavior> for TimeBehaviorWire {
    fn from(value: TimeBehavior) -> Self {
        match value {
            TimeBehavior::Invariant => Self::Invariant,
            TimeBehavior::CurrentFrame => Self::CurrentFrame,
            TimeBehavior::FrameWindow {
                frames_before,
                frames_after,
            } => Self::FrameWindow {
                frames_before,
                frames_after,
            },
            TimeBehavior::Unbounded => Self::Unbounded,
        }
    }
}

impl TimeBehaviorWire {
    const fn into_behavior(self) -> TimeBehavior {
        match self {
            Self::Invariant => TimeBehavior::Invariant,
            Self::CurrentFrame => TimeBehavior::CurrentFrame,
            Self::FrameWindow {
                frames_before,
                frames_after,
            } => TimeBehavior::FrameWindow {
                frames_before,
                frames_after,
            },
            Self::Unbounded => TimeBehavior::Unbounded,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RoiBehaviorWire {
    FullFrame,
    InputBounds,
    Expanded { pixels: u32 },
    Custom,
}

impl From<RoiBehavior> for RoiBehaviorWire {
    fn from(value: RoiBehavior) -> Self {
        match value {
            RoiBehavior::FullFrame => Self::FullFrame,
            RoiBehavior::InputBounds => Self::InputBounds,
            RoiBehavior::Expanded { pixels } => Self::Expanded { pixels },
            RoiBehavior::Custom => Self::Custom,
        }
    }
}

impl RoiBehaviorWire {
    const fn into_behavior(self) -> RoiBehavior {
        match self {
            Self::FullFrame => RoiBehavior::FullFrame,
            Self::InputBounds => RoiBehavior::InputBounds,
            Self::Expanded { pixels } => RoiBehavior::Expanded { pixels },
            Self::Custom => RoiBehavior::Custom,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ColorRequirementsWire {
    NotApplicable,
    Tagged,
    Exact { color_space: ColorSpace },
}

impl From<ColorRequirements> for ColorRequirementsWire {
    fn from(value: ColorRequirements) -> Self {
        match value {
            ColorRequirements::NotApplicable => Self::NotApplicable,
            ColorRequirements::Tagged => Self::Tagged,
            ColorRequirements::Exact(color_space) => Self::Exact { color_space },
        }
    }
}

impl ColorRequirementsWire {
    const fn into_requirements(self) -> ColorRequirements {
        match self {
            Self::NotApplicable => ColorRequirements::NotApplicable,
            Self::Tagged => ColorRequirements::Tagged,
            Self::Exact { color_space } => ColorRequirements::Exact(color_space),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DeterminismWire {
    Deterministic,
    Seeded,
    NonDeterministic,
}

impl From<Determinism> for DeterminismWire {
    fn from(value: Determinism) -> Self {
        match value {
            Determinism::Deterministic => Self::Deterministic,
            Determinism::Seeded => Self::Seeded,
            Determinism::NonDeterministic => Self::NonDeterministic,
        }
    }
}

impl DeterminismWire {
    const fn into_determinism(self) -> Determinism {
        match self {
            Self::Deterministic => Determinism::Deterministic,
            Self::Seeded => Determinism::Seeded,
            Self::NonDeterministic => Determinism::NonDeterministic,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CachePolicyWire {
    Disabled,
    Static,
    PerFrame,
    PerRegion,
}

impl From<CachePolicy> for CachePolicyWire {
    fn from(value: CachePolicy) -> Self {
        match value {
            CachePolicy::Disabled => Self::Disabled,
            CachePolicy::Static => Self::Static,
            CachePolicy::PerFrame => Self::PerFrame,
            CachePolicy::PerRegion => Self::PerRegion,
        }
    }
}

impl CachePolicyWire {
    const fn into_policy(self) -> CachePolicy {
        match self {
            Self::Disabled => CachePolicy::Disabled,
            Self::Static => CachePolicy::Static,
            Self::PerFrame => CachePolicy::PerFrame,
            Self::PerRegion => CachePolicy::PerRegion,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeWire {
    id: NodeId,
    schema: NodeSchemaIdWire,
    inputs: Vec<PortBindingWire>,
    outputs: Vec<PortBindingWire>,
    parameters: Vec<ParameterWire>,
}

impl NodeWire {
    fn from_node<T>(id: NodeId, node: &EditableNode<T>) -> Result<Self>
    where
        T: Serialize,
    {
        let mut parameters = Vec::with_capacity(node.parameters().len());
        for parameter in node.parameters().values() {
            let payload = serde_json::to_value(parameter.value().payload()).map_err(|source| {
                document_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "encode_parameter",
                    "graph parameter payload cannot be represented as JSON",
                    [
                        ("parameter_id", parameter.id().to_string()),
                        ("source", source.to_string()),
                    ],
                )
            })?;
            parameters.push(ParameterWire {
                id: parameter.id(),
                name: parameter.name().as_str().to_owned(),
                value_type: parameter.value().value_type().as_str().to_owned(),
                payload: canonicalize_value(payload),
            });
        }

        Ok(Self {
            id,
            schema: NodeSchemaIdWire::from_id(node.schema().id()),
            inputs: node
                .inputs()
                .iter()
                .map(|(id, name)| PortBindingWire {
                    id: *id,
                    name: name.as_str().to_owned(),
                })
                .collect(),
            outputs: node
                .outputs()
                .iter()
                .map(|(id, name)| PortBindingWire {
                    id: *id,
                    name: name.as_str().to_owned(),
                })
                .collect(),
            parameters,
        })
    }

    fn into_node<T>(
        self,
        schemas: &BTreeMap<NodeSchemaId, Arc<NodeSchema>>,
    ) -> Result<EditableNode<T>>
    where
        T: DeserializeOwned,
    {
        let schema_id = self.schema.into_id()?;
        let schema = schemas.get(&schema_id).cloned().ok_or_else(|| {
            document_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "validate_document",
                "graph node references a schema absent from the document",
                [
                    ("node_id", self.id.to_string()),
                    ("schema_id", schema_id.to_string()),
                ],
            )
        })?;
        let inputs = self
            .inputs
            .into_iter()
            .map(PortBindingWire::into_instance)
            .collect::<Result<Vec<_>>>()?;
        let outputs = self
            .outputs
            .into_iter()
            .map(PortBindingWire::into_instance)
            .collect::<Result<Vec<_>>>()?;
        let parameters = self
            .parameters
            .into_iter()
            .map(ParameterWire::into_parameter)
            .collect::<Result<Vec<_>>>()?;
        EditableNode::new(schema, inputs, outputs, parameters)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortBindingWire {
    id: PortId,
    name: String,
}

impl PortBindingWire {
    fn into_instance(self) -> Result<InstancePort> {
        Ok(InstancePort::new(self.id, parse_port_name(&self.name)?))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterWire {
    id: ParameterId,
    name: String,
    value_type: String,
    payload: Value,
}

impl ParameterWire {
    fn into_parameter<T>(self) -> Result<EditableParameter<T>>
    where
        T: DeserializeOwned,
    {
        let payload = serde_json::from_value(self.payload).map_err(|source| {
            document_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "decode_parameter",
                "graph parameter payload does not match the requested consumer type",
                [
                    ("parameter_id", self.id.to_string()),
                    ("source", source.to_string()),
                ],
            )
        })?;
        Ok(EditableParameter::new(
            self.id,
            parse_parameter_name(&self.name)?,
            TypedParameterValue::new(parse_value_type(&self.value_type)?, payload),
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointWire {
    node_id: NodeId,
    port_id: PortId,
}

impl From<GraphEndpoint> for EndpointWire {
    fn from(endpoint: GraphEndpoint) -> Self {
        Self {
            node_id: endpoint.node_id(),
            port_id: endpoint.port_id(),
        }
    }
}

impl From<EndpointWire> for GraphEndpoint {
    fn from(endpoint: EndpointWire) -> Self {
        Self::new(endpoint.node_id, endpoint.port_id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeWire {
    id: EdgeId,
    source: EndpointWire,
    destination: EndpointWire,
}

impl From<GraphEdge> for EdgeWire {
    fn from(edge: GraphEdge) -> Self {
        Self {
            id: edge.id(),
            source: EndpointWire::from(edge.source()),
            destination: EndpointWire::from(edge.destination()),
        }
    }
}

impl From<EdgeWire> for GraphEdge {
    fn from(edge: EdgeWire) -> Self {
        Self::new(edge.id, edge.source.into(), edge.destination.into())
    }
}

fn inspect_header(raw: &Value) -> Result<u32> {
    let object = raw.as_object().ok_or_else(|| {
        document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "decode_document",
            "graph document root must be an object",
            [],
        )
    })?;
    let format = object
        .get("format")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            document_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "decode_document",
                "graph document is missing its string format tag",
                [],
            )
        })?;
    let revision = object
        .get("format_revision")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            document_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "decode_document",
                "graph document is missing its unsigned format revision",
                [],
            )
        })?;
    let revision =
        u32::try_from(revision).map_err(|_| unsupported_format_error(format, u32::MAX))?;
    if format != GRAPH_DOCUMENT_FORMAT {
        return Err(unsupported_format_error(format, revision));
    }
    Ok(revision)
}

fn validate_current_envelope(envelope: &CurrentEnvelopeWire) -> Result<()> {
    if envelope.format != GRAPH_DOCUMENT_FORMAT
        || envelope.format_revision != GRAPH_DOCUMENT_FORMAT_REVISION
    {
        return Err(unsupported_format_error(
            &envelope.format,
            envelope.format_revision,
        ));
    }
    if envelope.primitive_schema_revision != STABLE_PRIMITIVE_SCHEMA_REVISION {
        return Err(document_error(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "decode_document",
            "graph document uses an unsupported shared primitive schema revision",
            [
                (
                    "document_revision",
                    envelope.primitive_schema_revision.to_string(),
                ),
                (
                    "supported_revision",
                    STABLE_PRIMITIVE_SCHEMA_REVISION.to_string(),
                ),
            ],
        ));
    }
    if !is_lower_sha256(&envelope.payload_sha256) {
        return Err(document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "verify_integrity",
            "graph document payload checksum is not canonical SHA-256",
            [("payload_sha256", envelope.payload_sha256.clone())],
        ));
    }
    let actual = payload_sha256(&envelope.payload)?;
    if actual != envelope.payload_sha256 {
        return Err(document_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "verify_integrity",
            "graph document payload checksum does not match its editable state",
            [
                ("expected_sha256", envelope.payload_sha256.clone()),
                ("actual_sha256", actual),
            ],
        ));
    }
    Ok(())
}

fn encode_current_document(payload: &GraphPayloadWire) -> Result<Vec<u8>> {
    let payload_sha256 = payload_sha256(payload)?;
    serde_json::to_vec(&CurrentEnvelopeRef {
        format: GRAPH_DOCUMENT_FORMAT,
        format_revision: GRAPH_DOCUMENT_FORMAT_REVISION,
        primitive_schema_revision: STABLE_PRIMITIVE_SCHEMA_REVISION,
        payload_sha256,
        payload,
    })
    .map_err(|source| {
        document_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "encode_document",
            "validated graph document could not be encoded",
            [("source", source.to_string())],
        )
    })
}

fn payload_sha256(payload: &GraphPayloadWire) -> Result<String> {
    let value = serde_json::to_value(payload)
        .map(canonicalize_value)
        .map_err(|source| {
            document_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "hash_payload",
                "graph payload could not be encoded for integrity verification",
                [("source", source.to_string())],
            )
        })?;
    let bytes = serde_json::to_vec(&value).map_err(|source| {
        document_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "hash_payload",
            "canonical graph payload could not be encoded for integrity verification",
            [("source", source.to_string())],
        )
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn canonicalize_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_value).collect()),
        Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize_value(value)))
                    .collect(),
            )
        }
        value => value,
    }
}

fn validate_node_order<T>(
    order: &[NodeId],
    nodes: &BTreeMap<NodeId, EditableNode<T>>,
) -> Result<()> {
    if order.len() != nodes.len() {
        return Err(document_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "validate_document",
            "graph presentation order must contain every node exactly once",
            [
                ("order_count", order.len().to_string()),
                ("node_count", nodes.len().to_string()),
            ],
        ));
    }
    let unique = order.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != order.len() {
        return Err(document_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "validate_document",
            "graph presentation order contains a duplicate node identity",
            [],
        ));
    }
    if let Some(missing) = unique.iter().find(|node_id| !nodes.contains_key(node_id)) {
        return Err(document_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "validate_document",
            "graph presentation order references a node absent from the document",
            [("node_id", missing.to_string())],
        ));
    }
    Ok(())
}

fn parse_revision(value: &str) -> Result<u64> {
    let canonical = value == "0"
        || (!value.is_empty()
            && !value.starts_with('0')
            && value.bytes().all(|byte| byte.is_ascii_digit()));
    if !canonical {
        return Err(invalid_field_error(
            "revision",
            value,
            "expected a canonical unsigned decimal integer string",
        ));
    }
    value.parse().map_err(|source: std::num::ParseIntError| {
        invalid_field_error("revision", value, source.to_string())
    })
}

fn parse_port_name(value: &str) -> Result<PortName> {
    PortName::from_str(value)
        .map_err(|source| invalid_field_error("port_name", value, source.to_string()))
}

fn parse_parameter_name(value: &str) -> Result<ParameterName> {
    ParameterName::from_str(value)
        .map_err(|source| invalid_field_error("parameter_name", value, source.to_string()))
}

fn parse_value_type(value: &str) -> Result<ValueTypeId> {
    ValueTypeId::from_str(value)
        .map_err(|source| invalid_field_error("value_type", value, source.to_string()))
}

fn invalid_field_error(field: &'static str, value: &str, source: impl Into<String>) -> Error {
    document_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "validate_document",
        "graph document contains a noncanonical field value",
        [
            ("field", field.to_owned()),
            ("value", value.to_owned()),
            ("source", source.into()),
        ],
    )
}

fn unsupported_format_error(format: &str, revision: u32) -> Error {
    document_error(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        "decode_document",
        "graph document format or revision is unsupported",
        [
            ("format", format.to_owned()),
            ("format_revision", revision.to_string()),
            (
                "supported_revision",
                GRAPH_DOCUMENT_FORMAT_REVISION.to_string(),
            ),
        ],
    )
}

fn document_error<const N: usize>(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
    fields: [(&'static str, String); N],
) -> Error {
    let mut context = ErrorContext::new(COMPONENT, operation);
    for (field, value) in fields {
        context.insert_field(field, value);
    }
    Error::new(category, recoverability, message).with_context(context)
}
