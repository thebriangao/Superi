//! Final-frame and intermediate-node memory caches with exact color identity.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use superi_core::error::{ErrorCategory, Result};
use superi_core::ids::{DeviceId, ProjectId};
use superi_gpu::pool::{GpuMemoryPool, MemoryEvictor};
use superi_graph::eval::{EvaluationCacheEntryKind, EvaluationCacheIdentity, EvaluationValueCache};
use superi_graph::node::GraphColorMetadata;
use superi_image::metadata::ColorPipelineMetadata;

use crate::eviction::{
    CacheBudgetManager, CacheBudgetReservation, CacheBudgetStats, CacheCost, CachePressureScope,
    LruMap, MemoryCacheTier,
};
use crate::key::{
    FrameCacheKey, FrameCacheKeyInputs, MediaCacheIdentity, ParameterStateFingerprint,
    RenderSettingsFingerprint,
};

/// Thread-safe retained values for final frames and intermediate graph outputs.
///
/// The two tiers are independent so pressure can reclaim intermediate outputs before final frames.
/// Successful hits and insertions promote exact entries through strict LRU order inside each tier.
/// Every retained entry owns one exact cache-budget reservation, including a shared GPU-pool
/// reservation for device values. Values remain caller-defined and are stored behind `Arc`; lookup
/// clones the caller's handle only after releasing the cache lock. Removed and replaced values
/// release their reservations only after unlock. This cache does not invalidate generations or
/// persist data.
pub struct FrameMemoryCache<V> {
    final_frames: Mutex<CacheEntries<V>>,
    intermediate_nodes: Mutex<CacheEntries<V>>,
    budgets: CacheBudgetManager,
    value_cost: fn(EvaluationCacheEntryKind, &V) -> CacheCost,
}

impl<V> FrameMemoryCache<V> {
    /// Creates empty budgeted final-frame and intermediate-node tiers.
    ///
    /// `value_cost` must report the exact managed payload bytes and frame-result count retained by
    /// the supplied value. The function executes without any cache or budget lock held.
    #[must_use]
    pub fn new(
        budgets: CacheBudgetManager,
        value_cost: fn(EvaluationCacheEntryKind, &V) -> CacheCost,
    ) -> Self {
        Self {
            final_frames: Mutex::new(LruMap::default()),
            intermediate_nodes: Mutex::new(LruMap::default()),
            budgets,
            value_cost,
        }
    }

    /// Returns the shared hierarchical budget owner used by this cache.
    #[must_use]
    pub const fn budget_manager(&self) -> &CacheBudgetManager {
        &self.budgets
    }

    /// Returns one consistent budget and usage snapshot.
    pub fn budget_stats(&self) -> Result<CacheBudgetStats> {
        self.budgets.stats()
    }

    /// Returns the number of retained final-frame values.
    #[must_use]
    pub fn final_frame_len(&self) -> usize {
        lock_entries(&self.final_frames).len()
    }

    /// Returns the number of retained intermediate-node values.
    #[must_use]
    pub fn intermediate_node_len(&self) -> usize {
        lock_entries(&self.intermediate_nodes).len()
    }

    /// Returns final-frame identities in deterministic key order.
    #[must_use]
    pub fn final_frame_keys(&self) -> Vec<FrameCacheKey> {
        let mut keys = lock_entries(&self.final_frames)
            .keys()
            .map(|key| key.frame_key)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        keys
    }

    /// Returns intermediate-node identities in deterministic key order.
    #[must_use]
    pub fn intermediate_node_keys(&self) -> Vec<FrameCacheKey> {
        let mut keys = lock_entries(&self.intermediate_nodes)
            .keys()
            .map(|key| key.frame_key)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        keys
    }

    /// Releases every final and intermediate value and its matching reservation.
    pub fn clear(&self) {
        let final_frames = take_entries(&self.final_frames);
        let intermediate_nodes = take_entries(&self.intermediate_nodes);
        drop((final_frames, intermediate_nodes));
    }

    fn entries(&self, kind: EvaluationCacheEntryKind) -> &Mutex<CacheEntries<V>> {
        match kind {
            EvaluationCacheEntryKind::FinalFrame => &self.final_frames,
            EvaluationCacheEntryKind::IntermediateNode => &self.intermediate_nodes,
        }
    }

    fn tier(&self, tier: MemoryCacheTier) -> &Mutex<CacheEntries<V>> {
        match tier {
            MemoryCacheTier::FinalFrame => &self.final_frames,
            MemoryCacheTier::IntermediateNode => &self.intermediate_nodes,
        }
    }

