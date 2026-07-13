//! Codec backend registration, capability selection, and fallback policy.
//!
//! Concrete codec crates register only their declared capabilities. The registry
//! ranks compatible primary backends deterministically and exposes fallback
//! backends only when the caller explicitly permits them. It never probes,
//! loads, or silently substitutes a concrete codec implementation.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Read};
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::decode::{Decoder, DecoderConfig};
use crate::demux::{
    BackendId, CodecId, ContainerId, MediaSource, ProbeConfidence, SourceLocation, SourceProbe,
    SourceProbeLimits, SourceProbeResult, SourceRequest,
};
use crate::encode::{Encoder, EncoderConfig};

/// Human-readable identity for one backend implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendDescriptor {
    id: BackendId,
    display_name: String,
}

impl BackendDescriptor {
    /// Creates a backend descriptor.
    pub fn new(id: BackendId, display_name: impl Into<String>) -> Result<Self> {
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "backend display name must not be empty",
            )
            .with_context(ErrorContext::new(
                "superi-media-io.backend",
                "create_backend_descriptor",
            )));
        }
        Ok(Self { id, display_name })
    }

    /// Returns stable machine identity.
    #[must_use]
    pub const fn id(&self) -> &BackendId {
        &self.id
    }

    /// Returns a diagnostic display name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

/// One operation a backend can perform for a declared codec capability.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum BackendCapability {
    /// Open a codec-neutral packet source for ingest, playback, or relinking.
    Source,
    /// Decode packets for a declared compressed codec.
    Decode(CodecId),
    /// Encode frames or audio blocks for a declared compressed codec.
    Encode(CodecId),
}

/// Deterministic capability declarations for one backend.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackendCapabilities(BTreeSet<BackendCapability>);

impl BackendCapabilities {
    /// Builds a capability set. Duplicate declarations are idempotent.
    #[must_use]
    pub fn new(values: impl IntoIterator<Item = BackendCapability>) -> Self {
        Self(values.into_iter().collect())
    }

    /// Returns whether a capability was declared by the backend.
    #[must_use]
    pub fn contains(&self, capability: &BackendCapability) -> bool {
        self.0.contains(capability)
    }

    /// Iterates capabilities in stable declaration order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &BackendCapability> {
        self.0.iter()
    }

    /// Returns whether no operation was declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One media operation used to select a registered backend.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BackendRequirement {
    /// Select a backend that can open a packet source.
    Source,
    /// Select a backend that can decode the specified codec.
    Decode(CodecId),
    /// Select a backend that can encode the specified codec.
    Encode(CodecId),
}

impl BackendRequirement {
    /// Creates a source-open requirement.
    #[must_use]
    pub const fn source() -> Self {
        Self::Source
    }

    /// Creates a decode requirement.
    #[must_use]
    pub fn decode(codec: CodecId) -> Self {
        Self::Decode(codec)
    }

    /// Creates an encode requirement.
    #[must_use]
    pub fn encode(codec: CodecId) -> Self {
        Self::Encode(codec)
    }

    fn capability(&self) -> BackendCapability {
        match self {
            Self::Source => BackendCapability::Source,
            Self::Decode(codec) => BackendCapability::Decode(codec.clone()),
            Self::Encode(codec) => BackendCapability::Encode(codec.clone()),
        }
    }
}

/// The policy tier assigned when a backend is registered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BackendTier {
    /// A normal candidate that can be selected without fallback permission.
    Primary,
    /// A candidate available only through an explicit fallback policy.
    Fallback,
}

/// Whether a selection may contain fallback candidates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FallbackPolicy {
    /// Return only the highest-ranked primary candidate.
    Disallow,
    /// Return declared fallback candidates in deterministic rank order.
    AllowRegistered,
}

/// A concrete backend plus its immutable policy declaration.
pub struct BackendRegistration {
    backend: Arc<dyn MediaBackend>,
    capabilities: BackendCapabilities,
    priority: u16,
    tier: BackendTier,
}

