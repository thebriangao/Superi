//! Deterministic compilation from editorial timelines into editable graph state.
//!
//! Compilation keeps stable editorial identities separate from mutable names,
//! timing, ordering, and parameter values. A selected timeline and every nested
//! timeline reachable from it become one typed graph transaction with an
//! inspectable bidirectional provenance index.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ProjectId, TimelineId, TrackId};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{Duration, RationalTime, TimeRange, Timebase};
use superi_graph::dag::{GraphEdge, GraphEndpoint};
use superi_graph::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphSnapshot, GraphTransaction,
    InstancePort, TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};

use crate::model::{
    ClipSource, EditorialObjectId, EditorialProject, Timeline, Track, TrackItem, TrackKind,
    TrackSemantics,
};
use crate::multicam::{MulticamClip, MulticamSource};
use crate::retime::ClipTimeMap;

const COMPONENT: &str = "superi-timeline.compile";
const HASH_NAMESPACE: &[u8] = b"superi.timeline.compile.v1";

/// One typed editable value retained by a compiled timeline graph.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TimelineGraphValue {
    /// Stable project identity.
    ProjectId(ProjectId),
    /// Stable timeline identity.
    TimelineId(TimelineId),
    /// Stable track identity.
    TrackId(TrackId),
    /// Stable editorial object identity.
    EditorialObjectId(EditorialObjectId),
    /// Required editor-facing or semantic text.
    Text(String),
    /// Optional editor-facing or semantic text.
    OptionalText(Option<String>),
    /// Exact timeline or track timebase.
    Timebase(Timebase),
    /// Exact timeline coordinate.
    RationalTime(RationalTime),
    /// Exact half-open source or record range.
    TimeRange(TimeRange),
    /// Exact transition overlap duration.
    Duration(Duration),
    /// Media or nested-timeline source relationship.
    ClipSource(ClipSource),
    /// Complete clip-local record-to-source mapping.
    ClipTimeMap(ClipTimeMap),
    /// Optional synchronized angle catalog and authored synchronization provenance.
    OptionalMulticamSource(Option<MulticamSource>),
    /// Optional clip-local multicam switching and audio intent.
    OptionalMulticamClip(Option<MulticamClip>),
    /// Complete typed track behavior.
    TrackSemantics(TrackSemantics),
    /// Bottom-to-top authored track order retained independently of edge order.
    TrackOrder(Vec<TrackId>),
    /// Authored object order retained independently of graph adjacency order.
    ObjectOrder(Vec<EditorialObjectId>),
    /// Deterministically ordered generator parameters.
    StringMap(BTreeMap<String, String>),
}

impl TimelineGraphValue {
    fn value_type_code(&self) -> &'static str {
        match self {
            Self::ProjectId(_) => "superi.value.timeline.project-id",
            Self::TimelineId(_) => "superi.value.timeline.timeline-id",
            Self::TrackId(_) => "superi.value.timeline.track-id",
            Self::EditorialObjectId(_) => "superi.value.timeline.object-id",
            Self::Text(_) => "superi.value.timeline.text",
            Self::OptionalText(_) => "superi.value.timeline.optional-text",
            Self::Timebase(_) => "superi.value.timeline.timebase",
            Self::RationalTime(_) => "superi.value.timeline.time",
            Self::TimeRange(_) => "superi.value.timeline.range",
            Self::Duration(_) => "superi.value.timeline.duration",
            Self::ClipSource(_) => "superi.value.timeline.clip-source",
            Self::ClipTimeMap(_) => "superi.value.timeline.clip-time-map",
            Self::OptionalMulticamSource(_) => "superi.value.timeline.optional-multicam-source",
            Self::OptionalMulticamClip(_) => "superi.value.timeline.optional-multicam-clip",
            Self::TrackSemantics(_) => "superi.value.timeline.track-semantics",
            Self::TrackOrder(_) => "superi.value.timeline.track-order",
            Self::ObjectOrder(_) => "superi.value.timeline.object-order",
            Self::StringMap(_) => "superi.value.timeline.string-map",
        }
    }
}

