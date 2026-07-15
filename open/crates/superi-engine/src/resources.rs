//! Transactional timeline graph and media resource acquisition.
//!
//! Preparation either compiles one reachable editorial timeline graph or retains the exact root
//! compilation from an immutable project snapshot. It opens every source used by that graph,
//! selects explicit stream decoders, and publishes the resulting owners only after every step
//! succeeds. Playback, render, export, scheduling, and resource arbitration consume this shared
//! bundle but remain separate orchestration responsibilities.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{MediaId, TimelineId};
use superi_media_io::audio_io::AudioFormat;
use superi_media_io::backend::{
    BackendRegistry, BackendRequirement, FallbackPolicy, SourceProbeCandidate, SourceProbeSelection,
};
use superi_media_io::decode::{Decoder, DecoderConfig};
use superi_media_io::demux::{
    BackendId, ContainerId, MediaSource, ProbeConfidence, SourceInfo, SourceLocation,
    SourceProbeLimits, SourceRequest, StreamId,
};
use superi_media_io::operation::OperationContext;
use superi_project::document::ProjectSnapshot;
use superi_project::media::ReferencedMediaPath;
use superi_timeline::compile::{compile_timeline, TimelineGraphCompilation};
use superi_timeline::media::{LinkedMediaReference, RelinkStatus};
use superi_timeline::model::{ClipSource, EditorialProject};

const COMPONENT: &str = "superi-engine.resources";

/// One source stream that must have a decoder ready before the bundle is published.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecoderResourceRequest {
    stream_id: StreamId,
    audio_format: Option<AudioFormat>,
}

impl DecoderResourceRequest {
    /// Requests the decoder for one source-local stream.
    #[must_use]
    pub const fn new(stream_id: StreamId) -> Self {
        Self {
            stream_id,
            audio_format: None,
        }
    }

    /// Supplies the decoded representation required by a headerless audio codec.
    #[must_use]
    pub fn with_audio_format(mut self, audio_format: AudioFormat) -> Self {
        self.audio_format = Some(audio_format);
        self
    }

    /// Returns the source-local stream identity.
    #[must_use]
    pub const fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// Returns the caller-supplied decoded audio representation, when needed.
    #[must_use]
    pub const fn audio_format(&self) -> Option<&AudioFormat> {
        self.audio_format.as_ref()
    }
}

/// The explicit source and stream resources required for one reachable media identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaResourceRequest {
    source: SourceRequest,
    decoders: BTreeMap<StreamId, DecoderResourceRequest>,
}

impl MediaResourceRequest {
    /// Creates a request with at least one uniquely identified decoder stream.
    pub fn new<I>(source: SourceRequest, decoders: I) -> Result<Self>
    where
        I: IntoIterator<Item = DecoderResourceRequest>,
    {
        let mut requests = BTreeMap::new();
        for request in decoders {
            let stream_id = request.stream_id();
            if requests.insert(stream_id, request).is_some() {
                return Err(resource_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "create_media_resource_request",
                    "media resource request contains duplicate decoder streams",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "identify_duplicate_decoder_stream")
                        .with_field("media_id", media_id_text(source.media_id()))
                        .with_field("stream_id", stream_id.value().to_string()),
                ));
            }
        }
        if requests.is_empty() {
            return Err(resource_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "create_media_resource_request",
                "media resource request must select at least one decoder stream",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "require_decoder_stream")
                    .with_field("media_id", media_id_text(source.media_id())),
            ));
        }
        Ok(Self {
            source,
            decoders: requests,
        })
    }

    /// Creates a request from one persistent filesystem target in an editorial project.
    ///
    /// Relative targets resolve from the absolute path of the owning `.superi` file. The stable
    /// MediaId and expected content fingerprint come from project state, so acquisition cannot
    /// substitute a caller-selected path or identity while claiming to open this reference.
    pub fn from_project_media<I>(
        project: &EditorialProject,
        project_file: impl AsRef<Path>,
        media_id: MediaId,
        decoders: I,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = DecoderResourceRequest>,
    {
        let linked_media = project.media_reference(media_id).ok_or_else(|| {
            resource_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "create_project_media_resource_request",
                "linked media identity was not found in the project",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "resolve_project_media_reference")
                    .with_field("media_id", media_id_text(media_id)),
            )
        })?;
        if linked_media.relink_state().status() == RelinkStatus::Missing {
            return Err(resource_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "create_project_media_resource_request",
                "linked media target is marked missing in project state",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "resolve_project_media_reference")
                    .with_field("media_id", media_id_text(media_id))
                    .with_field("target", linked_media.target().to_owned()),
            ));
        }
        let path = ReferencedMediaPath::from_target(linked_media.target())?.ok_or_else(|| {
            resource_error(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "create_project_media_resource_request",
                "linked media target is not a supported filesystem path",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "resolve_project_media_target")
                    .with_field("media_id", media_id_text(media_id))
                    .with_field("target", linked_media.target().to_owned()),
            )
        })?;
        let location = SourceLocation::Path(path.resolve(project_file)?);
        let mut source = SourceRequest::new(media_id, location);
        if let Some(fingerprint) = linked_media.relink_state().expected_fingerprint() {
            source = source.with_expected_fingerprint(fingerprint.to_owned())?;
        }
        Self::new(source, decoders)
    }

    /// Returns the immutable source request.
    #[must_use]
    pub const fn source(&self) -> &SourceRequest {
        &self.source
    }

    /// Iterates decoder requests in source-local stream order.
    pub fn decoders(
        &self,
    ) -> impl ExactSizeIterator<Item = &DecoderResourceRequest> + DoubleEndedIterator {
        self.decoders.values()
    }
}

