use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::settings::{
    CapabilityId, CapabilitySet, ComponentId, FeatureAvailability, FeatureDescriptor, FeatureId,
    SemanticVersion, VersionIdentifier,
};
use superi_engine::extensions::{
    ExtensionFailureSummary, ExtensionLifecycle, ExtensionRegistration, ExtensionRegistry,
    ExtensionUserAction,
};

fn capability(value: &str) -> CapabilityId {
    CapabilityId::new(value).unwrap()
}

fn producer(version: u64) -> VersionIdentifier {
    VersionIdentifier::new(
        ComponentId::new("example.extension").unwrap(),
        SemanticVersion::new(version, 0, 0),
    )
}

fn registration(version: u64, lifecycle: ExtensionLifecycle) -> ExtensionRegistration {
    let requested = CapabilitySet::new([
        capability("superi.capability.project-read"),
        capability("superi.capability.project-mutate"),
    ]);
    let granted = CapabilitySet::new([capability("superi.capability.project-read")]);
    let availability = if lifecycle == ExtensionLifecycle::Ready {
        FeatureAvailability::Available
    } else {
        FeatureAvailability::Disabled
    };
    ExtensionRegistration::versioned(
        producer(version),
        format!("Example Extension {version}"),
        requested,
        granted.clone(),
        lifecycle,
        [FeatureDescriptor::new(
            FeatureId::new("example.extension.render").unwrap(),
            SemanticVersion::new(1, 0, 0),
            availability,
            granted,
        )],
        None,
    )
    .unwrap()
}

#[test]
fn exact_identity_order_duplicates_and_change_only_revisions_are_deterministic() {
    let mut registry = ExtensionRegistry::new();
    assert_eq!(registry.snapshot().revision(), 0);

    registry
        .register(registration(2, ExtensionLifecycle::Ready))
        .unwrap();
    registry
        .register(registration(1, ExtensionLifecycle::Disabled))
        .unwrap();
    assert_eq!(registry.snapshot().revision(), 2);
    assert_eq!(
        registry
            .snapshot()
            .registrations()
            .iter()
            .map(|value| value.identity().producer().version().major())
            .collect::<Vec<_>>(),
        [1, 2]
    );

    let duplicate = registry
        .register(registration(1, ExtensionLifecycle::Ready))
        .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::Conflict);
    assert_eq!(registry.snapshot().revision(), 2);

    let current = registry.snapshot().registrations().to_vec();
    assert!(!registry.synchronize(current.clone()).unwrap());
    assert_eq!(registry.snapshot().revision(), 2);
    assert!(registry.synchronize(current.into_iter().rev()).is_ok());
    assert_eq!(registry.snapshot().revision(), 2);
}

#[test]
fn grants_features_lifecycle_and_safe_failure_projection_fail_closed() {
    let requested = CapabilitySet::new([capability("superi.capability.project-read")]);
    let unrequested = CapabilitySet::new([capability("superi.capability.network")]);
    let invalid_grant = ExtensionRegistration::versioned(
        producer(1),
        "Invalid Grant",
        requested,
        unrequested.clone(),
        ExtensionLifecycle::Disabled,
        [],
        None,
    )
    .unwrap_err();
    assert_eq!(invalid_grant.category(), ErrorCategory::PermissionDenied);

    let nonready_available = ExtensionRegistration::versioned(
        producer(1),
        "Invalid Availability",
        unrequested.clone(),
        unrequested.clone(),
        ExtensionLifecycle::Disabled,
        [FeatureDescriptor::new(
            FeatureId::new("example.extension.network").unwrap(),
            SemanticVersion::new(1, 0, 0),
            FeatureAvailability::Available,
            unrequested,
        )],
        None,
    )
    .unwrap_err();
    assert_eq!(nonready_available.category(), ErrorCategory::Conflict);

    let failure = ExtensionFailureSummary::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "worker_restart",
        3,
        2,
    )
    .unwrap();
    assert_eq!(failure.recommended_action(), ExtensionUserAction::Retry);
    assert!(!format!("{failure:?}").contains("/Users/"));

    let faulted_without_failure = ExtensionRegistration::versioned(
        producer(1),
        "Faulted",
        CapabilitySet::default(),
        CapabilitySet::default(),
        ExtensionLifecycle::Faulted,
        [],
        None,
    )
    .unwrap_err();
    assert_eq!(faulted_without_failure.category(), ErrorCategory::Conflict);
}