impl BackendRegistration {
    /// Creates a backend registration with an explicit selection tier and rank.
    pub fn new(
        backend: Arc<dyn MediaBackend>,
        capabilities: BackendCapabilities,
        priority: u16,
        tier: BackendTier,
    ) -> Result<Self> {
        if capabilities.is_empty() {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "backend registration must declare at least one capability",
            )
            .with_context(ErrorContext::new(
                "superi-media-io.backend",
                "create_backend_registration",
            )));
        }
        Ok(Self {
            backend,
            capabilities,
            priority,
            tier,
        })
    }

    /// Returns the registered implementation.
    #[must_use]
    pub fn backend(&self) -> &Arc<dyn MediaBackend> {
        &self.backend
    }

    /// Returns the declared capabilities.
    #[must_use]
    pub const fn capabilities(&self) -> &BackendCapabilities {
        &self.capabilities
    }

    /// Returns the explicit ranking priority. Larger values rank first.
    #[must_use]
    pub const fn priority(&self) -> u16 {
        self.priority
    }

    /// Returns the explicit policy tier.
    #[must_use]
    pub const fn tier(&self) -> BackendTier {
        self.tier
    }
}

/// Ordered backend candidates for one operation.
pub struct BackendSelection {
    primary: Arc<dyn MediaBackend>,
    fallbacks: Vec<Arc<dyn MediaBackend>>,
    fallback_used: bool,
}

/// One backend whose content probe recognized a source format.
pub struct SourceProbeCandidate {
    backend: Arc<dyn MediaBackend>,
    container: ContainerId,
    confidence: ProbeConfidence,
}

impl SourceProbeCandidate {
    /// Returns the recognized backend through the codec-neutral interface.
    #[must_use]
    pub const fn backend(&self) -> &Arc<dyn MediaBackend> {
        &self.backend
    }

    /// Returns the stable recognized container identity.
    #[must_use]
    pub const fn container(&self) -> &ContainerId {
        &self.container
    }

    /// Returns the content-based detection confidence.
    #[must_use]
    pub const fn confidence(&self) -> ProbeConfidence {
        self.confidence
    }
}

/// Deterministically ordered probe result for one immutable source request.
pub struct SourceProbeSelection {
    request: SourceRequest,
    primary: SourceProbeCandidate,
    fallbacks: Vec<SourceProbeCandidate>,
    fallback_used: bool,
    bytes_examined: usize,
    source_length: u64,
}

impl SourceProbeSelection {
    /// Returns the request whose bytes produced this selection.
    #[must_use]
    pub const fn request(&self) -> &SourceRequest {
        &self.request
    }

    /// Returns the selected source backend and its content match.
    #[must_use]
    pub const fn primary(&self) -> &SourceProbeCandidate {
        &self.primary
    }

    /// Returns explicitly allowed fallback-tier content matches.
    #[must_use]
    pub fn fallbacks(&self) -> &[SourceProbeCandidate] {
        &self.fallbacks
    }

    /// Returns whether no primary-tier backend recognized the source.
    #[must_use]
    pub const fn fallback_used(&self) -> bool {
        self.fallback_used
    }

    /// Returns the final bounded prefix length presented to backends.
    #[must_use]
    pub const fn bytes_examined(&self) -> usize {
        self.bytes_examined
    }

    /// Returns the source length observed before probing.
    #[must_use]
    pub const fn source_length(&self) -> u64 {
        self.source_length
    }

    /// Opens the exact request through the selected codec-neutral backend.
    pub fn open(&self) -> Result<Box<dyn MediaSource>> {
        self.primary
            .backend
            .open_source(&self.request)
            .map_err(|mut error| {
                error.push_context(
                    ErrorContext::new("superi-media-io.backend", "open_probed_source")
                        .with_field(
                            "backend_id",
                            self.primary.backend.descriptor().id().as_str(),
                        )
                        .with_field("container_id", self.primary.container.as_str()),
                );
                error
            })
    }
}

impl BackendSelection {
    /// Returns the selected primary backend.
    #[must_use]
    pub fn primary(&self) -> &Arc<dyn MediaBackend> {
        &self.primary
    }

    /// Returns allowed fallback candidates in deterministic rank order.
    #[must_use]
    pub fn fallbacks(&self) -> &[Arc<dyn MediaBackend>] {
        &self.fallbacks
    }

