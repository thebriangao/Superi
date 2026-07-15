//! Stable, explicitly versioned whole-project SQLite serialization.
//!
//! This module owns the schema and connection-level interpretation of one
//! `.superi` application database. Migration policy is implemented by the
//! private sibling module. Destination publication, autosave, and recovery
//! policy remain separate project concerns.

use std::fmt;
use std::fs::OpenOptions;
use std::path::Path;
use std::time::Duration;

use rusqlite::config::DbConfig;
use rusqlite::{params, Connection, ErrorCode, OpenFlags, TransactionBehavior};
use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{GraphId, ProjectId, TimelineId};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_graph::serialize::{deserialize_graph, serialize_graph, GRAPH_DOCUMENT_FORMAT_REVISION};
use superi_timeline::compile::CompiledTimelineGraphValue;
use superi_timeline::serialize::{
    deserialize_timeline_state, serialize_timeline_state, TIMELINE_STATE_FORMAT_REVISION,
};

use crate::document::{ProjectDocument, ProjectGraph, ProjectSnapshot, StandaloneProjectGraph};
use crate::migrate::migrate_connection;

const COMPONENT: &str = "superi-project.persistence";
const MANIFEST_DOMAIN: &[u8] = b"superi.project.manifest.v1";
pub(crate) const MAX_COMPONENT_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_GRAPH_COUNT: usize = 4096;
pub(crate) const MAX_STANDALONE_NAME_BYTES: usize = 16 * 1024;

/// SQLite application identity stored in every `.superi` database (`SUPR`).
pub const PROJECT_APPLICATION_ID: u32 = 0x5355_5052;
/// Oldest project database schema with a registered lossless forward migration.
pub const PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION: u32 = 0;
/// Current monotonic project database schema revision.
pub const PROJECT_SCHEMA_REVISION: u32 = 1;
/// Stable semantic identity of the whole-project format.
pub const PROJECT_FORMAT: &str = "superi.project";
/// Current semantic project format version.
pub const PROJECT_FORMAT_VERSION: &str = "1.0.0";

pub(crate) const PROJECT_METADATA_SCHEMA: &str = "CREATE TABLE project_metadata (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format TEXT NOT NULL CHECK (format = 'superi.project'), format_version TEXT NOT NULL, primitive_schema_revision INTEGER NOT NULL CHECK (primitive_schema_revision > 0), project_id BLOB NOT NULL CHECK (length(project_id) = 16), document_revision TEXT NOT NULL, root_timeline_id BLOB NOT NULL CHECK (length(root_timeline_id) = 16), manifest_sha256 BLOB NOT NULL CHECK (length(manifest_sha256) = 32)) STRICT";
pub(crate) const TIMELINE_COMPONENT_SCHEMA: &str = "CREATE TABLE timeline_component (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format_revision INTEGER NOT NULL CHECK (format_revision > 0), byte_length INTEGER NOT NULL CHECK (byte_length >= 0 AND byte_length <= 67108864), sha256 BLOB NOT NULL CHECK (length(sha256) = 32), document BLOB NOT NULL CHECK (length(document) = byte_length)) STRICT";
pub(crate) const GRAPH_COMPONENTS_SCHEMA: &str = "CREATE TABLE graph_components (graph_id BLOB PRIMARY KEY CHECK (length(graph_id) = 16), graph_kind TEXT NOT NULL CHECK (graph_kind IN ('timeline', 'standalone')), root_timeline_id BLOB CHECK (root_timeline_id IS NULL OR length(root_timeline_id) = 16), name TEXT, graph_revision TEXT NOT NULL, format_revision INTEGER NOT NULL CHECK (format_revision > 0), byte_length INTEGER NOT NULL CHECK (byte_length >= 0 AND byte_length <= 67108864), sha256 BLOB NOT NULL CHECK (length(sha256) = 32), document BLOB NOT NULL CHECK (length(document) = byte_length), CHECK ((graph_kind = 'timeline' AND root_timeline_id IS NOT NULL AND name IS NULL) OR (graph_kind = 'standalone' AND root_timeline_id IS NULL AND name IS NOT NULL AND length(name) > 0))) STRICT, WITHOUT ROWID";

