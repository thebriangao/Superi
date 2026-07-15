//! Transport-neutral public capability and health API.
//!
//! UI, scripting, extensions, CLI clients, and automation execute the same
//! typed queries and consume the same full replacement events. Bulk media never
//! crosses this boundary.

use serde::{Deserialize, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::SemanticVersion;
use superi_engine::error::{
    EngineFailureDisposition as InternalFailureDisposition,
    EngineRecoveryRequest as InternalRecoveryRequest,
};
use superi_engine::introspection::{
    EngineFailureStatus as InternalFailureStatus,
    EngineIntrospectionSnapshot as InternalIntrospectionSnapshot,
    EngineLifecyclePhase as InternalLifecyclePhase,
    EngineResourceClassStatus as InternalResourceClassStatus,
    EngineResourceStatus as InternalResourceStatus,
    EngineSubsystemStatus as InternalSubsystemStatus,
    EngineWorkflowStatus as InternalWorkflowStatus, MediaBackendTier as EngineMediaBackendTier,
    MediaCapabilities as EngineMediaCapabilities,
    MediaCapabilityConstraint as EngineCapabilityConstraint,
    MediaChromaSampling as EngineChromaSampling, MediaCodecCapability as EngineCodecCapability,
    MediaHardwareAcceleration as EngineHardwareAcceleration,
    MediaOperation as EngineMediaOperation,
};
use superi_engine::lifecycle::{
    EngineHealth as InternalHealth, EngineSubsystem as InternalSubsystem,
    EngineSubsystemState as InternalSubsystemState, EngineWorkKind as InternalWorkflow,
};
use superi_engine::resource_arbitration::ResourceClass as InternalResourceClass;

use crate::commands::{
    GetEngineIntrospection, GetEngineIntrospectionResult, GetMediaCapabilities,
    GetMediaCapabilitiesResult,
};
use crate::events::{EngineIntrospectionChanged, MediaCapabilitiesChanged};
use crate::version::{ENGINE_INTROSPECTION_SCHEMA_VERSION, MEDIA_CAPABILITIES_SCHEMA_VERSION};

/// The stable public policy tier for one media backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MediaBackendTier {
    /// A normal backend available without fallback permission.
    Primary,
    /// A backend available only under explicit fallback policy.
    Fallback,
}

/// Whether one media capability dimension is fixed, dynamic, absent, or undeclared.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
#[non_exhaustive]
pub enum CapabilityConstraint<T> {
    /// The dimension does not apply to this operation.
    NotApplicable,
    /// The value is selected when a concrete codec session is created.
    RuntimeNegotiated,
    /// The backend or external protocol does not report the value.
    Unreported,
    /// Every listed value is supported by this correlated capability row.
    Values {
        /// Stable deterministic values.
        values: Vec<T>,
    },
}

/// Chroma sampling exposed by one video codec capability row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ChromaSampling {
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HardwareAcceleration {
    /// The backend does not report its execution mode.
    Unreported,
    /// Operations execute in software.
    Software,
    /// Operations require a hardware codec path.
    Hardware,
    /// The operating system selects hardware or software for each session.
    PlatformManaged,
}

/// One stable media action exposed to API consumers.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum MediaOperation {
    /// Open a media source for ingest, seeking, playback, or relinking.
    Source,
    /// Decode packets for a named codec.
    Decode {
        /// Stable codec identifier.
        codec: String,
    },
    /// Encode frames or audio blocks for a named codec.
    Encode {
        /// Stable codec identifier.
        codec: String,
    },
}

/// One valid correlated profile, level, depth, and chroma tuple.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaCodecCapability {
    operation: MediaOperation,
    profiles: CapabilityConstraint<String>,
    levels: CapabilityConstraint<String>,
    bit_depths: CapabilityConstraint<u8>,
    chroma_sampling: CapabilityConstraint<ChromaSampling>,
}