/// Probe and fallback policy applied consistently to one preparation transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceAcquisitionPolicy {
    probe_limits: SourceProbeLimits,
    source_fallback: FallbackPolicy,
    decoder_fallback: FallbackPolicy,
}

impl ResourceAcquisitionPolicy {
    /// Creates an explicit source probing and backend fallback policy.
    #[must_use]
    pub const fn new(
        probe_limits: SourceProbeLimits,
        source_fallback: FallbackPolicy,
        decoder_fallback: FallbackPolicy,
    ) -> Self {
        Self {
            probe_limits,
            source_fallback,
            decoder_fallback,
        }
    }

    /// Returns the bounded source prefix policy.
    #[must_use]
    pub const fn probe_limits(self) -> SourceProbeLimits {
        self.probe_limits
    }

    /// Returns whether registered fallback source backends are eligible.
    #[must_use]
    pub const fn source_fallback(self) -> FallbackPolicy {
        self.source_fallback
    }

    /// Returns whether registered fallback decoder backends are eligible.
    #[must_use]
    pub const fn decoder_fallback(self) -> FallbackPolicy {
        self.decoder_fallback
    }
}

impl Default for ResourceAcquisitionPolicy {
    fn default() -> Self {
        Self::new(
            SourceProbeLimits::default(),
            FallbackPolicy::Disallow,
            FallbackPolicy::Disallow,
        )
    }
}

/// Stable evidence for one content-recognizing source backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceCandidateEvidence {
    backend_id: BackendId,
    container_id: ContainerId,
    confidence: ProbeConfidence,
}

impl SourceCandidateEvidence {
    fn from_candidate(candidate: &SourceProbeCandidate) -> Self {
        Self {
            backend_id: candidate.backend().descriptor().id().clone(),
            container_id: candidate.container().clone(),
            confidence: candidate.confidence(),
        }
    }

    /// Returns the stable backend identity.
    #[must_use]
    pub const fn backend_id(&self) -> &BackendId {
        &self.backend_id
    }

    /// Returns the recognized container identity.
    #[must_use]
    pub const fn container_id(&self) -> &ContainerId {
        &self.container_id
    }

    /// Returns the content-probe confidence.
    #[must_use]
    pub const fn confidence(&self) -> ProbeConfidence {
        self.confidence
    }
}

/// Immutable source selection and bounded-probe evidence retained with an open source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSelectionEvidence {
    selected: SourceCandidateEvidence,
    fallbacks: Vec<SourceCandidateEvidence>,
    fallback_used: bool,
    bytes_examined: usize,
    source_length: u64,
}

impl SourceSelectionEvidence {
    fn from_selection(selection: &SourceProbeSelection) -> Self {
        Self {
            selected: SourceCandidateEvidence::from_candidate(selection.primary()),
            fallbacks: selection
                .fallbacks()
                .iter()
                .map(SourceCandidateEvidence::from_candidate)
                .collect(),
            fallback_used: selection.fallback_used(),
            bytes_examined: selection.bytes_examined(),
            source_length: selection.source_length(),
        }
    }