/// One secured connection to a stable whole-project database.
pub struct ProjectDatabase {
    connection: Connection,
    writable: bool,
    source_schema_revision: u32,
}

impl fmt::Debug for ProjectDatabase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectDatabase")
            .field("writable", &self.writable)
            .field("source_schema_revision", &self.source_schema_revision)
            .finish_non_exhaustive()
    }
}

impl ProjectDatabase {
    /// Creates a new schema-1 database without replacing an existing path.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|source| create_path_error(source, "create_database"))?;

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = match Connection::open_with_flags(path, flags) {
            Ok(connection) => connection,
            Err(source) => {
                let _ = std::fs::remove_file(path);
                return Err(database_error(source, "open_created_database"));
            }
        };
        if let Err(error) =
            initialize_connection(&connection, true).and_then(|()| initialize_schema(&connection))
        {
            drop(connection);
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        Ok(Self {
            connection,
            writable: true,
            source_schema_revision: PROJECT_SCHEMA_REVISION,
        })
    }

    /// Creates a secured in-memory schema-1 database.
    pub fn memory() -> Result<Self> {
        let connection = Connection::open_in_memory()
            .map_err(|source| database_error(source, "open_memory_database"))?;
        initialize_connection(&connection, true)?;
        initialize_schema(&connection)?;
        Ok(Self {
            connection,
            writable: true,
            source_schema_revision: PROJECT_SCHEMA_REVISION,
        })
    }

    /// Opens an existing project with write authority and migrates supported schemas.
    ///
    /// Current projects are validated without mutation. Every registered older
    /// schema is upgraded to the current schema in one immediate transaction.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut connection = Connection::open_with_flags(path, flags)
            .map_err(|source| database_error(source, "open_writable_database"))?;
        initialize_connection(&connection, true)?;
        let source_schema_revision = migrate_connection(&mut connection)?;
        Ok(Self {
            connection,
            writable: true,
            source_schema_revision,
        })
    }

    /// Opens an existing `.superi` database without write authority.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(path, flags)
            .map_err(|source| database_error(source, "open_read_only_database"))?;
        initialize_connection(&connection, false)?;
        validate_identity_and_schema(&connection)?;
        Ok(Self {
            connection,
            writable: false,
            source_schema_revision: PROJECT_SCHEMA_REVISION,
        })
    }

    /// Returns the schema revision observed when this connection was opened.
    #[must_use]
    pub const fn source_schema_revision(&self) -> u32 {
        self.source_schema_revision
    }

    /// Returns whether writable open upgraded a supported older schema.
    #[must_use]
    pub const fn was_migrated(&self) -> bool {
        self.source_schema_revision != PROJECT_SCHEMA_REVISION
    }

    /// Replaces all semantic database rows in one immediate transaction.
    ///
    /// The complete snapshot is encoded and bounded before the transaction.
    /// The candidate rows are loaded through the public component decoders and
    /// compared with the supplied snapshot before commit.
    pub fn replace(&mut self, snapshot: &ProjectSnapshot) -> Result<()> {
        if !self.writable {
            return Err(project_error(
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "replace_project",
                "project database was opened without write authority",
            ));
        }

        let prepared = PreparedProject::from_snapshot(snapshot)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|source| database_error(source, "begin_replace_project"))?;
        write_prepared_project(&transaction, &prepared)?;
        let loaded = load_connection(&transaction)?;
        if loaded.snapshot() != *snapshot {
            return Err(project_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "verify_replace_project",
                "project database candidate did not reproduce the supplied snapshot",
            ));
        }
        transaction
            .commit()
            .map_err(|source| database_error(source, "commit_replace_project"))
    }

    /// Loads one fully checked project document or publishes no partial state.
    pub fn load(&self) -> Result<ProjectDocument> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|source| database_error(source, "begin_load_project"))?;
        let document = load_connection(&transaction)?;
        transaction
            .commit()
            .map_err(|source| database_error(source, "finish_load_project"))?;
        Ok(document)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StoredGraphKind {
    Timeline,
    Standalone,
}

impl StoredGraphKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Timeline => "timeline",
            Self::Standalone => "standalone",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "timeline" => Ok(Self::Timeline),
            "standalone" => Ok(Self::Standalone),
            _ => Err(corrupt("decode_graph_kind", "unknown graph kind")),
        }
    }
}

