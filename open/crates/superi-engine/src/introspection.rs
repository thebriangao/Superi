//! Deterministic assembled capability and health introspection.
//!
//! This module reports registered media declarations and their effective
//! primary and fallback order without instantiating sources, decoders, or
//! encoders. It also derives one immutable health, workflow, failure, recovery,
//! and optional finite-resource view from existing engine owners. Runtime
//! probing of a particular media file remains with the media I/O source probing
//! contract.

use std::collections::BTreeSet;

use superi_concurrency::lifecycle::LifecyclePhase;
use superi_core::diagnostics::UserSafeError;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::backend::{
    BackendCapability, BackendRegistry, BackendTier,
    CapabilityConstraint as BackendCapabilityConstraint, ChromaSampling as BackendChromaSampling,
    CodecCapability as BackendCodecCapability, CodecOperation as BackendCodecOperation,
    HardwareAcceleration as BackendHardwareAcceleration,
};

use crate::dispatcher::EngineRecoveryState;
use crate::error::{EngineFailureDisposition, EngineRecoveryRequest};
use crate::lifecycle::{EngineHealth, EngineSubsystem, EngineSubsystemState, EngineWorkKind};
use crate::resource_arbitration::{ResourceArbitrationSnapshot, ResourceClass};

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

/// API-neutral lifecycle phase for the complete engine introspection snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EngineLifecyclePhase {
    /// Subsystem owners are acquiring initial resources.
    Starting,
    /// Normal work may run.
    Running,
    /// Work is moving toward a resumable boundary.
    Pausing,
    /// Work is quiescent while resumable resources remain owned.
    Paused,
    /// Subsystem owners are reacquiring work after a manual pause.
    Resuming,
    /// Work is quiescing for system sleep.
    PreparingSleep,
    /// The engine is prepared for system sleep.
    Sleeping,
    /// Subsystem owners are validating resources after wake.
    Waking,
    /// Subsystem owners are releasing resources.
    Stopping,
    /// Every subsystem completed teardown.
    Stopped,
    /// A classified failure prevents normal work in this lifetime.
    Failed,
    /// A newer internal phase is not understood by this introspection version.
    Unknown,
}

impl EngineLifecyclePhase {
    fn from_lifecycle(value: LifecyclePhase) -> Self {
        match value {
            LifecyclePhase::Starting => Self::Starting,
            LifecyclePhase::Running => Self::Running,
            LifecyclePhase::Pausing => Self::Pausing,
            LifecyclePhase::Paused => Self::Paused,
            LifecyclePhase::Resuming => Self::Resuming,
            LifecyclePhase::PreparingSleep => Self::PreparingSleep,
            LifecyclePhase::Sleeping => Self::Sleeping,
            LifecyclePhase::Waking => Self::Waking,
            LifecyclePhase::Stopping => Self::Stopping,
            LifecyclePhase::Stopped => Self::Stopped,
            LifecyclePhase::Failed => Self::Failed,
            _ => Self::Unknown,
        }
    }
}

/// Immutable state for one canonical engine subsystem.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineSubsystemStatus {
    subsystem: EngineSubsystem,
    state: EngineSubsystemState,
}

impl EngineSubsystemStatus {
    /// Returns the canonical subsystem.
    #[must_use]
    pub const fn subsystem(&self) -> EngineSubsystem {
        self.subsystem
    }

    /// Returns the current resource state.
    #[must_use]
    pub const fn state(&self) -> EngineSubsystemState {
        self.state
    }
}

/// Immutable readiness for one coherent engine workflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineWorkflowStatus {
    work: EngineWorkKind,
    available: bool,
    blocking_subsystem: Option<EngineSubsystem>,
}

impl EngineWorkflowStatus {
    /// Returns the workflow described by this state.
    #[must_use]
    pub const fn work(&self) -> EngineWorkKind {
        self.work
    }

    /// Returns whether the current engine snapshot admits this workflow.
    #[must_use]
    pub const fn available(&self) -> bool {
        self.available
    }