/// The editorial owner represented by one compiled node.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum TimelineGraphOrigin {
    /// A timeline output node.
    Timeline(TimelineId),
    /// An ordered track node.
    Track(TrackId),
    /// A clip, gap, transition, generator, or caption node.
    Object(EditorialObjectId),
}

/// Bidirectional stable addressing between editorial state and compiled nodes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TimelineGraphIndex {
    timelines: BTreeMap<TimelineId, NodeId>,
    tracks: BTreeMap<TrackId, NodeId>,
    objects: BTreeMap<EditorialObjectId, NodeId>,
    origins: BTreeMap<NodeId, TimelineGraphOrigin>,
}

impl TimelineGraphIndex {
    /// Returns the output node for one reachable timeline.
    #[must_use]
    pub fn timeline_output(&self, timeline_id: TimelineId) -> Option<NodeId> {
        self.timelines.get(&timeline_id).copied()
    }

    /// Returns the node for one reachable track.
    #[must_use]
    pub fn track_node(&self, track_id: TrackId) -> Option<NodeId> {
        self.tracks.get(&track_id).copied()
    }

    /// Returns the node for one reachable editorial object.
    #[must_use]
    pub fn object_node(&self, object_id: EditorialObjectId) -> Option<NodeId> {
        self.objects.get(&object_id).copied()
    }

    /// Resolves any editorial origin to its compiled node.
    #[must_use]
    pub fn node(&self, origin: TimelineGraphOrigin) -> Option<NodeId> {
        match origin {
            TimelineGraphOrigin::Timeline(id) => self.timeline_output(id),
            TimelineGraphOrigin::Track(id) => self.track_node(id),
            TimelineGraphOrigin::Object(id) => self.object_node(id),
        }
    }

    /// Resolves a compiled node back to its editorial owner.
    #[must_use]
    pub fn origin(&self, node_id: NodeId) -> Option<TimelineGraphOrigin> {
        self.origins.get(&node_id).copied()
    }

    fn insert(&mut self, origin: TimelineGraphOrigin, node_id: NodeId) -> Result<()> {
        if let Some(existing) = self.origins.get(&node_id) {
            if *existing != origin {
                return Err(collision_error(
                    "derive_node_id",
                    node_id.to_string(),
                    format!("{existing:?}"),
                    format!("{origin:?}"),
                ));
            }
            return Err(internal_error(
                "index_node",
                "one editorial origin was compiled more than once",
            ));
        }
        let replaced = match origin {
            TimelineGraphOrigin::Timeline(id) => self.timelines.insert(id, node_id),
            TimelineGraphOrigin::Track(id) => self.tracks.insert(id, node_id),
            TimelineGraphOrigin::Object(id) => self.objects.insert(id, node_id),
        };
        if replaced.is_some() {
            return Err(internal_error(
                "index_node",
                "one stable editorial identity resolved to multiple nodes",
            ));
        }
        self.origins.insert(node_id, origin);
        Ok(())
    }
}

/// One complete editable graph compiled from a validated editorial project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineGraphCompilation {
    project_id: ProjectId,
    root_timeline_id: TimelineId,
    project_revision: u64,
    graph: EditableGraph<TimelineGraphValue>,
    index: TimelineGraphIndex,
}

impl TimelineGraphCompilation {
    /// Returns the source project identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Returns the selected root timeline identity.
    #[must_use]
    pub const fn root_timeline_id(&self) -> TimelineId {
        self.root_timeline_id
    }

    /// Returns the source project revision captured by this compilation.
    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    /// Returns the editable graph document.
    #[must_use]
    pub const fn graph(&self) -> &EditableGraph<TimelineGraphValue> {
        &self.graph
    }

    /// Returns mutable access for ordinary checked graph transactions.
    pub fn graph_mut(&mut self) -> &mut EditableGraph<TimelineGraphValue> {
        &mut self.graph
    }