    /// Returns the backend and container selected by content probing.
    #[must_use]
    pub const fn selected(&self) -> &SourceCandidateEvidence {
        &self.selected
    }

    /// Returns explicitly eligible fallback-tier content matches.
    #[must_use]
    pub fn fallbacks(&self) -> &[SourceCandidateEvidence] {
        &self.fallbacks
    }

    /// Returns whether source opening required fallback-tier permission.
    #[must_use]
    pub const fn fallback_used(&self) -> bool {
        self.fallback_used
    }

    /// Returns the bounded prefix length inspected during source selection.
    #[must_use]
    pub const fn bytes_examined(&self) -> usize {
        self.bytes_examined
    }

    /// Returns the complete source length observed before probing.
    #[must_use]
    pub const fn source_length(&self) -> u64 {
        self.source_length
    }
}

/// Immutable decoder backend selection evidence retained with a live decoder.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecoderSelectionEvidence {
    selected_backend_id: BackendId,
    fallback_backend_ids: Vec<BackendId>,
    fallback_used: bool,
}

impl DecoderSelectionEvidence {
    /// Returns the selected decoder backend identity.
    #[must_use]
    pub const fn selected_backend_id(&self) -> &BackendId {
        &self.selected_backend_id
    }

    /// Returns eligible fallback candidates in deterministic selection order.
    #[must_use]
    pub fn fallback_backend_ids(&self) -> &[BackendId] {
        &self.fallback_backend_ids
    }

    /// Returns whether decoder creation required fallback-tier permission.
    #[must_use]
    pub const fn fallback_used(&self) -> bool {
        self.fallback_used
    }
}

/// One configured live decoder plus the policy evidence that selected it.
pub struct AcquiredDecoder {
    config: DecoderConfig,
    selection: DecoderSelectionEvidence,
    decoder: Box<dyn Decoder>,
}

impl AcquiredDecoder {
    /// Returns the exact source stream and optional decoded audio representation.
    #[must_use]
    pub const fn config(&self) -> &DecoderConfig {
        &self.config
    }

    /// Returns immutable backend selection evidence.
    #[must_use]
    pub const fn selection(&self) -> &DecoderSelectionEvidence {
        &self.selection
    }

    /// Returns mutable access to the stateful decoder lifecycle.
    pub fn decoder_mut(&mut self) -> &mut (dyn Decoder + '_) {
        self.decoder.as_mut()
    }
}

impl fmt::Debug for AcquiredDecoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcquiredDecoder")
            .field("config", &self.config)
            .field("selection", &self.selection)
            .finish_non_exhaustive()
    }
}

/// One opened source and every explicitly requested decoder for its streams.
pub struct AcquiredMediaSource {
    source_selection: SourceSelectionEvidence,
    source: Box<dyn MediaSource>,
    decoders: BTreeMap<StreamId, AcquiredDecoder>,
}

impl AcquiredMediaSource {
    /// Returns immutable source identity, timing, streams, and metadata.
    #[must_use]
    pub fn info(&self) -> &SourceInfo {
        self.source.info()
    }

    /// Returns immutable source selection and probing evidence.
    #[must_use]
    pub const fn source_selection(&self) -> &SourceSelectionEvidence {
        &self.source_selection
    }

    /// Returns mutable access to packet reads and source seeking.
    pub fn source_mut(&mut self) -> &mut (dyn MediaSource + '_) {
        self.source.as_mut()
    }

    /// Looks up one configured decoder without exposing its mutable lifecycle.
    #[must_use]
    pub fn decoder(&self, stream_id: StreamId) -> Option<&AcquiredDecoder> {
        self.decoders.get(&stream_id)
    }

    /// Looks up one configured decoder for packet submission and output draining.
    pub fn decoder_mut(&mut self, stream_id: StreamId) -> Option<&mut AcquiredDecoder> {
        self.decoders.get_mut(&stream_id)
    }

    /// Iterates configured decoders in source-local stream order.
    pub fn decoders(&self) -> impl ExactSizeIterator<Item = (StreamId, &AcquiredDecoder)> {
        self.decoders
            .iter()
            .map(|(stream_id, decoder)| (*stream_id, decoder))
    }
}