impl MediaCodecCapability {
    /// Returns the decode or encode operation covered by this tuple.
    #[must_use]
    pub const fn operation(&self) -> &MediaOperation {
        &self.operation
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

    /// Returns meaningful component bit depths for this tuple.
    #[must_use]
    pub const fn bit_depths(&self) -> &CapabilityConstraint<u8> {
        &self.bit_depths
    }

    /// Returns chroma sampling support for this tuple.
    #[must_use]
    pub const fn chroma_sampling(&self) -> &CapabilityConstraint<ChromaSampling> {
        &self.chroma_sampling
    }
}

/// Public capability declaration for one registered backend.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaBackendCapabilities {
    id: String,
    display_name: String,
    priority: u16,
    tier: MediaBackendTier,
    capabilities: Vec<MediaOperation>,
    hardware_acceleration: HardwareAcceleration,
    codec_capabilities: Vec<MediaCodecCapability>,
}

impl MediaBackendCapabilities {
    /// Returns stable backend identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the diagnostic display name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Returns the selection priority. Larger values rank first.
    #[must_use]
    pub const fn priority(&self) -> u16 {
        self.priority
    }

    /// Returns the backend's primary or fallback policy tier.
    #[must_use]
    pub const fn tier(&self) -> MediaBackendTier {
        self.tier
    }

    /// Returns declared actions in stable order.
    #[must_use]
    pub fn capabilities(&self) -> &[MediaOperation] {
        &self.capabilities
    }

    /// Returns how this backend executes its media operations.
    #[must_use]
    pub const fn hardware_acceleration(&self) -> HardwareAcceleration {
        self.hardware_acceleration
    }

    /// Returns detailed codec tuples in deterministic order.
    #[must_use]
    pub fn codec_capabilities(&self) -> &[MediaCodecCapability] {
        &self.codec_capabilities
    }
}

/// Effective ranked backend support for one media action.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaOperationSupport {
    operation: MediaOperation,
    primary_backends: Vec<String>,
    fallback_backends: Vec<String>,
}

impl MediaOperationSupport {
    /// Returns the reported media action.
    #[must_use]
    pub const fn operation(&self) -> &MediaOperation {
        &self.operation
    }

    /// Returns normal candidates in actual selection order.
    #[must_use]
    pub fn primary_backends(&self) -> &[String] {
        &self.primary_backends
    }

    /// Returns candidates requiring explicit fallback permission.
    #[must_use]
    pub fn fallback_backends(&self) -> &[String] {
        &self.fallback_backends
    }
}

/// Strict, versioned, point-in-time media capability state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaCapabilitiesSnapshot {
    schema_version: SemanticVersion,
    revision: u64,
    backends: Vec<MediaBackendCapabilities>,
    operations: Vec<MediaOperationSupport>,
}

impl MediaCapabilitiesSnapshot {
    /// Returns the schema that defines this payload.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns the monotonic API-local state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns backend declarations in stable identifier order.
    #[must_use]
    pub fn backends(&self) -> &[MediaBackendCapabilities] {
        &self.backends
    }

    /// Returns supported actions in stable source, decode, and encode order.
    #[must_use]
    pub fn operations(&self) -> &[MediaOperationSupport] {
        &self.operations
    }

    fn same_state(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.backends == other.backends
            && self.operations == other.operations
    }
}

/// API state that serves media capability commands and produces change events.
#[derive(Clone, Debug)]
pub struct MediaCapabilitiesApi {
    snapshot: MediaCapabilitiesSnapshot,
}

impl MediaCapabilitiesApi {
    /// Creates API state from an engine capability snapshot.
    #[must_use]
    pub fn new(engine: &EngineMediaCapabilities) -> Self {
        Self {
            snapshot: public_snapshot(engine, 0),
        }
    }

    /// Executes the same typed query used by UI and automation clients.
    #[must_use]
    pub fn execute(&self, _command: GetMediaCapabilities) -> GetMediaCapabilitiesResult {
        GetMediaCapabilitiesResult::new(self.snapshot.clone())
    }

