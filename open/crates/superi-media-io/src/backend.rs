//! Codec backend registration, capability selection, and fallback policy.
//!
//! Concrete codec crates register only their declared capabilities. The registry
//! ranks compatible primary backends deterministically and exposes fallback
//! backends only when the caller explicitly permits them. It never probes,
//! loads, or silently substitutes a concrete codec implementation.

use std::collections::BTreeSet;
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::decode::{Decoder, DecoderConfig};
use crate::demux::{BackendId, CodecId, MediaSource, SourceRequest};
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

    /// Opens a source for ingest, playback, or relinking.
    fn open_source(&self, request: &SourceRequest) -> Result<Box<dyn MediaSource>>;

    /// Creates a decoder for one source stream.
    fn create_decoder(&self, config: &DecoderConfig) -> Result<Box<dyn Decoder>>;

    /// Creates an encoder for one output stream.
    fn create_encoder(&self, config: &EncoderConfig) -> Result<Box<dyn Encoder>>;
}
