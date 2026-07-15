use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use superi_cache::disk::{
    DiskCacheCodec, DiskCacheConfig, DiskCacheValueSchema, FrameDiskCache, FrameDiskCacheContext,
    PERSISTENT_CACHE_FORMAT_REVISION,
};
use superi_cache::key::{ParameterStateFingerprint, RenderSettingsFingerprint};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::ids::ProjectId;
use superi_core::settings::SemanticVersion;
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::{
    EvaluateNode, EvaluationCacheEntryKind, EvaluationContext, EvaluationRequest, LazyEvaluator,
};
use superi_graph::ids::{EdgeId, GraphId, NodeId, PortId};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, NodeTypeId,
    RoiBehavior, TimeBehavior,
};
use superi_image::metadata::{ColorPipelineMetadata, ImageColorTags};

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(1);
type MutateEntry = fn(&Path);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new(test: &str) -> Self {
        let nonce = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "superi-disk-cache-{test}-{}-{nonce}",
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

#[derive(Clone)]
struct ValueNode {
    node_type: &'static str,
    value: i64,
    policy: CachePolicy,
    calls: Arc<AtomicUsize>,
}

impl EvaluateNode<i64> for ValueNode {
    fn evaluate(&self, _context: &EvaluationContext<'_, i64>) -> Result<i64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.value)
    }
}

impl IntrospectNode for ValueNode {
    fn introspection(&self) -> NodeIntrospection {
        NodeIntrospection::new(
            NodeSchemaId::new(
                NodeTypeId::from_str(self.node_type).unwrap(),
                SemanticVersion::from_str("1.0.0").unwrap(),
            ),
            NodeBehavior::new(
                if self.policy == CachePolicy::Static {
                    TimeBehavior::Invariant
                } else {
                    TimeBehavior::CurrentFrame
                },
                RoiBehavior::InputBounds,
                ColorRequirements::NotApplicable,
                Determinism::Deterministic,
                self.policy,
            ),
            NodeStateFingerprint::from_canonical_bytes(self.value.to_be_bytes()),
        )
    }
}

struct I64Codec {
    schema: DiskCacheValueSchema,
}

impl I64Codec {
    fn new(revision: u32) -> Self {
        Self {
            schema: DiskCacheValueSchema::new("superi.test.i64", revision).unwrap(),
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
                "cached i64 payload has the wrong length",
            )
        })?;
        Ok(i64::from_be_bytes(bytes))
    }
}

struct FailingEncodeCodec {
    schema: DiskCacheValueSchema,
    category: ErrorCategory,
    recoverability: Recoverability,
}

impl FailingEncodeCodec {
    fn new(category: ErrorCategory, recoverability: Recoverability) -> Self {
        Self {
            schema: DiskCacheValueSchema::new("superi.test.failing-i64", 1).unwrap(),
            category,
            recoverability,
        }
    }
}

impl DiskCacheCodec<i64> for FailingEncodeCodec {
    fn schema(&self) -> &DiskCacheValueSchema {
        &self.schema
    }

    fn encode(&self, _kind: EvaluationCacheEntryKind, _value: &i64) -> Result<Vec<u8>> {
        Err(Error::new(
            self.category,
            self.recoverability,
            "injected value codec failure",
        )
        .with_context(ErrorContext::new("superi-test.codec", "encode")))
    }

    fn decode(&self, _kind: EvaluationCacheEntryKind, _bytes: &[u8]) -> Result<i64> {
        unreachable!("a failing encoder never publishes an entry")
    }
}

struct FailingDecodeCodec {
    schema: DiskCacheValueSchema,
    category: ErrorCategory,
    recoverability: Recoverability,
}

impl FailingDecodeCodec {
    fn new(category: ErrorCategory, recoverability: Recoverability) -> Self {
        Self {
            schema: DiskCacheValueSchema::new("superi.test.i64", 1).unwrap(),
            category,
            recoverability,
        }
    }
}

impl DiskCacheCodec<i64> for FailingDecodeCodec {
    fn schema(&self) -> &DiskCacheValueSchema {
        &self.schema
    }

    fn encode(&self, _kind: EvaluationCacheEntryKind, value: &i64) -> Result<Vec<u8>> {
        Ok(value.to_be_bytes().to_vec())
    }

