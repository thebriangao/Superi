//! Deterministic assembled capability introspection.
//!
//! This module reports registered media declarations and their effective
//! primary and fallback order without instantiating sources, decoders, or
//! encoders. Runtime probing of a particular media file remains with the media
//! I/O source probing contract.

use std::collections::BTreeSet;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::backend::{
    BackendCapability, BackendRegistry, BackendTier,
    CapabilityConstraint as BackendCapabilityConstraint, ChromaSampling as BackendChromaSampling,
    CodecCapability as BackendCodecCapability, CodecOperation as BackendCodecOperation,
    HardwareAcceleration as BackendHardwareAcceleration,
};

/// The API-neutral policy tier of one media backend declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaBackendTier {
    /// A normal candidate selected without fallback permission.
    Primary,
    /// A candidate that requires explicit fallback permission.
    Fallback,
}

/// Whether one public media capability dimension is fixed, dynamic, absent, or undeclared.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MediaCapabilityConstraint<T> {
    /// The dimension does not apply to this operation.
    NotApplicable,
    /// The value is negotiated when the concrete codec session is created.
    RuntimeNegotiated,
    /// The backend or external protocol does not report the value.
    Unreported,
    /// Every listed value is supported by this correlated capability row.
    Values(Vec<T>),
}

/// Chroma sampling exposed by one video codec capability row.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MediaChromaSampling {
    /// A luma-only picture.
    Monochrome,
    /// 4:2:0 chroma sampling.
    Cs420,
    /// 4:2:2 chroma sampling.
    Cs422,
    /// 4:4:4 chroma sampling.
    Cs444,
}

/// How one backend executes its declared media operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaHardwareAcceleration {
    /// The backend does not report its execution mode.
    Unreported,
    /// Operations execute in software.
    Software,
    /// Operations require a hardware codec path.
    Hardware,
    /// The operating system selects hardware or software for each session.
    PlatformManaged,
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

/// One valid correlated profile, level, depth, and chroma tuple.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MediaCodecCapability {
    operation: MediaOperation,
    profiles: MediaCapabilityConstraint<String>,
    levels: MediaCapabilityConstraint<String>,
    bit_depths: MediaCapabilityConstraint<u8>,
    chroma_sampling: MediaCapabilityConstraint<MediaChromaSampling>,
}

impl MediaCodecCapability {
    /// Returns the decode or encode operation covered by this tuple.
    #[must_use]
    pub const fn operation(&self) -> &MediaOperation {
        &self.operation
    }

    /// Returns profile support for this tuple.
    #[must_use]
    pub const fn profiles(&self) -> &MediaCapabilityConstraint<String> {
        &self.profiles
    }

    /// Returns level support for this tuple.
    #[must_use]
    pub const fn levels(&self) -> &MediaCapabilityConstraint<String> {
        &self.levels
    }

    /// Returns component bit-depth support for this tuple.
    #[must_use]
    pub const fn bit_depths(&self) -> &MediaCapabilityConstraint<u8> {
        &self.bit_depths
    }

    /// Returns chroma sampling support for this tuple.
    #[must_use]
    pub const fn chroma_sampling(&self) -> &MediaCapabilityConstraint<MediaChromaSampling> {
        &self.chroma_sampling
    }
}

