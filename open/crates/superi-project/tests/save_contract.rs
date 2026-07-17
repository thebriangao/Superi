use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{Connection, OpenFlags};
use superi_core::error::ErrorCategory;
use superi_core::ids::{GraphId, MediaId, ProjectId, TimelineId};
use superi_core::settings::{ComponentId, SemanticVersion, VersionIdentifier};
use superi_core::time::{RationalTime, Timebase};
use superi_graph::mutate::EditableGraph;
use superi_project::document::{
    ProjectDocument, ProjectGraph, ProjectSnapshot, StandaloneProjectGraph,
};
use superi_project::extensions::{
    ProjectExtensionCommand, ProjectExtensionKind, ProjectExtensionLifecycle,
    ProjectExtensionRecord, ProjectExtensionRecordId,
};
use superi_project::media::{PortableRelativePath, ReferencedMediaPath};
use superi_project::{
    ProjectDatabase, ProjectDestinationCollision, ProjectSaveCommand, ProjectSaveOperation,
    PROJECT_APPLICATION_ID, PROJECT_SCHEMA_REVISION,
};
use superi_timeline::compile::CompiledTimelineGraphValue;
use superi_timeline::model::{EditorialProject, LinkedMediaReference, Timeline};

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);
const MEDIA: MediaId = MediaId::from_raw(0x40ff);

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-save-{label}-{}-{}",
            std::process::id(),
            NEXT_PATH.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn project(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn snapshot(label: &str, identity: u128) -> ProjectSnapshot {
    let root = TimelineId::from_raw(identity + 1);
    let timebase = Timebase::integer(24).unwrap();
    let timeline = Timeline::new(
        root,
        format!("{label} timeline"),
        timebase,
        RationalTime::zero(timebase),
        vec![],
    );
    let project = EditorialProject::new(
        ProjectId::from_raw(identity),
        format!("{label} project"),
        [],
        [timeline],
    )
    .unwrap();
    let mut document = ProjectDocument::new(project, root).unwrap();
    let extension = ProjectExtensionRecord::new(
        ComponentId::new("example.save-extension").unwrap(),
        ProjectExtensionRecordId::new(format!("state-{identity:x}")).unwrap(),
        SemanticVersion::new(1, 0, 0),
        ProjectExtensionKind::new(ComponentId::new("example.unknown-kind").unwrap()),
        VersionIdentifier::new(
            ComponentId::new("example.opaque-state").unwrap(),
            SemanticVersion::new(2, 0, 0),
        ),
        Default::default(),
        Default::default(),
        ProjectExtensionLifecycle::Enabled,
        None,
        vec![0, 0xff, identity as u8],
    )
    .unwrap();
    document
        .execute_extension_command(0, ProjectExtensionCommand::upsert(extension))
        .unwrap();
    document.snapshot()
}

fn relative_media_snapshot() -> ProjectSnapshot {
    let root = TimelineId::from_raw(0x4101);
    let timebase = Timebase::integer(24).unwrap();
    let target = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("Media/camera-original.mov").unwrap(),
    )
    .to_target();
    let media = LinkedMediaReference::with_fingerprint(
        MEDIA,
        "camera original",
        target,
        None,
        "sha256:save-contract",
    )
    .unwrap();
    let timeline = Timeline::new(
        root,
        "relative media timeline",
        timebase,
        RationalTime::zero(timebase),
        vec![],
    );
    let project = EditorialProject::new(
        ProjectId::from_raw(0x4100),
        "relative media project",
        [media],
        [timeline],
    )
    .unwrap();
    ProjectDocument::new(project, root).unwrap().snapshot()
}