    fn decode(&self, _kind: EvaluationCacheEntryKind, _bytes: &[u8]) -> Result<i64> {
        Err(Error::new(
            self.category,
            self.recoverability,
            "injected value decoder failure",
        )
        .with_context(ErrorContext::new("superi-test.codec", "decode")))
    }
}

fn color_pipeline() -> ColorPipelineMetadata {
    ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::UNSPECIFIED)).unwrap()
}

fn context(color: &ColorPipelineMetadata) -> FrameDiskCacheContext<'_> {
    FrameDiskCacheContext::new(
        ProjectId::from_raw(1),
        &[],
        ParameterStateFingerprint::from_canonical_bytes(b"disk-cache-test-parameters-v1"),
        color,
        RenderSettingsFingerprint::from_canonical_bytes(b"disk-cache-test-render-v1"),
    )
}

fn request_for(node: u128, port: u128, min_x: i32, max_x: i32) -> EvaluationRequest {
    EvaluationRequest::new(
        GraphEndpoint::new(NodeId::from_raw(node), PortId::from_raw(port)),
        RationalTime::new(0, Timebase::integer(24).unwrap()),
        PixelBounds::new(min_x, 0, max_x, 64).unwrap(),
    )
}

fn single_node_graph(calls: Arc<AtomicUsize>, value: i64) -> DirectedAcyclicGraph<ValueNode> {
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(501));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.disk-cache",
                value,
                policy: CachePolicy::PerRegion,
                calls,
            },
        )
        .unwrap();
    graph
}

fn cache(root: &Path, schema_revision: u32) -> FrameDiskCache<i64> {
    FrameDiskCache::new(
        DiskCacheConfig::new(root, 1_024).unwrap(),
        I64Codec::new(schema_revision),
    )
    .unwrap()
}

fn walk_files(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn cache_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_files(root, &mut files);
    files.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "scache")
    });
    files.sort();
    files
}

fn quarantined_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_files(root, &mut files);
    files.retain(|path| {
        path.file_name()
            .is_some_and(|name| name.to_string_lossy().contains(".corrupt-"))
    });
    files.sort();
    files
}

fn assert_actionable(error: &Error, expected_reason: &str) {
    let context = error.contexts().last().unwrap();
    assert_eq!(context.component(), "superi-cache.disk");
    assert!(context.field("path").is_some());
    assert!(context.field("key").is_some());
    assert!(context.field("kind").is_some());
    assert!(context.field("schema_id").is_some());
    assert_eq!(context.field("reason"), Some(expected_reason));
}

#[test]
fn exact_final_value_survives_cache_reconstruction_and_concurrent_reads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<FrameDiskCache<i64>>();

    let root = TempRoot::new("restart");
    let calls = Arc::new(AtomicUsize::new(0));
    let graph = single_node_graph(calls.clone(), 73);
    let color = color_pipeline();

    {
        let first_cache = cache(root.path(), 1);
        let first_scope = first_cache.scope(context(&color));
        let first =
            LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &first_scope)
                .unwrap();
        assert_eq!(*first.value(), 73);
        assert!(first_cache.take_diagnostics().is_empty());
    }

    let reopened = Arc::new(cache(root.path(), 1));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let reopened = Arc::clone(&reopened);
            let graph = &graph;
            let color = &color;
            scope.spawn(move || {
                let disk_scope = reopened.scope(context(color));
                let result = LazyEvaluator::evaluate_with_cache(
                    graph,
                    request_for(1, 101, 0, 64),
                    &disk_scope,
                )
                .unwrap();
                assert_eq!(*result.value(), 73);
            });
        }
    });

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache_files(root.path()).len(), 1);
    assert!(reopened.take_diagnostics().is_empty());
}

