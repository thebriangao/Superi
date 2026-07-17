//! Read-only whole-project integrity validation and deterministic repair reporting.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ProjectId, TimelineId};

use crate::migrate::inspect_project_revision;
use crate::persist::{
    close_connection, collect_sqlite_integrity, database_error, open_file_connection,
    SqliteForeignKeyViolation, PROJECT_APPLICATION_ID, PROJECT_SCHEMA_REVISION,
};

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// Maximum findings returned by one integrity command, including truncation evidence.
pub const MAX_PROJECT_INTEGRITY_FINDINGS: usize = 64;
/// Maximum UTF-8 bytes retained in one integrity evidence value.
pub const MAX_PROJECT_INTEGRITY_EVIDENCE_VALUE_BYTES: usize = 4096;

/// One project integrity command shared by editor, script, and headless callers.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectIntegrityCommand {
    /// Validates one existing project without changing it.
    Validate {
        /// Project file to inspect.
        path: PathBuf,
    },
}

/// Overall interpretation of a completed project integrity inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectIntegrityStatus {
    /// The current project fully reconstructed with no finding.
    Valid,
    /// A supported older project fully reconstructed and can migrate forward.
    MigrationRequired,
    /// The inspected project is corrupt or semantically inconsistent.
    Invalid,
    /// The input or one of its revisions requires different software.
    Unsupported,
    /// Access, concurrency, or bounded evidence prevented a conclusive result.
    Indeterminate,
}

impl ProjectIntegrityStatus {
    /// Returns the permanent public code for this status.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::MigrationRequired => "migration_required",
            Self::Invalid => "invalid",
            Self::Unsupported => "unsupported",
            Self::Indeterminate => "indeterminate",
        }
    }
}

/// Stable stage at which one integrity finding was produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectIntegrityStage {
    /// Source path and database open.
    Open,
    /// SQLite page, index, record, and constraint structure.
    SqliteStructure,
    /// SQLite foreign-key relationships.
    ForeignKeys,
    /// Superi application identity.
    ApplicationIdentity,
    /// Project schema revision and exact schema objects.
    Schema,
    /// Stored component lengths, digests, formats, and manifest.
    ComponentEvidence,
    /// Timeline, graph, settings, audio, and extension reconstruction.
    SemanticReconstruction,
    /// Complete whole-project relationship validation.
    Aggregate,
    /// Source stability across the read snapshot.
    SourceStability,
}

impl ProjectIntegrityStage {
    /// Returns the permanent public code for this stage.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::SqliteStructure => "sqlite_structure",
            Self::ForeignKeys => "foreign_keys",
            Self::ApplicationIdentity => "application_identity",
            Self::Schema => "schema",
            Self::ComponentEvidence => "component_evidence",
            Self::SemanticReconstruction => "semantic_reconstruction",
            Self::Aggregate => "aggregate",
            Self::SourceStability => "source_stability",
        }
    }

    const fn order(self) -> u8 {
        match self {
            Self::Open => 0,
            Self::SqliteStructure => 1,
            Self::ForeignKeys => 2,
            Self::ApplicationIdentity => 3,
            Self::Schema => 4,
            Self::ComponentEvidence => 5,
            Self::SemanticReconstruction => 6,
            Self::Aggregate => 7,
            Self::SourceStability => 8,
        }
    }
}

/// Stable semantic reason for one integrity finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectIntegrityFindingCode {
    /// The requested source path does not exist.
    SourceNotFound,
    /// The source could not be read with current permissions.
    SourceAccessDenied,
    /// The source is temporarily busy or locked.
    SourceBusy,
    /// The source is temporarily unavailable for another reason.
    SourceUnavailable,
    /// The file is not a SQLite application database.
    NotSqliteDatabase,
    /// SQLite reported page, index, record, or constraint corruption.
    SqliteIntegrityViolation,
    /// SQLite reported a foreign-key violation.
    ForeignKeyViolation,
    /// The bounded inspection omitted additional findings.
    InspectionTruncated,
    /// A bounded runtime resource could not complete the inspection.
    InspectionResourceExhausted,
    /// The SQLite application ID is not Superi's application ID.
    ApplicationIdentityMismatch,
    /// The schema revision is negative, unrepresentable, or unregistered.
    SchemaRevisionInvalid,
    /// The schema revision requires newer software.
    SchemaRevisionUnsupported,
    /// Schema objects do not exactly match the declared revision.
    SchemaObjectsInvalid,
    /// Project metadata is missing, malformed, or inconsistent.
    MetadataInvalid,
    /// Timeline component evidence or meaning is invalid.
    TimelineComponentInvalid,
    /// Graph component evidence or meaning is invalid.
    GraphComponentInvalid,
    /// Settings component evidence or meaning is invalid.
    SettingsComponentInvalid,
    /// Audio component evidence or meaning is invalid.
    AudioComponentInvalid,
    /// Extension metadata, payload evidence, or meaning is invalid.
    ExtensionComponentInvalid,
    /// The project manifest does not match the stored component evidence.
    ManifestInvalid,
    /// A component or semantic format requires newer software.
    SemanticFormatUnsupported,
    /// Stored state failed checked domain reconstruction.
    SemanticStateInvalid,
    /// Reconstructed components failed whole-project relationship validation.
    AggregateInvalid,
    /// Another connection committed while the inspection was active.
    SourceChanged,
    /// A fully validated older schema has a registered forward migration.
    SchemaMigrationRequired,
}

