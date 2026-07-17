//! Stable, explicitly versioned whole-project SQLite serialization.
//!
//! This module owns the schema and connection-level interpretation of one
//! `.superi` application database. Migration policy is implemented by the
//! private sibling module. Destination publication, autosave, and recovery
//! policy remain separate project concerns.

use std::fmt;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use rusqlite::config::DbConfig;
use rusqlite::{params, Connection, ErrorCode, OpenFlags, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use superi_audio::mixing::ClipMixState;
use superi_audio::serialize::{
    deserialize_clip_mix_state, serialize_clip_mix_state, CLIP_MIX_FORMAT_REVISION,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{GraphId, ProjectId, TimelineId};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::{
    CapabilityId, CapabilitySet, ComponentId, SemanticVersion, VersionIdentifier,
};
use superi_graph::serialize::{deserialize_graph, serialize_graph, GRAPH_DOCUMENT_FORMAT_REVISION};
use superi_timeline::compile::CompiledTimelineGraphValue;
use superi_timeline::serialize::{
    deserialize_timeline_state, serialize_timeline_state, TIMELINE_STATE_FORMAT_REVISION,
};

use crate::document::{ProjectDocument, ProjectGraph, ProjectSnapshot, StandaloneProjectGraph};
use crate::extensions::{
    ProjectExtensionFailure, ProjectExtensionKind, ProjectExtensionLifecycle,
    ProjectExtensionRecord, ProjectExtensionRecordId, MAX_PROJECT_EXTENSION_PAYLOAD_BYTES,
    MAX_PROJECT_EXTENSION_RECORDS,
};
use crate::migrate::migrate_connection;
use crate::save::ProjectSaveCommand;
use crate::settings::{ProjectSettings, PROJECT_SETTINGS_FORMAT_REVISION};

const COMPONENT: &str = "superi-project.persistence";
const MANIFEST_DOMAIN_V1: &[u8] = b"superi.project.manifest.v1";
const MANIFEST_DOMAIN_V2: &[u8] = b"superi.project.manifest.v2";
const MANIFEST_DOMAIN_V3: &[u8] = b"superi.project.manifest.v3";
const MANIFEST_DOMAIN_V4: &[u8] = b"superi.project.manifest.v4";
pub(crate) const MAX_COMPONENT_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_SETTINGS_COMPONENT_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_EXTENSION_METADATA_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_GRAPH_COUNT: usize = 4096;
pub(crate) const MAX_STANDALONE_NAME_BYTES: usize = 16 * 1024;

/// SQLite application identity stored in every `.superi` database (`SUPR`).
pub const PROJECT_APPLICATION_ID: u32 = 0x5355_5052;
/// Oldest project database schema with a registered lossless forward migration.
pub const PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION: u32 = 0;
/// Current monotonic project database schema revision.
pub const PROJECT_SCHEMA_REVISION: u32 = 4;
/// Stable semantic identity of the whole-project format.
pub const PROJECT_FORMAT: &str = "superi.project";
/// Current semantic project format version.
pub const PROJECT_FORMAT_VERSION: &str = "1.3.0";
pub(crate) const PROJECT_FORMAT_VERSION_SCHEMA_THREE: &str = "1.2.0";
pub(crate) const PROJECT_FORMAT_VERSION_SCHEMA_TWO: &str = "1.1.0";
pub(crate) const PROJECT_FORMAT_VERSION_SCHEMA_ONE: &str = "1.0.0";

pub(crate) const PROJECT_METADATA_SCHEMA: &str = "CREATE TABLE project_metadata (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format TEXT NOT NULL CHECK (format = 'superi.project'), format_version TEXT NOT NULL, primitive_schema_revision INTEGER NOT NULL CHECK (primitive_schema_revision > 0), project_id BLOB NOT NULL CHECK (length(project_id) = 16), document_revision TEXT NOT NULL, root_timeline_id BLOB NOT NULL CHECK (length(root_timeline_id) = 16), manifest_sha256 BLOB NOT NULL CHECK (length(manifest_sha256) = 32)) STRICT";
pub(crate) const TIMELINE_COMPONENT_SCHEMA: &str = "CREATE TABLE timeline_component (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format_revision INTEGER NOT NULL CHECK (format_revision > 0), byte_length INTEGER NOT NULL CHECK (byte_length >= 0 AND byte_length <= 67108864), sha256 BLOB NOT NULL CHECK (length(sha256) = 32), document BLOB NOT NULL CHECK (length(document) = byte_length)) STRICT";
pub(crate) const GRAPH_COMPONENTS_SCHEMA: &str = "CREATE TABLE graph_components (graph_id BLOB PRIMARY KEY CHECK (length(graph_id) = 16), graph_kind TEXT NOT NULL CHECK (graph_kind IN ('timeline', 'standalone')), root_timeline_id BLOB CHECK (root_timeline_id IS NULL OR length(root_timeline_id) = 16), name TEXT, graph_revision TEXT NOT NULL, format_revision INTEGER NOT NULL CHECK (format_revision > 0), byte_length INTEGER NOT NULL CHECK (byte_length >= 0 AND byte_length <= 67108864), sha256 BLOB NOT NULL CHECK (length(sha256) = 32), document BLOB NOT NULL CHECK (length(document) = byte_length), CHECK ((graph_kind = 'timeline' AND root_timeline_id IS NOT NULL AND name IS NULL) OR (graph_kind = 'standalone' AND root_timeline_id IS NULL AND name IS NOT NULL AND length(name) > 0))) STRICT, WITHOUT ROWID";
pub(crate) const SETTINGS_COMPONENT_SCHEMA: &str = "CREATE TABLE settings_component (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format_revision INTEGER NOT NULL CHECK (format_revision >= 1), byte_length INTEGER NOT NULL CHECK (byte_length >= 0 AND byte_length <= 1048576), sha256 BLOB NOT NULL CHECK (length(sha256) = 32), document BLOB NOT NULL CHECK (length(document) = byte_length)) STRICT";
pub(crate) const AUDIO_COMPONENT_SCHEMA: &str = "CREATE TABLE audio_component (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format_revision INTEGER NOT NULL CHECK (format_revision > 0), byte_length INTEGER NOT NULL CHECK (byte_length >= 0 AND byte_length <= 67108864), sha256 BLOB NOT NULL CHECK (length(sha256) = 32), document BLOB NOT NULL CHECK (length(document) = byte_length)) STRICT";
pub(crate) const EXTENSION_RECORDS_SCHEMA: &str = "CREATE TABLE extension_records (extension_id TEXT NOT NULL CHECK (length(extension_id) > 0), record_id TEXT NOT NULL CHECK (length(record_id) > 0 AND length(record_id) <= 128), metadata_format_revision INTEGER NOT NULL CHECK (metadata_format_revision = 1), metadata_byte_length INTEGER NOT NULL CHECK (metadata_byte_length >= 0 AND metadata_byte_length <= 8388608), metadata_sha256 BLOB NOT NULL CHECK (length(metadata_sha256) = 32), metadata BLOB NOT NULL CHECK (length(metadata) = metadata_byte_length), payload_byte_length INTEGER NOT NULL CHECK (payload_byte_length >= 0 AND payload_byte_length <= 67108864), payload_sha256 BLOB NOT NULL CHECK (length(payload_sha256) = 32), payload BLOB NOT NULL CHECK (length(payload) = payload_byte_length), PRIMARY KEY (extension_id, record_id)) STRICT, WITHOUT ROWID";

/// Current canonical metadata representation for one opaque extension record.
pub const PROJECT_EXTENSION_METADATA_FORMAT_REVISION: u32 = 1;

enum ProjectDatabaseStorage {
    File { active_path: PathBuf },
    Memory { connection: Connection },
}

/// One secured authority over a stable whole-project database.
pub struct ProjectDatabase {
    storage: ProjectDatabaseStorage,
    writable: bool,
    source_schema_revision: u32,
}

impl fmt::Debug for ProjectDatabase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectDatabase")
            .field("active_path", &self.active_path())
            .field("writable", &self.writable)
            .field("source_schema_revision", &self.source_schema_revision)
            .finish_non_exhaustive()
    }
}

