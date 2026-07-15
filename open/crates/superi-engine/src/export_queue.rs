//! Coherent render and export orchestration over shared engine contracts.
//!
//! One transaction connects a prepared source and decoder set to the shared graph scene value,
//! explicit delivery color and audio graph stages, and codec-neutral encoders. Output remains a
//! set of complete elementary packet streams because the open media layer does not own a muxer.

use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use superi_audio::graph::AudioGraphId;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::AlphaMode;
use superi_core::time::{Duration, RationalTime, SampleTime, TimeRange};
use superi_graph::dag::GraphEndpoint;
use superi_graph::eval::{EvaluateNode, EvaluationRequest};
use superi_graph::headless::GraphEvaluationSnapshot;
use superi_image::metadata::ColorPipelineMetadata;
use superi_media_io::audio_io::{AudioBlock, AudioFormat};
use superi_media_io::backend::{BackendRegistry, BackendRequirement, FallbackPolicy};
use superi_media_io::decode::{DecodeOutput, DecoderConfig, VideoFormat, VideoFrame};
use superi_media_io::demux::{
    BackendId, MediaMetadata, Packet, SeekMode, SeekRequest, SourceIdentity, SourceInfo, StreamId,
    StreamKind,
};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

use crate::lifecycle::{EngineLifecycleSnapshot, EngineWorkKind, EngineWorkPermit};
use crate::playback::{DecodedFrameMetadata, PlaybackSceneFrame};
use crate::resources::AcquiredMediaSource;

const COMPONENT: &str = "superi-engine.render_export";
const MAX_DRAIN_OUTPUTS: usize = 1_000_000;

/// One source stream routed to one exact encoder configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportStreamRoute {
    source_stream_id: StreamId,
    encoder_config: EncoderConfig,
}

impl ExportStreamRoute {
    /// Creates a source-to-encoder route.
    #[must_use]
    pub const fn new(source_stream_id: StreamId, encoder_config: EncoderConfig) -> Self {
        Self {
            source_stream_id,
            encoder_config,
        }
    }

    /// Returns the selected source stream.
    #[must_use]
    pub const fn source_stream_id(&self) -> StreamId {
        self.source_stream_id
    }

    /// Returns the exact output configuration.
    #[must_use]
    pub const fn encoder_config(&self) -> &EncoderConfig {
        &self.encoder_config
    }
}

/// One complete render and export transaction request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderExportRequest {
    source_start: RationalTime,
    routes: Vec<ExportStreamRoute>,
    fallback_policy: FallbackPolicy,
    permit: EngineWorkPermit,
}

impl RenderExportRequest {
    /// Creates and validates an export request.
    pub fn new(
        source_start: RationalTime,
        routes: Vec<ExportStreamRoute>,
        fallback_policy: FallbackPolicy,
        permit: EngineWorkPermit,
    ) -> Result<Self> {
        if permit.work() != EngineWorkKind::Export {
            return Err(invalid(
                "create_export_request",
                "render and export requires an export lifecycle permit",
            ));
        }
        if routes.is_empty() {
            return Err(invalid(
                "create_export_request",
                "render and export requires at least one stream route",
            ));
        }

        let mut source_streams = BTreeSet::new();
        let mut output_streams = BTreeSet::new();
        let mut video_routes = 0_usize;
        let mut audio_routes = 0_usize;
        for route in &routes {
            if !source_streams.insert(route.source_stream_id) {
                return Err(invalid(
                    "create_export_request",
                    "a source stream may be routed only once",
                ));
            }
            if !output_streams.insert(route.encoder_config.stream_id()) {
                return Err(invalid(
                    "create_export_request",
                    "an output stream identifier may be used only once",
                ));
            }
            match route.encoder_config.media_format() {
                EncoderMediaFormat::Video(_) => video_routes += 1,
                EncoderMediaFormat::Audio(_) => audio_routes += 1,
                _ => {
                    return Err(unsupported(
                        "create_export_request",
                        "the requested encoder media format is not supported by render and export",
                    ));
                }
            }
        }
        if video_routes > 1 || audio_routes > 1 {
            return Err(unsupported(
                "create_export_request",
                "one render and export transaction supports at most one video and one audio route",
            ));
        }

        Ok(Self {
            source_start,
            routes,
            fallback_policy,
            permit,
        })
    }

    /// Returns the exact source seek coordinate.
    #[must_use]
    pub const fn source_start(&self) -> RationalTime {
        self.source_start
    }

    /// Returns routes in caller-declared output order.
    #[must_use]
    pub fn routes(&self) -> &[ExportStreamRoute] {
        &self.routes
    }

    /// Returns the explicit backend fallback policy.
    #[must_use]
    pub const fn fallback_policy(&self) -> FallbackPolicy {
        self.fallback_policy
    }

    /// Returns the revision-scoped export permit.
    #[must_use]
    pub const fn permit(&self) -> EngineWorkPermit {
        self.permit
    }
}

/// Immutable evidence for one deterministic encoder selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncoderSelectionEvidence {
    selected_backend_id: BackendId,
    fallback_backend_ids: Vec<BackendId>,
    fallback_used: bool,
}

impl EncoderSelectionEvidence {
    /// Returns the backend whose encoder was constructed.
    #[must_use]
    pub const fn selected_backend_id(&self) -> &BackendId {
        &self.selected_backend_id
    }

    /// Returns eligible fallback candidates in stable rank order.
    #[must_use]
    pub fn fallback_backend_ids(&self) -> &[BackendId] {
        &self.fallback_backend_ids
    }

    /// Returns whether selection required the registered fallback tier.
    #[must_use]
    pub const fn fallback_used(&self) -> bool {
        self.fallback_used
    }
}

/// Preserved video semantics delivered to one encoder input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportVideoProvenance {
    timestamp: RationalTime,
    duration: Duration,
    decoded: Vec<DecodedFrameMetadata>,
    metadata: MediaMetadata,
    graph_revision: u64,
    color_pipeline: ColorPipelineMetadata,
    scene_alpha_mode: AlphaMode,
    output_alpha_mode: AlphaMode,
    output_format: VideoFormat,
}

impl ExportVideoProvenance {
    /// Returns the exact output timestamp.
    #[must_use]
    pub const fn timestamp(&self) -> RationalTime {
        self.timestamp
    }

