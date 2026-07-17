use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::SemanticVersion;
use superi_project::{
    negotiate_project_format, project_format_support, ProjectFormatIdentity,
    ProjectVersionDisposition, ProjectVersionReason, PROJECT_APPLICATION_ID, PROJECT_FORMAT,
    PROJECT_SCHEMA_REVISION,
};

#[test]
fn project_format_registry_is_complete_and_authoritative() {
    let support = project_format_support();

    assert_eq!(support.application_id(), PROJECT_APPLICATION_ID);
    assert_eq!(support.format(), PROJECT_FORMAT);
    assert_eq!(
        support.primitive_schema_revision(),
        STABLE_PRIMITIVE_SCHEMA_REVISION
    );
    assert_eq!(
        support
            .releases()
            .iter()
            .map(|release| (
                release.schema_revision(),
                release.format_version().to_string()
            ))
            .collect::<Vec<_>>(),
        vec![
            (0, "0.9.0".to_owned()),
            (1, "1.0.0".to_owned()),
            (2, "1.1.0".to_owned()),
            (3, "1.2.0".to_owned()),
            (4, "1.3.0".to_owned()),
            (5, "1.4.0".to_owned()),
        ]
    );
    assert_eq!(support.current().schema_revision(), PROJECT_SCHEMA_REVISION);
}

#[test]
fn project_negotiation_distinguishes_current_migration_future_and_invalid_inputs() {
    let current = negotiate_project_format(ProjectFormatIdentity::new(
        PROJECT_APPLICATION_ID,
        PROJECT_FORMAT,
        SemanticVersion::new(1, 4, 0),
        STABLE_PRIMITIVE_SCHEMA_REVISION,
        5,
    ));
    assert_eq!(current.disposition(), ProjectVersionDisposition::Current);
    assert!(current.reasons().is_empty());
    assert!(current.migration_path().is_empty());

    let legacy = negotiate_project_format(ProjectFormatIdentity::new(
        PROJECT_APPLICATION_ID,
        PROJECT_FORMAT,
        SemanticVersion::new(1, 0, 0),
        STABLE_PRIMITIVE_SCHEMA_REVISION,
        1,
    ));
    assert_eq!(
        legacy.disposition(),
        ProjectVersionDisposition::MigrationRequired
    );
    assert_eq!(
        legacy.reasons(),
        &[ProjectVersionReason::RegisteredMigration]
    );
    assert_eq!(
        legacy
            .migration_path()
            .iter()
            .map(|release| release.schema_revision())
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5]
    );

    let future = negotiate_project_format(ProjectFormatIdentity::new(
        PROJECT_APPLICATION_ID,
        PROJECT_FORMAT,
        SemanticVersion::new(2, 0, 0),
        STABLE_PRIMITIVE_SCHEMA_REVISION + 1,
        PROJECT_SCHEMA_REVISION + 1,
    ));
    assert_eq!(
        future.disposition(),
        ProjectVersionDisposition::RequiresNewerApplication
    );
    assert_eq!(
        future.reasons(),
        &[
            ProjectVersionReason::FutureSchemaRevision,
            ProjectVersionReason::FutureSemanticFormat,
            ProjectVersionReason::FuturePrimitiveRevision,
        ]
    );

    let inconsistent = negotiate_project_format(ProjectFormatIdentity::new(
        PROJECT_APPLICATION_ID,
        PROJECT_FORMAT,
        SemanticVersion::new(1, 2, 0),
        STABLE_PRIMITIVE_SCHEMA_REVISION,
        2,
    ));
    assert_eq!(
        inconsistent.disposition(),
        ProjectVersionDisposition::Invalid
    );
    assert_eq!(
        inconsistent.reasons(),
        &[ProjectVersionReason::InconsistentSchemaFormat]
    );

    let foreign = negotiate_project_format(ProjectFormatIdentity::new(
        7,
        "other.project",
        SemanticVersion::new(1, 4, 0),
        STABLE_PRIMITIVE_SCHEMA_REVISION,
        5,
    ));
    assert_eq!(
        foreign.disposition(),
        ProjectVersionDisposition::Unsupported
    );
    assert_eq!(
        foreign.reasons(),
        &[
            ProjectVersionReason::ForeignApplicationIdentity,
            ProjectVersionReason::ForeignFormatIdentity,
        ]
    );
}
