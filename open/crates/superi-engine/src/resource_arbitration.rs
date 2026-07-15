//! Shared finite-resource arbitration for coherent engine operation.
//!
//! This module owns the engine-wide managed-byte envelope across decoded media, GPU payloads,
//! caches, audio preparation, AI work, and exports. Lower subsystem allocators remain authoritative
//! for their own resources. A caller binds their real resource to a [`ResourceReservation`], so the
//! arbiter never converts or copies timing, precision, metadata, color, or alpha-bearing values.
//!
//! The configured protected values are reclaim floors, not permanently partitioned allocations.
//! Unused protected capacity may be borrowed. Under pressure, cooperative owners release their own
//! reservations in a fixed order and the arbiter measures the resulting accounting change. The
//! engine can then admit the exact request or return a typed scheduling fallback without confusing
//! ordinary finite-capacity pressure with a subsystem lifecycle failure.

use std::cell::Cell;
use std::fmt;
use std::sync::{Arc, Mutex, Weak};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const COMPONENT: &str = "engine.resource_arbitration";
const RESOURCE_CLASS_COUNT: usize = 6;

thread_local! {
    static IN_RECLAIM_CALLBACK: Cell<bool> = const { Cell::new(false) };
}

/// One class of finite resource managed by the shared engine envelope.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResourceClass {
    /// Decoded video or audio payloads waiting for a consumer.
    DecodeBuffer,
    /// Managed GPU textures, buffers, and other device-resident payloads.
    GpuMemory,
    /// Reconstructible cache entries.
    Cache,
    /// Audio payloads prepared outside the real-time callback.
    Audio,
    /// AI models, tensors, and temporary inference payloads.
    Ai,
    /// Export queues, encoder staging, and retained export outputs.
    Export,
}

impl ResourceClass {
    /// Every resource class in stable configuration order.
    pub const ALL: [Self; RESOURCE_CLASS_COUNT] = [
        Self::DecodeBuffer,
        Self::GpuMemory,
        Self::Cache,
        Self::Audio,
        Self::Ai,
        Self::Export,
    ];

    const RECLAIM_ORDER: [Self; RESOURCE_CLASS_COUNT] = [
        Self::Cache,
        Self::Ai,
        Self::Export,
        Self::DecodeBuffer,
        Self::GpuMemory,
        Self::Audio,
    ];

    const fn index(self) -> usize {
        match self {
            Self::DecodeBuffer => 0,
            Self::GpuMemory => 1,
            Self::Cache => 2,
            Self::Audio => 3,
            Self::Ai => 4,
            Self::Export => 5,
        }
    }

    const fn code(self) -> &'static str {
        match self {
            Self::DecodeBuffer => "decode_buffer",
            Self::GpuMemory => "gpu_memory",
            Self::Cache => "cache",
            Self::Audio => "audio",
            Self::Ai => "ai",
            Self::Export => "export",
        }
    }
}

/// The runtime path requesting a reservation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResourceConsumer {
    /// Foreground playback.
    Playback,
    /// Interactive or offline rendering.
    Render,
    /// Export encoding or delivery.
    Export,
    /// Noninteractive preparation or maintenance.
    Background,
    /// Explicit recovery work after a prior release.
    Recovery,
}

impl ResourceConsumer {
    const fn code(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::Render => "render",
            Self::Export => "export",
            Self::Background => "background",
            Self::Recovery => "recovery",
        }
    }
}

/// Protected and hard byte limits for one resource class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceClassBudget {
    class: ResourceClass,
    protected_bytes: u64,
    hard_limit_bytes: u64,
}

impl ResourceClassBudget {
    /// Creates one class budget.
    pub fn new(class: ResourceClass, protected_bytes: u64, hard_limit_bytes: u64) -> Result<Self> {
        if hard_limit_bytes == 0 {
            return Err(invalid_input(
                "configure_class_budget",
                "resource class hard limit must be greater than zero",
            ));
        }
        if protected_bytes > hard_limit_bytes {
            return Err(invalid_input(
                "configure_class_budget",
                "resource class protected bytes exceed its hard limit",
            ));
        }
        Ok(Self {
            class,
            protected_bytes,
            hard_limit_bytes,
        })
    }

    /// Returns the configured class.
    #[must_use]
    pub const fn class(&self) -> ResourceClass {
        self.class
    }

    /// Returns the amount protected from other classes during global reclaim.
    #[must_use]
    pub const fn protected_bytes(&self) -> u64 {
        self.protected_bytes
    }

