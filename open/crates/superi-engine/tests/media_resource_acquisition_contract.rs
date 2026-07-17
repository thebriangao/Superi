use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rusqlite::Connection;
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ClipId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_engine::media::media_backend_registry;
use superi_engine::resources::{
    acquire_project_resources, acquire_timeline_resources, DecoderResourceRequest,
    MediaResourceRequest, ResourceAcquisitionPolicy,
};
use superi_graph::mutate::{GraphMutation, GraphTransaction, TypedParameterValue};
use superi_graph::value::GraphValue;
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration,
    BackendRegistry, BackendTier, FallbackPolicy, MediaBackend,
};
use superi_media_io::decode::{DecodeOutput, Decoder, DecoderConfig};
use superi_media_io::demux::{
    BackendId, CodecId, MediaSource, SourceLocation, SourceProbe, SourceProbeLimits,
    SourceProbeResult, SourceRequest, StreamId,
};
use superi_media_io::encode::{Encoder, EncoderConfig};
use superi_media_io::mkv_webm::MkvWebmBackend;
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;
use superi_project::document::ProjectDocument;
use superi_project::media::{PortableRelativePath, ReferencedMediaPath};
use superi_project::ProjectDatabase;
use superi_timeline::compile::{TimelineGraphOrigin, TimelineGraphValue};
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};

const MEDIA: MediaId = MediaId::from_raw(0x100);
const ROOT: TimelineId = TimelineId::from_raw(0x200);
const CLIP: ClipId = ClipId::from_raw(0x300);
const STREAM: StreamId = StreamId::new(1);
const SOURCE_FINGERPRINT: &str =
    "sha256:117f5cebcaaf788d1891e84aec57066c73e33d4af308368f640f28a8419f4bbc";
static NEXT_PROJECT_PATH: AtomicUsize = AtomicUsize::new(0);

struct TempProjectFile {
    path: PathBuf,
}

impl TempProjectFile {
    fn new() -> Self {
        Self {
            path: std::env::temp_dir().join(format!(
                "superi-engine-migrated-project-{}-{}.superi",
                std::process::id(),
                NEXT_PROJECT_PATH.fetch_add(1, Ordering::Relaxed)
            )),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempProjectFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        for suffix in ["-journal", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.path.display()));
        }
    }
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/slice/video-cfr/v1/input.webm")
}

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn project() -> EditorialProject {
    let source_timebase = Timebase::NANOSECONDS;
    let edit_timebase = Timebase::integer(24).unwrap();
    let path = fixture_path();
    let media = LinkedMediaReference::with_fingerprint(
        MEDIA,
        "canonical source",
        path.to_string_lossy(),
        Some(range(0, 4_000_000_000, source_timebase)),
        SOURCE_FINGERPRINT,
    )
    .unwrap();
    let clip = Clip::new(
        CLIP,
        "trimmed canonical source",
        ClipSource::Media(MEDIA),
        range(1_000_000_000, 2_000_000_000, source_timebase),
        range(0, 48, edit_timebase),
    )
    .unwrap();
    let timeline = Timeline::new(
        ROOT,
        "canonical",
        edit_timebase,
        RationalTime::zero(edit_timebase),
        vec![Track::new(
            TrackId::from_raw(0x400),
            "V1",
            TrackSemantics::Video(VideoTrackSemantics::new(
                FrameRate::FPS_24,
                VideoCompositing::Over,
            )),
            vec![TrackItem::Clip(clip)],
        )],
    );
    EditorialProject::new(
        ProjectId::from_raw(0x500),
        "resource project",
        [media],
        [timeline],
    )
    .unwrap()
}

fn request() -> MediaResourceRequest {
    MediaResourceRequest::new(
        SourceRequest::new(MEDIA, SourceLocation::Path(fixture_path())),
        [DecoderResourceRequest::new(STREAM)],
    )
    .unwrap()
}

fn operation() -> OperationContext {
    OperationContext::new(MediaPriority::Interactive)
}