impl ProjectIntegrityFindingCode {
    /// Returns the permanent public code for this finding.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::SourceNotFound => "source_not_found",
            Self::SourceAccessDenied => "source_access_denied",
            Self::SourceBusy => "source_busy",
            Self::SourceUnavailable => "source_unavailable",
            Self::NotSqliteDatabase => "not_sqlite_database",
            Self::SqliteIntegrityViolation => "sqlite_integrity_violation",
            Self::ForeignKeyViolation => "foreign_key_violation",
            Self::InspectionTruncated => "inspection_truncated",
            Self::InspectionResourceExhausted => "inspection_resource_exhausted",
            Self::ApplicationIdentityMismatch => "application_identity_mismatch",
            Self::SchemaRevisionInvalid => "schema_revision_invalid",
            Self::SchemaRevisionUnsupported => "schema_revision_unsupported",
            Self::SchemaObjectsInvalid => "schema_objects_invalid",
            Self::MetadataInvalid => "metadata_invalid",
            Self::TimelineComponentInvalid => "timeline_component_invalid",
            Self::GraphComponentInvalid => "graph_component_invalid",
            Self::SettingsComponentInvalid => "settings_component_invalid",
            Self::AudioComponentInvalid => "audio_component_invalid",
            Self::ExtensionComponentInvalid => "extension_component_invalid",
            Self::ManifestInvalid => "manifest_invalid",
            Self::SemanticFormatUnsupported => "semantic_format_unsupported",
            Self::SemanticStateInvalid => "semantic_state_invalid",
            Self::AggregateInvalid => "aggregate_invalid",
            Self::SourceChanged => "source_changed",
            Self::SchemaMigrationRequired => "schema_migration_required",
        }
    }
}

/// Safe next action described by a project integrity report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectRepairDisposition {
    /// No project repair is needed or applicable.
    NotNeeded,
    /// Use the existing checked forward migration path.
    MigrateForward,
    /// Discover through the project recovery controller and restore only a fully validated
    /// candidate through the engine recovery coordinator.
    RestoreValidatedRecovery,
    /// Retry the complete read-only inspection.
    RetryInspection,
    /// Resolve the missing path or access authority, then retry.
    ResolveAccess,
    /// Open with a newer compatible application.
    UseNewerApplication,
    /// No validated automatic action is available.
    ManualRecoveryRequired,
}

impl ProjectRepairDisposition {
    /// Returns the permanent public code for this disposition.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotNeeded => "not_needed",
            Self::MigrateForward => "migrate_forward",
            Self::RestoreValidatedRecovery => "restore_validated_recovery",
            Self::RetryInspection => "retry_inspection",
            Self::ResolveAccess => "resolve_access",
            Self::UseNewerApplication => "use_newer_application",
            Self::ManualRecoveryRequired => "manual_recovery_required",
        }
    }
}

/// One bounded, deterministic integrity finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectIntegrityFinding {
    code: ProjectIntegrityFindingCode,
    stage: ProjectIntegrityStage,
    category: ErrorCategory,
    recoverability: Recoverability,
    repair_disposition: ProjectRepairDisposition,
    evidence: BTreeMap<String, String>,
}

impl ProjectIntegrityFinding {
    /// Returns the stable semantic finding code.
    #[must_use]
    pub const fn code(&self) -> ProjectIntegrityFindingCode {
        self.code
    }

    /// Returns the stable inspection stage.
    #[must_use]
    pub const fn stage(&self) -> ProjectIntegrityStage {
        self.stage
    }

    /// Returns the shared failure category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the shared recoverability classification.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    /// Returns the safe repair recommendation for this finding.
    #[must_use]
    pub const fn repair_disposition(&self) -> ProjectRepairDisposition {
        self.repair_disposition
    }

    /// Returns deterministic diagnostic evidence.
    ///
    /// Evidence can include paths and internal identifiers and is not automatically user-safe.
    #[must_use]
    pub const fn evidence(&self) -> &BTreeMap<String, String> {
        &self.evidence
    }
}

/// Identity exposed only after complete semantic reconstruction succeeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectIntegrityIdentity {
    project_id: ProjectId,
    document_revision: u64,
    root_timeline_id: TimelineId,
    source_schema_revision: u32,
    project_format_version: String,
    media_count: usize,
    graph_count: usize,
    extension_count: usize,
}

impl ProjectIntegrityIdentity {
    /// Returns the verified project identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    /// Returns the verified document revision.
    #[must_use]
    pub const fn document_revision(&self) -> u64 {
        self.document_revision
    }

    /// Returns the verified root timeline identity.
    #[must_use]
    pub const fn root_timeline_id(&self) -> TimelineId {
        self.root_timeline_id
    }

    /// Returns the schema revision used to reconstruct this identity.
    #[must_use]
    pub const fn source_schema_revision(&self) -> u32 {
        self.source_schema_revision
    }

    /// Returns the verified semantic project format version.
    #[must_use]
    pub fn project_format_version(&self) -> &str {
        &self.project_format_version
    }

    /// Returns the verified media-reference count.
    #[must_use]
    pub const fn media_count(&self) -> usize {
        self.media_count
    }

    /// Returns the verified retained-graph count.
    #[must_use]
    pub const fn graph_count(&self) -> usize {
        self.graph_count
    }

    /// Returns the verified durable extension-record count.
    #[must_use]
    pub const fn extension_count(&self) -> usize {
        self.extension_count
    }
}

/// Complete deterministic result of one project integrity command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectIntegrityReport {
    path: PathBuf,
    current_schema_revision: u32,
    observed_schema_revision: Option<u32>,
    status: ProjectIntegrityStatus,
    repair_disposition: ProjectRepairDisposition,
    identity: Option<ProjectIntegrityIdentity>,
    findings: Vec<ProjectIntegrityFinding>,
    inspection_complete: bool,
}

impl ProjectIntegrityReport {
    /// Returns the requested path exactly as supplied by the command.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the schema revision supported by this application.
    #[must_use]
    pub const fn current_schema_revision(&self) -> u32 {
        self.current_schema_revision
    }

    /// Returns the observed project schema when it could be read safely.
    #[must_use]
    pub const fn observed_schema_revision(&self) -> Option<u32> {
        self.observed_schema_revision
    }

    /// Returns the overall integrity status.
    #[must_use]
    pub const fn status(&self) -> ProjectIntegrityStatus {
        self.status
    }