    /// Returns the exact output duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns every decoded input represented by the graph scene value.
    #[must_use]
    pub fn decoded(&self) -> &[DecodedFrameMetadata] {
        &self.decoded
    }

    /// Returns metadata supplied to the encoder.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }

    /// Returns the immutable graph revision that produced this scene.
    #[must_use]
    pub fn graph_revision(&self) -> u64 {
        self.graph_revision
    }

    /// Returns the terminal delivery color history supplied to the encoder.
    #[must_use]
    pub const fn color_pipeline(&self) -> &ColorPipelineMetadata {
        &self.color_pipeline
    }

    /// Returns alpha association at the shared scene boundary.
    #[must_use]
    pub const fn scene_alpha_mode(&self) -> AlphaMode {
        self.scene_alpha_mode
    }

    /// Returns alpha association at the encoder boundary.
    #[must_use]
    pub const fn output_alpha_mode(&self) -> AlphaMode {
        self.output_alpha_mode
    }

    /// Returns the exact encoder-facing video representation.
    #[must_use]
    pub const fn output_format(&self) -> VideoFormat {
        self.output_format
    }
}

/// Preserved audio semantics delivered to one encoder input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportAudioProvenance {
    timestamp: SampleTime,
    duration: Duration,
    metadata: MediaMetadata,
    graph_id: AudioGraphId,
    output_format: AudioFormat,
}

impl ExportAudioProvenance {
    /// Returns the exact first output sample.
    #[must_use]
    pub const fn timestamp(&self) -> SampleTime {
        self.timestamp
    }

    /// Returns the exact sample duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns metadata supplied to the encoder.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }

    /// Returns the stable prepared audio graph identity.
    #[must_use]
    pub const fn graph_id(&self) -> AudioGraphId {
        self.graph_id
    }

    /// Returns the exact encoder-facing audio representation.
    #[must_use]
    pub const fn output_format(&self) -> &AudioFormat {
        &self.output_format
    }
}

/// One encoder input with complete orchestration provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ExportInputProvenance {
    /// One graph and delivery processed video frame.
    Video(ExportVideoProvenance),
    /// One audio graph processed block.
    Audio(ExportAudioProvenance),
}

impl ExportInputProvenance {
    fn time_range(&self) -> Result<TimeRange> {
        match self {
            Self::Video(value) => TimeRange::new(value.timestamp, value.duration),
            Self::Audio(value) => TimeRange::new(
                RationalTime::from_sample_time(value.timestamp),
                value.duration,
            ),
        }
    }

    fn metadata(&self) -> &MediaMetadata {
        match self {
            Self::Video(value) => &value.metadata,
            Self::Audio(value) => &value.metadata,
        }
    }
}

/// One complete elementary encoded stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElementaryPacketStream {
    source_stream_id: StreamId,
    config: EncoderConfig,
    selection: EncoderSelectionEvidence,
    inputs: Arc<[ExportInputProvenance]>,
    packets: Arc<[Packet]>,
}

impl ElementaryPacketStream {
    /// Returns the source stream whose decoded values were processed.
    #[must_use]
    pub const fn source_stream_id(&self) -> StreamId {
        self.source_stream_id
    }

    /// Returns the exact encoder configuration.
    #[must_use]
    pub const fn config(&self) -> &EncoderConfig {
        &self.config
    }

    /// Returns deterministic backend selection evidence.
    #[must_use]
    pub const fn selection(&self) -> &EncoderSelectionEvidence {
        &self.selection
    }

    /// Returns complete encoder-input provenance in presentation order.
    #[must_use]
    pub fn inputs(&self) -> &[ExportInputProvenance] {
        &self.inputs
    }

    /// Returns complete packets in encoder output order.
    #[must_use]
    pub fn packets(&self) -> &[Packet] {
        &self.packets
    }
}

/// Atomic transaction result containing only complete elementary streams.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderExportArtifact {
    source_identity: SourceIdentity,
    streams: Arc<[ElementaryPacketStream]>,
}

impl RenderExportArtifact {
    /// Returns the exact project and content identity that was rendered.
    #[must_use]
    pub const fn source_identity(&self) -> &SourceIdentity {
        &self.source_identity
    }

    /// Returns complete elementary streams in request order.
    #[must_use]
    pub fn streams(&self) -> &[ElementaryPacketStream] {
        &self.streams
    }
}

/// One graph-produced scene and the immutable revision that produced it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedExportVideo<V> {
    scene: PlaybackSceneFrame<V>,
    graph_revision: u64,
}

impl<V> RenderedExportVideo<V> {
    /// Creates graph-render evidence.
    #[must_use]
    pub const fn new(scene: PlaybackSceneFrame<V>, graph_revision: u64) -> Self {
        Self {
            scene,
            graph_revision,
        }
    }

    /// Returns the shared graph-produced scene value.
    #[must_use]
    pub const fn scene(&self) -> &PlaybackSceneFrame<V> {
        &self.scene
    }

    /// Returns the immutable graph revision.
    #[must_use]
    pub const fn graph_revision(&self) -> u64 {
        self.graph_revision
    }
}

/// One audio graph output with stable graph identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedExportAudio {
    block: AudioBlock,
    graph_id: AudioGraphId,
}

impl RenderedExportAudio {
    /// Creates audio graph output evidence.
    #[must_use]
    pub const fn new(block: AudioBlock, graph_id: AudioGraphId) -> Self {
        Self { block, graph_id }
    }

    /// Returns the processed audio block.
    #[must_use]
    pub const fn block(&self) -> &AudioBlock {
        &self.block
    }

    /// Returns the stable prepared audio graph identity.
    #[must_use]
    pub const fn graph_id(&self) -> AudioGraphId {
        self.graph_id
    }
}

/// Shared graph render seam used by offline export.
pub trait ExportVideoGraph<V> {
    /// Returns the exact nonterminal scene pipeline expected from the graph.
    fn scene_pipeline(&self) -> &ColorPipelineMetadata;

    /// Returns the exact alpha association expected from the graph.
    fn scene_alpha_mode(&self) -> AlphaMode;

    /// Evaluates one decoded frame through the shared graph path.
    fn render(
        &mut self,
        decoded: &VideoFrame,
        operation: &OperationContext,
    ) -> Result<RenderedExportVideo<V>>;
}