#[test]
fn project_relative_target_drives_the_real_source_request_and_acquisition_path() {
    let mut project = project();
    let relative = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("../../test-fixtures/slice/video-cfr/v1/input.webm").unwrap(),
    );
    project
        .edit(0, |draft| {
            draft
                .media_reference_mut(MEDIA)?
                .set_target(relative.to_target());
            Ok(())
        })
        .unwrap();
    let project_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("portable.superi");

    let request = MediaResourceRequest::from_project_media(
        &project,
        &project_file,
        MEDIA,
        [DecoderResourceRequest::new(STREAM)],
    )
    .unwrap();
    assert_eq!(request.source().media_id(), MEDIA);
    assert_eq!(
        request.source().expected_fingerprint(),
        Some(SOURCE_FINGERPRINT)
    );
    let SourceLocation::Path(resolved) = request.source().location() else {
        panic!("project media path must resolve to a local source path")
    };
    assert_eq!(
        fs::canonicalize(resolved).unwrap(),
        fs::canonicalize(fixture_path()).unwrap()
    );

    let registry = media_backend_registry().unwrap();
    let resources = acquire_timeline_resources(
        &project,
        ROOT,
        &registry,
        [request],
        ResourceAcquisitionPolicy::default(),
        &operation(),
    )
    .unwrap();
    let acquired = resources.media(MEDIA).unwrap();
    assert_eq!(acquired.info().identity().media_id(), MEDIA);
    assert_eq!(acquired.info().identity().fingerprint(), SOURCE_FINGERPRINT);
    assert_eq!(
        acquired
            .source_selection()
            .selected()
            .container_id()
            .as_str(),
        "webm"
    );
}