    /// Returns the report's safe repair recommendation.
    #[must_use]
    pub const fn repair_disposition(&self) -> ProjectRepairDisposition {
        self.repair_disposition
    }

    /// Returns verified identity only after complete reconstruction.
    #[must_use]
    pub const fn identity(&self) -> Option<&ProjectIntegrityIdentity> {
        self.identity.as_ref()
    }

    /// Returns findings in canonical stage, code, and evidence order.
    #[must_use]
    pub fn findings(&self) -> &[ProjectIntegrityFinding] {
        &self.findings
    }

    /// Returns whether the inspection reached a conclusive, untruncated result.
    #[must_use]
    pub const fn inspection_complete(&self) -> bool {
        self.inspection_complete
    }
}

/// Executes one read-only project integrity command.
pub fn execute_project_integrity_command(
    command: ProjectIntegrityCommand,
) -> Result<ProjectIntegrityReport> {
    match command {
        ProjectIntegrityCommand::Validate { path } => validate_project(path),
    }
}

fn validate_project(path: PathBuf) -> Result<ProjectIntegrityReport> {
    match has_sqlite_header(&path) {
        Ok(true) => {}
        Ok(false) => {
            return Ok(single_finding_report(
                path,
                None,
                ProjectIntegrityStatus::Unsupported,
                ProjectRepairDisposition::NotNeeded,
                true,
                finding::<String, String, _>(
                    ProjectIntegrityFindingCode::NotSqliteDatabase,
                    ProjectIntegrityStage::Open,
                    ErrorCategory::Unsupported,
                    Recoverability::UserCorrectable,
                    ProjectRepairDisposition::NotNeeded,
                    [],
                ),
            ));
        }
        Err(source) => return Ok(io_error_report(path, source)),
    }

    let connection = match open_file_connection(&path, false) {
        Ok(connection) => connection,
        Err(error) => return checked_error_report(path, None, ProjectIntegrityStage::Open, error),
    };
    let before_data_version = match read_data_version(&connection) {
        Ok(value) => value,
        Err(error) => {
            let report = checked_error_report(
                path.clone(),
                None,
                ProjectIntegrityStage::SourceStability,
                error,
            );
            let _ = close_connection(connection, "close_integrity_database");
            return report;
        }
    };
    let transaction = match connection.unchecked_transaction() {
        Ok(transaction) => transaction,
        Err(source) => {
            let error = database_error(source, "begin_integrity_snapshot");
            let report = checked_error_report(
                path.clone(),
                None,
                ProjectIntegrityStage::SourceStability,
                error,
            );
            return report;
        }
    };

    let mut report = inspect_snapshot(&transaction, path.clone())?;
    if let Err(source) = transaction.commit() {
        let error = database_error(source, "finish_integrity_snapshot");
        let _ = close_connection(connection, "close_integrity_database");
        return checked_error_report(
            path,
            report.observed_schema_revision,
            ProjectIntegrityStage::SourceStability,
            error,
        );
    }
    let after_data_version = match read_data_version(&connection) {
        Ok(value) => value,
        Err(error) => {
            let _ = close_connection(connection, "close_integrity_database");
            return checked_error_report(
                path,
                report.observed_schema_revision,
                ProjectIntegrityStage::SourceStability,
                error,
            );
        }
    };
    if before_data_version != after_data_version {
        mark_source_changed(&mut report, before_data_version, after_data_version);
    }
    if let Err(error) = close_connection(connection, "close_integrity_database") {
        return checked_error_report(
            path,
            report.observed_schema_revision,
            ProjectIntegrityStage::SourceStability,
            error,
        );
    }
    sort_findings(&mut report.findings);
    Ok(report)
}