pub(crate) struct PreparedProject {
    project_id: ProjectId,
    revision: u64,
    root_timeline_id: TimelineId,
    timeline_document: Vec<u8>,
    timeline_digest: [u8; 32],
    graphs: Vec<PreparedGraph>,
    manifest_digest: [u8; 32],
}

struct PreparedGraph {
    graph_id: GraphId,
    kind: StoredGraphKind,
    root_timeline_id: Option<TimelineId>,
    name: Option<String>,
    revision: u64,
    document: Vec<u8>,
    digest: [u8; 32],
}

impl PreparedProject {
    pub(crate) fn from_snapshot(snapshot: &ProjectSnapshot) -> Result<Self> {
        let timeline_document = serialize_timeline_state(snapshot.editorial_project())?;
        check_component_size(timeline_document.len(), "encode_timeline_component")?;
        let timeline_digest = sha256(&timeline_document);

        if snapshot.graphs().len() > MAX_GRAPH_COUNT {
            return Err(resource_exhausted(
                "encode_graph_components",
                "project graph count exceeds the stable schema limit",
            ));
        }
        let graphs = snapshot
            .graphs()
            .map(|graph| {
                let graph_snapshot = graph.snapshot();
                let document = serialize_graph(&graph_snapshot)?;
                check_component_size(document.len(), "encode_graph_component")?;
                let (kind, root_timeline_id, name) = match graph {
                    ProjectGraph::Timeline(compilation) => (
                        StoredGraphKind::Timeline,
                        Some(compilation.root_timeline_id()),
                        None,
                    ),
                    ProjectGraph::Standalone(standalone) => {
                        if standalone.name().len() > MAX_STANDALONE_NAME_BYTES {
                            return Err(resource_exhausted(
                                "encode_graph_component",
                                "standalone graph name exceeds the stable schema limit",
                            ));
                        }
                        (
                            StoredGraphKind::Standalone,
                            None,
                            Some(standalone.name().to_owned()),
                        )
                    }
                };
                Ok(PreparedGraph {
                    graph_id: graph_snapshot.graph_id(),
                    kind,
                    root_timeline_id,
                    name,
                    revision: graph_snapshot.revision(),
                    digest: sha256(&document),
                    document,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let mut prepared = Self {
            project_id: snapshot.project_id(),
            revision: snapshot.revision(),
            root_timeline_id: snapshot.root_timeline_id(),
            timeline_document,
            timeline_digest,
            graphs,
            manifest_digest: [0; 32],
        };
        prepared.manifest_digest = manifest_digest(&prepared);
        Ok(prepared)
    }
}

fn initialize_connection(connection: &Connection, writable: bool) -> Result<()> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|source| database_error(source, "configure_busy_timeout"))?;
    for (configuration, value, operation) in [
        (
            DbConfig::SQLITE_DBCONFIG_DEFENSIVE,
            true,
            "enable_defensive_mode",
        ),
        (
            DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY,
            true,
            "enable_foreign_keys",
        ),
        (
            DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER,
            false,
            "disable_triggers",
        ),
        (
            DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW,
            false,
            "disable_views",
        ),
        (
            DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA,
            false,
            "disable_trusted_schema",
        ),
        (
            DbConfig::SQLITE_DBCONFIG_DQS_DDL,
            false,
            "disable_ddl_double_quotes",
        ),
        (
            DbConfig::SQLITE_DBCONFIG_DQS_DML,
            false,
            "disable_dml_double_quotes",
        ),
    ] {
        let actual = connection
            .set_db_config(configuration, value)
            .map_err(|source| database_error(source, operation))?;
        if actual != value {
            return Err(project_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                operation,
                "SQLite did not apply the required connection configuration",
            ));
        }
    }
    connection
        .pragma_update(None, "cell_size_check", true)
        .map_err(|source| database_error(source, "enable_cell_size_check"))?;
    connection
        .pragma_update(None, "mmap_size", 0_i64)
        .map_err(|source| database_error(source, "disable_memory_mapping"))?;
    if !writable {
        connection
            .pragma_update(None, "query_only", true)
            .map_err(|source| database_error(source, "enable_query_only"))?;
    }
    Ok(())
}

pub(crate) fn initialize_schema(connection: &Connection) -> Result<()> {
    let schema =
        format!("{PROJECT_METADATA_SCHEMA};{TIMELINE_COMPONENT_SCHEMA};{GRAPH_COMPONENTS_SCHEMA};");
    connection
        .execute_batch(&schema)
        .map_err(|source| database_error(source, "create_project_schema"))?;
    connection
        .pragma_update(None, "application_id", i64::from(PROJECT_APPLICATION_ID))
        .map_err(|source| database_error(source, "set_application_id"))?;
    connection
        .pragma_update(None, "user_version", i64::from(PROJECT_SCHEMA_REVISION))
        .map_err(|source| database_error(source, "set_schema_revision"))?;
    validate_identity_and_schema(connection)
}

pub(crate) fn validate_identity_and_schema(connection: &Connection) -> Result<()> {
    let quick_check: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|source| database_error(source, "quick_check"))?;
    if quick_check != "ok" {
        return Err(corrupt("quick_check", "SQLite integrity check failed"));
    }