    /// Returns the class-local hard limit.
    #[must_use]
    pub const fn hard_limit_bytes(&self) -> u64 {
        self.hard_limit_bytes
    }
}

/// Complete finite-resource configuration for one engine instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceArbitrationConfig {
    total_bytes: u64,
    budgets: [ResourceClassBudget; RESOURCE_CLASS_COUNT],
}

impl ResourceArbitrationConfig {
    /// Creates a complete configuration and canonicalizes budgets by class.
    pub fn new<const N: usize>(
        total_bytes: u64,
        budgets: [ResourceClassBudget; N],
    ) -> Result<Self> {
        if total_bytes == 0 {
            return Err(invalid_input(
                "configure_resource_arbiter",
                "the shared resource budget must be greater than zero",
            ));
        }

        let mut canonical = [None; RESOURCE_CLASS_COUNT];
        let mut protected_total = 0_u64;
        for budget in budgets {
            if budget.hard_limit_bytes > total_bytes {
                return Err(invalid_input(
                    "configure_resource_arbiter",
                    "a resource class hard limit exceeds the shared budget",
                ));
            }
            let index = budget.class.index();
            if canonical[index].replace(budget).is_some() {
                return Err(invalid_input(
                    "configure_resource_arbiter",
                    "resource configuration contains a duplicate class budget",
                ));
            }
            protected_total = protected_total
                .checked_add(budget.protected_bytes)
                .ok_or_else(|| {
                    invalid_input(
                        "configure_resource_arbiter",
                        "resource protected-byte total overflowed",
                    )
                })?;
        }

        if canonical.iter().any(Option::is_none) {
            return Err(invalid_input(
                "configure_resource_arbiter",
                "resource configuration must contain every class exactly once",
            ));
        }
        if protected_total > total_bytes {
            return Err(invalid_input(
                "configure_resource_arbiter",
                "protected resource floors exceed the shared budget",
            ));
        }

        Ok(Self {
            total_bytes,
            budgets: std::array::from_fn(|index| {
                canonical[index].expect("complete budgets were validated")
            }),
        })
    }

    /// Returns the engine-wide managed-byte hard limit.
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Returns the canonical budget for a class.
    #[must_use]
    pub const fn budget(&self, class: ResourceClass) -> &ResourceClassBudget {
        &self.budgets[class.index()]
    }

    /// Returns all class budgets in stable class order.
    #[must_use]
    pub const fn budgets(&self) -> &[ResourceClassBudget; RESOURCE_CLASS_COUNT] {
        &self.budgets
    }
}

/// One exact managed-byte reservation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceRequest {
    class: ResourceClass,
    consumer: ResourceConsumer,
    bytes: u64,
}

impl ResourceRequest {
    /// Creates a nonempty request.
    pub fn new(class: ResourceClass, consumer: ResourceConsumer, bytes: u64) -> Result<Self> {
        if bytes == 0 {
            return Err(invalid_input(
                "create_resource_request",
                "resource request bytes must be greater than zero",
            ));
        }
        Ok(Self {
            class,
            consumer,
            bytes,
        })
    }

    /// Returns the requested resource class.
    #[must_use]
    pub const fn class(&self) -> ResourceClass {
        self.class
    }

    /// Returns the requesting runtime path.
    #[must_use]
    pub const fn consumer(&self) -> ResourceConsumer {
        self.consumer
    }

    /// Returns the exact requested managed bytes.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// The kind of finite-capacity pressure preventing immediate admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourcePressureKind {
    /// Only the shared hard limit is exceeded.
    GlobalBudget,
    /// Only the class-local hard limit is exceeded.
    ClassBudget,
    /// Both shared and class-local limits are exceeded.
    GlobalAndClassBudget,
}

/// A semantic fallback selected for a denied request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceFallback {
    /// Decode less media ahead of the playback head.
    ReduceDecodeLookahead,
    /// Defer GPU work without changing source or render semantics.
    DeferGpuWork,
    /// Read through or recompute instead of retaining a cache entry.
    BypassCache,
    /// Preserve the audio-master clock and emit timed silence.
    PreservePlaybackClockWithSilence,
    /// Defer AI work until finite capacity becomes available.
    DeferAi,
    /// Pause export admission while preserving the export request.
    PauseExport,
    /// Retry a generic scheduling request later.
    RetryLater,
}

/// A cooperative release request sent to one resource owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceReclaimRequest {
    class: ResourceClass,
    requester: ResourceRequest,
    bytes_to_release: u64,
    pressure_kind: ResourcePressureKind,
}

