//! Exact layered render caching and bounded background population.
//!
//! This module composes the graph evaluator's deterministic retained-value seam with the existing
//! memory and optional persistent stores. It owns no project or graph mutation, media selection,
//! proxy generation, or rendering algorithm. A background render is one bounded cache-population
//! job over an immutable graph evaluation snapshot and complete caller-owned render identity.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use superi_concurrency::jobs::{
    BoundedWorkerPool, JobCancellationToken, JobControl, JobExecution, JobExecutionContext,
    JobKind, JobPriority, JobProgress, JobTaskHandle, ScheduledJob, WorkerPoolConfig,
    WorkerPoolShutdownReport, WorkerPoolSnapshot,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{DeviceId, JobId, ProjectId};
use superi_gpu::pool::{GpuMemoryPool, MemoryEvictor};
use superi_graph::diagnostics::{CacheKeyStatus, IntrospectNode};
use superi_graph::eval::{
    EvaluateNode, EvaluationCacheEntryKind, EvaluationCacheIdentity, EvaluationRequest,
    EvaluationResult, EvaluationValueCache,
};
use superi_graph::headless::GraphEvaluationSnapshot;
use superi_graph::ids::GraphId;
use superi_image::metadata::ColorPipelineMetadata;

use crate::disk::{FrameDiskCache, FrameDiskCacheContext, FrameDiskCacheScope};
use crate::frame::{
    CacheMemoryPlacement, FrameMemoryCache, FrameMemoryCacheContext, FrameMemoryCacheScope,
};
use crate::key::{
    FrameCacheKey, FrameCacheKeyInputs, MediaCacheIdentity, ParameterStateFingerprint,
    RenderSettingsFingerprint,
};

const COMPONENT: &str = "superi-cache.render";

/// Owned host or processing-device placement for one render-cache context.
#[derive(Clone)]
pub enum RenderCachePlacement {
    /// Cached handles and their managed payload remain host-resident.
    Host,
    /// Cached handles retain managed payload on one processing device.
    Device {
        /// Stable processing-device identity used by cache limits.
        device_id: DeviceId,
        /// Shared GPU managed-payload budget for the same exact cache bytes.
        gpu_memory: GpuMemoryPool,
        /// Ordered pressure cooperators retained for background evaluations.
        evictors: Arc<[Arc<dyn MemoryEvictor>]>,
    },
}

impl fmt::Debug for RenderCachePlacement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host => formatter.write_str("Host"),
            Self::Device {
                device_id,
                evictors,
                ..
            } => formatter
                .debug_struct("Device")
                .field("device_id", device_id)
                .field("evictor_count", &evictors.len())
                .finish_non_exhaustive(),
        }
    }
}

/// Complete owned non-graph identity for foreground and background render reuse.
#[derive(Clone, Debug)]
pub struct RenderCacheContext {
    project_id: ProjectId,
    placement: RenderCachePlacement,
    media: Arc<[MediaCacheIdentity]>,
    parameters: ParameterStateFingerprint,
    color: Arc<ColorPipelineMetadata>,
    render_settings: RenderSettingsFingerprint,
}

impl RenderCacheContext {
    /// Retains every authoritative outer identity needed by the existing cache tiers.
    pub fn new<M>(
        project_id: ProjectId,
        placement: RenderCachePlacement,
        media: M,
        parameters: ParameterStateFingerprint,
        color: Arc<ColorPipelineMetadata>,
        render_settings: RenderSettingsFingerprint,
    ) -> Self
    where
        M: Into<Arc<[MediaCacheIdentity]>>,
    {
        Self {
            project_id,
            placement,
            media: media.into(),
            parameters,
            color,
            render_settings,
        }
    }

    /// Returns the project whose replaceable cache partition owns these results.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Returns the retained memory placement policy.
    #[must_use]
    pub const fn placement(&self) -> &RenderCachePlacement {
        &self.placement
    }

    /// Returns exact source media identities in caller-provided order.
    #[must_use]
    pub fn media(&self) -> &[MediaCacheIdentity] {
        &self.media
    }

    /// Returns the evaluated parameter-state identity.
    #[must_use]
    pub const fn parameters(&self) -> ParameterStateFingerprint {
        self.parameters
    }

