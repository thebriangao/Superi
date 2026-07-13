//! Portable GPU memory budgeting, pressure delivery, and eviction cooperation.
//!
//! Wgpu does not expose one cross-platform driver budget or pressure callback.
//! This module therefore accounts for Superi-managed payload bytes and accepts
//! explicit platform pressure from the host. Driver allocation overhead and
//! backend suballocation remain outside these deterministic counters.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const COMPONENT: &str = "superi-gpu.memory-pool";
const MEMORY_CLASS_COUNT: usize = 4;

/// Soft and hard limits for Superi-managed GPU payload bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryBudget {
    soft_limit_bytes: u64,
    hard_limit_bytes: u64,
}

impl MemoryBudget {
    /// Creates a nonzero budget whose soft limit does not exceed its hard limit.
    pub fn new(soft_limit_bytes: u64, hard_limit_bytes: u64) -> Result<Self> {
        if soft_limit_bytes == 0 || hard_limit_bytes == 0 {
            return Err(invalid(
                "create_memory_budget",
                "GPU memory budget limits must be greater than zero",
            ));
        }
        if soft_limit_bytes > hard_limit_bytes {
            return Err(invalid(
                "create_memory_budget",
                "GPU memory budget soft limit must not exceed its hard limit",
            ));
        }
        Ok(Self {
            soft_limit_bytes,
            hard_limit_bytes,
        })
    }

    /// Creates a compatibility budget that only fails on integer exhaustion.
    #[must_use]
    pub const fn unbounded() -> Self {
        Self {
            soft_limit_bytes: u64::MAX,
            hard_limit_bytes: u64::MAX,
        }
    }

    /// Returns the level that starts cooperative reclamation.
    #[must_use]
    pub const fn soft_limit_bytes(self) -> u64 {
        self.soft_limit_bytes
    }

    /// Returns the level that no new reservation may exceed.
    #[must_use]
    pub const fn hard_limit_bytes(self) -> u64 {
        self.hard_limit_bytes
    }
}

/// Stable accounting classes for managed GPU payloads.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MemoryClass {
    /// Texture pixel storage, including mip levels, layers, and samples.
    Texture,
    /// Buffer payload storage.
    Buffer,
    /// Reconstructable frame, render, or analysis cache storage.
    Cache,
    /// Other explicitly accounted GPU payload storage.
    Other,
}

impl MemoryClass {
    /// Every stable accounting class.
    pub const ALL: &'static [Self] = &[Self::Texture, Self::Buffer, Self::Cache, Self::Other];

    /// Returns the stable diagnostic code for this class.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Texture => "texture",
            Self::Buffer => "buffer",
            Self::Cache => "cache",
            Self::Other => "other",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Texture => 0,
            Self::Buffer => 1,
            Self::Cache => 2,
            Self::Other => 3,
        }
    }
}

impl fmt::Display for MemoryClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// Severity of one memory-pressure signal.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MemoryPressureLevel {
    /// Managed bytes crossed the soft budget and should be reduced soon.
    Elevated,
    /// A hard-limit risk or host signal requires immediate reclamation.
    Critical,
}

/// Source of one memory-pressure signal.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryPressureCause {
    /// A reservation crossed the configured soft budget.
    BudgetThreshold,
    /// The platform host supplied an operating-system or driver pressure signal.
    External,
    /// A reservation remained above the hard budget after cooperation.
    AllocationDenied,
}

/// Immutable pressure information delivered to subscribers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryPressure {
    sequence: u64,
    level: MemoryPressureLevel,
    cause: MemoryPressureCause,
    resident_bytes: u64,
    pending_bytes: u64,
    bytes_to_release: u64,
    budget: MemoryBudget,
}

impl MemoryPressure {
    /// Returns the process-local monotonic event sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Returns the pressure severity.
    #[must_use]
    pub const fn level(self) -> MemoryPressureLevel {
        self.level
    }

    /// Returns why this pressure signal was emitted.
    #[must_use]
    pub const fn cause(self) -> MemoryPressureCause {
        self.cause
    }

    /// Returns currently retained managed payload bytes at notification time.
    #[must_use]
    pub const fn resident_bytes(self) -> u64 {
        self.resident_bytes
    }

    /// Returns bytes reserved by an allocation attempt at notification time.
    #[must_use]
    pub const fn pending_bytes(self) -> u64 {
        self.pending_bytes
    }

    /// Returns the minimum requested cooperative release.
    #[must_use]
    pub const fn bytes_to_release(self) -> u64 {
        self.bytes_to_release
    }

