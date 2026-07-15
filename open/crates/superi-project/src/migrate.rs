//! Ordered, transactional whole-project schema migration.

use rusqlite::{Connection, TransactionBehavior};
use superi_core::error::{ErrorCategory, Recoverability, Result};
use superi_core::ids::{GraphId, ProjectId, TimelineId};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_graph::serialize::{deserialize_graph, GRAPH_DOCUMENT_FORMAT_REVISION};
use superi_timeline::compile::CompiledTimelineGraphValue;
use superi_timeline::serialize::{deserialize_timeline_state, TIMELINE_STATE_FORMAT_REVISION};

use crate::document::{ProjectDocument, ProjectGraph, StandaloneProjectGraph};
use crate::persist::{
    check_component_size, corrupt, database_error, fixed_bytes, initialize_schema, load_connection,
    parse_revision, project_error, stored_state_error, unsupported, validate_identity_and_schema,
    write_prepared_project, PreparedProject, StoredGraphKind, MAX_GRAPH_COUNT,
    MAX_STANDALONE_NAME_BYTES, PROJECT_APPLICATION_ID, PROJECT_FORMAT,
    PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION, PROJECT_SCHEMA_REVISION,
};

const LEGACY_FORMAT_VERSION: &str = "0.9.0";
const LEGACY_PROJECT_METADATA_SCHEMA: &str = "CREATE TABLE project_metadata (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format TEXT NOT NULL CHECK (format = 'superi.project'), format_version TEXT NOT NULL, primitive_schema_revision INTEGER NOT NULL CHECK (primitive_schema_revision > 0), project_id BLOB NOT NULL CHECK (length(project_id) = 16), document_revision TEXT NOT NULL, root_timeline_id BLOB NOT NULL CHECK (length(root_timeline_id) = 16)) STRICT";
const LEGACY_TIMELINE_COMPONENT_SCHEMA: &str = "CREATE TABLE timeline_component (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format_revision INTEGER NOT NULL CHECK (format_revision >= 0), document BLOB NOT NULL CHECK (length(document) <= 67108864)) STRICT";
const LEGACY_GRAPH_COMPONENTS_SCHEMA: &str = "CREATE TABLE graph_components (graph_id BLOB PRIMARY KEY CHECK (length(graph_id) = 16), graph_kind TEXT NOT NULL CHECK (graph_kind IN ('timeline', 'standalone')), root_timeline_id BLOB CHECK (root_timeline_id IS NULL OR length(root_timeline_id) = 16), name TEXT, graph_revision TEXT NOT NULL, format_revision INTEGER NOT NULL CHECK (format_revision >= 0), document BLOB NOT NULL CHECK (length(document) <= 67108864), CHECK ((graph_kind = 'timeline' AND root_timeline_id IS NOT NULL AND name IS NULL) OR (graph_kind = 'standalone' AND root_timeline_id IS NULL AND name IS NOT NULL AND length(name) > 0))) STRICT, WITHOUT ROWID";

type MigrationFunction = fn(&Connection) -> Result<()>;

#[derive(Clone, Copy)]
struct MigrationStep {
    source: u32,
    target: u32,
    apply: MigrationFunction,
}

const MIGRATIONS: &[MigrationStep] = &[MigrationStep {
    source: 0,
    target: 1,
    apply: migrate_schema_zero_to_one,
}];

pub(crate) fn migrate_connection(connection: &mut Connection) -> Result<u32> {
    migrate_connection_with_guard(connection, |_| Ok(()))
}

fn migrate_connection_with_guard<F>(connection: &mut Connection, guard: F) -> Result<u32>
where
    F: FnOnce(&Connection) -> Result<()>,
{
    validate_registry()?;
    let source = read_database_revision(connection)?;
    if source == PROJECT_SCHEMA_REVISION {
        validate_identity_and_schema(connection)?;
        return Ok(source);
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|source| database_error(source, "begin_project_migration"))?;
    let locked_source = read_database_revision(&transaction)?;
    if locked_source != source {
        return Err(project_error(
            ErrorCategory::Conflict,
            Recoverability::Retryable,
            "lock_project_migration",
            "project schema revision changed before migration acquired write authority",
        ));
    }
    validate_sqlite_integrity(&transaction, "validate_legacy_integrity")?;

    let mut revision = source;
    while revision < PROJECT_SCHEMA_REVISION {
        let step = MIGRATIONS
            .iter()
            .find(|step| step.source == revision)
            .ok_or_else(|| {
                unsupported(
                    "select_project_migration",
                    "project schema has no registered forward migration path",
                )
            })?;
        (step.apply)(&transaction)?;
        let actual = read_database_revision(&transaction)?;
        if actual != step.target {
            return Err(project_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "verify_project_migration_step",
                "project migration did not publish its declared successor revision",
            ));
        }
        revision = actual;
    }

    validate_identity_and_schema(&transaction)?;
    validate_sqlite_integrity(&transaction, "validate_migrated_integrity")?;
    guard(&transaction)?;
    transaction
        .commit()
        .map_err(|source| database_error(source, "commit_project_migration"))?;
    Ok(source)
}

