//! Final-frame and intermediate-node memory caches with exact color identity.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use superi_core::error::{ErrorCategory, Result};
use superi_core::ids::{DeviceId, ProjectId};
use superi_gpu::pool::{GpuMemoryPool, MemoryEvictor};
use superi_graph::eval::{EvaluationCacheEntryKind, EvaluationCacheIdentity, EvaluationValueCache};
use superi_graph::ids::{GraphId, NodeId};
use superi_graph::invalidation::GraphEditInvalidation;
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
/// Both budgeted tiers and their revision fences share one lock so edit invalidation is atomic.
/// Pressure reclaims intermediate outputs before final frames, and successful hits and insertions
/// promote exact entries through strict LRU order inside each tier.
/// Every retained entry owns one exact cache-budget reservation, including a shared GPU-pool
/// reservation for device values. Values remain caller-defined and are stored behind `Arc`; lookup
/// clones the caller's handle only after releasing the cache lock. Removed, replaced, and
/// invalidated values release their reservations only after unlock. The cache rejects late
/// insertion from invalidated snapshots and does not persist data.
pub struct FrameMemoryCache<V> {
    state: Mutex<FrameMemoryCacheState<V>>,
    budgets: CacheBudgetManager,
    value_cost: fn(EvaluationCacheEntryKind, &V) -> CacheCost,
}

struct FrameMemoryCacheState<V> {
    final_frames: CacheEntries<V>,
    intermediate_nodes: CacheEntries<V>,
    minimum_valid_revisions: BTreeMap<(GraphId, NodeId), u64>,
}

impl<V> FrameMemoryCacheState<V> {
    fn entries_mut(&mut self, kind: EvaluationCacheEntryKind) -> &mut CacheEntries<V> {
        match kind {
            EvaluationCacheEntryKind::FinalFrame => &mut self.final_frames,
            EvaluationCacheEntryKind::IntermediateNode => &mut self.intermediate_nodes,
        }
    }

    fn tier_mut(&mut self, tier: MemoryCacheTier) -> &mut CacheEntries<V> {
        match tier {
            MemoryCacheTier::FinalFrame => &mut self.final_frames,
            MemoryCacheTier::IntermediateNode => &mut self.intermediate_nodes,
        }
    }

    fn permits(&self, identity: EvaluationCacheIdentity) -> bool {
        let lineage = (
            identity.graph_id(),
            identity.evaluation_key().output().node_id(),
        );
        let Some(minimum_revision) = self.minimum_valid_revisions.get(&lineage) else {
            return true;
        };
        identity
            .graph_revision()
            .is_some_and(|revision| revision >= *minimum_revision)
    }
}

/// Deterministic evidence of values removed by one graph edit invalidation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheInvalidationReport {
    graph_id: GraphId,
    from_revision: u64,
    to_revision: u64,
    removed_final_frame_keys: Vec<FrameCacheKey>,
    removed_intermediate_node_keys: Vec<FrameCacheKey>,
}

impl CacheInvalidationReport {
    /// Returns the graph whose retained lineage was inspected.
    #[must_use]
    pub const fn graph_id(&self) -> GraphId {
        self.graph_id
    }

    /// Returns the oldest compared editable revision.
    #[must_use]
    pub const fn from_revision(&self) -> u64 {
        self.from_revision
    }

    /// Returns the newly published editable revision.
    #[must_use]
    pub const fn to_revision(&self) -> u64 {
        self.to_revision
    }

    /// Returns removed final-frame keys in deterministic order.
    #[must_use]
    pub fn removed_final_frame_keys(&self) -> &[FrameCacheKey] {
        &self.removed_final_frame_keys
    }

    /// Returns removed intermediate-node keys in deterministic order.
    #[must_use]
    pub fn removed_intermediate_node_keys(&self) -> &[FrameCacheKey] {
        &self.removed_intermediate_node_keys
    }