#[test]
fn final_and_intermediate_tiers_persist_independently_and_prune_upstream_work() {
    let root = TempRoot::new("tiers");
    let source_calls = Arc::new(AtomicUsize::new(0));
    let output_calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(502));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.disk-source",
                value: 79,
                policy: CachePolicy::Static,
                calls: source_calls.clone(),
            },
        )
        .unwrap();
    graph
        .insert_node(
            NodeId::from_raw(2),
            ValueNode {
                node_type: "superi.test.disk-output",
                value: 83,
                policy: CachePolicy::PerRegion,
                calls: output_calls.clone(),
            },
        )
        .unwrap();
    graph
        .insert_edge(GraphEdge::new(
            EdgeId::from_raw(10),
            GraphEndpoint::new(NodeId::from_raw(1), PortId::from_raw(101)),
            GraphEndpoint::new(NodeId::from_raw(2), PortId::from_raw(201)),
        ))
        .unwrap();
    let color = color_pipeline();

    {
        let first_cache = cache(root.path(), 1);
        let first_scope = first_cache.scope(context(&color));
        let result =
            LazyEvaluator::evaluate_with_cache(&graph, request_for(2, 202, 0, 128), &first_scope)
                .unwrap();
        assert_eq!(*result.value(), 83);
    }

    let files = cache_files(root.path());
    assert_eq!(files.len(), 2);
    assert!(files
        .iter()
        .any(|path| path.components().any(|part| part.as_os_str() == "final")));
    assert!(files.iter().any(|path| path
        .components()
        .any(|part| part.as_os_str() == "intermediate")));

    let reopened = cache(root.path(), 1);
    let reopened_scope = reopened.scope(context(&color));
    let result =
        LazyEvaluator::evaluate_with_cache(&graph, request_for(2, 202, 32, 96), &reopened_scope)
            .unwrap();
    assert_eq!(*result.value(), 83);
    assert_eq!(source_calls.load(Ordering::SeqCst), 1);
    assert_eq!(output_calls.load(Ordering::SeqCst), 2);
}

#[test]
fn payload_corruption_is_quarantined_recomputed_and_replaced_without_result_drift() {
    let root = TempRoot::new("corruption");
    let calls = Arc::new(AtomicUsize::new(0));
    let graph = single_node_graph(calls.clone(), 89);
    let color = color_pipeline();
    let first_cache = cache(root.path(), 1);
    let first_scope = first_cache.scope(context(&color));
    LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &first_scope).unwrap();

    let path = cache_files(root.path()).pop().unwrap();
    let mut bytes = fs::read(&path).unwrap();
    *bytes.last_mut().unwrap() ^= 0xff;
    fs::write(&path, bytes).unwrap();

    let reopened = cache(root.path(), 1);
    let reopened_scope = reopened.scope(context(&color));
    let recovered =
        LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &reopened_scope)
            .unwrap();
    assert_eq!(*recovered.value(), 89);
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let diagnostics = reopened.take_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].category(), ErrorCategory::CorruptData);
    assert_eq!(diagnostics[0].recoverability(), Recoverability::Degraded);
    assert_actionable(&diagnostics[0], "payload_digest_mismatch");
    assert_eq!(quarantined_files(root.path()).len(), 1);

    let second_reopen = cache(root.path(), 1);
    let second_scope = second_reopen.scope(context(&color));
    let reused =
        LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &second_scope)
            .unwrap();
    assert_eq!(*reused.value(), 89);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn semantic_decode_corruption_is_quarantined_while_terminal_codec_failure_retains_bytes() {
    for (label, category, recoverability, expected_action, expected_quarantine) in [
        (
            "decode-corrupt",
            ErrorCategory::CorruptData,
            Recoverability::Degraded,
            "quarantined",
            1,
        ),
        (
            "decode-terminal",
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "entry_retained",
            0,
        ),
    ] {
        let root = TempRoot::new(label);
        let calls = Arc::new(AtomicUsize::new(0));
        let graph = single_node_graph(calls.clone(), 93);
        let color = color_pipeline();
        let first_cache = cache(root.path(), 1);
        let first_scope = first_cache.scope(context(&color));
        LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &first_scope)
            .unwrap();

        let failing = FrameDiskCache::new(
            DiskCacheConfig::new(root.path(), 1_024).unwrap(),
            FailingDecodeCodec::new(category, recoverability),
        )
        .unwrap();
        let failing_scope = failing.scope(context(&color));
        let result =
            LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &failing_scope)
                .unwrap();
        assert_eq!(*result.value(), 93);
        assert_eq!(calls.load(Ordering::SeqCst), 2);

        let diagnostics = failing.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].category(), category);
        assert_eq!(diagnostics[0].recoverability(), recoverability);
        assert_actionable(&diagnostics[0], "codec_decode_failed");
        assert_eq!(
            diagnostics[0].contexts().last().unwrap().field("action"),
            Some(expected_action)
        );
        assert_eq!(quarantined_files(root.path()).len(), expected_quarantine);
        assert_eq!(cache_files(root.path()).len(), 1);
    }
}