    /// Returns whether the selected backend required explicit fallback permission.
    #[must_use]
    pub const fn fallback_used(&self) -> bool {
        self.fallback_used
    }
}

/// Registry of known media implementations and their selection policy.
#[derive(Default)]
pub struct BackendRegistry {
    registrations: Vec<BackendRegistration>,
}

impl BackendRegistry {
    /// Creates an empty backend registry.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            registrations: Vec::new(),
        }
    }

    /// Adds one backend. Stable backend identifiers may only be registered once.
    pub fn register(&mut self, registration: BackendRegistration) -> Result<()> {
        let id = registration.backend.descriptor().id();
        if self
            .registrations
            .iter()
            .any(|existing| existing.backend.descriptor().id() == id)
        {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "backend identifier is already registered",
            )
            .with_context(
                ErrorContext::new("superi-media-io.backend", "register_backend")
                    .with_field("backend_id", id.as_str()),
            ));
        }
        self.registrations.push(registration);
        Ok(())
    }

    /// Selects a primary backend and, when allowed, its declared fallbacks.
    pub fn select(
        &self,
        requirement: &BackendRequirement,
        fallback_policy: FallbackPolicy,
    ) -> Result<BackendSelection> {
        let capability = requirement.capability();
        let mut primary = self
            .registrations
            .iter()
            .filter(|registration| {
                registration.tier == BackendTier::Primary
                    && registration.capabilities.contains(&capability)
            })
            .collect::<Vec<_>>();
        primary.sort_by(|left, right| rank_order(left, right));
        let mut fallback = if fallback_policy == FallbackPolicy::AllowRegistered {
            let mut values = self
                .registrations
                .iter()
                .filter(|registration| {
                    registration.tier == BackendTier::Fallback
                        && registration.capabilities.contains(&capability)
                })
                .collect::<Vec<_>>();
            values.sort_by(|left, right| rank_order(left, right));
            values
        } else {
            Vec::new()
        };

        if let Some(primary) = primary.first() {
            return Ok(BackendSelection {
                primary: Arc::clone(&primary.backend),
                fallbacks: fallback
                    .into_iter()
                    .map(|registration| Arc::clone(&registration.backend))
                    .collect(),
                fallback_used: false,
            });
        }

        let Some(selected) = fallback.first() else {
            return Err(unsupported(requirement));
        };
        let primary = Arc::clone(&selected.backend);
        fallback.remove(0);

        Ok(BackendSelection {
            primary,
            fallbacks: fallback
                .into_iter()
                .map(|registration| Arc::clone(&registration.backend))
                .collect(),
            fallback_used: true,
        })
    }

    /// Probes a bounded source prefix and deterministically selects a matching backend.
    ///
    /// Filesystem access is blocking and must be called by the repository-defined I/O worker.
    /// Extension and file-name hints are provided to backends, but only a backend content match is
    /// eligible for selection.
    pub fn probe_source(
        &self,
        request: SourceRequest,
        limits: SourceProbeLimits,
        fallback_policy: FallbackPolicy,
    ) -> Result<SourceProbeSelection> {
        let source_capability = BackendCapability::Source;
        let registrations = self
            .registrations
            .iter()
            .filter(|registration| registration.capabilities.contains(&source_capability))
            .filter(|registration| {
                registration.tier == BackendTier::Primary
                    || fallback_policy == FallbackPolicy::AllowRegistered
            })
            .collect::<Vec<_>>();
        if registrations.is_empty() {
            return Err(unsupported(&BackendRequirement::Source));
        }

        let mut reader = SourceProbeReader::new(request.location())?;
        let mut target_bytes = limits.initial_bytes();
        let matches = loop {
            reader.read_to(target_bytes)?;
            let probe = SourceProbe::new(
                request.location(),
                reader.bytes(),
                reader.source_length(),
                reader.is_complete(),
            );
            let mut matches = Vec::new();
            let mut requested_bytes = reader.bytes().len();

            for registration in &registrations {
                match registration
                    .backend
                    .probe_source(&probe)
                    .map_err(|mut error| {
                        error.push_context(
                            ErrorContext::new("superi-media-io.backend", "probe_backend")
                                .with_field(
                                    "backend_id",
                                    registration.backend.descriptor().id().as_str(),
                                ),
                        );
                        error
                    })? {
                    SourceProbeResult::NoMatch => {}
                    SourceProbeResult::NeedMoreData { minimum_bytes } => {
                        requested_bytes = requested_bytes.max(minimum_bytes.get());
                    }
                    SourceProbeResult::Match {
                        container,
                        confidence,
                    } => matches.push(RankedProbeCandidate {
                        candidate: SourceProbeCandidate {
                            backend: Arc::clone(&registration.backend),
                            container,
                            confidence,
                        },
                        priority: registration.priority,
                        tier: registration.tier,
                    }),
                }
            }

            if requested_bytes > reader.bytes().len()
                && !reader.is_complete()
                && reader.bytes().len() < limits.maximum_bytes()
            {
                let grown = reader
                    .bytes()
                    .len()
                    .saturating_mul(2)
                    .max(requested_bytes)
                    .min(limits.maximum_bytes());
                if grown > reader.bytes().len() {
                    target_bytes = grown;
                    continue;
                }
            }
            break matches;
        };

        let bytes_examined = reader.bytes().len();
        let source_length = reader.source_length();
        drop(reader);
        build_probe_selection(
            request,
            matches,
            bytes_examined,
            source_length,
            limits.maximum_bytes(),
        )
    }
}