    /// Synchronizes engine state and emits one full replacement event on change.
    ///
    /// # Errors
    ///
    /// Returns a terminal error if the monotonic revision space is exhausted.
    pub fn synchronize(
        &mut self,
        engine: &EngineMediaCapabilities,
    ) -> Result<Option<MediaCapabilitiesChanged>> {
        let mut next = public_snapshot(engine, self.snapshot.revision);
        if self.snapshot.same_state(&next) {
            return Ok(None);
        }

        next.revision = self.snapshot.revision.checked_add(1).ok_or_else(|| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "media capability revision is exhausted",
            )
            .with_context(ErrorContext::new(
                "superi-api.media_capabilities",
                "synchronize",
            ))
        })?;
        self.snapshot = next.clone();
        Ok(Some(MediaCapabilitiesChanged::new(next)))
    }
}

fn public_snapshot(engine: &EngineMediaCapabilities, revision: u64) -> MediaCapabilitiesSnapshot {
    MediaCapabilitiesSnapshot {
        schema_version: MEDIA_CAPABILITIES_SCHEMA_VERSION.clone(),
        revision,
        backends: engine
            .backends()
            .iter()
            .map(|backend| MediaBackendCapabilities {
                id: backend.id().to_owned(),
                display_name: backend.display_name().to_owned(),
                priority: backend.priority(),
                tier: public_tier(backend.tier()),
                capabilities: backend
                    .capabilities()
                    .iter()
                    .map(public_operation)
                    .collect(),
                hardware_acceleration: public_hardware_acceleration(
                    backend.hardware_acceleration(),
                ),
                codec_capabilities: backend
                    .codec_capabilities()
                    .iter()
                    .map(public_codec_capability)
                    .collect(),
            })
            .collect(),
        operations: engine
            .operations()
            .iter()
            .map(|support| MediaOperationSupport {
                operation: public_operation(support.operation()),
                primary_backends: support.primary_backends().to_vec(),
                fallback_backends: support.fallback_backends().to_vec(),
            })
            .collect(),
    }
}

fn public_tier(tier: EngineMediaBackendTier) -> MediaBackendTier {
    match tier {
        EngineMediaBackendTier::Primary => MediaBackendTier::Primary,
        EngineMediaBackendTier::Fallback => MediaBackendTier::Fallback,
    }
}

fn public_operation(operation: &EngineMediaOperation) -> MediaOperation {
    match operation {
        EngineMediaOperation::Source => MediaOperation::Source,
        EngineMediaOperation::Decode { codec } => MediaOperation::Decode {
            codec: codec.clone(),
        },
        EngineMediaOperation::Encode { codec } => MediaOperation::Encode {
            codec: codec.clone(),
        },
    }
}

fn public_hardware_acceleration(value: EngineHardwareAcceleration) -> HardwareAcceleration {
    match value {
        EngineHardwareAcceleration::Unreported => HardwareAcceleration::Unreported,
        EngineHardwareAcceleration::Software => HardwareAcceleration::Software,
        EngineHardwareAcceleration::Hardware => HardwareAcceleration::Hardware,
        EngineHardwareAcceleration::PlatformManaged => HardwareAcceleration::PlatformManaged,
    }
}

fn public_codec_capability(value: &EngineCodecCapability) -> MediaCodecCapability {
    MediaCodecCapability {
        operation: public_operation(value.operation()),
        profiles: public_constraint(value.profiles(), Clone::clone),
        levels: public_constraint(value.levels(), Clone::clone),
        bit_depths: public_constraint(value.bit_depths(), |value| *value),
        chroma_sampling: public_constraint(value.chroma_sampling(), public_chroma_sampling),
    }
}

fn public_constraint<T, U>(
    value: &EngineCapabilityConstraint<T>,
    convert: impl Fn(&T) -> U,
) -> CapabilityConstraint<U> {
    match value {
        EngineCapabilityConstraint::NotApplicable => CapabilityConstraint::NotApplicable,
        EngineCapabilityConstraint::RuntimeNegotiated => CapabilityConstraint::RuntimeNegotiated,
        EngineCapabilityConstraint::Unreported => CapabilityConstraint::Unreported,
        EngineCapabilityConstraint::Values(values) => CapabilityConstraint::Values {
            values: values.iter().map(convert).collect(),
        },
    }
}

