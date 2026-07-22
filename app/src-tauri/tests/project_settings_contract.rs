use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use superi_desktop::project_lifecycle::{
    DesktopProjectCommand, DesktopProjectCreateRequest, DesktopProjectFailureClass,
    DesktopProjectLifecycle, DesktopProjectSettingsUpdate, LocalProjectBackend,
};

fn owned_test_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "superi-project-settings-{}-{nonce}",
        std::process::id()
    ))
}

fn full_update(expected_project_revision: u64) -> DesktopProjectSettingsUpdate {
    DesktopProjectSettingsUpdate {
        expected_project_revision,
        frame_rate_numerator: 24_000,
        frame_rate_denominator: 1_001,
        timecode_mode: "non_drop_frame".into(),
        resolution_width: Some(3_840),
        resolution_height: Some(2_160),
        color_mode: "built_in_acescg".into(),
        color_working_space: "acescg".into(),
        color_config_id: None,
        color_config_fingerprint: None,
        audio_sample_rate_hz: 48_000,
        audio_output_layout: "surround_5_1".into(),
        cache_mode: "bounded".into(),
        cache_max_bytes: Some(8 * 1_024 * 1_024),
        cache_max_frames: Some(96),
        proxy_mode: "prefer".into(),
        proxy_quality: "half".into(),
        working_folder: Some("work".into()),
        cache_folder: Some("work/cache".into()),
        proxy_folder: Some("work/proxy".into()),
    }
}

#[test]
fn project_settings_attach_to_lifecycle_and_round_trip_durably() {
    let root = owned_test_root();
    let recovery_root = root.join("recovery");
    let project_path = root.join("settings.superi");
    std::fs::create_dir_all(&root).unwrap();

    let mut lifecycle =
        DesktopProjectLifecycle::new(LocalProjectBackend::new(recovery_root), 4).unwrap();
    lifecycle
        .execute(DesktopProjectCommand::Create {
            path: project_path.to_string_lossy().into_owned(),
            project: DesktopProjectCreateRequest {
                project_id: "project:00000000000000000000000000000302".into(),
                project_name: "Settings Contract".into(),
                root_timeline_id: "timeline:00000000000000000000000000010302".into(),
                root_timeline_name: "Settings Timeline".into(),
                edit_rate_numerator: 24,
                edit_rate_denominator: 1,
            },
        })
        .unwrap();
    assert!(!lifecycle.snapshot().dirty());

    let initial = lifecycle.inspect_settings().unwrap();
    assert_eq!(initial.project_revision(), 0);
    assert_eq!(initial.frame_rate(), (24, 1));
    assert_eq!(initial.resolution(), None);
    assert_eq!(initial.audio_sample_rate_hz(), 48_000);
    assert_eq!(initial.audio_output_layout(), "stereo");
    assert_eq!(initial.working_folder(), None);

    let updated = lifecycle.update_settings(full_update(0)).unwrap();
    assert_eq!(updated.project_revision(), 1);
    assert_eq!(updated.frame_rate(), (24_000, 1_001));
    assert_eq!(updated.resolution(), Some((3_840, 2_160)));
    assert_eq!(updated.color_mode(), "built_in_acescg");
    assert_eq!(updated.color_working_space(), "acescg");
    assert_eq!(updated.audio_sample_rate_hz(), 48_000);
    assert_eq!(updated.audio_output_layout(), "surround_5_1");
    assert_eq!(updated.cache_mode(), "bounded");
    assert_eq!(updated.cache_budget(), Some((8 * 1_024 * 1_024, 96)));
    assert_eq!(updated.proxy_mode(), "prefer");
    assert_eq!(updated.proxy_quality(), "half");
    assert_eq!(updated.working_folder(), Some("work"));
    assert_eq!(updated.cache_folder(), Some("work/cache"));
    assert_eq!(updated.proxy_folder(), Some("work/proxy"));
    assert_eq!(
        lifecycle.snapshot().active().unwrap().project_revision(),
        updated.project_revision()
    );
    assert!(lifecycle.snapshot().dirty());

    lifecycle.execute(DesktopProjectCommand::Close).unwrap();
    assert!(!lifecycle.snapshot().dirty());
    lifecycle
        .execute(DesktopProjectCommand::Open {
            path: project_path.to_string_lossy().into_owned(),
        })
        .unwrap();
    assert_eq!(lifecycle.inspect_settings().unwrap(), updated);
    assert!(!lifecycle.snapshot().dirty());

    let stable_active = lifecycle.snapshot().active().unwrap().clone();
    let stable_settings = lifecycle.inspect_settings().unwrap();
    let stale = lifecycle
        .update_settings(full_update(0))
        .expect_err("the old project revision must not overwrite durable settings");
    assert_eq!(stale.class(), DesktopProjectFailureClass::UserCorrectable);
    assert_eq!(lifecycle.snapshot().active(), Some(&stable_active));
    assert_eq!(lifecycle.inspect_settings().unwrap(), stable_settings);

    let _ = std::fs::remove_dir_all(root);
}