impl ResourceReclaimRequest {
    /// Returns the class whose owner should release reservations.
    #[must_use]
    pub const fn class(&self) -> ResourceClass {
        self.class
    }

    /// Returns the request that caused pressure.
    #[must_use]
    pub const fn requester(&self) -> ResourceRequest {
        self.requester
    }

    /// Returns the requested release amount.
    #[must_use]
    pub const fn bytes_to_release(&self) -> u64 {
        self.bytes_to_release
    }

    /// Returns the pressure classification observed before the callback.
    #[must_use]
    pub const fn pressure_kind(&self) -> ResourcePressureKind {
        self.pressure_kind
    }
}

/// A subsystem-owned cooperative release seam.
///
/// Implementations release their own [`ResourceReservation`] values. The callback runs outside the
/// accounting mutex, but admission remains serialized. Calling [`EngineResourceArbiter::reserve`]
/// recursively from this callback is rejected before it can wait on the admission gate.
pub trait ResourceReclaimer: Send + Sync {
    /// Cooperatively releases reservations up to the requested amount when possible.
    fn reclaim(&self, request: ResourceReclaimRequest) -> Result<()>;
}

/// Classified evidence retained when a cooperative owner reports failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceReclaimFailure {
    category: ErrorCategory,
    recoverability: Recoverability,
    message: String,
}

impl ResourceReclaimFailure {
    fn from_error(error: &Error) -> Self {
        Self {
            category: error.category(),
            recoverability: error.recoverability(),
            message: error.message().to_owned(),
        }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the callback's recovery classification.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    /// Returns the callback's concise diagnostic summary.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Outcome of one class-specific cooperative reclaim attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceReclaimStatus {
    /// No owner was registered for the class.
    Unavailable,
    /// The callback released at least one managed byte.
    Released,
    /// The callback succeeded but no managed bytes were released.
    NoProgress,
    /// The callback returned a classified error.
    Failed,
}

/// Observable evidence from one class-specific reclaim attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceReclaimAttempt {
    class: ResourceClass,
    requested_bytes: u64,
    released_bytes: u64,
    status: ResourceReclaimStatus,
    failure: Option<ResourceReclaimFailure>,
}

impl ResourceReclaimAttempt {
    /// Returns the class asked to release capacity.
    #[must_use]
    pub const fn class(&self) -> ResourceClass {
        self.class
    }

    /// Returns the requested release amount.
    #[must_use]
    pub const fn requested_bytes(&self) -> u64 {
        self.requested_bytes
    }

    /// Returns the authoritative observed release amount.
    #[must_use]
    pub const fn released_bytes(&self) -> u64 {
        self.released_bytes
    }

    /// Returns the attempt outcome.
    #[must_use]
    pub const fn status(&self) -> ResourceReclaimStatus {
        self.status
    }

    /// Returns classified callback failure evidence, when present.
    #[must_use]
    pub const fn failure(&self) -> Option<&ResourceReclaimFailure> {
        self.failure.as_ref()
    }
}

/// Admission evidence shared by granted and degraded outcomes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceAdmissionEvidence {
    used_before_bytes: u64,
    used_after_reclaim_bytes: u64,
    reclaimed_bytes: u64,
    attempts: Vec<ResourceReclaimAttempt>,
}

impl ResourceAdmissionEvidence {
    /// Returns shared usage before cooperative reclaim began.
    #[must_use]
    pub const fn used_before_bytes(&self) -> u64 {
        self.used_before_bytes
    }

    /// Returns shared usage after cooperative reclaim and before admission.
    #[must_use]
    pub const fn used_after_reclaim_bytes(&self) -> u64 {
        self.used_after_reclaim_bytes
    }

    /// Returns the authoritative shared-byte reduction observed during reclaim.
    #[must_use]
    pub const fn reclaimed_bytes(&self) -> u64 {
        self.reclaimed_bytes
    }

    /// Returns reclaim attempts in deterministic execution order.
    #[must_use]
    pub fn attempts(&self) -> &[ResourceReclaimAttempt] {
        &self.attempts
    }
}

/// An admitted reservation and its pressure evidence.
#[derive(Debug)]
pub struct ResourceGrant {
    request: ResourceRequest,
    reservation: ResourceReservation,
    evidence: ResourceAdmissionEvidence,
}

impl ResourceGrant {
    /// Returns the admitted request.
    #[must_use]
    pub const fn request(&self) -> ResourceRequest {
        self.request
    }