fn public_chroma_sampling(value: &EngineChromaSampling) -> ChromaSampling {
    match value {
        EngineChromaSampling::Monochrome => ChromaSampling::Monochrome,
        EngineChromaSampling::Cs420 => ChromaSampling::Cs420,
        EngineChromaSampling::Cs422 => ChromaSampling::Cs422,
        EngineChromaSampling::Cs444 => ChromaSampling::Cs444,
    }
}

/// Stable public lifecycle phase for the complete engine state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    /// This API version does not understand the internal phase.
    Unknown,
}

/// Stable public aggregate engine health.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineHealth {
    /// No subsystem has an unresolved failure.
    Healthy,
    /// A recoverable subsystem failure limits dependent work.
    Degraded,
    /// A terminal or lifecycle failure prevents all work.
    Failed,
    /// This API version does not understand the internal health state.
    Unknown,
}

/// Stable public engine subsystem identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineSubsystem {
    /// Authoritative shared engine state.
    SharedState,
    /// Authoritative project state and mutation coordination.
    Projects,
    /// Volatile audio and graphics device coordination.
    Devices,
    /// Interactive playback ownership.
    Playback,
    /// Frame rendering ownership.
    Rendering,
    /// Export ownership built on rendering.
    Export,
    /// This API version does not understand the internal subsystem.
    Unknown,
}

/// Stable public state for one engine subsystem.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineSubsystemState {
    /// No resource is owned.
    Offline,
    /// Resources are being acquired.
    Initializing,
    /// The subsystem is ready.
    Ready,
    /// Resources remain owned but work is limited.
    Degraded,
    /// Resources are being revalidated or reacquired.
    Recovering,
    /// Work is quiescing and volatile resources are being released for sleep.
    PreparingSleep,
    /// The subsystem is quiescent with its retention policy applied.
    Sleeping,
    /// Retained state is being revalidated or volatile resources are being reacquired.
    Waking,
    /// Resources are being released.
    Stopping,
    /// The subsystem has an unresolved failure.
    Failed,
    /// This API version does not understand the internal state.
    Unknown,
}

/// Stable public workflow identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineWorkflow {
    /// Interactive playback work.
    Playback,
    /// Frame rendering work.
    Rendering,
    /// Export work, including rendering dependencies.
    Export,
    /// This API version does not understand the internal workflow.
    Unknown,
}

/// Stable public response to one active failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineFailureDisposition {
    /// The same operation may be attempted again.
    RetryAvailable,
    /// Unrelated workflows may continue while a capability is restored.
    ContinueDegraded,
    /// User input, permission, configuration, or project state must change.
    UserCorrectionRequired,
    /// The current engine lifetime must restart.
    RestartRequired,
    /// This API version does not understand the internal disposition.
    Unknown,
}

/// Stable public recovery intent for one active failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineRecoveryRequest {
    /// Retry the failed operation.
    RetryOperation,
    /// Restore the degraded subsystem.
    RestoreSubsystem,
    /// Revalidate after an explicit user correction.
    ApplyUserCorrection,
    /// This API version does not understand the internal recovery request.
    Unknown,
}

/// Stable public resource class identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineResourceClass {
    /// Decoded video or audio payloads.
    DecodeBuffer,
    /// Managed GPU payloads.
    GpuMemory,
    /// Reconstructible cache entries.
    Cache,
    /// Prepared audio payloads.
    Audio,
    /// AI models, tensors, and temporary work.
    Ai,
    /// Export queues, staging, and retained outputs.
    Export,
    /// This API version does not understand the internal class.
    Unknown,
}

/// Strict public state for one canonical engine subsystem.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

    /// Returns the current subsystem state.
    #[must_use]
    pub const fn state(&self) -> EngineSubsystemState {
        self.state
    }
}

