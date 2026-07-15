use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use superi_cache::disk::{DiskCacheCodec, DiskCacheConfig, DiskCacheValueSchema, FrameDiskCache};
use superi_cache::eviction::{CacheBudgetLimit, CacheBudgetManager, CacheCost, CacheMemoryBudgets};
use superi_cache::frame::FrameMemoryCache;
use superi_cache::key::{ParameterStateFingerprint, RenderSettingsFingerprint};
use superi_cache::render::{
    BackgroundRenderQueue, RenderCache, RenderCacheContext, RenderCachePlacement,
};
use superi_concurrency::jobs::{JobControl, JobOutcome, JobTerminalState, WorkerPoolConfig};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::ids::{DeviceId, JobId, ProjectId};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_gpu::pool::{GpuMemoryPool, MemoryBudget, MemoryClass, MemoryEvictor};
use superi_graph::dag::GraphEndpoint;
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::{
    EvaluateNode, EvaluationCacheEntryKind, EvaluationContext, EvaluationRequest,
};
use superi_graph::headless::{GraphEvaluationSnapshot, NodeCompiler};
use superi_graph::ids::{GraphId, NodeId, PortId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, GraphMutation, GraphSnapshot, GraphTransaction, InstancePort,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, PortCardinality, PortName, PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_image::metadata::{ColorPipelineMetadata, ImageColorTags};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(1);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(test: &str) -> Self {
        let nonce = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "superi-render-cache-{test}-{}-{nonce}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct BlockingGate {
    state: Mutex<BlockingState>,
    changed: Condvar,
}

#[derive(Default)]
struct BlockingState {
    started: bool,
    released: bool,
}

impl BlockingGate {
    fn enter(&self) {
        let mut state = self.state.lock().unwrap();
        state.started = true;
        self.changed.notify_all();
        while !state.released {
            state = self.changed.wait(state).unwrap();
        }
    }

    fn wait_until_started(&self) {
        let state = self.state.lock().unwrap();
        let (state, timeout) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(5), |state| !state.started)
            .unwrap();
        assert!(
            state.started,
            "background render did not reach the test node"
        );
        assert!(!timeout.timed_out(), "background render start timed out");
    }

    fn release(&self) {
        let mut state = self.state.lock().unwrap();
        state.released = true;
        self.changed.notify_all();
    }
}

#[derive(Clone)]
struct RuntimeNode {
    value: i64,
    policy: CachePolicy,
    determinism: Determinism,
    calls: Arc<AtomicUsize>,
    gate: Option<Arc<BlockingGate>>,
}

impl EvaluateNode<i64> for RuntimeNode {
    fn evaluate(&self, _context: &EvaluationContext<'_, i64>) -> Result<i64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.enter();
        }
        Ok(self.value)
    }
}

impl IntrospectNode for RuntimeNode {
    fn introspection(&self) -> NodeIntrospection {
        NodeIntrospection::new(
            schema_id(),
            NodeBehavior::new(
                TimeBehavior::CurrentFrame,
                RoiBehavior::InputBounds,
                ColorRequirements::NotApplicable,
                self.determinism,
                self.policy,
            ),
            NodeStateFingerprint::from_canonical_bytes(self.value.to_be_bytes()),
        )
    }
}

struct RuntimeCompiler(RuntimeNode);

impl NodeCompiler<(), RuntimeNode> for RuntimeCompiler {
    fn compile_node(
        &mut self,
        _snapshot: &GraphSnapshot<()>,
        _node_id: NodeId,
        _node: &EditableNode<()>,
    ) -> Result<RuntimeNode> {
        Ok(self.0.clone())
    }
}

struct I64Codec {
    schema: DiskCacheValueSchema,
}

impl I64Codec {
    fn new() -> Self {
        Self {
            schema: DiskCacheValueSchema::new("superi.test.render-cache-i64", 1).unwrap(),
        }
    }
}

impl DiskCacheCodec<i64> for I64Codec {
    fn schema(&self) -> &DiskCacheValueSchema {
        &self.schema
    }

