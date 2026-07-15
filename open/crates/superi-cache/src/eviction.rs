//! Cache memory admission, accounting, and eviction-policy foundation.
//!
//! This module enforces exact managed payload limits before cache values become
//! resident. It owns global, per-project, and per-device byte and frame
//! accounting. Device-resident values additionally hold a reservation from the
//! shared GPU memory pool. Eviction order and victim selection remain the next
//! policy layer; callers can release one reservation and retry admission without
//! bypassing any limit.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{DeviceId, ProjectId};
use superi_gpu::pool::{
    GpuMemoryPool, MemoryClass, MemoryEvictor, MemoryReservation as GpuMemoryReservation,
};

const COMPONENT: &str = "superi-cache.memory-budget";

/// Hard byte and frame limits for one cache accounting scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheBudgetLimit {
    max_bytes: u64,
    max_frames: u64,
}

impl CacheBudgetLimit {
    /// Creates one finite nonzero cache limit.
    pub fn new(max_bytes: u64, max_frames: u64) -> Result<Self> {
        if max_bytes == 0 {
            return Err(invalid(
                "create_budget_limit",
                "cache byte limit must be greater than zero",
            ));
        }
        if max_frames == 0 {
            return Err(invalid(
                "create_budget_limit",
                "cache frame limit must be greater than zero",
            ));
        }
        Ok(Self {
            max_bytes,
            max_frames,
        })
    }

    /// Returns the exact maximum managed payload bytes.
    #[must_use]
    pub const fn max_bytes(self) -> u64 {
        self.max_bytes
    }

    /// Returns the exact maximum resident frame results.
    #[must_use]
    pub const fn max_frames(self) -> u64 {
        self.max_frames
    }
}

/// Complete hierarchical limits for one cache manager.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheMemoryBudgets {
    total: CacheBudgetLimit,
    per_project: CacheBudgetLimit,
    per_device: CacheBudgetLimit,
}

impl CacheMemoryBudgets {
    /// Creates global, per-project, and per-device limits.
    ///
    /// Child scopes cannot exceed the global limit. This keeps configuration
    /// truthful even though the global scope remains the final shared bound.
    pub fn new(
        total: CacheBudgetLimit,
        per_project: CacheBudgetLimit,
        per_device: CacheBudgetLimit,
    ) -> Result<Self> {
        validate_child_limit("project", per_project, total)?;
        validate_child_limit("device", per_device, total)?;
        Ok(Self {
            total,
            per_project,
            per_device,
        })
    }

    /// Returns the process-local cache limit shared by every project and device.
    #[must_use]
    pub const fn total(self) -> CacheBudgetLimit {
        self.total
    }

    /// Returns the independent limit applied to each active project.
    #[must_use]
    pub const fn per_project(self) -> CacheBudgetLimit {
        self.per_project
    }

    /// Returns the independent limit applied to each active processing device.
    #[must_use]
    pub const fn per_device(self) -> CacheBudgetLimit {
        self.per_device
    }
}

/// Exact managed payload cost of one cache reservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheCost {
    bytes: u64,
    frames: u64,
}

impl CacheCost {
    /// Creates one nonempty cost.
    ///
    /// A cached final frame or intermediate output contributes at least one
    /// frame result even when several results refer to the same physical time.
    pub fn new(bytes: u64, frames: u64) -> Result<Self> {
        if bytes == 0 {
            return Err(invalid(
                "create_cache_cost",
                "cache reservations must contain at least one payload byte",
            ));
        }
        if frames == 0 {
            return Err(invalid(
                "create_cache_cost",
                "cache reservations must contain at least one frame result",
            ));
        }
        Ok(Self { bytes, frames })
    }

    /// Returns exact managed payload bytes.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    /// Returns exact frame results charged to the frame limit.
    #[must_use]
    pub const fn frames(self) -> u64 {
        self.frames
    }
}

/// Exact current or peak cache usage for one scope.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheBudgetUsage {
    bytes: u64,
    frames: u64,
}

impl CacheBudgetUsage {
    /// Returns resident managed payload bytes.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    /// Returns resident frame results.
    #[must_use]
    pub const fn frames(self) -> u64 {
        self.frames
    }

    fn add(&mut self, cost: CacheCost) {
        self.bytes = self
            .bytes
            .checked_add(cost.bytes)
            .expect("preflighted cache byte usage remains representable");
        self.frames = self
            .frames
            .checked_add(cost.frames)
            .expect("preflighted cache frame usage remains representable");
    }