/// Strict public readiness for one coherent engine workflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineWorkflowStatus {
    workflow: EngineWorkflow,
    available: bool,
    blocking_subsystem: Option<EngineSubsystem>,
}

impl EngineWorkflowStatus {
    /// Returns the described workflow.
    #[must_use]
    pub const fn workflow(&self) -> EngineWorkflow {
        self.workflow
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

/// Strict user-safe failure text and stable classification codes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineUserSafeError {
    category: String,
    recoverability: String,
    code: String,
    title: String,
    action: String,
}

impl EngineUserSafeError {
    /// Returns the stable shared error category code.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    /// Returns the stable shared recoverability code.
    #[must_use]
    pub fn recoverability(&self) -> &str {
        &self.recoverability
    }

    /// Returns the stable localization and automation code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the safe default title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the safe default recovery action.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }
}

/// Strict user-safe active failure state for one subsystem.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineFailureStatus {
    subsystem: EngineSubsystem,
    disposition: EngineFailureDisposition,
    recovery_request: Option<EngineRecoveryRequest>,
    recovery_in_progress: bool,
    error: EngineUserSafeError,
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

    /// Returns the accepted recovery intent, when one exists.
    #[must_use]
    pub const fn recovery_request(&self) -> Option<EngineRecoveryRequest> {
        self.recovery_request
    }

    /// Returns whether this exact failure currently owns recovery work.
    #[must_use]
    pub const fn recovery_in_progress(&self) -> bool {
        self.recovery_in_progress
    }

    /// Returns only the reviewed user-safe failure projection.
    #[must_use]
    pub const fn error(&self) -> &EngineUserSafeError {
        &self.error
    }
}

/// Strict public accounting for one shared engine resource class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineResourceClassStatus {
    class: EngineResourceClass,
    protected_bytes: u64,
    hard_limit_bytes: u64,
    used_bytes: u64,
    available_bytes: u64,
    peak_used_bytes: u64,
    active_reservations: u64,
    denied_reservations: u64,
    reclaim_attempts: u64,
    reclaimed_bytes: u64,
}

impl EngineResourceClassStatus {
    /// Returns the stable resource class.
    #[must_use]
    pub const fn class(&self) -> EngineResourceClass {
        self.class
    }

    /// Returns capacity protected from other classes during reclaim.
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
        self.available_bytes
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

    /// Returns reclaim attempts targeting this class.
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

/// Strict point-in-time state for the optional shared resource envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineResourceStatus {
    revision: u64,
    total_limit_bytes: u64,
    total_used_bytes: u64,
    available_bytes: u64,
    peak_used_bytes: u64,
    active_reservations: u64,
    denied_reservations: u64,
    reclaim_attempts: u64,
    reclaimed_bytes: u64,
    classes: Vec<EngineResourceClassStatus>,
}

impl EngineResourceStatus {
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
        self.available_bytes
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

    /// Returns every cooperative reclaim attempt.
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

    /// Returns exact accounting for one resource class.
    #[must_use]
    pub fn class(&self, class: EngineResourceClass) -> Option<&EngineResourceClassStatus> {
        self.classes.iter().find(|status| status.class == class)
    }
}