#[test]
fn truncated_oversized_and_future_entries_recover_as_bounded_degraded_misses() {
    let cases: [(&str, MutateEntry); 3] = [
        ("truncated_header", |path| {
            OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_len(7)
                .unwrap();
        }),
        ("payload_too_large", |path| {
            let mut file = OpenOptions::new().write(true).open(path).unwrap();
            file.seek(SeekFrom::Start(84)).unwrap();
            file.write_all(&1_025_u64.to_be_bytes()).unwrap();
        }),
        ("unsupported_format_revision", |path| {
            let mut file = OpenOptions::new().write(true).open(path).unwrap();
            file.seek(SeekFrom::Start(8)).unwrap();
            file.write_all(&(PERSISTENT_CACHE_FORMAT_REVISION + 1).to_be_bytes())
                .unwrap();
        }),
    ];

    for (reason, mutate) in cases {
        let root = TempRoot::new(reason);
        let calls = Arc::new(AtomicUsize::new(0));
        let graph = single_node_graph(calls.clone(), 97);
        let color = color_pipeline();
        let first_cache = cache(root.path(), 1);
        let first_scope = first_cache.scope(context(&color));
        LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &first_scope)
            .unwrap();
        let path = cache_files(root.path()).pop().unwrap();
        mutate(&path);

        let reopened = cache(root.path(), 1);
        let reopened_scope = reopened.scope(context(&color));
        let result =
            LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &reopened_scope)
                .unwrap();
        assert_eq!(*result.value(), 97);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let diagnostics = reopened.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].recoverability(), Recoverability::Degraded);
        let expected_category = if reason == "unsupported_format_revision" {
            ErrorCategory::Unsupported
        } else {
            ErrorCategory::CorruptData
        };
        assert_eq!(diagnostics[0].category(), expected_category);
        assert_actionable(&diagnostics[0], reason);
        assert_eq!(quarantined_files(root.path()).len(), 1);
    }
}

#[test]
fn value_schema_revision_isolated_namespaces_without_reinterpreting_old_bytes() {
    let root = TempRoot::new("schema");
    let calls = Arc::new(AtomicUsize::new(0));
    let graph = single_node_graph(calls.clone(), 101);
    let color = color_pipeline();

    let revision_one = cache(root.path(), 1);
    let scope_one = revision_one.scope(context(&color));
    LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &scope_one).unwrap();

    let revision_two = cache(root.path(), 2);
    let scope_two = revision_two.scope(context(&color));
    LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &scope_two).unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(cache_files(root.path()).len(), 2);
    assert!(revision_one.take_diagnostics().is_empty());
    assert!(revision_two.take_diagnostics().is_empty());
}

#[test]
fn persistence_failures_preserve_retryable_user_correctable_and_terminal_classification() {
    let root = TempRoot::new("classification");
    let invalid = DiskCacheConfig::new(root.path(), 0).unwrap_err();
    assert_eq!(invalid.category(), ErrorCategory::InvalidInput);
    assert_eq!(invalid.recoverability(), Recoverability::UserCorrectable);
    assert!(invalid
        .contexts()
        .last()
        .unwrap()
        .field("max_entry_bytes")
        .is_some());

    let color = color_pipeline();
    for (index, category, recoverability) in [
        (
            1_u128,
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
        ),
        (2_u128, ErrorCategory::Internal, Recoverability::Terminal),
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let graph = single_node_graph(calls.clone(), 103);
        let cache = FrameDiskCache::new(
            DiskCacheConfig::new(root.path().join(index.to_string()), 1_024).unwrap(),
            FailingEncodeCodec::new(category, recoverability),
        )
        .unwrap();
        let scope = cache.scope(context(&color));
        let result =
            LazyEvaluator::evaluate_with_cache(&graph, request_for(1, 101, 0, 64), &scope).unwrap();
        assert_eq!(*result.value(), 103);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let diagnostics = cache.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].category(), category);
        assert_eq!(diagnostics[0].recoverability(), recoverability);
        assert_actionable(&diagnostics[0], "codec_encode_failed");
    }
}