    fn encode(&self, _kind: EvaluationCacheEntryKind, value: &i64) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }

    fn decode(&self, _kind: EvaluationCacheEntryKind, bytes: &[u8]) -> Result<i64> {
        let bytes: [u8; 8] = bytes.try_into().map_err(|_| {
            Error::new(
                ErrorCategory::CorruptData,
                Recoverability::Degraded,
                "render-cache test payload has the wrong length",
            )
        })?;
        Ok(i64::from_be_bytes(bytes))
    }
}

fn schema_id() -> NodeSchemaId {
    NodeSchemaId::new(
        NodeTypeId::new("superi.test.render-cache").unwrap(),
        SemanticVersion::from_str("1.0.0").unwrap(),
    )
}

fn schema() -> Arc<NodeSchema> {
    Arc::new(
        NodeSchema::new(
            schema_id(),
            [],
            [PortSchema::new(
                PortName::new("value").unwrap(),
                ValueTypeId::new("superi.value.i64").unwrap(),
                PortCardinality::Single,
            )],
            [],
            NodeBehavior::new(
                TimeBehavior::CurrentFrame,
                RoiBehavior::InputBounds,
                ColorRequirements::NotApplicable,
                Determinism::Deterministic,
                CachePolicy::PerRegion,
            ),
            CapabilitySet::new([]),
        )
        .unwrap(),
    )
}

fn evaluation_snapshot(
    graph_id: u128,
    value: i64,
    calls: Arc<AtomicUsize>,
    gate: Option<Arc<BlockingGate>>,
    determinism: Determinism,
) -> Arc<GraphEvaluationSnapshot<(), RuntimeNode>> {
    let node = EditableNode::new(
        schema(),
        [],
        [InstancePort::new(
            PortId::from_raw(101),
            PortName::new("value").unwrap(),
        )],
        [],
    )
    .unwrap();
    let mut graph = EditableGraph::new(GraphId::from_raw(graph_id));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id: NodeId::from_raw(1),
                node,
                position: 0,
            }],
        ))
        .unwrap();
    let runtime = RuntimeNode {
        value,
        policy: CachePolicy::PerRegion,
        determinism,
        calls,
        gate,
    };
    Arc::new(GraphEvaluationSnapshot::compile(graph.snapshot(), RuntimeCompiler(runtime)).unwrap())
}

fn request(frame: i64) -> EvaluationRequest {
    EvaluationRequest::new(
        GraphEndpoint::new(NodeId::from_raw(1), PortId::from_raw(101)),
        RationalTime::new(frame, Timebase::integer(24).unwrap()),
        PixelBounds::new(0, 0, 1920, 1080).unwrap(),
    )
}

fn budgets() -> CacheMemoryBudgets {
    let limit = CacheBudgetLimit::new(1_024, 128).unwrap();
    CacheMemoryBudgets::new(limit, limit, limit).unwrap()
}

fn value_cost(_kind: EvaluationCacheEntryKind, _value: &i64) -> CacheCost {
    CacheCost::new(8, 1).unwrap()
}

fn memory_cache() -> Arc<FrameMemoryCache<i64>> {
    Arc::new(FrameMemoryCache::new(
        CacheBudgetManager::new(budgets()),
        value_cost,
    ))
}

fn disk_cache(root: &TempRoot) -> Arc<FrameDiskCache<i64>> {
    Arc::new(
        FrameDiskCache::new(
            DiskCacheConfig::new(root.path(), 1_024).unwrap(),
            I64Codec::new(),
        )
        .unwrap(),
    )
}

fn context(settings: &[u8]) -> RenderCacheContext {
    RenderCacheContext::new(
        ProjectId::from_raw(9),
        RenderCachePlacement::Host,
        Vec::new(),
        ParameterStateFingerprint::from_canonical_bytes(b"render-cache-parameters-v1"),
        Arc::new(ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::UNSPECIFIED)).unwrap()),
        RenderSettingsFingerprint::from_canonical_bytes(settings),
    )
}