    /// Captures an immutable graph snapshot for inspection or evaluation.
    #[must_use]
    pub fn snapshot(&self) -> GraphSnapshot<TimelineGraphValue> {
        self.graph.snapshot()
    }

    /// Returns stable editorial-to-graph provenance.
    #[must_use]
    pub const fn index(&self) -> &TimelineGraphIndex {
        &self.index
    }

    /// Consumes the compilation and returns its editable graph document.
    #[must_use]
    pub fn into_graph(self) -> EditableGraph<TimelineGraphValue> {
        self.graph
    }
}

/// Compiles one selected timeline and every reachable nested timeline.
///
/// Node, port, parameter, edge, and graph identities depend only on stable
/// editorial identities and semantic roles. Mutable values remain ordinary
/// typed parameters, so recompilation preserves addresses while reflecting
/// names, timings, ordering, sources, and processing intent.
pub fn compile_timeline(
    project: &EditorialProject,
    root_timeline_id: TimelineId,
) -> Result<TimelineGraphCompilation> {
    if project.timeline(root_timeline_id).is_none() {
        return Err(Error::new(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "root timeline was not found in the editorial project",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "compile_timeline")
                .with_field("project_id", project.id().to_string())
                .with_field("timeline_id", root_timeline_id.to_string()),
        ));
    }

    let mut reachable = Vec::new();
    collect_reachable_timelines(
        project,
        root_timeline_id,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut reachable,
    )?;

    let mut compiler = Compiler::new(project, root_timeline_id);
    for timeline_id in &reachable {
        let timeline = project.timeline(*timeline_id).ok_or_else(|| {
            internal_error(
                "compile_timeline",
                "validated nested timeline disappeared during compilation",
            )
        })?;
        compiler.add_timeline_nodes(timeline)?;
    }
    for timeline_id in &reachable {
        let timeline = project.timeline(*timeline_id).ok_or_else(|| {
            internal_error(
                "compile_timeline",
                "validated nested timeline disappeared during connection planning",
            )
        })?;
        compiler.add_timeline_edges(timeline)?;
    }
    compiler.finish()
}

fn collect_reachable_timelines(
    project: &EditorialProject,
    timeline_id: TimelineId,
    visiting: &mut BTreeSet<TimelineId>,
    visited: &mut BTreeSet<TimelineId>,
    output: &mut Vec<TimelineId>,
) -> Result<()> {
    if visited.contains(&timeline_id) {
        return Ok(());
    }
    if !visiting.insert(timeline_id) {
        return Err(Error::new(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "nested timeline cycle reached the graph compiler",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "collect_reachable_timelines")
                .with_field("timeline_id", timeline_id.to_string()),
        ));
    }
    let timeline = project.timeline(timeline_id).ok_or_else(|| {
        Error::new(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "nested clip references a missing timeline",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "collect_reachable_timelines")
                .with_field("timeline_id", timeline_id.to_string()),
        )
    })?;
    output.push(timeline_id);
    for track in timeline.tracks() {
        for item in track.items() {
            if let TrackItem::Clip(clip) = item {
                if let ClipSource::Timeline(child_id) = clip.source() {
                    collect_reachable_timelines(project, child_id, visiting, visited, output)?;
                }
            }
        }
    }
    visiting.remove(&timeline_id);
    visited.insert(timeline_id);
    Ok(())
}

struct Compiler<'a> {
    project: &'a EditorialProject,
    root_timeline_id: TimelineId,
    node_mutations: Vec<GraphMutation<TimelineGraphValue>>,
    edge_mutations: Vec<GraphMutation<TimelineGraphValue>>,
    index: TimelineGraphIndex,
}

impl<'a> Compiler<'a> {
    fn new(project: &'a EditorialProject, root_timeline_id: TimelineId) -> Self {
        Self {
            project,
            root_timeline_id,
            node_mutations: Vec::new(),
            edge_mutations: Vec::new(),
            index: TimelineGraphIndex::default(),
        }
    }