    /// Returns the budget that produced this event.
    #[must_use]
    pub const fn budget(self) -> MemoryBudget {
        self.budget
    }
}

/// A nonblocking coalescing pressure subscription.
///
/// One pending event is retained per subscriber. Newer pressure replaces an
/// unread event so critical severity and the latest release target are never
/// hidden behind stale notification state.
#[derive(Debug)]
pub struct MemoryPressureSubscription {
    pending: Arc<Mutex<Option<MemoryPressure>>>,
}

impl MemoryPressureSubscription {
    /// Returns the pending pressure edge, if any.
    #[must_use]
    pub fn try_recv(&self) -> Option<MemoryPressure> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
}

/// One synchronous request presented to a cooperative eviction participant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryEvictionRequest {
    bytes_to_release: u64,
    level: MemoryPressureLevel,
    cause: MemoryPressureCause,
}

impl MemoryEvictionRequest {
    /// Returns the remaining requested release when this participant is called.
    #[must_use]
    pub const fn bytes_to_release(self) -> u64 {
        self.bytes_to_release
    }

    /// Returns the pressure severity driving eviction.
    #[must_use]
    pub const fn level(self) -> MemoryPressureLevel {
        self.level
    }

    /// Returns the pressure cause driving eviction.
    #[must_use]
    pub const fn cause(self) -> MemoryPressureCause {
        self.cause
    }
}

/// A participant that can release reconstructable reservations on demand.
///
/// Participants are called in caller-provided order and must never evict
/// checked-out, externally retained, or otherwise in-use resources. An
/// eviction callback must only release memory and must not reserve from the
/// same pool while the serialized reservation gate is held.
pub trait MemoryEvictor: Send + Sync {
    /// Attempts to release bytes and reports the exact managed total released.
    fn evict(&self, request: MemoryEvictionRequest) -> Result<u64>;
}

/// Exact counters from one shared memory pool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryPoolStats {
    budget: MemoryBudget,
    resident_bytes: u64,
    resident_by_class: [u64; MEMORY_CLASS_COUNT],
    pending_bytes: u64,
    peak_resident_bytes: u64,
    active_reservations: u64,
    pressure_events: u64,
    denied_reservations: u64,
    eviction_calls: u64,
    evicted_bytes: u64,
}

impl MemoryPoolStats {
    /// Returns the configured limits.
    #[must_use]
    pub const fn budget(self) -> MemoryBudget {
        self.budget
    }

    /// Returns all currently retained managed payload bytes.
    #[must_use]
    pub const fn resident_bytes(self) -> u64 {
        self.resident_bytes
    }

    /// Returns retained bytes for one accounting class.
    #[must_use]
    pub const fn resident_bytes_for(self, class: MemoryClass) -> u64 {
        self.resident_by_class[class.index()]
    }

    /// Returns bytes held by the serialized reservation attempt.
    #[must_use]
    pub const fn pending_bytes(self) -> u64 {
        self.pending_bytes
    }

    /// Returns the highest resident byte count observed after a reservation.
    #[must_use]
    pub const fn peak_resident_bytes(self) -> u64 {
        self.peak_resident_bytes
    }

    /// Returns the number of live accounting reservations.
    #[must_use]
    pub const fn active_reservations(self) -> u64 {
        self.active_reservations
    }

    /// Returns the number of pressure edges produced by this pool.
    #[must_use]
    pub const fn pressure_events(self) -> u64 {
        self.pressure_events
    }

    /// Returns the number of reservation attempts refused at the hard limit.
    #[must_use]
    pub const fn denied_reservations(self) -> u64 {
        self.denied_reservations
    }

    /// Returns the number of cooperative participant calls.
    #[must_use]
    pub const fn eviction_calls(self) -> u64 {
        self.eviction_calls
    }

    /// Returns bytes participants reported releasing during cooperative calls.
    #[must_use]
    pub const fn evicted_bytes(self) -> u64 {
        self.evicted_bytes
    }
}

#[derive(Debug)]
struct MemoryPoolState {
    resident_bytes: u64,
    resident_by_class: [u64; MEMORY_CLASS_COUNT],
    pending_bytes: u64,
    peak_resident_bytes: u64,
    active_reservations: u64,
    pressure_events: u64,
    denied_reservations: u64,
    eviction_calls: u64,
    evicted_bytes: u64,
    next_pressure_sequence: u64,
    subscribers: Vec<Weak<Mutex<Option<MemoryPressure>>>>,
}