fn validate_registry() -> Result<()> {
    let mut expected = PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION;
    for step in MIGRATIONS {
        if step.source != expected || step.target != expected + 1 {
            return Err(project_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "validate_project_migration_registry",
                "project migration registry is not a contiguous forward chain",
            ));
        }
        expected = step.target;
    }
    if expected != PROJECT_SCHEMA_REVISION {
        return Err(project_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "validate_project_migration_registry",
            "project migration registry does not end at the current schema revision",
        ));
    }
    Ok(())
}

fn read_database_revision(connection: &Connection) -> Result<u32> {
    let application_id: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|source| database_error(source, "read_migration_application_id"))?;
    if application_id != i64::from(PROJECT_APPLICATION_ID) {
        return Err(unsupported(
            "read_migration_application_id",
            "database is not a supported Superi project",
        ));
    }
    let revision: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|source| database_error(source, "read_migration_schema_revision"))?;
    let revision = u32::try_from(revision).map_err(|_| {
        corrupt(
            "read_migration_schema_revision",
            "project database schema revision is not representable",
        )
    })?;
    if revision > PROJECT_SCHEMA_REVISION {
        return Err(unsupported(
            "read_migration_schema_revision",
            "project database uses a future schema revision",
        ));
    }
    Ok(revision)
}

fn migrate_schema_zero_to_one(connection: &Connection) -> Result<()> {
    validate_legacy_schema(connection)?;
    let legacy = load_legacy_project(connection)?;
    let expected = legacy.snapshot();
    let prepared = PreparedProject::from_snapshot(&expected)?;

    connection
        .execute_batch(
            "DROP TABLE graph_components;\
             DROP TABLE timeline_component;\
             DROP TABLE project_metadata;",
        )
        .map_err(|source| database_error(source, "replace_legacy_project_schema"))?;
    initialize_schema(connection)?;
    write_prepared_project(connection, &prepared)?;
    let migrated = load_connection(connection)?;
    if migrated.snapshot() != expected {
        return Err(project_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "verify_project_migration",
            "migrated project did not reproduce the complete legacy snapshot",
        ));
    }
    Ok(())
}

fn validate_legacy_schema(connection: &Connection) -> Result<()> {
    let revision = read_database_revision(connection)?;
    if revision != PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION {
        return Err(corrupt(
            "validate_legacy_schema",
            "legacy migration received an unexpected schema revision",
        ));
    }
    let mut statement = connection
        .prepare(
            "SELECT type, name, sql FROM sqlite_schema \
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .map_err(|source| database_error(source, "inspect_legacy_project_schema"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|source| database_error(source, "inspect_legacy_project_schema"))?;
    let actual = rows
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|source| database_error(source, "inspect_legacy_project_schema"))?;
    let expected = vec![
        (
            "table".to_owned(),
            "graph_components".to_owned(),
            LEGACY_GRAPH_COMPONENTS_SCHEMA.to_owned(),
        ),
        (
            "table".to_owned(),
            "project_metadata".to_owned(),
            LEGACY_PROJECT_METADATA_SCHEMA.to_owned(),
        ),
        (
            "table".to_owned(),
            "timeline_component".to_owned(),
            LEGACY_TIMELINE_COMPONENT_SCHEMA.to_owned(),
        ),
    ];
    if actual != expected {
        return Err(corrupt(
            "inspect_legacy_project_schema",
            "project database schema objects do not match supported schema revision 0",
        ));
    }
    Ok(())
}

struct LegacyGraph {
    graph_id: GraphId,
    kind: StoredGraphKind,
    root_timeline_id: Option<TimelineId>,
    name: Option<String>,
    revision: u64,
    format_revision: u32,
    document: Vec<u8>,
}

