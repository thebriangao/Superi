//! Codec backend registration, capability selection, and fallback policy.
//!
//! Concrete codec crates register only their declared capabilities. The registry
//! ranks compatible primary backends deterministically and exposes fallback
//! backends only when the caller explicitly permits them. It never probes,
//! loads, or silently substitutes a concrete codec implementation.

use std::collections::BTreeSet;
use std::fs::File;
use std::io;
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::decode::{Decoder, DecoderConfig};
use crate::demux::{
    BackendId, CodecId, ContainerId, MediaSource, ProbeConfidence, SourceLocation, SourceProbe,
    SourceProbeLimits, SourceProbeResult, SourceRequest,
};
use crate::encode::{Encoder, EncoderConfig};
use crate::operation::OperationContext;
use crate::read::{read_exact_interruptible, ReadOutcome};

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
    /// Open a codec-neutral packet source for ingest, seeking, playback, or relinking.
    Source,
    /// Decode packets for a declared compressed codec.
    Decode(CodecId),
    /// Encode frames or audio blocks for a declared compressed codec.
    Encode(CodecId),
}

/// Direction of one detailed codec capability row.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum CodecOperation {
    /// Convert compressed packets into frames or audio blocks.
    Decode,
    /// Convert frames or audio blocks into compressed packets.
    Encode,
}

/// Chroma sampling carried by a video codec profile.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ChromaSampling {
    /// A single luma plane with no chroma planes.
    Monochrome,
    /// Half horizontal and half vertical chroma resolution.
    Cs420,
    /// Half horizontal and full vertical chroma resolution.
    Cs422,
    /// Full chroma resolution in both dimensions.
    Cs444,
}

/// How a backend can execute its declared media operations.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum HardwareAcceleration {
    /// Capability metadata does not report the execution mode.
    #[default]
    Unreported,
    /// The declared operations execute in software.
    Software,
    /// The declared operations require a hardware codec path.
    Hardware,
    /// The operating system may select hardware or software for each session.
    PlatformManaged,
}

/// Whether one codec dimension is fixed, dynamic, absent, or undeclared.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum CapabilityConstraint<T> {
    /// The codec dimension does not apply to this operation.
    NotApplicable,
    /// The concrete value is negotiated when the codec session is created.
    RuntimeNegotiated,
    /// The backend or external protocol does not report this dimension.
    Unreported,
    /// Every listed value is supported by this capability row.
    Values(BTreeSet<T>),
}

impl<T> CapabilityConstraint<T> {
    /// Returns fixed values, or `None` for a non-fixed constraint state.
    #[must_use]
    pub const fn values(&self) -> Option<&BTreeSet<T>> {
        match self {
            Self::Values(values) => Some(values),
            Self::NotApplicable | Self::RuntimeNegotiated | Self::Unreported => None,
        }
    }
}

/// One valid correlated codec capability tuple for a backend.
///
/// Multiple rows for the same codec and direction preserve relationships between profile, level,
/// component depth, and chroma instead of implying that every cross-product is supported.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodecCapability {
    operation: CodecOperation,
    codec: CodecId,
    profiles: CapabilityConstraint<String>,
    levels: CapabilityConstraint<String>,
    bit_depths: CapabilityConstraint<u8>,
    chroma_sampling: CapabilityConstraint<ChromaSampling>,
}

impl CodecCapability {
    /// Creates an unreported capability row for one codec and direction.
    #[must_use]
    pub const fn new(operation: CodecOperation, codec: CodecId) -> Self {
        Self {
            operation,
            codec,
            profiles: CapabilityConstraint::Unreported,
            levels: CapabilityConstraint::Unreported,
            bit_depths: CapabilityConstraint::Unreported,
            chroma_sampling: CapabilityConstraint::Unreported,
        }
    }

    /// Declares one or more stable profile codes.
    pub fn with_profiles<S>(mut self, values: impl IntoIterator<Item = S>) -> Result<Self>
    where
        S: Into<String>,
    {
        self.profiles = CapabilityConstraint::Values(text_constraint_values(
            values,
            "set_codec_profiles",
            "codec profile values must use lowercase ASCII letters, digits, dots, underscores, or hyphens",
        )?);
        Ok(self)
    }