impl fmt::Debug for AcquiredMediaSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AcquiredMediaSource")
            .field("info", self.info())
            .field("source_selection", &self.source_selection)
            .field("decoders", &self.decoders)
            .finish_non_exhaustive()
    }
}

/// A complete compiled timeline and its all-or-nothing acquired media owners.
#[derive(Debug)]
pub struct TimelineResources {
    compilation: TimelineGraphCompilation,
    media: BTreeMap<MediaId, AcquiredMediaSource>,
}

impl TimelineResources {
    /// Returns the single graph compilation shared by playback, render, and export consumers.
    #[must_use]
    pub const fn compilation(&self) -> &TimelineGraphCompilation {
        &self.compilation
    }

    /// Returns mutable graph access for later checked engine transactions.
    pub fn compilation_mut(&mut self) -> &mut TimelineGraphCompilation {
        &mut self.compilation
    }

    /// Looks up one opened reachable media identity.
    #[must_use]
    pub fn media(&self, media_id: MediaId) -> Option<&AcquiredMediaSource> {
        self.media.get(&media_id)
    }

    /// Looks up one opened source for packet and decoder lifecycle operations.
    pub fn media_mut(&mut self, media_id: MediaId) -> Option<&mut AcquiredMediaSource> {
        self.media.get_mut(&media_id)
    }

    /// Iterates acquired media in stable project identity order.
    pub fn media_resources(
        &self,
    ) -> impl ExactSizeIterator<Item = (MediaId, &AcquiredMediaSource)> {
        self.media
            .iter()
            .map(|(media_id, resource)| (*media_id, resource))
    }
}

/// Compiles one timeline and acquires its exact source and decoder set transactionally.
///
/// Every reachable linked media identity must have exactly one request, and every request must
/// select at least one unique stream. Content probing and decoder ranking may expose registered
/// fallback evidence, but a selected backend failure is returned directly and never retried through
/// another implementation. The returned bundle is the only publication point.
pub fn acquire_timeline_resources<I>(
    project: &EditorialProject,
    root_timeline_id: TimelineId,
    registry: &BackendRegistry,
    requests: I,
    policy: ResourceAcquisitionPolicy,
    operation: &OperationContext,
) -> Result<TimelineResources>
where
    I: IntoIterator<Item = MediaResourceRequest>,
{
    operation.check("acquire_timeline_resources")?;
    let compilation = compile_timeline(project, root_timeline_id)?;
    operation.check("acquire_timeline_resources")?;

    acquire_compiled_resources(project, compilation, registry, requests, policy, operation)
}

/// Acquires resources for the exact root graph retained by a project snapshot.
///
/// Direct graph edits, generated results, and editorial provenance remain
/// unchanged because this path clones the already published compilation. It
/// never recompiles the snapshot. Media source and decoder owners are still
/// published only after the complete transaction succeeds.
pub fn acquire_project_resources<I>(
    project: &ProjectSnapshot,
    registry: &BackendRegistry,
    requests: I,
    policy: ResourceAcquisitionPolicy,
    operation: &OperationContext,
) -> Result<TimelineResources>
where
    I: IntoIterator<Item = MediaResourceRequest>,
{
    operation.check("acquire_project_resources")?;
    let root_timeline_id = project.root_timeline_id();
    let compilation = project
        .timeline_graph(root_timeline_id)
        .ok_or_else(|| {
            resource_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "acquire_project_resources",
                "project snapshot has no retained root timeline compilation",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "resolve_project_root_compilation")
                    .with_field("project_id", project.project_id().to_string())
                    .with_field("timeline_id", root_timeline_id.to_string()),
            )
        })?
        .clone();
    operation.check("acquire_project_resources")?;

    acquire_compiled_resources(
        project.editorial_project(),
        compilation,
        registry,
        requests,
        policy,
        operation,
    )
}