    /// Returns the first unavailable dependency, when a subsystem blocks work.
    #[must_use]
    pub const fn blocking_subsystem(&self) -> Option<EngineSubsystem> {
        self.blocking_subsystem
    }
}

/// User-safe active failure state for one engine subsystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineFailureStatus {
    subsystem: EngineSubsystem,
    disposition: EngineFailureDisposition,
    recovery_request: Option<EngineRecoveryRequest>,
    recovery_in_progress: bool,
    user_safe_error: UserSafeError,
}

impl EngineFailureStatus {
    /// Returns the subsystem with the active failure.
    #[must_use]
    pub const fn subsystem(&self) -> EngineSubsystem {
        self.subsystem
    }

    /// Returns the stable engine response to this failure.
    #[must_use]
    pub const fn disposition(&self) -> EngineFailureDisposition {
        self.disposition
    }

    /// Returns the recovery intent accepted for this failure, when one exists.
    #[must_use]
    pub const fn recovery_request(&self) -> Option<EngineRecoveryRequest> {
        self.recovery_request
    }

    /// Returns whether the exact failure currently owns recovery work.
    #[must_use]
    pub const fn recovery_in_progress(&self) -> bool {
        self.recovery_in_progress
    }

    /// Returns the projection that excludes raw messages, contexts, fields, and sources.
    #[must_use]
    pub const fn user_safe_error(&self) -> &UserSafeError {
        &self.user_safe_error
    }
}

/// Exact inspectable accounting for one shared engine resource class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineResourceClassStatus {
    class: ResourceClass,
    protected_bytes: u64,
    hard_limit_bytes: u64,
    used_bytes: u64,
    peak_used_bytes: u64,
    active_reservations: u64,
    denied_reservations: u64,
    reclaim_attempts: u64,
    reclaimed_bytes: u64,
}

impl EngineResourceClassStatus {
    /// Returns the resource class.
    #[must_use]
    pub const fn class(&self) -> ResourceClass {
        self.class
    }

    /// Returns the amount protected from another class during reclaim.
    #[must_use]
    pub const fn protected_bytes(&self) -> u64 {
        self.protected_bytes
    }

    /// Returns the class-local hard limit.
    #[must_use]
    pub const fn hard_limit_bytes(&self) -> u64 {
        self.hard_limit_bytes
    }

    /// Returns current managed-byte usage.
    #[must_use]
    pub const fn used_bytes(&self) -> u64 {
        self.used_bytes
    }

    /// Returns unused class-local capacity.
    #[must_use]
    pub const fn available_bytes(&self) -> u64 {
        self.hard_limit_bytes.saturating_sub(self.used_bytes)
    }

    /// Returns peak managed-byte usage.
    #[must_use]
    pub const fn peak_used_bytes(&self) -> u64 {
        self.peak_used_bytes
    }

    /// Returns live reservations in this class.
    #[must_use]
    pub const fn active_reservations(&self) -> u64 {
        self.active_reservations
    }

    /// Returns denied requests observed for this class.
    #[must_use]
    pub const fn denied_reservations(&self) -> u64 {
        self.denied_reservations
    }

    /// Returns cooperative reclaim attempts targeting this class.
    #[must_use]
    pub const fn reclaim_attempts(&self) -> u64 {
        self.reclaim_attempts
    }

    /// Returns managed bytes observed released by this class.
    #[must_use]
    pub const fn reclaimed_bytes(&self) -> u64 {
        self.reclaimed_bytes
    }
}

/// Exact point-in-time accounting for the optional shared resource envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineResourceStatus {
    revision: u64,
    total_limit_bytes: u64,
    total_used_bytes: u64,
    peak_used_bytes: u64,
    active_reservations: u64,
    denied_reservations: u64,
    reclaim_attempts: u64,
    reclaimed_bytes: u64,
    classes: Vec<EngineResourceClassStatus>,
}