fn load_legacy_project(connection: &Connection) -> Result<ProjectDocument> {
    require_row_count(connection, "project_metadata", 1)?;
    require_row_count(connection, "timeline_component", 1)?;
    let metadata = connection
        .query_row(
            "SELECT format, format_version, primitive_schema_revision, project_id, \
             document_revision, root_timeline_id FROM project_metadata WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                ))
            },
        )
        .map_err(|source| database_error(source, "read_legacy_project_metadata"))?;
    if metadata.0 != PROJECT_FORMAT || metadata.1 != LEGACY_FORMAT_VERSION {
        return Err(unsupported(
            "read_legacy_project_metadata",
            "project uses an unsupported legacy semantic format version",
        ));
    }
    if metadata.2 > i64::from(STABLE_PRIMITIVE_SCHEMA_REVISION) {
        return Err(unsupported(
            "read_legacy_project_metadata",
            "legacy project uses a future stable primitive revision",
        ));
    }
    if metadata.2 != i64::from(STABLE_PRIMITIVE_SCHEMA_REVISION) {
        return Err(corrupt(
            "read_legacy_project_metadata",
            "legacy project primitive revision is not supported by schema revision 0",
        ));
    }
    let project_id = ProjectId::from_bytes(fixed_bytes::<16>(
        metadata.3,
        "read_legacy_project_metadata",
        "project identity",
    )?);
    let document_revision = parse_revision(&metadata.4, "read_legacy_project_metadata")?;
    let root_timeline_id = TimelineId::from_bytes(fixed_bytes::<16>(
        metadata.5,
        "read_legacy_project_metadata",
        "root timeline identity",
    )?);

    let timeline = connection
        .query_row(
            "SELECT format_revision, document FROM timeline_component WHERE singleton = 1",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .map_err(|source| database_error(source, "read_legacy_timeline_component"))?;
    let timeline_revision = supported_component_revision(
        timeline.0,
        TIMELINE_STATE_FORMAT_REVISION,
        "read_legacy_timeline_component",
    )?;
    check_component_size(timeline.1.len(), "read_legacy_timeline_component")?;

    let graph_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM graph_components", [], |row| {
            row.get(0)
        })
        .map_err(|source| database_error(source, "count_legacy_graph_components"))?;
    let graph_count = usize::try_from(graph_count).map_err(|_| {
        corrupt(
            "count_legacy_graph_components",
            "legacy project graph count is not representable",
        )
    })?;
    if graph_count > MAX_GRAPH_COUNT {
        return Err(corrupt(
            "count_legacy_graph_components",
            "legacy project graph count exceeds the stable schema limit",
        ));
    }
    let mut statement = connection
        .prepare(
            "SELECT graph_id, graph_kind, root_timeline_id, name, graph_revision, \
             format_revision, document FROM graph_components ORDER BY graph_id",
        )
        .map_err(|source| database_error(source, "read_legacy_graph_components"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<Vec<u8>>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Vec<u8>>(6)?,
            ))
        })
        .map_err(|source| database_error(source, "read_legacy_graph_components"))?;
    let mut graphs = Vec::with_capacity(graph_count);
    for row in rows {
        let row = row.map_err(|source| database_error(source, "read_legacy_graph_component"))?;
        check_component_size(row.6.len(), "read_legacy_graph_component")?;
        let kind = StoredGraphKind::parse(&row.1)?;
        let root = row
            .2
            .map(|value| {
                fixed_bytes::<16>(
                    value,
                    "read_legacy_graph_component",
                    "root timeline identity",
                )
                .map(TimelineId::from_bytes)
            })
            .transpose()?;
        if row
            .3
            .as_ref()
            .is_some_and(|name| name.trim().is_empty() || name.len() > MAX_STANDALONE_NAME_BYTES)
        {
            return Err(corrupt(
                "read_legacy_graph_component",
                "legacy standalone graph name is outside stable schema bounds",
            ));
        }
        match (kind, root, row.3.as_ref()) {
            (StoredGraphKind::Timeline, Some(_), None)
            | (StoredGraphKind::Standalone, None, Some(_)) => {}
            _ => {
                return Err(corrupt(
                    "read_legacy_graph_component",
                    "legacy graph ownership fields do not match the graph kind",
                ));
            }
        }
        graphs.push(LegacyGraph {
            graph_id: GraphId::from_bytes(fixed_bytes::<16>(
                row.0,
                "read_legacy_graph_component",
                "graph identity",
            )?),
            kind,
            root_timeline_id: root,
            name: row.3,
            revision: parse_revision(&row.4, "read_legacy_graph_component")?,
            format_revision: supported_component_revision(
                row.5,
                GRAPH_DOCUMENT_FORMAT_REVISION,
                "read_legacy_graph_component",
            )?,
            document: row.6,
        });
    }
    if graphs.len() != graph_count {
        return Err(corrupt(
            "read_legacy_graph_components",
            "legacy project graph count changed during interpretation",
        ));
    }

    let timeline_load = deserialize_timeline_state(&timeline.1)
        .map_err(|source| stored_state_error(source, "decode_legacy_timeline_component"))?;
    if timeline_load.source_format_revision() != timeline_revision {
        return Err(corrupt(
            "decode_legacy_timeline_component",
            "legacy timeline revision evidence does not match its document",
        ));
    }
    let editorial_project = timeline_load.into_project();
    if editorial_project.id() != project_id {
        return Err(corrupt(
            "decode_legacy_timeline_component",
            "legacy timeline project identity does not match project metadata",
        ));
    }

    let mut restored_graphs = Vec::with_capacity(graphs.len());
    for graph in graphs {
        let graph_load = deserialize_graph::<CompiledTimelineGraphValue>(&graph.document)
            .map_err(|source| stored_state_error(source, "decode_legacy_graph_component"))?;
        if graph_load.source_format_revision() != graph.format_revision {
            return Err(corrupt(
                "decode_legacy_graph_component",
                "legacy graph revision evidence does not match its document",
            ));
        }
        let editable = graph_load.into_graph();
        let graph_snapshot = editable.snapshot();
        if graph_snapshot.graph_id() != graph.graph_id
            || graph_snapshot.revision() != graph.revision
        {
            return Err(corrupt(
                "decode_legacy_graph_component",
                "legacy graph evidence does not match decoded graph state",
            ));
        }
        let restored = match graph.kind {
            StoredGraphKind::Timeline => ProjectGraph::restore_timeline(
                &editorial_project,
                graph.root_timeline_id.expect("validated timeline owner"),
                editable,
            )
            .map_err(|source| stored_state_error(source, "restore_legacy_timeline_graph"))?,
            StoredGraphKind::Standalone => ProjectGraph::Standalone(
                StandaloneProjectGraph::new(
                    graph.name.expect("validated standalone name"),
                    editable,
                )
                .map_err(|source| stored_state_error(source, "restore_legacy_standalone_graph"))?,
            ),
        };
        restored_graphs.push(restored);
    }
    ProjectDocument::from_parts(
        document_revision,
        editorial_project,
        root_timeline_id,
        restored_graphs,
    )
    .map_err(|source| stored_state_error(source, "restore_legacy_project_document"))
}