impl Default for MemoryPoolState {
    fn default() -> Self {
        Self {
            resident_bytes: 0,
            resident_by_class: [0; MEMORY_CLASS_COUNT],
            pending_bytes: 0,
            peak_resident_bytes: 0,
            active_reservations: 0,
            pressure_events: 0,
            denied_reservations: 0,
            eviction_calls: 0,
            evicted_bytes: 0,
            next_pressure_sequence: 1,
            subscribers: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct MemoryPoolInner {
    budget: MemoryBudget,
    reservation_gate: Mutex<()>,
    state: Mutex<MemoryPoolState>,
}

impl MemoryPoolInner {
    fn lock_state(&self, operation: &'static str) -> Result<MutexGuard<'_, MemoryPoolState>> {
        self.state.lock().map_err(|_| {
            internal(
                operation,
                "GPU memory pool state is unavailable after a panic",
            )
        })
    }

    fn lock_gate(&self, operation: &'static str) -> Result<MutexGuard<'_, ()>> {
        self.reservation_gate.lock().map_err(|_| {
            internal(
                operation,
                "GPU memory reservation gate is unavailable after a panic",
            )
        })
    }
}

/// One shareable pool for a finite managed GPU payload budget.
#[derive(Clone, Debug)]
pub struct GpuMemoryPool {
    inner: Arc<MemoryPoolInner>,
}

impl GpuMemoryPool {
    /// Creates an empty pool with explicit limits.
    #[must_use]
    pub fn new(budget: MemoryBudget) -> Self {
        Self {
            inner: Arc::new(MemoryPoolInner {
                budget,
                reservation_gate: Mutex::new(()),
                state: Mutex::new(MemoryPoolState::default()),
            }),
        }
    }

    /// Creates an accounting pool without a practical configured limit.
    #[must_use]
    pub fn unbounded() -> Self {
        Self::new(MemoryBudget::unbounded())
    }

    /// Registers one coalescing pressure listener.
    #[must_use]
    pub fn subscribe(&self) -> MemoryPressureSubscription {
        let pending = Arc::new(Mutex::new(None));
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .subscribers
            .push(Arc::downgrade(&pending));
        MemoryPressureSubscription { pending }
    }

    /// Reserves managed payload bytes, reclaiming in caller-provided order first.
    ///
    /// Crossing the soft limit emits pressure and requests enough eviction to
    /// return to it. A reservation may remain above the soft limit when active
    /// resources cannot be reclaimed, but it can never cross the hard limit.
    pub fn reserve(
        &self,
        bytes: u64,
        class: MemoryClass,
        evictors: &[&dyn MemoryEvictor],
    ) -> Result<MemoryReservation> {
        if bytes == 0 {
            return Err(invalid(
                "reserve_memory",
                "GPU memory reservations must contain at least one byte",
            ));
        }
        let _gate = self.inner.lock_gate("reserve_memory")?;
        let planned = {
            let mut state = self.inner.lock_state("reserve_memory")?;
            let pending_bytes = state.pending_bytes.checked_add(bytes).ok_or_else(|| {
                exhausted(
                    "reserve_memory",
                    "pending GPU memory reservations exceed the supported range",
                )
            })?;
            let planned = state
                .resident_bytes
                .checked_add(pending_bytes)
                .ok_or_else(|| {
                    exhausted(
                        "reserve_memory",
                        "managed GPU memory accounting exceeds the supported range",
                    )
                })?;
            state.pending_bytes = pending_bytes;
            planned
        };

        let bytes_to_release = planned.saturating_sub(self.inner.budget.soft_limit_bytes);
        if bytes_to_release > 0 {
            let level = if planned > self.inner.budget.hard_limit_bytes {
                MemoryPressureLevel::Critical
            } else {
                MemoryPressureLevel::Elevated
            };
            if let Err(error) = self.publish_and_evict(
                level,
                MemoryPressureCause::BudgetThreshold,
                bytes_to_release,
                evictors,
            ) {
                self.cancel_pending(bytes);
                return Err(error);
            }
        }

        let mut state = self.inner.lock_state("commit_memory_reservation")?;
        let final_planned = state
            .resident_bytes
            .checked_add(state.pending_bytes)
            .ok_or_else(|| {
                exhausted(
                    "commit_memory_reservation",
                    "managed GPU memory accounting exceeds the supported range",
                )
            })?;
        if final_planned > self.inner.budget.hard_limit_bytes {
            state.pending_bytes = state
                .pending_bytes
                .checked_sub(bytes)
                .expect("the serialized reservation owns its pending bytes");
            state.denied_reservations = state.denied_reservations.saturating_add(1);
            drop(state);
            self.publish(
                MemoryPressureLevel::Critical,
                MemoryPressureCause::AllocationDenied,
                final_planned.saturating_sub(self.inner.budget.hard_limit_bytes),
            )?;
            return Err(self.budget_error(bytes, class)?);
        }

        state.pending_bytes = state
            .pending_bytes
            .checked_sub(bytes)
            .expect("the serialized reservation owns its pending bytes");
        state.resident_bytes = state.resident_bytes.checked_add(bytes).ok_or_else(|| {
            exhausted(
                "commit_memory_reservation",
                "managed GPU memory accounting exceeds the supported range",
            )
        })?;
        let class_bytes = &mut state.resident_by_class[class.index()];
        *class_bytes = class_bytes.checked_add(bytes).ok_or_else(|| {
            exhausted(
                "commit_memory_reservation",
                "GPU memory class accounting exceeds the supported range",
            )
        })?;
        state.peak_resident_bytes = state.peak_resident_bytes.max(state.resident_bytes);
        state.active_reservations = state.active_reservations.saturating_add(1);
        drop(state);

        Ok(MemoryReservation {
            pool: self.clone(),
            bytes,
            class,
        })
    }

    /// Applies an explicit host pressure target through the same cooperation path.
    ///
    /// Platform integrations translate native operating-system or driver
    /// signals into a severity and maximum resident target without exposing a
    /// backend-specific API to consumers.
    pub fn apply_external_pressure(
        &self,
        level: MemoryPressureLevel,
        target_resident_bytes: u64,
        evictors: &[&dyn MemoryEvictor],
    ) -> Result<MemoryEvictionOutcome> {
        if target_resident_bytes > self.inner.budget.hard_limit_bytes {
            return Err(invalid(
                "apply_external_pressure",
                "external pressure target must not exceed the hard GPU memory budget",
            ));
        }
        let _gate = self.inner.lock_gate("apply_external_pressure")?;
        let before = self
            .inner
            .lock_state("apply_external_pressure")?
            .resident_bytes;
        let requested = before.saturating_sub(target_resident_bytes);
        let released =
            self.publish_and_evict(level, MemoryPressureCause::External, requested, evictors)?;
        let after = self
            .inner
            .lock_state("apply_external_pressure")?
            .resident_bytes;
        Ok(MemoryEvictionOutcome {
            requested_bytes: requested,
            released_bytes: released,
            resident_bytes: after,
        })
    }

    /// Returns a consistent snapshot of current accounting and diagnostics.
    pub fn stats(&self) -> Result<MemoryPoolStats> {
        let state = self.inner.lock_state("read_memory_pool_stats")?;
        Ok(MemoryPoolStats {
            budget: self.inner.budget,
            resident_bytes: state.resident_bytes,
            resident_by_class: state.resident_by_class,
            pending_bytes: state.pending_bytes,
            peak_resident_bytes: state.peak_resident_bytes,
            active_reservations: state.active_reservations,
            pressure_events: state.pressure_events,
            denied_reservations: state.denied_reservations,
            eviction_calls: state.eviction_calls,
            evicted_bytes: state.evicted_bytes,
        })
    }

    fn publish_and_evict(
        &self,
        level: MemoryPressureLevel,
        cause: MemoryPressureCause,
        bytes_to_release: u64,
        evictors: &[&dyn MemoryEvictor],
    ) -> Result<u64> {
        self.publish(level, cause, bytes_to_release)?;
        let starting_resident = self.inner.lock_state("cooperate_eviction")?.resident_bytes;
        let mut reported_total = 0_u64;
        for evictor in evictors {
            let current = self.inner.lock_state("cooperate_eviction")?.resident_bytes;
            let released = starting_resident.saturating_sub(current);
            let remaining = bytes_to_release.saturating_sub(released);
            if remaining == 0 {
                break;
            }
            {
                let mut state = self.inner.lock_state("cooperate_eviction")?;
                state.eviction_calls = state.eviction_calls.saturating_add(1);
            }
            let request = MemoryEvictionRequest {
                bytes_to_release: remaining,
                level,
                cause,
            };
            match evictor.evict(request) {
                Ok(reported) => {
                    reported_total = reported_total.saturating_add(reported);
                }
                Err(error) => {
                    self.record_cooperative_eviction(reported_total)?;
                    return Err(error.with_context(
                        ErrorContext::new(COMPONENT, "cooperate_eviction")
                            .with_field("requested_bytes", remaining.to_string()),
                    ));
                }
            }
        }
        self.record_cooperative_eviction(reported_total)?;
        Ok(reported_total)
    }

    fn record_cooperative_eviction(&self, reported_bytes: u64) -> Result<()> {
        let mut state = self.inner.lock_state("record_cooperative_eviction")?;
        state.evicted_bytes = state.evicted_bytes.saturating_add(reported_bytes);
        Ok(())
    }

    fn publish(
        &self,
        level: MemoryPressureLevel,
        cause: MemoryPressureCause,
        bytes_to_release: u64,
    ) -> Result<()> {
        let mut state = self.inner.lock_state("publish_memory_pressure")?;
        let event = MemoryPressure {
            sequence: state.next_pressure_sequence,
            level,
            cause,
            resident_bytes: state.resident_bytes,
            pending_bytes: state.pending_bytes,
            bytes_to_release,
            budget: self.inner.budget,
        };
        state.next_pressure_sequence = state.next_pressure_sequence.saturating_add(1);
        state.pressure_events = state.pressure_events.saturating_add(1);
        state.subscribers.retain(|subscriber| {
            let Some(pending) = subscriber.upgrade() else {
                return false;
            };
            *pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(event);
            true
        });
        Ok(())
    }

    fn cancel_pending(&self, bytes: u64) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.pending_bytes = state
            .pending_bytes
            .checked_sub(bytes)
            .expect("the serialized reservation owns its pending bytes");
    }

    fn budget_error(&self, requested_bytes: u64, class: MemoryClass) -> Result<Error> {
        let state = self.inner.lock_state("describe_memory_budget_failure")?;
        Ok(Error::new(
            ErrorCategory::ResourceExhausted,
            Recoverability::Retryable,
            "GPU memory budget cannot satisfy the allocation after cooperative eviction",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "reserve_memory")
                .with_field("class", class.code())
                .with_field("requested_bytes", requested_bytes.to_string())
                .with_field("resident_bytes", state.resident_bytes.to_string())
                .with_field(
                    "soft_limit_bytes",
                    self.inner.budget.soft_limit_bytes.to_string(),
                )
                .with_field(
                    "hard_limit_bytes",
                    self.inner.budget.hard_limit_bytes.to_string(),
                ),
        ))
    }

    fn release(&self, bytes: u64, class: MemoryClass) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.resident_bytes = state
            .resident_bytes
            .checked_sub(bytes)
            .expect("a live reservation is included in resident bytes");
        let class_bytes = &mut state.resident_by_class[class.index()];
        *class_bytes = class_bytes
            .checked_sub(bytes)
            .expect("a live reservation is included in its memory class");
        state.active_reservations = state
            .active_reservations
            .checked_sub(1)
            .expect("a live reservation is included in the active count");
    }
}

impl Default for GpuMemoryPool {
    fn default() -> Self {
        Self::unbounded()
    }
}

/// One non-cloneable accounting owner for managed GPU payload bytes.
pub struct MemoryReservation {
    pool: GpuMemoryPool,
    bytes: u64,
    class: MemoryClass,
}

impl MemoryReservation {
    /// Returns the exact managed payload bytes held by this reservation.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Returns the stable accounting class.
    #[must_use]
    pub const fn class(&self) -> MemoryClass {
        self.class
    }
}

impl fmt::Debug for MemoryReservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MemoryReservation")
            .field("bytes", &self.bytes)
            .field("class", &self.class)
            .finish_non_exhaustive()
    }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
        self.pool.release(self.bytes, self.class);
    }
}

/// Result of one explicit external pressure pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryEvictionOutcome {
    requested_bytes: u64,
    released_bytes: u64,
    resident_bytes: u64,
}

impl MemoryEvictionOutcome {
    /// Returns the requested release at the start of cooperation.
    #[must_use]
    pub const fn requested_bytes(self) -> u64 {
        self.requested_bytes
    }

    /// Returns managed bytes actually released by participants.
    #[must_use]
    pub const fn released_bytes(self) -> u64 {
        self.released_bytes
    }

    /// Returns managed bytes still resident after cooperation.
    #[must_use]
    pub const fn resident_bytes(self) -> u64 {
        self.resident_bytes
    }
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn exhausted(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Terminal,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn internal(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
