use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{Connection, OpenFlags};
use superi_audio::mixing::{ChannelMap, ClipMixControls, ClipMixMutation};
use superi_audio::serialize::CLIP_MIX_FORMAT_REVISION;
use superi_core::error::{ErrorCategory, ErrorContext, Recoverability};
use superi_core::ids::{ClipId, GraphId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::{
    CapabilityId, CapabilitySet, ComponentId, SemanticVersion, VersionIdentifier,
};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_graph::mutate::{EditableGraph, GraphMutation, GraphTransaction, TypedParameterValue};
use superi_graph::serialize::GRAPH_DOCUMENT_FORMAT_REVISION;
use superi_graph::value::GraphValue;
use superi_project::document::{ProjectDocument, ProjectGraph, StandaloneProjectGraph};
use superi_project::extensions::{
    ProjectExtensionCommand, ProjectExtensionFailure, ProjectExtensionKind,
    ProjectExtensionLifecycle, ProjectExtensionRecord, ProjectExtensionRecordId,
};
use superi_project::settings::PROJECT_SETTINGS_FORMAT_REVISION;
use superi_project::{
    ProjectDatabase, PROJECT_APPLICATION_ID, PROJECT_FORMAT, PROJECT_FORMAT_VERSION,
    PROJECT_SCHEMA_REVISION,
};
use superi_timeline::compile::{
    CompiledTimelineGraphValue, TimelineGraphOrigin, TimelineGraphValue,
};
use superi_timeline::ids::MulticamAngleId;
use superi_timeline::media::{RelinkDecision, RelinkStatus};
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};
use superi_timeline::multicam::{
    MulticamAngle, MulticamAudioPolicy, MulticamClip, MulticamSource, MulticamSyncMethod,
};
use superi_timeline::retime::{ClipTimeMap, PlaybackRate};
use superi_timeline::serialize::TIMELINE_STATE_FORMAT_REVISION;

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

const PROJECT: ProjectId = ProjectId::from_raw(700);
const SOURCE: TimelineId = TimelineId::from_raw(701);
const ROOT: TimelineId = TimelineId::from_raw(702);
const SOURCE_TRACK_A: TrackId = TrackId::from_raw(703);
const SOURCE_TRACK_B: TrackId = TrackId::from_raw(704);
const ROOT_TRACK: TrackId = TrackId::from_raw(705);
const CAMERA_A: MediaId = MediaId::from_raw(706);
const CAMERA_B: MediaId = MediaId::from_raw(707);
const SOURCE_CLIP_A: ClipId = ClipId::from_raw(708);
const SOURCE_CLIP_B: ClipId = ClipId::from_raw(709);
const ROOT_CLIP: ClipId = ClipId::from_raw(710);
const ANGLE_A: MulticamAngleId = MulticamAngleId::from_raw(711);
const ANGLE_B: MulticamAngleId = MulticamAngleId::from_raw(712);
const STANDALONE: GraphId = GraphId::from_raw(713);

struct TempProject {
    path: PathBuf,
}

impl TempProject {
    fn new(label: &str) -> Self {
        Self {
            path: std::env::temp_dir().join(format!(
                "superi-project-{label}-{}-{}.superi",
                std::process::id(),
                NEXT_PATH.fetch_add(1, Ordering::Relaxed)
            )),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        for suffix in ["-journal", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.path.display()));
        }
    }
}

fn record_rate() -> Timebase {
    FrameRate::FPS_24.timebase()
}

fn media_rate() -> Timebase {
    FrameRate::FPS_48.timebase()
}

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn semantics() -> TrackSemantics {
    TrackSemantics::Video(VideoTrackSemantics::new(
        FrameRate::FPS_24,
        VideoCompositing::Over,
    ))
}

fn source_clip(id: ClipId, media: MediaId, source_start: i64) -> TrackItem {
    TrackItem::Clip(
        Clip::new(
            id,
            format!("source {}", id.raw()),
            ClipSource::Media(media),
            range(source_start, 48, media_rate()),
            range(0, 24, record_rate()),
        )
        .unwrap(),
    )
}

