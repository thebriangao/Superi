use std::fs;

use superi_color::config::{ColorManagementConfig, ProjectColorSettings};
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{ErrorCategory, Recoverability};

const CONFIG: &str = r#"{
  "schema": "superi.color-config",
  "version": 1,
  "id": "studio.feature",
  "default_working_space": "acescg",
  "roles": { "scene_linear": "lin-ap1" },
  "working_spaces": [
    {
      "id": "acescg",
      "aliases": ["lin-ap1"],
      "primaries": "aces_ap1",
      "transfer": "linear",
      "matrix": "rgb",
      "range": "full"
    },
    {
      "id": "bt2020-linear",
      "primaries": "bt2020",
      "transfer": "linear",
      "matrix": "rgb",
      "range": "full"
    }
  ]
}"#;

#[test]
fn versioned_config_resolves_named_working_spaces_and_default() {
    let config = ColorManagementConfig::from_json(CONFIG.as_bytes()).unwrap();

    assert_eq!(config.id(), "studio.feature");
    assert_eq!(config.default_working_space_id(), "acescg");
    assert_eq!(
        config.default_working_space().color_space(),
        ColorSpace::ACESCG
    );
    assert_eq!(
        config.working_space("bt2020-linear").unwrap().color_space(),
        ColorSpace::new(
            ColorPrimaries::Bt2020,
            TransferFunction::Linear,
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        )
    );
    assert_eq!(
        config.working_space_ids().collect::<Vec<_>>(),
        vec!["acescg", "bt2020-linear"]
    );
    assert_eq!(
        config.working_space("lin-ap1"),
        Some(config.default_working_space())
    );
    assert_eq!(
        config.role("scene_linear"),
        Some(config.default_working_space())
    );
}

#[test]
fn semantic_fingerprint_is_stable_across_json_formatting() {
    let pretty = ColorManagementConfig::from_json(CONFIG.as_bytes()).unwrap();
    let compact = ColorManagementConfig::from_json(
        br#"{"working_spaces":[{"range":"full","matrix":"rgb","transfer":"linear","primaries":"aces_ap1","aliases":["lin-ap1"],"id":"acescg"},{"id":"bt2020-linear","primaries":"bt2020","transfer":"linear","matrix":"rgb","range":"full"}],"roles":{"scene_linear":"lin-ap1"},"id":"studio.feature","version":1,"default_working_space":"acescg","schema":"superi.color-config"}"#,
    )
    .unwrap();

    assert_eq!(pretty.fingerprint(), compact.fingerprint());
    assert_eq!(pretty.fingerprint().len(), 64);

    let changed = ColorManagementConfig::from_json(
        CONFIG
            .replace(
                "\"scene_linear\": \"lin-ap1\"",
                "\"scene_linear\": \"bt2020-linear\"",
            )
            .as_bytes(),
    )
    .unwrap();
    assert_ne!(pretty.fingerprint(), changed.fingerprint());
}

#[test]
fn project_selection_is_serializable_and_rejects_config_drift() {
    let config = ColorManagementConfig::from_json(CONFIG.as_bytes()).unwrap();
    let settings = ProjectColorSettings::new(&config, "bt2020-linear").unwrap();
    let encoded = serde_json::to_string(&settings).unwrap();
    let decoded: ProjectColorSettings = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, settings);
    assert_eq!(decoded.version(), 1);
    assert_eq!(decoded.config_id(), config.id());
    assert_eq!(decoded.config_fingerprint(), config.fingerprint());
    assert_eq!(decoded.working_space_id(), "bt2020-linear");
    assert_eq!(
        decoded.resolve(&config).unwrap().color_space().primaries(),
        ColorPrimaries::Bt2020
    );

    let changed = ColorManagementConfig::from_json(
        CONFIG
            .replace(
                "bt2020-linear\",\n      \"primaries\": \"bt2020",
                "bt2020-linear\",\n      \"primaries\": \"display_p3",
            )
            .as_bytes(),
    )
    .unwrap();
    let error = decoded.resolve(&changed).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
}

#[test]
fn config_can_be_loaded_from_a_real_file() {
    let path = std::env::temp_dir().join(format!(
        "superi-color-config-{}-{}.json",
        std::process::id(),
        std::thread::current().name().unwrap_or("contract")
    ));
    fs::write(&path, CONFIG).unwrap();

    let config = ColorManagementConfig::load(&path).unwrap();
    fs::remove_file(&path).unwrap();

    assert_eq!(config.default_working_space_id(), "acescg");
}

#[test]
fn file_loading_rejects_oversized_artifacts_before_json_parsing() {
    let path = std::env::temp_dir().join(format!(
        "superi-color-config-oversized-{}.json",
        std::process::id()
    ));
    fs::write(&path, vec![b' '; ColorManagementConfig::MAX_FILE_BYTES + 1]).unwrap();

    let error = ColorManagementConfig::load(&path).unwrap_err();
    fs::remove_file(&path).unwrap();

    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
}

#[test]
fn malformed_ambiguous_and_unsupported_configs_fail_closed() {
    let cases = [
        CONFIG.replace("\"version\": 1", "\"version\": 2"),
        CONFIG.replace(
            "\"default_working_space\": \"acescg\"",
            "\"default_working_space\": \"missing\"",
        ),
        CONFIG.replace("\"transfer\": \"linear\"", "\"transfer\": \"pq\""),
        CONFIG.replace("\"id\": \"bt2020-linear\"", "\"id\": \"acescg\""),
        CONFIG.replace("\"range\": \"full\"", "\"range\": \"limited\""),
        CONFIG.replace(
            "\"schema\": \"superi.color-config\"",
            "\"schema\": \"other\"",
        ),
        CONFIG.replace("\n}", ",\n  \"future\": true\n}"),
    ];

    for source in cases {
        let error = ColorManagementConfig::from_json(source.as_bytes()).unwrap_err();
        assert!(matches!(
            error.category(),
            ErrorCategory::InvalidInput | ErrorCategory::Unsupported
        ));
        assert!(error
            .contexts()
            .iter()
            .any(|context| context.component() == "superi-color.config"));
    }

    let oversized = vec![b' '; ColorManagementConfig::MAX_FILE_BYTES + 1];
    assert_eq!(
        ColorManagementConfig::from_json(&oversized)
            .unwrap_err()
            .category(),
        ErrorCategory::ResourceExhausted
    );
}

#[test]
fn config_and_project_values_are_safe_to_share() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<ColorManagementConfig>();
    assert_send_sync::<ProjectColorSettings>();
}