/// Strict, versioned, point-in-time engine capability and health state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineIntrospectionSnapshot {
    schema_version: SemanticVersion,
    revision: u64,
    media_capabilities: MediaCapabilitiesSnapshot,
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
    /// Returns the schema that defines this payload.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns the monotonic API-local full-state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the complete nested media capability snapshot.
    #[must_use]
    pub const fn media_capabilities(&self) -> &MediaCapabilitiesSnapshot {
        &self.media_capabilities
    }

    /// Returns exact resource state when the engine host supplied it.
    #[must_use]
    pub const fn resources(&self) -> Option<&EngineResourceStatus> {
        self.resources.as_ref()
    }

    /// Returns the acknowledged lifecycle phase.
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

    /// Returns aggregate engine health.
    #[must_use]
    pub const fn health(&self) -> EngineHealth {
        self.health
    }

    /// Returns subsystem state in canonical engine order.
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
    pub fn workflow(&self, workflow: EngineWorkflow) -> Option<&EngineWorkflowStatus> {
        self.workflows
            .iter()
            .find(|status| status.workflow == workflow)
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

    fn same_state(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.media_capabilities == other.media_capabilities
            && self.resources == other.resources
            && self.phase == other.phase
            && self.lifecycle_revision == other.lifecycle_revision
            && self.state_revision == other.state_revision
            && self.recovery_revision == other.recovery_revision
            && self.lifetime == other.lifetime
            && self.health == other.health
            && self.subsystems == other.subsystems
            && self.workflows == other.workflows
            && self.active_failures == other.active_failures
    }
}

/// API state that serves complete engine introspection and replacement events.
#[derive(Clone, Debug)]
pub struct EngineIntrospectionApi {
    snapshot: EngineIntrospectionSnapshot,
}

impl EngineIntrospectionApi {
    /// Creates API state from one immutable engine observation.
    #[must_use]
    pub fn new(engine: &InternalIntrospectionSnapshot) -> Self {
        Self {
            snapshot: public_engine_snapshot(engine, 0, 0),
        }
    }

    /// Executes the same typed query used by editor and automation clients.
    #[must_use]
    pub fn execute(&self, _command: GetEngineIntrospection) -> GetEngineIntrospectionResult {
        GetEngineIntrospectionResult::new(self.snapshot.clone())
    }

    /// Synchronizes one engine observation and emits one full replacement event on change.
    ///
    /// Media capability state keeps its independent nested revision. Any visible change also
    /// advances the enclosing engine introspection revision.
    ///
    /// # Errors
    ///
    /// Returns a terminal error before mutation if either monotonic revision space is exhausted.
    pub fn synchronize(
        &mut self,
        engine: &InternalIntrospectionSnapshot,
    ) -> Result<Option<EngineIntrospectionChanged>> {
        let mut next = public_engine_snapshot(
            engine,
            self.snapshot.revision,
            self.snapshot.media_capabilities.revision,
        );
        if !self
            .snapshot
            .media_capabilities
            .same_state(&next.media_capabilities)
        {
            next.media_capabilities.revision = self
                .snapshot
                .media_capabilities
                .revision
                .checked_add(1)
                .ok_or_else(|| {
                    revision_exhausted(
                        "superi-api.engine_introspection",
                        "synchronize_media_capabilities",
                        "nested media capability revision is exhausted",
                    )
                })?;
        }
        if self.snapshot.same_state(&next) {
            return Ok(None);
        }
        next.revision = self.snapshot.revision.checked_add(1).ok_or_else(|| {
            revision_exhausted(
                "superi-api.engine_introspection",
                "synchronize",
                "engine introspection revision is exhausted",
            )
        })?;
        self.snapshot = next.clone();
        Ok(Some(EngineIntrospectionChanged::new(next)))
    }
}

pub(crate) fn public_engine_snapshot(
    engine: &InternalIntrospectionSnapshot,
    revision: u64,
    media_revision: u64,
) -> EngineIntrospectionSnapshot {
    EngineIntrospectionSnapshot {
        schema_version: ENGINE_INTROSPECTION_SCHEMA_VERSION.clone(),
        revision,
        media_capabilities: public_snapshot(engine.media_capabilities(), media_revision),
        resources: engine.resources().map(public_resource_status),
        phase: public_lifecycle_phase(engine.phase()),
        lifecycle_revision: engine.lifecycle_revision(),
        state_revision: engine.state_revision(),
        recovery_revision: engine.recovery_revision(),
        lifetime: engine.lifetime(),
        health: public_health(engine.health()),
        subsystems: engine
            .subsystems()
            .iter()
            .map(public_subsystem_status)
            .collect(),
        workflows: engine
            .workflows()
            .iter()
            .map(public_workflow_status)
            .collect(),
        active_failures: engine
            .active_failures()
            .iter()
            .map(public_failure_status)
            .collect(),
    }
}

