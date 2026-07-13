//! Deterministic assembled capability introspection.
//!
//! This module reports registered media declarations and their effective
//! primary and fallback order without instantiating sources, decoders, or
//! encoders. Runtime probing of a particular media file remains with the media
//! I/O source probing contract.

use std::collections::BTreeSet;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::backend::{BackendCapability, BackendRegistry, BackendTier};

/// The API-neutral policy tier of one media backend declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaBackendTier {
    /// A normal candidate selected without fallback permission.
    Primary,
    /// A candidate that requires explicit fallback permission.
    Fallback,
}

/// One media operation available to engine consumers.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MediaOperation {
    /// Open a packet source for ingest, seeking, playback, or relinking.
    Source,
    /// Decode packets for the named codec.
    Decode {
        /// Stable codec identifier.
        codec: String,
    },
    /// Encode frames or audio blocks for the named codec.
    Encode {
        /// Stable codec identifier.
        codec: String,
    },
}

/// Publicly inspectable declarations for one registered media backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaBackendCapabilities {
    id: String,
    display_name: String,
    priority: u16,
    tier: MediaBackendTier,
    capabilities: Vec<MediaOperation>,
}

impl MediaBackendCapabilities {
    /// Returns stable machine identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the diagnostic name suitable for capability displays.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Returns the selection priority. Larger values rank first.
    #[must_use]
    pub const fn priority(&self) -> u16 {
        self.priority
    }

    /// Returns whether this is a primary or explicit fallback backend.
    #[must_use]
    pub const fn tier(&self) -> MediaBackendTier {
        self.tier
    }

    /// Returns source, decode, and encode declarations in stable order.
    #[must_use]
    pub fn capabilities(&self) -> &[MediaOperation] {
        &self.capabilities
    }
}

/// Effective backend support for one media operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaOperationSupport {
    operation: MediaOperation,
    primary_backends: Vec<String>,
    fallback_backends: Vec<String>,
}

impl MediaOperationSupport {
    /// Returns the source, decode, or encode operation being reported.
    #[must_use]
    pub const fn operation(&self) -> &MediaOperation {
        &self.operation
    }

    /// Returns normal candidates in the same order used for selection.
    #[must_use]
    pub fn primary_backends(&self) -> &[String] {
        &self.primary_backends
    }

    /// Returns candidates that require explicit fallback permission.
    #[must_use]
    pub fn fallback_backends(&self) -> &[String] {
        &self.fallback_backends
    }
}

/// Immutable media capability state assembled from one backend registry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MediaCapabilities {
    backends: Vec<MediaBackendCapabilities>,
    operations: Vec<MediaOperationSupport>,
}

impl MediaCapabilities {
    /// Captures current backend declarations without invoking backend factories.
    ///
    /// # Errors
    ///
    /// Returns an unsupported error if the registry contains a declaration
    /// unknown to this engine version.
    pub fn from_registry(registry: &BackendRegistry) -> Result<Self> {
        let mut backends = registry
            .registrations()
            .map(|registration| {
                let descriptor = registration.backend().descriptor();
                Ok(MediaBackendCapabilities {
                    id: descriptor.id().as_str().to_owned(),
                    display_name: descriptor.display_name().to_owned(),
                    priority: registration.priority(),
                    tier: media_tier(registration.tier())?,
                    capabilities: registration
                        .capabilities()
                        .iter()
                        .map(media_operation)
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        backends.sort_by(|left, right| left.id.cmp(&right.id));

        let declared_operations = registry
            .registrations()
            .flat_map(|registration| registration.capabilities().iter().cloned())
            .collect::<BTreeSet<_>>();
        let operations = declared_operations
            .into_iter()
            .map(|capability| {
                Ok(MediaOperationSupport {
                    primary_backends: ranked_ids(registry, &capability, BackendTier::Primary),
                    fallback_backends: ranked_ids(registry, &capability, BackendTier::Fallback),
                    operation: media_operation(&capability)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            backends,
            operations,
        })
    }

    /// Returns backend declarations in stable identifier order.
    #[must_use]
    pub fn backends(&self) -> &[MediaBackendCapabilities] {
        &self.backends
    }

    /// Returns operation support in stable source, decode, and encode order.
    #[must_use]
    pub fn operations(&self) -> &[MediaOperationSupport] {
        &self.operations
    }

    /// Returns whether no media operation is declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

fn ranked_ids(
    registry: &BackendRegistry,
    capability: &BackendCapability,
    tier: BackendTier,
) -> Vec<String> {
    registry
        .ranked_registrations(capability, tier)
        .into_iter()
        .map(|registration| registration.backend().descriptor().id().as_str().to_owned())
        .collect()
}

fn media_tier(tier: BackendTier) -> Result<MediaBackendTier> {
    match tier {
        BackendTier::Primary => Ok(MediaBackendTier::Primary),
        BackendTier::Fallback => Ok(MediaBackendTier::Fallback),
        _ => Err(unsupported_declaration("backend tier")),
    }
}

fn media_operation(capability: &BackendCapability) -> Result<MediaOperation> {
    match capability {
        BackendCapability::Source => Ok(MediaOperation::Source),
        BackendCapability::Decode(codec) => Ok(MediaOperation::Decode {
            codec: codec.as_str().to_owned(),
        }),
        BackendCapability::Encode(codec) => Ok(MediaOperation::Encode {
            codec: codec.as_str().to_owned(),
        }),
        _ => Err(unsupported_declaration("backend capability")),
    }
}

fn unsupported_declaration(kind: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        "registered media declaration is not supported by engine introspection",
    )
    .with_context(
        ErrorContext::new("superi-engine.introspection", "build_media_capabilities")
            .with_field("declaration_kind", kind),
    )
}