fn acquire_compiled_resources<I>(
    project: &EditorialProject,
    compilation: TimelineGraphCompilation,
    registry: &BackendRegistry,
    requests: I,
    policy: ResourceAcquisitionPolicy,
    operation: &OperationContext,
) -> Result<TimelineResources>
where
    I: IntoIterator<Item = MediaResourceRequest>,
{
    let root_timeline_id = compilation.root_timeline_id();

    let required_media = reachable_media(project, root_timeline_id)?;
    let requests = index_requests(requests)?;
    validate_exact_request_set(&required_media, &requests)?;

    let mut acquired = BTreeMap::new();
    for media_id in required_media {
        operation.check("acquire_timeline_media")?;
        let linked_media = project.media_reference(media_id).ok_or_else(|| {
            resource_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "acquire_timeline_media",
                "reachable timeline media is missing from the project",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "resolve_linked_media")
                    .with_field("media_id", media_id_text(media_id)),
            )
        })?;
        let request = requests.get(&media_id).ok_or_else(|| {
            resource_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "acquire_timeline_media",
                "validated media request disappeared before acquisition",
            )
        })?;
        let source_request = bind_project_fingerprint(request.source.clone(), linked_media)?;
        let probe_selection = registry.probe_source(
            source_request,
            policy.probe_limits(),
            policy.source_fallback(),
            operation,
        )?;
        let source_selection = SourceSelectionEvidence::from_selection(&probe_selection);
        let source = probe_selection.open(operation)?;
        verify_source_identity(source.info(), linked_media)?;
        let decoders = acquire_decoders(
            registry,
            source.info(),
            &request.decoders,
            policy.decoder_fallback(),
            operation,
        )?;
        acquired.insert(
            media_id,
            AcquiredMediaSource {
                source_selection,
                source,
                decoders,
            },
        );
    }

    operation.check("publish_timeline_resources")?;
    Ok(TimelineResources {
        compilation,
        media: acquired,
    })
}

fn reachable_media(
    project: &EditorialProject,
    root_timeline_id: TimelineId,
) -> Result<BTreeSet<MediaId>> {
    let mut pending = vec![root_timeline_id];
    let mut visited = BTreeSet::new();
    let mut media = BTreeSet::new();
    while let Some(timeline_id) = pending.pop() {
        if !visited.insert(timeline_id) {
            continue;
        }
        let timeline = project.timeline(timeline_id).ok_or_else(|| {
            resource_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "collect_reachable_media",
                "reachable timeline is missing from the project",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "resolve_reachable_timeline")
                    .with_field("timeline_id", timeline_id.to_string()),
            )
        })?;
        for track in timeline.tracks() {
            for clip in track.items().iter().filter_map(|item| item.as_clip()) {
                match clip.source() {
                    ClipSource::Media(media_id) => {
                        media.insert(media_id);
                    }
                    ClipSource::Timeline(nested_id) => pending.push(nested_id),
                    _ => {
                        return Err(resource_error(
                            ErrorCategory::Unsupported,
                            Recoverability::UserCorrectable,
                            "collect_reachable_media",
                            "timeline contains a clip source without a resource acquisition path",
                        )
                        .with_context(
                            ErrorContext::new(COMPONENT, "identify_clip_source")
                                .with_field("clip_id", clip.id().to_string()),
                        ));
                    }
                }
            }
        }
    }
    Ok(media)
}

fn index_requests<I>(requests: I) -> Result<BTreeMap<MediaId, MediaResourceRequest>>
where
    I: IntoIterator<Item = MediaResourceRequest>,
{
    let mut indexed = BTreeMap::new();
    for request in requests {
        let media_id = request.source.media_id();
        if indexed.insert(media_id, request).is_some() {
            return Err(resource_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "index_media_resource_requests",
                "timeline resource acquisition contains duplicate media requests",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "identify_duplicate_media_request")
                    .with_field("media_id", media_id_text(media_id)),
            ));
        }
    }
    Ok(indexed)
}

fn validate_exact_request_set(
    required: &BTreeSet<MediaId>,
    requests: &BTreeMap<MediaId, MediaResourceRequest>,
) -> Result<()> {
    if let Some(missing) = required
        .iter()
        .find(|media_id| !requests.contains_key(media_id))
    {
        return Err(resource_error(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "validate_media_resource_requests",
            "reachable timeline media has no resource request",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "identify_missing_media_request")
                .with_field("media_id", media_id_text(*missing)),
        ));
    }
    if let Some(extra) = requests
        .keys()
        .find(|media_id| !required.contains(media_id))
    {
        return Err(resource_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "validate_media_resource_requests",
            "resource request does not belong to the reachable timeline graph",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "identify_extra_media_request")
                .with_field("media_id", media_id_text(*extra)),
        ));
    }
    Ok(())
}

