use superi_core::error::{ErrorCategory, ErrorContext, Recoverability};
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::settings::{
    CapabilityId, CapabilitySet, ComponentId, SemanticVersion, VersionIdentifier,
};
use superi_core::time::{RationalTime, Timebase};
use superi_project::document::ProjectDocument;
use superi_project::extensions::{
    ProjectExtensionCommand, ProjectExtensionCommandResult, ProjectExtensionFailure,
    ProjectExtensionKey, ProjectExtensionKind, ProjectExtensionLifecycle, ProjectExtensionRecord,
    ProjectExtensionRecordId, MAX_PROJECT_EXTENSION_PAYLOAD_BYTES, MAX_PROJECT_EXTENSION_RECORDS,
};
use superi_timeline::model::{EditorialProject, Timeline};

const PROJECT: ProjectId = ProjectId::from_raw(0xc011_0000);
const ROOT: TimelineId = TimelineId::from_raw(0xc011_0001);

fn component(value: &str) -> ComponentId {
    ComponentId::new(value).unwrap()
}

fn capability(value: &str) -> CapabilityId {
    CapabilityId::new(value).unwrap()
}

fn document() -> ProjectDocument {
    let timebase = Timebase::integer(24).unwrap();
    let timeline = Timeline::new(
        ROOT,
        "extension state",
        timebase,
        RationalTime::zero(timebase),
        vec![],
    );
    let project = EditorialProject::new(PROJECT, "extension state", [], [timeline]).unwrap();
    ProjectDocument::new(project, ROOT).unwrap()
}

fn record(
    extension: &str,
    record_id: &str,
    kind: ProjectExtensionKind,
    payload: &[u8],
) -> ProjectExtensionRecord {
    let requested = CapabilitySet::new([
        capability("superi.capability.project-read"),
        capability("superi.capability.project-mutate"),
    ]);
    let granted = CapabilitySet::new([capability("superi.capability.project-read")]);
    ProjectExtensionRecord::new(
        component(extension),
        ProjectExtensionRecordId::new(record_id).unwrap(),
        SemanticVersion::new(2, 1, 0),
        kind,
        VersionIdentifier::new(
            component("example.extension-state"),
            SemanticVersion::new(3, 4, 5),
        ),
        requested,
        granted,
        ProjectExtensionLifecycle::Enabled,
        None,
        payload.to_vec(),
    )
    .unwrap()
}

#[test]
fn plugin_effect_ai_and_unknown_records_share_one_revisioned_command_surface() {
    let mut document = document();
    let records = [
        record(
            "example.plugin",
            "plugin-state",
            ProjectExtensionKind::plugin(),
            b"\0plugin\xffstate",
        ),
        record(
            "example.effect",
            "effect-extension-state",
            ProjectExtensionKind::effect(),
            b"effect-host-checkpoint",
        ),
        record(
            "example.ai-tool",
            "artifact-provenance",
            ProjectExtensionKind::ai_artifact(),
            b"provenance-for-ordinary-artifact",
        ),
        record(
            "example.future-extension",
            "opaque-state",
            ProjectExtensionKind::new(component("example.future-kind")),
            &[0, 1, 2, 0xfe, 0xff],
        ),
    ];

    for (expected_revision, record) in records.into_iter().enumerate() {
        let expected_revision = u64::try_from(expected_revision).unwrap();
        let key = record.key().clone();
        let outcome = document
            .execute_extension_command(
                expected_revision,
                ProjectExtensionCommand::upsert(record.clone()),
            )
            .unwrap();
        assert_eq!(outcome.snapshot().revision(), expected_revision + 1);
        assert_eq!(
            outcome.result(),
            &ProjectExtensionCommandResult::Upserted {
                key: key.clone(),
                replaced: false,
            }
        );
        assert_eq!(outcome.snapshot().extension_record(&key), Some(&record));
    }

    let unknown = ProjectExtensionKey::new(
        component("example.future-extension"),
        ProjectExtensionRecordId::new("opaque-state").unwrap(),
    );
    let snapshot = document.snapshot();
    let preserved = snapshot.extension_record(&unknown).unwrap();
    assert_eq!(preserved.kind().as_str(), "example.future-kind");
    assert_eq!(preserved.payload(), &[0, 1, 2, 0xfe, 0xff]);
    assert_eq!(snapshot.extension_records().len(), 4);

    let unchanged = document
        .execute_extension_command(
            snapshot.revision(),
            ProjectExtensionCommand::upsert(preserved.clone()),
        )
        .unwrap();
    assert_eq!(unchanged.snapshot().revision(), snapshot.revision());
    assert_eq!(
        unchanged.result(),
        &ProjectExtensionCommandResult::Upserted {
            key: unknown,
            replaced: true,
        }
    );
}