    /// Removes up to `maximum` entries in strict least-recently-used order.
    ///
    /// Successful evaluator hits and insertions are accesses. Returned keys use exact victim order,
    /// which lets management callers observe reclamation without owning a second recency model.
    /// The selected tier is independent, and retained values are destroyed only after its lock is
    /// released. Eviction is ordinary replacement behavior: a later request remains a cache miss and
    /// recomputes through the unchanged graph evaluator.
    #[must_use]
    pub fn evict_lru(&self, tier: MemoryCacheTier, maximum: usize) -> Vec<FrameCacheKey> {
        let removed = {
            let mut entries = lock_entries(self.tier(tier));
            entries.evict_lru(maximum)
        };
        let keys = removed.iter().map(|(key, _)| key.frame_key).collect();
        drop(removed);
        keys
    }

    fn evict_one_for_pressure(&self, pressure: CachePressureScope) -> bool {
        for tier in [
            MemoryCacheTier::IntermediateNode,
            MemoryCacheTier::FinalFrame,
        ] {
            let removed = {
                let mut entries = lock_entries(self.tier(tier));
                entries
                    .evict_lru_where(1, |key| key.matches_pressure(pressure))
                    .pop()
            };
            if let Some((_key, value)) = removed {
                drop(value);
                return true;
            }
        }
        false
    }

    /// Binds caller-owned result identity to graph-driven lookup and insertion.
    #[must_use]
    pub const fn scope<'cache, 'context>(
        &'cache self,
        context: FrameMemoryCacheContext<'context>,
    ) -> FrameMemoryCacheScope<'cache, 'context, V> {
        FrameMemoryCacheScope {
            cache: self,
            context,
        }
    }
}

struct RetainedCacheValue<V> {
    value: Arc<V>,
    _reservation: CacheBudgetReservation,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CacheEntryKey {
    project_id: ProjectId,
    placement: CachePlacementId,
    frame_key: FrameCacheKey,
}

impl CacheEntryKey {
    fn matches_pressure(self, pressure: CachePressureScope) -> bool {
        match pressure {
            CachePressureScope::Total => true,
            CachePressureScope::Project(project_id) => self.project_id == project_id,
            CachePressureScope::Device(device_id) => {
                matches!(self.placement, CachePlacementId::Device(id) if id == device_id)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum CachePlacementId {
    Host,
    Device(DeviceId),
}

type CacheEntries<V> = LruMap<CacheEntryKey, RetainedCacheValue<V>>;

/// Host or processing-device placement charged for retained values in one scope.
#[derive(Clone, Copy)]
pub enum CacheMemoryPlacement<'a> {
    /// Managed payload bytes remain host-resident.
    Host,
    /// Managed payload bytes remain resident on one processing device.
    Device {
        /// Stable processing-device identity for the per-device cache limit.
        device_id: DeviceId,
        /// Existing shared GPU memory owner for the same exact managed bytes.
        gpu_memory: &'a GpuMemoryPool,
        /// Ordered pressure cooperators available to the shared GPU memory owner.
        evictors: &'a [&'a dyn MemoryEvictor],
    },
}

impl fmt::Debug for CacheMemoryPlacement<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host => formatter.write_str("Host"),
            Self::Device { device_id, .. } => formatter
                .debug_struct("Device")
                .field("device_id", device_id)
                .finish_non_exhaustive(),
        }
    }
}

impl CacheMemoryPlacement<'_> {
    const fn id(self) -> CachePlacementId {
        match self {
            Self::Host => CachePlacementId::Host,
            Self::Device { device_id, .. } => CachePlacementId::Device(device_id),
        }
    }

    const fn device_id(self) -> Option<DeviceId> {
        match self {
            Self::Host => None,
            Self::Device { device_id, .. } => Some(device_id),
        }
    }

    fn reserve(
        self,
        budgets: &CacheBudgetManager,
        project_id: ProjectId,
        cost: CacheCost,
    ) -> Result<CacheBudgetReservation> {
        match self {
            Self::Host => budgets.reserve_host(project_id, cost),
            Self::Device {
                device_id,
                gpu_memory,
                evictors,
            } => budgets.reserve_device(project_id, device_id, cost, gpu_memory, evictors),
        }
    }
}

/// Caller-owned result identity shared by one cached evaluation scope.
///
/// Media, evaluated parameters, color meaning, and render settings come from their authoritative
/// orchestration owners. Each graph lookup contributes its own graph lineage and physical time.
#[derive(Clone, Copy, Debug)]
pub struct FrameMemoryCacheContext<'a> {
    project_id: ProjectId,
    placement: CacheMemoryPlacement<'a>,
    media: &'a [MediaCacheIdentity],
    parameters: ParameterStateFingerprint,
    color: &'a ColorPipelineMetadata,
    render_settings: RenderSettingsFingerprint,
}