#[test]
fn project_media_request_rejects_opaque_targets_missing_ids_and_relative_project_files() {
    let mut opaque = project();
    opaque
        .edit(0, |draft| {
            draft
                .media_reference_mut(MEDIA)?
                .set_target("urn:opaque:media");
            Ok(())
        })
        .unwrap();
    let project_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("opaque.superi");
    let opaque_error = MediaResourceRequest::from_project_media(
        &opaque,
        &project_file,
        MEDIA,
        [DecoderResourceRequest::new(STREAM)],
    )
    .unwrap_err();
    assert_eq!(opaque_error.category(), ErrorCategory::Unsupported);

    let missing_error = MediaResourceRequest::from_project_media(
        &project(),
        &project_file,
        MediaId::from_raw(999),
        [DecoderResourceRequest::new(STREAM)],
    )
    .unwrap_err();
    assert_eq!(missing_error.category(), ErrorCategory::NotFound);

    let mut unavailable = project();
    unavailable
        .edit(0, |draft| {
            draft.media_reference_mut(MEDIA)?.mark_missing();
            Ok(())
        })
        .unwrap();
    let unavailable_error = MediaResourceRequest::from_project_media(
        &unavailable,
        &project_file,
        MEDIA,
        [DecoderResourceRequest::new(STREAM)],
    )
    .unwrap_err();
    assert_eq!(unavailable_error.category(), ErrorCategory::NotFound);

    let mut relative = project();
    relative
        .edit(0, |draft| {
            draft.media_reference_mut(MEDIA)?.set_target(
                ReferencedMediaPath::project_relative(PortableRelativePath::new(
                    "Media/clip.webm",
                )?)
                .to_target(),
            );
            Ok(())
        })
        .unwrap();
    let relative_error = MediaResourceRequest::from_project_media(
        &relative,
        "relative/project.superi",
        MEDIA,
        [DecoderResourceRequest::new(STREAM)],
    )
    .unwrap_err();
    assert_eq!(relative_error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn engine_registry_owns_all_in_tree_container_sources_without_changing_codec_policy() {
    let registry = media_backend_registry().unwrap();
    let sources = registry
        .registrations()
        .filter(|registration| {
            registration
                .capabilities()
                .contains(&BackendCapability::Source)
        })
        .map(|registration| {
            (
                registration.backend().descriptor().id().as_str(),
                registration.priority(),
                registration.tier(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        sources,
        [
            ("mkv-webm", 100, BackendTier::Primary),
            ("mp4-mov", 100, BackendTier::Primary),
            ("mxf", 100, BackendTier::Primary),
            ("pcm-containers", 100, BackendTier::Primary),
        ]
    );
    let av1 = registry
        .select(
            &superi_media_io::backend::BackendRequirement::decode(CodecId::new("av1").unwrap()),
            FallbackPolicy::Disallow,
        )
        .unwrap();
    assert_eq!(av1.primary().descriptor().id().as_str(), "rust-av1");
    assert!(!av1.fallback_used());
}

#[test]
fn real_timeline_acquisition_compiles_once_and_preserves_media_semantics() {
    let project = project();
    let registry = media_backend_registry().unwrap();
    let mut resources = acquire_timeline_resources(
        &project,
        ROOT,
        &registry,
        [request()],
        ResourceAcquisitionPolicy::default(),
        &operation(),
    )
    .unwrap();

    let compilation = resources.compilation();
    assert_eq!(compilation.project_id(), project.id());
    assert_eq!(compilation.root_timeline_id(), ROOT);
    assert_eq!(compilation.project_revision(), project.revision());
    assert_eq!(compilation.snapshot().dag().node_count(), 3);
    assert_eq!(compilation.snapshot().dag().edge_count(), 2);

    let media = resources.media(MEDIA).unwrap();
    assert_eq!(media.info().identity().media_id(), MEDIA);
    assert_eq!(media.info().identity().fingerprint(), SOURCE_FINGERPRINT);
    assert_eq!(media.info().streams().len(), 1);
    assert_eq!(media.info().streams()[0].id(), STREAM);
    assert_eq!(media.info().streams()[0].codec().as_str(), "av1");
    assert_eq!(
        media.source_selection().selected().backend_id().as_str(),
        "mkv-webm"
    );
    assert_eq!(
        media.source_selection().selected().container_id().as_str(),
        "webm"
    );
    assert_eq!(
        media.source_selection().selected().confidence().value(),
        100
    );
    assert!(!media.source_selection().fallback_used());
    assert!(media.source_selection().fallbacks().is_empty());
    assert!(media.source_selection().bytes_examined() > 0);
    assert_eq!(media.source_selection().source_length(), 28_178);

    let decoder = media.decoder(STREAM).unwrap();
    assert_eq!(
        decoder.selection().selected_backend_id().as_str(),
        "rust-av1"
    );
    assert!(!decoder.selection().fallback_used());
    assert!(decoder.selection().fallback_backend_ids().is_empty());
    assert_eq!(decoder.config().stream(), &media.info().streams()[0]);

    let frame = loop {
        let packet = match resources
            .media_mut(MEDIA)
            .unwrap()
            .source_mut()
            .read_packet(&operation())
            .unwrap()
        {
            ReadOutcome::Complete(packet) => packet,
            ReadOutcome::Partial { .. } => panic!("canonical source returned a partial packet"),
            ReadOutcome::EndOfStream => panic!("canonical source ended before a decoded frame"),
            _ => panic!("canonical source returned an unknown packet-read outcome"),
        };
        let packet_timing = packet.timing();
        assert!(packet.metadata().get("container.offset").is_some());
        let decoder = resources
            .media_mut(MEDIA)
            .unwrap()
            .decoder_mut(STREAM)
            .unwrap();
        decoder
            .decoder_mut()
            .send_packet(packet, &operation())
            .unwrap();
        match decoder.decoder_mut().receive(&operation()).unwrap() {
            DecodeOutput::Frame(frame) => {
                assert_eq!(
                    frame.timestamp(),
                    packet_timing.presentation_time().unwrap()
                );
                assert_eq!(frame.duration(), packet_timing.duration().unwrap());
                break frame;
            }
            DecodeOutput::NeedInput => {}
            DecodeOutput::Audio(_) => panic!("AV1 decoder returned audio"),
            DecodeOutput::EndOfStream => panic!("AV1 decoder ended before flush"),
            _ => panic!("unexpected decoder output"),
        }
    };
    assert_eq!(frame.timestamp(), RationalTime::zero(Timebase::NANOSECONDS));
    assert_eq!(frame.duration().value(), 41_666_666);
    assert_eq!(frame.format().width(), 96);
    assert_eq!(frame.format().height(), 54);
    assert_eq!(frame.format().pixel_format(), PixelFormat::Yuv420p8);
    assert_eq!(frame.format().pixel_format().bits_per_component(), 8);
    assert_eq!(
        frame.format().color_space(),
        ColorSpace::new(
            ColorPrimaries::Unspecified,
            TransferFunction::Unspecified,
            MatrixCoefficients::Bt709,
            ColorRange::Limited,
        )
    );
    assert_eq!(frame.format().alpha_mode(), AlphaMode::Opaque);
    assert!(frame.metadata().get("container.offset").is_some());
}

fn edited_project_document() -> ProjectDocument {
    let mut document = ProjectDocument::new(project(), ROOT).unwrap();
    let compilation = document.timeline_graph(ROOT).unwrap();
    let node_id = compilation
        .index()
        .node(TimelineGraphOrigin::Object(EditorialObjectId::Clip(CLIP)))
        .unwrap();
    let graph_snapshot = compilation.snapshot();
    let parameter = graph_snapshot
        .node(node_id)
        .unwrap()
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "name")
        .unwrap();
    let parameter_id = parameter.id();
    let value_type = parameter.value().value_type().clone();

    document
        .edit(0, |draft| {
            let compilation = draft.timeline_graph_mut(ROOT)?;
            let graph_revision = compilation.graph().revision();
            compilation
                .graph_mut()
                .apply(GraphTransaction::with_mutations(
                    graph_revision,
                    [GraphMutation::SetParameter {
                        node_id,
                        parameter_id,
                        value: TypedParameterValue::new(
                            value_type,
                            GraphValue::domain(TimelineGraphValue::Text(
                                "published intelligent result".to_owned(),
                            )),
                        ),
                    }],
                ))?;
            Ok(())
        })
        .unwrap();

    document
}

fn downgrade_project_fixture_to_schema_zero(path: &Path) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(
            "BEGIN IMMEDIATE;
             DROP TABLE command_log_records;
             DROP TABLE command_log_metadata;
             DROP TABLE extension_records;
             DROP TABLE settings_component;
             ALTER TABLE project_metadata RENAME TO current_project_metadata;
             ALTER TABLE timeline_component RENAME TO current_timeline_component;
             ALTER TABLE graph_components RENAME TO current_graph_components;
             CREATE TABLE project_metadata (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format TEXT NOT NULL CHECK (format = 'superi.project'), format_version TEXT NOT NULL, primitive_schema_revision INTEGER NOT NULL CHECK (primitive_schema_revision > 0), project_id BLOB NOT NULL CHECK (length(project_id) = 16), document_revision TEXT NOT NULL, root_timeline_id BLOB NOT NULL CHECK (length(root_timeline_id) = 16)) STRICT;
             CREATE TABLE timeline_component (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format_revision INTEGER NOT NULL CHECK (format_revision >= 0), document BLOB NOT NULL CHECK (length(document) <= 67108864)) STRICT;
             CREATE TABLE graph_components (graph_id BLOB PRIMARY KEY CHECK (length(graph_id) = 16), graph_kind TEXT NOT NULL CHECK (graph_kind IN ('timeline', 'standalone')), root_timeline_id BLOB CHECK (root_timeline_id IS NULL OR length(root_timeline_id) = 16), name TEXT, graph_revision TEXT NOT NULL, format_revision INTEGER NOT NULL CHECK (format_revision >= 0), document BLOB NOT NULL CHECK (length(document) <= 67108864), CHECK ((graph_kind = 'timeline' AND root_timeline_id IS NOT NULL AND name IS NULL) OR (graph_kind = 'standalone' AND root_timeline_id IS NULL AND name IS NOT NULL AND length(name) > 0))) STRICT, WITHOUT ROWID;
             INSERT INTO project_metadata (singleton, format, format_version, primitive_schema_revision, project_id, document_revision, root_timeline_id) SELECT singleton, format, '0.9.0', primitive_schema_revision, project_id, document_revision, root_timeline_id FROM current_project_metadata;
             INSERT INTO timeline_component (singleton, format_revision, document) SELECT singleton, format_revision, document FROM current_timeline_component;
             INSERT INTO graph_components (graph_id, graph_kind, root_timeline_id, name, graph_revision, format_revision, document) SELECT graph_id, graph_kind, root_timeline_id, name, graph_revision, format_revision, document FROM current_graph_components;
             DROP TABLE current_graph_components;
             DROP TABLE current_timeline_component;
             DROP TABLE current_project_metadata;
             DROP TABLE audio_component;
             PRAGMA user_version = 0;
             COMMIT;",
        )
        .unwrap();
}

#[test]
fn project_acquisition_preserves_published_editable_graph_state() {
    let document = edited_project_document();

    let snapshot = document.snapshot();
    let registry = media_backend_registry().unwrap();
    let resources = acquire_project_resources(
        &snapshot,
        &registry,
        [request()],
        ResourceAcquisitionPolicy::default(),
        &operation(),
    )
    .unwrap();

    assert_eq!(
        resources.compilation(),
        snapshot.timeline_graph(ROOT).unwrap()
    );
    assert_eq!(resources.compilation().snapshot().revision(), 2);
    assert_eq!(
        resources.media(MEDIA).unwrap().info().streams()[0].id(),
        STREAM
    );
}

#[test]
fn migrated_project_reaches_engine_with_the_exact_edited_graph() {
    let artifact = TempProjectFile::new();
    let expected = edited_project_document().snapshot();
    let mut database = ProjectDatabase::create(artifact.path()).unwrap();
    database.replace(&expected).unwrap();
    drop(database);
    downgrade_project_fixture_to_schema_zero(artifact.path());

    let database = ProjectDatabase::open(artifact.path()).unwrap();
    assert!(database.was_migrated());
    assert_eq!(database.source_schema_revision(), 0);
    let migrated = database.load().unwrap().snapshot();
    assert_eq!(migrated, expected);

    let registry = media_backend_registry().unwrap();
    let resources = acquire_project_resources(
        &migrated,
        &registry,
        [request()],
        ResourceAcquisitionPolicy::default(),
        &operation(),
    )
    .unwrap();
    assert_eq!(
        resources.compilation(),
        expected.timeline_graph(ROOT).unwrap()
    );
    assert_eq!(resources.compilation().snapshot().revision(), 2);
    assert_eq!(
        resources.media(MEDIA).unwrap().info().streams()[0].id(),
        STREAM
    );
}

#[test]
fn exact_resource_set_and_explicit_decoder_selection_fail_before_publication() {
    let project = project();
    let registry = media_backend_registry().unwrap();
    let missing = acquire_timeline_resources(
        &project,
        ROOT,
        &registry,
        [],
        ResourceAcquisitionPolicy::default(),
        &operation(),
    )
    .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::NotFound);

    let duplicate = acquire_timeline_resources(
        &project,
        ROOT,
        &registry,
        [request(), request()],
        ResourceAcquisitionPolicy::default(),
        &operation(),
    )
    .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::Conflict);

    let no_decoders = MediaResourceRequest::new(
        SourceRequest::new(MEDIA, SourceLocation::Path(fixture_path())),
        [],
    )
    .unwrap_err();
    assert_eq!(no_decoders.category(), ErrorCategory::InvalidInput);

    let duplicate_streams = MediaResourceRequest::new(
        SourceRequest::new(MEDIA, SourceLocation::Path(fixture_path())),
        [
            DecoderResourceRequest::new(STREAM),
            DecoderResourceRequest::new(STREAM),
        ],
    )
    .unwrap_err();
    assert_eq!(duplicate_streams.category(), ErrorCategory::Conflict);
}

struct TestDecoderBackend {
    descriptor: BackendDescriptor,
    fail: bool,
    create_calls: AtomicUsize,
}

impl TestDecoderBackend {
    fn new(id: &str, fail: bool) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                BackendId::new(id).unwrap(),
                format!("{id} test decoder"),
            )
            .unwrap(),
            fail,
            create_calls: AtomicUsize::new(0),
        }
    }

    fn create_calls(&self) -> usize {
        self.create_calls.load(Ordering::SeqCst)
    }
}