#[test]
fn layered_render_cache_promotes_persistent_hits_and_separates_quality_identity() {
    let root = TempRoot::new("layered");
    let calls = Arc::new(AtomicUsize::new(0));
    let snapshot = evaluation_snapshot(
        101,
        41,
        Arc::clone(&calls),
        None,
        Determinism::Deterministic,
    );
    let memory = memory_cache();
    let disk = disk_cache(&root);
    let cache = RenderCache::new(
        context(b"quality=full;proxy=original"),
        Arc::clone(&memory),
        Some(Arc::clone(&disk)),
    );

    let first = cache.evaluate(&snapshot, request(0)).unwrap();
    assert_eq!(*first.value(), 41);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(memory.final_frame_len(), 1);

    memory.clear();
    let persistent = cache.evaluate(&snapshot, request(0)).unwrap();
    assert_eq!(*persistent.value(), 41);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(memory.final_frame_len(), 1);
    assert!(disk.take_diagnostics().is_empty());

    let other_memory = memory_cache();
    let other_quality = RenderCache::new(
        context(b"quality=half;proxy=quarter"),
        Arc::clone(&other_memory),
        Some(Arc::clone(&disk)),
    );
    assert_ne!(
        cache.frame_key(&snapshot, request(0)).unwrap(),
        other_quality.frame_key(&snapshot, request(0)).unwrap()
    );
    assert_eq!(
        *other_quality
            .evaluate(&snapshot, request(0))
            .unwrap()
            .value(),
        41
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(other_memory.final_frame_len(), 1);
}

#[test]
fn owned_device_placement_charges_and_releases_gpu_cache_memory() {
    let calls = Arc::new(AtomicUsize::new(0));
    let snapshot = evaluation_snapshot(
        105,
        43,
        Arc::clone(&calls),
        None,
        Determinism::Deterministic,
    );
    let gpu_memory = GpuMemoryPool::new(MemoryBudget::new(64, 64).unwrap());
    let evictors: Arc<[Arc<dyn MemoryEvictor>]> = Arc::from(Vec::new());
    let memory = memory_cache();
    let cache = RenderCache::new(
        RenderCacheContext::new(
            ProjectId::from_raw(9),
            RenderCachePlacement::Device {
                device_id: DeviceId::from_raw(7),
                gpu_memory: gpu_memory.clone(),
                evictors,
            },
            Vec::new(),
            ParameterStateFingerprint::from_canonical_bytes(b"render-cache-parameters-v1"),
            Arc::new(
                ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::UNSPECIFIED)).unwrap(),
            ),
            RenderSettingsFingerprint::from_canonical_bytes(
                b"quality=full;proxy=original;device=7",
            ),
        ),
        Arc::clone(&memory),
        None,
    );

    assert_eq!(*cache.evaluate(&snapshot, request(0)).unwrap().value(), 43);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        gpu_memory
            .stats()
            .unwrap()
            .resident_bytes_for(MemoryClass::Cache),
        8
    );

    memory.clear();
    assert_eq!(
        gpu_memory
            .stats()
            .unwrap()
            .resident_bytes_for(MemoryClass::Cache),
        0
    );
}

#[test]
fn bounded_background_render_populates_the_real_cache_for_foreground_reuse() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BackgroundRenderQueue<(), RuntimeNode, i64>>();

    let root = TempRoot::new("background");
    let calls = Arc::new(AtomicUsize::new(0));
    let snapshot = evaluation_snapshot(
        102,
        53,
        Arc::clone(&calls),
        None,
        Determinism::Deterministic,
    );
    let memory = memory_cache();
    let disk = disk_cache(&root);
    let cache = Arc::new(RenderCache::new(
        context(b"quality=full;proxy=original"),
        Arc::clone(&memory),
        Some(Arc::clone(&disk)),
    ));
    let queue = BackgroundRenderQueue::new(
        Arc::clone(&snapshot),
        Arc::clone(&cache),
        WorkerPoolConfig::new(2, 8).unwrap(),
    )
    .unwrap();

    let first = queue
        .submit(JobId::from_raw(1), request(0), JobControl::new())
        .unwrap();
    let second = queue
        .submit(JobId::from_raw(2), request(1), JobControl::new())
        .unwrap();
    assert_eq!(first.job_id(), JobId::from_raw(1));
    let first_key = first.frame_key();
    let first_execution = first.wait().unwrap();
    let second_execution = second.wait().unwrap();
    assert_eq!(
        first_execution.completion().status(),
        JobTerminalState::Succeeded
    );
    assert_eq!(
        second_execution.completion().status(),
        JobTerminalState::Succeeded
    );
    let first_render = first_execution
        .into_completion()
        .into_outcome()
        .into_success()
        .unwrap();
    assert_eq!(first_render.frame_key(), first_key);
    assert_eq!(first_render.graph_id(), snapshot.graph_id());
    assert_eq!(first_render.graph_revision(), snapshot.graph_revision());
    assert_eq!(*first_render.evaluation().value(), 53);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(queue.active_frame_keys().is_empty());
    assert!(queue.snapshot().maximum_active_workers() <= 2);

    memory.clear();
    let foreground = cache.evaluate(&snapshot, request(0)).unwrap();
    assert_eq!(*foreground.value(), 53);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(memory.final_frame_len(), 1);
    assert!(disk.take_diagnostics().is_empty());
    queue.shutdown().unwrap();
}