fn inspect_snapshot(connection: &Connection, path: PathBuf) -> Result<ProjectIntegrityReport> {
    let sqlite = match collect_sqlite_integrity(
        connection,
        MAX_PROJECT_INTEGRITY_FINDINGS,
        "inspect_project_integrity",
    ) {
        Ok(evidence) => evidence,
        Err(error) => {
            return checked_error_report(path, None, ProjectIntegrityStage::SqliteStructure, error);
        }
    };
    if !sqlite.integrity_messages.is_empty()
        || !sqlite.foreign_key_violations.is_empty()
        || sqlite.truncated
    {
        return Ok(sqlite_failure_report(path, sqlite));
    }

    let application_id: i64 =
        match connection.pragma_query_value(None, "application_id", |row| row.get(0)) {
            Ok(value) => value,
            Err(source) => {
                return checked_error_report(
                    path,
                    None,
                    ProjectIntegrityStage::ApplicationIdentity,
                    database_error(source, "read_integrity_application_id"),
                );
            }
        };
    if application_id != i64::from(PROJECT_APPLICATION_ID) {
        return Ok(single_finding_report(
            path,
            None,
            ProjectIntegrityStatus::Unsupported,
            ProjectRepairDisposition::NotNeeded,
            true,
            finding(
                ProjectIntegrityFindingCode::ApplicationIdentityMismatch,
                ProjectIntegrityStage::ApplicationIdentity,
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                ProjectRepairDisposition::NotNeeded,
                [
                    ("observed", application_id.to_string()),
                    ("expected", i64::from(PROJECT_APPLICATION_ID).to_string()),
                ],
            ),
        ));
    }

    let raw_schema_revision: i64 =
        match connection.pragma_query_value(None, "user_version", |row| row.get(0)) {
            Ok(value) => value,
            Err(source) => {
                return checked_error_report(
                    path,
                    None,
                    ProjectIntegrityStage::Schema,
                    database_error(source, "read_integrity_schema_revision"),
                );
            }
        };
    let schema_revision = match u32::try_from(raw_schema_revision) {
        Ok(value) => value,
        Err(_) => {
            return Ok(single_finding_report(
                path,
                None,
                ProjectIntegrityStatus::Invalid,
                ProjectRepairDisposition::RestoreValidatedRecovery,
                true,
                finding(
                    ProjectIntegrityFindingCode::SchemaRevisionInvalid,
                    ProjectIntegrityStage::Schema,
                    ErrorCategory::CorruptData,
                    Recoverability::UserCorrectable,
                    ProjectRepairDisposition::RestoreValidatedRecovery,
                    [("observed", raw_schema_revision.to_string())],
                ),
            ));
        }
    };
    if schema_revision > PROJECT_SCHEMA_REVISION {
        return Ok(single_finding_report(
            path,
            Some(schema_revision),
            ProjectIntegrityStatus::Unsupported,
            ProjectRepairDisposition::UseNewerApplication,
            true,
            finding(
                ProjectIntegrityFindingCode::SchemaRevisionUnsupported,
                ProjectIntegrityStage::Schema,
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                ProjectRepairDisposition::UseNewerApplication,
                [
                    ("observed", schema_revision.to_string()),
                    ("supported", PROJECT_SCHEMA_REVISION.to_string()),
                ],
            ),
        ));
    }
    let document = match inspect_project_revision(connection, schema_revision) {
        Ok(document) => document,
        Err(error) => {
            return checked_error_report(
                path,
                Some(schema_revision),
                ProjectIntegrityStage::SemanticReconstruction,
                error,
            );
        }
    };
    let project_format_version: String = match connection.query_row(
        "SELECT format_version FROM project_metadata WHERE singleton = 1",
        [],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(source) => {
            return checked_error_report(
                path,
                Some(schema_revision),
                ProjectIntegrityStage::ComponentEvidence,
                database_error(source, "read_integrity_project_format"),
            );
        }
    };
    let identity = ProjectIntegrityIdentity {
        project_id: document.project_id(),
        document_revision: document.revision(),
        root_timeline_id: document.root_timeline_id(),
        source_schema_revision: schema_revision,
        project_format_version,
        media_count: document.editorial_project().media_references().count(),
        graph_count: document.graphs().len(),
        extension_count: document.extension_records().len(),
    };
    if schema_revision == PROJECT_SCHEMA_REVISION {
        Ok(ProjectIntegrityReport {
            path,
            current_schema_revision: PROJECT_SCHEMA_REVISION,
            observed_schema_revision: Some(schema_revision),
            status: ProjectIntegrityStatus::Valid,
            repair_disposition: ProjectRepairDisposition::NotNeeded,
            identity: Some(identity),
            findings: Vec::new(),
            inspection_complete: true,
        })
    } else {
        Ok(ProjectIntegrityReport {
            path,
            current_schema_revision: PROJECT_SCHEMA_REVISION,
            observed_schema_revision: Some(schema_revision),
            status: ProjectIntegrityStatus::MigrationRequired,
            repair_disposition: ProjectRepairDisposition::MigrateForward,
            identity: Some(identity),
            findings: vec![finding(
                ProjectIntegrityFindingCode::SchemaMigrationRequired,
                ProjectIntegrityStage::Schema,
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                ProjectRepairDisposition::MigrateForward,
                [
                    ("observed", schema_revision.to_string()),
                    ("target", PROJECT_SCHEMA_REVISION.to_string()),
                ],
            )],
            inspection_complete: true,
        })
    }
}

fn sqlite_failure_report(
    path: PathBuf,
    sqlite: crate::persist::SqliteIntegrityEvidence,
) -> ProjectIntegrityReport {
    let mut findings = Vec::new();
    for message in sqlite.integrity_messages {
        findings.push(finding(
            ProjectIntegrityFindingCode::SqliteIntegrityViolation,
            ProjectIntegrityStage::SqliteStructure,
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            ProjectRepairDisposition::RestoreValidatedRecovery,
            [("detail", message)],
        ));
    }
    for violation in sqlite.foreign_key_violations {
        findings.push(foreign_key_finding(violation));
    }
    sort_findings(&mut findings);
    let truncated = sqlite.truncated || findings.len() > MAX_PROJECT_INTEGRITY_FINDINGS;
    if truncated {
        findings.truncate(MAX_PROJECT_INTEGRITY_FINDINGS.saturating_sub(1));
        findings.push(truncation_finding());
    }
    ProjectIntegrityReport {
        path,
        current_schema_revision: PROJECT_SCHEMA_REVISION,
        observed_schema_revision: None,
        status: if truncated {
            ProjectIntegrityStatus::Indeterminate
        } else {
            ProjectIntegrityStatus::Invalid
        },
        repair_disposition: if truncated {
            ProjectRepairDisposition::RetryInspection
        } else {
            ProjectRepairDisposition::RestoreValidatedRecovery
        },
        identity: None,
        findings,
        inspection_complete: !truncated,
    }
}

fn foreign_key_finding(violation: SqliteForeignKeyViolation) -> ProjectIntegrityFinding {
    finding(
        ProjectIntegrityFindingCode::ForeignKeyViolation,
        ProjectIntegrityStage::ForeignKeys,
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        ProjectRepairDisposition::RestoreValidatedRecovery,
        [
            ("table", violation.table),
            (
                "rowid",
                violation
                    .row_id
                    .map_or_else(|| "null".to_owned(), |value| value.to_string()),
            ),
            ("parent", violation.parent_table),
            ("constraint", violation.constraint_index.to_string()),
        ],
    )
}