    let application_id: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(|source| database_error(source, "read_application_id"))?;
    if application_id != i64::from(PROJECT_APPLICATION_ID) {
        return Err(unsupported(
            "read_application_id",
            "database is not a supported Superi project",
        ));
    }
    let schema_revision: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|source| database_error(source, "read_schema_revision"))?;
    if schema_revision > i64::from(PROJECT_SCHEMA_REVISION) {
        return Err(unsupported(
            "read_schema_revision",
            "project database uses a future schema revision",
        ));
    }
    if schema_revision >= 0
        && schema_revision < i64::from(PROJECT_SCHEMA_REVISION)
        && schema_revision >= i64::from(PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION)
    {
        return Err(unsupported(
            "read_schema_revision",
            "project database requires writable migration",
        ));
    }
    if schema_revision != i64::from(PROJECT_SCHEMA_REVISION) {
        return Err(corrupt(
            "read_schema_revision",
            "project database does not declare schema revision 1",
        ));
    }

    let mut statement = connection
        .prepare(
            "SELECT type, name, sql FROM sqlite_schema \
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .map_err(|source| database_error(source, "inspect_project_schema"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|source| database_error(source, "inspect_project_schema"))?;
    let actual = rows
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|source| database_error(source, "inspect_project_schema"))?;
    let expected = vec![
        (
            "table".to_owned(),
            "graph_components".to_owned(),
            GRAPH_COMPONENTS_SCHEMA.to_owned(),
        ),
        (
            "table".to_owned(),
            "project_metadata".to_owned(),
            PROJECT_METADATA_SCHEMA.to_owned(),
        ),
        (
            "table".to_owned(),
            "timeline_component".to_owned(),
            TIMELINE_COMPONENT_SCHEMA.to_owned(),
        ),
    ];
    if actual != expected {
        return Err(corrupt(
            "inspect_project_schema",
            "project database schema objects do not match schema revision 1",
        ));
    }
    Ok(())
}