    /// Returns the exact color interpretation and transform history.
    #[must_use]
    pub fn color(&self) -> &ColorPipelineMetadata {
        &self.color
    }

    /// Returns the caller-owned output, quality, and proxy-policy identity.
    #[must_use]
    pub const fn render_settings(&self) -> RenderSettingsFingerprint {
        self.render_settings
    }

    fn frame_key<T, N, V>(
        &self,
        snapshot: &GraphEvaluationSnapshot<T, N>,
        request: EvaluationRequest,
    ) -> Result<FrameCacheKey>
    where
        N: EvaluateNode<V> + IntrospectNode,
    {
        let inspection = snapshot.inspect::<V>(request)?;
        let target = inspection.node(request.key()).ok_or_else(|| {
            internal(
                "inspect_render_key",
                "graph inspection omitted its requested render target",
            )
        })?;
        let graph_key = match *target.cache_status() {
            CacheKeyStatus::Available(key) => key,
            status => {
                return Err(unsupported(
                    "inspect_render_key",
                    "background rendering requires one complete deterministic cache identity",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "cache_status")
                        .with_field("graph_id", snapshot.graph_id().to_string())
                        .with_field("graph_revision", snapshot.graph_revision().to_string())
                        .with_field("status", status.code()),
                ));
            }
        };

        Ok(FrameCacheKey::derive(FrameCacheKeyInputs::new(
            &self.media,
            graph_key,
            self.parameters,
            &self.color,
            request.frame(),
            self.render_settings,
        )))
    }

    fn memory_context<'a>(
        &'a self,
        placement: CacheMemoryPlacement<'a>,
    ) -> FrameMemoryCacheContext<'a> {
        FrameMemoryCacheContext::new(
            self.project_id,
            placement,
            &self.media,
            self.parameters,
            &self.color,
            self.render_settings,
        )
    }

    fn disk_context(&self) -> FrameDiskCacheContext<'_> {
        FrameDiskCacheContext::new(
            self.project_id,
            &self.media,
            self.parameters,
            &self.color,
            self.render_settings,
        )
    }
}

/// Memory-first render reuse with optional persistent fallback and promotion.
pub struct RenderCache<V> {
    context: RenderCacheContext,
    memory: Arc<FrameMemoryCache<V>>,
    disk: Option<Arc<FrameDiskCache<V>>>,
}

impl<V> RenderCache<V> {
    /// Creates one immutable render identity over caller-owned memory and disk tiers.
    #[must_use]
    pub fn new(
        context: RenderCacheContext,
        memory: Arc<FrameMemoryCache<V>>,
        disk: Option<Arc<FrameDiskCache<V>>>,
    ) -> Self {
        Self {
            context,
            memory,
            disk,
        }
    }

    /// Returns the complete outer identity used by both tiers.
    #[must_use]
    pub const fn context(&self) -> &RenderCacheContext {
        &self.context
    }

    /// Returns the configured memory tier.
    #[must_use]
    pub const fn memory_cache(&self) -> &Arc<FrameMemoryCache<V>> {
        &self.memory
    }

    /// Returns the optional persistent tier.
    #[must_use]
    pub const fn disk_cache(&self) -> Option<&Arc<FrameDiskCache<V>>> {
        self.disk.as_ref()
    }

    /// Derives the exact final-frame identity without evaluating node values.
    pub fn frame_key<T, N>(
        &self,
        snapshot: &GraphEvaluationSnapshot<T, N>,
        request: EvaluationRequest,
    ) -> Result<FrameCacheKey>
    where
        N: EvaluateNode<V> + IntrospectNode,
    {
        self.context.frame_key(snapshot, request)
    }

    /// Evaluates through memory, then disk, then the unchanged graph path.
    ///
    /// A disk hit is promoted to memory. A fresh successful value is offered to memory and disk,
    /// while admission or persistence failure retains the freshly evaluated result unchanged.
    pub fn evaluate<T, N>(
        &self,
        snapshot: &GraphEvaluationSnapshot<T, N>,
        request: EvaluationRequest,
    ) -> Result<EvaluationResult<V>>
    where
        N: EvaluateNode<V> + IntrospectNode,
        V: Clone,
    {
        match &self.context.placement {
            RenderCachePlacement::Host => {
                self.evaluate_with_placement(snapshot, request, CacheMemoryPlacement::Host)
            }
            RenderCachePlacement::Device {
                device_id,
                gpu_memory,
                evictors,
            } => {
                let evictors = evictors
                    .iter()
                    .map(AsRef::as_ref)
                    .collect::<Vec<&dyn MemoryEvictor>>();
                self.evaluate_with_placement(
                    snapshot,
                    request,
                    CacheMemoryPlacement::Device {
                        device_id: *device_id,
                        gpu_memory,
                        evictors: &evictors,
                    },
                )
            }
        }
    }

