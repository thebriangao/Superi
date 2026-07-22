use superi_ai::capabilities::{discover_local_capabilities, AiRuntimeAvailability};

#[test]
fn local_capability_discovery_is_honest_about_the_absent_runtime() {
    let snapshot = discover_local_capabilities();

    assert_eq!(snapshot.schema_version(), 1);
    assert!(snapshot.local_only());
    assert!(snapshot.requires_editable_artifacts());
    assert_eq!(snapshot.runtime(), AiRuntimeAvailability::Unavailable);
    assert!(snapshot.available_pipelines().is_empty());
}