pub(crate) fn write_prepared_project(
    connection: &Connection,
    prepared: &PreparedProject,
) -> Result<()> {
    connection
        .execute_batch(
            "DELETE FROM graph_components;\
             DELETE FROM timeline_component;\
             DELETE FROM project_metadata;",
        )
        .map_err(|source| database_error(source, "clear_project_rows"))?;
    connection
        .execute(
            "INSERT INTO project_metadata \
             (singleton, format, format_version, primitive_schema_revision, project_id, \
              document_revision, root_timeline_id, manifest_sha256) \
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                PROJECT_FORMAT,
                PROJECT_FORMAT_VERSION,
                i64::from(STABLE_PRIMITIVE_SCHEMA_REVISION),
                prepared.project_id.to_bytes().as_slice(),
                prepared.revision.to_string(),
                prepared.root_timeline_id.to_bytes().as_slice(),
                prepared.manifest_digest.as_slice(),
            ],
        )
        .map_err(|source| database_error(source, "write_project_metadata"))?;
    connection
        .execute(
            "INSERT INTO timeline_component \
             (singleton, format_revision, byte_length, sha256, document) \
             VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                i64::from(TIMELINE_STATE_FORMAT_REVISION),
                prepared.timeline_document.len() as i64,
                prepared.timeline_digest.as_slice(),
                prepared.timeline_document.as_slice(),
            ],
        )
        .map_err(|source| database_error(source, "write_timeline_component"))?;

    let mut statement = connection
        .prepare(
            "INSERT INTO graph_components \
             (graph_id, graph_kind, root_timeline_id, name, graph_revision, format_revision, \
              byte_length, sha256, document) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .map_err(|source| database_error(source, "prepare_graph_components"))?;
    for graph in &prepared.graphs {
        let root_timeline_id = graph
            .root_timeline_id
            .map(|value| value.to_bytes().to_vec());
        statement
            .execute(params![
                graph.graph_id.to_bytes().as_slice(),
                graph.kind.as_str(),
                root_timeline_id,
                graph.name.as_deref(),
                graph.revision.to_string(),
                i64::from(GRAPH_DOCUMENT_FORMAT_REVISION),
                graph.document.len() as i64,
                graph.digest.as_slice(),
                graph.document.as_slice(),
            ])
            .map_err(|source| database_error(source, "write_graph_component"))?;
    }
    Ok(())
}