#[test]
fn capability_and_failure_lifecycle_controls_are_validated_and_revisioned() {
    let mut document = document();
    let plugin = record(
        "example.unstable-plugin",
        "project-state",
        ProjectExtensionKind::plugin(),
        b"checkpoint",
    );
    let key = plugin.key().clone();
    document
        .execute_extension_command(0, ProjectExtensionCommand::upsert(plugin))
        .unwrap();

    let unrequested = CapabilitySet::new([capability("superi.capability.network")]);
    let denied = document
        .execute_extension_command(
            1,
            ProjectExtensionCommand::set_granted_capabilities(key.clone(), unrequested),
        )
        .unwrap_err();
    assert_eq!(denied.category(), ErrorCategory::PermissionDenied);
    assert_eq!(document.revision(), 1);

    let failure =
        ProjectExtensionFailure::new(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "plugin worker exited",
            [ErrorContext::new("superi-engine.plugins", "worker_exit")
                .with_field("status", "signal")],
            3,
            2,
        )
        .unwrap();
    let quarantined = document
        .execute_extension_command(
            1,
            ProjectExtensionCommand::record_failure(key.clone(), failure.clone(), true),
        )
        .unwrap();
    assert_eq!(quarantined.snapshot().revision(), 2);
    let stored = quarantined.snapshot().extension_record(&key).unwrap();
    assert_eq!(stored.lifecycle(), ProjectExtensionLifecycle::Quarantined);
    assert_eq!(stored.failure(), Some(&failure));

    let clear_while_quarantined = document
        .execute_extension_command(2, ProjectExtensionCommand::clear_failure(key.clone()))
        .unwrap_err();
    assert_eq!(clear_while_quarantined.category(), ErrorCategory::Conflict);
    assert_eq!(document.revision(), 2);

    document
        .execute_extension_command(
            2,
            ProjectExtensionCommand::set_lifecycle(
                key.clone(),
                ProjectExtensionLifecycle::Disabled,
            ),
        )
        .unwrap();
    let cleared = document
        .execute_extension_command(3, ProjectExtensionCommand::clear_failure(key.clone()))
        .unwrap();
    assert_eq!(cleared.snapshot().revision(), 4);
    assert_eq!(
        cleared.snapshot().extension_record(&key).unwrap().failure(),
        None
    );

    let stale = document
        .execute_extension_command(3, ProjectExtensionCommand::remove(key))
        .unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(document.revision(), 4);
}

#[test]
fn invalid_record_bounds_and_quarantine_invariants_fail_before_publication() {
    let requested = CapabilitySet::new([capability("superi.capability.project-read")]);
    let granted = CapabilitySet::new([capability("superi.capability.network")]);
    let invalid_grant = ProjectExtensionRecord::new(
        component("example.plugin"),
        ProjectExtensionRecordId::new("state").unwrap(),
        SemanticVersion::new(1, 0, 0),
        ProjectExtensionKind::plugin(),
        VersionIdentifier::new(component("example.state"), SemanticVersion::new(1, 0, 0)),
        requested,
        granted,
        ProjectExtensionLifecycle::Enabled,
        None,
        vec![],
    )
    .unwrap_err();
    assert_eq!(invalid_grant.category(), ErrorCategory::PermissionDenied);

    let quarantined_without_failure = ProjectExtensionRecord::new(
        component("example.plugin"),
        ProjectExtensionRecordId::new("state").unwrap(),
        SemanticVersion::new(1, 0, 0),
        ProjectExtensionKind::plugin(),
        VersionIdentifier::new(component("example.state"), SemanticVersion::new(1, 0, 0)),
        CapabilitySet::default(),
        CapabilitySet::default(),
        ProjectExtensionLifecycle::Quarantined,
        None,
        vec![],
    )
    .unwrap_err();
    assert_eq!(
        quarantined_without_failure.category(),
        ErrorCategory::Conflict
    );

    assert!(ProjectExtensionRecordId::new("").is_err());
    assert!(ProjectExtensionRecordId::new("x".repeat(129)).is_err());

    let oversized_payload = ProjectExtensionRecord::new(
        component("example.plugin"),
        ProjectExtensionRecordId::new("oversized-payload").unwrap(),
        SemanticVersion::new(1, 0, 0),
        ProjectExtensionKind::plugin(),
        VersionIdentifier::new(component("example.state"), SemanticVersion::new(1, 0, 0)),
        CapabilitySet::default(),
        CapabilitySet::default(),
        ProjectExtensionLifecycle::Enabled,
        None,
        vec![0; MAX_PROJECT_EXTENSION_PAYLOAD_BYTES + 1],
    )
    .unwrap_err();
    assert_eq!(
        oversized_payload.category(),
        ErrorCategory::ResourceExhausted
    );

    let base = document().snapshot();
    let too_many_records = (0..=MAX_PROJECT_EXTENSION_RECORDS).map(|index| {
        record(
            "example.plugin",
            &format!("state-{index}"),
            ProjectExtensionKind::plugin(),
            b"",
        )
    });
    let too_many_records = ProjectDocument::from_complete_parts_with_settings_and_extensions(
        base.revision(),
        base.editorial_project().clone(),
        base.root_timeline_id(),
        base.settings().clone(),
        base.graphs().cloned(),
        base.clip_mix_state().clone(),
        too_many_records,
    )
    .unwrap_err();
    assert_eq!(
        too_many_records.category(),
        ErrorCategory::ResourceExhausted
    );
}