    /// Returns the live reservation.
    #[must_use]
    pub const fn reservation(&self) -> &ResourceReservation {
        &self.reservation
    }

    /// Returns deterministic admission and reclaim evidence.
    #[must_use]
    pub const fn evidence(&self) -> &ResourceAdmissionEvidence {
        &self.evidence
    }

    /// Consumes the grant and returns the live reservation.
    #[must_use]
    pub fn into_reservation(self) -> ResourceReservation {
        self.reservation
    }

    /// Binds a real opaque resource to this exact reservation.
    #[must_use]
    pub fn bind<T>(self, value: T) -> ArbitratedResource<T> {
        ArbitratedResource {
            value,
            reservation: self.reservation,
        }
    }
}

/// A denied request with a semantic scheduling fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceDegradation {
    request: ResourceRequest,
    fallback: ResourceFallback,
    pressure_kind: ResourcePressureKind,
    total_available_bytes: u64,
    class_available_bytes: u64,
    shortage_bytes: u64,
    evidence: ResourceAdmissionEvidence,
}

impl ResourceDegradation {
    /// Returns the denied request.
    #[must_use]
    pub const fn request(&self) -> ResourceRequest {
        self.request
    }

    /// Returns the semantic fallback selected for the caller.
    #[must_use]
    pub const fn fallback(&self) -> ResourceFallback {
        self.fallback
    }

    /// Returns the remaining pressure classification.
    #[must_use]
    pub const fn pressure_kind(&self) -> ResourcePressureKind {
        self.pressure_kind
    }

    /// Returns unused shared capacity after cooperative reclaim.
    #[must_use]
    pub const fn total_available_bytes(&self) -> u64 {
        self.total_available_bytes
    }

    /// Returns unused class-local capacity after cooperative reclaim.
    #[must_use]
    pub const fn class_available_bytes(&self) -> u64 {
        self.class_available_bytes
    }

    /// Returns the minimum additional release required for admission.
    #[must_use]
    pub const fn shortage_bytes(&self) -> u64 {
        self.shortage_bytes
    }

    /// Returns deterministic admission and reclaim evidence.
    #[must_use]
    pub const fn evidence(&self) -> &ResourceAdmissionEvidence {
        &self.evidence
    }
}

/// An exact admission or a typed degraded scheduling outcome.
#[derive(Debug)]
pub enum ResourceAdmission {
    /// The exact request was admitted.
    Granted(ResourceGrant),
    /// The exact request could not be admitted without violating a hard limit.
    Degraded(ResourceDegradation),
}

/// A real resource whose lifetime owns an exact managed-byte reservation.
#[derive(Debug)]
pub struct ArbitratedResource<T> {
    value: T,
    reservation: ResourceReservation,
}

impl<T> ArbitratedResource<T> {
    /// Returns the original resource without conversion or semantic projection.
    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Returns the reservation bound to the resource.
    #[must_use]
    pub const fn reservation(&self) -> &ResourceReservation {
        &self.reservation
    }

    /// Consumes the binding, releases its reservation, and returns the original value.
    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }
}

/// One live noncloneable managed-byte reservation.
pub struct ResourceReservation {
    inner: Weak<ArbiterInner>,
    id: u64,
    request: ResourceRequest,
}

impl ResourceReservation {
    /// Returns the reserved class.
    #[must_use]
    pub const fn class(&self) -> ResourceClass {
        self.request.class
    }

    /// Returns the consumer that owns this reservation.
    #[must_use]
    pub const fn consumer(&self) -> ResourceConsumer {
        self.request.consumer
    }

    /// Returns the exact managed-byte charge.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.request.bytes
    }
}

impl fmt::Debug for ResourceReservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResourceReservation")
            .field("id", &self.id)
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

impl Drop for ResourceReservation {
    fn drop(&mut self) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        let mut state = match inner.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let index = self.request.class.index();
        state.total_used_bytes = state.total_used_bytes.saturating_sub(self.request.bytes);
        state.class_used_bytes[index] =
            state.class_used_bytes[index].saturating_sub(self.request.bytes);
        state.active_reservations = state.active_reservations.saturating_sub(1);
        state.class_active_reservations[index] =
            state.class_active_reservations[index].saturating_sub(1);
        state.revision = state.revision.saturating_add(1);
    }
}

/// Exact observable accounting for one resource class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceClassSnapshot {
    class: ResourceClass,
    budget: ResourceClassBudget,
    used_bytes: u64,
    peak_used_bytes: u64,
    active_reservations: u64,
    denied_reservations: u64,
    reclaim_attempts: u64,
    reclaimed_bytes: u64,
}