pub(crate) fn load_connection(connection: &Connection) -> Result<ProjectDocument> {
    validate_identity_and_schema(connection)?;
    require_row_count(connection, "project_metadata", 1)?;
    require_row_count(connection, "timeline_component", 1)?;

    let metadata = connection
        .query_row(
            "SELECT format, format_version, primitive_schema_revision, project_id, \
             document_revision, root_timeline_id, manifest_sha256 \
             FROM project_metadata WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                ))
            },
        )
        .map_err(|source| database_error(source, "read_project_metadata"))?;
    if metadata.0 != PROJECT_FORMAT || metadata.1 != PROJECT_FORMAT_VERSION {
        return Err(unsupported(
            "read_project_metadata",
            "project uses an unsupported semantic format version",
        ));
    }
    if metadata.2 > i64::from(STABLE_PRIMITIVE_SCHEMA_REVISION) {
        return Err(unsupported(
            "read_project_metadata",
            "project uses a future stable primitive revision",
        ));
    }
    if metadata.2 != i64::from(STABLE_PRIMITIVE_SCHEMA_REVISION) {
        return Err(corrupt(
            "read_project_metadata",
            "project primitive revision does not match schema revision 1",
        ));
    }
    let project_id = ProjectId::from_bytes(fixed_bytes::<16>(
        metadata.3,
        "read_project_metadata",
        "project identity",
    )?);
    let revision = parse_revision(&metadata.4, "read_project_metadata")?;
    let root_timeline_id = TimelineId::from_bytes(fixed_bytes::<16>(
        metadata.5,
        "read_project_metadata",
        "root timeline identity",
    )?);
    let stored_manifest = fixed_bytes::<32>(
        metadata.6,
        "read_project_metadata",
        "project manifest digest",
    )?;

    let timeline = connection
        .query_row(
            "SELECT format_revision, byte_length, sha256, document \
             FROM timeline_component WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .map_err(|source| database_error(source, "read_timeline_component"))?;
    let timeline_format_revision = supported_revision(
        timeline.0,
        TIMELINE_STATE_FORMAT_REVISION,
        "read_timeline_component",
    )?;
    validate_component(
        timeline.1,
        &timeline.2,
        &timeline.3,
        "read_timeline_component",
    )?;
    let timeline_digest =
        fixed_bytes::<32>(timeline.2, "read_timeline_component", "timeline digest")?;

    let graph_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM graph_components", [], |row| {
            row.get(0)
        })
        .map_err(|source| database_error(source, "count_graph_components"))?;
    let graph_count = usize::try_from(graph_count).map_err(|_| {
        corrupt(
            "count_graph_components",
            "project graph count is not representable",
        )
    })?;
    if graph_count > MAX_GRAPH_COUNT {
        return Err(corrupt(
            "count_graph_components",
            "project graph count exceeds the stable schema limit",
        ));
    }
    let mut statement = connection
        .prepare(
            "SELECT graph_id, graph_kind, root_timeline_id, name, graph_revision, \
             format_revision, byte_length, sha256, document \
             FROM graph_components ORDER BY graph_id",
        )
        .map_err(|source| database_error(source, "read_graph_components"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<Vec<u8>>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Vec<u8>>(7)?,
                row.get::<_, Vec<u8>>(8)?,
            ))
        })
        .map_err(|source| database_error(source, "read_graph_components"))?;
    let mut graphs = Vec::with_capacity(graph_count);
    for row in rows {
        let row = row.map_err(|source| database_error(source, "read_graph_component"))?;
        let format_revision = supported_revision(
            row.5,
            GRAPH_DOCUMENT_FORMAT_REVISION,
            "read_graph_component",
        )?;
        debug_assert_eq!(format_revision, GRAPH_DOCUMENT_FORMAT_REVISION);
        validate_component(row.6, &row.7, &row.8, "read_graph_component")?;
        let kind = StoredGraphKind::parse(&row.1)?;
        let root = row
            .2
            .map(|value| {
                fixed_bytes::<16>(value, "read_graph_component", "root timeline identity")
                    .map(TimelineId::from_bytes)
            })
            .transpose()?;
        if row
            .3
            .as_ref()
            .is_some_and(|name| name.trim().is_empty() || name.len() > MAX_STANDALONE_NAME_BYTES)
        {
            return Err(corrupt(
                "read_graph_component",
                "standalone graph name is outside stable schema bounds",
            ));
        }
        match (kind, root, row.3.as_ref()) {
            (StoredGraphKind::Timeline, Some(_), None)
            | (StoredGraphKind::Standalone, None, Some(_)) => {}
            _ => {
                return Err(corrupt(
                    "read_graph_component",
                    "graph ownership fields do not match the graph kind",
                ));
            }
        }
        graphs.push(PreparedGraph {
            graph_id: GraphId::from_bytes(fixed_bytes::<16>(
                row.0,
                "read_graph_component",
                "graph identity",
            )?),
            kind,
            root_timeline_id: root,
            name: row.3,
            revision: parse_revision(&row.4, "read_graph_component")?,
            document: row.8,
            digest: fixed_bytes::<32>(row.7, "read_graph_component", "graph digest")?,
        });
    }
    if graphs.len() != graph_count {
        return Err(corrupt(
            "read_graph_components",
            "project graph count changed during interpretation",
        ));
    }

    let prepared = PreparedProject {
        project_id,
        revision,
        root_timeline_id,
        timeline_document: timeline.3,
        timeline_digest,
        graphs,
        manifest_digest: stored_manifest,
    };
    if manifest_digest(&prepared) != stored_manifest {
        return Err(corrupt(
            "verify_project_manifest",
            "project manifest digest does not match the stored components",
        ));
    }

    let timeline_load = deserialize_timeline_state(&prepared.timeline_document)
        .map_err(|source| stored_state_error(source, "decode_timeline_component"))?;
    if timeline_load.source_format_revision() != timeline_format_revision
        || timeline_load.canonical_document() != prepared.timeline_document
    {
        return Err(corrupt(
            "decode_timeline_component",
            "timeline component is not canonical for schema revision 1",
        ));
    }
    let editorial_project = timeline_load.into_project();
    if editorial_project.id() != prepared.project_id {
        return Err(corrupt(
            "decode_timeline_component",
            "timeline project identity does not match project metadata",
        ));
    }

    let mut restored_graphs = Vec::with_capacity(prepared.graphs.len());
    for graph in prepared.graphs {
        let graph_load = deserialize_graph::<CompiledTimelineGraphValue>(&graph.document)
            .map_err(|source| stored_state_error(source, "decode_graph_component"))?;
        if graph_load.source_format_revision() != GRAPH_DOCUMENT_FORMAT_REVISION
            || graph_load.canonical_document() != graph.document
        {
            return Err(corrupt(
                "decode_graph_component",
                "graph component is not canonical for schema revision 1",
            ));
        }
        let editable = graph_load.into_graph();
        let graph_snapshot = editable.snapshot();
        if graph_snapshot.graph_id() != graph.graph_id
            || graph_snapshot.revision() != graph.revision
        {
            return Err(corrupt(
                "decode_graph_component",
                "graph component evidence does not match decoded graph state",
            ));
        }
        let restored = match graph.kind {
            StoredGraphKind::Timeline => ProjectGraph::restore_timeline(
                &editorial_project,
                graph.root_timeline_id.expect("validated timeline owner"),
                editable,
            )
            .map_err(|source| stored_state_error(source, "restore_timeline_graph"))?,
            StoredGraphKind::Standalone => ProjectGraph::Standalone(
                StandaloneProjectGraph::new(
                    graph.name.expect("validated standalone name"),
                    editable,
                )
                .map_err(|source| stored_state_error(source, "restore_standalone_graph"))?,
            ),
        };
        restored_graphs.push(restored);
    }
    ProjectDocument::from_parts(
        prepared.revision,
        editorial_project,
        prepared.root_timeline_id,
        restored_graphs,
    )
    .map_err(|source| stored_state_error(source, "restore_project_document"))
}