fn checked_error_report(
    path: PathBuf,
    observed_schema_revision: Option<u32>,
    default_stage: ProjectIntegrityStage,
    error: Error,
) -> Result<ProjectIntegrityReport> {
    if error.category() == ErrorCategory::Internal
        && error.recoverability() == Recoverability::Terminal
    {
        return Err(error);
    }
    let source_stage = matches!(
        default_stage,
        ProjectIntegrityStage::Open | ProjectIntegrityStage::SourceStability
    );
    let (status, repair_disposition, code) = match error.category() {
        ErrorCategory::NotFound if source_stage => (
            ProjectIntegrityStatus::Indeterminate,
            ProjectRepairDisposition::ResolveAccess,
            ProjectIntegrityFindingCode::SourceNotFound,
        ),
        ErrorCategory::PermissionDenied if source_stage => (
            ProjectIntegrityStatus::Indeterminate,
            ProjectRepairDisposition::ResolveAccess,
            ProjectIntegrityFindingCode::SourceAccessDenied,
        ),
        ErrorCategory::Unavailable | ErrorCategory::Timeout | ErrorCategory::Cancelled => (
            ProjectIntegrityStatus::Indeterminate,
            ProjectRepairDisposition::RetryInspection,
            if error.message().contains("busy") {
                ProjectIntegrityFindingCode::SourceBusy
            } else {
                ProjectIntegrityFindingCode::SourceUnavailable
            },
        ),
        ErrorCategory::ResourceExhausted => (
            ProjectIntegrityStatus::Indeterminate,
            ProjectRepairDisposition::RetryInspection,
            ProjectIntegrityFindingCode::InspectionResourceExhausted,
        ),
        ErrorCategory::Unsupported => (
            ProjectIntegrityStatus::Unsupported,
            ProjectRepairDisposition::UseNewerApplication,
            ProjectIntegrityFindingCode::SemanticFormatUnsupported,
        ),
        _ => (
            ProjectIntegrityStatus::Invalid,
            ProjectRepairDisposition::RestoreValidatedRecovery,
            semantic_finding_code(&error),
        ),
    };
    let stage = semantic_finding_stage(&error).unwrap_or(default_stage);
    let mut evidence = BTreeMap::new();
    if let Some(operation) = error_operation(&error) {
        evidence.insert("operation".to_owned(), bounded_text(operation));
    }
    evidence.insert("category".to_owned(), error.category().code().to_owned());
    evidence.insert(
        "recoverability".to_owned(),
        error.recoverability().code().to_owned(),
    );
    for context in error.contexts() {
        for (key, value) in context.fields() {
            evidence
                .entry(bounded_text(key))
                .or_insert_with(|| bounded_text(value));
        }
    }
    Ok(single_finding_report(
        path,
        observed_schema_revision,
        status,
        repair_disposition,
        status != ProjectIntegrityStatus::Indeterminate,
        ProjectIntegrityFinding {
            code,
            stage,
            category: error.category(),
            recoverability: error.recoverability(),
            repair_disposition,
            evidence,
        },
    ))
}

fn semantic_finding_code(error: &Error) -> ProjectIntegrityFindingCode {
    if let Some(table) = error_context_field(error, "table") {
        return match table {
            "project_metadata" => ProjectIntegrityFindingCode::MetadataInvalid,
            "timeline_component" => ProjectIntegrityFindingCode::TimelineComponentInvalid,
            "graph_components" => ProjectIntegrityFindingCode::GraphComponentInvalid,
            "settings_component" => ProjectIntegrityFindingCode::SettingsComponentInvalid,
            "audio_component" => ProjectIntegrityFindingCode::AudioComponentInvalid,
            "extension_records" => ProjectIntegrityFindingCode::ExtensionComponentInvalid,
            _ => ProjectIntegrityFindingCode::SemanticStateInvalid,
        };
    }
    let operation = error_operation(error).unwrap_or_default();
    if operation.contains("manifest") {
        ProjectIntegrityFindingCode::ManifestInvalid
    } else if operation.contains("extension") {
        ProjectIntegrityFindingCode::ExtensionComponentInvalid
    } else if operation.contains("metadata") {
        ProjectIntegrityFindingCode::MetadataInvalid
    } else if operation.contains("timeline") {
        ProjectIntegrityFindingCode::TimelineComponentInvalid
    } else if operation.contains("graph") {
        ProjectIntegrityFindingCode::GraphComponentInvalid
    } else if operation.contains("settings") {
        ProjectIntegrityFindingCode::SettingsComponentInvalid
    } else if operation.contains("audio") || operation.contains("clip_mix") {
        ProjectIntegrityFindingCode::AudioComponentInvalid
    } else if operation.contains("schema") {
        ProjectIntegrityFindingCode::SchemaObjectsInvalid
    } else if operation.contains("restore_project_document")
        || operation.contains("restore_legacy_project_document")
    {
        ProjectIntegrityFindingCode::AggregateInvalid
    } else {
        ProjectIntegrityFindingCode::SemanticStateInvalid
    }
}

fn semantic_finding_stage(error: &Error) -> Option<ProjectIntegrityStage> {
    let code = semantic_finding_code(error);
    Some(match code {
        ProjectIntegrityFindingCode::SchemaRevisionInvalid
        | ProjectIntegrityFindingCode::SchemaRevisionUnsupported
        | ProjectIntegrityFindingCode::SchemaObjectsInvalid
        | ProjectIntegrityFindingCode::SchemaMigrationRequired => ProjectIntegrityStage::Schema,
        ProjectIntegrityFindingCode::MetadataInvalid
        | ProjectIntegrityFindingCode::TimelineComponentInvalid
        | ProjectIntegrityFindingCode::GraphComponentInvalid
        | ProjectIntegrityFindingCode::SettingsComponentInvalid
        | ProjectIntegrityFindingCode::AudioComponentInvalid
        | ProjectIntegrityFindingCode::ExtensionComponentInvalid
        | ProjectIntegrityFindingCode::ManifestInvalid => ProjectIntegrityStage::ComponentEvidence,
        ProjectIntegrityFindingCode::AggregateInvalid => ProjectIntegrityStage::Aggregate,
        ProjectIntegrityFindingCode::SemanticFormatUnsupported
        | ProjectIntegrityFindingCode::SemanticStateInvalid => {
            ProjectIntegrityStage::SemanticReconstruction
        }
        _ => return None,
    })
}

fn error_operation(error: &Error) -> Option<&str> {
    error
        .contexts()
        .iter()
        .rev()
        .map(ErrorContext::operation)
        .find(|operation| !operation.is_empty())
}