#[test]
fn duplicate_work_is_rejected_and_cancelled_work_does_not_publish_new_values() {
    let calls = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(BlockingGate::default());
    let snapshot = evaluation_snapshot(
        103,
        67,
        Arc::clone(&calls),
        Some(Arc::clone(&gate)),
        Determinism::Deterministic,
    );
    let memory = memory_cache();
    let cache = Arc::new(RenderCache::new(
        context(b"quality=full;proxy=original"),
        Arc::clone(&memory),
        None,
    ));
    let queue = BackgroundRenderQueue::new(
        Arc::clone(&snapshot),
        Arc::clone(&cache),
        WorkerPoolConfig::new(1, 4).unwrap(),
    )
    .unwrap();

    let task = queue
        .submit(JobId::from_raw(10), request(0), JobControl::new())
        .unwrap();
    gate.wait_until_started();
    assert_eq!(queue.active_frame_keys(), vec![task.frame_key()]);

    let duplicate = queue.submit(JobId::from_raw(11), request(0), JobControl::new());
    let error = match duplicate {
        Ok(_) => panic!("duplicate background render was accepted"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Conflict);

    let queued = queue
        .submit(JobId::from_raw(12), request(1), JobControl::new())
        .unwrap();
    assert_eq!(queue.active_frame_keys().len(), 2);
    assert!(queue.cancel_frame(queued.frame_key()));

    assert!(queue.cancel_frame(task.frame_key()));
    task.cancel();
    gate.release();
    let completion = task.wait().unwrap().into_completion();
    assert_eq!(completion.status(), JobTerminalState::Cancelled);
    assert!(matches!(completion.into_outcome(), JobOutcome::Cancelled));
    let queued_completion = queued.wait().unwrap().into_completion();
    assert_eq!(queued_completion.status(), JobTerminalState::Cancelled);
    assert!(matches!(
        queued_completion.into_outcome(),
        JobOutcome::Cancelled
    ));
    assert!(queue.active_frame_keys().is_empty());
    assert_eq!(memory.final_frame_len(), 0);
    assert_eq!(memory.intermediate_node_len(), 0);

    assert_eq!(*cache.evaluate(&snapshot, request(0)).unwrap().value(), 67);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(memory.final_frame_len(), 1);
    queue.shutdown().unwrap();
}

#[test]
fn noncacheable_background_work_fails_before_queueing() {
    let calls = Arc::new(AtomicUsize::new(0));
    let snapshot = evaluation_snapshot(
        104,
        71,
        Arc::clone(&calls),
        None,
        Determinism::NonDeterministic,
    );
    let cache = Arc::new(RenderCache::new(
        context(b"quality=full;proxy=original"),
        memory_cache(),
        None,
    ));
    let queue = BackgroundRenderQueue::new(
        Arc::clone(&snapshot),
        cache,
        WorkerPoolConfig::new(1, 2).unwrap(),
    )
    .unwrap();

    let submission = queue.submit(JobId::from_raw(20), request(0), JobControl::new());
    let error = match submission {
        Ok(_) => panic!("noncacheable background render was accepted"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert!(queue.active_frame_keys().is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    queue.shutdown().unwrap();
}