fn oversized_snapshot() -> ProjectSnapshot {
    let root = TimelineId::from_raw(0x4201);
    let timebase = Timebase::integer(24).unwrap();
    let timeline = Timeline::new(
        root,
        "bounded timeline",
        timebase,
        RationalTime::zero(timebase),
        vec![],
    );
    let project = EditorialProject::new(
        ProjectId::from_raw(0x4200),
        "bounded project",
        [],
        [timeline],
    )
    .unwrap();
    let mut document = ProjectDocument::new(project, root).unwrap();
    document
        .edit(0, |draft| {
            draft.insert_graph(ProjectGraph::Standalone(
                StandaloneProjectGraph::new(
                    "x".repeat(16 * 1024 + 1),
                    EditableGraph::<CompiledTimelineGraphValue>::new(GraphId::from_raw(0x4202)),
                )
                .unwrap(),
            ))
        })
        .unwrap();
    document.snapshot()
}

fn create_project(path: &Path, snapshot: &ProjectSnapshot) -> ProjectDatabase {
    let mut database = ProjectDatabase::create(path).unwrap();
    database.replace(snapshot).unwrap();
    database
}

fn assert_project(path: &Path, expected: &ProjectSnapshot) {
    let database = ProjectDatabase::open_read_only(path).unwrap();
    assert_eq!(database.load().unwrap().snapshot(), *expected);

    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    let application_id: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .unwrap();
    let schema_revision: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let integrity: String = connection
        .query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(application_id, i64::from(PROJECT_APPLICATION_ID));
    assert_eq!(schema_revision, i64::from(PROJECT_SCHEMA_REVISION));
    assert_eq!(integrity, "ok");
}

fn assert_no_candidates(directory: &Path) {
    let candidates = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name.to_string_lossy().contains(".superi-save-candidate-"))
        .collect::<Vec<_>>();
    assert!(candidates.is_empty(), "orphan candidates: {candidates:?}");
}

#[test]
fn save_replaces_the_active_project_with_exact_current_schema_state() {
    let directory = TempDirectory::new("save");
    let active = directory.project("active.superi");
    let old = snapshot("old", 0x4000);
    let new = snapshot("new", 0x4010);
    let save_as_same_path = snapshot("save as same path", 0x4018);
    let mut database = create_project(&active, &old);
    let canonical_active = fs::canonicalize(&active).unwrap();

    let outcome = database
        .execute_save_command(ProjectSaveCommand::Save, &new)
        .unwrap();

    assert_eq!(outcome.operation(), ProjectSaveOperation::Save);
    assert_eq!(outcome.destination(), canonical_active);
    assert_eq!(outcome.active_path(), Some(canonical_active.as_path()));
    assert!(outcome.replaced_existing());
    assert_eq!(database.active_path(), Some(canonical_active.as_path()));
    assert_eq!(database.load().unwrap().snapshot(), new);
    assert_project(&active, &new);

    let same_path_outcome = database
        .execute_save_command(
            ProjectSaveCommand::SaveAs {
                destination: active.clone(),
                collision: ProjectDestinationCollision::RequireAbsent,
            },
            &save_as_same_path,
        )
        .unwrap();
    assert_eq!(same_path_outcome.operation(), ProjectSaveOperation::SaveAs);
    assert_eq!(database.active_path(), Some(canonical_active.as_path()));
    assert_project(&active, &save_as_same_path);
    assert_no_candidates(directory.path());
}

#[test]
fn save_as_rebinds_memory_authority_and_relative_media_to_the_committed_path() {
    let directory = TempDirectory::new("save-as-memory");
    let destination = directory.project("published.superi");
    let expected = relative_media_snapshot();
    let later = snapshot("later", 0x4020);
    let mut database = ProjectDatabase::memory().unwrap();
    database.replace(&expected).unwrap();

    let outcome = database
        .execute_save_command(
            ProjectSaveCommand::SaveAs {
                destination: destination.clone(),
                collision: ProjectDestinationCollision::RequireAbsent,
            },
            &expected,
        )
        .unwrap();

    let canonical_destination = fs::canonicalize(&destination).unwrap();
    assert_eq!(outcome.operation(), ProjectSaveOperation::SaveAs);
    assert_eq!(outcome.destination(), canonical_destination);
    assert_eq!(outcome.active_path(), Some(canonical_destination.as_path()));
    assert!(!outcome.replaced_existing());
    assert_eq!(
        database.active_path(),
        Some(canonical_destination.as_path())
    );
    let loaded = database.load().unwrap();
    let target = loaded
        .editorial_project()
        .media_reference(MEDIA)
        .unwrap()
        .target();
    let reference = ReferencedMediaPath::from_target(target).unwrap().unwrap();
    assert_eq!(
        reference.resolve(database.active_path().unwrap()).unwrap(),
        fs::canonicalize(directory.path())
            .unwrap()
            .join("Media/camera-original.mov")
    );
    assert_project(&destination, &expected);

    database.replace(&later).unwrap();
    assert_project(&destination, &later);
    assert_no_candidates(directory.path());
}