impl<'a> FrameMemoryCacheContext<'a> {
    /// Creates the non-graph identity inputs for one cached evaluation scope.
    #[must_use]
    pub const fn new(
        project_id: ProjectId,
        placement: CacheMemoryPlacement<'a>,
        media: &'a [MediaCacheIdentity],
        parameters: ParameterStateFingerprint,
        color: &'a ColorPipelineMetadata,
        render_settings: RenderSettingsFingerprint,
    ) -> Self {
        Self {
            project_id,
            placement,
            media,
            parameters,
            color,
            render_settings,
        }
    }

    fn key(self, identity: EvaluationCacheIdentity) -> FrameCacheKey {
        FrameCacheKey::derive(FrameCacheKeyInputs::new(
            self.media,
            identity.graph_key(),
            self.parameters,
            self.color,
            identity.evaluation_key().frame(),
            self.render_settings,
        ))
    }

    fn entry_key(self, identity: EvaluationCacheIdentity) -> CacheEntryKey {
        CacheEntryKey {
            project_id: self.project_id,
            placement: self.placement.id(),
            frame_key: self.key(identity),
        }
    }
}

/// Graph evaluator adapter for one complete result-identity scope.
pub struct FrameMemoryCacheScope<'cache, 'context, V> {
    cache: &'cache FrameMemoryCache<V>,
    context: FrameMemoryCacheContext<'context>,
}

impl<V: Clone> EvaluationValueCache<V> for FrameMemoryCacheScope<'_, '_, V> {
    fn get(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity) -> Option<V> {
        let key = self.context.entry_key(identity);
        let retained = lock_entries(self.cache.entries(kind))
            .get(&key)
            .map(|entry| Arc::clone(&entry.value));
        retained.map(|value| (*value).clone())
    }

    fn insert(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity, value: V) {
        let key = self.context.entry_key(identity);
        let cost = (self.cache.value_cost)(kind, &value);
        let entries = self.cache.entries(kind);

        let replaced = {
            let mut entries = lock_entries(entries);
            entries.remove(&key)
        };
        drop(replaced);

        let reservation = loop {
            match self
                .context
                .placement
                .reserve(&self.cache.budgets, self.context.project_id, cost)
            {
                Ok(reservation) => break reservation,
                Err(error) => {
                    let pressure = match self.cache.budgets.pressure_scope(
                        self.context.project_id,
                        self.context.placement.device_id(),
                        cost,
                    ) {
                        Ok(Some(pressure)) => pressure,
                        Ok(None) if error.category() == ErrorCategory::ResourceExhausted => {
                            match self.context.placement.device_id() {
                                Some(device_id) => CachePressureScope::Device(device_id),
                                None => return,
                            }
                        }
                        Ok(None) => return,
                        Err(_) => return,
                    };
                    if !self.cache.evict_one_for_pressure(pressure) {
                        return;
                    }
                }
            }
        };
        let retained = RetainedCacheValue {
            value: Arc::new(value),
            _reservation: reservation,
        };
        let replaced = {
            let mut entries = lock_entries(self.cache.entries(kind));
            entries.insert(key, retained)
        };
        drop(replaced);
    }
}

fn lock_entries<V>(entries: &Mutex<CacheEntries<V>>) -> MutexGuard<'_, CacheEntries<V>> {
    entries
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn take_entries<V>(entries: &Mutex<CacheEntries<V>>) -> CacheEntries<V> {
    std::mem::take(&mut *lock_entries(entries))
}

/// Complete color identity stored beside one cached graph result.
///
/// Equality and hashing include preserved source payloads and every ordered
/// transform stage, preventing reuse across appearance-changing differences.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CachedFrameColorMetadata {
    pipeline: ColorPipelineMetadata,
}

impl CachedFrameColorMetadata {
    /// Captures the exact graph output color identity.
    #[must_use]
    pub fn from_graph(graph: &GraphColorMetadata) -> Self {
        Self {
            pipeline: graph.pipeline().clone(),
        }
    }

    /// Returns whether a requested result has identical complete color identity.
    #[must_use]
    pub fn matches(&self, requested: &ColorPipelineMetadata) -> bool {
        self.pipeline == *requested
    }

    /// Returns the complete cached source identity and transform history.
    #[must_use]
    pub const fn pipeline(&self) -> &ColorPipelineMetadata {
        &self.pipeline
    }
}