fn bind_project_fingerprint(
    request: SourceRequest,
    linked_media: &LinkedMediaReference,
) -> Result<SourceRequest> {
    let expected = linked_media
        .relink_state()
        .expected_fingerprint()
        .map(str::to_owned);
    match (request.expected_fingerprint(), expected.as_deref()) {
        (Some(requested), Some(project)) if requested != project => Err(resource_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "bind_project_fingerprint",
            "source request conflicts with the project's persistent content identity",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "compare_expected_fingerprint")
                .with_field("media_id", media_id_text(linked_media.id()))
                .with_field("project_fingerprint", project)
                .with_field("request_fingerprint", requested),
        )),
        (None, Some(project)) => request.with_expected_fingerprint(project),
        _ => Ok(request),
    }
}

fn verify_source_identity(info: &SourceInfo, linked_media: &LinkedMediaReference) -> Result<()> {
    if info.identity().media_id() != linked_media.id() {
        return Err(resource_error(
            ErrorCategory::CorruptData,
            Recoverability::Terminal,
            "verify_opened_source_identity",
            "opened source returned a different project media identity",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "compare_opened_media_id")
                .with_field("expected_media_id", media_id_text(linked_media.id()))
                .with_field("actual_media_id", media_id_text(info.identity().media_id())),
        ));
    }
    if let Some(expected) = linked_media.relink_state().expected_fingerprint() {
        if info.identity().fingerprint() != expected {
            return Err(resource_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "verify_opened_source_identity",
                "opened source does not match the project's persistent content identity",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compare_opened_fingerprint")
                    .with_field("media_id", media_id_text(linked_media.id()))
                    .with_field("expected_fingerprint", expected)
                    .with_field("actual_fingerprint", info.identity().fingerprint()),
            ));
        }
    }
    Ok(())
}

fn acquire_decoders(
    registry: &BackendRegistry,
    source_info: &SourceInfo,
    requests: &BTreeMap<StreamId, DecoderResourceRequest>,
    fallback_policy: FallbackPolicy,
    operation: &OperationContext,
) -> Result<BTreeMap<StreamId, AcquiredDecoder>> {
    let mut acquired = BTreeMap::new();
    for request in requests.values() {
        operation.check("acquire_stream_decoder")?;
        let stream = source_info
            .streams()
            .iter()
            .find(|stream| stream.id() == request.stream_id())
            .ok_or_else(|| {
                resource_error(
                    ErrorCategory::NotFound,
                    Recoverability::UserCorrectable,
                    "acquire_stream_decoder",
                    "requested decoder stream is absent from the opened source",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "resolve_decoder_stream")
                        .with_field("media_id", media_id_text(source_info.identity().media_id()))
                        .with_field("stream_id", request.stream_id().value().to_string()),
                )
            })?;
        let mut config = DecoderConfig::new(stream.clone());
        if let Some(audio_format) = request.audio_format().cloned() {
            config = config.with_audio_format(audio_format)?;
        }
        let selection = registry.select(
            &BackendRequirement::decode(stream.codec().clone()),
            fallback_policy,
        )?;
        let evidence = DecoderSelectionEvidence {
            selected_backend_id: selection.primary().descriptor().id().clone(),
            fallback_backend_ids: selection
                .fallbacks()
                .iter()
                .map(|backend| backend.descriptor().id().clone())
                .collect(),
            fallback_used: selection.fallback_used(),
        };
        let decoder = selection
            .primary()
            .create_decoder(&config, operation)
            .map_err(|mut error| {
                error.push_context(
                    ErrorContext::new(COMPONENT, "create_selected_decoder")
                        .with_field("media_id", media_id_text(source_info.identity().media_id()))
                        .with_field("stream_id", request.stream_id().value().to_string())
                        .with_field("backend_id", evidence.selected_backend_id().as_str()),
                );
                error
            })?;
        if decoder.config() != &config {
            return Err(resource_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "acquire_stream_decoder",
                "selected decoder did not retain the requested configuration",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "verify_decoder_configuration")
                    .with_field("media_id", media_id_text(source_info.identity().media_id()))
                    .with_field("stream_id", request.stream_id().value().to_string())
                    .with_field("backend_id", evidence.selected_backend_id().as_str()),
            ));
        }
        acquired.insert(
            request.stream_id(),
            AcquiredDecoder {
                config,
                selection: evidence,
                decoder,
            },
        );
    }
    Ok(acquired)
}

fn media_id_text(media_id: MediaId) -> String {
    format!("{:032x}", media_id.raw())
}

fn resource_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