    fn add_timeline_nodes(&mut self, timeline: &Timeline) -> Result<()> {
        let output_id = timeline_node_id(self.project.id(), timeline.id());
        self.index
            .insert(TimelineGraphOrigin::Timeline(timeline.id()), output_id)?;
        self.add_node(
            output_id,
            timeline_output_node(self.project, timeline, output_id)?,
        );

        for track in timeline.tracks() {
            let track_id = track_node_id(self.project.id(), track.id());
            self.index
                .insert(TimelineGraphOrigin::Track(track.id()), track_id)?;
            self.add_node(track_id, track_node(track, track_id)?);
            for item in track.items() {
                let origin = TimelineGraphOrigin::Object(item.id());
                let item_node_id = object_node_id(self.project.id(), item.id());
                self.index.insert(origin, item_node_id)?;
                self.add_node(
                    item_node_id,
                    item_node(timeline, track.kind(), item, item_node_id)?,
                );
            }
        }
        Ok(())
    }

    fn add_timeline_edges(&mut self, timeline: &Timeline) -> Result<()> {
        let timeline_output = self.required_node(TimelineGraphOrigin::Timeline(timeline.id()))?;
        for track in timeline.tracks() {
            let track_node = self.required_node(TimelineGraphOrigin::Track(track.id()))?;
            self.connect(
                track_node,
                output_port_id(track_node, "content"),
                timeline_output,
                input_port_id(timeline_output, kind_code(track.kind())),
            );

            for item in track.items() {
                let item_node = self.required_node(TimelineGraphOrigin::Object(item.id()))?;
                self.connect(
                    item_node,
                    output_port_id(item_node, "content"),
                    track_node,
                    input_port_id(track_node, "items"),
                );

                match item {
                    TrackItem::Clip(clip) => {
                        if let ClipSource::Timeline(child_id) = clip.source() {
                            let child =
                                self.required_node(TimelineGraphOrigin::Timeline(child_id))?;
                            self.connect(
                                child,
                                output_port_id(child, kind_code(track.kind())),
                                item_node,
                                input_port_id(item_node, "nested"),
                            );
                        }
                    }
                    TrackItem::Transition(transition) => {
                        let from =
                            self.required_node(TimelineGraphOrigin::Object(transition.from()))?;
                        let to =
                            self.required_node(TimelineGraphOrigin::Object(transition.to()))?;
                        self.connect(
                            from,
                            output_port_id(from, "content"),
                            item_node,
                            input_port_id(item_node, "from"),
                        );
                        self.connect(
                            to,
                            output_port_id(to, "content"),
                            item_node,
                            input_port_id(item_node, "to"),
                        );
                    }
                    TrackItem::Gap(_) | TrackItem::Generator(_) | TrackItem::Caption(_) => {}
                }
            }
        }
        Ok(())
    }

    fn add_node(&mut self, node_id: NodeId, node: EditableNode<TimelineGraphValue>) {
        let position = self.node_mutations.len();
        self.node_mutations.push(GraphMutation::Add {
            node_id,
            node,
            position,
        });
    }

    fn connect(
        &mut self,
        source_node: NodeId,
        source_port: PortId,
        destination_node: NodeId,
        destination_port: PortId,
    ) {
        let edge = GraphEdge::new(
            edge_id(source_node, source_port, destination_node, destination_port),
            GraphEndpoint::new(source_node, source_port),
            GraphEndpoint::new(destination_node, destination_port),
        );
        self.edge_mutations.push(GraphMutation::Connect { edge });
    }

    fn required_node(&self, origin: TimelineGraphOrigin) -> Result<NodeId> {
        self.index.node(origin).ok_or_else(|| {
            internal_error(
                "connect_graph",
                "compiled provenance did not contain a required editorial object",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "connect_graph")
                    .with_field("origin", format!("{origin:?}")),
            )
        })
    }

    fn finish(mut self) -> Result<TimelineGraphCompilation> {
        self.node_mutations.append(&mut self.edge_mutations);
        let mut graph = EditableGraph::new(graph_id(self.project.id(), self.root_timeline_id));
        graph.apply(GraphTransaction::with_mutations(0, self.node_mutations))?;
        Ok(TimelineGraphCompilation {
            project_id: self.project.id(),
            root_timeline_id: self.root_timeline_id,
            project_revision: self.project.revision(),
            graph,
            index: self.index,
        })
    }
}