/// Explicit decoded-frame binding used by a caller-owned runtime graph catalog.
pub trait ExportDecodedFrameBinder {
    /// Publishes one shared decoded frame immediately before its exact graph request.
    fn bind(&mut self, decoded: &VideoFrame, operation: &OperationContext) -> Result<()>;

    /// Clears the binding after evaluation or failure so stale media cannot be observed later.
    fn clear(&mut self);
}

/// Shared single-frame binding for runtime nodes that consume the active decoded frame.
///
/// Cloning this value shares only the frame owner and does not copy pixel storage. Offline export
/// is sequential, so the headless renderer publishes one binding, evaluates one request, and clears
/// it before advancing to the next decoded frame.
#[derive(Clone, Debug, Default)]
pub struct SharedExportDecodedFrame {
    current: Arc<Mutex<Option<VideoFrame>>>,
}

impl SharedExportDecodedFrame {
    /// Creates an empty decoded-frame binding.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the currently bound frame for one runtime graph node.
    pub fn current(&self) -> Result<VideoFrame> {
        self.current
            .lock()
            .map_err(|_| {
                internal(
                    "read_export_graph_binding",
                    "decoded-frame graph binding lock is poisoned",
                )
            })?
            .clone()
            .ok_or_else(|| {
                conflict(
                    "read_export_graph_binding",
                    "runtime graph requested decoded media outside an active export frame",
                )
            })
    }
}

impl ExportDecodedFrameBinder for SharedExportDecodedFrame {
    fn bind(&mut self, decoded: &VideoFrame, operation: &OperationContext) -> Result<()> {
        operation.check("bind_export_graph_frame")?;
        let mut current = self.current.lock().map_err(|_| {
            internal(
                "bind_export_graph_frame",
                "decoded-frame graph binding lock is poisoned",
            )
        })?;
        if current.is_some() {
            return Err(conflict(
                "bind_export_graph_frame",
                "decoded-frame graph binding is already occupied",
            ));
        }
        *current = Some(decoded.clone());
        Ok(())
    }

    fn clear(&mut self) {
        if let Ok(mut current) = self.current.lock() {
            *current = None;
        }
    }
}

/// Explicit delivery color and encoder-materialization seam.
pub trait ExportVideoDelivery<V> {
    /// Returns the exact nonterminal scene pipeline accepted by delivery.
    fn scene_pipeline(&self) -> &ColorPipelineMetadata;

    /// Returns the exact scene alpha association accepted by delivery.
    fn scene_alpha_mode(&self) -> AlphaMode;

    /// Returns the terminal output pipeline produced by delivery.
    fn output_pipeline(&self) -> &ColorPipelineMetadata;

    /// Returns the explicit alpha association produced by delivery.
    fn output_alpha_mode(&self) -> AlphaMode;

    /// Produces one encoder-ready frame from the shared scene value.
    fn transform(
        &mut self,
        scene: &PlaybackSceneFrame<V>,
        operation: &OperationContext,
    ) -> Result<VideoFrame>;
}

/// Explicit decoded-audio to prepared-audio-graph seam.
pub trait ExportAudioGraph {
    /// Processes one decoded block without changing its physical timeline meaning.
    fn render(
        &mut self,
        decoded: &AudioBlock,
        operation: &OperationContext,
    ) -> Result<RenderedExportAudio>;
}

/// Shared immutable graph evaluator used for exact offline frame requests.
pub struct HeadlessGraphVideoRenderer<T, N, V> {
    snapshot: Arc<GraphEvaluationSnapshot<T, N>>,
    output: GraphEndpoint,
    region: PixelBounds,
    scene_pipeline: ColorPipelineMetadata,
    scene_alpha_mode: AlphaMode,
    decoded_binding: Box<dyn ExportDecodedFrameBinder>,
    value: PhantomData<fn() -> V>,
}

impl<T, N, V> HeadlessGraphVideoRenderer<T, N, V> {
    /// Binds one immutable graph revision and exact output request scope.
    pub fn new<B>(
        snapshot: Arc<GraphEvaluationSnapshot<T, N>>,
        output: GraphEndpoint,
        region: PixelBounds,
        scene_pipeline: ColorPipelineMetadata,
        scene_alpha_mode: AlphaMode,
        decoded_binding: B,
    ) -> Result<Self>
    where
        B: ExportDecodedFrameBinder + 'static,
    {
        validate_nonterminal_pipeline(&scene_pipeline, "create_headless_export_renderer")?;
        Ok(Self {
            snapshot,
            output,
            region,
            scene_pipeline,
            scene_alpha_mode,
            decoded_binding: Box::new(decoded_binding),
            value: PhantomData,
        })
    }

    /// Returns the retained immutable graph revision.
    #[must_use]
    pub fn graph_revision(&self) -> u64 {
        self.snapshot.graph_revision()
    }
}

impl<T, N, V> ExportVideoGraph<V> for HeadlessGraphVideoRenderer<T, N, V>
where
    N: EvaluateNode<PlaybackSceneFrame<V>>,
    V: Clone,
{
    fn scene_pipeline(&self) -> &ColorPipelineMetadata {
        &self.scene_pipeline
    }

    fn scene_alpha_mode(&self) -> AlphaMode {
        self.scene_alpha_mode
    }

    fn render(
        &mut self,
        decoded: &VideoFrame,
        operation: &OperationContext,
    ) -> Result<RenderedExportVideo<V>> {
        operation.check("evaluate_export_graph")?;
        self.decoded_binding.bind(decoded, operation)?;
        let result = self.snapshot.evaluate(EvaluationRequest::new(
            self.output,
            decoded.timestamp(),
            self.region,
        ));
        self.decoded_binding.clear();
        let result = result?;
        let scene = result.value().clone();
        validate_graph_scene(&scene, decoded, &self.scene_pipeline, self.scene_alpha_mode)?;
        Ok(RenderedExportVideo::new(
            scene,
            self.snapshot.graph_revision(),
        ))
    }
}

/// Stateful prepared source and decoder boundary consumed by the transaction.
pub trait ExportMediaSession {
    /// Returns immutable source identity and stream declarations.
    fn source_info(&self) -> &SourceInfo;

    /// Returns one configured decoder contract.
    fn decoder_config(&self, stream_id: StreamId) -> Option<DecoderConfig>;