fn error_context_field<'a>(error: &'a Error, key: &str) -> Option<&'a str> {
    error
        .contexts()
        .iter()
        .rev()
        .find_map(|context| context.field(key))
}

fn has_sqlite_header(path: &Path) -> std::io::Result<bool> {
    let mut file = File::open(path)?;
    let mut header = [0_u8; 16];
    let mut read = 0;
    while read < header.len() {
        let count = file.read(&mut header[read..])?;
        if count == 0 {
            break;
        }
        read += count;
    }
    Ok(read == header.len() && &header == SQLITE_HEADER)
}

fn io_error_report(path: PathBuf, source: std::io::Error) -> ProjectIntegrityReport {
    let (code, category, recoverability, disposition) = match source.kind() {
        std::io::ErrorKind::NotFound => (
            ProjectIntegrityFindingCode::SourceNotFound,
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            ProjectRepairDisposition::ResolveAccess,
        ),
        std::io::ErrorKind::PermissionDenied => (
            ProjectIntegrityFindingCode::SourceAccessDenied,
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
            ProjectRepairDisposition::ResolveAccess,
        ),
        _ => (
            ProjectIntegrityFindingCode::SourceUnavailable,
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            ProjectRepairDisposition::RetryInspection,
        ),
    };
    single_finding_report(
        path,
        None,
        ProjectIntegrityStatus::Indeterminate,
        disposition,
        false,
        finding::<String, String, _>(
            code,
            ProjectIntegrityStage::Open,
            category,
            recoverability,
            disposition,
            [],
        ),
    )
}

fn read_data_version(connection: &Connection) -> Result<i64> {
    connection
        .pragma_query_value(None, "data_version", |row| row.get(0))
        .map_err(|source| database_error(source, "read_integrity_data_version"))
}

fn mark_source_changed(report: &mut ProjectIntegrityReport, before: i64, after: i64) {
    report.status = ProjectIntegrityStatus::Indeterminate;
    report.repair_disposition = ProjectRepairDisposition::RetryInspection;
    report.identity = None;
    report.inspection_complete = false;
    if report.findings.len() >= MAX_PROJECT_INTEGRITY_FINDINGS {
        let already_truncated = report
            .findings
            .iter()
            .any(|finding| finding.code == ProjectIntegrityFindingCode::InspectionTruncated);
        while report.findings.len()
            >= MAX_PROJECT_INTEGRITY_FINDINGS.saturating_sub(usize::from(!already_truncated))
        {
            remove_last_nontruncation(&mut report.findings);
        }
        if !already_truncated {
            report.findings.push(truncation_finding());
        }
    }
    report.findings.push(finding(
        ProjectIntegrityFindingCode::SourceChanged,
        ProjectIntegrityStage::SourceStability,
        ErrorCategory::Conflict,
        Recoverability::Retryable,
        ProjectRepairDisposition::RetryInspection,
        [
            ("before_data_version", before.to_string()),
            ("after_data_version", after.to_string()),
        ],
    ));
}

fn remove_last_nontruncation(findings: &mut Vec<ProjectIntegrityFinding>) {
    let removable = findings
        .iter()
        .rposition(|finding| finding.code != ProjectIntegrityFindingCode::InspectionTruncated)
        .expect("a bounded nonempty report has a removable nontruncation finding");
    findings.remove(removable);
}

fn single_finding_report(
    path: PathBuf,
    observed_schema_revision: Option<u32>,
    status: ProjectIntegrityStatus,
    repair_disposition: ProjectRepairDisposition,
    inspection_complete: bool,
    finding: ProjectIntegrityFinding,
) -> ProjectIntegrityReport {
    ProjectIntegrityReport {
        path,
        current_schema_revision: PROJECT_SCHEMA_REVISION,
        observed_schema_revision,
        status,
        repair_disposition,
        identity: None,
        findings: vec![finding],
        inspection_complete,
    }
}

fn finding<K, V, I>(
    code: ProjectIntegrityFindingCode,
    stage: ProjectIntegrityStage,
    category: ErrorCategory,
    recoverability: Recoverability,
    repair_disposition: ProjectRepairDisposition,
    evidence: I,
) -> ProjectIntegrityFinding
where
    K: Into<String>,
    V: Into<String>,
    I: IntoIterator<Item = (K, V)>,
{
    ProjectIntegrityFinding {
        code,
        stage,
        category,
        recoverability,
        repair_disposition,
        evidence: evidence
            .into_iter()
            .map(|(key, value)| (bounded_text(&key.into()), bounded_text(&value.into())))
            .collect(),
    }
}

fn truncation_finding() -> ProjectIntegrityFinding {
    finding::<String, String, _>(
        ProjectIntegrityFindingCode::InspectionTruncated,
        ProjectIntegrityStage::SqliteStructure,
        ErrorCategory::ResourceExhausted,
        Recoverability::Retryable,
        ProjectRepairDisposition::RetryInspection,
        [],
    )
}