fn timeline_output_node(
    project: &EditorialProject,
    timeline: &Timeline,
    node_id: NodeId,
) -> Result<EditableNode<TimelineGraphValue>> {
    let ports = all_stream_ports(PortCardinality::Variadic);
    let outputs = all_stream_ports(PortCardinality::Single);
    editable_node(
        node_id,
        "superi.timeline.output",
        &ports,
        &outputs,
        vec![
            parameter("project-id", TimelineGraphValue::ProjectId(project.id())),
            parameter("timeline-id", TimelineGraphValue::TimelineId(timeline.id())),
            parameter("name", TimelineGraphValue::Text(timeline.name().to_owned())),
            parameter(
                "edit-rate",
                TimelineGraphValue::Timebase(timeline.edit_rate()),
            ),
            parameter(
                "global-start",
                TimelineGraphValue::RationalTime(timeline.global_start()),
            ),
            parameter(
                "track-order",
                TimelineGraphValue::TrackOrder(timeline.tracks().iter().map(Track::id).collect()),
            ),
            parameter(
                "multicam-source",
                TimelineGraphValue::OptionalMulticamSource(timeline.multicam_source().cloned()),
            ),
        ],
        true,
    )
}

fn track_node(track: &Track, node_id: NodeId) -> Result<EditableNode<TimelineGraphValue>> {
    editable_node(
        node_id,
        &format!("superi.timeline.{}.track", kind_code(track.kind())),
        &[PortSpec::new(
            "items",
            track.kind(),
            PortCardinality::Variadic,
        )],
        &[PortSpec::new(
            "content",
            track.kind(),
            PortCardinality::Single,
        )],
        vec![
            parameter("track-id", TimelineGraphValue::TrackId(track.id())),
            parameter("name", TimelineGraphValue::Text(track.name().to_owned())),
            parameter(
                "semantics",
                TimelineGraphValue::TrackSemantics(track.semantics().clone()),
            ),
            parameter(
                "item-order",
                TimelineGraphValue::ObjectOrder(track.items().iter().map(TrackItem::id).collect()),
            ),
        ],
        track.kind() == TrackKind::Video,
    )
}