    fn evaluate_with_placement<T, N>(
        &self,
        snapshot: &GraphEvaluationSnapshot<T, N>,
        request: EvaluationRequest,
        placement: CacheMemoryPlacement<'_>,
    ) -> Result<EvaluationResult<V>>
    where
        N: EvaluateNode<V> + IntrospectNode,
        V: Clone,
    {
        let memory = self.memory.scope(self.context.memory_context(placement));
        let disk = self
            .disk
            .as_ref()
            .map(|cache| cache.scope(self.context.disk_context()));
        let cache = LayeredRenderCacheScope { memory, disk };
        snapshot.evaluate_with_cache(request, &cache)
    }

    fn evaluate_staged<T, N>(
        &self,
        snapshot: &GraphEvaluationSnapshot<T, N>,
        request: EvaluationRequest,
        execution: &JobExecutionContext,
    ) -> Result<EvaluationResult<V>>
    where
        N: EvaluateNode<V> + IntrospectNode,
        V: Clone,
    {
        match &self.context.placement {
            RenderCachePlacement::Host => self.evaluate_staged_with_placement(
                snapshot,
                request,
                execution,
                CacheMemoryPlacement::Host,
            ),
            RenderCachePlacement::Device {
                device_id,
                gpu_memory,
                evictors,
            } => {
                let evictors = evictors
                    .iter()
                    .map(AsRef::as_ref)
                    .collect::<Vec<&dyn MemoryEvictor>>();
                self.evaluate_staged_with_placement(
                    snapshot,
                    request,
                    execution,
                    CacheMemoryPlacement::Device {
                        device_id: *device_id,
                        gpu_memory,
                        evictors: &evictors,
                    },
                )
            }
        }
    }

    fn evaluate_staged_with_placement<T, N>(
        &self,
        snapshot: &GraphEvaluationSnapshot<T, N>,
        request: EvaluationRequest,
        execution: &JobExecutionContext,
        placement: CacheMemoryPlacement<'_>,
    ) -> Result<EvaluationResult<V>>
    where
        N: EvaluateNode<V> + IntrospectNode,
        V: Clone,
    {
        let memory = self.memory.scope(self.context.memory_context(placement));
        let disk = self
            .disk
            .as_ref()
            .map(|cache| cache.scope(self.context.disk_context()));
        let layered = LayeredRenderCacheScope { memory, disk };
        let staged = StagedRenderCacheScope::new(&layered);
        let result = snapshot.evaluate_with_cache(request, &staged)?;
        execution.check("publish_background_render")?;
        execution.progress().finish()?;
        execution.check("commit_background_render")?;
        staged.commit();
        Ok(result)
    }
}

impl<V> fmt::Debug for RenderCache<V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RenderCache")
            .field("context", &self.context)
            .field("persistent", &self.disk.is_some())
            .finish_non_exhaustive()
    }
}

struct LayeredRenderCacheScope<'cache, 'context, V> {
    memory: FrameMemoryCacheScope<'cache, 'context, V>,
    disk: Option<FrameDiskCacheScope<'cache, 'context, V>>,
}

impl<V: Clone> EvaluationValueCache<V> for LayeredRenderCacheScope<'_, '_, V> {
    fn get(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity) -> Option<V> {
        if let Some(value) = self.memory.get(kind, identity) {
            return Some(value);
        }
        let value = self.disk.as_ref()?.get(kind, identity)?;
        self.memory.insert(kind, identity, value.clone());
        Some(value)
    }

    fn insert(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity, value: V) {
        if let Some(disk) = &self.disk {
            self.memory.insert(kind, identity, value.clone());
            disk.insert(kind, identity, value);
        } else {
            self.memory.insert(kind, identity, value);
        }
    }
}