fn public_subsystem_status(value: &InternalSubsystemStatus) -> EngineSubsystemStatus {
    EngineSubsystemStatus {
        subsystem: public_subsystem(value.subsystem()),
        state: public_subsystem_state(value.state()),
    }
}

fn public_workflow_status(value: &InternalWorkflowStatus) -> EngineWorkflowStatus {
    EngineWorkflowStatus {
        workflow: public_workflow(value.work()),
        available: value.available(),
        blocking_subsystem: value.blocking_subsystem().map(public_subsystem),
    }
}

fn public_failure_status(value: &InternalFailureStatus) -> EngineFailureStatus {
    let safe = value.user_safe_error();
    EngineFailureStatus {
        subsystem: public_subsystem(value.subsystem()),
        disposition: public_failure_disposition(value.disposition()),
        recovery_request: value.recovery_request().map(public_recovery_request),
        recovery_in_progress: value.recovery_in_progress(),
        error: EngineUserSafeError {
            category: safe.category().code().to_owned(),
            recoverability: safe.recoverability().code().to_owned(),
            code: safe.code().to_owned(),
            title: safe.title().to_owned(),
            action: safe.action().to_owned(),
        },
    }
}

fn public_resource_status(value: &InternalResourceStatus) -> EngineResourceStatus {
    EngineResourceStatus {
        revision: value.revision(),
        total_limit_bytes: value.total_limit_bytes(),
        total_used_bytes: value.total_used_bytes(),
        available_bytes: value.available_bytes(),
        peak_used_bytes: value.peak_used_bytes(),
        active_reservations: value.active_reservations(),
        denied_reservations: value.denied_reservations(),
        reclaim_attempts: value.reclaim_attempts(),
        reclaimed_bytes: value.reclaimed_bytes(),
        classes: value
            .classes()
            .iter()
            .map(public_resource_class_status)
            .collect(),
    }
}

fn public_resource_class_status(value: &InternalResourceClassStatus) -> EngineResourceClassStatus {
    EngineResourceClassStatus {
        class: public_resource_class(value.class()),
        protected_bytes: value.protected_bytes(),
        hard_limit_bytes: value.hard_limit_bytes(),
        used_bytes: value.used_bytes(),
        available_bytes: value.available_bytes(),
        peak_used_bytes: value.peak_used_bytes(),
        active_reservations: value.active_reservations(),
        denied_reservations: value.denied_reservations(),
        reclaim_attempts: value.reclaim_attempts(),
        reclaimed_bytes: value.reclaimed_bytes(),
    }
}

pub(crate) fn public_lifecycle_phase(value: InternalLifecyclePhase) -> EngineLifecyclePhase {
    match value {
        InternalLifecyclePhase::Starting => EngineLifecyclePhase::Starting,
        InternalLifecyclePhase::Running => EngineLifecyclePhase::Running,
        InternalLifecyclePhase::Pausing => EngineLifecyclePhase::Pausing,
        InternalLifecyclePhase::Paused => EngineLifecyclePhase::Paused,
        InternalLifecyclePhase::Resuming => EngineLifecyclePhase::Resuming,
        InternalLifecyclePhase::PreparingSleep => EngineLifecyclePhase::PreparingSleep,
        InternalLifecyclePhase::Sleeping => EngineLifecyclePhase::Sleeping,
        InternalLifecyclePhase::Waking => EngineLifecyclePhase::Waking,
        InternalLifecyclePhase::Stopping => EngineLifecyclePhase::Stopping,
        InternalLifecyclePhase::Stopped => EngineLifecyclePhase::Stopped,
        InternalLifecyclePhase::Failed => EngineLifecyclePhase::Failed,
        InternalLifecyclePhase::Unknown => EngineLifecyclePhase::Unknown,
        _ => EngineLifecyclePhase::Unknown,
    }
}