fn item_node(
    timeline: &Timeline,
    kind: TrackKind,
    item: &TrackItem,
    node_id: NodeId,
) -> Result<EditableNode<TimelineGraphValue>> {
    let output = [PortSpec::new("content", kind, PortCardinality::Single)];
    match item {
        TrackItem::Clip(clip) => editable_node(
            node_id,
            &format!("superi.timeline.{}.clip", kind_code(kind)),
            &[PortSpec::new("nested", kind, PortCardinality::Optional)],
            &output,
            vec![
                parameter(
                    "object-id",
                    TimelineGraphValue::EditorialObjectId(item.id()),
                ),
                parameter("name", TimelineGraphValue::Text(clip.name().to_owned())),
                parameter("source", TimelineGraphValue::ClipSource(clip.source())),
                parameter(
                    "source-range",
                    TimelineGraphValue::TimeRange(clip.source_range()),
                ),
                parameter(
                    "record-range",
                    TimelineGraphValue::TimeRange(clip.record_range()),
                ),
                parameter(
                    "time-map",
                    TimelineGraphValue::ClipTimeMap(clip.time_map().clone()),
                ),
                parameter(
                    "multicam-clip",
                    TimelineGraphValue::OptionalMulticamClip(
                        timeline.multicam_clip(clip.id()).cloned(),
                    ),
                ),
            ],
            kind == TrackKind::Video,
        ),
        TrackItem::Gap(gap) => editable_node(
            node_id,
            &format!("superi.timeline.{}.gap", kind_code(kind)),
            &[],
            &output,
            vec![
                parameter(
                    "object-id",
                    TimelineGraphValue::EditorialObjectId(item.id()),
                ),
                parameter("name", TimelineGraphValue::Text(gap.name().to_owned())),
                parameter(
                    "record-range",
                    TimelineGraphValue::TimeRange(gap.record_range()),
                ),
            ],
            kind == TrackKind::Video,
        ),
        TrackItem::Transition(transition) => editable_node(
            node_id,
            &format!("superi.timeline.{}.transition", kind_code(kind)),
            &[
                PortSpec::new("from", kind, PortCardinality::Single),
                PortSpec::new("to", kind, PortCardinality::Single),
            ],
            &output,
            vec![
                parameter(
                    "object-id",
                    TimelineGraphValue::EditorialObjectId(item.id()),
                ),
                parameter(
                    "name",
                    TimelineGraphValue::Text(transition.name().to_owned()),
                ),
                parameter(
                    "from",
                    TimelineGraphValue::EditorialObjectId(transition.from()),
                ),
                parameter("to", TimelineGraphValue::EditorialObjectId(transition.to())),
                parameter(
                    "from-offset",
                    TimelineGraphValue::Duration(transition.from_offset()),
                ),
                parameter(
                    "to-offset",
                    TimelineGraphValue::Duration(transition.to_offset()),
                ),
            ],
            kind == TrackKind::Video,
        ),
        TrackItem::Generator(generator) => editable_node(
            node_id,
            &format!("superi.timeline.{}.generator", kind_code(kind)),
            &[],
            &output,
            vec![
                parameter(
                    "object-id",
                    TimelineGraphValue::EditorialObjectId(item.id()),
                ),
                parameter(
                    "name",
                    TimelineGraphValue::Text(generator.name().to_owned()),
                ),
                parameter(
                    "generator-kind",
                    TimelineGraphValue::Text(generator.kind().to_owned()),
                ),
                parameter(
                    "generator-parameters",
                    TimelineGraphValue::StringMap(generator.parameters().clone()),
                ),
                parameter(
                    "record-range",
                    TimelineGraphValue::TimeRange(generator.record_range()),
                ),
            ],
            kind == TrackKind::Video,
        ),
        TrackItem::Caption(caption) => editable_node(
            node_id,
            &format!("superi.timeline.{}.caption", kind_code(kind)),
            &[],
            &output,
            vec![
                parameter(
                    "object-id",
                    TimelineGraphValue::EditorialObjectId(item.id()),
                ),
                parameter("name", TimelineGraphValue::Text(caption.name().to_owned())),
                parameter(
                    "caption-text",
                    TimelineGraphValue::Text(caption.text().to_owned()),
                ),
                parameter(
                    "language",
                    TimelineGraphValue::OptionalText(caption.language().map(str::to_owned)),
                ),
                parameter(
                    "record-range",
                    TimelineGraphValue::TimeRange(caption.record_range()),
                ),
            ],
            false,
        ),
    }
}

#[derive(Clone, Copy)]
struct PortSpec {
    name: &'static str,
    kind: TrackKind,
    cardinality: PortCardinality,
}

impl PortSpec {
    const fn new(name: &'static str, kind: TrackKind, cardinality: PortCardinality) -> Self {
        Self {
            name,
            kind,
            cardinality,
        }
    }
}

struct ParameterSpec {
    name: &'static str,
    value: TimelineGraphValue,
}

fn parameter(name: &'static str, value: TimelineGraphValue) -> ParameterSpec {
    ParameterSpec { name, value }
}

