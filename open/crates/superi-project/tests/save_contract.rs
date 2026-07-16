use std::error::Error as StdError;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

use rusqlite::{params, Connection};
use superi_core::error::{Error, ErrorCategory, Recoverability};
use superi_core::ids::{ClipId, GraphId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_graph::mutate::{EditableGraph, GraphMutation, GraphTransaction, TypedParameterValue};
use superi_graph::value::GraphValue;
use superi_project::document::{
    ProjectDocument, ProjectGraph, ProjectSnapshot, StandaloneProjectGraph,
};
use superi_project::media::{PortableRelativePath, ReferencedMediaPath};
use superi_project::{
    ProjectDatabase, ProjectFileSession, ProjectSaveCommand, ProjectSaveKind,
    PROJECT_SCHEMA_REVISION,
};
use superi_timeline::compile::{
    CompiledTimelineGraphValue, TimelineGraphOrigin, TimelineGraphValue,
};
use superi_timeline::ids::MulticamAngleId;
use superi_timeline::media::RelinkDecision;
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};
use superi_timeline::multicam::{
    MulticamAngle, MulticamAudioPolicy, MulticamClip, MulticamSource, MulticamSyncMethod,
};
use superi_timeline::retime::{ClipTimeMap, PlaybackRate};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

const PROJECT: ProjectId = ProjectId::from_raw(0x5a00);
const SOURCE: TimelineId = TimelineId::from_raw(0x5a01);
const ROOT: TimelineId = TimelineId::from_raw(0x5a02);
const SOURCE_TRACK_A: TrackId = TrackId::from_raw(0x5a03);
const SOURCE_TRACK_B: TrackId = TrackId::from_raw(0x5a04);
const ROOT_TRACK: TrackId = TrackId::from_raw(0x5a05);
const CAMERA_A: MediaId = MediaId::from_raw(0x5a06);
const CAMERA_B: MediaId = MediaId::from_raw(0x5a07);
const SOURCE_CLIP_A: ClipId = ClipId::from_raw(0x5a08);
const SOURCE_CLIP_B: ClipId = ClipId::from_raw(0x5a09);
const ROOT_CLIP: ClipId = ClipId::from_raw(0x5a0a);
const ANGLE_A: MulticamAngleId = MulticamAngleId::from_raw(0x5a0b);
const ANGLE_B: MulticamAngleId = MulticamAngleId::from_raw(0x5a0c);
const STANDALONE: GraphId = GraphId::from_raw(0x5a0d);
const LEGACY_PROJECT_METADATA_SCHEMA: &str = "CREATE TABLE project_metadata (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format TEXT NOT NULL CHECK (format = 'superi.project'), format_version TEXT NOT NULL, primitive_schema_revision INTEGER NOT NULL CHECK (primitive_schema_revision > 0), project_id BLOB NOT NULL CHECK (length(project_id) = 16), document_revision TEXT NOT NULL, root_timeline_id BLOB NOT NULL CHECK (length(root_timeline_id) = 16)) STRICT";
const LEGACY_TIMELINE_COMPONENT_SCHEMA: &str = "CREATE TABLE timeline_component (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format_revision INTEGER NOT NULL CHECK (format_revision >= 0), document BLOB NOT NULL CHECK (length(document) <= 67108864)) STRICT";
const LEGACY_GRAPH_COMPONENTS_SCHEMA: &str = "CREATE TABLE graph_components (graph_id BLOB PRIMARY KEY CHECK (length(graph_id) = 16), graph_kind TEXT NOT NULL CHECK (graph_kind IN ('timeline', 'standalone')), root_timeline_id BLOB CHECK (root_timeline_id IS NULL OR length(root_timeline_id) = 16), name TEXT, graph_revision TEXT NOT NULL, format_revision INTEGER NOT NULL CHECK (format_revision >= 0), document BLOB NOT NULL CHECK (length(document) <= 67108864), CHECK ((graph_kind = 'timeline' AND root_timeline_id IS NOT NULL AND name IS NULL) OR (graph_kind = 'standalone' AND root_timeline_id IS NULL AND name IS NOT NULL AND length(name) > 0))) STRICT, WITHOUT ROWID";

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-save-{label}-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self {
            path: fs::canonicalize(path).unwrap(),
        }
    }

    fn project(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
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

fn rich_document() -> ProjectDocument {
    let relative = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("Media/day-01/camera-a.mov").unwrap(),
    );
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
    let mut editorial = EditorialProject::new(
        PROJECT,
        "durable save project",
        [
            LinkedMediaReference::with_fingerprint(
                CAMERA_A,
                "camera a",
                relative.to_target(),
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
    editorial
        .edit(0, |draft| {
            assert_eq!(
                draft
                    .media_reference_mut(CAMERA_B)?
                    .consider_relink("urn:camera:b:replacement", "wrong-fingerprint")?,
                RelinkDecision::RejectedFingerprintMismatch
            );
            draft
                .timeline_mut(SOURCE)?
                .set_multicam_source(MulticamSource::new(
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

    let mut document = ProjectDocument::new(editorial, ROOT).unwrap();
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
            draft.insert_graph(ProjectGraph::Standalone(standalone))
        })
        .unwrap();
    document
}

fn rename_project(document: &mut ProjectDocument, name: &str) -> ProjectSnapshot {
    let revision = document.revision();
    document
        .edit(revision, |draft| {
            let editorial_revision = draft.editorial_project().revision();
            draft
                .editorial_project_mut()
                .edit(editorial_revision, |editorial| {
                    editorial.set_name(name);
                    Ok(())
                })?;
            draft.recompile_timeline(ROOT)
        })
        .unwrap()
}

fn persist(path: &Path, snapshot: &ProjectSnapshot) {
    let mut database = ProjectDatabase::create(path).unwrap();
    database.replace(snapshot).unwrap();
}

type LegacyMetadata = (i64, Vec<u8>, String, Vec<u8>);
type LegacyTimeline = (i64, Vec<u8>);
type LegacyGraph = (
    Vec<u8>,
    String,
    Option<Vec<u8>>,
    Option<String>,
    String,
    i64,
    Vec<u8>,
);

fn downgrade_to_schema_zero(path: &Path) {
    let mut connection = Connection::open(path).unwrap();
    let transaction = connection.transaction().unwrap();
    let metadata: LegacyMetadata = transaction
        .query_row(
            "SELECT primitive_schema_revision, project_id, document_revision, root_timeline_id FROM project_metadata",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let timeline: LegacyTimeline = transaction
        .query_row(
            "SELECT format_revision, document FROM timeline_component",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let graphs = {
        let mut statement = transaction
            .prepare(
                "SELECT graph_id, graph_kind, root_timeline_id, name, graph_revision, format_revision, document FROM graph_components ORDER BY graph_id",
            )
            .unwrap();
        statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<LegacyGraph>>>()
            .unwrap()
    };

    transaction
        .execute_batch(&format!(
            "DROP TABLE graph_components;DROP TABLE timeline_component;DROP TABLE project_metadata;{LEGACY_PROJECT_METADATA_SCHEMA};{LEGACY_TIMELINE_COMPONENT_SCHEMA};{LEGACY_GRAPH_COMPONENTS_SCHEMA};PRAGMA user_version = 0;"
        ))
        .unwrap();
    transaction
        .execute(
            "INSERT INTO project_metadata (singleton, format, format_version, primitive_schema_revision, project_id, document_revision, root_timeline_id) VALUES (1, 'superi.project', '0.9.0', ?1, ?2, ?3, ?4)",
            params![metadata.0, metadata.1, metadata.2, metadata.3],
        )
        .unwrap();
    transaction
        .execute(
            "INSERT INTO timeline_component (singleton, format_revision, document) VALUES (1, ?1, ?2)",
            params![timeline.0, timeline.1],
        )
        .unwrap();
    for graph in graphs {
        transaction
            .execute(
                "INSERT INTO graph_components (graph_id, graph_kind, root_timeline_id, name, graph_revision, format_revision, document) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![graph.0, graph.1, graph.2, graph.3, graph.4, graph.5, graph.6],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
}

fn load(path: &Path) -> ProjectSnapshot {
    ProjectDatabase::open_read_only(path)
        .unwrap()
        .load()
        .unwrap()
        .snapshot()
}

fn assert_no_sidecars(path: &Path) {
    for suffix in ["-journal", "-wal", "-shm"] {
        assert!(!PathBuf::from(format!("{}{suffix}", path.display())).exists());
    }
}

fn assert_no_candidates(root: &Path) {
    let candidates = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".superi-save-candidate-"))
        .collect::<Vec<_>>();
    assert!(
        candidates.is_empty(),
        "owned candidates remain: {candidates:?}"
    );
}

fn assert_prepare_conflict(error: &Error, operation: &str, destination: &Path) {
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    let context = error
        .contexts()
        .iter()
        .find(|context| {
            context.component() == "superi-project.save"
                && context.operation() == operation
                && context.field("phase") == Some("prepare_command")
        })
        .expect("public command context");
    assert_eq!(context.field("phase"), Some("prepare_command"));
    assert_eq!(
        context.field("destination"),
        Some(destination.display().to_string().as_str())
    );
    assert!(context.field("active_path").is_some());
    assert!(context.field("revision").is_some());
}

#[test]
fn all_save_commands_publish_exact_state_and_apply_only_explicit_adoption() {
    let root = TempRoot::new("commands");
    let active = root.project("active.superi");
    let save_as = root.project("adopted.superi");
    let copy = root.project("copy.superi");
    let backup = root.project("backup.superi");
    let mut document = rich_document();
    let original = document.snapshot();
    persist(&active, &original);
    let saved = rename_project(&mut document, "saved revision");

    let mut session = ProjectFileSession::new(active.clone()).unwrap();
    assert_eq!(session.active_path(), active);
    let outcome = session
        .execute(ProjectSaveCommand::Save {
            snapshot: saved.clone(),
        })
        .unwrap();
    assert_eq!(outcome.kind(), ProjectSaveKind::Save);
    assert_eq!(outcome.destination(), active);
    assert_eq!(outcome.active_path(), active);
    assert_eq!(outcome.revision(), saved.revision());
    assert!(!outcome.adopted());
    assert!(outcome.durable());
    assert_eq!(load(&active), saved);

    let outcome = session
        .execute(ProjectSaveCommand::SaveAs {
            destination: save_as.clone(),
            snapshot: saved.clone(),
        })
        .unwrap();
    assert_eq!(outcome.kind(), ProjectSaveKind::SaveAs);
    assert_eq!(outcome.destination(), save_as);
    assert_eq!(outcome.active_path(), save_as);
    assert!(outcome.adopted());
    assert_eq!(session.active_path(), save_as);
    assert_eq!(load(&save_as), saved);

    let outcome = session
        .execute(ProjectSaveCommand::Copy {
            destination: copy.clone(),
            snapshot: saved.clone(),
        })
        .unwrap();
    assert_eq!(outcome.kind(), ProjectSaveKind::Copy);
    assert_eq!(outcome.active_path(), save_as);
    assert!(!outcome.adopted());
    assert_eq!(session.active_path(), save_as);
    assert_eq!(load(&copy), saved);

    let unsaved = rename_project(&mut document, "unsaved revision");
    assert_ne!(unsaved, saved);
    let outcome = session
        .execute(ProjectSaveCommand::Backup {
            destination: backup.clone(),
        })
        .unwrap();
    assert_eq!(outcome.kind(), ProjectSaveKind::Backup);
    assert_eq!(outcome.revision(), saved.revision());
    assert_eq!(outcome.active_path(), save_as);
    assert!(!outcome.adopted());
    assert_eq!(load(&backup), saved);
    assert_ne!(load(&backup), unsaved);

    let authored_target = saved
        .editorial_project()
        .media_reference(CAMERA_A)
        .unwrap()
        .target()
        .to_owned();
    for destination in [&active, &save_as, &copy, &backup] {
        let loaded = load(destination);
        assert_eq!(
            loaded
                .editorial_project()
                .media_reference(CAMERA_A)
                .unwrap()
                .target(),
            authored_target
        );
        assert_eq!(
            loaded
                .media_path(CAMERA_A)
                .unwrap()
                .resolve(destination)
                .unwrap(),
            destination
                .parent()
                .unwrap()
                .join("Media/day-01/camera-a.mov")
        );
        assert_no_sidecars(destination);
    }
    assert_no_candidates(&root.path);
}

#[test]
fn save_may_create_or_replace_only_a_valid_current_project() {
    let root = TempRoot::new("replace");
    let active = root.project("active.superi");
    let mut document = rich_document();
    let first = document.snapshot();
    let second = rename_project(&mut document, "replacement revision");
    let mut session = ProjectFileSession::new(active.clone()).unwrap();

    session
        .execute(ProjectSaveCommand::Save {
            snapshot: first.clone(),
        })
        .unwrap();
    assert_eq!(load(&active), first);
    session
        .execute(ProjectSaveCommand::Save {
            snapshot: second.clone(),
        })
        .unwrap();
    assert_eq!(load(&active), second);

    for (label, mutate, expected_category) in [
        (
            "future",
            "PRAGMA user_version = 2",
            ErrorCategory::Unsupported,
        ),
        (
            "wrong-application",
            "PRAGMA application_id = 1",
            ErrorCategory::Unsupported,
        ),
        (
            "extended",
            "CREATE TABLE unexpected_extension (value TEXT) STRICT",
            ErrorCategory::CorruptData,
        ),
    ] {
        let path = root.project(&format!("{label}.superi"));
        persist(&path, &first);
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(mutate).unwrap();
        drop(connection);
        let before = fs::read(&path).unwrap();
        let mut session = ProjectFileSession::new(path.clone()).unwrap();
        let error = session
            .execute(ProjectSaveCommand::Save {
                snapshot: second.clone(),
            })
            .unwrap_err();
        assert_eq!(error.category(), expected_category, "case {label}");
        assert_eq!(
            error.recoverability(),
            Recoverability::UserCorrectable,
            "case {label}"
        );
        assert!(error.contexts().iter().any(|context| {
            context.component() == "superi-project.save"
                && context.operation() == "save"
                && context.field("phase") == Some("prepare_command")
        }));
        assert_eq!(fs::read(&path).unwrap(), before, "case {label}");
    }

    let corrupt = root.project("corrupt.superi");
    fs::write(&corrupt, b"not a project database").unwrap();
    let before = fs::read(&corrupt).unwrap();
    let mut session = ProjectFileSession::new(corrupt.clone()).unwrap();
    let error = session
        .execute(ProjectSaveCommand::Save { snapshot: second })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert_eq!(fs::read(corrupt).unwrap(), before);
    assert_no_candidates(&root.path);
}

#[test]
fn save_preserves_existing_destination_permissions() {
    let root = TempRoot::new("save-permissions");
    let active = root.project("active.superi");
    let mut document = rich_document();
    let original = document.snapshot();
    let replacement = rename_project(&mut document, "permission preserving save");
    persist(&active, &original);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&active, fs::Permissions::from_mode(0o640)).unwrap();
    }
    let before = fs::metadata(&active).unwrap().permissions();
    ProjectFileSession::new(active.clone())
        .unwrap()
        .execute(ProjectSaveCommand::Save {
            snapshot: replacement.clone(),
        })
        .unwrap();
    let after = fs::metadata(&active).unwrap().permissions();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(after.mode(), before.mode());
    }
    #[cfg(not(unix))]
    assert_eq!(after.readonly(), before.readonly());
    assert_eq!(load(&active), replacement);
}

#[test]
fn no_clobber_commands_reject_entries_aliases_symlinks_and_missing_parents() {
    let root = TempRoot::new("no-clobber");
    let active = root.project("active.superi");
    let occupied = root.project("occupied.superi");
    let snapshot = rich_document().snapshot();
    persist(&active, &snapshot);
    fs::write(&occupied, b"preserve this entry").unwrap();
    let occupied_before = fs::read(&occupied).unwrap();
    let mut session = ProjectFileSession::new(active.clone()).unwrap();

    for (operation, command) in [
        (
            "save_as",
            ProjectSaveCommand::SaveAs {
                destination: occupied.clone(),
                snapshot: snapshot.clone(),
            },
        ),
        (
            "copy",
            ProjectSaveCommand::Copy {
                destination: occupied.clone(),
                snapshot: snapshot.clone(),
            },
        ),
        (
            "backup",
            ProjectSaveCommand::Backup {
                destination: occupied.clone(),
            },
        ),
    ] {
        let error = session.execute(command).unwrap_err();
        assert_prepare_conflict(&error, operation, &occupied);
        assert_eq!(fs::read(&occupied).unwrap(), occupied_before);
        assert_eq!(session.active_path(), active);
    }

    for command in [
        ProjectSaveCommand::SaveAs {
            destination: active.clone(),
            snapshot: snapshot.clone(),
        },
        ProjectSaveCommand::Copy {
            destination: active.clone(),
            snapshot: snapshot.clone(),
        },
        ProjectSaveCommand::Backup {
            destination: active.clone(),
        },
    ] {
        assert_eq!(
            session.execute(command).unwrap_err().category(),
            ErrorCategory::Conflict
        );
    }

    let missing_parent = root.project("missing").join("copy.superi");
    let error = session
        .execute(ProjectSaveCommand::Copy {
            destination: missing_parent,
            snapshot: snapshot.clone(),
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(StdError::source(&error).is_some());
    assert!(error.contexts().iter().any(|context| {
        context.component() == "superi-project.save"
            && context.operation() == "copy"
            && context.field("phase") == Some("prepare_command")
    }));
    assert_eq!(
        ProjectFileSession::new(PathBuf::from("relative.superi"))
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        session
            .execute(ProjectSaveCommand::Copy {
                destination: PathBuf::from("relative-copy.superi"),
                snapshot: snapshot.clone(),
            })
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let hard_link = root.project("hard-link.superi");
        fs::hard_link(&active, &hard_link).unwrap();
        assert_eq!(
            session
                .execute(ProjectSaveCommand::Copy {
                    destination: hard_link,
                    snapshot: snapshot.clone(),
                })
                .unwrap_err()
                .category(),
            ErrorCategory::Conflict
        );

        let symlink_path = root.project("symlink.superi");
        symlink(&active, &symlink_path).unwrap();
        assert_eq!(
            session
                .execute(ProjectSaveCommand::SaveAs {
                    destination: symlink_path,
                    snapshot: snapshot.clone(),
                })
                .unwrap_err()
                .category(),
            ErrorCategory::Conflict
        );
    }
    let directory = root.project("directory.superi");
    fs::create_dir(&directory).unwrap();
    let error = session
        .execute(ProjectSaveCommand::Copy {
            destination: directory.clone(),
            snapshot,
        })
        .unwrap_err();
    assert_prepare_conflict(&error, "copy", &directory);
    assert_no_candidates(&root.path);
}

#[test]
fn exact_legacy_state_is_preserved_and_a_migrated_snapshot_saves_as_current() {
    let root = TempRoot::new("migration-save");
    let legacy = root.project("legacy.superi");
    let mut document = rich_document();
    let original = document.snapshot();
    let replacement = rename_project(&mut document, "replacement must not publish");
    persist(&legacy, &original);
    downgrade_to_schema_zero(&legacy);
    let legacy_before = fs::read(&legacy).unwrap();

    let mut legacy_session = ProjectFileSession::new(legacy.clone()).unwrap();
    let error = legacy_session
        .execute(ProjectSaveCommand::Save {
            snapshot: replacement,
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(error.contexts().iter().any(|context| {
        context.component() == "superi-project.save"
            && context.operation() == "save"
            && context.field("phase") == Some("prepare_command")
    }));
    assert_eq!(fs::read(&legacy).unwrap(), legacy_before);

    let source = root.project("migration-source.superi");
    let destination = root.project("migrated-save-as.superi");
    persist(&source, &original);
    downgrade_to_schema_zero(&source);
    let database = ProjectDatabase::open(&source).unwrap();
    assert!(database.was_migrated());
    let migrated = database.load().unwrap().snapshot();
    assert_eq!(migrated, original);
    drop(database);
    let source_before_save_as = fs::read(&source).unwrap();

    let mut session = ProjectFileSession::new(source.clone()).unwrap();
    let outcome = session
        .execute(ProjectSaveCommand::SaveAs {
            destination: destination.clone(),
            snapshot: migrated.clone(),
        })
        .unwrap();
    assert_eq!(outcome.kind(), ProjectSaveKind::SaveAs);
    assert_eq!(outcome.destination(), destination);
    assert_eq!(outcome.active_path(), destination);
    assert_eq!(outcome.revision(), migrated.revision());
    assert!(outcome.adopted());
    assert!(outcome.durable());
    assert_eq!(fs::read(&source).unwrap(), source_before_save_as);
    assert_eq!(load(&source), migrated);
    assert_eq!(load(&destination), migrated);
    assert_no_sidecars(&destination);
    assert_no_candidates(&root.path);
}

#[test]
fn concurrent_no_clobber_copies_have_exactly_one_complete_winner() {
    let root = TempRoot::new("race");
    let active = root.project("active.superi");
    let destination = root.project("winner.superi");
    let mut first_document = rich_document();
    let mut second_document = rich_document();
    let baseline = first_document.snapshot();
    let first = rename_project(&mut first_document, "first racing snapshot");
    let second = rename_project(&mut second_document, "second racing snapshot");
    assert_ne!(first, second);
    persist(&active, &baseline);

    let barrier = Arc::new(Barrier::new(3));
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for snapshot in [first.clone(), second.clone()] {
            let barrier = Arc::clone(&barrier);
            let active = active.clone();
            let destination = destination.clone();
            handles.push(scope.spawn(move || {
                let mut session = ProjectFileSession::new(active).unwrap();
                barrier.wait();
                let result = session.execute(ProjectSaveCommand::Copy {
                    destination,
                    snapshot: snapshot.clone(),
                });
                (snapshot, result)
            }));
        }
        barrier.wait();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });

    assert_eq!(
        results.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    assert_eq!(
        results.iter().filter(|(_, result)| result.is_err()).count(),
        1
    );
    let winner = results
        .iter()
        .find_map(|(snapshot, result)| result.is_ok().then_some(snapshot))
        .unwrap();
    let loser = results
        .iter()
        .find_map(|(_, result)| result.as_ref().err())
        .unwrap();
    assert_eq!(loser.category(), ErrorCategory::Conflict);
    assert_eq!(loser.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(load(&destination), *winner);
    assert_eq!(load(&active), baseline);
    assert_no_sidecars(&destination);
    assert_no_candidates(&root.path);
}

#[test]
fn outcomes_report_current_schema_without_advancing_document_revision() {
    let root = TempRoot::new("outcome");
    let active = root.project("active.superi");
    let document = rich_document();
    let snapshot = document.snapshot();
    let revision = snapshot.revision();
    let mut session = ProjectFileSession::new(active.clone()).unwrap();
    let outcome = session
        .execute(ProjectSaveCommand::Save {
            snapshot: snapshot.clone(),
        })
        .unwrap();

    assert_eq!(outcome.revision(), revision);
    assert_eq!(snapshot.revision(), revision);
    let database = ProjectDatabase::open_read_only(active).unwrap();
    assert_eq!(database.source_schema_revision(), PROJECT_SCHEMA_REVISION);
    assert_eq!(database.load().unwrap().snapshot(), snapshot);
}