impl EngineResourceStatus {
    fn from_snapshot(snapshot: &ResourceArbitrationSnapshot) -> Self {
        Self {
            revision: snapshot.revision(),
            total_limit_bytes: snapshot.total_limit_bytes(),
            total_used_bytes: snapshot.total_used_bytes(),
            peak_used_bytes: snapshot.peak_used_bytes(),
            active_reservations: snapshot.active_reservations(),
            denied_reservations: snapshot.denied_reservations(),
            reclaim_attempts: snapshot.reclaim_attempts(),
            reclaimed_bytes: snapshot.reclaimed_bytes(),
            classes: snapshot
                .classes()
                .iter()
                .map(|class| EngineResourceClassStatus {
                    class: class.class(),
                    protected_bytes: class.budget().protected_bytes(),
                    hard_limit_bytes: class.budget().hard_limit_bytes(),
                    used_bytes: class.used_bytes(),
                    peak_used_bytes: class.peak_used_bytes(),
                    active_reservations: class.active_reservations(),
                    denied_reservations: class.denied_reservations(),
                    reclaim_attempts: class.reclaim_attempts(),
                    reclaimed_bytes: class.reclaimed_bytes(),
                })
                .collect(),
        }
    }

    /// Returns the resource owner's monotonic accounting revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the shared hard limit.
    #[must_use]
    pub const fn total_limit_bytes(&self) -> u64 {
        self.total_limit_bytes
    }

    /// Returns current shared managed-byte usage.
    #[must_use]
    pub const fn total_used_bytes(&self) -> u64 {
        self.total_used_bytes
    }

    /// Returns unused shared capacity.
    #[must_use]
    pub const fn available_bytes(&self) -> u64 {
        self.total_limit_bytes.saturating_sub(self.total_used_bytes)
    }

    /// Returns peak shared managed-byte usage.
    #[must_use]
    pub const fn peak_used_bytes(&self) -> u64 {
        self.peak_used_bytes
    }

    /// Returns every live shared reservation.
    #[must_use]
    pub const fn active_reservations(&self) -> u64 {
        self.active_reservations
    }

    /// Returns every denied shared request.
    #[must_use]
    pub const fn denied_reservations(&self) -> u64 {
        self.denied_reservations
    }

    /// Returns all cooperative reclaim attempts.
    #[must_use]
    pub const fn reclaim_attempts(&self) -> u64 {
        self.reclaim_attempts
    }

    /// Returns all managed bytes observed released by callbacks.
    #[must_use]
    pub const fn reclaimed_bytes(&self) -> u64 {
        self.reclaimed_bytes
    }

    /// Returns every class in stable resource-class order.
    #[must_use]
    pub fn classes(&self) -> &[EngineResourceClassStatus] {
        &self.classes
    }

    /// Returns exact accounting for one class.
    #[must_use]
    pub fn class(&self, class: ResourceClass) -> &EngineResourceClassStatus {
        self.classes
            .iter()
            .find(|status| status.class == class)
            .expect("the resource owner always publishes every class")
    }
}

/// Complete immutable capability and health state for one engine observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineIntrospectionSnapshot {
    media_capabilities: MediaCapabilities,
    resources: Option<EngineResourceStatus>,
    phase: EngineLifecyclePhase,
    lifecycle_revision: u64,
    state_revision: u64,
    recovery_revision: u64,
    lifetime: u64,
    health: EngineHealth,
    subsystems: Vec<EngineSubsystemStatus>,
    workflows: Vec<EngineWorkflowStatus>,
    active_failures: Vec<EngineFailureStatus>,
}