#[test]
fn read_only_copy_backup_and_save_as_preserve_source_then_publish_writable_authority() {
    let directory = TempDirectory::new("read-only");
    let source = directory.project("source.superi");
    let copy = directory.project("copy.superi");
    let backup = directory.project("backup.superi");
    let rebound = directory.project("rebound.superi");
    let old = snapshot("read-only source", 0x4030);
    let live = snapshot("live edits", 0x4040);
    let later = snapshot("post rebind", 0x4050);
    drop(create_project(&source, &old));
    let source_bytes = fs::read(&source).unwrap();
    let mut database = ProjectDatabase::open_read_only(&source).unwrap();
    let canonical_source = fs::canonicalize(&source).unwrap();

    database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: copy.clone(),
                collision: ProjectDestinationCollision::RequireAbsent,
            },
            &live,
        )
        .unwrap();
    database
        .execute_save_command(
            ProjectSaveCommand::Backup {
                destination: backup.clone(),
            },
            &live,
        )
        .unwrap();
    assert_eq!(database.active_path(), Some(canonical_source.as_path()));
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_project(&copy, &live);
    assert_project(&backup, &live);

    database
        .execute_save_command(
            ProjectSaveCommand::SaveAs {
                destination: rebound.clone(),
                collision: ProjectDestinationCollision::RequireAbsent,
            },
            &live,
        )
        .unwrap();
    let canonical_rebound = fs::canonicalize(&rebound).unwrap();
    assert_eq!(database.active_path(), Some(canonical_rebound.as_path()));
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_project(&rebound, &live);

    database.replace(&later).unwrap();
    assert_project(&rebound, &later);
    assert_project(&source, &old);
    assert_no_candidates(directory.path());
}

#[test]
fn save_copy_preserves_active_identity_and_the_subsequent_save_target() {
    let directory = TempDirectory::new("save-copy");
    let active = directory.project("active.superi");
    let copy = directory.project("copy.superi");
    let old = snapshot("copy source", 0x4060);
    let copied = snapshot("copied live state", 0x4070);
    let later = snapshot("later active state", 0x4080);
    let replacement = snapshot("replacement copy", 0x4090);
    let mut database = create_project(&active, &old);
    let canonical_active = fs::canonicalize(&active).unwrap();

    let outcome = database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: copy.clone(),
                collision: ProjectDestinationCollision::RequireAbsent,
            },
            &copied,
        )
        .unwrap();
    assert_eq!(outcome.operation(), ProjectSaveOperation::SaveCopy);
    assert_eq!(outcome.active_path(), Some(canonical_active.as_path()));
    assert!(!outcome.replaced_existing());
    assert_project(&active, &old);
    assert_project(&copy, &copied);

    database
        .execute_save_command(ProjectSaveCommand::Save, &later)
        .unwrap();
    assert_project(&active, &later);
    assert_project(&copy, &copied);

    let collision = database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: copy.clone(),
                collision: ProjectDestinationCollision::RequireAbsent,
            },
            &replacement,
        )
        .unwrap_err();
    assert_eq!(collision.category(), ErrorCategory::Conflict);
    assert_project(&copy, &copied);

    let replacement_outcome = database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: copy.clone(),
                collision: ProjectDestinationCollision::ReplaceExisting,
            },
            &replacement,
        )
        .unwrap();
    assert!(replacement_outcome.replaced_existing());
    assert_eq!(database.active_path(), Some(canonical_active.as_path()));
    assert_project(&active, &later);
    assert_project(&copy, &replacement);
    assert_no_candidates(directory.path());
}