fn bounded_text(value: &str) -> String {
    if value.len() <= MAX_PROJECT_INTEGRITY_EVIDENCE_VALUE_BYTES {
        return value.to_owned();
    }
    let mut end = MAX_PROJECT_INTEGRITY_EVIDENCE_VALUE_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn sort_findings(findings: &mut [ProjectIntegrityFinding]) {
    findings.sort_by(|left, right| {
        left.stage
            .order()
            .cmp(&right.stage.order())
            .then_with(|| left.code.code().cmp(right.code.code()))
            .then_with(|| left.evidence.cmp(&right.evidence))
    });
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability};

    use super::*;
    use crate::persist::{collect_sqlite_integrity, SqliteIntegrityEvidence};

    #[test]
    fn public_codes_are_stable() {
        assert_eq!(
            [
                ProjectIntegrityStatus::Valid.code(),
                ProjectIntegrityStatus::MigrationRequired.code(),
                ProjectIntegrityStatus::Invalid.code(),
                ProjectIntegrityStatus::Unsupported.code(),
                ProjectIntegrityStatus::Indeterminate.code(),
            ],
            [
                "valid",
                "migration_required",
                "invalid",
                "unsupported",
                "indeterminate",
            ]
        );
        assert_eq!(
            [
                ProjectIntegrityStage::Open.code(),
                ProjectIntegrityStage::SqliteStructure.code(),
                ProjectIntegrityStage::ForeignKeys.code(),
                ProjectIntegrityStage::ApplicationIdentity.code(),
                ProjectIntegrityStage::Schema.code(),
                ProjectIntegrityStage::ComponentEvidence.code(),
                ProjectIntegrityStage::SemanticReconstruction.code(),
                ProjectIntegrityStage::Aggregate.code(),
                ProjectIntegrityStage::SourceStability.code(),
            ],
            [
                "open",
                "sqlite_structure",
                "foreign_keys",
                "application_identity",
                "schema",
                "component_evidence",
                "semantic_reconstruction",
                "aggregate",
                "source_stability",
            ]
        );
        assert_eq!(
            [
                ProjectRepairDisposition::NotNeeded.code(),
                ProjectRepairDisposition::MigrateForward.code(),
                ProjectRepairDisposition::RestoreValidatedRecovery.code(),
                ProjectRepairDisposition::RetryInspection.code(),
                ProjectRepairDisposition::ResolveAccess.code(),
                ProjectRepairDisposition::UseNewerApplication.code(),
                ProjectRepairDisposition::ManualRecoveryRequired.code(),
            ],
            [
                "not_needed",
                "migrate_forward",
                "restore_validated_recovery",
                "retry_inspection",
                "resolve_access",
                "use_newer_application",
                "manual_recovery_required",
            ]
        );
        assert_eq!(
            [
                ProjectIntegrityFindingCode::SourceNotFound.code(),
                ProjectIntegrityFindingCode::SourceAccessDenied.code(),
                ProjectIntegrityFindingCode::SourceBusy.code(),
                ProjectIntegrityFindingCode::SourceUnavailable.code(),
                ProjectIntegrityFindingCode::NotSqliteDatabase.code(),
                ProjectIntegrityFindingCode::SqliteIntegrityViolation.code(),
                ProjectIntegrityFindingCode::ForeignKeyViolation.code(),
                ProjectIntegrityFindingCode::InspectionTruncated.code(),
                ProjectIntegrityFindingCode::InspectionResourceExhausted.code(),
                ProjectIntegrityFindingCode::ApplicationIdentityMismatch.code(),
                ProjectIntegrityFindingCode::SchemaRevisionInvalid.code(),
                ProjectIntegrityFindingCode::SchemaRevisionUnsupported.code(),
                ProjectIntegrityFindingCode::SchemaObjectsInvalid.code(),
                ProjectIntegrityFindingCode::MetadataInvalid.code(),
                ProjectIntegrityFindingCode::TimelineComponentInvalid.code(),
                ProjectIntegrityFindingCode::GraphComponentInvalid.code(),
                ProjectIntegrityFindingCode::SettingsComponentInvalid.code(),
                ProjectIntegrityFindingCode::AudioComponentInvalid.code(),
                ProjectIntegrityFindingCode::ExtensionComponentInvalid.code(),
                ProjectIntegrityFindingCode::ManifestInvalid.code(),
                ProjectIntegrityFindingCode::SemanticFormatUnsupported.code(),
                ProjectIntegrityFindingCode::SemanticStateInvalid.code(),
                ProjectIntegrityFindingCode::AggregateInvalid.code(),
                ProjectIntegrityFindingCode::SourceChanged.code(),
                ProjectIntegrityFindingCode::SchemaMigrationRequired.code(),
            ],
            [
                "source_not_found",
                "source_access_denied",
                "source_busy",
                "source_unavailable",
                "not_sqlite_database",
                "sqlite_integrity_violation",
                "foreign_key_violation",
                "inspection_truncated",
                "inspection_resource_exhausted",
                "application_identity_mismatch",
                "schema_revision_invalid",
                "schema_revision_unsupported",
                "schema_objects_invalid",
                "metadata_invalid",
                "timeline_component_invalid",
                "graph_component_invalid",
                "settings_component_invalid",
                "audio_component_invalid",
                "extension_component_invalid",
                "manifest_invalid",
                "semantic_format_unsupported",
                "semantic_state_invalid",
                "aggregate_invalid",
                "source_changed",
                "schema_migration_required",
            ]
        );
    }

    #[test]
    fn findings_are_canonically_ordered_and_utf8_bounded() {
        let oversized = "é".repeat(MAX_PROJECT_INTEGRITY_EVIDENCE_VALUE_BYTES);
        let bounded = finding(
            ProjectIntegrityFindingCode::SchemaObjectsInvalid,
            ProjectIntegrityStage::Schema,
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            ProjectRepairDisposition::RestoreValidatedRecovery,
            [("detail", oversized)],
        );
        let detail = bounded.evidence().get("detail").unwrap();
        assert!(detail.len() <= MAX_PROJECT_INTEGRITY_EVIDENCE_VALUE_BYTES);
        assert!(detail.is_char_boundary(detail.len()));

        let mut findings = vec![
            bounded,
            finding(
                ProjectIntegrityFindingCode::SourceChanged,
                ProjectIntegrityStage::SourceStability,
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                ProjectRepairDisposition::RetryInspection,
                [("order", "last")],
            ),
            finding(
                ProjectIntegrityFindingCode::SourceNotFound,
                ProjectIntegrityStage::Open,
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                ProjectRepairDisposition::ResolveAccess,
                [("order", "first")],
            ),
        ];
        sort_findings(&mut findings);
        assert_eq!(findings[0].stage(), ProjectIntegrityStage::Open);
        assert_eq!(findings[1].stage(), ProjectIntegrityStage::Schema);
        assert_eq!(findings[2].stage(), ProjectIntegrityStage::SourceStability);
    }

    #[test]
    fn collector_consumes_foreign_key_rows_and_detects_its_sentinel() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "PRAGMA foreign_keys = OFF;
                 CREATE TABLE parent (id INTEGER PRIMARY KEY);
                 CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER NOT NULL REFERENCES parent(id));
                 INSERT INTO child VALUES (3, 30), (1, 10), (2, 20);",
            )
            .unwrap();

        let complete = collect_sqlite_integrity(&connection, 8, "test_integrity").unwrap();
        assert!(complete.integrity_messages.is_empty());
        assert_eq!(complete.foreign_key_violations.len(), 3);
        assert!(!complete.truncated);
        assert_eq!(complete.foreign_key_violations[0].row_id, Some(1));
        assert_eq!(complete.foreign_key_violations[2].row_id, Some(3));

        let bounded = collect_sqlite_integrity(&connection, 2, "test_integrity").unwrap();
        assert_eq!(bounded.foreign_key_violations.len(), 3);
        assert!(bounded.truncated);
    }

    #[test]
    fn exact_limit_is_conclusive_but_overflow_is_indeterminate() {
        let exact = sqlite_failure_report(
            PathBuf::from("exact.superi"),
            SqliteIntegrityEvidence {
                integrity_messages: (0..MAX_PROJECT_INTEGRITY_FINDINGS)
                    .map(|index| format!("failure {index:02}"))
                    .collect(),
                foreign_key_violations: Vec::new(),
                truncated: false,
            },
        );
        assert_eq!(exact.status(), ProjectIntegrityStatus::Invalid);
        assert!(exact.inspection_complete());
        assert_eq!(exact.findings().len(), MAX_PROJECT_INTEGRITY_FINDINGS);
        assert!(exact
            .findings()
            .iter()
            .all(|finding| finding.code() != ProjectIntegrityFindingCode::InspectionTruncated));

        let overflow = sqlite_failure_report(
            PathBuf::from("overflow.superi"),
            SqliteIntegrityEvidence {
                integrity_messages: (0..=MAX_PROJECT_INTEGRITY_FINDINGS)
                    .map(|index| format!("failure {index:02}"))
                    .collect(),
                foreign_key_violations: Vec::new(),
                truncated: true,
            },
        );
        assert_eq!(overflow.status(), ProjectIntegrityStatus::Indeterminate);
        assert!(!overflow.inspection_complete());
        assert_eq!(overflow.findings().len(), MAX_PROJECT_INTEGRITY_FINDINGS);
        assert!(overflow
            .findings()
            .iter()
            .any(|finding| finding.code() == ProjectIntegrityFindingCode::InspectionTruncated));
    }

    #[test]
    fn source_change_has_precedence_without_losing_truncation_visibility() {
        let mut report = ProjectIntegrityReport {
            path: PathBuf::from("changed.superi"),
            current_schema_revision: PROJECT_SCHEMA_REVISION,
            observed_schema_revision: Some(PROJECT_SCHEMA_REVISION),
            status: ProjectIntegrityStatus::Invalid,
            repair_disposition: ProjectRepairDisposition::RestoreValidatedRecovery,
            identity: None,
            findings: (0..MAX_PROJECT_INTEGRITY_FINDINGS)
                .map(|index| {
                    finding(
                        ProjectIntegrityFindingCode::SqliteIntegrityViolation,
                        ProjectIntegrityStage::SqliteStructure,
                        ErrorCategory::CorruptData,
                        Recoverability::UserCorrectable,
                        ProjectRepairDisposition::RestoreValidatedRecovery,
                        [("detail", format!("failure {index:02}"))],
                    )
                })
                .collect(),
            inspection_complete: true,
        };

        mark_source_changed(&mut report, 4, 5);
        sort_findings(&mut report.findings);

        assert_eq!(report.status(), ProjectIntegrityStatus::Indeterminate);
        assert_eq!(
            report.repair_disposition(),
            ProjectRepairDisposition::RetryInspection
        );
        assert!(!report.inspection_complete());
        assert_eq!(report.findings().len(), MAX_PROJECT_INTEGRITY_FINDINGS);
        assert!(report
            .findings()
            .iter()
            .any(|finding| finding.code() == ProjectIntegrityFindingCode::InspectionTruncated));
        assert!(report
            .findings()
            .iter()
            .any(|finding| finding.code() == ProjectIntegrityFindingCode::SourceChanged));
    }

    #[test]
    fn semantic_not_found_is_invalid_while_open_not_found_is_indeterminate() {
        let semantic_error = Error::new(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "stored relationship is missing",
        )
        .with_context(ErrorContext::new(
            "superi-project.persist",
            "restore_project_document",
        ));
        let semantic = checked_error_report(
            PathBuf::from("semantic.superi"),
            Some(PROJECT_SCHEMA_REVISION),
            ProjectIntegrityStage::SemanticReconstruction,
            semantic_error,
        )
        .unwrap();
        assert_eq!(semantic.status(), ProjectIntegrityStatus::Invalid);
        assert_eq!(
            semantic.findings()[0].code(),
            ProjectIntegrityFindingCode::AggregateInvalid
        );
        assert_eq!(
            semantic.findings()[0].stage(),
            ProjectIntegrityStage::Aggregate
        );

        let open_error = Error::new(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "source is missing",
        )
        .with_context(ErrorContext::new(
            "superi-project.persist",
            "open_project_path",
        ));
        let open = checked_error_report(
            PathBuf::from("missing.superi"),
            None,
            ProjectIntegrityStage::Open,
            open_error,
        )
        .unwrap();
        assert_eq!(open.status(), ProjectIntegrityStatus::Indeterminate);
        assert_eq!(
            open.findings()[0].code(),
            ProjectIntegrityFindingCode::SourceNotFound
        );
    }
}