    /// Seeks the opened source.
    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime>;

    /// Reads one packet or explicit source boundary.
    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>>;

    /// Resets one decoder before a fresh source lifetime.
    fn reset_decoder(&mut self, stream_id: StreamId, operation: &OperationContext) -> Result<()>;

    /// Supplies one complete source packet to its decoder.
    fn send_packet(
        &mut self,
        stream_id: StreamId,
        packet: Packet,
        operation: &OperationContext,
    ) -> Result<()>;

    /// Receives one decoded value or lifecycle state.
    fn receive_decoder(
        &mut self,
        stream_id: StreamId,
        operation: &OperationContext,
    ) -> Result<DecodeOutput>;

    /// Flushes one decoder at end of source.
    fn flush_decoder(&mut self, stream_id: StreamId, operation: &OperationContext) -> Result<()>;
}

impl ExportMediaSession for AcquiredMediaSource {
    fn source_info(&self) -> &SourceInfo {
        self.info()
    }

    fn decoder_config(&self, stream_id: StreamId) -> Option<DecoderConfig> {
        self.decoder(stream_id).map(|value| value.config().clone())
    }

    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime> {
        self.source_mut().seek(request, operation)
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        self.source_mut().read_packet(operation)
    }

    fn reset_decoder(&mut self, stream_id: StreamId, operation: &OperationContext) -> Result<()> {
        acquired_decoder_mut(self, stream_id)?
            .decoder_mut()
            .reset(operation)
    }

    fn send_packet(
        &mut self,
        stream_id: StreamId,
        packet: Packet,
        operation: &OperationContext,
    ) -> Result<()> {
        acquired_decoder_mut(self, stream_id)?
            .decoder_mut()
            .send_packet(packet, operation)
    }

    fn receive_decoder(
        &mut self,
        stream_id: StreamId,
        operation: &OperationContext,
    ) -> Result<DecodeOutput> {
        acquired_decoder_mut(self, stream_id)?
            .decoder_mut()
            .receive(operation)
    }

    fn flush_decoder(&mut self, stream_id: StreamId, operation: &OperationContext) -> Result<()> {
        acquired_decoder_mut(self, stream_id)?
            .decoder_mut()
            .flush(operation)
    }
}

/// Borrowed stage set for video-only, audio-only, or paired export.
pub struct RenderExportStages<'a, V> {
    video_graph: Option<&'a mut dyn ExportVideoGraph<V>>,
    video_delivery: Option<&'a mut dyn ExportVideoDelivery<V>>,
    audio_graph: Option<&'a mut dyn ExportAudioGraph>,
}

impl<'a, V> RenderExportStages<'a, V> {
    /// Creates a paired video and audio stage set.
    #[must_use]
    pub fn av(
        video_graph: &'a mut dyn ExportVideoGraph<V>,
        video_delivery: &'a mut dyn ExportVideoDelivery<V>,
        audio_graph: &'a mut dyn ExportAudioGraph,
    ) -> Self {
        Self {
            video_graph: Some(video_graph),
            video_delivery: Some(video_delivery),
            audio_graph: Some(audio_graph),
        }
    }

    /// Creates a video-only stage set.
    #[must_use]
    pub fn video(
        video_graph: &'a mut dyn ExportVideoGraph<V>,
        video_delivery: &'a mut dyn ExportVideoDelivery<V>,
    ) -> Self {
        Self {
            video_graph: Some(video_graph),
            video_delivery: Some(video_delivery),
            audio_graph: None,
        }
    }

    /// Creates an audio-only stage set.
    #[must_use]
    pub fn audio(audio_graph: &'a mut dyn ExportAudioGraph) -> Self {
        Self {
            video_graph: None,
            video_delivery: None,
            audio_graph: Some(audio_graph),
        }
    }
}

struct ActiveRoute {
    route: ExportStreamRoute,
    selection: EncoderSelectionEvidence,
    encoder: Box<dyn Encoder>,
    inputs: Vec<ExportInputProvenance>,
    packets: Vec<Packet>,
    last_input_start: Option<RationalTime>,
    graph_revision: Option<u64>,
    audio_graph_id: Option<AudioGraphId>,
}

/// Renders and encodes one complete source lifetime without publishing partial output.
pub fn render_and_export<S, V, F>(
    request: &RenderExportRequest,
    lifecycle_snapshot: F,
    session: &mut S,
    registry: &BackendRegistry,
    mut stages: RenderExportStages<'_, V>,
    operation: &OperationContext,
) -> Result<RenderExportArtifact>
where
    S: ExportMediaSession,
    F: Fn() -> Result<EngineLifecycleSnapshot>,
{
    operation.check("begin_render_export")?;
    if operation.priority() != MediaPriority::Export {
        return Err(invalid(
            "begin_render_export",
            "render and export requires export media priority",
        ));
    }
    validate_permit(
        &lifecycle_snapshot()?,
        request.permit,
        "admit_render_export",
    )?;
    validate_session_routes(request, session, &stages)?;

    let source_identity = session.source_info().identity().clone();
    let mut active = create_encoders(request, registry, operation)?;
    let route_indexes = active
        .iter()
        .enumerate()
        .map(|(index, route)| (route.route.source_stream_id, index))
        .collect::<BTreeMap<_, _>>();

    let execution = execute_transaction(
        request,
        session,
        &route_indexes,
        &mut active,
        &mut stages,
        operation,
    );
    if let Err(error) = execution {
        recover_transaction(session, &mut active);
        return Err(error);
    }

    let finalization = (|| {
        operation.check("validate_render_export")?;
        for route in &active {
            validate_complete_stream(route)?;
        }
        validate_permit(
            &lifecycle_snapshot()?,
            request.permit,
            "publish_render_export",
        )?;
        reset_transaction(session, &mut active, operation)?;
        operation.check("publish_render_export")
    })();
    if let Err(error) = finalization {
        recover_transaction(session, &mut active);
        return Err(error);
    }

    let streams = active
        .into_iter()
        .map(|route| ElementaryPacketStream {
            source_stream_id: route.route.source_stream_id,
            config: route.route.encoder_config,
            selection: route.selection,
            inputs: Arc::from(route.inputs),
            packets: Arc::from(route.packets),
        })
        .collect::<Vec<_>>();
    Ok(RenderExportArtifact {
        source_identity,
        streams: Arc::from(streams),
    })
}