    fn subtract(&mut self, cost: CacheCost) {
        self.bytes = self
            .bytes
            .checked_sub(cost.bytes)
            .expect("a live reservation is included in cache byte usage");
        self.frames = self
            .frames
            .checked_sub(cost.frames)
            .expect("a live reservation is included in cache frame usage");
    }

    const fn is_empty(self) -> bool {
        self.bytes == 0 && self.frames == 0
    }
}

/// Immutable accounting and diagnostic snapshot from one cache manager.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheBudgetStats {
    budgets: CacheMemoryBudgets,
    total_usage: CacheBudgetUsage,
    peak_total_usage: CacheBudgetUsage,
    projects: BTreeMap<ProjectId, CacheBudgetUsage>,
    devices: BTreeMap<DeviceId, CacheBudgetUsage>,
    active_reservations: u64,
    denied_reservations: u64,
}

impl CacheBudgetStats {
    /// Returns the exact configured limits.
    #[must_use]
    pub const fn budgets(&self) -> CacheMemoryBudgets {
        self.budgets
    }

    /// Returns current usage across every project and device.
    #[must_use]
    pub const fn total_usage(&self) -> CacheBudgetUsage {
        self.total_usage
    }

    /// Returns the independent highest byte and frame totals observed.
    #[must_use]
    pub const fn peak_total_usage(&self) -> CacheBudgetUsage {
        self.peak_total_usage
    }

    /// Returns current usage for one project, or zero for an inactive project.
    #[must_use]
    pub fn project_usage(&self, project_id: ProjectId) -> CacheBudgetUsage {
        self.projects.get(&project_id).copied().unwrap_or_default()
    }

    /// Returns current usage for one device, or zero for an inactive device.
    #[must_use]
    pub fn device_usage(&self, device_id: DeviceId) -> CacheBudgetUsage {
        self.devices.get(&device_id).copied().unwrap_or_default()
    }

    /// Returns the number of projects that currently own reservations.
    #[must_use]
    pub fn active_projects(&self) -> usize {
        self.projects.len()
    }

    /// Returns the number of devices that currently own reservations.
    #[must_use]
    pub fn active_devices(&self) -> usize {
        self.devices.len()
    }

    /// Returns the number of live reservation owners.
    #[must_use]
    pub const fn active_reservations(&self) -> u64 {
        self.active_reservations
    }

    /// Returns the number of locally refused reservation attempts.
    #[must_use]
    pub const fn denied_reservations(&self) -> u64 {
        self.denied_reservations
    }
}

#[derive(Debug, Default)]
struct CacheBudgetState {
    total_usage: CacheBudgetUsage,
    peak_total_usage: CacheBudgetUsage,
    projects: BTreeMap<ProjectId, CacheBudgetUsage>,
    devices: BTreeMap<DeviceId, CacheBudgetUsage>,
    active_reservations: u64,
    denied_reservations: u64,
}

#[derive(Debug)]
struct CacheBudgetInner {
    budgets: CacheMemoryBudgets,
    state: Mutex<CacheBudgetState>,
}

impl CacheBudgetInner {
    fn lock_state(&self, operation: &'static str) -> Result<MutexGuard<'_, CacheBudgetState>> {
        self.state.lock().map_err(|_| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "cache memory budget state is unavailable after a panic",
            )
            .with_context(ErrorContext::new(COMPONENT, operation))
        })
    }
}

/// Shareable owner of exact hierarchical cache memory accounting.
#[derive(Clone, Debug)]
pub struct CacheBudgetManager {
    inner: Arc<CacheBudgetInner>,
}

impl CacheBudgetManager {
    /// Creates an empty manager with validated explicit limits.
    #[must_use]
    pub fn new(budgets: CacheMemoryBudgets) -> Self {
        Self {
            inner: Arc::new(CacheBudgetInner {
                budgets,
                state: Mutex::new(CacheBudgetState::default()),
            }),
        }
    }

    /// Returns the exact configured limits.
    #[must_use]
    pub fn budgets(&self) -> CacheMemoryBudgets {
        self.inner.budgets
    }

    /// Reserves host-resident cache capacity for one project.
    pub fn reserve_host(
        &self,
        project_id: ProjectId,
        cost: CacheCost,
    ) -> Result<CacheBudgetReservation> {
        self.reserve_local(project_id, None, cost)
    }