fn editable_node(
    node_id: NodeId,
    node_type: &str,
    inputs: &[PortSpec],
    outputs: &[PortSpec],
    parameters: Vec<ParameterSpec>,
    color_bearing: bool,
) -> Result<EditableNode<TimelineGraphValue>> {
    let input_schema = inputs
        .iter()
        .map(|spec| {
            Ok(PortSchema::new(
                parse_port_name(spec.name)?,
                stream_value_type(spec.kind)?,
                spec.cardinality,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let output_schema = outputs
        .iter()
        .map(|spec| {
            Ok(PortSchema::new(
                parse_port_name(spec.name)?,
                stream_value_type(spec.kind)?,
                spec.cardinality,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut parameter_schema = Vec::with_capacity(parameters.len());
    let mut parameter_instances = Vec::with_capacity(parameters.len());
    for spec in parameters {
        let name = parse_parameter_name(spec.name)?;
        let value_type = parse_value_type(spec.value.value_type_code())?;
        parameter_schema.push(ParameterSchema::new(
            name.clone(),
            value_type.clone(),
            false,
        ));
        parameter_instances.push(EditableParameter::new(
            parameter_id(node_id, spec.name),
            name,
            TypedParameterValue::new(value_type, spec.value),
        ));
    }
    let schema = Arc::new(NodeSchema::new(
        NodeSchemaId::new(
            NodeTypeId::new(node_type).map_err(|_| schema_error("parse_node_type", node_type))?,
            SemanticVersion::new(1, 0, 0),
        ),
        input_schema,
        output_schema,
        parameter_schema,
        NodeBehavior::new(
            TimeBehavior::CurrentFrame,
            RoiBehavior::InputBounds,
            if color_bearing {
                ColorRequirements::Tagged
            } else {
                ColorRequirements::NotApplicable
            },
            Determinism::Deterministic,
            if color_bearing {
                CachePolicy::PerRegion
            } else {
                CachePolicy::PerFrame
            },
        ),
        CapabilitySet::default(),
    )?);
    let input_instances = inputs.iter().map(|spec| {
        Ok(InstancePort::new(
            input_port_id(node_id, spec.name),
            parse_port_name(spec.name)?,
        ))
    });
    let output_instances = outputs.iter().map(|spec| {
        Ok(InstancePort::new(
            output_port_id(node_id, spec.name),
            parse_port_name(spec.name)?,
        ))
    });
    EditableNode::new(
        schema,
        input_instances.collect::<Result<Vec<_>>>()?,
        output_instances.collect::<Result<Vec<_>>>()?,
        parameter_instances,
    )
}

fn all_stream_ports(cardinality: PortCardinality) -> [PortSpec; 4] {
    [
        PortSpec::new("video", TrackKind::Video, cardinality),
        PortSpec::new("audio", TrackKind::Audio, cardinality),
        PortSpec::new("caption", TrackKind::Caption, cardinality),
        PortSpec::new("data", TrackKind::Data, cardinality),
    ]
}

fn kind_code(kind: TrackKind) -> &'static str {
    match kind {
        TrackKind::Video => "video",
        TrackKind::Audio => "audio",
        TrackKind::Caption => "caption",
        TrackKind::Data => "data",
    }
}

fn stream_value_type(kind: TrackKind) -> Result<ValueTypeId> {
    parse_value_type(match kind {
        TrackKind::Video => "superi.value.timeline.video",
        TrackKind::Audio => "superi.value.timeline.audio",
        TrackKind::Caption => "superi.value.timeline.caption",
        TrackKind::Data => "superi.value.timeline.data",
    })
}

fn parse_value_type(input: &str) -> Result<ValueTypeId> {
    ValueTypeId::new(input).map_err(|_| schema_error("parse_value_type", input))
}

fn parse_port_name(input: &str) -> Result<PortName> {
    PortName::new(input).map_err(|_| schema_error("parse_port_name", input))
}

fn parse_parameter_name(input: &str) -> Result<ParameterName> {
    ParameterName::new(input).map_err(|_| schema_error("parse_parameter_name", input))
}

fn graph_id(project_id: ProjectId, root_timeline_id: TimelineId) -> GraphId {
    let project = project_id.to_bytes();
    let timeline = root_timeline_id.to_bytes();
    GraphId::from_raw(stable_raw("graph", &[&project, &timeline]))
}

fn timeline_node_id(project_id: ProjectId, timeline_id: TimelineId) -> NodeId {
    let project = project_id.to_bytes();
    let timeline = timeline_id.to_bytes();
    NodeId::from_raw(stable_raw("node.timeline", &[&project, &timeline]))
}

fn track_node_id(project_id: ProjectId, track_id: TrackId) -> NodeId {
    let project = project_id.to_bytes();
    let track = track_id.to_bytes();
    NodeId::from_raw(stable_raw("node.track", &[&project, &track]))
}

fn object_node_id(project_id: ProjectId, object_id: EditorialObjectId) -> NodeId {
    let project = project_id.to_bytes();
    let object = object_bytes(object_id);
    NodeId::from_raw(stable_raw(
        "node.object",
        &[&project, object_kind(object_id).as_bytes(), &object],
    ))
}

fn input_port_id(node_id: NodeId, name: &str) -> PortId {
    let node = node_id.to_bytes();
    PortId::from_raw(stable_raw("port.input", &[&node, name.as_bytes()]))
}

fn output_port_id(node_id: NodeId, name: &str) -> PortId {
    let node = node_id.to_bytes();
    PortId::from_raw(stable_raw("port.output", &[&node, name.as_bytes()]))
}

fn parameter_id(node_id: NodeId, name: &str) -> ParameterId {
    let node = node_id.to_bytes();
    ParameterId::from_raw(stable_raw("parameter", &[&node, name.as_bytes()]))
}

fn edge_id(
    source_node: NodeId,
    source_port: PortId,
    destination_node: NodeId,
    destination_port: PortId,
) -> EdgeId {
    let source_node = source_node.to_bytes();
    let source_port = source_port.to_bytes();
    let destination_node = destination_node.to_bytes();
    let destination_port = destination_port.to_bytes();
    EdgeId::from_raw(stable_raw(
        "edge",
        &[
            &source_node,
            &source_port,
            &destination_node,
            &destination_port,
        ],
    ))
}

fn object_kind(id: EditorialObjectId) -> &'static str {
    match id {
        EditorialObjectId::Clip(_) => "clip",
        EditorialObjectId::Gap(_) => "gap",
        EditorialObjectId::Transition(_) => "transition",
        EditorialObjectId::Generator(_) => "generator",
        EditorialObjectId::Caption(_) => "caption",
    }
}

fn object_bytes(id: EditorialObjectId) -> [u8; 16] {
    match id {
        EditorialObjectId::Clip(id) => id.to_bytes(),
        EditorialObjectId::Gap(id) => id.to_bytes(),
        EditorialObjectId::Transition(id) => id.to_bytes(),
        EditorialObjectId::Generator(id) => id.to_bytes(),
        EditorialObjectId::Caption(id) => id.to_bytes(),
    }
}

fn stable_raw(domain: &str, parts: &[&[u8]]) -> u128 {
    let mut hasher = Sha256::new();
    hash_part(&mut hasher, HASH_NAMESPACE);
    hash_part(&mut hasher, domain.as_bytes());
    for part in parts {
        hash_part(&mut hasher, part);
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    u128::from_be_bytes(bytes)
}

fn hash_part(hasher: &mut Sha256, part: &[u8]) {
    hasher.update((part.len() as u64).to_be_bytes());
    hasher.update(part);
}

fn internal_error(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn schema_error(operation: &'static str, value: &str) -> Error {
    internal_error(operation, "timeline compiler schema constant is invalid")
        .with_context(ErrorContext::new(COMPONENT, operation).with_field("value", value.to_owned()))
}

fn collision_error(
    operation: &'static str,
    derived_id: String,
    existing: String,
    incoming: String,
) -> Error {
    internal_error(operation, "stable timeline graph identifier collision").with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("derived_id", derived_id)
            .with_field("existing", existing)
            .with_field("incoming", incoming),
    )
}