fn public_health(value: InternalHealth) -> EngineHealth {
    match value {
        InternalHealth::Healthy => EngineHealth::Healthy,
        InternalHealth::Degraded => EngineHealth::Degraded,
        InternalHealth::Failed => EngineHealth::Failed,
        _ => EngineHealth::Unknown,
    }
}

pub(crate) fn public_subsystem(value: InternalSubsystem) -> EngineSubsystem {
    match value {
        InternalSubsystem::SharedState => EngineSubsystem::SharedState,
        InternalSubsystem::Projects => EngineSubsystem::Projects,
        InternalSubsystem::Devices => EngineSubsystem::Devices,
        InternalSubsystem::Playback => EngineSubsystem::Playback,
        InternalSubsystem::Rendering => EngineSubsystem::Rendering,
        InternalSubsystem::Export => EngineSubsystem::Export,
        _ => EngineSubsystem::Unknown,
    }
}

fn public_subsystem_state(value: InternalSubsystemState) -> EngineSubsystemState {
    match value {
        InternalSubsystemState::Offline => EngineSubsystemState::Offline,
        InternalSubsystemState::Initializing => EngineSubsystemState::Initializing,
        InternalSubsystemState::Ready => EngineSubsystemState::Ready,
        InternalSubsystemState::Degraded => EngineSubsystemState::Degraded,
        InternalSubsystemState::Recovering => EngineSubsystemState::Recovering,
        InternalSubsystemState::PreparingSleep => EngineSubsystemState::PreparingSleep,
        InternalSubsystemState::Sleeping => EngineSubsystemState::Sleeping,
        InternalSubsystemState::Waking => EngineSubsystemState::Waking,
        InternalSubsystemState::Stopping => EngineSubsystemState::Stopping,
        InternalSubsystemState::Failed => EngineSubsystemState::Failed,
        _ => EngineSubsystemState::Unknown,
    }
}

pub(crate) fn public_workflow(value: InternalWorkflow) -> EngineWorkflow {
    match value {
        InternalWorkflow::Playback => EngineWorkflow::Playback,
        InternalWorkflow::Rendering => EngineWorkflow::Rendering,
        InternalWorkflow::Export => EngineWorkflow::Export,
        _ => EngineWorkflow::Unknown,
    }
}

fn public_failure_disposition(value: InternalFailureDisposition) -> EngineFailureDisposition {
    match value {
        InternalFailureDisposition::RetryAvailable => EngineFailureDisposition::RetryAvailable,
        InternalFailureDisposition::ContinueDegraded => EngineFailureDisposition::ContinueDegraded,
        InternalFailureDisposition::UserCorrectionRequired => {
            EngineFailureDisposition::UserCorrectionRequired
        }
        InternalFailureDisposition::RestartRequired => EngineFailureDisposition::RestartRequired,
        _ => EngineFailureDisposition::Unknown,
    }
}

pub(crate) fn public_recovery_request(value: InternalRecoveryRequest) -> EngineRecoveryRequest {
    match value {
        InternalRecoveryRequest::RetryOperation => EngineRecoveryRequest::RetryOperation,
        InternalRecoveryRequest::RestoreSubsystem => EngineRecoveryRequest::RestoreSubsystem,
        InternalRecoveryRequest::ApplyUserCorrection => EngineRecoveryRequest::ApplyUserCorrection,
        _ => EngineRecoveryRequest::Unknown,
    }
}

const fn public_resource_class(value: InternalResourceClass) -> EngineResourceClass {
    match value {
        InternalResourceClass::DecodeBuffer => EngineResourceClass::DecodeBuffer,
        InternalResourceClass::GpuMemory => EngineResourceClass::GpuMemory,
        InternalResourceClass::Cache => EngineResourceClass::Cache,
        InternalResourceClass::Audio => EngineResourceClass::Audio,
        InternalResourceClass::Ai => EngineResourceClass::Ai,
        InternalResourceClass::Export => EngineResourceClass::Export,
    }
}

fn revision_exhausted(
    component: &'static str,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(component, operation))
}