    /// Reserves device-resident cache capacity for one project and device.
    ///
    /// Local cache scopes are admitted first without holding the cache lock
    /// across GPU cooperation. The returned owner then holds a shared GPU pool
    /// reservation in `MemoryClass::Cache`. A GPU refusal drops and rolls back
    /// the provisional local reservation before the error returns.
    pub fn reserve_device(
        &self,
        project_id: ProjectId,
        device_id: DeviceId,
        cost: CacheCost,
        gpu_memory: &GpuMemoryPool,
        evictors: &[&dyn MemoryEvictor],
    ) -> Result<CacheBudgetReservation> {
        let mut reservation = self.reserve_local(project_id, Some(device_id), cost)?;
        let gpu_reservation = gpu_memory
            .reserve(cost.bytes, MemoryClass::Cache, evictors)
            .map_err(|error| {
                error.with_context(
                    ErrorContext::new(COMPONENT, "reserve_device")
                        .with_field("project", project_id.to_string())
                        .with_field("device", device_id.to_string())
                        .with_field("requested_bytes", cost.bytes.to_string())
                        .with_field("requested_frames", cost.frames.to_string()),
                )
            })?;
        reservation.gpu_reservation = Some(gpu_reservation);
        Ok(reservation)
    }

    /// Returns one consistent immutable accounting snapshot.
    pub fn stats(&self) -> Result<CacheBudgetStats> {
        let state = self.inner.lock_state("read_budget_stats")?;
        Ok(CacheBudgetStats {
            budgets: self.inner.budgets,
            total_usage: state.total_usage,
            peak_total_usage: state.peak_total_usage,
            projects: state.projects.clone(),
            devices: state.devices.clone(),
            active_reservations: state.active_reservations,
            denied_reservations: state.denied_reservations,
        })
    }

    fn reserve_local(
        &self,
        project_id: ProjectId,
        device_id: Option<DeviceId>,
        cost: CacheCost,
    ) -> Result<CacheBudgetReservation> {
        let mut state = self.inner.lock_state("reserve_cache_memory")?;
        let project_usage = state.projects.get(&project_id).copied().unwrap_or_default();
        let device_usage = device_id
            .and_then(|id| state.devices.get(&id).copied())
            .unwrap_or_default();

        let failure = first_limit_failure(
            self.inner.budgets,
            state.total_usage,
            project_usage,
            device_id.map(|_| device_usage),
            cost,
        );
        if let Some(failure) = failure {
            state.denied_reservations = state.denied_reservations.saturating_add(1);
            return Err(budget_exhausted(failure, project_id, device_id, cost));
        }

        state.total_usage.add(cost);
        state.peak_total_usage.bytes = state.peak_total_usage.bytes.max(state.total_usage.bytes);
        state.peak_total_usage.frames = state.peak_total_usage.frames.max(state.total_usage.frames);
        state.projects.entry(project_id).or_default().add(cost);
        if let Some(device_id) = device_id {
            state.devices.entry(device_id).or_default().add(cost);
        }
        state.active_reservations = state
            .active_reservations
            .checked_add(1)
            .expect("frame limits bound the number of live cache reservations");
        drop(state);

        Ok(CacheBudgetReservation {
            manager: self.clone(),
            project_id,
            device_id,
            cost,
            gpu_reservation: None,
        })
    }

    fn release(&self, project_id: ProjectId, device_id: Option<DeviceId>, cost: CacheCost) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        state.total_usage.subtract(cost);
        let remove_project = {
            let usage = state
                .projects
                .get_mut(&project_id)
                .expect("a live reservation has project usage");
            usage.subtract(cost);
            usage.is_empty()
        };
        if remove_project {
            state.projects.remove(&project_id);
        }

        if let Some(device_id) = device_id {
            let remove_device = {
                let usage = state
                    .devices
                    .get_mut(&device_id)
                    .expect("a live device reservation has device usage");
                usage.subtract(cost);
                usage.is_empty()
            };
            if remove_device {
                state.devices.remove(&device_id);
            }
        }

        state.active_reservations = state
            .active_reservations
            .checked_sub(1)
            .expect("a live reservation is included in the active count");
    }
}

/// One non-cloneable owner of admitted cache capacity.
pub struct CacheBudgetReservation {
    manager: CacheBudgetManager,
    project_id: ProjectId,
    device_id: Option<DeviceId>,
    cost: CacheCost,
    gpu_reservation: Option<GpuMemoryReservation>,
}

impl CacheBudgetReservation {
    /// Returns the project charged by this reservation.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Returns the charged processing device for device-resident data.
    #[must_use]
    pub const fn device_id(&self) -> Option<DeviceId> {
        self.device_id
    }

    /// Returns the exact admitted payload cost.
    #[must_use]
    pub const fn cost(&self) -> CacheCost {
        self.cost
    }
}