fn supported_component_revision(value: i64, current: u32, operation: &'static str) -> Result<u32> {
    let value = u32::try_from(value)
        .map_err(|_| corrupt(operation, "component revision is not representable"))?;
    if value > current {
        return Err(unsupported(
            operation,
            "legacy component uses a future format revision",
        ));
    }
    Ok(value)
}

fn require_row_count(connection: &Connection, table: &'static str, expected: i64) -> Result<()> {
    let query = format!("SELECT COUNT(*) FROM {table}");
    let count: i64 = connection
        .query_row(&query, [], |row| row.get(0))
        .map_err(|source| database_error(source, "count_legacy_project_rows"))?;
    if count != expected {
        return Err(corrupt(
            "count_legacy_project_rows",
            "legacy project singleton row count is invalid",
        ));
    }
    Ok(())
}

fn validate_sqlite_integrity(connection: &Connection, operation: &'static str) -> Result<()> {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|source| database_error(source, operation))?;
    if integrity != "ok" {
        return Err(corrupt(operation, "SQLite integrity check failed"));
    }
    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|source| database_error(source, operation))?;
    let mut rows = statement
        .query([])
        .map_err(|source| database_error(source, operation))?;
    if rows
        .next()
        .map_err(|source| database_error(source, operation))?
        .is_some()
    {
        return Err(corrupt(
            operation,
            "SQLite foreign key consistency check failed",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::{params, Connection};
    use superi_core::error::{ErrorCategory, Recoverability};
    use superi_core::ids::{ProjectId, TimelineId};
    use superi_core::time::{RationalTime, Timebase};
    use superi_graph::serialize::serialize_graph;
    use superi_timeline::model::{EditorialProject, Timeline};
    use superi_timeline::serialize::serialize_timeline_state;

    use super::*;

    fn legacy_connection() -> (Connection, crate::document::ProjectSnapshot) {
        let project_id = ProjectId::from_raw(0xa00);
        let root = TimelineId::from_raw(0xa01);
        let timebase = Timebase::integer(24).unwrap();
        let timeline = Timeline::new(
            root,
            "rollback timeline",
            timebase,
            RationalTime::zero(timebase),
            vec![],
        );
        let editorial =
            EditorialProject::new(project_id, "rollback project", [], [timeline]).unwrap();
        let document = ProjectDocument::new(editorial, root).unwrap();
        let snapshot = document.snapshot();

        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(&format!(
                "{LEGACY_PROJECT_METADATA_SCHEMA};{LEGACY_TIMELINE_COMPONENT_SCHEMA};{LEGACY_GRAPH_COMPONENTS_SCHEMA};"
            ))
            .unwrap();
        connection
            .pragma_update(None, "application_id", i64::from(PROJECT_APPLICATION_ID))
            .unwrap();
        connection
            .pragma_update(
                None,
                "user_version",
                i64::from(PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION),
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO project_metadata (singleton, format, format_version, primitive_schema_revision, project_id, document_revision, root_timeline_id) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    PROJECT_FORMAT,
                    LEGACY_FORMAT_VERSION,
                    i64::from(STABLE_PRIMITIVE_SCHEMA_REVISION),
                    snapshot.project_id().to_bytes().as_slice(),
                    snapshot.revision().to_string(),
                    snapshot.root_timeline_id().to_bytes().as_slice(),
                ],
            )
            .unwrap();
        let timeline_document = serialize_timeline_state(snapshot.editorial_project()).unwrap();
        connection
            .execute(
                "INSERT INTO timeline_component (singleton, format_revision, document) VALUES (1, ?1, ?2)",
                params![
                    i64::from(TIMELINE_STATE_FORMAT_REVISION),
                    timeline_document
                ],
            )
            .unwrap();
        for graph in snapshot.graphs() {
            let graph_snapshot = graph.snapshot();
            let document = serialize_graph(&graph_snapshot).unwrap();
            let (kind, root_timeline_id, name) = match graph {
                ProjectGraph::Timeline(compilation) => (
                    "timeline",
                    Some(compilation.root_timeline_id().to_bytes().to_vec()),
                    None,
                ),
                ProjectGraph::Standalone(standalone) => {
                    ("standalone", None, Some(standalone.name().to_owned()))
                }
            };
            connection
                .execute(
                    "INSERT INTO graph_components (graph_id, graph_kind, root_timeline_id, name, graph_revision, format_revision, document) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        graph_snapshot.graph_id().to_bytes().as_slice(),
                        kind,
                        root_timeline_id,
                        name,
                        graph_snapshot.revision().to_string(),
                        i64::from(GRAPH_DOCUMENT_FORMAT_REVISION),
                        document,
                    ],
                )
                .unwrap();
        }
        (connection, snapshot)
    }

    #[test]
    fn registry_is_contiguous_and_precommit_interruption_rolls_back_schema_rewrite() {
        validate_registry().unwrap();
        assert_eq!(MIGRATIONS.len(), 1);
        assert_eq!(MIGRATIONS[0].source, 0);
        assert_eq!(MIGRATIONS[0].target, PROJECT_SCHEMA_REVISION);

        let (mut connection, expected) = legacy_connection();
        let error = migrate_connection_with_guard(&mut connection, |_| {
            Err(project_error(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "simulate_migration_interruption",
                "simulated interruption before migration commit",
            ))
        })
        .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(
            read_database_revision(&connection).unwrap(),
            PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION
        );
        validate_legacy_schema(&connection).unwrap();
        assert_eq!(
            load_legacy_project(&connection).unwrap().snapshot(),
            expected
        );

        assert_eq!(
            migrate_connection(&mut connection).unwrap(),
            PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION
        );
        assert_eq!(load_connection(&connection).unwrap().snapshot(), expected);
    }
}
