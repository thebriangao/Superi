//! Independent editable and prepared audio processing graphs.
//!
//! [`AudioGraph`] owns deterministic topology and exact channel-layout validation. Preparation
//! resolves one destination into a fixed processing order and allocates every intermediate buffer
//! before [`PreparedAudioGraph::process`] enters the platform-owned real-time audio domain.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::ChannelLayout;
use superi_core::time::SampleTime;

const COMPONENT: &str = "superi-audio.graph";

macro_rules! audio_id {
    ($name:ident, $prefix:literal) => {
        #[doc = concat!("Stable audio-owned `", stringify!($name), "` identity.")]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u128);

        impl $name {
            /// Creates an identity from its opaque raw value.
            #[must_use]
            pub const fn from_raw(raw: u128) -> Self {
                Self(raw)
            }

            /// Returns the opaque raw value.
            #[must_use]
            pub const fn raw(self) -> u128 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, concat!($prefix, ":{:032x}"), self.0)
            }
        }
    };
}

audio_id!(AudioGraphId, "audio-graph");
audio_id!(AudioNodeId, "audio-node");
audio_id!(AudioEdgeId, "audio-edge");

/// The routing role of one multi-input audio bus.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AudioBusKind {
    /// An intermediate summing destination on the dry signal path.
    Submix,
    /// A parallel effects destination reached by sends and left by returns.
    Auxiliary,
    /// The single terminal destination rendered for final output.
    Master,
}

/// The processing role of one audio node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AudioNodeKind {
    /// A node that produces audio without a graph input.
    Source,
    /// A node with exactly one direct input.
    Processor,
    /// A node that sums one or more ordered routes.
    Bus(AudioBusKind),
}

/// The authored meaning of one audio route.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AudioRouteKind {
    /// A route along the ordinary dry or submix path.
    Direct,
    /// A parallel route into an auxiliary bus.
    Send,
    /// A route from an auxiliary bus back to a submix or the master.
    AuxReturn,
}

/// One editable audio processing node with explicit channel meaning.
///
/// A node with no input layout is a source. Ordinary processors accept exactly one connection,
/// while typed buses accept multiple exact-layout routes in stable edge-identity order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioNode {
    id: AudioNodeId,
    kind: AudioNodeKind,
    input_layout: Option<ChannelLayout>,
    output_layout: ChannelLayout,
}

impl AudioNode {
    /// Creates a node descriptor.
    #[must_use]
    pub const fn new(
        id: AudioNodeId,
        input_layout: Option<ChannelLayout>,
        output_layout: ChannelLayout,
    ) -> Self {
        Self {
            id,
            kind: if input_layout.is_some() {
                AudioNodeKind::Processor
            } else {
                AudioNodeKind::Source
            },
            input_layout,
            output_layout,
        }
    }

    /// Creates a multi-input bus whose inputs and output share exact channel meaning.
    #[must_use]
    pub fn bus(id: AudioNodeId, kind: AudioBusKind, layout: ChannelLayout) -> Self {
        Self {
            id,
            kind: AudioNodeKind::Bus(kind),
            input_layout: Some(layout.clone()),
            output_layout: layout,
        }
    }

    /// Returns the stable node identity.
    #[must_use]
    pub const fn id(&self) -> AudioNodeId {
        self.id
    }

    /// Returns the source, single-input processor, or bus role.
    #[must_use]
    pub const fn kind(&self) -> AudioNodeKind {
        self.kind
    }

    /// Returns the required input layout, or `None` for a source.
    #[must_use]
    pub const fn input_layout(&self) -> Option<&ChannelLayout> {
        self.input_layout.as_ref()
    }

    /// Returns channels emitted in exact routing order.
    #[must_use]
    pub const fn output_layout(&self) -> &ChannelLayout {
        &self.output_layout
    }
}

/// One directed audio route.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AudioEdge {
    id: AudioEdgeId,
    source: AudioNodeId,
    destination: AudioNodeId,
    kind: AudioRouteKind,
}

impl AudioEdge {
    /// Creates a directed route from one node output to one node input.
    #[must_use]
    pub const fn new(id: AudioEdgeId, source: AudioNodeId, destination: AudioNodeId) -> Self {
        Self {
            id,
            source,
            destination,
            kind: AudioRouteKind::Direct,
        }
    }

