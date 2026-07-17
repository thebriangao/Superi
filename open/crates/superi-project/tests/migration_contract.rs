use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{params, Connection, OpenFlags};
use serde_json::{json, Value};
use superi_core::error::ErrorCategory;
use superi_core::ids::{GraphId, ProjectId, TimelineId};
use superi_core::time::{RationalTime, Timebase};
use superi_graph::mutate::EditableGraph;
use superi_project::document::{ProjectDocument, ProjectGraph, StandaloneProjectGraph};
use superi_project::{
    ProjectDatabase, ProjectDestinationCollision, ProjectSaveCommand, PROJECT_APPLICATION_ID,
    PROJECT_FORMAT_VERSION, PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION, PROJECT_SCHEMA_REVISION,
};
use superi_timeline::compile::CompiledTimelineGraphValue;
use superi_timeline::model::{EditorialProject, Timeline};

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

const PROJECT: ProjectId = ProjectId::from_raw(0x900);
const ROOT: TimelineId = TimelineId::from_raw(0x901);
const STANDALONE: GraphId = GraphId::from_raw(0x902);
const LEGACY_FORMAT_VERSION: &str = "0.9.0";
const LEGACY_PROJECT_METADATA_SCHEMA: &str = "CREATE TABLE project_metadata (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format TEXT NOT NULL CHECK (format = 'superi.project'), format_version TEXT NOT NULL, primitive_schema_revision INTEGER NOT NULL CHECK (primitive_schema_revision > 0), project_id BLOB NOT NULL CHECK (length(project_id) = 16), document_revision TEXT NOT NULL, root_timeline_id BLOB NOT NULL CHECK (length(root_timeline_id) = 16)) STRICT";
const LEGACY_TIMELINE_COMPONENT_SCHEMA: &str = "CREATE TABLE timeline_component (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format_revision INTEGER NOT NULL CHECK (format_revision >= 0), document BLOB NOT NULL CHECK (length(document) <= 67108864)) STRICT";
const LEGACY_GRAPH_COMPONENTS_SCHEMA: &str = "CREATE TABLE graph_components (graph_id BLOB PRIMARY KEY CHECK (length(graph_id) = 16), graph_kind TEXT NOT NULL CHECK (graph_kind IN ('timeline', 'standalone')), root_timeline_id BLOB CHECK (root_timeline_id IS NULL OR length(root_timeline_id) = 16), name TEXT, graph_revision TEXT NOT NULL, format_revision INTEGER NOT NULL CHECK (format_revision >= 0), document BLOB NOT NULL CHECK (length(document) <= 67108864), CHECK ((graph_kind = 'timeline' AND root_timeline_id IS NOT NULL AND name IS NULL) OR (graph_kind = 'standalone' AND root_timeline_id IS NULL AND name IS NOT NULL AND length(name) > 0))) STRICT, WITHOUT ROWID";

struct TempProject {
    path: PathBuf,
}

