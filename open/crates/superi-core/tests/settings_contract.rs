use std::cmp::Ordering;
use std::str::FromStr;

use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::settings::{
    CapabilityId, CapabilitySet, ComponentId, FeatureAvailability, FeatureDescriptor,
    FeatureDiscovery, FeatureId, SemanticVersion, SettingKey, SettingValue, SettingValueKind,
    SettingsSnapshot, VersionIdentifier,
};

fn version(input: &str) -> SemanticVersion {
    SemanticVersion::from_str(input).expect("valid semantic version")
}

fn capability(input: &str) -> CapabilityId {
    CapabilityId::from_str(input).expect("valid capability")
}

#[test]
fn shared_names_are_typed_canonical_and_never_normalized() {
    let setting = SettingKey::from_str("superi.playback.loop").unwrap();
    let capability = CapabilityId::from_str("superi.codec.av1-decode").unwrap();
    let feature = FeatureId::from_str("superi.export.image_sequence").unwrap();
    let component = ComponentId::from_str("superi.engine").unwrap();

    assert_eq!(setting.as_str(), "superi.playback.loop");
    assert_eq!(capability.to_string(), "superi.codec.av1-decode");
    assert_eq!(feature.as_ref(), "superi.export.image_sequence");
    assert_eq!(component.to_string(), "superi.engine");

    for invalid in [
        "superi",
        "Superi.engine",
        " superi.engine",
        "superi..engine",
        "superi.2engine",
        "superi.engine/preview",
        "superi.engine.",
    ] {
        assert!(
            ComponentId::from_str(invalid).is_err(),
            "accepted {invalid}"
        );
    }
}

#[test]
fn semantic_versions_round_trip_every_identity_field() {
    let release = version("12.34.56-alpha.7+x86-64.release-42");
    assert_eq!(release.major(), 12);
    assert_eq!(release.minor(), 34);
    assert_eq!(release.patch(), 56);
    assert_eq!(release.pre_release(), Some("alpha.7"));
    assert_eq!(release.build_metadata(), Some("x86-64.release-42"));
    assert_eq!(release.to_string(), "12.34.56-alpha.7+x86-64.release-42");
    assert_eq!(
        SemanticVersion::from_str(&release.to_string()).unwrap(),
        release
    );

    let package = version(env!("CARGO_PKG_VERSION"));
    assert_eq!(package.major().to_string(), env!("CARGO_PKG_VERSION_MAJOR"));
    assert_eq!(package.minor().to_string(), env!("CARGO_PKG_VERSION_MINOR"));
    assert_eq!(package.patch().to_string(), env!("CARGO_PKG_VERSION_PATCH"));
}

#[test]
fn semantic_version_precedence_matches_the_official_sequence() {
    let sequence = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
    ];

    for pair in sequence.windows(2) {
        assert_eq!(
            version(pair[0]).precedence_cmp(&version(pair[1])),
            Ordering::Less
        );
    }

    let left = version("1.2.3+macos");
    let right = version("1.2.3+windows");
    assert_ne!(left, right);
    assert_eq!(left.precedence_cmp(&right), Ordering::Equal);
}

#[test]
fn malformed_semantic_versions_fail_instead_of_being_reinterpreted() {
    for invalid in [
        "1",
        "1.2",
        "1.2.3.4",
        "01.2.3",
        "1.02.3",
        "1.2.03",
        "1.2.3-",
        "1.2.3-alpha..1",
        "1.2.3-01",
        "1.2.3+",
        "1.2.3+meta..value",
        "1.2.3-alpha_beta",
        "1.2.3+metadata!",
    ] {
        assert!(
            SemanticVersion::from_str(invalid).is_err(),
            "accepted {invalid}"
        );
    }
}