    /// Creates a parallel send into an auxiliary bus.
    #[must_use]
    pub const fn send(id: AudioEdgeId, source: AudioNodeId, auxiliary: AudioNodeId) -> Self {
        Self {
            id,
            source,
            destination: auxiliary,
            kind: AudioRouteKind::Send,
        }
    }

    /// Creates a return from an auxiliary bus to a submix or master bus.
    #[must_use]
    pub const fn aux_return(
        id: AudioEdgeId,
        auxiliary: AudioNodeId,
        destination: AudioNodeId,
    ) -> Self {
        Self {
            id,
            source: auxiliary,
            destination,
            kind: AudioRouteKind::AuxReturn,
        }
    }

    /// Returns the stable edge identity.
    #[must_use]
    pub const fn id(self) -> AudioEdgeId {
        self.id
    }

    /// Returns the upstream node.
    #[must_use]
    pub const fn source(self) -> AudioNodeId {
        self.source
    }

    /// Returns the downstream node.
    #[must_use]
    pub const fn destination(self) -> AudioNodeId {
        self.destination
    }

    /// Returns whether this route is direct, a send, or an auxiliary return.
    #[must_use]
    pub const fn kind(self) -> AudioRouteKind {
        self.kind
    }
}

/// Editable deterministic audio DAG.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioGraph {
    id: AudioGraphId,
    sample_rate: u32,
    maximum_frames: usize,
    nodes: BTreeMap<AudioNodeId, AudioNode>,
    edges: BTreeMap<AudioEdgeId, AudioEdge>,
    incoming: BTreeMap<AudioNodeId, BTreeSet<AudioEdgeId>>,
    outgoing: BTreeMap<AudioNodeId, BTreeSet<AudioEdgeId>>,
    master: Option<AudioNodeId>,
}

impl AudioGraph {
    /// Creates an empty graph with one integral sample clock and positive block bound.
    pub fn new(id: AudioGraphId, sample_rate: u32, maximum_frames: usize) -> Result<Self> {
        if sample_rate == 0 {
            return Err(audio_error(
                ErrorCategory::InvalidInput,
                "create_graph",
                "audio graph sample rate must be greater than zero",
                [("graph_id", id.to_string())],
            ));
        }
        if maximum_frames == 0 {
            return Err(audio_error(
                ErrorCategory::InvalidInput,
                "create_graph",
                "audio graph maximum frame count must be greater than zero",
                [("graph_id", id.to_string())],
            ));
        }
        Ok(Self {
            id,
            sample_rate,
            maximum_frames,
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            incoming: BTreeMap::new(),
            outgoing: BTreeMap::new(),
            master: None,
        })
    }

    /// Returns the stable graph identity.
    #[must_use]
    pub const fn id(&self) -> AudioGraphId {
        self.id
    }

    /// Returns the exact sample clock used by every block.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the largest process block prepared by this graph.
    #[must_use]
    pub const fn maximum_frames(&self) -> usize {
        self.maximum_frames
    }

    /// Returns nodes in stable identity order.
    #[must_use]
    pub const fn nodes(&self) -> &BTreeMap<AudioNodeId, AudioNode> {
        &self.nodes
    }

    /// Returns edges in stable identity order.
    #[must_use]
    pub const fn edges(&self) -> &BTreeMap<AudioEdgeId, AudioEdge> {
        &self.edges
    }

    /// Returns the single authored master bus, when one exists.
    #[must_use]
    pub const fn master_node(&self) -> Option<AudioNodeId> {
        self.master
    }

    /// Inserts one uniquely identified node.
    pub fn insert_node(&mut self, node: AudioNode) -> Result<()> {
        if self.nodes.contains_key(&node.id) {
            return Err(self.error(
                ErrorCategory::Conflict,
                "insert_node",
                "audio node identity already exists",
                [("node_id", node.id.to_string())],
            ));
        }
        if node.kind == AudioNodeKind::Bus(AudioBusKind::Master) && self.master.is_some() {
            return Err(self.error(
                ErrorCategory::Conflict,
                "insert_node",
                "audio graph already contains a master bus",
                [("node_id", node.id.to_string())],
            ));
        }
        let id = node.id;
        if node.kind == AudioNodeKind::Bus(AudioBusKind::Master) {
            self.master = Some(id);
        }
        self.nodes.insert(id, node);
        self.incoming.insert(id, BTreeSet::new());
        self.outgoing.insert(id, BTreeSet::new());
        Ok(())
    }