struct StagedRenderCacheScope<'scope, 'cache, 'context, V> {
    target: &'scope LayeredRenderCacheScope<'cache, 'context, V>,
    pending: Mutex<Vec<(EvaluationCacheEntryKind, EvaluationCacheIdentity, V)>>,
}

impl<'scope, 'cache, 'context, V> StagedRenderCacheScope<'scope, 'cache, 'context, V> {
    fn new(target: &'scope LayeredRenderCacheScope<'cache, 'context, V>) -> Self {
        Self {
            target,
            pending: Mutex::new(Vec::new()),
        }
    }
}

impl<V: Clone> StagedRenderCacheScope<'_, '_, '_, V> {
    fn commit(&self) {
        let pending = std::mem::take(&mut *lock_unpoisoned(&self.pending));
        for (kind, identity, value) in pending {
            self.target.insert(kind, identity, value);
        }
    }
}

impl<V: Clone> EvaluationValueCache<V> for StagedRenderCacheScope<'_, '_, '_, V> {
    fn get(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity) -> Option<V> {
        self.target.get(kind, identity)
    }

    fn insert(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity, value: V) {
        lock_unpoisoned(&self.pending).push((kind, identity, value));
    }
}

/// Successful background render evidence for one immutable graph revision and frame key.
#[derive(Debug)]
pub struct BackgroundRenderResult<V> {
    frame_key: FrameCacheKey,
    graph_id: GraphId,
    graph_revision: u64,
    evaluation: EvaluationResult<V>,
}

impl<V> BackgroundRenderResult<V> {
    /// Returns the exact outer and graph-derived frame identity populated by this job.
    #[must_use]
    pub const fn frame_key(&self) -> FrameCacheKey {
        self.frame_key
    }

    /// Returns the immutable graph identity used for evaluation.
    #[must_use]
    pub const fn graph_id(&self) -> GraphId {
        self.graph_id
    }

    /// Returns the immutable editable revision used for evaluation.
    #[must_use]
    pub const fn graph_revision(&self) -> u64 {
        self.graph_revision
    }

    /// Returns the unchanged graph evaluation result.
    #[must_use]
    pub const fn evaluation(&self) -> &EvaluationResult<V> {
        &self.evaluation
    }

    /// Consumes this evidence and returns the unchanged graph evaluation result.
    #[must_use]
    pub fn into_evaluation(self) -> EvaluationResult<V> {
        self.evaluation
    }
}

/// One submitted background render with exact cancellation and typed observation.
#[must_use = "retain the task to observe or cancel the background render"]
pub struct BackgroundRenderTask<V> {
    frame_key: FrameCacheKey,
    cancellation: JobCancellationToken,
    handle: JobTaskHandle<BackgroundRenderResult<V>>,
}

impl<V> BackgroundRenderTask<V> {
    /// Returns the stable job identity supplied at submission.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.handle.job_id()
    }

    /// Returns the exact final-frame key reserved by this in-flight task.
    #[must_use]
    pub const fn frame_key(&self) -> FrameCacheKey {
        self.frame_key
    }

    /// Requests cooperative cancellation without removing any prior complete cache value.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Observes completion without blocking the caller.
    pub fn try_completion(&self) -> Result<Option<JobExecution<BackgroundRenderResult<V>>>> {
        self.handle.try_completion()
    }

    /// Waits only when the current execution domain permits blocking.
    pub fn wait(&self) -> Result<JobExecution<BackgroundRenderResult<V>>> {
        self.handle.wait()
    }
}

impl<V> fmt::Debug for BackgroundRenderTask<V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BackgroundRenderTask")
            .field("frame_key", &self.frame_key)
            .field("job_id", &self.handle.job_id())
            .finish_non_exhaustive()
    }
}

/// Bounded single-flight population for one immutable graph snapshot and render context.
pub struct BackgroundRenderQueue<T, N, V> {
    snapshot: Arc<GraphEvaluationSnapshot<T, N>>,
    cache: Arc<RenderCache<V>>,
    pool: BoundedWorkerPool,
    active: Arc<Mutex<BTreeMap<FrameCacheKey, JobCancellationToken>>>,
}