impl ResourceClassSnapshot {
    /// Returns the resource class.
    #[must_use]
    pub const fn class(&self) -> ResourceClass {
        self.class
    }

    /// Returns the configured protected and hard limits.
    #[must_use]
    pub const fn budget(&self) -> ResourceClassBudget {
        self.budget
    }

    /// Returns current managed-byte usage.
    #[must_use]
    pub const fn used_bytes(&self) -> u64 {
        self.used_bytes
    }

    /// Returns unused class-local hard-limit capacity.
    #[must_use]
    pub const fn available_bytes(&self) -> u64 {
        self.budget.hard_limit_bytes.saturating_sub(self.used_bytes)
    }

    /// Returns peak managed-byte usage.
    #[must_use]
    pub const fn peak_used_bytes(&self) -> u64 {
        self.peak_used_bytes
    }

    /// Returns live reservations for this class.
    #[must_use]
    pub const fn active_reservations(&self) -> u64 {
        self.active_reservations
    }

    /// Returns denied requests for this class.
    #[must_use]
    pub const fn denied_reservations(&self) -> u64 {
        self.denied_reservations
    }

    /// Returns cooperative reclaim attempts targeting this class.
    #[must_use]
    pub const fn reclaim_attempts(&self) -> u64 {
        self.reclaim_attempts
    }

    /// Returns managed bytes observed released by callbacks for this class.
    #[must_use]
    pub const fn reclaimed_bytes(&self) -> u64 {
        self.reclaimed_bytes
    }
}

/// Exact observable accounting for the complete engine envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceArbitrationSnapshot {
    total_limit_bytes: u64,
    total_used_bytes: u64,
    peak_used_bytes: u64,
    active_reservations: u64,
    denied_reservations: u64,
    reclaim_attempts: u64,
    reclaimed_bytes: u64,
    revision: u64,
    classes: [ResourceClassSnapshot; RESOURCE_CLASS_COUNT],
}

impl ResourceArbitrationSnapshot {
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

    /// Returns all live reservations.
    #[must_use]
    pub const fn active_reservations(&self) -> u64 {
        self.active_reservations
    }

    /// Returns all denied requests.
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

    /// Returns the monotonic accounting revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns exact accounting for one class.
    #[must_use]
    pub const fn class(&self, class: ResourceClass) -> &ResourceClassSnapshot {
        &self.classes[class.index()]
    }

    /// Returns every class snapshot in stable class order.
    #[must_use]
    pub const fn classes(&self) -> &[ResourceClassSnapshot; RESOURCE_CLASS_COUNT] {
        &self.classes
    }
}

/// Thread-safe shared finite-resource arbiter.
#[derive(Clone)]
pub struct EngineResourceArbiter {
    inner: Arc<ArbiterInner>,
}

impl EngineResourceArbiter {
    /// Creates an empty arbiter from a validated configuration.
    #[must_use]
    pub fn new(config: ResourceArbitrationConfig) -> Self {
        Self {
            inner: Arc::new(ArbiterInner {
                config,
                admission_gate: Mutex::new(()),
                state: Mutex::new(ResourceState::default()),
                reclaimers: Mutex::new(std::array::from_fn(|_| None)),
            }),
        }
    }

    /// Installs or removes the cooperative owner for one class.
    pub fn set_reclaimer(
        &self,
        class: ResourceClass,
        reclaimer: Option<Arc<dyn ResourceReclaimer>>,
    ) -> Result<()> {
        let mut reclaimers = self.inner.reclaimers.lock().map_err(|_| {
            internal(
                "set_resource_reclaimer",
                "resource reclaimer registry lock is poisoned",
            )
        })?;
        reclaimers[class.index()] = reclaimer;
        Ok(())
    }