    /// Removes one disconnected node.
    pub fn remove_node(&mut self, id: AudioNodeId) -> Result<AudioNode> {
        if !self.nodes.contains_key(&id) {
            return Err(self.error(
                ErrorCategory::NotFound,
                "remove_node",
                "audio node does not exist",
                [("node_id", id.to_string())],
            ));
        }
        if !self.incoming[&id].is_empty() || !self.outgoing[&id].is_empty() {
            return Err(self.error(
                ErrorCategory::Conflict,
                "remove_node",
                "audio node must be disconnected before removal",
                [("node_id", id.to_string())],
            ));
        }
        self.incoming.remove(&id);
        self.outgoing.remove(&id);
        if self.master == Some(id) {
            self.master = None;
        }
        Ok(self
            .nodes
            .remove(&id)
            .expect("validated audio node remains"))
    }

    /// Inserts one layout-compatible route without permitting cycles or ambiguous inputs.
    pub fn insert_edge(&mut self, edge: AudioEdge) -> Result<()> {
        if self.edges.contains_key(&edge.id) {
            return Err(self.edge_error(
                ErrorCategory::Conflict,
                "audio edge identity already exists",
                edge,
            ));
        }
        let source = self.nodes.get(&edge.source).ok_or_else(|| {
            self.edge_error(
                ErrorCategory::NotFound,
                "audio edge source node does not exist",
                edge,
            )
        })?;
        let destination = self.nodes.get(&edge.destination).ok_or_else(|| {
            self.edge_error(
                ErrorCategory::NotFound,
                "audio edge destination node does not exist",
                edge,
            )
        })?;
        if source.kind == AudioNodeKind::Bus(AudioBusKind::Master) {
            return Err(self.edge_error(
                ErrorCategory::InvalidInput,
                "audio master bus cannot have an outgoing route",
                edge,
            ));
        }
        match edge.kind {
            AudioRouteKind::Direct => {
                if source.kind == AudioNodeKind::Bus(AudioBusKind::Auxiliary)
                    || destination.kind == AudioNodeKind::Bus(AudioBusKind::Auxiliary)
                {
                    return Err(self.edge_error(
                        ErrorCategory::InvalidInput,
                        "auxiliary buses require explicit send and return routes",
                        edge,
                    ));
                }
            }
            AudioRouteKind::Send => {
                if destination.kind != AudioNodeKind::Bus(AudioBusKind::Auxiliary) {
                    return Err(self.edge_error(
                        ErrorCategory::InvalidInput,
                        "audio send destination must be an auxiliary bus",
                        edge,
                    ));
                }
            }
            AudioRouteKind::AuxReturn => {
                if source.kind != AudioNodeKind::Bus(AudioBusKind::Auxiliary)
                    || !matches!(
                        destination.kind,
                        AudioNodeKind::Bus(AudioBusKind::Submix | AudioBusKind::Master)
                    )
                {
                    return Err(self.edge_error(
                        ErrorCategory::InvalidInput,
                        "audio return must route an auxiliary bus into a submix or master bus",
                        edge,
                    ));
                }
            }
        }
        if edge.source == edge.destination || self.reaches(edge.destination, edge.source) {
            return Err(self.edge_error(
                ErrorCategory::Conflict,
                "audio edge would create a directed cycle",
                edge,
            ));
        }
        let Some(input_layout) = destination.input_layout() else {
            return Err(self.edge_error(
                ErrorCategory::InvalidInput,
                "audio source node cannot accept an input edge",
                edge,
            ));
        };
        if destination.kind == AudioNodeKind::Processor
            && !self.incoming[&edge.destination].is_empty()
        {
            return Err(self.edge_error(
                ErrorCategory::Conflict,
                "audio node input is already connected",
                edge,
            ));
        }
        if source.output_layout() != input_layout {
            return Err(self.edge_error(
                ErrorCategory::InvalidInput,
                "audio edge channel layouts do not match",
                edge,
            ));
        }
        self.edges.insert(edge.id, edge);
        self.outgoing
            .get_mut(&edge.source)
            .expect("validated source owns adjacency")
            .insert(edge.id);
        self.incoming
            .get_mut(&edge.destination)
            .expect("validated destination owns adjacency")
            .insert(edge.id);
        Ok(())
    }