fn require_row_count(connection: &Connection, table: &'static str, expected: i64) -> Result<()> {
    let query = format!("SELECT COUNT(*) FROM {table}");
    let count: i64 = connection
        .query_row(&query, [], |row| row.get(0))
        .map_err(|source| database_error(source, "count_project_rows"))?;
    if count != expected {
        return Err(corrupt(
            "count_project_rows",
            "project singleton row count is invalid",
        ));
    }
    Ok(())
}

fn validate_component(
    stored_length: i64,
    stored_digest: &[u8],
    document: &[u8],
    operation: &'static str,
) -> Result<()> {
    let stored_length = usize::try_from(stored_length)
        .map_err(|_| corrupt(operation, "component length is not representable"))?;
    check_component_size(stored_length, operation)?;
    if stored_length != document.len() {
        return Err(corrupt(
            operation,
            "component length does not match its stored evidence",
        ));
    }
    let stored_digest = fixed_bytes::<32>(stored_digest.to_vec(), operation, "component digest")?;
    if sha256(document) != stored_digest {
        return Err(corrupt(
            operation,
            "component digest does not match its stored bytes",
        ));
    }
    Ok(())
}

fn supported_revision(value: i64, current: u32, operation: &'static str) -> Result<u32> {
    if value > i64::from(current) {
        return Err(unsupported(
            operation,
            "component uses a future format revision",
        ));
    }
    if value != i64::from(current) {
        return Err(corrupt(
            operation,
            "component revision does not match project schema revision 1",
        ));
    }
    Ok(current)
}

pub(crate) fn check_component_size(bytes: usize, operation: &'static str) -> Result<()> {
    if bytes > MAX_COMPONENT_BYTES {
        return Err(resource_exhausted(
            operation,
            "project component exceeds the stable schema size limit",
        ));
    }
    Ok(())
}

pub(crate) fn parse_revision(value: &str, operation: &'static str) -> Result<u64> {
    let revision = value
        .parse::<u64>()
        .map_err(|_| corrupt(operation, "stored revision is not an unsigned integer"))?;
    if revision.to_string() != value {
        return Err(corrupt(
            operation,
            "stored revision is not in canonical decimal form",
        ));
    }
    Ok(revision)
}

