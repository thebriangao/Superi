use std::fs;

use superi_session::migration::{LegacySessionMigrator, MigrationOutcome};

#[test]
fn valid_legacy_state_is_copied_exactly_and_repeated_migration_is_idempotent() {
    let temporary = tempfile::tempdir().expect("temporary migration root");
    let legacy = temporary.path().join("legacy");
    let session = temporary.path().join("session");
    fs::create_dir_all(&legacy).expect("legacy directory");
    let project = br#"{"active":{"project_id":"orion"},"unknown":{"future":true}}"#;
    let media = br#"{"projects":{"orion":{"bins":[],"extension":{"keep":9}}}}"#;
    fs::write(legacy.join("project-session.json"), project).expect("project source");
    fs::write(legacy.join("media-library-views.json"), media).expect("media source");

    let first = LegacySessionMigrator::new(&legacy, &session)
        .migrate()
        .expect("first migration");
    assert_eq!(first.outcome(), MigrationOutcome::Migrated);
    assert_eq!(
        fs::read(session.join("project-session.json")).unwrap(),
        project
    );
    assert_eq!(
        fs::read(session.join("media-library-views.json")).unwrap(),
        media
    );
    assert_eq!(
        fs::read(legacy.join("project-session.json")).unwrap(),
        project
    );

    let second = LegacySessionMigrator::new(&legacy, &session)
        .migrate()
        .expect("repeated migration");
    assert_eq!(second.outcome(), MigrationOutcome::AlreadyCurrent);
    assert_eq!(first.files(), second.files());
}

#[test]
fn corrupt_or_conflicting_state_fails_closed_without_changing_the_source() {
    let temporary = tempfile::tempdir().expect("temporary migration root");
    let legacy = temporary.path().join("legacy");
    let session = temporary.path().join("session");
    fs::create_dir_all(&legacy).expect("legacy directory");
    fs::write(legacy.join("project-session.json"), b"{not-json").expect("corrupt source");

    let error = LegacySessionMigrator::new(&legacy, &session)
        .migrate()
        .expect_err("corrupt source fails");
    assert!(error.to_string().contains("invalid JSON"));
    assert_eq!(
        fs::read(legacy.join("project-session.json")).unwrap(),
        b"{not-json"
    );
    assert!(!session.join("project-session.json").exists());

    fs::write(legacy.join("project-session.json"), b"{}").expect("repair source");
    fs::create_dir_all(&session).expect("session directory");
    fs::write(
        session.join("project-session.json"),
        b"{\"different\":true}",
    )
    .expect("conflicting destination");
    let error = LegacySessionMigrator::new(&legacy, &session)
        .migrate()
        .expect_err("conflicting destination fails");
    assert!(error.to_string().contains("conflicts"));
    assert_eq!(
        fs::read(session.join("project-session.json")).unwrap(),
        b"{\"different\":true}"
    );
}