    /// Removes one route.
    pub fn remove_edge(&mut self, id: AudioEdgeId) -> Result<AudioEdge> {
        let edge = self.edges.get(&id).copied().ok_or_else(|| {
            self.error(
                ErrorCategory::NotFound,
                "remove_edge",
                "audio edge does not exist",
                [("edge_id", id.to_string())],
            )
        })?;
        self.outgoing
            .get_mut(&edge.source)
            .expect("stored source owns adjacency")
            .remove(&id);
        self.incoming
            .get_mut(&edge.destination)
            .expect("stored destination owns adjacency")
            .remove(&id);
        self.edges.remove(&id);
        Ok(edge)
    }

    /// Returns the stable complete topological order.
    #[must_use]
    pub fn topological_order(&self) -> Vec<AudioNodeId> {
        let mut indegree = self
            .incoming
            .iter()
            .map(|(node, edges)| (*node, edges.len()))
            .collect::<BTreeMap<_, _>>();
        let mut ready = indegree
            .iter()
            .filter_map(|(node, degree)| (*degree == 0).then_some(*node))
            .collect::<BTreeSet<_>>();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(node) = ready.pop_first() {
            order.push(node);
            for edge_id in &self.outgoing[&node] {
                let destination = self.edges[edge_id].destination;
                let degree = indegree
                    .get_mut(&destination)
                    .expect("stored destination owns indegree");
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(destination);
                }
            }
        }
        debug_assert_eq!(order.len(), self.nodes.len());
        order
    }

    /// Resolves one audible destination into an immutable topology with preallocated buffers.
    pub fn prepare(
        &self,
        destination: AudioNodeId,
        mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>>,
    ) -> Result<PreparedAudioGraph> {
        if !self.nodes.contains_key(&destination) {
            return Err(self.error(
                ErrorCategory::NotFound,
                "prepare_graph",
                "audio graph destination does not exist",
                [("node_id", destination.to_string())],
            ));
        }

        let required = self.required_ancestors(destination);
        for node_id in &required {
            let node = &self.nodes[node_id];
            if node.input_layout.is_some() && self.incoming[node_id].is_empty() {
                return Err(self.error(
                    ErrorCategory::Conflict,
                    "prepare_graph",
                    "audio processing node input is not connected",
                    [("node_id", node_id.to_string())],
                ));
            }
            if !processors.contains_key(node_id) {
                return Err(self.error(
                    ErrorCategory::NotFound,
                    "prepare_graph",
                    "audio processor is missing for a required node",
                    [("node_id", node_id.to_string())],
                ));
            }
        }

        let order = self
            .topological_order()
            .into_iter()
            .filter(|node| required.contains(node))
            .collect::<Vec<_>>();
        let indices = order
            .iter()
            .enumerate()
            .map(|(index, node)| (*node, index))
            .collect::<BTreeMap<_, _>>();
        let mut prepared_nodes = Vec::with_capacity(order.len());
        for node_id in &order {
            let descriptor = self.nodes[node_id].clone();
            let input_routes = self.incoming[node_id]
                .iter()
                .map(|edge_id| PreparedAudioRoute {
                    edge_id: *edge_id,
                    source_node: self.edges[edge_id].source,
                    source_index: indices[&self.edges[edge_id].source],
                })
                .collect();
            let samples = self
                .maximum_frames
                .checked_mul(descriptor.output_layout.len())
                .ok_or_else(|| {
                    self.error(
                        ErrorCategory::ResourceExhausted,
                        "prepare_graph",
                        "audio processing buffer size overflowed",
                        [("node_id", node_id.to_string())],
                    )
                })?;
            let mut buffer = Vec::new();
            buffer.try_reserve_exact(samples).map_err(|_| {
                self.error(
                    ErrorCategory::ResourceExhausted,
                    "prepare_graph",
                    "audio processing buffer allocation failed",
                    [("node_id", node_id.to_string())],
                )
            })?;
            buffer.resize(samples, 0.0);
            prepared_nodes.push(PreparedNode {
                descriptor,
                input_routes,
                processor: processors
                    .remove(node_id)
                    .expect("required audio processor was validated"),
                buffer,
            });
        }

        Ok(PreparedAudioGraph {
            id: self.id,
            sample_rate: self.sample_rate,
            maximum_frames: self.maximum_frames,
            order,
            output_index: indices[&destination],
            nodes: prepared_nodes,
            next_sample: None,
        })
    }

    /// Prepares the graph's single master bus and every connected ancestor.
    pub fn prepare_master(
        &self,
        processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>>,
    ) -> Result<PreparedAudioGraph> {
        let master = self.master.ok_or_else(|| {
            self.error(
                ErrorCategory::NotFound,
                "prepare_master",
                "audio graph does not contain a master bus",
                [],
            )
        })?;
        self.prepare(master, processors)
    }

    fn reaches(&self, start: AudioNodeId, target: AudioNodeId) -> bool {
        let mut pending = vec![start];
        let mut visited = BTreeSet::new();
        while let Some(node) = pending.pop() {
            if node == target {
                return true;
            }
            if !visited.insert(node) {
                continue;
            }
            pending.extend(
                self.outgoing[&node]
                    .iter()
                    .map(|edge_id| self.edges[edge_id].destination),
            );
        }
        false
    }

    fn required_ancestors(&self, destination: AudioNodeId) -> BTreeSet<AudioNodeId> {
        let mut required = BTreeSet::new();
        let mut pending = vec![destination];
        while let Some(node) = pending.pop() {
            if !required.insert(node) {
                continue;
            }
            pending.extend(
                self.incoming[&node]
                    .iter()
                    .map(|edge_id| self.edges[edge_id].source),
            );
        }
        required
    }

    fn edge_error(&self, category: ErrorCategory, message: &'static str, edge: AudioEdge) -> Error {
        self.error(
            category,
            "insert_edge",
            message,
            [
                ("edge_id", edge.id.to_string()),
                ("source_node_id", edge.source.to_string()),
                ("destination_node_id", edge.destination.to_string()),
            ],
        )
    }

    fn error<const N: usize>(
        &self,
        category: ErrorCategory,
        operation: &'static str,
        message: &'static str,
        fields: [(&'static str, String); N],
    ) -> Error {
        let mut context =
            ErrorContext::new(COMPONENT, operation).with_field("graph_id", self.id.to_string());
        for (name, value) in fields {
            context.insert_field(name, value);
        }
        Error::new(category, Recoverability::UserCorrectable, message).with_context(context)
    }
}

/// One bounded block presented to an [`AudioProcessor`].
///
/// Samples are interleaved in their corresponding layout order. Implementations must not block,
/// allocate, free memory, retain the borrowed slices, or change sample timing on the successful
/// path when called by [`PreparedAudioGraph::process`].
pub struct AudioProcessBlock<'a> {
    /// Exact first-sample coordinate.
    pub start_time: SampleTime,
    /// Number of sample frames per channel.
    pub frame_count: usize,
    /// The first connected input in `input_layout` order, retained for single-input processors.
    /// Multi-input processors consume every route through [`AudioProcessor::process_inputs`].
    pub input: Option<&'a [f32]>,
    /// Optional input channel meaning. This is `None` only for a source node.
    pub input_layout: Option<&'a ChannelLayout>,
    /// Mutable output samples in `output_layout` order.
    pub output: &'a mut [f32],
    /// Output channel meaning.
    pub output_layout: &'a ChannelLayout,
}