    /// Marks profiles as selected when the codec session is created.
    #[must_use]
    pub fn with_profiles_runtime(mut self) -> Self {
        self.profiles = CapabilityConstraint::RuntimeNegotiated;
        self
    }

    /// Marks profiles as not applicable to this codec operation.
    #[must_use]
    pub fn with_profiles_not_applicable(mut self) -> Self {
        self.profiles = CapabilityConstraint::NotApplicable;
        self
    }

    /// Declares one or more stable level codes.
    pub fn with_levels<S>(mut self, values: impl IntoIterator<Item = S>) -> Result<Self>
    where
        S: Into<String>,
    {
        self.levels = CapabilityConstraint::Values(text_constraint_values(
            values,
            "set_codec_levels",
            "codec level values must use lowercase ASCII letters, digits, dots, underscores, or hyphens",
        )?);
        Ok(self)
    }

    /// Marks levels as selected when the codec session is created.
    #[must_use]
    pub fn with_levels_runtime(mut self) -> Self {
        self.levels = CapabilityConstraint::RuntimeNegotiated;
        self
    }

    /// Marks levels as not applicable to this codec operation.
    #[must_use]
    pub fn with_levels_not_applicable(mut self) -> Self {
        self.levels = CapabilityConstraint::NotApplicable;
        self
    }

    /// Declares one or more meaningful component bit depths.
    pub fn with_bit_depths(mut self, values: impl IntoIterator<Item = u8>) -> Result<Self> {
        let values = values.into_iter().collect::<BTreeSet<_>>();
        if values.is_empty() || values.iter().any(|value| !(1..=64).contains(value)) {
            return Err(invalid_capability(
                "set_codec_bit_depths",
                "codec bit depths must contain values from 1 through 64",
            ));
        }
        self.bit_depths = CapabilityConstraint::Values(values);
        Ok(self)
    }

    /// Marks component depth as selected when the codec session is created.
    #[must_use]
    pub fn with_bit_depths_runtime(mut self) -> Self {
        self.bit_depths = CapabilityConstraint::RuntimeNegotiated;
        self
    }

    /// Marks component depth as not applicable to this codec operation.
    #[must_use]
    pub fn with_bit_depths_not_applicable(mut self) -> Self {
        self.bit_depths = CapabilityConstraint::NotApplicable;
        self
    }

    /// Declares one or more chroma sampling modes.
    pub fn with_chroma_sampling(
        mut self,
        values: impl IntoIterator<Item = ChromaSampling>,
    ) -> Result<Self> {
        let values = values.into_iter().collect::<BTreeSet<_>>();
        if values.is_empty() {
            return Err(invalid_capability(
                "set_codec_chroma_sampling",
                "codec chroma sampling must contain at least one value",
            ));
        }
        self.chroma_sampling = CapabilityConstraint::Values(values);
        Ok(self)
    }

    /// Marks chroma sampling as selected when the codec session is created.
    #[must_use]
    pub fn with_chroma_sampling_runtime(mut self) -> Self {
        self.chroma_sampling = CapabilityConstraint::RuntimeNegotiated;
        self
    }

    /// Marks chroma sampling as not applicable to this codec operation.
    #[must_use]
    pub fn with_chroma_sampling_not_applicable(mut self) -> Self {
        self.chroma_sampling = CapabilityConstraint::NotApplicable;
        self
    }

    /// Returns the decode or encode direction.
    #[must_use]
    pub const fn operation(&self) -> CodecOperation {
        self.operation
    }

    /// Returns the stable codec identity.
    #[must_use]
    pub const fn codec(&self) -> &CodecId {
        &self.codec
    }

    /// Returns profile support for this tuple.
    #[must_use]
    pub const fn profiles(&self) -> &CapabilityConstraint<String> {
        &self.profiles
    }

    /// Returns level support for this tuple.
    #[must_use]
    pub const fn levels(&self) -> &CapabilityConstraint<String> {
        &self.levels
    }

    /// Returns meaningful component depths for this tuple.
    #[must_use]
    pub const fn bit_depths(&self) -> &CapabilityConstraint<u8> {
        &self.bit_depths
    }