    /// Admits an exact request, cooperatively reclaims, or returns a typed fallback.
    pub fn reserve(&self, request: ResourceRequest) -> Result<ResourceAdmission> {
        if IN_RECLAIM_CALLBACK.with(Cell::get) {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "resource admission cannot recurse from a reclaim callback",
            )
            .with_context(request_context("reserve_resource", request)));
        }

        let _admission = self.inner.admission_gate.lock().map_err(|_| {
            internal(
                "reserve_resource",
                "resource admission gate lock is poisoned",
            )
        })?;
        let used_before_bytes = self.total_used()?;
        let mut attempts = Vec::new();
        let class_budget = self.inner.config.budgets[request.class.index()];
        let request_can_fit_empty = request.bytes <= self.inner.config.total_bytes
            && request.bytes <= class_budget.hard_limit_bytes;

        if request_can_fit_empty {
            if let Some(initial_pressure) = self.pressure(request)? {
                self.reclaim_for(request, initial_pressure, &mut attempts)?;
            }
        }

        let mut state = self.inner.state.lock().map_err(|_| {
            internal(
                "reserve_resource",
                "resource accounting state lock is poisoned",
            )
        })?;
        let index = request.class.index();
        let used_after_reclaim_bytes = state.total_used_bytes;
        let evidence = ResourceAdmissionEvidence {
            used_before_bytes,
            used_after_reclaim_bytes,
            reclaimed_bytes: used_before_bytes.saturating_sub(used_after_reclaim_bytes),
            attempts,
        };
        let total_available_bytes = self
            .inner
            .config
            .total_bytes
            .saturating_sub(state.total_used_bytes);
        let class_available_bytes = self.inner.config.budgets[index]
            .hard_limit_bytes
            .saturating_sub(state.class_used_bytes[index]);

        if request.bytes <= total_available_bytes && request.bytes <= class_available_bytes {
            let id = state.next_reservation_id;
            state.next_reservation_id = state.next_reservation_id.saturating_add(1);
            state.total_used_bytes = state.total_used_bytes.saturating_add(request.bytes);
            state.class_used_bytes[index] =
                state.class_used_bytes[index].saturating_add(request.bytes);
            state.peak_used_bytes = state.peak_used_bytes.max(state.total_used_bytes);
            state.class_peak_used_bytes[index] =
                state.class_peak_used_bytes[index].max(state.class_used_bytes[index]);
            state.active_reservations = state.active_reservations.saturating_add(1);
            state.class_active_reservations[index] =
                state.class_active_reservations[index].saturating_add(1);
            state.revision = state.revision.saturating_add(1);
            drop(state);
            return Ok(ResourceAdmission::Granted(ResourceGrant {
                request,
                reservation: ResourceReservation {
                    inner: Arc::downgrade(&self.inner),
                    id,
                    request,
                },
                evidence,
            }));
        }

        let pressure =
            Pressure::from_available(request.bytes, total_available_bytes, class_available_bytes)
                .expect("a request that cannot fit has finite pressure");
        state.denied_reservations = state.denied_reservations.saturating_add(1);
        state.class_denied_reservations[index] =
            state.class_denied_reservations[index].saturating_add(1);
        state.revision = state.revision.saturating_add(1);
        drop(state);
        Ok(ResourceAdmission::Degraded(ResourceDegradation {
            request,
            fallback: fallback_for(request),
            pressure_kind: pressure.kind,
            total_available_bytes,
            class_available_bytes,
            shortage_bytes: pressure.shortage_bytes(),
            evidence,
        }))
    }

    /// Returns an exact point-in-time accounting snapshot.
    pub fn snapshot(&self) -> Result<ResourceArbitrationSnapshot> {
        let state = self.inner.state.lock().map_err(|_| {
            internal(
                "snapshot_resources",
                "resource accounting state lock is poisoned",
            )
        })?;
        Ok(ResourceArbitrationSnapshot {
            total_limit_bytes: self.inner.config.total_bytes,
            total_used_bytes: state.total_used_bytes,
            peak_used_bytes: state.peak_used_bytes,
            active_reservations: state.active_reservations,
            denied_reservations: state.denied_reservations,
            reclaim_attempts: state.reclaim_attempts,
            reclaimed_bytes: state.reclaimed_bytes,
            revision: state.revision,
            classes: std::array::from_fn(|index| ResourceClassSnapshot {
                class: ResourceClass::ALL[index],
                budget: self.inner.config.budgets[index],
                used_bytes: state.class_used_bytes[index],
                peak_used_bytes: state.class_peak_used_bytes[index],
                active_reservations: state.class_active_reservations[index],
                denied_reservations: state.class_denied_reservations[index],
                reclaim_attempts: state.class_reclaim_attempts[index],
                reclaimed_bytes: state.class_reclaimed_bytes[index],
            }),
        })
    }

    fn total_used(&self) -> Result<u64> {
        self.inner
            .state
            .lock()
            .map(|state| state.total_used_bytes)
            .map_err(|_| {
                internal(
                    "read_resource_usage",
                    "resource accounting state lock is poisoned",
                )
            })
    }

    fn pressure(&self, request: ResourceRequest) -> Result<Option<Pressure>> {
        let state = self.inner.state.lock().map_err(|_| {
            internal(
                "measure_resource_pressure",
                "resource accounting state lock is poisoned",
            )
        })?;
        let total_available = self
            .inner
            .config
            .total_bytes
            .saturating_sub(state.total_used_bytes);
        let index = request.class.index();
        let class_available = self.inner.config.budgets[index]
            .hard_limit_bytes
            .saturating_sub(state.class_used_bytes[index]);
        Ok(Pressure::from_available(
            request.bytes,
            total_available,
            class_available,
        ))
    }

    fn reclaim_for(
        &self,
        request: ResourceRequest,
        initial_pressure: Pressure,
        attempts: &mut Vec<ResourceReclaimAttempt>,
    ) -> Result<()> {
        let mut candidates = Vec::with_capacity(RESOURCE_CLASS_COUNT + 1);
        if initial_pressure.class_shortage > 0 {
            candidates.push(request.class);
        }
        for class in ResourceClass::RECLAIM_ORDER {
            if !candidates.contains(&class) {
                candidates.push(class);
            }
        }

        for class in candidates {
            let Some(pressure) = self.pressure(request)? else {
                break;
            };
            let state = self.inner.state.lock().map_err(|_| {
                internal(
                    "plan_resource_reclaim",
                    "resource accounting state lock is poisoned",
                )
            })?;
            let class_index = class.index();
            let class_used = state.class_used_bytes[class_index];
            let protected = self.inner.config.budgets[class_index].protected_bytes;
            let class_reclaimable = if class == request.class && pressure.class_shortage > 0 {
                class_used
            } else {
                class_used.saturating_sub(protected)
            };
            let class_need = if class == request.class {
                pressure.class_shortage
            } else {
                0
            };
            let requested_bytes = pressure
                .global_shortage
                .max(class_need)
                .min(class_reclaimable);
            drop(state);
            if requested_bytes == 0 {
                continue;
            }

            let reclaimer = self.inner.reclaimers.lock().map_err(|_| {
                internal(
                    "select_resource_reclaimer",
                    "resource reclaimer registry lock is poisoned",
                )
            })?[class_index]
                .clone();
            let before_total = self.total_used()?;
            let before_class = self.class_used(class)?;
            let callback_result = if let Some(reclaimer) = reclaimer {
                let _callback_scope = ReclaimCallbackScope::enter();
                Some(reclaimer.reclaim(ResourceReclaimRequest {
                    class,
                    requester: request,
                    bytes_to_release: requested_bytes,
                    pressure_kind: pressure.kind,
                }))
            } else {
                None
            };
            let after_total = self.total_used()?;
            let after_class = self.class_used(class)?;
            let released_bytes = before_total.saturating_sub(after_total);
            let class_released_bytes = before_class.saturating_sub(after_class);

            let (status, failure) = match callback_result {
                None => (ResourceReclaimStatus::Unavailable, None),
                Some(Ok(())) if class_released_bytes > 0 => (ResourceReclaimStatus::Released, None),
                Some(Ok(())) => (ResourceReclaimStatus::NoProgress, None),
                Some(Err(error)) => (
                    ResourceReclaimStatus::Failed,
                    Some(ResourceReclaimFailure::from_error(&error)),
                ),
            };
            self.record_reclaim(class, class_released_bytes, released_bytes)?;
            attempts.push(ResourceReclaimAttempt {
                class,
                requested_bytes,
                released_bytes: class_released_bytes,
                status,
                failure,
            });
        }
        Ok(())
    }

    fn class_used(&self, class: ResourceClass) -> Result<u64> {
        self.inner
            .state
            .lock()
            .map(|state| state.class_used_bytes[class.index()])
            .map_err(|_| {
                internal(
                    "read_resource_class_usage",
                    "resource accounting state lock is poisoned",
                )
            })
    }

    fn record_reclaim(
        &self,
        class: ResourceClass,
        class_released_bytes: u64,
        released_bytes: u64,
    ) -> Result<()> {
        let mut state = self.inner.state.lock().map_err(|_| {
            internal(
                "record_resource_reclaim",
                "resource accounting state lock is poisoned",
            )
        })?;
        let index = class.index();
        state.reclaim_attempts = state.reclaim_attempts.saturating_add(1);
        state.class_reclaim_attempts[index] = state.class_reclaim_attempts[index].saturating_add(1);
        state.reclaimed_bytes = state.reclaimed_bytes.saturating_add(released_bytes);
        state.class_reclaimed_bytes[index] =
            state.class_reclaimed_bytes[index].saturating_add(class_released_bytes);
        state.revision = state.revision.saturating_add(1);
        Ok(())
    }
}