/// One immutable prepared route into a processing node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedAudioRoute {
    edge_id: AudioEdgeId,
    source_node: AudioNodeId,
    source_index: usize,
}

impl PreparedAudioRoute {
    /// Returns the stable route identity.
    #[must_use]
    pub const fn edge_id(self) -> AudioEdgeId {
        self.edge_id
    }

    /// Returns the upstream node identity.
    #[must_use]
    pub const fn source_node(self) -> AudioNodeId {
        self.source_node
    }
}

/// One borrowed input block yielded without allocating or copying samples.
#[derive(Clone, Copy, Debug)]
pub struct AudioProcessInput<'a> {
    route: PreparedAudioRoute,
    samples: &'a [f32],
    layout: &'a ChannelLayout,
}

impl<'a> AudioProcessInput<'a> {
    /// Returns the route identity that supplied these samples.
    #[must_use]
    pub const fn edge_id(self) -> AudioEdgeId {
        self.route.edge_id
    }

    /// Returns the upstream node identity.
    #[must_use]
    pub const fn source_node(self) -> AudioNodeId {
        self.route.source_node
    }

    /// Returns interleaved samples for the current exact process window.
    #[must_use]
    pub const fn samples(self) -> &'a [f32] {
        self.samples
    }

    /// Returns the ordered channel meaning of the samples.
    #[must_use]
    pub const fn layout(self) -> &'a ChannelLayout {
        self.layout
    }
}