fn create_encoders(
    request: &RenderExportRequest,
    registry: &BackendRegistry,
    operation: &OperationContext,
) -> Result<Vec<ActiveRoute>> {
    let mut active = Vec::with_capacity(request.routes.len());
    for route in &request.routes {
        if let Err(error) = operation.check("select_export_encoder") {
            recover_encoders(&mut active);
            return Err(error);
        }
        let selection = match registry.select(
            &BackendRequirement::encode(route.encoder_config.codec().clone()),
            request.fallback_policy,
        ) {
            Ok(selection) => selection,
            Err(error) => {
                recover_encoders(&mut active);
                return Err(error);
            }
        };
        let evidence = EncoderSelectionEvidence {
            selected_backend_id: selection.primary().descriptor().id().clone(),
            fallback_backend_ids: selection
                .fallbacks()
                .iter()
                .map(|backend| backend.descriptor().id().clone())
                .collect(),
            fallback_used: selection.fallback_used(),
        };
        let mut encoder = match selection
            .primary()
            .create_encoder(&route.encoder_config, operation)
        {
            Ok(encoder) => encoder,
            Err(error) => {
                recover_encoders(&mut active);
                return Err(with_stream_context(
                    error,
                    "create_export_encoder",
                    route.source_stream_id,
                ));
            }
        };
        if encoder.config() != &route.encoder_config {
            let recovery = OperationContext::new(MediaPriority::Export);
            let _ = encoder.reset(&recovery);
            recover_encoders(&mut active);
            return Err(internal(
                "create_export_encoder",
                "selected encoder configuration differs from the requested export route",
            ));
        }
        active.push(ActiveRoute {
            route: route.clone(),
            selection: evidence,
            encoder,
            inputs: Vec::new(),
            packets: Vec::new(),
            last_input_start: None,
            graph_revision: None,
            audio_graph_id: None,
        });
    }
    Ok(active)
}

fn execute_transaction<S, V>(
    request: &RenderExportRequest,
    session: &mut S,
    route_indexes: &BTreeMap<StreamId, usize>,
    active: &mut [ActiveRoute],
    stages: &mut RenderExportStages<'_, V>,
    operation: &OperationContext,
) -> Result<()>
where
    S: ExportMediaSession,
{
    for route in active.iter_mut() {
        operation.check("reset_export_stream")?;
        session.reset_decoder(route.route.source_stream_id, operation)?;
        route.encoder.reset(operation)?;
    }
    let selected = session.seek(
        SeekRequest::new(request.source_start, SeekMode::Exact),
        operation,
    )?;
    if selected != request.source_start {
        return Err(corrupt(
            "seek_export_source",
            "exact export seek returned a different physical coordinate",
        ));
    }

    loop {
        operation.check("read_export_packet")?;
        match session.read_packet(operation)? {
            ReadOutcome::Complete(packet) => {
                let Some(index) = route_indexes.get(&packet.stream_id()).copied() else {
                    continue;
                };
                let stream_id = packet.stream_id();
                session.send_packet(stream_id, packet, operation)?;
                drain_decoder_before_flush(
                    session,
                    stream_id,
                    &mut active[index],
                    stages,
                    operation,
                )?;
            }
            ReadOutcome::Partial { report, .. } => {
                return Err(report.to_error("read_export_packet"));
            }
            ReadOutcome::EndOfStream => break,
            _ => {
                return Err(unsupported(
                    "read_export_packet",
                    "source returned an unknown read outcome",
                ));
            }
        }
    }

    for route in active.iter_mut() {
        let stream_id = route.route.source_stream_id;
        operation.check("flush_export_decoder")?;
        session.flush_decoder(stream_id, operation)?;
        drain_decoder_after_flush(session, stream_id, route, stages, operation)?;
        operation.check("flush_export_encoder")?;
        route.encoder.flush(operation)?;
        drain_encoder_after_flush(route, operation)?;
    }
    Ok(())
}

fn drain_decoder_before_flush<S, V>(
    session: &mut S,
    stream_id: StreamId,
    route: &mut ActiveRoute,
    stages: &mut RenderExportStages<'_, V>,
    operation: &OperationContext,
) -> Result<()>
where
    S: ExportMediaSession,
{
    for _ in 0..MAX_DRAIN_OUTPUTS {
        operation.check("drain_export_decoder")?;
        match session.receive_decoder(stream_id, operation)? {
            DecodeOutput::Frame(frame) => process_video(frame, route, stages, operation)?,
            DecodeOutput::Audio(block) => process_audio(block, route, stages, operation)?,
            DecodeOutput::NeedInput => return Ok(()),
            DecodeOutput::EndOfStream => {
                return Err(internal(
                    "drain_export_decoder",
                    "decoder reached end of stream before export flush",
                ));
            }
            _ => {
                return Err(unsupported(
                    "drain_export_decoder",
                    "decoder returned an unknown output variant",
                ));
            }
        }
    }
    Err(resource_exhausted(
        "drain_export_decoder",
        "decoder produced too many outputs without reaching a lifecycle boundary",
    ))
}

fn drain_decoder_after_flush<S, V>(
    session: &mut S,
    stream_id: StreamId,
    route: &mut ActiveRoute,
    stages: &mut RenderExportStages<'_, V>,
    operation: &OperationContext,
) -> Result<()>
where
    S: ExportMediaSession,
{
    for _ in 0..MAX_DRAIN_OUTPUTS {
        operation.check("drain_flushed_export_decoder")?;
        match session.receive_decoder(stream_id, operation)? {
            DecodeOutput::Frame(frame) => process_video(frame, route, stages, operation)?,
            DecodeOutput::Audio(block) => process_audio(block, route, stages, operation)?,
            DecodeOutput::EndOfStream => return Ok(()),
            DecodeOutput::NeedInput => {
                return Err(internal(
                    "drain_flushed_export_decoder",
                    "flushed decoder requested more export input",
                ));
            }
            _ => {
                return Err(unsupported(
                    "drain_flushed_export_decoder",
                    "decoder returned an unknown output variant",
                ));
            }
        }
    }
    Err(resource_exhausted(
        "drain_flushed_export_decoder",
        "flushed decoder produced too many outputs without ending",
    ))
}