#[test]
fn backup_captures_live_state_never_clobbers_and_never_rebinds() {
    let directory = TempDirectory::new("backup");
    let active = directory.project("active.superi");
    let backup = directory.project("backup.superi");
    let old = snapshot("backup source", 0x40a0);
    let live = snapshot("unsaved live state", 0x40b0);
    let conflicting = snapshot("must not publish", 0x40c0);
    let mut database = create_project(&active, &old);
    let canonical_active = fs::canonicalize(&active).unwrap();

    let outcome = database
        .execute_save_command(
            ProjectSaveCommand::Backup {
                destination: backup.clone(),
            },
            &live,
        )
        .unwrap();
    assert_eq!(outcome.operation(), ProjectSaveOperation::Backup);
    assert_eq!(outcome.active_path(), Some(canonical_active.as_path()));
    assert!(!outcome.replaced_existing());
    assert_project(&active, &old);
    assert_project(&backup, &live);

    let error = database
        .execute_save_command(
            ProjectSaveCommand::Backup {
                destination: backup.clone(),
            },
            &conflicting,
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(database.active_path(), Some(canonical_active.as_path()));
    assert_project(&active, &old);
    assert_project(&backup, &live);
    assert_no_candidates(directory.path());
}

#[test]
fn invalid_source_authority_aliases_and_destination_types_fail_without_publication() {
    let directory = TempDirectory::new("invalid");
    let active = directory.project("active.superi");
    let old = snapshot("valid source", 0x40d0);
    let live = snapshot("live state", 0x40e0);
    let mut memory = ProjectDatabase::memory().unwrap();
    memory.replace(&live).unwrap();
    let no_active_path = memory
        .execute_save_command(ProjectSaveCommand::Save, &live)
        .unwrap_err();
    assert_eq!(no_active_path.category(), ErrorCategory::Conflict);

    drop(create_project(&active, &old));
    let canonical_active = fs::canonicalize(&active).unwrap();
    let mut read_only = ProjectDatabase::open_read_only(&active).unwrap();
    let denied = read_only
        .execute_save_command(ProjectSaveCommand::Save, &live)
        .unwrap_err();
    assert_eq!(denied.category(), ErrorCategory::PermissionDenied);
    let denied_same_path = read_only
        .execute_save_command(
            ProjectSaveCommand::SaveAs {
                destination: active.clone(),
                collision: ProjectDestinationCollision::ReplaceExisting,
            },
            &live,
        )
        .unwrap_err();
    assert_eq!(denied_same_path.category(), ErrorCategory::PermissionDenied);

    let mut database = ProjectDatabase::open(&active).unwrap();
    for command in [
        ProjectSaveCommand::SaveCopy {
            destination: active.clone(),
            collision: ProjectDestinationCollision::ReplaceExisting,
        },
        ProjectSaveCommand::Backup {
            destination: active.clone(),
        },
    ] {
        let error = database.execute_save_command(command, &live).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Conflict);
        assert_eq!(database.active_path(), Some(canonical_active.as_path()));
        assert_project(&active, &old);
    }

    let directory_destination = directory.project("directory.superi");
    fs::create_dir(&directory_destination).unwrap();
    let directory_error = database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: directory_destination,
                collision: ProjectDestinationCollision::ReplaceExisting,
            },
            &live,
        )
        .unwrap_err();
    assert_eq!(directory_error.category(), ErrorCategory::InvalidInput);

    let foreign = directory.project("foreign.superi");
    fs::write(&foreign, b"not a project").unwrap();
    let foreign_error = database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: foreign.clone(),
                collision: ProjectDestinationCollision::ReplaceExisting,
            },
            &live,
        )
        .unwrap_err();
    assert!(matches!(
        foreign_error.category(),
        ErrorCategory::Unsupported | ErrorCategory::CorruptData
    ));
    assert_eq!(fs::read(&foreign).unwrap(), b"not a project");

    let missing_parent = directory.project("missing").join("project.superi");
    let missing_error = database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: missing_parent,
                collision: ProjectDestinationCollision::RequireAbsent,
            },
            &live,
        )
        .unwrap_err();
    assert_eq!(missing_error.category(), ErrorCategory::NotFound);
    assert_project(&active, &old);
    assert_no_candidates(directory.path());
}