/// Publicly inspectable declarations for one registered media backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaBackendCapabilities {
    id: String,
    display_name: String,
    priority: u16,
    tier: MediaBackendTier,
    capabilities: Vec<MediaOperation>,
    hardware_acceleration: MediaHardwareAcceleration,
    codec_capabilities: Vec<MediaCodecCapability>,
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

    /// Returns how this backend executes its media operations.
    #[must_use]
    pub const fn hardware_acceleration(&self) -> MediaHardwareAcceleration {
        self.hardware_acceleration
    }

    /// Returns detailed codec tuples in deterministic order.
    #[must_use]
    pub fn codec_capabilities(&self) -> &[MediaCodecCapability] {
        &self.codec_capabilities
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
                    hardware_acceleration: media_hardware_acceleration(
                        registration.capabilities().hardware_acceleration(),
                    )?,
                    codec_capabilities: media_codec_capabilities(registration)?,
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

fn media_hardware_acceleration(
    value: BackendHardwareAcceleration,
) -> Result<MediaHardwareAcceleration> {
    match value {
        BackendHardwareAcceleration::Unreported => Ok(MediaHardwareAcceleration::Unreported),
        BackendHardwareAcceleration::Software => Ok(MediaHardwareAcceleration::Software),
        BackendHardwareAcceleration::Hardware => Ok(MediaHardwareAcceleration::Hardware),
        BackendHardwareAcceleration::PlatformManaged => {
            Ok(MediaHardwareAcceleration::PlatformManaged)
        }
        _ => Err(unsupported_declaration("hardware acceleration")),
    }
}

fn media_codec_capabilities(
    registration: &superi_media_io::backend::BackendRegistration,
) -> Result<Vec<MediaCodecCapability>> {
    let mut values = registration
        .capabilities()
        .codec_capabilities()
        .map(media_codec_capability)
        .collect::<Result<Vec<_>>>()?;

    for capability in registration.capabilities().iter() {
        let (BackendCapability::Decode(_) | BackendCapability::Encode(_)) = capability else {
            continue;
        };
        let operation = media_operation(capability)?;
        if !values.iter().any(|value| value.operation == operation) {
            values.push(MediaCodecCapability {
                operation,
                profiles: MediaCapabilityConstraint::Unreported,
                levels: MediaCapabilityConstraint::Unreported,
                bit_depths: MediaCapabilityConstraint::Unreported,
                chroma_sampling: MediaCapabilityConstraint::Unreported,
            });
        }
    }
    values.sort();
    Ok(values)
}

fn media_codec_capability(value: &BackendCodecCapability) -> Result<MediaCodecCapability> {
    let operation = match value.operation() {
        BackendCodecOperation::Decode => MediaOperation::Decode {
            codec: value.codec().as_str().to_owned(),
        },
        BackendCodecOperation::Encode => MediaOperation::Encode {
            codec: value.codec().as_str().to_owned(),
        },
        _ => return Err(unsupported_declaration("codec operation")),
    };
    Ok(MediaCodecCapability {
        operation,
        profiles: media_constraint(value.profiles(), |value| Ok(value.clone()))?,
        levels: media_constraint(value.levels(), |value| Ok(value.clone()))?,
        bit_depths: media_constraint(value.bit_depths(), |value| Ok(*value))?,
        chroma_sampling: media_constraint(value.chroma_sampling(), media_chroma_sampling)?,
    })
}

fn media_constraint<T, U>(
    value: &BackendCapabilityConstraint<T>,
    convert: impl Fn(&T) -> Result<U>,
) -> Result<MediaCapabilityConstraint<U>> {
    match value {
        BackendCapabilityConstraint::NotApplicable => Ok(MediaCapabilityConstraint::NotApplicable),
        BackendCapabilityConstraint::RuntimeNegotiated => {
            Ok(MediaCapabilityConstraint::RuntimeNegotiated)
        }
        BackendCapabilityConstraint::Unreported => Ok(MediaCapabilityConstraint::Unreported),
        BackendCapabilityConstraint::Values(values) => Ok(MediaCapabilityConstraint::Values(
            values.iter().map(convert).collect::<Result<Vec<_>>>()?,
        )),
        _ => Err(unsupported_declaration("codec constraint")),
    }
}

fn media_chroma_sampling(value: &BackendChromaSampling) -> Result<MediaChromaSampling> {
    match value {
        BackendChromaSampling::Monochrome => Ok(MediaChromaSampling::Monochrome),
        BackendChromaSampling::Cs420 => Ok(MediaChromaSampling::Cs420),
        BackendChromaSampling::Cs422 => Ok(MediaChromaSampling::Cs422),
        BackendChromaSampling::Cs444 => Ok(MediaChromaSampling::Cs444),
        _ => Err(unsupported_declaration("chroma sampling")),
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