fn process_video<V>(
    decoded: VideoFrame,
    route: &mut ActiveRoute,
    stages: &mut RenderExportStages<'_, V>,
    operation: &OperationContext,
) -> Result<()> {
    let EncoderMediaFormat::Video(expected_format) = route.route.encoder_config.media_format()
    else {
        return Err(corrupt(
            "route_export_video",
            "audio export route received decoded video",
        ));
    };
    let graph = stages.video_graph.as_deref_mut().ok_or_else(|| {
        invalid(
            "route_export_video",
            "video export requires a shared graph stage",
        )
    })?;
    let delivery = stages.video_delivery.as_deref_mut().ok_or_else(|| {
        invalid(
            "route_export_video",
            "video export requires a delivery color stage",
        )
    })?;

    operation.check("render_export_video")?;
    let rendered = graph.render(&decoded, operation)?;
    validate_graph_scene(
        rendered.scene(),
        &decoded,
        graph.scene_pipeline(),
        graph.scene_alpha_mode(),
    )?;
    if let Some(revision) = route.graph_revision {
        if revision != rendered.graph_revision {
            return Err(conflict(
                "render_export_video",
                "one export stream cannot mix immutable graph revisions",
            ));
        }
    } else {
        route.graph_revision = Some(rendered.graph_revision);
    }

    let scene = rendered.scene();
    if delivery.scene_pipeline() != scene.color_pipeline()
        || delivery.scene_alpha_mode() != scene.alpha_mode()
    {
        return Err(corrupt(
            "deliver_export_video",
            "delivery input does not match the graph scene color and alpha identity",
        ));
    }
    validate_delivery_pipeline(scene.color_pipeline(), delivery.output_pipeline())?;
    operation.check("deliver_export_video")?;
    let output = delivery.transform(scene, operation)?;
    if output.timestamp() != scene.timestamp() || output.duration() != scene.duration() {
        return Err(corrupt(
            "deliver_export_video",
            "delivery changed exact video timing",
        ));
    }
    if output.metadata() != decoded.metadata() {
        return Err(corrupt(
            "deliver_export_video",
            "delivery did not preserve decoded video metadata",
        ));
    }
    if output.color_pipeline().current_space() != expected_format.color_space()
        || output.color_pipeline().display_space().is_some()
    {
        return Err(corrupt(
            "deliver_export_video",
            "encoder-facing video color interpretation differs from delivery output",
        ));
    }
    if output.format() != *expected_format
        || output.format().alpha_mode() != delivery.output_alpha_mode()
    {
        return Err(corrupt(
            "deliver_export_video",
            "delivery output precision, format, or alpha differs from encoder configuration",
        ));
    }

    let provenance = ExportInputProvenance::Video(ExportVideoProvenance {
        timestamp: output.timestamp(),
        duration: output.duration(),
        decoded: scene.decoded().to_vec(),
        metadata: output.metadata().clone(),
        graph_revision: rendered.graph_revision,
        color_pipeline: delivery.output_pipeline().clone(),
        scene_alpha_mode: scene.alpha_mode(),
        output_alpha_mode: delivery.output_alpha_mode(),
        output_format: output.format(),
    });
    validate_next_input(route, &provenance)?;
    route.encoder.send(EncodeInput::Video(output), operation)?;
    route.inputs.push(provenance);
    drain_encoder_before_flush(route, operation)
}

fn process_audio<V>(
    decoded: AudioBlock,
    route: &mut ActiveRoute,
    stages: &mut RenderExportStages<'_, V>,
    operation: &OperationContext,
) -> Result<()> {
    let EncoderMediaFormat::Audio(expected_format) = route.route.encoder_config.media_format()
    else {
        return Err(corrupt(
            "route_export_audio",
            "video export route received decoded audio",
        ));
    };
    let graph = stages.audio_graph.as_deref_mut().ok_or_else(|| {
        invalid(
            "route_export_audio",
            "audio export requires a prepared audio graph stage",
        )
    })?;
    operation.check("render_export_audio")?;
    let rendered = graph.render(&decoded, operation)?;
    let output = rendered.block();
    if output.timestamp() != decoded.timestamp()
        || output.duration() != decoded.duration()
        || output.metadata() != decoded.metadata()
    {
        return Err(corrupt(
            "render_export_audio",
            "audio graph changed exact timing or source metadata",
        ));
    }
    if output.format() != expected_format {
        return Err(corrupt(
            "render_export_audio",
            "audio graph output representation differs from encoder configuration",
        ));
    }
    if let Some(graph_id) = route.audio_graph_id {
        if graph_id != rendered.graph_id {
            return Err(conflict(
                "render_export_audio",
                "one export stream cannot mix prepared audio graph identities",
            ));
        }
    } else {
        route.audio_graph_id = Some(rendered.graph_id);
    }

    let provenance = ExportInputProvenance::Audio(ExportAudioProvenance {
        timestamp: output.timestamp(),
        duration: output.duration(),
        metadata: output.metadata().clone(),
        graph_id: rendered.graph_id,
        output_format: output.format().clone(),
    });
    validate_next_input(route, &provenance)?;
    route
        .encoder
        .send(EncodeInput::Audio(output.clone()), operation)?;
    route.inputs.push(provenance);
    drain_encoder_before_flush(route, operation)
}

fn drain_encoder_before_flush(route: &mut ActiveRoute, operation: &OperationContext) -> Result<()> {
    for _ in 0..MAX_DRAIN_OUTPUTS {
        operation.check("drain_export_encoder")?;
        match route.encoder.receive(operation)? {
            EncodeOutput::Packet(packet) => route.packets.push(packet),
            EncodeOutput::NeedInput => return Ok(()),
            EncodeOutput::EndOfStream => {
                return Err(internal(
                    "drain_export_encoder",
                    "encoder reached end of stream before export flush",
                ));
            }
            _ => {
                return Err(unsupported(
                    "drain_export_encoder",
                    "encoder returned an unknown output variant",
                ));
            }
        }
    }
    Err(resource_exhausted(
        "drain_export_encoder",
        "encoder produced too many packets without reaching a lifecycle boundary",
    ))
}