impl TempProject {
    fn new(label: &str) -> Self {
        Self {
            path: std::env::temp_dir().join(format!(
                "superi-migration-{label}-{}-{}.superi",
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

fn project_document() -> ProjectDocument {
    let timebase = Timebase::integer(24).unwrap();
    let timeline = Timeline::new(
        ROOT,
        "legacy timeline",
        timebase,
        RationalTime::zero(timebase),
        vec![],
    );
    let project = EditorialProject::new(PROJECT, "legacy project", [], [timeline]).unwrap();
    let mut document = ProjectDocument::new(project, ROOT).unwrap();
    document
        .edit(0, |draft| {
            draft.insert_graph(ProjectGraph::Standalone(
                StandaloneProjectGraph::new(
                    "editable legacy graph",
                    EditableGraph::<CompiledTimelineGraphValue>::new(STANDALONE),
                )
                .unwrap(),
            ))
        })
        .unwrap();
    document
}

fn create_current(path: &Path, document: &ProjectDocument) {
    let mut database = ProjectDatabase::create(path).unwrap();
    database.replace(&document.snapshot()).unwrap();
}

type MetadataRow = (i64, Vec<u8>, String, Vec<u8>);
type TimelineRow = (i64, Vec<u8>);
type GraphRow = (
    Vec<u8>,
    String,
    Option<Vec<u8>>,
    Option<String>,
    String,
    i64,
    Vec<u8>,
);

fn legacy_component(format: &str, payload_field: &str, current: &[u8]) -> Vec<u8> {
    let current: Value = serde_json::from_slice(current).unwrap();
    serde_json::to_vec(&json!({
        "format": format,
        "format_revision": 0,
        payload_field: current["payload"].clone(),
    }))
    .unwrap()
}

fn downgrade_to_schema_zero(path: &Path, legacy_components: bool) {
    let mut connection = Connection::open(path).unwrap();
    let transaction = connection.transaction().unwrap();
    let metadata: MetadataRow = transaction
        .query_row(
            "SELECT primitive_schema_revision, project_id, document_revision, root_timeline_id FROM project_metadata",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let timeline: TimelineRow = transaction
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
            .collect::<rusqlite::Result<Vec<GraphRow>>>()
            .unwrap()
    };

    transaction
        .execute_batch(&format!(
            "DROP TABLE audio_component;DROP TABLE graph_components;DROP TABLE settings_component;DROP TABLE timeline_component;DROP TABLE project_metadata;{LEGACY_PROJECT_METADATA_SCHEMA};{LEGACY_TIMELINE_COMPONENT_SCHEMA};{LEGACY_GRAPH_COMPONENTS_SCHEMA};PRAGMA user_version = 0;"
        ))
        .unwrap();
    transaction
        .execute(
            "INSERT INTO project_metadata (singleton, format, format_version, primitive_schema_revision, project_id, document_revision, root_timeline_id) VALUES (1, 'superi.project', ?1, ?2, ?3, ?4, ?5)",
            params![LEGACY_FORMAT_VERSION, metadata.0, metadata.1, metadata.2, metadata.3],
        )
        .unwrap();
    let timeline_document = if legacy_components {
        legacy_component("superi.timeline", "timeline_state", &timeline.1)
    } else {
        timeline.1
    };
    transaction
        .execute(
            "INSERT INTO timeline_component (singleton, format_revision, document) VALUES (1, ?1, ?2)",
            params![if legacy_components { 0 } else { timeline.0 }, timeline_document],
        )
        .unwrap();
    for graph in graphs {
        let document = if legacy_components {
            legacy_component("superi.graph", "graph", &graph.6)
        } else {
            graph.6
        };
        transaction
            .execute(
                "INSERT INTO graph_components (graph_id, graph_kind, root_timeline_id, name, graph_revision, format_revision, document) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    graph.0,
                    graph.1,
                    graph.2,
                    graph.3,
                    graph.4,
                    if legacy_components { 0 } else { graph.5 },
                    document,
                ],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
}

#[derive(Debug, Eq, PartialEq)]
struct LegacyEvidence {
    application_id: i64,
    schema_revision: i64,
    schema: Vec<(String, String, String)>,
    metadata: (String, String, i64, Vec<u8>, String, Vec<u8>),
    timeline: TimelineRow,
    graphs: Vec<GraphRow>,
}

fn legacy_evidence(path: &Path) -> LegacyEvidence {
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
                "SELECT type, name, sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
            )
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    let metadata = connection
        .query_row(
            "SELECT format, format_version, primitive_schema_revision, project_id, document_revision, root_timeline_id FROM project_metadata",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    let timeline = connection
        .query_row(
            "SELECT format_revision, document FROM timeline_component",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let graphs = {
        let mut statement = connection
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
            .collect::<rusqlite::Result<Vec<GraphRow>>>()
            .unwrap()
    };
    LegacyEvidence {
        application_id,
        schema_revision,
        schema,
        metadata,
        timeline,
        graphs,
    }
}

#[test]
fn supported_legacy_project_migrates_canonically_and_remains_editable() {
    assert_eq!(PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION, 0);
    let artifact = TempProject::new("supported");
    let save_as = TempProject::new("save-as");
    let copy = TempProject::new("copy");
    let backup = TempProject::new("backup");
    let expected = project_document();
    create_current(artifact.path(), &expected);
    downgrade_to_schema_zero(artifact.path(), true);

    let read_only = ProjectDatabase::open_read_only(artifact.path()).unwrap_err();
    assert_eq!(read_only.category(), ErrorCategory::Unsupported);

    let mut database = ProjectDatabase::open(artifact.path()).unwrap();
    assert!(database.was_migrated());
    assert_eq!(database.source_schema_revision(), 0);
    let mut migrated = database.load().unwrap();
    assert_eq!(migrated.snapshot(), expected.snapshot());

    let revision = migrated.revision();
    migrated
        .edit(revision, |draft| {
            draft
                .graph_mut(STANDALONE)?
                .as_standalone_mut()
                .unwrap()
                .set_name("edited after migration")
        })
        .unwrap();
    database.replace(&migrated.snapshot()).unwrap();
    let source_bytes_before_publications = std::fs::read(artifact.path()).unwrap();
    database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: copy.path().to_path_buf(),
                collision: ProjectDestinationCollision::RequireAbsent,
            },
            &migrated.snapshot(),
        )
        .unwrap();
    database
        .execute_save_command(
            ProjectSaveCommand::Backup {
                destination: backup.path().to_path_buf(),
            },
            &migrated.snapshot(),
        )
        .unwrap();
    database
        .execute_save_command(
            ProjectSaveCommand::SaveAs {
                destination: save_as.path().to_path_buf(),
                collision: ProjectDestinationCollision::RequireAbsent,
            },
            &migrated.snapshot(),
        )
        .unwrap();
    assert_eq!(
        std::fs::read(artifact.path()).unwrap(),
        source_bytes_before_publications
    );
    assert!(database.was_migrated());
    assert_eq!(database.source_schema_revision(), 0);
    assert_eq!(
        database.active_path(),
        Some(std::fs::canonicalize(save_as.path()).unwrap().as_path())
    );
    drop(database);

    for path in [artifact.path(), save_as.path(), copy.path(), backup.path()] {
        let reopened = ProjectDatabase::open_read_only(path).unwrap();
        assert_eq!(reopened.load().unwrap().snapshot(), migrated.snapshot());
        let connection = Connection::open(path).unwrap();
        let schema_revision: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let format_version: String = connection
            .query_row("SELECT format_version FROM project_metadata", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(schema_revision, i64::from(PROJECT_SCHEMA_REVISION));
        assert_eq!(format_version, PROJECT_FORMAT_VERSION);
        let timeline_revision: i64 = connection
            .query_row(
                "SELECT format_revision FROM timeline_component",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(timeline_revision, 1);
        let noncurrent_graphs: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM graph_components WHERE format_revision != 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(noncurrent_graphs, 0);
    }
}

#[test]
fn current_open_is_byte_stable_and_future_schema_is_never_mutated() {
    let current = TempProject::new("current");
    let document = project_document();
    create_current(current.path(), &document);
    let before = std::fs::read(current.path()).unwrap();
    let database = ProjectDatabase::open(current.path()).unwrap();
    assert!(!database.was_migrated());
    assert_eq!(database.source_schema_revision(), PROJECT_SCHEMA_REVISION);
    assert_eq!(database.load().unwrap().snapshot(), document.snapshot());
    drop(database);
    assert_eq!(std::fs::read(current.path()).unwrap(), before);

    let future = TempProject::new("future");
    create_current(future.path(), &document);
    let connection = Connection::open(future.path()).unwrap();
    connection
        .pragma_update(None, "user_version", i64::from(PROJECT_SCHEMA_REVISION + 1))
        .unwrap();
    drop(connection);
    let before = std::fs::read(future.path()).unwrap();
    let error = ProjectDatabase::open(future.path()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(std::fs::read(future.path()).unwrap(), before);
}

#[test]
fn malformed_legacy_project_rolls_back_without_changing_logical_state() {
    let artifact = TempProject::new("malformed");
    let document = project_document();
    create_current(artifact.path(), &document);
    downgrade_to_schema_zero(artifact.path(), false);
    let connection = Connection::open(artifact.path()).unwrap();
    connection
        .execute("UPDATE timeline_component SET document = x'7b7d'", [])
        .unwrap();
    drop(connection);
    let before = legacy_evidence(artifact.path());

    let error = ProjectDatabase::open(artifact.path()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert_eq!(legacy_evidence(artifact.path()), before);
    assert_eq!(before.application_id, i64::from(PROJECT_APPLICATION_ID));
    assert_eq!(before.schema_revision, 0);
}