/// Allocation-free view of a node's prepared inputs.
#[derive(Clone, Copy)]
pub struct AudioProcessInputs<'a> {
    previous: &'a [PreparedNode],
    routes: &'a [PreparedAudioRoute],
    frame_count: usize,
}

impl<'a> AudioProcessInputs<'a> {
    /// Returns the number of connected routes.
    #[must_use]
    pub const fn len(self) -> usize {
        self.routes.len()
    }

    /// Returns whether the node has no connected route.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.routes.is_empty()
    }

    /// Returns one input by stable route order.
    #[must_use]
    pub fn get(self, index: usize) -> Option<AudioProcessInput<'a>> {
        let route = *self.routes.get(index)?;
        let source = self.previous.get(route.source_index)?;
        let sample_count = self.frame_count * source.descriptor.output_layout.len();
        Some(AudioProcessInput {
            route,
            samples: &source.buffer[..sample_count],
            layout: source.descriptor.output_layout(),
        })
    }

    /// Iterates inputs without allocating in stable route-identity order.
    #[must_use]
    pub const fn iter(self) -> AudioProcessInputsIter<'a> {
        AudioProcessInputsIter {
            inputs: self,
            next_index: 0,
        }
    }
}

impl<'a> IntoIterator for AudioProcessInputs<'a> {
    type Item = AudioProcessInput<'a>;
    type IntoIter = AudioProcessInputsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over borrowed prepared audio inputs.
#[derive(Clone)]
pub struct AudioProcessInputsIter<'a> {
    inputs: AudioProcessInputs<'a>,
    next_index: usize,
}