impl fmt::Debug for EngineResourceArbiter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EngineResourceArbiter")
            .field("config", &self.inner.config)
            .finish_non_exhaustive()
    }
}

struct ArbiterInner {
    config: ResourceArbitrationConfig,
    admission_gate: Mutex<()>,
    state: Mutex<ResourceState>,
    reclaimers: Mutex<[Option<Arc<dyn ResourceReclaimer>>; RESOURCE_CLASS_COUNT]>,
}

#[derive(Debug)]
struct ResourceState {
    total_used_bytes: u64,
    peak_used_bytes: u64,
    active_reservations: u64,
    denied_reservations: u64,
    reclaim_attempts: u64,
    reclaimed_bytes: u64,
    revision: u64,
    next_reservation_id: u64,
    class_used_bytes: [u64; RESOURCE_CLASS_COUNT],
    class_peak_used_bytes: [u64; RESOURCE_CLASS_COUNT],
    class_active_reservations: [u64; RESOURCE_CLASS_COUNT],
    class_denied_reservations: [u64; RESOURCE_CLASS_COUNT],
    class_reclaim_attempts: [u64; RESOURCE_CLASS_COUNT],
    class_reclaimed_bytes: [u64; RESOURCE_CLASS_COUNT],
}

impl Default for ResourceState {
    fn default() -> Self {
        Self {
            total_used_bytes: 0,
            peak_used_bytes: 0,
            active_reservations: 0,
            denied_reservations: 0,
            reclaim_attempts: 0,
            reclaimed_bytes: 0,
            revision: 0,
            next_reservation_id: 1,
            class_used_bytes: [0; RESOURCE_CLASS_COUNT],
            class_peak_used_bytes: [0; RESOURCE_CLASS_COUNT],
            class_active_reservations: [0; RESOURCE_CLASS_COUNT],
            class_denied_reservations: [0; RESOURCE_CLASS_COUNT],
            class_reclaim_attempts: [0; RESOURCE_CLASS_COUNT],
            class_reclaimed_bytes: [0; RESOURCE_CLASS_COUNT],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Pressure {
    kind: ResourcePressureKind,
    global_shortage: u64,
    class_shortage: u64,
}

impl Pressure {
    fn from_available(bytes: u64, total_available: u64, class_available: u64) -> Option<Self> {
        let global_shortage = bytes.saturating_sub(total_available);
        let class_shortage = bytes.saturating_sub(class_available);
        let kind = match (global_shortage > 0, class_shortage > 0) {
            (false, false) => return None,
            (true, false) => ResourcePressureKind::GlobalBudget,
            (false, true) => ResourcePressureKind::ClassBudget,
            (true, true) => ResourcePressureKind::GlobalAndClassBudget,
        };
        Some(Self {
            kind,
            global_shortage,
            class_shortage,
        })
    }

    const fn shortage_bytes(self) -> u64 {
        if self.global_shortage > self.class_shortage {
            self.global_shortage
        } else {
            self.class_shortage
        }
    }
}

struct ReclaimCallbackScope;

impl ReclaimCallbackScope {
    fn enter() -> Self {
        IN_RECLAIM_CALLBACK.with(|flag| flag.set(true));
        Self
    }
}

impl Drop for ReclaimCallbackScope {
    fn drop(&mut self) {
        IN_RECLAIM_CALLBACK.with(|flag| flag.set(false));
    }
}

const fn fallback_for(request: ResourceRequest) -> ResourceFallback {
    match (request.class, request.consumer) {
        (ResourceClass::Cache, _) => ResourceFallback::BypassCache,
        (ResourceClass::Export, _) | (_, ResourceConsumer::Export) => ResourceFallback::PauseExport,
        (ResourceClass::DecodeBuffer, ResourceConsumer::Playback) => {
            ResourceFallback::ReduceDecodeLookahead
        }
        (ResourceClass::GpuMemory, _) => ResourceFallback::DeferGpuWork,
        (ResourceClass::Audio, ResourceConsumer::Playback) => {
            ResourceFallback::PreservePlaybackClockWithSilence
        }
        (ResourceClass::Ai, _) => ResourceFallback::DeferAi,
        _ => ResourceFallback::RetryLater,
    }
}

fn request_context(operation: &'static str, request: ResourceRequest) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
        .with_field("class", request.class.code())
        .with_field("consumer", request.consumer.code())
        .with_field("bytes", request.bytes.to_string())
}

fn invalid_input(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