fn editorial_project() -> EditorialProject {
    let source = Timeline::new(
        SOURCE,
        "synchronized sources",
        record_rate(),
        RationalTime::zero(record_rate()),
        vec![
            Track::new(
                SOURCE_TRACK_A,
                "camera a",
                semantics(),
                vec![source_clip(SOURCE_CLIP_A, CAMERA_A, 100)],
            ),
            Track::new(
                SOURCE_TRACK_B,
                "camera b",
                semantics(),
                vec![source_clip(SOURCE_CLIP_B, CAMERA_B, 200)],
            ),
        ],
    );
    let mut nested_clip = Clip::new(
        ROOT_CLIP,
        "multicam interview",
        ClipSource::Timeline(SOURCE),
        range(0, 24, record_rate()),
        range(0, 24, record_rate()),
    )
    .unwrap();
    nested_clip
        .set_time_map(
            ClipTimeMap::speed(
                nested_clip.record_range().duration(),
                RationalTime::zero(record_rate()),
                PlaybackRate::new(2, 1).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let root = Timeline::new(
        ROOT,
        "edited interview",
        record_rate(),
        RationalTime::new(86_400, record_rate()),
        vec![Track::new(
            ROOT_TRACK,
            "V1",
            semantics(),
            vec![TrackItem::Clip(nested_clip)],
        )],
    );
    let mut project = EditorialProject::new(
        PROJECT,
        "durable project",
        [
            LinkedMediaReference::with_fingerprint(
                CAMERA_A,
                "camera a",
                "urn:camera:a",
                Some(range(0, 400, media_rate())),
                "fingerprint-a",
            )
            .unwrap(),
            LinkedMediaReference::with_fingerprint(
                CAMERA_B,
                "camera b",
                "urn:camera:b",
                Some(range(0, 400, media_rate())),
                "fingerprint-b",
            )
            .unwrap(),
        ],
        [source, root],
    )
    .unwrap();
    project
        .edit(0, |draft| {
            assert_eq!(
                draft
                    .media_reference_mut(CAMERA_B)?
                    .consider_relink("urn:camera:b:replacement", "wrong-fingerprint")?,
                RelinkDecision::RejectedFingerprintMismatch
            );
            let source = draft.timeline_mut(SOURCE)?;
            source.set_multicam_source(MulticamSource::new(
                MulticamSyncMethod::ClipMarker("sync".to_owned()),
                [
                    MulticamAngle::new(ANGLE_A, "wide", "A", [SOURCE_CLIP_A])?,
                    MulticamAngle::new(ANGLE_B, "close", "B", [SOURCE_CLIP_B])?,
                ],
            )?)?;
            let mut multicam = MulticamClip::new(
                ROOT_CLIP,
                range(0, 24, record_rate()),
                ANGLE_A,
                MulticamAudioPolicy::Fixed(ANGLE_A),
            )?;
            multicam.switch_range(range(12, 8, record_rate()), ANGLE_B)?;
            draft.timeline_mut(ROOT)?.upsert_multicam_clip(multicam)?;
            Ok(())
        })
        .unwrap();
    project
}

fn project_document() -> ProjectDocument {
    let mut document = ProjectDocument::new(editorial_project(), ROOT).unwrap();
    let compilation = document.timeline_graph(ROOT).unwrap();
    let node_id = compilation
        .index()
        .node(TimelineGraphOrigin::Object(EditorialObjectId::Clip(
            ROOT_CLIP,
        )))
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
    let standalone = StandaloneProjectGraph::new(
        "reusable analysis",
        EditableGraph::<CompiledTimelineGraphValue>::new(STANDALONE),
    )
    .unwrap();

    document
        .edit(0, |draft| {
            let stereo = ChannelLayout::stereo();
            let controls = ClipMixControls::new(
                stereo.clone(),
                stereo,
                [
                    ChannelMap::new(ChannelPosition::FrontLeft, ChannelPosition::FrontRight, 0.5)?,
                    ChannelMap::new(
                        ChannelPosition::FrontRight,
                        ChannelPosition::FrontLeft,
                        0.75,
                    )?,
                ],
            )?
            .with_gain(f32::from_bits(0x3f40_0001))?
            .with_fades(4_801, 9_601)?
            .with_pan(f32::from_bits(0xbe80_0001))?
            .with_phase_inverted([ChannelPosition::FrontRight])?;
            draft
                .clip_mix_state_mut()
                .apply(0, &[ClipMixMutation::set(ROOT_CLIP, controls)])?;
            {
                let compilation = draft.timeline_graph_mut(ROOT)?;
                let revision = compilation.graph().revision();
                compilation
                    .graph_mut()
                    .apply(GraphTransaction::with_mutations(
                        revision,
                        [GraphMutation::SetParameter {
                            node_id,
                            parameter_id,
                            value: TypedParameterValue::new(
                                value_type,
                                GraphValue::domain(TimelineGraphValue::Text(
                                    "durable direct graph edit".to_owned(),
                                )),
                            ),
                        }],
                    ))?;
            }
            draft.insert_graph(ProjectGraph::Standalone(standalone))
        })
        .unwrap();
    let requested = CapabilitySet::new([
        CapabilityId::new("superi.capability.project-read").unwrap(),
        CapabilityId::new("superi.capability.project-mutate").unwrap(),
    ]);
    let failure =
        ProjectExtensionFailure::new(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "future extension runtime is unavailable",
            [ErrorContext::new("example.future-extension", "restore")
                .with_field("opaque", "retained")],
            5,
            2,
        )
        .unwrap();
    let extension = ProjectExtensionRecord::new(
        ComponentId::new("example.future-extension").unwrap(),
        ProjectExtensionRecordId::new("opaque-record").unwrap(),
        SemanticVersion::new(9, 8, 7),
        ProjectExtensionKind::new(ComponentId::new("example.future-kind").unwrap()),
        VersionIdentifier::new(
            ComponentId::new("example.future-schema").unwrap(),
            SemanticVersion::new(6, 5, 4),
        ),
        requested.clone(),
        CapabilitySet::new([CapabilityId::new("superi.capability.project-read").unwrap()]),
        ProjectExtensionLifecycle::Quarantined,
        Some(failure),
        vec![0, 1, 2, 0xfe, 0xff],
    )
    .unwrap();
    document
        .execute_extension_command(
            document.revision(),
            ProjectExtensionCommand::upsert(extension),
        )
        .unwrap();
    document
}

fn clip_name(document: &ProjectDocument) -> String {
    let compilation = document.timeline_graph(ROOT).unwrap();
    let node_id = compilation
        .index()
        .node(TimelineGraphOrigin::Object(EditorialObjectId::Clip(
            ROOT_CLIP,
        )))
        .unwrap();
    compilation
        .snapshot()
        .node(node_id)
        .unwrap()
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "name")
        .unwrap()
        .value()
        .payload()
        .as_domain()
        .and_then(|value| match value {
            TimelineGraphValue::Text(value) => Some(value.clone()),
            _ => None,
        })
        .unwrap()
}

#[derive(Debug, Eq, PartialEq)]
struct GraphEvidence {
    graph_id: Vec<u8>,
    kind: String,
    root_timeline_id: Option<Vec<u8>>,
    name: Option<String>,
    revision: String,
    format_revision: i64,
    byte_length: i64,
    digest: Vec<u8>,
    document: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
struct ExtensionEvidence {
    extension_id: String,
    record_id: String,
    metadata_format_revision: i64,
    metadata_byte_length: i64,
    metadata_digest: Vec<u8>,
    metadata: Vec<u8>,
    payload_byte_length: i64,
    payload_digest: Vec<u8>,
    payload: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
struct DatabaseEvidence {
    application_id: i64,
    schema_revision: i64,
    schema: Vec<(String, String)>,
    metadata: (String, String, i64, Vec<u8>, String, Vec<u8>, Vec<u8>),
    timeline: (i64, i64, Vec<u8>, Vec<u8>),
    settings: (i64, i64, Vec<u8>, Vec<u8>),
    audio: (i64, i64, Vec<u8>, Vec<u8>),
    extensions: Vec<ExtensionEvidence>,
    graphs: Vec<GraphEvidence>,
}

fn database_evidence(path: &Path) -> DatabaseEvidence {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(path, flags).unwrap();
    let application_id = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .unwrap();
    let schema_revision = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let schema = {
        let mut statement = connection
            .prepare(
                "SELECT type, name FROM sqlite_schema \
                 WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
            )
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    let metadata = connection
        .query_row(
            "SELECT format, format_version, primitive_schema_revision, project_id, \
             document_revision, root_timeline_id, manifest_sha256 FROM project_metadata",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .unwrap();
    let timeline = connection
        .query_row(
            "SELECT format_revision, byte_length, sha256, document FROM timeline_component",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let settings = connection
        .query_row(
            "SELECT format_revision, byte_length, sha256, document FROM settings_component",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let audio = connection
        .query_row(
            "SELECT format_revision, byte_length, sha256, document FROM audio_component",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let extensions = {
        let mut statement = connection
            .prepare(
                "SELECT extension_id, record_id, metadata_format_revision, metadata_byte_length, \
                 metadata_sha256, metadata, payload_byte_length, payload_sha256, payload \
                 FROM extension_records ORDER BY extension_id, record_id",
            )
            .unwrap();
        statement
            .query_map([], |row| {
                Ok(ExtensionEvidence {
                    extension_id: row.get(0)?,
                    record_id: row.get(1)?,
                    metadata_format_revision: row.get(2)?,
                    metadata_byte_length: row.get(3)?,
                    metadata_digest: row.get(4)?,
                    metadata: row.get(5)?,
                    payload_byte_length: row.get(6)?,
                    payload_digest: row.get(7)?,
                    payload: row.get(8)?,
                })
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    let graphs = {
        let mut statement = connection
            .prepare(
                "SELECT graph_id, graph_kind, root_timeline_id, name, graph_revision, \
                 format_revision, byte_length, sha256, document \
                 FROM graph_components ORDER BY graph_id",
            )
            .unwrap();
        statement
            .query_map([], |row| {
                Ok(GraphEvidence {
                    graph_id: row.get(0)?,
                    kind: row.get(1)?,
                    root_timeline_id: row.get(2)?,
                    name: row.get(3)?,
                    revision: row.get(4)?,
                    format_revision: row.get(5)?,
                    byte_length: row.get(6)?,
                    digest: row.get(7)?,
                    document: row.get(8)?,
                })
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    DatabaseEvidence {
        application_id,
        schema_revision,
        schema,
        metadata,
        timeline,
        settings,
        audio,
        extensions,
        graphs,
    }
}

fn create_persisted(path: &Path, document: &ProjectDocument) {
    let mut database = ProjectDatabase::create(path).unwrap();
    database.replace(&document.snapshot()).unwrap();
}

#[test]
fn database_creates_reopens_and_round_trips_a_project_document() {
    let artifact = TempProject::new("round-trip");
    let document = project_document();

    create_persisted(artifact.path(), &document);
    let reopened = ProjectDatabase::open_read_only(artifact.path()).unwrap();
    assert_eq!(reopened.load().unwrap().snapshot(), document.snapshot());
    drop(reopened);

    let duplicate = ProjectDatabase::create(artifact.path()).unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::Conflict);
}

#[test]
fn schema_identity_and_semantic_rows_are_explicit_and_deterministic() {
    let first = TempProject::new("deterministic-a");
    let second = TempProject::new("deterministic-b");
    let document = project_document();
    create_persisted(first.path(), &document);
    create_persisted(second.path(), &document);

    let first = database_evidence(first.path());
    let second = database_evidence(second.path());
    assert_eq!(first, second);
    assert_eq!(first.application_id, i64::from(PROJECT_APPLICATION_ID));
    assert_eq!(first.schema_revision, i64::from(PROJECT_SCHEMA_REVISION));
    assert_eq!(
        first.schema,
        vec![
            ("table".to_owned(), "audio_component".to_owned()),
            ("table".to_owned(), "extension_records".to_owned()),
            ("table".to_owned(), "graph_components".to_owned()),
            ("table".to_owned(), "project_metadata".to_owned()),
            ("table".to_owned(), "settings_component".to_owned()),
            ("table".to_owned(), "timeline_component".to_owned()),
        ]
    );
    assert_eq!(first.metadata.0, PROJECT_FORMAT);
    assert_eq!(first.metadata.1, PROJECT_FORMAT_VERSION);
    assert_eq!(
        first.metadata.2,
        i64::from(STABLE_PRIMITIVE_SCHEMA_REVISION)
    );
    assert_eq!(first.metadata.3, PROJECT.to_bytes());
    assert_eq!(first.metadata.4, document.revision().to_string());
    assert_eq!(first.metadata.5, ROOT.to_bytes());
    assert_eq!(first.metadata.6.len(), 32);
    assert_eq!(first.timeline.0, i64::from(TIMELINE_STATE_FORMAT_REVISION));
    assert_eq!(first.timeline.1 as usize, first.timeline.3.len());
    assert_eq!(first.timeline.2.len(), 32);
    assert_eq!(
        first.settings.0,
        i64::from(PROJECT_SETTINGS_FORMAT_REVISION)
    );
    assert_eq!(first.settings.1 as usize, first.settings.3.len());
    assert_eq!(first.settings.2.len(), 32);
    assert_eq!(first.audio.0, i64::from(CLIP_MIX_FORMAT_REVISION));
    assert_eq!(first.audio.1 as usize, first.audio.3.len());
    assert_eq!(first.audio.2.len(), 32);
    assert_eq!(first.extensions.len(), 1);
    assert_eq!(first.extensions[0].extension_id, "example.future-extension");
    assert_eq!(first.extensions[0].record_id, "opaque-record");
    assert_eq!(first.extensions[0].metadata_format_revision, 1);
    assert_eq!(
        first.extensions[0].metadata_byte_length as usize,
        first.extensions[0].metadata.len()
    );
    assert_eq!(first.extensions[0].metadata_digest.len(), 32);
    assert_eq!(
        first.extensions[0].payload_byte_length as usize,
        first.extensions[0].payload.len()
    );
    assert_eq!(first.extensions[0].payload_digest.len(), 32);
    assert_eq!(first.extensions[0].payload, vec![0, 1, 2, 0xfe, 0xff]);
    assert_eq!(first.graphs.len(), 2);
    for graph in &first.graphs {
        assert_eq!(
            graph.format_revision,
            i64::from(GRAPH_DOCUMENT_FORMAT_REVISION)
        );
        assert_eq!(graph.byte_length as usize, graph.document.len());
        assert_eq!(graph.digest.len(), 32);
    }
    assert!(first.graphs[0].graph_id < first.graphs[1].graph_id);
    let standalone = first
        .graphs
        .iter()
        .find(|graph| graph.graph_id == STANDALONE.to_bytes())
        .unwrap();
    assert_eq!(standalone.kind, "standalone");
    assert_eq!(standalone.name.as_deref(), Some("reusable analysis"));
}

#[test]
fn reload_preserves_media_evidence_graph_edits_and_revision_conflicts() {
    let document = project_document();
    let expected = document.snapshot();
    let mut database = ProjectDatabase::memory().unwrap();
    database.replace(&expected).unwrap();
    let mut loaded = database.load().unwrap();

    assert_eq!(loaded.snapshot(), expected);
    let controls = loaded.clip_mix_state().controls(ROOT_CLIP).unwrap();
    assert_eq!(controls.gain().to_bits(), 0x3f40_0001);
    assert_eq!(controls.pan().to_bits(), 0xbe80_0001);
    assert_eq!(controls.fade_in_frames(), 4_801);
    assert_eq!(controls.fade_out_frames(), 9_601);
    assert_eq!(
        controls.channel_map()[0].destination(),
        ChannelPosition::FrontRight
    );
    assert_eq!(clip_name(&loaded), "durable direct graph edit");
    let media = loaded
        .editorial_project()
        .media_reference(CAMERA_B)
        .unwrap();
    assert_eq!(
        media.relink_state().status(),
        RelinkStatus::FingerprintMismatch
    );
    assert_eq!(
        media.relink_state().expected_fingerprint(),
        Some("fingerprint-b")
    );
    assert_eq!(
        media.relink_state().observed_fingerprint(),
        Some("wrong-fingerprint")
    );
    assert_eq!(
        media.relink_state().rejected_target(),
        Some("urn:camera:b:replacement")
    );

    let stale = loaded.edit(0, |_| Ok(())).unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    let revision = loaded.revision();
    let published = loaded
        .edit(revision, |draft| {
            draft
                .graph_mut(STANDALONE)?
                .as_standalone_mut()
                .unwrap()
                .set_name("renamed after reload")
        })
        .unwrap();
    assert_eq!(published.revision(), revision + 1);
}

#[test]
fn failed_replacement_rolls_back_and_read_only_connections_cannot_write() {
    let artifact = TempProject::new("rollback");
    let document = project_document();
    let baseline = document.snapshot();
    let mut database = ProjectDatabase::create(artifact.path()).unwrap();
    database.replace(&baseline).unwrap();

    let mut oversized = document.clone();
    let revision = oversized.revision();
    oversized
        .edit(revision, |draft| {
            draft.insert_graph(ProjectGraph::Standalone(
                StandaloneProjectGraph::new(
                    "x".repeat(16 * 1024 + 1),
                    EditableGraph::<CompiledTimelineGraphValue>::new(GraphId::from_raw(900)),
                )
                .unwrap(),
            ))
        })
        .unwrap();
    let failure = database.replace(&oversized.snapshot()).unwrap_err();
    assert_eq!(failure.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(database.load().unwrap().snapshot(), baseline);
    drop(database);

    let mut read_only = ProjectDatabase::open_read_only(artifact.path()).unwrap();
    let denied = read_only.replace(&baseline).unwrap_err();
    assert_eq!(denied.category(), ErrorCategory::PermissionDenied);
    assert_eq!(read_only.load().unwrap().snapshot(), baseline);
}

#[test]
fn unsupported_corrupt_and_extra_database_state_is_rejected() {
    let document = project_document();
    let cases = [
        (
            "wrong-application",
            "PRAGMA application_id = 1",
            ErrorCategory::Unsupported,
            true,
        ),
        (
            "future-schema",
            "PRAGMA user_version = 5",
            ErrorCategory::Unsupported,
            true,
        ),
        (
            "future-format",
            "UPDATE project_metadata SET format_version = '2.0.0'",
            ErrorCategory::Unsupported,
            false,
        ),
        (
            "timeline-bytes",
            "UPDATE timeline_component SET document = zeroblob(byte_length)",
            ErrorCategory::CorruptData,
            false,
        ),
        (
            "graph-bytes",
            "UPDATE graph_components SET document = zeroblob(byte_length) WHERE graph_id = (SELECT graph_id FROM graph_components ORDER BY graph_id LIMIT 1)",
            ErrorCategory::CorruptData,
            false,
        ),
        (
            "settings-bytes",
            "UPDATE settings_component SET document = zeroblob(byte_length)",
            ErrorCategory::CorruptData,
            false,
        ),
        (
            "audio-bytes",
            "UPDATE audio_component SET document = zeroblob(byte_length)",
            ErrorCategory::CorruptData,
            false,
        ),
        (
            "extension-metadata-bytes",
            "UPDATE extension_records SET metadata = zeroblob(metadata_byte_length)",
            ErrorCategory::CorruptData,
            false,
        ),
        (
            "extension-payload-bytes",
            "UPDATE extension_records SET payload = zeroblob(payload_byte_length)",
            ErrorCategory::CorruptData,
            false,
        ),
        (
            "manifest",
            "UPDATE project_metadata SET manifest_sha256 = zeroblob(32)",
            ErrorCategory::CorruptData,
            false,
        ),
        (
            "missing-timeline",
            "DELETE FROM timeline_component",
            ErrorCategory::CorruptData,
            false,
        ),
        (
            "missing-settings",
            "DELETE FROM settings_component",
            ErrorCategory::CorruptData,
            false,
        ),
        (
            "missing-audio",
            "DELETE FROM audio_component",
            ErrorCategory::CorruptData,
            false,
        ),
        (
            "extra-view",
            "CREATE VIEW unexpected_project_view AS SELECT singleton FROM project_metadata",
            ErrorCategory::CorruptData,
            true,
        ),
    ];

    for (label, mutation, category, rejected_on_open) in cases {
        let artifact = TempProject::new(label);
        create_persisted(artifact.path(), &document);
        let connection = Connection::open(artifact.path()).unwrap();
        connection.execute_batch(mutation).unwrap();
        drop(connection);

        let error = if rejected_on_open {
            ProjectDatabase::open_read_only(artifact.path()).unwrap_err()
        } else {
            ProjectDatabase::open_read_only(artifact.path())
                .unwrap()
                .load()
                .unwrap_err()
        };
        assert_eq!(error.category(), category, "case {label}");
    }
}