pub(crate) fn fixed_bytes<const N: usize>(
    bytes: Vec<u8>,
    operation: &'static str,
    field: &'static str,
) -> Result<[u8; N]> {
    bytes.try_into().map_err(|_| {
        corrupt(operation, "stored binary field has an invalid length")
            .with_context(ErrorContext::new(COMPONENT, operation).with_field("field", field))
    })
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn manifest_digest(project: &PreparedProject) -> [u8; 32] {
    let mut hasher = Sha256::new();
    manifest_field(&mut hasher, MANIFEST_DOMAIN);
    manifest_field(&mut hasher, PROJECT_FORMAT.as_bytes());
    manifest_field(&mut hasher, PROJECT_FORMAT_VERSION.as_bytes());
    manifest_field(&mut hasher, &PROJECT_SCHEMA_REVISION.to_be_bytes());
    manifest_field(&mut hasher, &STABLE_PRIMITIVE_SCHEMA_REVISION.to_be_bytes());
    manifest_field(&mut hasher, &project.project_id.to_bytes());
    manifest_field(&mut hasher, &project.revision.to_be_bytes());
    manifest_field(&mut hasher, &project.root_timeline_id.to_bytes());
    manifest_field(&mut hasher, &TIMELINE_STATE_FORMAT_REVISION.to_be_bytes());
    manifest_field(
        &mut hasher,
        &(project.timeline_document.len() as u64).to_be_bytes(),
    );
    manifest_field(&mut hasher, &project.timeline_digest);
    manifest_field(&mut hasher, &(project.graphs.len() as u64).to_be_bytes());
    for graph in &project.graphs {
        manifest_field(&mut hasher, &graph.graph_id.to_bytes());
        manifest_field(
            &mut hasher,
            &[match graph.kind {
                StoredGraphKind::Timeline => 1,
                StoredGraphKind::Standalone => 2,
            }],
        );
        match graph.root_timeline_id {
            Some(value) => {
                manifest_field(&mut hasher, &[1]);
                manifest_field(&mut hasher, &value.to_bytes());
            }
            None => manifest_field(&mut hasher, &[0]),
        }
        match graph.name.as_deref() {
            Some(value) => {
                manifest_field(&mut hasher, &[1]);
                manifest_field(&mut hasher, value.as_bytes());
            }
            None => manifest_field(&mut hasher, &[0]),
        }
        manifest_field(&mut hasher, &graph.revision.to_be_bytes());
        manifest_field(&mut hasher, &GRAPH_DOCUMENT_FORMAT_REVISION.to_be_bytes());
        manifest_field(&mut hasher, &(graph.document.len() as u64).to_be_bytes());
        manifest_field(&mut hasher, &graph.digest);
    }
    hasher.finalize().into()
}

fn manifest_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn create_path_error(source: std::io::Error, operation: &'static str) -> Error {
    let (category, recoverability, message) = match source.kind() {
        std::io::ErrorKind::AlreadyExists => (
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "project database path already exists",
        ),
        std::io::ErrorKind::NotFound => (
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "project database parent path does not exist",
        ),
        std::io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
            "project database path is not writable",
        ),
        _ => (
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "project database path could not be reserved",
        ),
    };
    Error::with_source(category, recoverability, message, source)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

pub(crate) fn database_error(source: rusqlite::Error, operation: &'static str) -> Error {
    let (category, recoverability, message) = match &source {
        rusqlite::Error::SqliteFailure(failure, _) => match failure.code {
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => (
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "project database is busy",
            ),
            ErrorCode::PermissionDenied
            | ErrorCode::ReadOnly
            | ErrorCode::AuthorizationForStatementDenied => (
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "project database operation is not permitted",
            ),
            ErrorCode::CannotOpen => (
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "project database could not be opened",
            ),
            ErrorCode::OutOfMemory | ErrorCode::DiskFull | ErrorCode::TooBig => (
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "project database resource limit was reached",
            ),
            ErrorCode::DatabaseCorrupt
            | ErrorCode::NotADatabase
            | ErrorCode::TypeMismatch
            | ErrorCode::ConstraintViolation => (
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "project database is corrupt or inconsistent",
            ),
            _ => (
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "project database operation failed",
            ),
        },
        rusqlite::Error::QueryReturnedNoRows
        | rusqlite::Error::InvalidColumnType(..)
        | rusqlite::Error::IntegralValueOutOfRange(..)
        | rusqlite::Error::FromSqlConversionFailure(..) => (
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "project database row is missing or malformed",
        ),
        _ => (
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "project database integration failed",
        ),
    };
    Error::with_source(category, recoverability, message, source)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

pub(crate) fn stored_state_error(source: Error, operation: &'static str) -> Error {
    Error::with_source(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        "stored project component failed checked reconstruction",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

pub(crate) fn project_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

pub(crate) fn corrupt(operation: &'static str, message: &'static str) -> Error {
    project_error(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}

pub(crate) fn unsupported(operation: &'static str, message: &'static str) -> Error {
    project_error(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}

fn resource_exhausted(operation: &'static str, message: &'static str) -> Error {
    project_error(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}