struct TestDecoder {
    config: DecoderConfig,
    flushed: bool,
}

impl Decoder for TestDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(
        &mut self,
        _packet: superi_media_io::demux::Packet,
        operation: &OperationContext,
    ) -> Result<()> {
        operation.check("test_decoder_send")
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("test_decoder_receive")?;
        Ok(if self.flushed {
            DecodeOutput::EndOfStream
        } else {
            DecodeOutput::NeedInput
        })
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("test_decoder_flush")?;
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("test_decoder_reset")?;
        self.flushed = false;
        Ok(())
    }
}

impl MediaBackend for TestDecoderBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("test_decoder_probe")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("test_decoder_open")?;
        Err(test_backend_error("decoder backend cannot open sources"))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("test_decoder_create")?;
        self.create_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(test_backend_error("selected decoder factory failed"));
        }
        Ok(Box::new(TestDecoder {
            config: config.clone(),
            flushed: false,
        }))
    }

    fn create_encoder(
        &self,
        _config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("test_decoder_encode")?;
        Err(test_backend_error("decoder backend cannot encode"))
    }
}

fn test_backend_error(message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        message,
    )
    .with_context(ErrorContext::new("resource-contract", "test_backend"))
}

fn register_fallback_source(registry: &mut BackendRegistry) {
    registry
        .register(
            BackendRegistration::new(
                Arc::new(MkvWebmBackend::new().unwrap()),
                BackendCapabilities::new([BackendCapability::Source]),
                100,
                BackendTier::Fallback,
            )
            .unwrap(),
        )
        .unwrap();
}