    /// Returns chroma sampling support for this tuple.
    #[must_use]
    pub const fn chroma_sampling(&self) -> &CapabilityConstraint<ChromaSampling> {
        &self.chroma_sampling
    }

    fn selection_capability(&self) -> BackendCapability {
        match self.operation {
            CodecOperation::Decode => BackendCapability::Decode(self.codec.clone()),
            CodecOperation::Encode => BackendCapability::Encode(self.codec.clone()),
        }
    }
}

/// Deterministic capability declarations for one backend.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackendCapabilities {
    operations: BTreeSet<BackendCapability>,
    hardware_acceleration: HardwareAcceleration,
    codec_capabilities: BTreeSet<CodecCapability>,
}

impl BackendCapabilities {
    /// Builds a capability set. Duplicate declarations are idempotent.
    #[must_use]
    pub fn new(values: impl IntoIterator<Item = BackendCapability>) -> Self {
        Self {
            operations: values.into_iter().collect(),
            hardware_acceleration: HardwareAcceleration::Unreported,
            codec_capabilities: BTreeSet::new(),
        }
    }

    /// Sets the backend-wide execution mode reported to capability consumers.
    #[must_use]
    pub const fn with_hardware_acceleration(
        mut self,
        hardware_acceleration: HardwareAcceleration,
    ) -> Self {
        self.hardware_acceleration = hardware_acceleration;
        self
    }

    /// Adds correlated codec rows after validating their selection operations.
    pub fn with_codec_capabilities(
        mut self,
        values: impl IntoIterator<Item = CodecCapability>,
    ) -> Result<Self> {
        for capability in values {
            let selection = capability.selection_capability();
            if !self.operations.contains(&selection) {
                return Err(invalid_capability(
                    "set_codec_capabilities",
                    "detailed codec capability must refer to a declared decode or encode operation",
                ));
            }
            self.codec_capabilities.insert(capability);
        }
        Ok(self)
    }

    /// Returns whether a capability was declared by the backend.
    #[must_use]
    pub fn contains(&self, capability: &BackendCapability) -> bool {
        self.operations.contains(capability)
    }

    /// Iterates capabilities in stable declaration order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &BackendCapability> {
        self.operations.iter()
    }

    /// Returns the backend-wide execution mode.
    #[must_use]
    pub const fn hardware_acceleration(&self) -> HardwareAcceleration {
        self.hardware_acceleration
    }

    /// Iterates correlated codec rows in stable tuple order.
    pub fn codec_capabilities(&self) -> impl ExactSizeIterator<Item = &CodecCapability> {
        self.codec_capabilities.iter()
    }

    /// Returns whether no operation was declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

fn text_constraint_values<S>(
    values: impl IntoIterator<Item = S>,
    operation: &'static str,
    message: &'static str,
) -> Result<BTreeSet<String>>
where
    S: Into<String>,
{
    let values = values.into_iter().map(Into::into).collect::<BTreeSet<_>>();
    if values.is_empty() || values.iter().any(|value| !valid_capability_code(value)) {
        return Err(invalid_capability(operation, message));
    }
    Ok(values)
}