impl<'a> Iterator for AudioProcessInputsIter<'a> {
    type Item = AudioProcessInput<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let input = self.inputs.get(self.next_index)?;
        self.next_index += 1;
        Some(input)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.inputs.len().saturating_sub(self.next_index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for AudioProcessInputsIter<'_> {}
impl std::iter::FusedIterator for AudioProcessInputsIter<'_> {}

/// One prepared audio node implementation.
pub trait AudioProcessor: Send {
    /// Processes one complete bounded block.
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()>;

    /// Processes one complete bounded block with every prepared input route available.
    ///
    /// Single-input processors retain source compatibility by using the default implementation.
    /// Multi-input bus processors override this method and consume `inputs` without allocation.
    fn process_inputs(
        &mut self,
        block: AudioProcessBlock<'_>,
        _inputs: AudioProcessInputs<'_>,
    ) -> Result<()> {
        self.process(block)
    }
}

struct PreparedNode {
    descriptor: AudioNode,
    input_routes: Vec<PreparedAudioRoute>,
    processor: Box<dyn AudioProcessor>,
    buffer: Vec<f32>,
}

/// Fixed processing topology and preallocated storage for one graph destination.
pub struct PreparedAudioGraph {
    id: AudioGraphId,
    sample_rate: u32,
    maximum_frames: usize,
    order: Vec<AudioNodeId>,
    output_index: usize,
    nodes: Vec<PreparedNode>,
    next_sample: Option<i64>,
}

impl PreparedAudioGraph {
    /// Returns the source graph identity.
    #[must_use]
    pub const fn id(&self) -> AudioGraphId {
        self.id
    }

    /// Returns the exact fixed processing order.
    #[must_use]
    pub fn node_order(&self) -> &[AudioNodeId] {
        &self.order
    }

    /// Returns the destination channel meaning.
    #[must_use]
    pub fn output_layout(&self) -> &ChannelLayout {
        self.nodes[self.output_index].descriptor.output_layout()
    }

    /// Returns the next required sample after successful processing begins.
    #[must_use]
    pub const fn next_sample(&self) -> Option<i64> {
        self.next_sample
    }

    /// Returns one prepared node's immutable input routes in processing order.
    #[must_use]
    pub fn input_routes(&self, node: AudioNodeId) -> Option<&[PreparedAudioRoute]> {
        self.order
            .iter()
            .position(|candidate| *candidate == node)
            .map(|index| self.nodes[index].input_routes.as_slice())
    }

    /// Processes one exact consecutive block on the platform-owned audio domain.
    ///
    /// Topology is fixed and all intermediate storage is allocated by [`AudioGraph::prepare`].
    /// Validation finishes before any processor runs. The successful graph-owned path takes no
    /// lock and performs no allocation or free.
    pub fn process(
        &mut self,
        start_time: SampleTime,
        frame_count: usize,
        output: &mut [f32],
    ) -> Result<()> {
        if let Err(mut error) = ExecutionDomain::Audio.require_current() {
            error.push_context(ErrorContext::new(COMPONENT, "process_block"));
            return Err(error);
        }
        if start_time.sample_rate() != self.sample_rate {
            return Err(self.process_error(
                ErrorCategory::InvalidInput,
                "audio block sample rate does not match the prepared graph",
                start_time,
                frame_count,
            ));
        }
        if frame_count == 0 || frame_count > self.maximum_frames {
            return Err(self.process_error(
                ErrorCategory::InvalidInput,
                "audio block frame count is outside the prepared bound",
                start_time,
                frame_count,
            ));
        }
        if self
            .next_sample
            .is_some_and(|sample| sample != start_time.sample())
        {
            return Err(self.process_error(
                ErrorCategory::Conflict,
                "audio block is not consecutive with the prior processed sample",
                start_time,
                frame_count,
            ));
        }
        let output_samples = frame_count
            .checked_mul(self.output_layout().len())
            .ok_or_else(|| {
                self.process_error(
                    ErrorCategory::InvalidInput,
                    "audio output sample count overflowed",
                    start_time,
                    frame_count,
                )
            })?;
        if output.len() != output_samples {
            return Err(self.process_error(
                ErrorCategory::InvalidInput,
                "audio output length does not match frames and channel layout",
                start_time,
                frame_count,
            ));
        }
        let frame_count_i64 = i64::try_from(frame_count).map_err(|_| {
            self.process_error(
                ErrorCategory::InvalidInput,
                "audio block frame count exceeds the sample coordinate domain",
                start_time,
                frame_count,
            )
        })?;
        let next_sample = start_time
            .sample()
            .checked_add(frame_count_i64)
            .ok_or_else(|| {
                self.process_error(
                    ErrorCategory::InvalidInput,
                    "audio block end exceeds the sample coordinate domain",
                    start_time,
                    frame_count,
                )
            })?;

        for index in 0..self.nodes.len() {
            let (previous, current_and_later) = self.nodes.split_at_mut(index);
            let current = &mut current_and_later[0];
            let output_len = frame_count * current.descriptor.output_layout.len();
            let inputs = AudioProcessInputs {
                previous,
                routes: &current.input_routes,
                frame_count,
            };
            let input = inputs.get(0).map(AudioProcessInput::samples);
            let block = AudioProcessBlock {
                start_time,
                frame_count,
                input,
                input_layout: current.descriptor.input_layout(),
                output: &mut current.buffer[..output_len],
                output_layout: current.descriptor.output_layout(),
            };
            if let Err(mut error) = current.processor.process_inputs(block, inputs) {
                error.push_context(
                    ErrorContext::new(COMPONENT, "process_node")
                        .with_field("graph_id", self.id.to_string())
                        .with_field("node_id", current.descriptor.id.to_string()),
                );
                return Err(error);
            }
        }

        output.copy_from_slice(&self.nodes[self.output_index].buffer[..output_samples]);
        self.next_sample = Some(next_sample);
        Ok(())
    }

    fn process_error(
        &self,
        category: ErrorCategory,
        message: &'static str,
        start_time: SampleTime,
        frame_count: usize,
    ) -> Error {
        audio_error(
            category,
            "process_block",
            message,
            [
                ("graph_id", self.id.to_string()),
                ("start_sample", start_time.sample().to_string()),
                ("sample_rate", start_time.sample_rate().to_string()),
                ("frame_count", frame_count.to_string()),
            ],
        )
    }
}

fn audio_error<const N: usize>(
    category: ErrorCategory,
    operation: &'static str,
    message: &'static str,
    fields: [(&'static str, String); N],
) -> Error {
    let mut context = ErrorContext::new(COMPONENT, operation);
    for (name, value) in fields {
        context.insert_field(name, value);
    }
    Error::new(category, Recoverability::UserCorrectable, message).with_context(context)
}