impl<T, N, V> BackgroundRenderQueue<T, N, V> {
    /// Starts the explicitly bounded background workers for one immutable render scope.
    pub fn new(
        snapshot: Arc<GraphEvaluationSnapshot<T, N>>,
        cache: Arc<RenderCache<V>>,
        config: WorkerPoolConfig,
    ) -> Result<Self> {
        Ok(Self {
            snapshot,
            cache,
            pool: BoundedWorkerPool::new(config)?,
            active: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    /// Submits one exact cacheable frame at background priority.
    ///
    /// A second task for the same complete frame identity is rejected while the first remains
    /// active. Noncacheable graph work fails before queue admission because it cannot safely warm a
    /// deterministic render cache.
    pub fn submit(
        &self,
        job_id: JobId,
        request: EvaluationRequest,
        control: JobControl,
    ) -> Result<BackgroundRenderTask<V>>
    where
        T: Send + Sync + 'static,
        N: EvaluateNode<V> + IntrospectNode + Send + Sync + 'static,
        V: Clone + Send + Sync + 'static,
    {
        let frame_key = self.cache.frame_key(&self.snapshot, request)?;
        let progress = JobProgress::with_total(1)?;
        let cancellation = control.cancellation_token().clone();
        {
            let mut active = lock_unpoisoned(&self.active);
            if active.contains_key(&frame_key) {
                return Err(conflict(
                    "submit_background_render",
                    "the exact render frame is already being populated",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "active_frame")
                        .with_field("frame_key", frame_key.to_string()),
                ));
            }
            active.insert(frame_key, cancellation.clone());
        }

        let snapshot = Arc::clone(&self.snapshot);
        let cache = Arc::clone(&self.cache);
        let active = Arc::clone(&self.active);
        let active_guard = ActiveRenderGuard::new(frame_key, Arc::clone(&active));
        let task = move |execution: JobExecutionContext| {
            let _active = active_guard;
            let evaluation = cache.evaluate_staged(&snapshot, request, &execution)?;
            Ok(BackgroundRenderResult {
                frame_key,
                graph_id: snapshot.graph_id(),
                graph_revision: snapshot.graph_revision(),
                evaluation,
            })
        };
        let scheduled = ScheduledJob::new(job_id, JobKind::Cache, JobPriority::Background, task);
        let handle = match self.pool.submit(scheduled, control, progress) {
            Ok(handle) => handle,
            Err(error) => {
                lock_unpoisoned(&self.active).remove(&frame_key);
                return Err(error);
            }
        };

        Ok(BackgroundRenderTask {
            frame_key,
            cancellation,
            handle,
        })
    }

    /// Returns exact active frame identities in deterministic order.
    #[must_use]
    pub fn active_frame_keys(&self) -> Vec<FrameCacheKey> {
        lock_unpoisoned(&self.active).keys().copied().collect()
    }

    /// Requests cooperative cancellation for one exact active frame.
    pub fn cancel_frame(&self, frame_key: FrameCacheKey) -> bool {
        let cancellation = lock_unpoisoned(&self.active).get(&frame_key).cloned();
        cancellation.is_some_and(|token| {
            token.cancel();
            true
        })
    }

    /// Returns coherent bounded-worker state without waiting for active work.
    #[must_use]
    pub fn snapshot(&self) -> WorkerPoolSnapshot {
        self.pool.snapshot()
    }

    /// Drains queued work, joins every worker, and returns final diagnostics.
    pub fn shutdown(self) -> Result<WorkerPoolShutdownReport> {
        self.pool.shutdown()
    }
}

impl<T, N, V> fmt::Debug for BackgroundRenderQueue<T, N, V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BackgroundRenderQueue")
            .field("graph_id", &self.snapshot.graph_id())
            .field("graph_revision", &self.snapshot.graph_revision())
            .field("active_frame_keys", &self.active_frame_keys())
            .field("worker_pool", &self.pool.snapshot())
            .finish_non_exhaustive()
    }
}

struct ActiveRenderGuard {
    frame_key: FrameCacheKey,
    active: Arc<Mutex<BTreeMap<FrameCacheKey, JobCancellationToken>>>,
}

impl ActiveRenderGuard {
    fn new(
        frame_key: FrameCacheKey,
        active: Arc<Mutex<BTreeMap<FrameCacheKey, JobCancellationToken>>>,
    ) -> Self {
        Self { frame_key, active }
    }
}

impl Drop for ActiveRenderGuard {
    fn drop(&mut self) {
        lock_unpoisoned(&self.active).remove(&self.frame_key);
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