impl ProjectDatabase {
    /// Creates a new current-schema database without replacing an existing path.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|source| create_path_error(source, "create_database"))?;

        let connection = match open_file_connection(path, true) {
            Ok(connection) => connection,
            Err(error) => {
                let _ = std::fs::remove_file(path);
                return Err(error);
            }
        };
        if let Err(error) = initialize_schema(&connection) {
            drop(connection);
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        if let Err(error) = close_connection(connection, "close_created_database") {
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        let active_path = canonicalize_project_path(path, "canonicalize_created_database")?;
        Ok(Self {
            storage: ProjectDatabaseStorage::File { active_path },
            writable: true,
            source_schema_revision: PROJECT_SCHEMA_REVISION,
        })
    }

    /// Creates a secured in-memory current-schema database.
    pub fn memory() -> Result<Self> {
        let connection = Connection::open_in_memory()
            .map_err(|source| database_error(source, "open_memory_database"))?;
        initialize_connection(&connection, true)?;
        initialize_schema(&connection)?;
        Ok(Self {
            storage: ProjectDatabaseStorage::Memory { connection },
            writable: true,
            source_schema_revision: PROJECT_SCHEMA_REVISION,
        })
    }

    /// Opens an existing project with write authority and migrates supported schemas.
    ///
    /// Current projects are validated without mutation. Every registered older
    /// schema is upgraded to the current schema in one immediate transaction.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let active_path =
            canonicalize_project_path(path.as_ref(), "canonicalize_writable_database")?;
        let mut connection = open_file_connection(&active_path, true)?;
        let source_schema_revision = migrate_connection(&mut connection)?;
        close_connection(connection, "close_writable_database")?;
        Ok(Self {
            storage: ProjectDatabaseStorage::File { active_path },
            writable: true,
            source_schema_revision,
        })
    }

    /// Opens an existing `.superi` database without write authority.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        let active_path =
            canonicalize_project_path(path.as_ref(), "canonicalize_read_only_database")?;
        let connection = open_file_connection(&active_path, false)?;
        validate_identity_and_schema(&connection)?;
        close_connection(connection, "close_read_only_database")?;
        Ok(Self {
            storage: ProjectDatabaseStorage::File { active_path },
            writable: false,
            source_schema_revision: PROJECT_SCHEMA_REVISION,
        })
    }

    /// Returns the absolute path that currently owns project-relative state.
    #[must_use]
    pub fn active_path(&self) -> Option<&Path> {
        match &self.storage {
            ProjectDatabaseStorage::File { active_path } => Some(active_path),
            ProjectDatabaseStorage::Memory { .. } => None,
        }
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

        if matches!(self.storage, ProjectDatabaseStorage::File { .. }) {
            return self
                .execute_save_command(ProjectSaveCommand::Save, snapshot)
                .map(|_| ());
        }

        let ProjectDatabaseStorage::Memory { connection } = &mut self.storage else {
            unreachable!("file-backed storage returned before memory replacement")
        };
        let prepared = PreparedProject::from_snapshot(snapshot)?;
        let transaction = connection
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
        match &self.storage {
            ProjectDatabaseStorage::Memory { connection } => load_from_connection(connection),
            ProjectDatabaseStorage::File { active_path } => {
                let connection = open_file_connection(active_path, false)?;
                let loaded = load_from_connection(&connection);
                let closed = close_connection(connection, "close_loaded_database");
                match (loaded, closed) {
                    (Ok(document), Ok(())) => Ok(document),
                    (Err(error), _) | (Ok(_), Err(error)) => Err(error),
                }
            }
        }
    }

    pub(crate) const fn is_writable(&self) -> bool {
        self.writable
    }

    pub(crate) fn rebind_after_save_as(&mut self, active_path: PathBuf) {
        self.storage = ProjectDatabaseStorage::File { active_path };
        self.writable = true;
    }
}

fn load_from_connection(connection: &Connection) -> Result<ProjectDocument> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(|source| database_error(source, "begin_load_project"))?;
    let document = load_connection(&transaction)?;
    transaction
        .commit()
        .map_err(|source| database_error(source, "finish_load_project"))?;
    Ok(document)
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
    settings_document: Vec<u8>,
    settings_digest: [u8; 32],
    audio_document: Vec<u8>,
    audio_digest: [u8; 32],
    extensions: Vec<PreparedExtension>,
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