struct RankedProbeCandidate {
    candidate: SourceProbeCandidate,
    priority: u16,
    tier: BackendTier,
}

fn build_probe_selection(
    request: SourceRequest,
    mut matches: Vec<RankedProbeCandidate>,
    bytes_examined: usize,
    source_length: u64,
    maximum_bytes: usize,
) -> Result<SourceProbeSelection> {
    matches.sort_by(|left, right| {
        right
            .candidate
            .confidence
            .cmp(&left.candidate.confidence)
            .then_with(|| right.priority.cmp(&left.priority))
            .then_with(|| {
                left.candidate
                    .backend
                    .descriptor()
                    .id()
                    .cmp(right.candidate.backend.descriptor().id())
            })
    });

    let mut primary = matches
        .iter()
        .position(|candidate| candidate.tier == BackendTier::Primary);
    let fallback_used = primary.is_none();
    if fallback_used {
        primary = (!matches.is_empty()).then_some(0);
    }
    let Some(primary_index) = primary else {
        return Err(unrecognized_source(
            bytes_examined,
            maximum_bytes,
            source_length,
        ));
    };
    let primary = matches.remove(primary_index).candidate;
    let fallbacks = matches
        .into_iter()
        .filter(|candidate| candidate.tier == BackendTier::Fallback)
        .map(|candidate| candidate.candidate)
        .collect();

    Ok(SourceProbeSelection {
        request,
        primary,
        fallbacks,
        fallback_used,
        bytes_examined,
        source_length,
    })
}

enum ProbeReadSource<'a> {
    File(File),
    Memory(&'a [u8]),
}

struct SourceProbeReader<'a> {
    source: ProbeReadSource<'a>,
    bytes: Vec<u8>,
    source_length: u64,
    exhausted: bool,
}

impl<'a> SourceProbeReader<'a> {
    fn new(location: &'a SourceLocation) -> Result<Self> {
        match location {
            SourceLocation::Path(path) => {
                let file =
                    File::open(path).map_err(|error| probe_io_error("open_probe_source", error))?;
                let source_length = file
                    .metadata()
                    .map_err(|error| probe_io_error("inspect_probe_source", error))?
                    .len();
                Ok(Self {
                    source: ProbeReadSource::File(file),
                    bytes: Vec::new(),
                    source_length,
                    exhausted: source_length == 0,
                })
            }
            SourceLocation::Memory { data, .. } => {
                let source_length = u64::try_from(data.len()).map_err(|_| {
                    Error::new(
                        ErrorCategory::ResourceExhausted,
                        Recoverability::UserCorrectable,
                        "memory source length cannot be represented",
                    )
                    .with_context(ErrorContext::new(
                        "superi-media-io.backend",
                        "inspect_probe_source",
                    ))
                })?;
                Ok(Self {
                    source: ProbeReadSource::Memory(data),
                    bytes: Vec::new(),
                    source_length,
                    exhausted: data.is_empty(),
                })
            }
        }
    }