#[test]
fn bounded_snapshot_failure_precedes_destination_inspection_and_candidate_creation() {
    let directory = TempDirectory::new("bounded");
    let destination = directory.project("destination.superi");
    let old = snapshot("bounded destination", 0x4300);
    drop(create_project(&destination, &old));
    let bytes_before = fs::read(&destination).unwrap();
    let mut database = ProjectDatabase::memory().unwrap();

    let error = database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: destination.clone(),
                collision: ProjectDestinationCollision::ReplaceExisting,
            },
            &oversized_snapshot(),
        )
        .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(fs::read(&destination).unwrap(), bytes_before);
    assert_project(&destination, &old);
    assert_no_candidates(directory.path());
}

#[cfg(unix)]
#[test]
fn unix_non_utf8_paths_alias_detection_and_permissions_preserve_file_semantics() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::{symlink, PermissionsExt};

    let directory = TempDirectory::new("unix");
    let non_utf8_destination = directory
        .path()
        .join(OsString::from_vec(b"non-utf8-\xff.superi".to_vec()));
    let fallback_destination = directory.project("permission-source.superi");
    let hard_link_alias = directory.project("hard-link-alias.superi");
    let dangling = directory.project("dangling.superi");
    let first = snapshot("non utf8", 0x4310);
    let second = snapshot("permission preserving", 0x4320);
    let mut database = ProjectDatabase::memory().unwrap();
    database.replace(&first).unwrap();

    let non_utf8_result = database.execute_save_command(
        ProjectSaveCommand::SaveAs {
            destination: non_utf8_destination.clone(),
            collision: ProjectDestinationCollision::RequireAbsent,
        },
        &first,
    );
    let destination = match non_utf8_result {
        Ok(_) => {
            assert_project(&non_utf8_destination, &first);
            non_utf8_destination
        }
        Err(error) => {
            assert!(matches!(
                error.category(),
                ErrorCategory::InvalidInput
                    | ErrorCategory::Unsupported
                    | ErrorCategory::Unavailable
            ));
            assert!(!non_utf8_destination.exists());
            assert_no_candidates(directory.path());
            database
                .execute_save_command(
                    ProjectSaveCommand::SaveAs {
                        destination: fallback_destination.clone(),
                        collision: ProjectDestinationCollision::RequireAbsent,
                    },
                    &first,
                )
                .unwrap();
            fallback_destination
        }
    };
    assert_project(&destination, &first);
    assert_eq!(
        fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
        0o600
    );

    fs::set_permissions(&destination, fs::Permissions::from_mode(0o640)).unwrap();
    database
        .execute_save_command(ProjectSaveCommand::Save, &second)
        .unwrap();
    assert_project(&destination, &second);
    assert_eq!(
        fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
        0o640
    );

    fs::hard_link(&destination, &hard_link_alias).unwrap();
    let alias_error = database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: hard_link_alias,
                collision: ProjectDestinationCollision::ReplaceExisting,
            },
            &first,
        )
        .unwrap_err();
    assert_eq!(alias_error.category(), ErrorCategory::Conflict);

    symlink(directory.project("missing-target.superi"), &dangling).unwrap();
    let dangling_error = database
        .execute_save_command(
            ProjectSaveCommand::SaveCopy {
                destination: dangling.clone(),
                collision: ProjectDestinationCollision::ReplaceExisting,
            },
            &first,
        )
        .unwrap_err();
    assert!(matches!(
        dangling_error.category(),
        ErrorCategory::NotFound | ErrorCategory::InvalidInput
    ));
    assert!(fs::symlink_metadata(&dangling)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_project(&destination, &second);
}