#[test]
fn component_versions_have_one_canonical_cross_process_identifier() {
    let identifier = VersionIdentifier::new(
        ComponentId::from_str("superi.engine").unwrap(),
        version("2.5.0-rc.1+build.9"),
    );
    assert_eq!(identifier.component().as_str(), "superi.engine");
    assert_eq!(identifier.version(), &version("2.5.0-rc.1+build.9"));
    assert_eq!(identifier.to_string(), "superi.engine@2.5.0-rc.1+build.9");
    assert_eq!(
        VersionIdentifier::from_str(&identifier.to_string()).unwrap(),
        identifier
    );
    assert!(VersionIdentifier::from_str("superi.engine").is_err());
}

#[test]
fn settings_snapshots_preserve_type_schema_unknown_keys_and_order() {
    let zeta = SettingKey::from_str("vendor.extension.zeta").unwrap();
    let alpha = SettingKey::from_str("superi.playback.loop").unwrap();
    let middle = SettingKey::from_str("superi.cache.frames").unwrap();
    let snapshot = SettingsSnapshot::new(
        version("3.1.0"),
        [
            (zeta.clone(), SettingValue::Text("opaque value".to_owned())),
            (alpha.clone(), SettingValue::Boolean(true)),
            (middle.clone(), SettingValue::Integer(240)),
        ],
    )
    .unwrap();

    assert_eq!(snapshot.schema_version(), &version("3.1.0"));
    assert_eq!(snapshot.len(), 3);
    assert!(!snapshot.is_empty());
    assert_eq!(
        snapshot.get(&alpha).and_then(SettingValue::as_bool),
        Some(true)
    );
    assert_eq!(
        snapshot.get(&middle).and_then(SettingValue::as_integer),
        Some(240)
    );
    assert_eq!(
        snapshot.get(&zeta).and_then(SettingValue::as_text),
        Some("opaque value")
    );
    assert_eq!(
        snapshot
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        [
            "superi.cache.frames",
            "superi.playback.loop",
            "vendor.extension.zeta",
        ]
    );

    assert_eq!(
        SettingValue::Boolean(false).kind(),
        SettingValueKind::Boolean
    );
    assert_eq!(SettingValue::Integer(-2).kind(), SettingValueKind::Integer);
    assert_eq!(
        SettingValue::Text(String::new()).kind(),
        SettingValueKind::Text
    );
    assert_eq!(SettingValueKind::Boolean.code(), "boolean");
    assert_eq!(
        SettingValueKind::from_code("integer"),
        Some(SettingValueKind::Integer)
    );
}

#[test]
fn settings_reject_duplicate_keys_with_shared_actionable_context() {
    let key = SettingKey::from_str("superi.playback.loop").unwrap();
    let error = SettingsSnapshot::new(
        version("1.0.0"),
        [
            (key.clone(), SettingValue::Boolean(true)),
            (key, SettingValue::Boolean(false)),
        ],
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].component(), "superi-core.settings");
    assert_eq!(error.contexts()[0].operation(), "create_settings_snapshot");
}

#[test]
fn capability_sets_are_symbolic_deterministic_and_support_subset_checks() {
    let gpu = capability("superi.render.gpu");
    let av1 = capability("superi.codec.av1-decode");
    let network = capability("superi.permission.network");
    let available = CapabilitySet::new([gpu.clone(), av1.clone(), gpu.clone()]);
    let required = CapabilitySet::new([av1.clone(), gpu.clone()]);

    assert_eq!(available.len(), 2);
    assert!(available.contains(&gpu));
    assert!(available.contains_all(&required));
    assert!(!available.contains(&network));
    assert_eq!(
        available
            .iter()
            .map(CapabilityId::as_str)
            .collect::<Vec<_>>(),
        ["superi.codec.av1-decode", "superi.render.gpu"]
    );
}