fn drain_encoder_after_flush(route: &mut ActiveRoute, operation: &OperationContext) -> Result<()> {
    for _ in 0..MAX_DRAIN_OUTPUTS {
        operation.check("drain_flushed_export_encoder")?;
        match route.encoder.receive(operation)? {
            EncodeOutput::Packet(packet) => route.packets.push(packet),
            EncodeOutput::EndOfStream => return Ok(()),
            EncodeOutput::NeedInput => {
                return Err(internal(
                    "drain_flushed_export_encoder",
                    "flushed encoder requested more export input",
                ));
            }
            _ => {
                return Err(unsupported(
                    "drain_flushed_export_encoder",
                    "encoder returned an unknown output variant",
                ));
            }
        }
    }
    Err(resource_exhausted(
        "drain_flushed_export_encoder",
        "flushed encoder produced too many packets without ending",
    ))
}

fn validate_session_routes<S, V>(
    request: &RenderExportRequest,
    session: &S,
    stages: &RenderExportStages<'_, V>,
) -> Result<()>
where
    S: ExportMediaSession,
{
    let source_streams = session
        .source_info()
        .streams()
        .iter()
        .map(|stream| (stream.id(), stream.kind()))
        .collect::<BTreeMap<_, _>>();
    for route in &request.routes {
        let Some(kind) = source_streams.get(&route.source_stream_id).copied() else {
            return Err(invalid(
                "validate_export_routes",
                "export route names a source stream that does not exist",
            ));
        };
        let config = session
            .decoder_config(route.source_stream_id)
            .ok_or_else(|| {
                invalid(
                    "validate_export_routes",
                    "export route has no prepared decoder",
                )
            })?;
        if config.stream().id() != route.source_stream_id || config.stream().kind() != kind {
            return Err(conflict(
                "validate_export_routes",
                "prepared decoder does not match its export route",
            ));
        }
        match (kind, route.encoder_config.media_format()) {
            (StreamKind::Video, EncoderMediaFormat::Video(format)) => {
                let graph = stages.video_graph.as_deref().ok_or_else(|| {
                    invalid(
                        "validate_export_routes",
                        "video route requires a graph stage",
                    )
                })?;
                let delivery = stages.video_delivery.as_deref().ok_or_else(|| {
                    invalid(
                        "validate_export_routes",
                        "video route requires a delivery stage",
                    )
                })?;
                validate_nonterminal_pipeline(graph.scene_pipeline(), "validate_export_routes")?;
                if graph.scene_pipeline() != delivery.scene_pipeline()
                    || graph.scene_alpha_mode() != delivery.scene_alpha_mode()
                {
                    return Err(invalid(
                        "validate_export_routes",
                        "video graph and delivery stages disagree on scene color or alpha",
                    ));
                }
                validate_delivery_pipeline(delivery.scene_pipeline(), delivery.output_pipeline())?;
                if delivery.output_pipeline().delivery_space() != Some(format.color_space()) {
                    return Err(invalid(
                        "validate_export_routes",
                        "encoder color interpretation differs from the terminal delivery space",
                    ));
                }
                if format.alpha_mode() != delivery.output_alpha_mode() {
                    return Err(invalid(
                        "validate_export_routes",
                        "delivery alpha declaration differs from the encoder format",
                    ));
                }
            }
            (StreamKind::Audio, EncoderMediaFormat::Audio(_)) => {
                if stages.audio_graph.is_none() {
                    return Err(invalid(
                        "validate_export_routes",
                        "audio route requires a prepared audio graph stage",
                    ));
                }
            }
            _ => {
                return Err(invalid(
                    "validate_export_routes",
                    "source stream kind differs from encoder media kind",
                ));
            }
        }
    }
    Ok(())
}

fn validate_graph_scene<V>(
    scene: &PlaybackSceneFrame<V>,
    decoded: &VideoFrame,
    expected_pipeline: &ColorPipelineMetadata,
    expected_alpha: AlphaMode,
) -> Result<()> {
    if scene.timestamp() != decoded.timestamp()
        || scene.duration() != decoded.duration()
        || scene.color_pipeline() != expected_pipeline
        || scene.alpha_mode() != expected_alpha
    {
        return Err(corrupt(
            "validate_export_graph_scene",
            "graph scene changed exact timing, color, or alpha identity",
        ));
    }
    let expected = DecodedFrameMetadata::from_frame(decoded);
    if !scene.decoded().contains(&expected) {
        return Err(corrupt(
            "validate_export_graph_scene",
            "graph scene omitted exact decoded-frame provenance",
        ));
    }
    validate_nonterminal_pipeline(scene.color_pipeline(), "validate_export_graph_scene")
}

fn validate_nonterminal_pipeline(
    pipeline: &ColorPipelineMetadata,
    operation: &'static str,
) -> Result<()> {
    if pipeline.display_space().is_some() || pipeline.delivery_space().is_some() {
        return Err(invalid(
            operation,
            "shared graph scene color must remain nonterminal",
        ));
    }
    Ok(())
}

fn validate_delivery_pipeline(
    scene: &ColorPipelineMetadata,
    output: &ColorPipelineMetadata,
) -> Result<()> {
    if output.source_tags() != scene.source_tags()
        || !output.stages().starts_with(scene.stages())
        || output.display_space().is_some()
        || output.delivery_space().is_none()
    {
        return Err(invalid(
            "validate_export_delivery",
            "delivery color must append one terminal output branch to the shared scene history",
        ));
    }
    Ok(())
}

fn validate_next_input(route: &mut ActiveRoute, input: &ExportInputProvenance) -> Result<()> {
    let range = input.time_range()?;
    if range.is_empty() {
        return Err(corrupt(
            "validate_export_input_timing",
            "encoder input duration must be nonzero",
        ));
    }
    if let Some(previous_start) = route.last_input_start {
        if range.start() < previous_start {
            return Err(corrupt(
                "validate_export_input_timing",
                "encoder inputs must remain in presentation order",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compare_export_input_timing")
                    .with_field("previous_start", previous_start.to_string())
                    .with_field("current_start", range.start().to_string()),
            ));
        }
    }
    route.last_input_start = Some(range.start());
    Ok(())
}