    fn read_to(&mut self, target_bytes: usize) -> Result<()> {
        let target_bytes = u64::try_from(target_bytes).unwrap_or(u64::MAX);
        let source_target = usize::try_from(self.source_length.min(target_bytes))
            .expect("source target never exceeds usize probe limit");
        if source_target <= self.bytes.len() {
            return Ok(());
        }
        let additional = source_target - self.bytes.len();
        match &mut self.source {
            ProbeReadSource::File(file) => {
                let read = file
                    .take(u64::try_from(additional).unwrap_or(u64::MAX))
                    .read_to_end(&mut self.bytes)
                    .map_err(|error| probe_io_error("read_probe_source", error))?;
                if read < additional {
                    self.exhausted = true;
                }
            }
            ProbeReadSource::Memory(data) => {
                self.bytes
                    .extend_from_slice(&data[self.bytes.len()..source_target]);
            }
        }
        if self.bytes.len() as u64 >= self.source_length {
            self.exhausted = true;
        }
        Ok(())
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    const fn source_length(&self) -> u64 {
        self.source_length
    }

    const fn is_complete(&self) -> bool {
        self.exhausted
    }
}

fn probe_io_error(operation: &'static str, source: io::Error) -> Error {
    let (category, recoverability) = match source.kind() {
        io::ErrorKind::NotFound => (ErrorCategory::NotFound, Recoverability::UserCorrectable),
        io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
        ),
        io::ErrorKind::InvalidInput => {
            (ErrorCategory::InvalidInput, Recoverability::UserCorrectable)
        }
        _ => (ErrorCategory::Unavailable, Recoverability::Retryable),
    };
    Error::with_source(
        category,
        recoverability,
        "media source could not be read",
        source,
    )
    .with_context(ErrorContext::new("superi-media-io.backend", operation))
}

fn unrecognized_source(bytes_examined: usize, maximum_bytes: usize, source_length: u64) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        "no allowed backend recognized the media source",
    )
    .with_context(
        ErrorContext::new("superi-media-io.backend", "probe_source")
            .with_field("bytes_examined", bytes_examined.to_string())
            .with_field("maximum_bytes", maximum_bytes.to_string())
            .with_field("source_length", source_length.to_string()),
    )
}

fn rank_order(left: &BackendRegistration, right: &BackendRegistration) -> std::cmp::Ordering {
    right.priority.cmp(&left.priority).then_with(|| {
        left.backend
            .descriptor()
            .id()
            .cmp(right.backend.descriptor().id())
    })
}

fn unsupported(requirement: &BackendRequirement) -> Error {
    let operation = match requirement {
        BackendRequirement::Source => "source",
        BackendRequirement::Decode(_) => "decode",
        BackendRequirement::Encode(_) => "encode",
    };
    let mut context = ErrorContext::new("superi-media-io.backend", "select_backend")
        .with_field("operation", operation);
    if let BackendRequirement::Decode(codec) | BackendRequirement::Encode(codec) = requirement {
        context.insert_field("codec", codec.as_str());
    }
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        "no allowed backend supports the requested media operation",
    )
    .with_context(context)
}

/// Codec-neutral backend factories selected by higher-level fallback policy.
pub trait MediaBackend: Send + Sync {
    /// Returns stable backend identity.
    fn descriptor(&self) -> &BackendDescriptor;

    /// Inspects bounded source bytes without opening a concrete decoder.
    fn probe_source(&self, probe: &SourceProbe<'_>) -> Result<SourceProbeResult>;

    /// Opens a source for ingest, playback, or relinking.
    fn open_source(&self, request: &SourceRequest) -> Result<Box<dyn MediaSource>>;

    /// Creates a decoder for one source stream.
    fn create_decoder(&self, config: &DecoderConfig) -> Result<Box<dyn Decoder>>;

    /// Creates an encoder for one output stream.
    fn create_encoder(&self, config: &EncoderConfig) -> Result<Box<dyn Encoder>>;
}