fn valid_capability_code(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn invalid_capability(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-media-io.backend", operation))
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
    pub fn open(&self, operation: &OperationContext) -> Result<Box<dyn MediaSource>> {
        self.primary
            .backend
            .open_source(&self.request, operation)
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

    /// Iterates immutable registrations in registration order.
    ///
    /// Consumers that publish capability state must impose their own stable
    /// presentation order instead of treating initialization order as policy.
    pub fn registrations(&self) -> impl ExactSizeIterator<Item = &BackendRegistration> {
        self.registrations.iter()
    }

    /// Returns registrations for one capability and tier in selection order.
    #[must_use]
    pub fn ranked_registrations(
        &self,
        capability: &BackendCapability,
        tier: BackendTier,
    ) -> Vec<&BackendRegistration> {
        let mut values = self
            .registrations
            .iter()
            .filter(|registration| {
                registration.tier == tier && registration.capabilities.contains(capability)
            })
            .collect::<Vec<_>>();
        values.sort_by(|left, right| rank_order(left, right));
        values
    }

    /// Selects a primary backend and, when allowed, its declared fallbacks.
    pub fn select(
        &self,
        requirement: &BackendRequirement,
        fallback_policy: FallbackPolicy,
    ) -> Result<BackendSelection> {
        let capability = requirement.capability();
        let primary = self.ranked_registrations(&capability, BackendTier::Primary);
        let mut fallback = if fallback_policy == FallbackPolicy::AllowRegistered {
            self.ranked_registrations(&capability, BackendTier::Fallback)
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
        operation: &OperationContext,
    ) -> Result<SourceProbeSelection> {
        operation.check("probe_source")?;
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

        let mut reader = SourceProbeReader::new(request.location(), operation)?;
        let mut target_bytes = limits.initial_bytes();
        let matches = loop {
            operation.check("probe_source")?;
            reader.read_to(target_bytes, operation)?;
            let probe = SourceProbe::new(
                request.location(),
                reader.bytes(),
                reader.source_length(),
                reader.is_complete(),
            );
            let mut matches = Vec::new();
            let mut requested_bytes = reader.bytes().len();

            for registration in &registrations {
                operation.check("probe_source")?;
                let probe_result = registration
                    .backend
                    .probe_source(&probe, operation)
                    .map_err(|mut error| {
                        error.push_context(
                            ErrorContext::new("superi-media-io.backend", "probe_backend")
                                .with_field(
                                    "backend_id",
                                    registration.backend.descriptor().id().as_str(),
                                ),
                        );
                        error
                    })?;
                match probe_result {
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
    fn new(location: &'a SourceLocation, operation: &OperationContext) -> Result<Self> {
        operation.check("open_probe_source")?;
        match location {
            SourceLocation::Path(path) => {
                let file =
                    File::open(path).map_err(|error| probe_io_error("open_probe_source", error))?;
                let source_length = file
                    .metadata()
                    .map_err(|error| probe_io_error("inspect_probe_source", error))?
                    .len();
                operation.check("inspect_probe_source")?;
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
                operation.check("inspect_probe_source")?;
                Ok(Self {
                    source: ProbeReadSource::Memory(data),
                    bytes: Vec::new(),
                    source_length,
                    exhausted: data.is_empty(),
                })
            }
        }
    }

    fn read_to(&mut self, target_bytes: usize, operation: &OperationContext) -> Result<()> {
        operation.check("read_probe_source")?;
        let target_bytes = u64::try_from(target_bytes).unwrap_or(u64::MAX);
        let source_target = usize::try_from(self.source_length.min(target_bytes))
            .expect("source target never exceeds usize probe limit");
        if source_target <= self.bytes.len() {
            return Ok(());
        }
        let additional = source_target - self.bytes.len();
        match &mut self.source {
            ProbeReadSource::File(file) => {
                let byte_offset = u64::try_from(self.bytes.len()).map_err(|_| {
                    Error::new(
                        ErrorCategory::ResourceExhausted,
                        Recoverability::UserCorrectable,
                        "probe byte offset cannot be represented",
                    )
                    .with_context(ErrorContext::new(
                        "superi-media-io.backend",
                        "read_probe_source",
                    ))
                })?;
                let mut buffer = vec![0_u8; additional];
                let read =
                    match read_exact_interruptible(file, &mut buffer, byte_offset, operation)? {
                        ReadOutcome::Complete(read) => read,
                        ReadOutcome::Partial { value: read, .. } => {
                            self.exhausted = true;
                            read
                        }
                        ReadOutcome::EndOfStream => {
                            self.exhausted = true;
                            0
                        }
                    };
                self.bytes.extend_from_slice(&buffer[..read]);
            }
            ProbeReadSource::Memory(data) => {
                self.bytes
                    .extend_from_slice(&data[self.bytes.len()..source_target]);
                operation.check("read_probe_source")?;
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
    fn probe_source(
        &self,
        probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult>;

    /// Opens a source for ingest, playback, or relinking.
    fn open_source(
        &self,
        request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>>;

    /// Creates a decoder for one source stream.
    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>>;

    /// Creates an encoder for one output stream.
    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>>;
}
