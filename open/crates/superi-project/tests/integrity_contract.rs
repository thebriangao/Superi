use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{params, Connection};
use superi_audio::mixing::{ChannelMap, ClipMixControls, ClipMixMutation};
use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, GraphId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::{ComponentId, SemanticVersion, SettingValue, VersionIdentifier};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_graph::mutate::EditableGraph;
use superi_project::document::{ProjectDocument, ProjectGraph, StandaloneProjectGraph};
use superi_project::extensions::{
    ProjectExtensionCommand, ProjectExtensionKind, ProjectExtensionLifecycle,
    ProjectExtensionRecord, ProjectExtensionRecordId,
};
use superi_project::settings::{
    ProjectSettingMutation, ProjectSettingsTransaction, AUDIO_SAMPLE_RATE_KEY,
};
use superi_project::{
    execute_project_integrity_command, ProjectDatabase, ProjectIntegrityCommand,
    ProjectIntegrityFindingCode, ProjectIntegrityStatus, ProjectRepairDisposition,
    PROJECT_APPLICATION_ID, PROJECT_FORMAT_VERSION, PROJECT_SCHEMA_REVISION,
};
use superi_timeline::compile::CompiledTimelineGraphValue;
use superi_timeline::model::{
    Clip, ClipSource, EditorialProject, LinkedMediaReference, Timeline, Track, TrackItem,
    TrackSemantics, VideoCompositing, VideoTrackSemantics,
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const PROJECT: ProjectId = ProjectId::from_raw(0xc1200);
const ROOT: TimelineId = TimelineId::from_raw(0xc1201);
const MEDIA: MediaId = MediaId::from_raw(0xc1202);
const STANDALONE: GraphId = GraphId::from_raw(0xc1203);
const TRACK: TrackId = TrackId::from_raw(0xc1204);
const CLIP: ClipId = ClipId::from_raw(0xc1205);

const LEGACY_FORMAT_VERSION: &str = "0.9.0";
const LEGACY_PROJECT_METADATA_SCHEMA: &str = "CREATE TABLE project_metadata (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format TEXT NOT NULL CHECK (format = 'superi.project'), format_version TEXT NOT NULL, primitive_schema_revision INTEGER NOT NULL CHECK (primitive_schema_revision > 0), project_id BLOB NOT NULL CHECK (length(project_id) = 16), document_revision TEXT NOT NULL, root_timeline_id BLOB NOT NULL CHECK (length(root_timeline_id) = 16)) STRICT";
const LEGACY_TIMELINE_COMPONENT_SCHEMA: &str = "CREATE TABLE timeline_component (singleton INTEGER PRIMARY KEY CHECK (singleton = 1), format_revision INTEGER NOT NULL CHECK (format_revision >= 0), document BLOB NOT NULL CHECK (length(document) <= 67108864)) STRICT";
const LEGACY_GRAPH_COMPONENTS_SCHEMA: &str = "CREATE TABLE graph_components (graph_id BLOB PRIMARY KEY CHECK (length(graph_id) = 16), graph_kind TEXT NOT NULL CHECK (graph_kind IN ('timeline', 'standalone')), root_timeline_id BLOB CHECK (root_timeline_id IS NULL OR length(root_timeline_id) = 16), name TEXT, graph_revision TEXT NOT NULL, format_revision INTEGER NOT NULL CHECK (format_revision >= 0), document BLOB NOT NULL CHECK (length(document) <= 67108864), CHECK ((graph_kind = 'timeline' AND root_timeline_id IS NOT NULL AND name IS NULL) OR (graph_kind = 'standalone' AND root_timeline_id IS NULL AND name IS NOT NULL AND length(name) > 0))) STRICT, WITHOUT ROWID";

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-integrity-{label}-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn project(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn project_document() -> ProjectDocument {
    project_document_with_modern_state(true)
}

fn legacy_project_document() -> ProjectDocument {
    project_document_with_modern_state(false)
}

fn project_document_with_modern_state(include_modern_state: bool) -> ProjectDocument {
    let rate = Timebase::integer(24).unwrap();
    let clip_range =
        TimeRange::new(RationalTime::zero(rate), Duration::new(24, rate).unwrap()).unwrap();
    let clip = Clip::new(
        CLIP,
        "integrity clip",
        ClipSource::Media(MEDIA),
        clip_range,
        clip_range,
    )
    .unwrap();
    let track = Track::new(
        TRACK,
        "V1",
        TrackSemantics::Video(VideoTrackSemantics::new(
            FrameRate::FPS_24,
            VideoCompositing::Over,
        )),
        vec![TrackItem::Clip(clip)],
    );
    let timeline = Timeline::new(
        ROOT,
        "integrity timeline",
        rate,
        RationalTime::zero(rate),
        vec![track],
    );
    let media = LinkedMediaReference::with_fingerprint(
        MEDIA,
        "camera original",
        "urn:camera:original",
        Some(clip_range),
        "sha256:integrity-contract",
    )
    .unwrap();
    let mut editorial =
        EditorialProject::new(PROJECT, "integrity project", [media], [timeline]).unwrap();
    editorial
        .edit(0, |draft| {
            draft
                .media_reference_mut(MEDIA)?
                .consider_relink("urn:camera:replacement", "sha256:wrong")?;
            Ok(())
        })
        .unwrap();

    let mut document = ProjectDocument::new(editorial, ROOT).unwrap();
    document
        .edit(0, |draft| {
            draft.insert_graph(ProjectGraph::Standalone(
                StandaloneProjectGraph::new(
                    "retained analysis",
                    EditableGraph::<CompiledTimelineGraphValue>::new(STANDALONE),
                )
                .unwrap(),
            ))?;
            if include_modern_state {
                let stereo = ChannelLayout::stereo();
                let controls = ClipMixControls::new(
                    stereo.clone(),
                    stereo,
                    [ChannelMap::new(
                        ChannelPosition::FrontLeft,
                        ChannelPosition::FrontRight,
                        0.5,
                    )?],
                )?
                .with_gain(0.75)?
                .with_fades(240, 480)?;
                draft
                    .clip_mix_state_mut()
                    .apply(0, &[ClipMixMutation::set(CLIP, controls)])?;
            }
            Ok(())
        })
        .unwrap();
    if include_modern_state {
        document
            .execute_settings_transaction(
                ProjectSettingsTransaction::new(
                    document.revision(),
                    vec![ProjectSettingMutation::set(
                        AUDIO_SAMPLE_RATE_KEY,
                        SettingValue::Integer(96_000),
                    )
                    .unwrap()],
                )
                .unwrap(),
            )
            .unwrap();
        let extension = ProjectExtensionRecord::new(
            ComponentId::new("example.integrity-extension").unwrap(),
            ProjectExtensionRecordId::new("opaque-state").unwrap(),
            SemanticVersion::new(1, 2, 3),
            ProjectExtensionKind::new(ComponentId::new("example.future-kind").unwrap()),
            VersionIdentifier::new(
                ComponentId::new("example.future-state").unwrap(),
                SemanticVersion::new(4, 5, 6),
            ),
            Default::default(),
            Default::default(),
            ProjectExtensionLifecycle::Enabled,
            None,
            vec![0, 1, 0xfe, 0xff],
        )
        .unwrap();
        document
            .execute_extension_command(
                document.revision(),
                ProjectExtensionCommand::upsert(extension),
            )
            .unwrap();
    }
    document
}

fn persist(path: &Path, document: &ProjectDocument) {
    let mut database = ProjectDatabase::create(path).unwrap();
    database.replace(&document.snapshot()).unwrap();
}

fn validate(path: &Path) -> superi_project::ProjectIntegrityReport {
    execute_project_integrity_command(ProjectIntegrityCommand::Validate {
        path: path.to_path_buf(),
    })
    .unwrap()
}

fn mutate(path: &Path, sql: &str) {
    let connection = Connection::open(path).unwrap();
    connection.execute_batch(sql).unwrap();
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

fn downgrade_to_schema_zero(path: &Path) {
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
            "DROP TABLE command_log_records;DROP TABLE command_log_metadata;DROP TABLE extension_records;DROP TABLE audio_component;DROP TABLE graph_components;DROP TABLE settings_component;DROP TABLE timeline_component;DROP TABLE project_metadata;{LEGACY_PROJECT_METADATA_SCHEMA};{LEGACY_TIMELINE_COMPONENT_SCHEMA};{LEGACY_GRAPH_COMPONENTS_SCHEMA};PRAGMA user_version = 0;"
        ))
        .unwrap();
    transaction
        .execute(
            "INSERT INTO project_metadata (singleton, format, format_version, primitive_schema_revision, project_id, document_revision, root_timeline_id) VALUES (1, 'superi.project', ?1, ?2, ?3, ?4, ?5)",
            params![
                LEGACY_FORMAT_VERSION,
                metadata.0,
                metadata.1,
                metadata.2,
                metadata.3
            ],
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

fn assert_single_finding(
    report: &superi_project::ProjectIntegrityReport,
    code: ProjectIntegrityFindingCode,
) {
    assert_eq!(report.findings().len(), 1, "{report:#?}");
    assert_eq!(report.findings()[0].code(), code);
}

#[test]
fn valid_current_project_is_deterministic_role_neutral_and_read_only() {
    let directory = TempDirectory::new("valid");
    let path = directory.project("current.superi");
    let document = project_document();
    persist(&path, &document);
    let before = std::fs::read(&path).unwrap();
    let command = ProjectIntegrityCommand::Validate { path: path.clone() };

    let editor = execute_project_integrity_command(command.clone()).unwrap();
    let script = execute_project_integrity_command(command.clone()).unwrap();
    let headless = execute_project_integrity_command(command).unwrap();

    assert_eq!(editor, script);
    assert_eq!(script, headless);
    assert_eq!(editor.status(), ProjectIntegrityStatus::Valid);
    assert_eq!(editor.status().code(), "valid");
    assert_eq!(
        editor.repair_disposition(),
        ProjectRepairDisposition::NotNeeded
    );
    assert!(editor.inspection_complete());
    assert!(editor.findings().is_empty());
    assert_eq!(
        editor.observed_schema_revision(),
        Some(PROJECT_SCHEMA_REVISION)
    );
    assert_eq!(editor.current_schema_revision(), PROJECT_SCHEMA_REVISION);
    let identity = editor.identity().unwrap();
    assert_eq!(identity.project_id(), PROJECT);
    assert_eq!(identity.document_revision(), document.revision());
    assert_eq!(identity.root_timeline_id(), ROOT);
    assert_eq!(identity.source_schema_revision(), PROJECT_SCHEMA_REVISION);
    assert_eq!(identity.project_format_version(), PROJECT_FORMAT_VERSION);
    assert_eq!(identity.media_count(), 1);
    assert_eq!(identity.graph_count(), 2);
    assert_eq!(identity.extension_count(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let reopened = ProjectDatabase::open_read_only(&path)
        .unwrap()
        .load()
        .unwrap();
    assert_eq!(reopened.snapshot(), document.snapshot());
    assert_eq!(
        reopened.settings().integer(AUDIO_SAMPLE_RATE_KEY),
        Some(96_000)
    );
    assert!(reopened.clip_mix_state().controls(CLIP).is_some());
    assert_eq!(reopened.extension_records().len(), 1);
}

#[test]
fn supported_legacy_project_reports_only_forward_migration_without_mutation() {
    let directory = TempDirectory::new("legacy");
    let path = directory.project("legacy.superi");
    let document = legacy_project_document();
    persist(&path, &document);
    downgrade_to_schema_zero(&path);
    let before = std::fs::read(&path).unwrap();

    let report = validate(&path);

    assert_eq!(report.status(), ProjectIntegrityStatus::MigrationRequired);
    assert_eq!(
        report.repair_disposition(),
        ProjectRepairDisposition::MigrateForward
    );
    assert!(report.inspection_complete());
    assert_eq!(report.observed_schema_revision(), Some(0));
    assert_eq!(report.identity().unwrap().project_id(), PROJECT);
    assert_eq!(report.identity().unwrap().source_schema_revision(), 0);
    assert_single_finding(
        &report,
        ProjectIntegrityFindingCode::SchemaMigrationRequired,
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let mut migrated = ProjectDatabase::open(&path).unwrap();
    assert!(migrated.was_migrated());
    assert_eq!(migrated.load().unwrap().snapshot(), document.snapshot());
    migrated.replace(&document.snapshot()).unwrap();
}

#[test]
fn unsupported_identity_future_schema_and_non_sqlite_input_stay_distinct() {
    let directory = TempDirectory::new("unsupported");
    let document = project_document();

    let wrong_identity = directory.project("wrong-identity.superi");
    persist(&wrong_identity, &document);
    mutate(&wrong_identity, "PRAGMA application_id = 1");
    let before = std::fs::read(&wrong_identity).unwrap();
    let report = validate(&wrong_identity);
    assert_eq!(report.status(), ProjectIntegrityStatus::Unsupported);
    assert_eq!(
        report.repair_disposition(),
        ProjectRepairDisposition::NotNeeded
    );
    assert!(report.identity().is_none());
    assert_single_finding(
        &report,
        ProjectIntegrityFindingCode::ApplicationIdentityMismatch,
    );
    assert_eq!(std::fs::read(&wrong_identity).unwrap(), before);

    let future = directory.project("future.superi");
    persist(&future, &document);
    mutate(
        &future,
        &format!("PRAGMA user_version = {}", PROJECT_SCHEMA_REVISION + 1),
    );
    let before = std::fs::read(&future).unwrap();
    let report = validate(&future);
    assert_eq!(report.status(), ProjectIntegrityStatus::Unsupported);
    assert_eq!(
        report.repair_disposition(),
        ProjectRepairDisposition::UseNewerApplication
    );
    assert_eq!(
        report.observed_schema_revision(),
        Some(PROJECT_SCHEMA_REVISION + 1)
    );
    assert_single_finding(
        &report,
        ProjectIntegrityFindingCode::SchemaRevisionUnsupported,
    );
    assert_eq!(std::fs::read(&future).unwrap(), before);

    let foreign = directory.project("foreign.superi");
    std::fs::write(&foreign, b"not a sqlite application file").unwrap();
    let before = std::fs::read(&foreign).unwrap();
    let report = validate(&foreign);
    assert_eq!(report.status(), ProjectIntegrityStatus::Unsupported);
    assert_eq!(
        report.repair_disposition(),
        ProjectRepairDisposition::NotNeeded
    );
    assert_single_finding(&report, ProjectIntegrityFindingCode::NotSqliteDatabase);
    assert_eq!(std::fs::read(&foreign).unwrap(), before);
}

#[test]
fn missing_source_is_indeterminate_and_never_created() {
    let directory = TempDirectory::new("missing");
    let path = directory.project("missing.superi");

    let report = validate(&path);

    assert_eq!(report.status(), ProjectIntegrityStatus::Indeterminate);
    assert_eq!(
        report.repair_disposition(),
        ProjectRepairDisposition::ResolveAccess
    );
    assert!(!report.inspection_complete());
    assert!(report.identity().is_none());
    assert_single_finding(&report, ProjectIntegrityFindingCode::SourceNotFound);
    assert!(!path.exists());
}

#[test]
fn component_and_manifest_corruption_report_recovery_without_rewriting_evidence() {
    let directory = TempDirectory::new("corrupt");
    let document = project_document();
    let authority = directory.project("authority.superi");
    persist(&authority, &document);
    let authority_bytes = std::fs::read(&authority).unwrap();

    for (name, mutation, code) in [
        (
            "timeline",
            "UPDATE timeline_component SET document = zeroblob(byte_length)",
            ProjectIntegrityFindingCode::TimelineComponentInvalid,
        ),
        (
            "graph",
            "UPDATE graph_components SET document = zeroblob(byte_length)",
            ProjectIntegrityFindingCode::GraphComponentInvalid,
        ),
        (
            "settings",
            "UPDATE settings_component SET document = zeroblob(byte_length)",
            ProjectIntegrityFindingCode::SettingsComponentInvalid,
        ),
        (
            "audio",
            "UPDATE audio_component SET document = zeroblob(byte_length)",
            ProjectIntegrityFindingCode::AudioComponentInvalid,
        ),
        (
            "extension-metadata",
            "UPDATE extension_records SET metadata = zeroblob(metadata_byte_length)",
            ProjectIntegrityFindingCode::ExtensionComponentInvalid,
        ),
        (
            "extension-payload",
            "UPDATE extension_records SET payload = zeroblob(payload_byte_length)",
            ProjectIntegrityFindingCode::ExtensionComponentInvalid,
        ),
        (
            "manifest",
            "UPDATE project_metadata SET manifest_sha256 = zeroblob(32)",
            ProjectIntegrityFindingCode::ManifestInvalid,
        ),
        (
            "missing-settings",
            "DELETE FROM settings_component",
            ProjectIntegrityFindingCode::SettingsComponentInvalid,
        ),
        (
            "schema-objects",
            "CREATE TABLE unexpected_project_state (value INTEGER) STRICT",
            ProjectIntegrityFindingCode::SchemaObjectsInvalid,
        ),
    ] {
        let path = directory.project(&format!("{name}.superi"));
        persist(&path, &document);
        mutate(&path, mutation);
        let before = std::fs::read(&path).unwrap();
        let report = validate(&path);

        assert_eq!(report.status(), ProjectIntegrityStatus::Invalid, "{name}");
        assert_eq!(
            report.repair_disposition(),
            ProjectRepairDisposition::RestoreValidatedRecovery,
            "{name}"
        );
        assert!(report.identity().is_none(), "{name}");
        assert_single_finding(&report, code);
        assert_eq!(
            report.findings()[0].category(),
            ErrorCategory::CorruptData,
            "{name}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "{name}");
        assert_eq!(
            std::fs::read(&authority).unwrap(),
            authority_bytes,
            "{name}"
        );
        assert_eq!(
            ProjectDatabase::open_read_only(&authority)
                .unwrap()
                .load()
                .unwrap()
                .snapshot(),
            document.snapshot(),
            "{name}"
        );
    }
}

#[test]
fn repeated_invalid_reports_retain_stable_codes_and_sorted_evidence() {
    let directory = TempDirectory::new("stable-invalid");
    let path = directory.project("manifest.superi");
    persist(&path, &project_document());
    mutate(
        &path,
        "UPDATE project_metadata SET manifest_sha256 = zeroblob(32)",
    );

    let first = validate(&path);
    let second = validate(&path);

    assert_eq!(first, second);
    assert_eq!(first.findings()[0].code().code(), "manifest_invalid");
    let evidence_keys = first.findings()[0]
        .evidence()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert!(evidence_keys.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(
        first.findings()[0].repair_disposition(),
        ProjectRepairDisposition::RestoreValidatedRecovery
    );
    assert_eq!(
        first.findings()[0].recoverability().code(),
        "user_correctable"
    );
    assert_eq!(
        i64::from(PROJECT_APPLICATION_ID),
        Connection::open(&path)
            .unwrap()
            .pragma_query_value(None, "application_id", |row| row.get::<_, i64>(0))
            .unwrap()
    );
    assert_eq!(STABLE_PRIMITIVE_SCHEMA_REVISION, 1);
}
