//! Final-frame and intermediate-node memory caches with exact color identity.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use superi_graph::eval::{EvaluationCacheEntryKind, EvaluationCacheIdentity, EvaluationValueCache};
use superi_graph::node::GraphColorMetadata;
use superi_image::metadata::ColorPipelineMetadata;

use crate::key::{
    FrameCacheKey, FrameCacheKeyInputs, MediaCacheIdentity, ParameterStateFingerprint,
    RenderSettingsFingerprint,
};

/// Thread-safe retained values for final frames and intermediate graph outputs.
///
/// The two tiers are independent so later budget and eviction policy can treat them separately.
/// Values remain caller-defined and are stored behind `Arc`; lookup clones the caller's handle only
/// after releasing the cache lock. This cache does not derive keys, invalidate generations, enforce
/// budgets, evict entries, or persist data.
pub struct FrameMemoryCache<V> {
    final_frames: Mutex<BTreeMap<FrameCacheKey, Arc<V>>>,
    intermediate_nodes: Mutex<BTreeMap<FrameCacheKey, Arc<V>>>,
}

impl<V> FrameMemoryCache<V> {
    /// Creates empty final-frame and intermediate-node tiers.
    #[must_use]
    pub fn new() -> Self {
        Self {
            final_frames: Mutex::new(BTreeMap::new()),
            intermediate_nodes: Mutex::new(BTreeMap::new()),
        }
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
        lock_entries(&self.final_frames).keys().copied().collect()
    }

    /// Returns intermediate-node identities in deterministic key order.
    #[must_use]
    pub fn intermediate_node_keys(&self) -> Vec<FrameCacheKey> {
        lock_entries(&self.intermediate_nodes)
            .keys()
            .copied()
            .collect()
    }

    fn entries(&self, kind: EvaluationCacheEntryKind) -> &Mutex<BTreeMap<FrameCacheKey, Arc<V>>> {
        match kind {
            EvaluationCacheEntryKind::FinalFrame => &self.final_frames,
            EvaluationCacheEntryKind::IntermediateNode => &self.intermediate_nodes,
        }
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

impl<V> Default for FrameMemoryCache<V> {
    fn default() -> Self {
        Self::new()
    }
}

/// Caller-owned result identity shared by one cached evaluation scope.
///
/// Media, evaluated parameters, color meaning, and render settings come from their authoritative
/// orchestration owners. Each graph lookup contributes its own graph lineage and physical time.
#[derive(Clone, Copy, Debug)]
pub struct FrameMemoryCacheContext<'a> {
    media: &'a [MediaCacheIdentity],
    parameters: ParameterStateFingerprint,
    color: &'a ColorPipelineMetadata,
    render_settings: RenderSettingsFingerprint,
}

impl<'a> FrameMemoryCacheContext<'a> {
    /// Creates the non-graph identity inputs for one cached evaluation scope.
    #[must_use]
    pub const fn new(
        media: &'a [MediaCacheIdentity],
        parameters: ParameterStateFingerprint,
        color: &'a ColorPipelineMetadata,
        render_settings: RenderSettingsFingerprint,
    ) -> Self {
        Self {
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
}

/// Graph evaluator adapter for one complete result-identity scope.
pub struct FrameMemoryCacheScope<'cache, 'context, V> {
    cache: &'cache FrameMemoryCache<V>,
    context: FrameMemoryCacheContext<'context>,
}

impl<V: Clone> EvaluationValueCache<V> for FrameMemoryCacheScope<'_, '_, V> {
    fn get(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity) -> Option<V> {
        let key = self.context.key(identity);
        let retained = lock_entries(self.cache.entries(kind)).get(&key).cloned();
        retained.map(|value| (*value).clone())
    }

    fn insert(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity, value: V) {
        let key = self.context.key(identity);
        lock_entries(self.cache.entries(kind)).insert(key, Arc::new(value));
    }
}

fn lock_entries<V>(
    entries: &Mutex<BTreeMap<FrameCacheKey, Arc<V>>>,
) -> MutexGuard<'_, BTreeMap<FrameCacheKey, Arc<V>>> {
    entries
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