#[test]
fn feature_discovery_is_versioned_queryable_and_deterministic() {
    let gpu = capability("superi.render.gpu");
    let av1 = capability("superi.codec.av1-decode");
    let capabilities = CapabilitySet::new([gpu.clone(), av1.clone()]);
    let render = FeatureDescriptor::new(
        FeatureId::from_str("superi.render.timeline").unwrap(),
        version("1.4.0"),
        FeatureAvailability::Available,
        CapabilitySet::new([gpu]),
    );
    let decode = FeatureDescriptor::new(
        FeatureId::from_str("superi.decode.av1").unwrap(),
        version("1.0.0"),
        FeatureAvailability::Available,
        CapabilitySet::new([av1]),
    );
    let producer = VersionIdentifier::from_str("superi.engine@0.8.0+abc123").unwrap();
    let discovery = FeatureDiscovery::new(
        version("1.0.0"),
        producer.clone(),
        capabilities,
        [render, decode],
    )
    .unwrap();

    assert_eq!(discovery.schema_version(), &version("1.0.0"));
    assert_eq!(discovery.producer(), &producer);
    assert_eq!(discovery.len(), 2);
    assert_eq!(
        discovery
            .iter()
            .map(|feature| feature.id().as_str())
            .collect::<Vec<_>>(),
        ["superi.decode.av1", "superi.render.timeline"]
    );
    let render_id = FeatureId::from_str("superi.render.timeline").unwrap();
    let found = discovery.feature(&render_id).unwrap();
    assert_eq!(found.version(), &version("1.4.0"));
    assert_eq!(found.availability(), FeatureAvailability::Available);
    assert!(discovery.is_available(&render_id));
    assert_eq!(FeatureAvailability::Disabled.code(), "disabled");
    assert_eq!(
        FeatureAvailability::from_code("unsupported"),
        Some(FeatureAvailability::Unsupported)
    );
}

#[test]
fn discovery_rejects_false_available_claims_and_duplicate_features() {
    let missing = capability("superi.permission.network");
    let remote = FeatureDescriptor::new(
        FeatureId::from_str("superi.extension.remote-assets").unwrap(),
        version("1.0.0"),
        FeatureAvailability::Available,
        CapabilitySet::new([missing.clone()]),
    );
    let producer = VersionIdentifier::from_str("superi.engine@1.0.0").unwrap();
    let error = FeatureDiscovery::new(
        version("1.0.0"),
        producer.clone(),
        CapabilitySet::default(),
        [remote],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.contexts()[0].operation(), "create_feature_discovery");

    let unavailable = FeatureDescriptor::new(
        FeatureId::from_str("superi.extension.remote-assets").unwrap(),
        version("1.0.0"),
        FeatureAvailability::Unsupported,
        CapabilitySet::new([missing]),
    );
    assert!(FeatureDiscovery::new(
        version("1.0.0"),
        producer.clone(),
        CapabilitySet::default(),
        [unavailable.clone()],
    )
    .is_ok());

    let duplicate = FeatureDiscovery::new(
        version("1.0.0"),
        producer,
        CapabilitySet::default(),
        [unavailable.clone(), unavailable],
    )
    .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::InvalidInput);
}

#[test]
fn shared_settings_and_discovery_values_are_thread_safe() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<SemanticVersion>();
    assert_send_sync::<VersionIdentifier>();
    assert_send_sync::<SettingsSnapshot>();
    assert_send_sync::<CapabilitySet>();
    assert_send_sync::<FeatureDiscovery>();
}

#[test]
fn every_closed_shared_tag_has_one_stable_round_tripping_code() {
    assert_eq!(
        SettingValueKind::ALL
            .iter()
            .map(|value| value.code())
            .collect::<Vec<_>>(),
        ["boolean", "integer", "text"]
    );
    for value in SettingValueKind::ALL {
        assert_eq!(SettingValueKind::from_code(value.code()), Some(*value));
        assert_eq!(value.to_string(), value.code());
    }

    assert_eq!(
        FeatureAvailability::ALL
            .iter()
            .map(|value| value.code())
            .collect::<Vec<_>>(),
        ["available", "disabled", "unsupported", "unavailable"]
    );
    for value in FeatureAvailability::ALL {
        assert_eq!(FeatureAvailability::from_code(value.code()), Some(*value));
        assert_eq!(value.to_string(), value.code());
    }
}