struct PreparedExtension {
    extension_id: String,
    record_id: String,
    metadata_document: Vec<u8>,
    metadata_digest: [u8; 32],
    payload: Vec<u8>,
    payload_digest: [u8; 32],
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExtensionMetadataV1 {
    extension_id: String,
    record_id: String,
    extension_version: String,
    kind: String,
    payload_schema: String,
    requested_capabilities: Vec<String>,
    granted_capabilities: Vec<String>,
    lifecycle: String,
    failure: Option<ExtensionFailureV1>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExtensionFailureV1 {
    category: String,
    recoverability: String,
    message: String,
    contexts: Vec<ExtensionFailureContextV1>,
    total_failures: u64,
    consecutive_failures: u32,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExtensionFailureContextV1 {
    component: String,
    operation: String,
    fields: std::collections::BTreeMap<String, String>,
}

impl PreparedProject {
    pub(crate) fn from_snapshot(snapshot: &ProjectSnapshot) -> Result<Self> {
        let timeline_document = serialize_timeline_state(snapshot.editorial_project())?;
        check_component_size(timeline_document.len(), "encode_timeline_component")?;
        let timeline_digest = sha256(&timeline_document);
        let settings_document = serde_json::to_vec(snapshot.settings().snapshot())
            .map_err(|source| settings_json_error(source, "encode_settings_component", false))?;
        check_settings_component_size(settings_document.len(), "encode_settings_component")?;
        let settings_digest = sha256(&settings_document);
        let audio_document = serialize_clip_mix_state(snapshot.clip_mix_state())?;
        check_component_size(audio_document.len(), "encode_audio_component")?;
        let audio_digest = sha256(&audio_document);

        if snapshot.extension_records().len() > MAX_PROJECT_EXTENSION_RECORDS {
            return Err(resource_exhausted(
                "encode_extension_records",
                "project extension record count exceeds the stable schema limit",
            ));
        }
        let extensions = snapshot
            .extension_records()
            .values()
            .map(prepare_extension_record)
            .collect::<Result<Vec<_>>>()?;

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
            settings_document,
            settings_digest,
            audio_document,
            audio_digest,
            extensions,
            graphs,
            manifest_digest: [0; 32],
        };
        prepared.manifest_digest = manifest_digest(&prepared);
        Ok(prepared)
    }
}

fn prepare_extension_record(record: &ProjectExtensionRecord) -> Result<PreparedExtension> {
    let metadata_document = encode_extension_metadata(record)?;
    if record.payload().len() > MAX_PROJECT_EXTENSION_PAYLOAD_BYTES {
        return Err(resource_exhausted(
            "encode_extension_payload",
            "project extension payload exceeds the stable schema limit",
        ));
    }
    Ok(PreparedExtension {
        extension_id: record.key().extension_id().to_string(),
        record_id: record.key().record_id().to_string(),
        metadata_digest: sha256(&metadata_document),
        metadata_document,
        payload: record.payload().to_vec(),
        payload_digest: sha256(record.payload()),
    })
}

fn encode_extension_metadata(record: &ProjectExtensionRecord) -> Result<Vec<u8>> {
    let failure = record.failure().map(|failure| ExtensionFailureV1 {
        category: failure.category().code().to_owned(),
        recoverability: failure.recoverability().code().to_owned(),
        message: failure.message().to_owned(),
        contexts: failure
            .contexts()
            .iter()
            .map(|context| ExtensionFailureContextV1 {
                component: context.component().to_owned(),
                operation: context.operation().to_owned(),
                fields: context.fields().clone(),
            })
            .collect(),
        total_failures: failure.total_failures(),
        consecutive_failures: failure.consecutive_failures(),
    });
    let metadata = ExtensionMetadataV1 {
        extension_id: record.key().extension_id().to_string(),
        record_id: record.key().record_id().to_string(),
        extension_version: record.extension_version().to_string(),
        kind: record.kind().as_str().to_owned(),
        payload_schema: record.payload_schema().to_string(),
        requested_capabilities: record
            .requested_capabilities()
            .iter()
            .map(ToString::to_string)
            .collect(),
        granted_capabilities: record
            .granted_capabilities()
            .iter()
            .map(ToString::to_string)
            .collect(),
        lifecycle: record.lifecycle().code().to_owned(),
        failure,
    };
    let document = serde_json::to_vec(&metadata).map_err(|source| {
        Error::with_source(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "extension metadata JSON encoding failed",
            source,
        )
        .with_context(ErrorContext::new(COMPONENT, "encode_extension_metadata"))
    })?;
    check_extension_metadata_size(document.len(), "encode_extension_metadata")?;
    Ok(document)
}

fn decode_extension_record(prepared: &PreparedExtension) -> Result<ProjectExtensionRecord> {
    let metadata: ExtensionMetadataV1 = serde_json::from_slice(&prepared.metadata_document)
        .map_err(|source| {
            Error::with_source(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "extension metadata JSON is invalid",
                source,
            )
            .with_context(ErrorContext::new(COMPONENT, "decode_extension_metadata"))
        })?;
    if metadata.extension_id != prepared.extension_id || metadata.record_id != prepared.record_id {
        return Err(corrupt(
            "decode_extension_metadata",
            "extension metadata identity does not match its database key",
        ));
    }

    let requested_capabilities = decode_capabilities(
        metadata.requested_capabilities,
        "decode_requested_capabilities",
    )?;
    let granted_capabilities =
        decode_capabilities(metadata.granted_capabilities, "decode_granted_capabilities")?;
    let failure = metadata
        .failure
        .map(|failure| {
            let category = ErrorCategory::from_code(&failure.category).ok_or_else(|| {
                corrupt(
                    "decode_extension_failure",
                    "extension failure uses an unknown category",
                )
            })?;
            let recoverability =
                Recoverability::from_code(&failure.recoverability).ok_or_else(|| {
                    corrupt(
                        "decode_extension_failure",
                        "extension failure uses an unknown recoverability code",
                    )
                })?;
            let contexts = failure.contexts.into_iter().map(|context| {
                let mut decoded = ErrorContext::new(context.component, context.operation);
                for (key, value) in context.fields {
                    decoded.insert_field(key, value);
                }
                decoded
            });
            ProjectExtensionFailure::new(
                category,
                recoverability,
                failure.message,
                contexts,
                failure.total_failures,
                failure.consecutive_failures,
            )
            .map_err(|source| stored_state_error(source, "decode_extension_failure"))
        })
        .transpose()?;

    let extension_id = ComponentId::new(&metadata.extension_id).map_err(|_| {
        corrupt(
            "decode_extension_metadata",
            "extension identity is not canonical",
        )
    })?;
    let record_id = ProjectExtensionRecordId::new(metadata.record_id)
        .map_err(|source| stored_state_error(source, "decode_extension_record_id"))?;
    let extension_version =
        SemanticVersion::from_str(&metadata.extension_version).map_err(|_| {
            corrupt(
                "decode_extension_metadata",
                "extension version is not canonical semantic version text",
            )
        })?;
    let kind = ProjectExtensionKind::new(ComponentId::new(&metadata.kind).map_err(|_| {
        corrupt(
            "decode_extension_metadata",
            "extension kind is not canonical",
        )
    })?);
    let payload_schema = VersionIdentifier::from_str(&metadata.payload_schema).map_err(|_| {
        corrupt(
            "decode_extension_metadata",
            "extension payload schema is not canonical",
        )
    })?;
    let lifecycle = ProjectExtensionLifecycle::from_code(&metadata.lifecycle).ok_or_else(|| {
        corrupt(
            "decode_extension_metadata",
            "extension lifecycle uses an unknown code",
        )
    })?;
    let record = ProjectExtensionRecord::new(
        extension_id,
        record_id,
        extension_version,
        kind,
        payload_schema,
        requested_capabilities,
        granted_capabilities,
        lifecycle,
        failure,
        prepared.payload.clone(),
    )
    .map_err(|source| stored_state_error(source, "validate_extension_record"))?;
    if encode_extension_metadata(&record)? != prepared.metadata_document {
        return Err(corrupt(
            "decode_extension_metadata",
            "extension metadata is not canonical for the project schema",
        ));
    }
    Ok(record)
}

fn decode_capabilities(values: Vec<String>, operation: &'static str) -> Result<CapabilitySet> {
    let decoded = values
        .iter()
        .map(|value| {
            CapabilityId::new(value)
                .map_err(|_| corrupt(operation, "extension capability is not canonical"))
        })
        .collect::<Result<Vec<_>>>()?;
    let set = CapabilitySet::new(decoded);
    let canonical = set.iter().map(ToString::to_string).collect::<Vec<_>>();
    if canonical != values {
        return Err(corrupt(
            operation,
            "extension capabilities are duplicated or not in canonical order",
        ));
    }
    Ok(set)
}

pub(crate) fn open_file_connection(path: &Path, writable: bool) -> Result<Connection> {
    let access = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let connection = Connection::open_with_flags(path, access | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|source| {
            database_error(
                source,
                if writable {
                    "open_writable_database"
                } else {
                    "open_read_only_database"
                },
            )
            .with_context(
                ErrorContext::new(COMPONENT, "open_project_path")
                    .with_field("path", path.display().to_string()),
            )
        })?;
    initialize_connection(&connection, writable)?;
    Ok(connection)
}

pub(crate) fn close_connection(connection: Connection, operation: &'static str) -> Result<()> {
    connection
        .close()
        .map_err(|(_, source)| database_error(source, operation))
}

fn canonicalize_project_path(path: &Path, operation: &'static str) -> Result<PathBuf> {
    std::fs::canonicalize(path).map_err(|source| path_error(source, operation, path))
}

pub(crate) fn initialize_connection(connection: &Connection, writable: bool) -> Result<()> {
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
    let schema = format!(
        "{PROJECT_METADATA_SCHEMA};{TIMELINE_COMPONENT_SCHEMA};{GRAPH_COMPONENTS_SCHEMA};{SETTINGS_COMPONENT_SCHEMA};{AUDIO_COMPONENT_SCHEMA};{EXTENSION_RECORDS_SCHEMA};"
    );
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

pub(crate) fn initialize_schema_one(connection: &Connection) -> Result<()> {
    let schema =
        format!("{PROJECT_METADATA_SCHEMA};{TIMELINE_COMPONENT_SCHEMA};{GRAPH_COMPONENTS_SCHEMA};");
    connection
        .execute_batch(&schema)
        .map_err(|source| database_error(source, "create_schema_one_project_schema"))?;
    connection
        .pragma_update(None, "application_id", i64::from(PROJECT_APPLICATION_ID))
        .map_err(|source| database_error(source, "set_schema_one_application_id"))?;
    connection
        .pragma_update(None, "user_version", 1_i64)
        .map_err(|source| database_error(source, "set_schema_one_revision"))?;
    validate_schema_one_identity_and_schema(connection)
}

pub(crate) fn initialize_schema_two(connection: &Connection) -> Result<()> {
    let schema = format!(
        "{PROJECT_METADATA_SCHEMA};{TIMELINE_COMPONENT_SCHEMA};{GRAPH_COMPONENTS_SCHEMA};{SETTINGS_COMPONENT_SCHEMA};"
    );
    connection
        .execute_batch(&schema)
        .map_err(|source| database_error(source, "create_schema_two_project_schema"))?;
    connection
        .pragma_update(None, "application_id", i64::from(PROJECT_APPLICATION_ID))
        .map_err(|source| database_error(source, "set_schema_two_application_id"))?;
    connection
        .pragma_update(None, "user_version", 2_i64)
        .map_err(|source| database_error(source, "set_schema_two_revision"))?;
    validate_schema_two_identity_and_schema(connection)
}

pub(crate) fn initialize_schema_three(connection: &Connection) -> Result<()> {
    let schema = format!(
        "{PROJECT_METADATA_SCHEMA};{TIMELINE_COMPONENT_SCHEMA};{GRAPH_COMPONENTS_SCHEMA};{SETTINGS_COMPONENT_SCHEMA};{AUDIO_COMPONENT_SCHEMA};"
    );
    connection
        .execute_batch(&schema)
        .map_err(|source| database_error(source, "create_schema_three_project_schema"))?;
    connection
        .pragma_update(None, "application_id", i64::from(PROJECT_APPLICATION_ID))
        .map_err(|source| database_error(source, "set_schema_three_application_id"))?;
    connection
        .pragma_update(None, "user_version", 3_i64)
        .map_err(|source| database_error(source, "set_schema_three_revision"))?;
    validate_schema_three_identity_and_schema(connection)
}

pub(crate) fn validate_identity_and_schema(connection: &Connection) -> Result<()> {
    validate_schema(connection, PROJECT_SCHEMA_REVISION, true, true, true, true)
}

pub(crate) fn validate_schema_one_identity_and_schema(connection: &Connection) -> Result<()> {
    validate_schema(connection, 1, false, false, false, false)
}

pub(crate) fn validate_schema_two_identity_and_schema(connection: &Connection) -> Result<()> {
    validate_schema(connection, 2, true, false, false, false)
}

pub(crate) fn validate_schema_three_identity_and_schema(connection: &Connection) -> Result<()> {
    validate_schema(connection, 3, true, true, false, false)
}

fn validate_schema(
    connection: &Connection,
    expected_revision: u32,
    include_settings: bool,
    include_audio: bool,
    include_extensions: bool,
    require_current: bool,
) -> Result<()> {
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
    if require_current
        && schema_revision >= 0
        && schema_revision < i64::from(PROJECT_SCHEMA_REVISION)
        && schema_revision >= i64::from(PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION)
    {
        return Err(unsupported(
            "read_schema_revision",
            "project database requires writable migration",
        ));
    }
    if schema_revision != i64::from(expected_revision) {
        return Err(corrupt(
            "read_schema_revision",
            "project database does not declare the expected schema revision",
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
    let mut expected = vec![
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
    ];
    if include_audio {
        expected.insert(
            0,
            (
                "table".to_owned(),
                "audio_component".to_owned(),
                AUDIO_COMPONENT_SCHEMA.to_owned(),
            ),
        );
    }
    if include_extensions {
        expected.insert(
            usize::from(include_audio),
            (
                "table".to_owned(),
                "extension_records".to_owned(),
                EXTENSION_RECORDS_SCHEMA.to_owned(),
            ),
        );
    }
    if include_settings {
        expected.push((
            "table".to_owned(),
            "settings_component".to_owned(),
            SETTINGS_COMPONENT_SCHEMA.to_owned(),
        ));
    }
    expected.push((
        "table".to_owned(),
        "timeline_component".to_owned(),
        TIMELINE_COMPONENT_SCHEMA.to_owned(),
    ));
    if actual != expected {
        return Err(corrupt(
            "inspect_project_schema",
            "project database schema objects do not match the expected schema revision",
        ));
    }
    Ok(())
}

pub(crate) fn validate_full_integrity(connection: &Connection) -> Result<()> {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))
        .map_err(|source| database_error(source, "integrity_check"))?;
    if integrity != "ok" {
        return Err(corrupt(
            "integrity_check",
            "SQLite full integrity check failed",
        ));
    }

    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|source| database_error(source, "foreign_key_check"))?;
    let mut rows = statement
        .query([])
        .map_err(|source| database_error(source, "foreign_key_check"))?;
    if rows
        .next()
        .map_err(|source| database_error(source, "foreign_key_check"))?
        .is_some()
    {
        return Err(corrupt(
            "foreign_key_check",
            "SQLite foreign key consistency check failed",
        ));
    }
    Ok(())
}

pub(crate) fn write_prepared_project(
    connection: &Connection,
    prepared: &PreparedProject,
) -> Result<()> {
    write_prepared_project_versioned(
        connection,
        prepared,
        PROJECT_FORMAT_VERSION,
        prepared.manifest_digest,
        true,
        true,
        true,
    )
}

pub(crate) fn write_schema_three_project(
    connection: &Connection,
    prepared: &PreparedProject,
) -> Result<()> {
    write_prepared_project_versioned(
        connection,
        prepared,
        PROJECT_FORMAT_VERSION_SCHEMA_THREE,
        manifest_digest_v3(prepared),
        true,
        true,
        false,
    )
}

pub(crate) fn write_schema_two_project(
    connection: &Connection,
    prepared: &PreparedProject,
) -> Result<()> {
    write_prepared_project_versioned(
        connection,
        prepared,
        PROJECT_FORMAT_VERSION_SCHEMA_TWO,
        manifest_digest_v2(prepared),
        true,
        false,
        false,
    )
}

fn write_prepared_project_versioned(
    connection: &Connection,
    prepared: &PreparedProject,
    format_version: &str,
    manifest_digest: [u8; 32],
    include_settings: bool,
    include_audio: bool,
    include_extensions: bool,
) -> Result<()> {
    let clear = match (include_settings, include_audio, include_extensions) {
        (true, true, true) => {
            "DELETE FROM extension_records;\
             DELETE FROM audio_component;\
             DELETE FROM graph_components;\
             DELETE FROM settings_component;\
             DELETE FROM timeline_component;\
             DELETE FROM project_metadata;"
        }
        (true, true, false) => {
            "DELETE FROM audio_component;\
             DELETE FROM graph_components;\
             DELETE FROM settings_component;\
             DELETE FROM timeline_component;\
             DELETE FROM project_metadata;"
        }
        (true, false, false) => {
            "DELETE FROM graph_components;\
             DELETE FROM settings_component;\
             DELETE FROM timeline_component;\
             DELETE FROM project_metadata;"
        }
        (false, false, false) => {
            "DELETE FROM graph_components;\
             DELETE FROM timeline_component;\
             DELETE FROM project_metadata;"
        }
        _ => unreachable!("audio and extension project schemas also retain earlier components"),
    };
    connection
        .execute_batch(clear)
        .map_err(|source| database_error(source, "clear_project_rows"))?;
    connection
        .execute(
            "INSERT INTO project_metadata \
             (singleton, format, format_version, primitive_schema_revision, project_id, \
              document_revision, root_timeline_id, manifest_sha256) \
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                PROJECT_FORMAT,
                format_version,
                i64::from(STABLE_PRIMITIVE_SCHEMA_REVISION),
                prepared.project_id.to_bytes().as_slice(),
                prepared.revision.to_string(),
                prepared.root_timeline_id.to_bytes().as_slice(),
                manifest_digest.as_slice(),
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
    if include_settings {
        connection
            .execute(
                "INSERT INTO settings_component \
                 (singleton, format_revision, byte_length, sha256, document) \
                 VALUES (1, ?1, ?2, ?3, ?4)",
                params![
                    i64::from(PROJECT_SETTINGS_FORMAT_REVISION),
                    prepared.settings_document.len() as i64,
                    prepared.settings_digest.as_slice(),
                    prepared.settings_document.as_slice(),
                ],
            )
            .map_err(|source| database_error(source, "write_settings_component"))?;
    }
    if include_audio {
        connection
            .execute(
                "INSERT INTO audio_component \
                 (singleton, format_revision, byte_length, sha256, document) \
                 VALUES (1, ?1, ?2, ?3, ?4)",
                params![
                    i64::from(CLIP_MIX_FORMAT_REVISION),
                    prepared.audio_document.len() as i64,
                    prepared.audio_digest.as_slice(),
                    prepared.audio_document.as_slice(),
                ],
            )
            .map_err(|source| database_error(source, "write_audio_component"))?;
    }
    if include_extensions {
        let mut statement = connection
            .prepare(
                "INSERT INTO extension_records \
                 (extension_id, record_id, metadata_format_revision, metadata_byte_length, \
                  metadata_sha256, metadata, payload_byte_length, payload_sha256, payload) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .map_err(|source| database_error(source, "prepare_extension_records"))?;
        for extension in &prepared.extensions {
            statement
                .execute(params![
                    extension.extension_id,
                    extension.record_id,
                    i64::from(PROJECT_EXTENSION_METADATA_FORMAT_REVISION),
                    extension.metadata_document.len() as i64,
                    extension.metadata_digest.as_slice(),
                    extension.metadata_document.as_slice(),
                    extension.payload.len() as i64,
                    extension.payload_digest.as_slice(),
                    extension.payload.as_slice(),
                ])
                .map_err(|source| database_error(source, "write_extension_record"))?;
        }
    }

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

pub(crate) fn write_schema_one_project(
    connection: &Connection,
    prepared: &PreparedProject,
) -> Result<()> {
    connection
        .execute_batch(
            "DELETE FROM graph_components;\
             DELETE FROM timeline_component;\
             DELETE FROM project_metadata;",
        )
        .map_err(|source| database_error(source, "clear_schema_one_project_rows"))?;
    connection
        .execute(
            "INSERT INTO project_metadata \
             (singleton, format, format_version, primitive_schema_revision, project_id, \
              document_revision, root_timeline_id, manifest_sha256) \
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                PROJECT_FORMAT,
                PROJECT_FORMAT_VERSION_SCHEMA_ONE,
                i64::from(STABLE_PRIMITIVE_SCHEMA_REVISION),
                prepared.project_id.to_bytes().as_slice(),
                prepared.revision.to_string(),
                prepared.root_timeline_id.to_bytes().as_slice(),
                manifest_digest_v1(prepared).as_slice(),
            ],
        )
        .map_err(|source| database_error(source, "write_schema_one_project_metadata"))?;
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
        .map_err(|source| database_error(source, "write_schema_one_timeline_component"))?;
    write_graph_rows(connection, prepared, "prepare_schema_one_graph_components")
}

fn write_graph_rows(
    connection: &Connection,
    prepared: &PreparedProject,
    prepare_operation: &'static str,
) -> Result<()> {
    let mut statement = connection
        .prepare(
            "INSERT INTO graph_components \
             (graph_id, graph_kind, root_timeline_id, name, graph_revision, format_revision, \
              byte_length, sha256, document) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .map_err(|source| database_error(source, prepare_operation))?;
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
            .map_err(|source| database_error(source, "write_schema_one_graph_component"))?;
    }
    Ok(())
}

pub(crate) fn load_connection(connection: &Connection) -> Result<ProjectDocument> {
    validate_identity_and_schema(connection)?;
    load_checked_connection(connection, PROJECT_SCHEMA_REVISION, true, true, true)
}

pub(crate) fn load_schema_three_connection(connection: &Connection) -> Result<ProjectDocument> {
    validate_schema_three_identity_and_schema(connection)?;
    load_checked_connection(connection, 3, true, true, false)
}

pub(crate) fn load_schema_two_connection(connection: &Connection) -> Result<ProjectDocument> {
    validate_schema_two_identity_and_schema(connection)?;
    load_checked_connection(connection, 2, true, false, false)
}

pub(crate) fn load_schema_one_connection(connection: &Connection) -> Result<ProjectDocument> {
    validate_schema_one_identity_and_schema(connection)?;
    load_checked_connection(connection, 1, false, false, false)
}

fn load_checked_connection(
    connection: &Connection,
    schema_revision: u32,
    includes_settings: bool,
    include_audio: bool,
    include_extensions: bool,
) -> Result<ProjectDocument> {
    require_row_count(connection, "project_metadata", 1)?;
    require_row_count(connection, "timeline_component", 1)?;
    if includes_settings {
        require_row_count(connection, "settings_component", 1)?;
    }
    if include_audio {
        require_row_count(connection, "audio_component", 1)?;
    }

    let extension_count = if include_extensions {
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM extension_records", [], |row| {
                row.get(0)
            })
            .map_err(|source| database_error(source, "count_extension_records"))?;
        let count = usize::try_from(count).map_err(|_| {
            corrupt(
                "count_extension_records",
                "project extension record count is not representable",
            )
        })?;
        if count > MAX_PROJECT_EXTENSION_RECORDS {
            return Err(corrupt(
                "count_extension_records",
                "project extension record count exceeds the stable schema limit",
            ));
        }
        count
    } else {
        0
    };

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
    let expected_format_version = match schema_revision {
        1 => PROJECT_FORMAT_VERSION_SCHEMA_ONE,
        2 => PROJECT_FORMAT_VERSION_SCHEMA_TWO,
        3 => PROJECT_FORMAT_VERSION_SCHEMA_THREE,
        PROJECT_SCHEMA_REVISION => PROJECT_FORMAT_VERSION,
        _ => {
            return Err(corrupt(
                "read_project_metadata",
                "project loader received an unsupported schema revision",
            ));
        }
    };
    if metadata.0 != PROJECT_FORMAT || metadata.1 != expected_format_version {
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
            "project primitive revision does not match the database schema",
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

    let settings = if includes_settings {
        let row = connection
            .query_row(
                "SELECT format_revision, byte_length, sha256, document \
                 FROM settings_component WHERE singleton = 1",
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
            .map_err(|source| database_error(source, "read_settings_component"))?;
        let format_revision = supported_revision(
            row.0,
            PROJECT_SETTINGS_FORMAT_REVISION,
            "read_settings_component",
        )?;
        validate_settings_component(row.1, &row.2, &row.3, "read_settings_component")?;
        Some((
            format_revision,
            row.3,
            fixed_bytes::<32>(row.2, "read_settings_component", "settings digest")?,
        ))
    } else {
        None
    };

    let (audio_document, audio_digest) = if include_audio {
        let audio = connection
            .query_row(
                "SELECT format_revision, byte_length, sha256, document \
                 FROM audio_component WHERE singleton = 1",
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
            .map_err(|source| database_error(source, "read_audio_component"))?;
        supported_revision(audio.0, CLIP_MIX_FORMAT_REVISION, "read_audio_component")?;
        validate_component(audio.1, &audio.2, &audio.3, "read_audio_component")?;
        let digest = fixed_bytes::<32>(audio.2, "read_audio_component", "audio digest")?;
        (audio.3, digest)
    } else {
        let document = serialize_clip_mix_state(&ClipMixState::new())?;
        let digest = sha256(&document);
        (document, digest)
    };

    let mut extensions = Vec::with_capacity(extension_count);
    if include_extensions {
        let mut statement = connection
            .prepare(
                "SELECT extension_id, record_id, metadata_format_revision, metadata_byte_length, \
                 metadata_sha256, metadata, payload_byte_length, payload_sha256, payload \
                 FROM extension_records ORDER BY extension_id, record_id",
            )
            .map_err(|source| database_error(source, "read_extension_records"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Vec<u8>>(7)?,
                    row.get::<_, Vec<u8>>(8)?,
                ))
            })
            .map_err(|source| database_error(source, "read_extension_records"))?;
        for row in rows {
            let row = row.map_err(|source| database_error(source, "read_extension_record"))?;
            supported_revision(
                row.2,
                PROJECT_EXTENSION_METADATA_FORMAT_REVISION,
                "read_extension_record",
            )?;
            validate_extension_metadata(row.3, &row.4, &row.5)?;
            validate_component(row.6, &row.7, &row.8, "read_extension_payload")?;
            extensions.push(PreparedExtension {
                extension_id: row.0,
                record_id: row.1,
                metadata_document: row.5,
                metadata_digest: fixed_bytes::<32>(
                    row.4,
                    "read_extension_record",
                    "extension metadata digest",
                )?,
                payload: row.8,
                payload_digest: fixed_bytes::<32>(
                    row.7,
                    "read_extension_record",
                    "extension payload digest",
                )?,
            });
        }
        if extensions.len() != extension_count {
            return Err(corrupt(
                "read_extension_records",
                "project extension record count changed during interpretation",
            ));
        }
    }

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
        settings_document: settings
            .as_ref()
            .map(|(_, document, _)| document.clone())
            .unwrap_or_default(),
        settings_digest: settings
            .as_ref()
            .map(|(_, _, digest)| *digest)
            .unwrap_or([0; 32]),
        audio_document,
        audio_digest,
        extensions,
        graphs,
        manifest_digest: stored_manifest,
    };
    let expected_manifest = match schema_revision {
        1 => manifest_digest_v1(&prepared),
        2 => manifest_digest_v2(&prepared),
        3 => manifest_digest_v3(&prepared),
        PROJECT_SCHEMA_REVISION => manifest_digest(&prepared),
        _ => unreachable!("validated loader schema revision"),
    };
    if expected_manifest != stored_manifest {
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
            "timeline component is not canonical for the project schema",
        ));
    }
    let editorial_project = timeline_load.into_project();
    if editorial_project.id() != prepared.project_id {
        return Err(corrupt(
            "decode_timeline_component",
            "timeline project identity does not match project metadata",
        ));
    }

    let project_settings = if let Some((format_revision, _, _)) = settings {
        if format_revision != PROJECT_SETTINGS_FORMAT_REVISION {
            return Err(corrupt(
                "decode_settings_component",
                "settings component revision is not canonical",
            ));
        }
        let snapshot: superi_core::settings::SettingsSnapshot =
            serde_json::from_slice(&prepared.settings_document)
                .map_err(|source| settings_json_error(source, "decode_settings_component", true))?;
        let project_settings = ProjectSettings::from_snapshot(snapshot)
            .map_err(|source| stored_state_error(source, "validate_settings_component"))?;
        let canonical = serde_json::to_vec(project_settings.snapshot())
            .map_err(|source| settings_json_error(source, "encode_settings_component", false))?;
        if canonical != prepared.settings_document {
            return Err(corrupt(
                "decode_settings_component",
                "settings component is not canonical for the project schema",
            ));
        }
        project_settings
    } else {
        let root_edit_rate = editorial_project
            .timeline(prepared.root_timeline_id)
            .ok_or_else(|| {
                corrupt(
                    "derive_schema_one_settings",
                    "schema-1 project root timeline is missing",
                )
            })?
            .edit_rate();
        ProjectSettings::defaults(root_edit_rate)
            .map_err(|source| stored_state_error(source, "derive_schema_one_settings"))?
    };

    let mut restored_graphs = Vec::with_capacity(prepared.graphs.len());
    for graph in prepared.graphs {
        let graph_load = deserialize_graph::<CompiledTimelineGraphValue>(&graph.document)
            .map_err(|source| stored_state_error(source, "decode_graph_component"))?;
        if graph_load.source_format_revision() != GRAPH_DOCUMENT_FORMAT_REVISION
            || graph_load.canonical_document() != graph.document
        {
            return Err(corrupt(
                "decode_graph_component",
                "graph component is not canonical for the project schema",
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
    let clip_mix_state = if include_audio {
        deserialize_clip_mix_state(&prepared.audio_document)
            .map_err(|source| stored_state_error(source, "decode_audio_component"))?
    } else {
        ClipMixState::new()
    };
    let restored_extensions = prepared
        .extensions
        .iter()
        .map(decode_extension_record)
        .collect::<Result<Vec<_>>>()?;
    ProjectDocument::from_complete_parts_with_settings_and_extensions(
        prepared.revision,
        editorial_project,
        prepared.root_timeline_id,
        project_settings,
        restored_graphs,
        clip_mix_state,
        restored_extensions,
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

fn validate_settings_component(
    stored_length: i64,
    stored_digest: &[u8],
    document: &[u8],
    operation: &'static str,
) -> Result<()> {
    let stored_length = usize::try_from(stored_length)
        .map_err(|_| corrupt(operation, "settings component length is not representable"))?;
    check_settings_component_size(stored_length, operation)?;
    if stored_length != document.len() {
        return Err(corrupt(
            operation,
            "settings component length does not match its stored evidence",
        ));
    }
    let stored_digest = fixed_bytes::<32>(
        stored_digest.to_vec(),
        operation,
        "settings component digest",
    )?;
    if sha256(document) != stored_digest {
        return Err(corrupt(
            operation,
            "settings component digest does not match its stored bytes",
        ));
    }
    Ok(())
}

fn validate_extension_metadata(
    stored_length: i64,
    stored_digest: &[u8],
    document: &[u8],
) -> Result<()> {
    let operation = "read_extension_metadata";
    let stored_length = usize::try_from(stored_length)
        .map_err(|_| corrupt(operation, "extension metadata length is not representable"))?;
    check_extension_metadata_size(stored_length, operation)?;
    if stored_length != document.len() {
        return Err(corrupt(
            operation,
            "extension metadata length does not match its stored evidence",
        ));
    }
    let stored_digest = fixed_bytes::<32>(
        stored_digest.to_vec(),
        operation,
        "extension metadata digest",
    )?;
    if sha256(document) != stored_digest {
        return Err(corrupt(
            operation,
            "extension metadata digest does not match its stored bytes",
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
            "component revision does not match the project schema",
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

pub(crate) fn check_settings_component_size(bytes: usize, operation: &'static str) -> Result<()> {
    if bytes > MAX_SETTINGS_COMPONENT_BYTES {
        return Err(resource_exhausted(
            operation,
            "project settings component exceeds the stable schema size limit",
        ));
    }
    Ok(())
}

fn check_extension_metadata_size(bytes: usize, operation: &'static str) -> Result<()> {
    if bytes > MAX_EXTENSION_METADATA_BYTES {
        return Err(resource_exhausted(
            operation,
            "project extension metadata exceeds the stable schema size limit",
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
    manifest_digest_for(
        project,
        MANIFEST_DOMAIN_V4,
        PROJECT_FORMAT_VERSION,
        PROJECT_SCHEMA_REVISION,
        true,
        true,
        true,
    )
}

fn manifest_digest_v3(project: &PreparedProject) -> [u8; 32] {
    manifest_digest_for(
        project,
        MANIFEST_DOMAIN_V3,
        PROJECT_FORMAT_VERSION_SCHEMA_THREE,
        3,
        true,
        true,
        false,
    )
}

fn manifest_digest_v2(project: &PreparedProject) -> [u8; 32] {
    manifest_digest_for(
        project,
        MANIFEST_DOMAIN_V2,
        PROJECT_FORMAT_VERSION_SCHEMA_TWO,
        2,
        true,
        false,
        false,
    )
}

fn manifest_digest_v1(project: &PreparedProject) -> [u8; 32] {
    manifest_digest_for(
        project,
        MANIFEST_DOMAIN_V1,
        PROJECT_FORMAT_VERSION_SCHEMA_ONE,
        1,
        false,
        false,
        false,
    )
}

fn manifest_digest_for(
    project: &PreparedProject,
    domain: &[u8],
    format_version: &str,
    schema_revision: u32,
    include_settings: bool,
    include_audio: bool,
    include_extensions: bool,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    manifest_field(&mut hasher, domain);
    manifest_field(&mut hasher, PROJECT_FORMAT.as_bytes());
    manifest_field(&mut hasher, format_version.as_bytes());
    manifest_field(&mut hasher, &schema_revision.to_be_bytes());
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
    if include_settings {
        manifest_field(&mut hasher, &PROJECT_SETTINGS_FORMAT_REVISION.to_be_bytes());
        manifest_field(
            &mut hasher,
            &(project.settings_document.len() as u64).to_be_bytes(),
        );
        manifest_field(&mut hasher, &project.settings_digest);
    }
    if include_audio {
        manifest_field(&mut hasher, &CLIP_MIX_FORMAT_REVISION.to_be_bytes());
        manifest_field(
            &mut hasher,
            &(project.audio_document.len() as u64).to_be_bytes(),
        );
        manifest_field(&mut hasher, &project.audio_digest);
    }
    if include_extensions {
        manifest_field(
            &mut hasher,
            &(project.extensions.len() as u64).to_be_bytes(),
        );
        for extension in &project.extensions {
            manifest_field(&mut hasher, extension.extension_id.as_bytes());
            manifest_field(&mut hasher, extension.record_id.as_bytes());
            manifest_field(
                &mut hasher,
                &PROJECT_EXTENSION_METADATA_FORMAT_REVISION.to_be_bytes(),
            );
            manifest_field(
                &mut hasher,
                &(extension.metadata_document.len() as u64).to_be_bytes(),
            );
            manifest_field(&mut hasher, &extension.metadata_digest);
            manifest_field(&mut hasher, &(extension.payload.len() as u64).to_be_bytes());
            manifest_field(&mut hasher, &extension.payload_digest);
        }
    }
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

fn path_error(source: std::io::Error, operation: &'static str, path: &Path) -> Error {
    let (category, recoverability, message) = match source.kind() {
        std::io::ErrorKind::NotFound => (
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "project database path does not exist",
        ),
        std::io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
            "project database path is not accessible",
        ),
        _ => (
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "project database path could not be resolved",
        ),
    };
    Error::with_source(category, recoverability, message, source).with_context(
        ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
    )
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

fn settings_json_error(
    source: serde_json::Error,
    operation: &'static str,
    stored_input: bool,
) -> Error {
    let (category, recoverability, message) = if stored_input {
        (
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "stored project settings are not valid strict JSON",
        )
    } else {
        (
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "validated project settings could not be serialized",
        )
    };
    Error::with_source(category, recoverability, message, source)
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