    /// Returns whether the edit found no retained affected value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.removed_final_frame_keys.is_empty() && self.removed_intermediate_node_keys.is_empty()
    }
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
            state: Mutex::new(FrameMemoryCacheState {
                final_frames: LruMap::default(),
                intermediate_nodes: LruMap::default(),
                minimum_valid_revisions: BTreeMap::new(),
            }),
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
        lock_state(&self.state).final_frames.len()
    }

    /// Returns the number of retained intermediate-node values.
    #[must_use]
    pub fn intermediate_node_len(&self) -> usize {
        lock_state(&self.state).intermediate_nodes.len()
    }

    /// Returns final-frame identities in deterministic key order.
    #[must_use]
    pub fn final_frame_keys(&self) -> Vec<FrameCacheKey> {
        let mut keys = lock_state(&self.state)
            .final_frames
            .keys()
            .map(|key| key.frame_key)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        keys
    }

    /// Returns intermediate-node identities in deterministic key order.
    #[must_use]
    pub fn intermediate_node_keys(&self) -> Vec<FrameCacheKey> {
        let mut keys = lock_state(&self.state)
            .intermediate_nodes
            .keys()
            .map(|key| key.frame_key)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        keys
    }

    /// Releases every final and intermediate value and its matching reservation.
    pub fn clear(&self) {
        let mut state = lock_state(&self.state);
        let final_frames = std::mem::take(&mut state.final_frames);
        let intermediate_nodes = std::mem::take(&mut state.intermediate_nodes);
        drop(state);
        drop((final_frames, intermediate_nodes));
    }

    /// Atomically removes affected older values from both tiers and advances revision fences.
    ///
    /// Unaffected values remain reusable across graph revisions because their complete semantic
    /// keys have not changed. A fence is installed even when no retained value exists, preventing
    /// an older or unversioned evaluation that finishes later from repopulating affected lineage.
    pub fn apply_invalidation(
        &self,
        invalidation: &GraphEditInvalidation,
    ) -> CacheInvalidationReport {
        let mut state = lock_state(&self.state);
        for node_id in invalidation.affected_nodes() {
            let fence = state
                .minimum_valid_revisions
                .entry((invalidation.graph_id(), *node_id))
                .or_insert(invalidation.to_revision());
            *fence = (*fence).max(invalidation.to_revision());
        }

        let (removed_final_frame_keys, removed_final_frames) =
            extract_invalidated_entries(&mut state.final_frames, invalidation);
        let (removed_intermediate_node_keys, removed_intermediate_nodes) =
            extract_invalidated_entries(&mut state.intermediate_nodes, invalidation);
        drop(state);
        drop((removed_final_frames, removed_intermediate_nodes));
        CacheInvalidationReport {
            graph_id: invalidation.graph_id(),
            from_revision: invalidation.from_revision(),
            to_revision: invalidation.to_revision(),
            removed_final_frame_keys,
            removed_intermediate_node_keys,
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
        let mut state = lock_state(&self.state);
        let removed = state.tier_mut(tier).evict_lru(maximum);
        drop(state);
        let keys = removed.iter().map(|(key, _)| key.frame_key).collect();
        drop(removed);
        keys
    }

    fn evict_one_for_pressure(&self, pressure: CachePressureScope) -> bool {
        for tier in [
            MemoryCacheTier::IntermediateNode,
            MemoryCacheTier::FinalFrame,
        ] {
            let mut state = lock_state(&self.state);
            let removed = state
                .tier_mut(tier)
                .evict_lru_where(1, |key| key.matches_pressure(pressure))
                .pop();
            drop(state);
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
    identity: EvaluationCacheIdentity,
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
        let mut state = lock_state(&self.cache.state);
        if !state.permits(identity) {
            return None;
        }
        let key = self.context.entry_key(identity);
        let retained = state
            .entries_mut(kind)
            .get(&key)
            .map(|entry| Arc::clone(&entry.value));
        drop(state);
        retained.map(|value| (*value).clone())
    }

    fn insert(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity, value: V) {
        let key = self.context.entry_key(identity);
        let cost = (self.cache.value_cost)(kind, &value);

        let mut state = lock_state(&self.cache.state);
        if !state.permits(identity) {
            return;
        }
        let replaced = state.entries_mut(kind).remove(&key);
        drop(state);
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
            identity,
            value: Arc::new(value),
            _reservation: reservation,
        };

        let mut state = lock_state(&self.cache.state);
        if !state.permits(identity) {
            drop(state);
            drop(retained);
            return;
        }
        let replaced = state.entries_mut(kind).insert(key, retained);
        drop(state);
        drop(replaced);
    }
}

fn extract_invalidated_entries<V>(
    entries: &mut CacheEntries<V>,
    invalidation: &GraphEditInvalidation,
) -> (Vec<FrameCacheKey>, Vec<RetainedCacheValue<V>>) {
    let removed = entries.remove_where(|_key, retained| {
        let identity = retained.identity;
        let affected = identity.graph_id() == invalidation.graph_id()
            && invalidation.affects(identity.evaluation_key().output().node_id());
        let superseded = identity
            .graph_revision()
            .map_or(true, |revision| revision < invalidation.to_revision());
        affected && superseded
    });
    let mut frame_keys = removed
        .iter()
        .map(|(key, _retained)| key.frame_key)
        .collect::<Vec<_>>();
    frame_keys.sort_unstable();
    let retained = removed
        .into_iter()
        .map(|(_key, retained)| retained)
        .collect();
    (frame_keys, retained)
}

fn lock_state<V>(
    state: &Mutex<FrameMemoryCacheState<V>>,
) -> MutexGuard<'_, FrameMemoryCacheState<V>> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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