impl EngineIntrospectionSnapshot {
    pub(crate) fn from_state(
        media_capabilities: &MediaCapabilities,
        resources: Option<&ResourceArbitrationSnapshot>,
        recovery: &EngineRecoveryState,
    ) -> Result<Self> {
        let lifecycle = recovery.lifecycle();
        let subsystems = lifecycle
            .subsystems()
            .iter()
            .map(|subsystem| EngineSubsystemStatus {
                subsystem: subsystem.subsystem(),
                state: subsystem.state(),
            })
            .collect();
        let workflows = [
            EngineWorkKind::Playback,
            EngineWorkKind::Rendering,
            EngineWorkKind::Export,
        ]
        .into_iter()
        .map(|work| {
            let admission = lifecycle.admit(work);
            EngineWorkflowStatus {
                work,
                available: admission.permit().is_some(),
                blocking_subsystem: admission
                    .denial()
                    .and_then(|denial| denial.blocking_subsystem()),
            }
        })
        .collect();
        let pending_recovery = recovery.pending_recovery();
        let active_failures = recovery
            .active_failures()
            .iter()
            .map(|failure| {
                let user_safe_error =
                    failure
                        .diagnostic()
                        .user_safe_error()
                        .cloned()
                        .ok_or_else(|| {
                            Error::new(
                                ErrorCategory::Internal,
                                Recoverability::Terminal,
                                "active engine failure is missing its user-safe projection",
                            )
                            .with_context(
                                ErrorContext::new(
                                    "superi-engine.introspection",
                                    "build_engine_introspection",
                                )
                                .with_field("subsystem", failure.subsystem().code()),
                            )
                        })?;
                Ok(EngineFailureStatus {
                    subsystem: failure.subsystem(),
                    disposition: failure.disposition(),
                    recovery_request: failure.disposition().recovery_request(),
                    recovery_in_progress: pending_recovery
                        .is_some_and(|action| action.sequence() == failure.sequence()),
                    user_safe_error,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            media_capabilities: media_capabilities.clone(),
            resources: resources.map(EngineResourceStatus::from_snapshot),
            phase: EngineLifecyclePhase::from_lifecycle(lifecycle.phase()),
            lifecycle_revision: lifecycle.lifecycle_revision(),
            state_revision: lifecycle.state_revision(),
            recovery_revision: recovery.revision(),
            lifetime: lifecycle.lifetime(),
            health: lifecycle.health(),
            subsystems,
            workflows,
            active_failures,
        })
    }

    /// Returns complete declaration-only media capabilities.
    #[must_use]
    pub const fn media_capabilities(&self) -> &MediaCapabilities {
        &self.media_capabilities
    }

    /// Returns exact shared resource state when a host supplied it.
    #[must_use]
    pub const fn resources(&self) -> Option<&EngineResourceStatus> {
        self.resources.as_ref()
    }

    /// Returns the acknowledged engine lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> EngineLifecyclePhase {
        self.phase
    }

    /// Returns the shared lifecycle coordinator revision.
    #[must_use]
    pub const fn lifecycle_revision(&self) -> u64 {
        self.lifecycle_revision
    }

    /// Returns the engine lifecycle state revision.
    #[must_use]
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Returns the dispatcher-owned recovery-state revision.
    #[must_use]
    pub const fn recovery_revision(&self) -> u64 {
        self.recovery_revision
    }

    /// Returns the current engine lifetime.
    #[must_use]
    pub const fn lifetime(&self) -> u64 {
        self.lifetime
    }

    /// Returns aggregate lifecycle health.
    #[must_use]
    pub const fn health(&self) -> EngineHealth {
        self.health
    }

    /// Returns subsystem states in canonical initialization order.
    #[must_use]
    pub fn subsystems(&self) -> &[EngineSubsystemStatus] {
        &self.subsystems
    }

    /// Returns one canonical subsystem state.
    #[must_use]
    pub fn subsystem(&self, subsystem: EngineSubsystem) -> Option<&EngineSubsystemStatus> {
        self.subsystems
            .iter()
            .find(|status| status.subsystem == subsystem)
    }

    /// Returns playback, rendering, and export readiness in canonical order.
    #[must_use]
    pub fn workflows(&self) -> &[EngineWorkflowStatus] {
        &self.workflows
    }

    /// Returns readiness for one engine workflow.
    #[must_use]
    pub fn workflow(&self, work: EngineWorkKind) -> Option<&EngineWorkflowStatus> {
        self.workflows.iter().find(|status| status.work == work)
    }

    /// Returns user-safe active failures in canonical subsystem order.
    #[must_use]
    pub fn active_failures(&self) -> &[EngineFailureStatus] {
        &self.active_failures
    }

    /// Returns the active failure for one subsystem.
    #[must_use]
    pub fn active_failure(&self, subsystem: EngineSubsystem) -> Option<&EngineFailureStatus> {
        self.active_failures
            .iter()
            .find(|status| status.subsystem == subsystem)
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