fn register_fallback_decoder(
    registry: &mut BackendRegistry,
    backend: Arc<TestDecoderBackend>,
    priority: u16,
) {
    registry
        .register(
            BackendRegistration::new(
                backend,
                BackendCapabilities::new([BackendCapability::Decode(CodecId::new("av1").unwrap())]),
                priority,
                BackendTier::Fallback,
            )
            .unwrap(),
        )
        .unwrap();
}

fn fallback_policy() -> ResourceAcquisitionPolicy {
    ResourceAcquisitionPolicy::new(
        SourceProbeLimits::default(),
        FallbackPolicy::AllowRegistered,
        FallbackPolicy::AllowRegistered,
    )
}

#[test]
fn fallback_is_policy_evidence_not_an_exception_retry() {
    let project = project();
    let mut registry = BackendRegistry::new();
    register_fallback_source(&mut registry);
    let failing = Arc::new(TestDecoderBackend::new("selected-fallback-decoder", true));
    let uncalled = Arc::new(TestDecoderBackend::new("uncalled-fallback-decoder", false));
    register_fallback_decoder(&mut registry, Arc::clone(&failing), 200);
    register_fallback_decoder(&mut registry, Arc::clone(&uncalled), 100);

    let error = acquire_timeline_resources(
        &project,
        ROOT,
        &registry,
        [request()],
        fallback_policy(),
        &operation(),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(failing.create_calls(), 1);
    assert_eq!(uncalled.create_calls(), 0);
}

#[test]
fn degraded_fallback_and_cancelled_acquisition_recover_with_fresh_context() {
    let project = project();
    let mut registry = BackendRegistry::new();
    register_fallback_source(&mut registry);
    let decoder_backend = Arc::new(TestDecoderBackend::new("fallback-av1", false));
    register_fallback_decoder(&mut registry, Arc::clone(&decoder_backend), 100);

    let disallowed = acquire_timeline_resources(
        &project,
        ROOT,
        &registry,
        [request()],
        ResourceAcquisitionPolicy::default(),
        &operation(),
    )
    .unwrap_err();
    assert_eq!(disallowed.category(), ErrorCategory::Unsupported);
    assert_eq!(decoder_backend.create_calls(), 0);

    let cancelled = operation();
    cancelled.cancellation_token().cancel();
    let cancelled_error = acquire_timeline_resources(
        &project,
        ROOT,
        &registry,
        [request()],
        fallback_policy(),
        &cancelled,
    )
    .unwrap_err();
    assert_eq!(cancelled_error.category(), ErrorCategory::Cancelled);
    assert_eq!(decoder_backend.create_calls(), 0);

    let recovered = acquire_timeline_resources(
        &project,
        ROOT,
        &registry,
        [request()],
        fallback_policy(),
        &operation(),
    )
    .unwrap();
    let media = recovered.media(MEDIA).unwrap();
    assert!(media.source_selection().fallback_used());
    assert_eq!(
        media.source_selection().selected().backend_id().as_str(),
        "mkv-webm"
    );
    let decoder = media.decoder(STREAM).unwrap();
    assert!(decoder.selection().fallback_used());
    assert_eq!(
        decoder.selection().selected_backend_id().as_str(),
        "fallback-av1"
    );
    assert!(decoder.selection().fallback_backend_ids().is_empty());
    assert_eq!(decoder_backend.create_calls(), 1);
}