impl fmt::Debug for CacheBudgetReservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CacheBudgetReservation")
            .field("project_id", &self.project_id)
            .field("device_id", &self.device_id)
            .field("cost", &self.cost)
            .field("gpu_accounted", &self.gpu_reservation.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for CacheBudgetReservation {
    fn drop(&mut self) {
        drop(self.gpu_reservation.take());
        self.manager
            .release(self.project_id, self.device_id, self.cost);
    }
}

#[derive(Clone, Copy, Debug)]
struct LimitFailure {
    kind: LimitKind,
    current: u64,
    maximum: u64,
}

#[derive(Clone, Copy, Debug)]
enum LimitKind {
    TotalBytes,
    TotalFrames,
    ProjectBytes,
    ProjectFrames,
    DeviceBytes,
    DeviceFrames,
}

impl LimitKind {
    const fn code(self) -> &'static str {
        match self {
            Self::TotalBytes => "total_bytes",
            Self::TotalFrames => "total_frames",
            Self::ProjectBytes => "project_bytes",
            Self::ProjectFrames => "project_frames",
            Self::DeviceBytes => "device_bytes",
            Self::DeviceFrames => "device_frames",
        }
    }

    const fn requested(self, cost: CacheCost) -> u64 {
        match self {
            Self::TotalBytes | Self::ProjectBytes | Self::DeviceBytes => cost.bytes,
            Self::TotalFrames | Self::ProjectFrames | Self::DeviceFrames => cost.frames,
        }
    }
}

fn first_limit_failure(
    budgets: CacheMemoryBudgets,
    total: CacheBudgetUsage,
    project: CacheBudgetUsage,
    device: Option<CacheBudgetUsage>,
    cost: CacheCost,
) -> Option<LimitFailure> {
    for failure in [
        limit_failure(
            LimitKind::TotalBytes,
            total.bytes,
            cost.bytes,
            budgets.total.max_bytes,
        ),
        limit_failure(
            LimitKind::TotalFrames,
            total.frames,
            cost.frames,
            budgets.total.max_frames,
        ),
        limit_failure(
            LimitKind::ProjectBytes,
            project.bytes,
            cost.bytes,
            budgets.per_project.max_bytes,
        ),
        limit_failure(
            LimitKind::ProjectFrames,
            project.frames,
            cost.frames,
            budgets.per_project.max_frames,
        ),
        device.and_then(|usage| {
            limit_failure(
                LimitKind::DeviceBytes,
                usage.bytes,
                cost.bytes,
                budgets.per_device.max_bytes,
            )
        }),
        device.and_then(|usage| {
            limit_failure(
                LimitKind::DeviceFrames,
                usage.frames,
                cost.frames,
                budgets.per_device.max_frames,
            )
        }),
    ] {
        if failure.is_some() {
            return failure;
        }
    }
    None
}

fn limit_failure(
    kind: LimitKind,
    current: u64,
    requested: u64,
    maximum: u64,
) -> Option<LimitFailure> {
    (requested > maximum.saturating_sub(current)).then_some(LimitFailure {
        kind,
        current,
        maximum,
    })
}

fn budget_exhausted(
    failure: LimitFailure,
    project_id: ProjectId,
    device_id: Option<DeviceId>,
    cost: CacheCost,
) -> Error {
    let mut context = ErrorContext::new(COMPONENT, "reserve_cache_memory")
        .with_field("limit", failure.kind.code())
        .with_field("current", failure.current.to_string())
        .with_field("maximum", failure.maximum.to_string())
        .with_field("requested", failure.kind.requested(cost).to_string())
        .with_field("requested_bytes", cost.bytes.to_string())
        .with_field("requested_frames", cost.frames.to_string())
        .with_field("project", project_id.to_string());
    if let Some(device_id) = device_id {
        context.insert_field("device", device_id.to_string());
    }
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Retryable,
        "cache memory budget cannot admit the requested result",
    )
    .with_context(context)
}

fn validate_child_limit(
    scope: &'static str,
    child: CacheBudgetLimit,
    total: CacheBudgetLimit,
) -> Result<()> {
    if child.max_bytes > total.max_bytes {
        return Err(invalid(
            "create_memory_budgets",
            format!("per-{scope} cache byte limit must not exceed the total byte limit"),
        ));
    }
    if child.max_frames > total.max_frames {
        return Err(invalid(
            "create_memory_budgets",
            format!("per-{scope} cache frame limit must not exceed the total frame limit"),
        ));
    }
    Ok(())
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