fn validate_complete_stream(route: &ActiveRoute) -> Result<()> {
    if route.inputs.is_empty() || route.packets.is_empty() {
        return Err(corrupt(
            "validate_export_stream",
            "complete export streams require encoder inputs and packets",
        ));
    }
    let mut packet_ranges = Vec::with_capacity(route.packets.len());
    for packet in &route.packets {
        if packet.stream_id() != route.route.encoder_config.stream_id()
            || packet.timing().timebase() != route.route.encoder_config.timebase()
            || packet.data().is_empty()
        {
            return Err(corrupt(
                "validate_export_stream",
                "encoder packet stream, timebase, or payload differs from its route",
            ));
        }
        let timestamp = packet.timing().presentation_time().ok_or_else(|| {
            corrupt(
                "validate_export_stream",
                "export encoder packet omitted presentation timing",
            )
        })?;
        let duration = packet.timing().duration().ok_or_else(|| {
            corrupt(
                "validate_export_stream",
                "export encoder packet omitted duration",
            )
        })?;
        if duration.is_zero() {
            return Err(corrupt(
                "validate_export_stream",
                "export encoder packet duration must be nonzero",
            ));
        }
        packet_ranges.push(TimeRange::new(timestamp, duration)?);
    }

    let input_ranges = route
        .inputs
        .iter()
        .map(ExportInputProvenance::time_range)
        .collect::<Result<Vec<_>>>()?;
    let input_coverage = normalized_coverage(&input_ranges)?;
    let packet_coverage = normalized_coverage(&packet_ranges)?;
    if input_coverage != packet_coverage {
        return Err(corrupt(
            "validate_export_stream",
            "encoded packets do not preserve the exact union of input intervals",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "compare_export_packet_coverage")
                .with_field("input_ranges", input_ranges.len().to_string())
                .with_field("packet_ranges", packet_ranges.len().to_string())
                .with_field("input_coverage", coverage_prefix(&input_coverage))
                .with_field("packet_coverage", coverage_prefix(&packet_coverage)),
        ));
    }

    let mut packet_order = (0..packet_ranges.len()).collect::<Vec<_>>();
    packet_order.sort_by(|left, right| {
        packet_ranges[*left]
            .start()
            .partial_cmp(&packet_ranges[*right].start())
            .expect("exact rational time ordering is total")
    });
    let mut first_candidate = 0_usize;
    for (input, range) in route.inputs.iter().zip(input_ranges.iter()) {
        if input.metadata().is_empty() {
            continue;
        }
        while first_candidate < packet_order.len()
            && packet_ranges[packet_order[first_candidate]].end_exclusive()? <= range.start()
        {
            first_candidate += 1;
        }
        let input_end = range.end_exclusive()?;
        let preserved = packet_order[first_candidate..]
            .iter()
            .take_while(|index| packet_ranges[**index].start() < input_end)
            .any(|index| {
                packet_ranges[*index].intersects(*range).unwrap_or(false)
                    && metadata_contains(route.packets[*index].metadata(), input.metadata())
            });
        if !preserved {
            return Err(corrupt(
                "validate_export_stream",
                "encoder packets did not preserve input metadata",
            ));
        }
    }
    Ok(())
}

fn normalized_coverage(ranges: &[TimeRange]) -> Result<Vec<(RationalTime, RationalTime)>> {
    let mut ranges = ranges.to_vec();
    ranges.sort_by(|left, right| {
        left.start()
            .partial_cmp(&right.start())
            .expect("exact rational time ordering is total")
    });
    let mut coverage: Vec<(RationalTime, RationalTime)> = Vec::new();
    for range in ranges {
        let start = range.start();
        let end = range.end_exclusive()?;
        if let Some((_, current_end)) = coverage.last_mut() {
            if start <= *current_end {
                if end > *current_end {
                    *current_end = end;
                }
                continue;
            }
        }
        coverage.push((start, end));
    }
    Ok(coverage)
}

fn coverage_prefix(coverage: &[(RationalTime, RationalTime)]) -> String {
    coverage
        .iter()
        .take(3)
        .map(|(start, end)| format!("{start}..{end}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn metadata_contains(candidate: &MediaMetadata, expected: &MediaMetadata) -> bool {
    expected
        .iter()
        .all(|(key, value)| candidate.get(key) == Some(value))
}

fn validate_permit(
    snapshot: &EngineLifecycleSnapshot,
    permit: EngineWorkPermit,
    operation: &'static str,
) -> Result<()> {
    if permit.work() != EngineWorkKind::Export || !snapshot.is_permit_current(permit) {
        return Err(conflict(
            operation,
            "export lifecycle permit is stale or current dependencies are unavailable",
        ));
    }
    Ok(())
}

fn reset_transaction<S>(
    session: &mut S,
    active: &mut [ActiveRoute],
    operation: &OperationContext,
) -> Result<()>
where
    S: ExportMediaSession,
{
    for route in active {
        operation.check("reset_completed_export")?;
        route.encoder.reset(operation)?;
        session.reset_decoder(route.route.source_stream_id, operation)?;
    }
    Ok(())
}

fn recover_transaction<S>(session: &mut S, active: &mut [ActiveRoute])
where
    S: ExportMediaSession,
{
    let recovery = OperationContext::new(MediaPriority::Export);
    for route in active {
        let _ = route.encoder.reset(&recovery);
        let _ = session.reset_decoder(route.route.source_stream_id, &recovery);
    }
}

fn recover_encoders(active: &mut [ActiveRoute]) {
    let recovery = OperationContext::new(MediaPriority::Export);
    for route in active {
        let _ = route.encoder.reset(&recovery);
    }
}

fn acquired_decoder_mut(
    source: &mut AcquiredMediaSource,
    stream_id: StreamId,
) -> Result<&mut crate::resources::AcquiredDecoder> {
    source.decoder_mut(stream_id).ok_or_else(|| {
        invalid(
            "access_export_decoder",
            "prepared source does not contain the requested export decoder",
        )
    })
}

fn with_stream_context(mut error: Error, operation: &'static str, stream_id: StreamId) -> Error {
    error.push_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("source_stream_id", stream_id.value().to_string()),
    );
    error
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    export_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    export_error(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        operation,
        message,
    )
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    export_error(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    export_error(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
        operation,
        message,
    )
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    export_error(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        operation,
        message,
    )
}

fn resource_exhausted(operation: &'static str, message: &'static str) -> Error {
    export_error(
        ErrorCategory::ResourceExhausted,
        Recoverability::Degraded,
        operation,
        message,
    )
}

fn export_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
