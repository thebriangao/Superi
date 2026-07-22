use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use superi_desktop::project_lifecycle::{
    DesktopImportedMediaKind, DesktopMediaImportOrigin, DesktopMediaImportRequest,
    DesktopProjectCommand, DesktopProjectCreateRequest, DesktopProjectLifecycle,
    LocalProjectBackend,
};

fn owned_test_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "superi-media-import-{}-{nonce}",
        std::process::id()
    ))
}

fn request(
    expected_project_revision: u64,
    origin: DesktopMediaImportOrigin,
    paths: Vec<PathBuf>,
) -> DesktopMediaImportRequest {
    DesktopMediaImportRequest {
        expected_project_revision,
        origin,
        paths: paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        recursive: true,
        detect_image_sequences: true,
    }
}

#[test]
fn picker_drop_folder_sequence_api_events_and_automation_share_one_import_transaction() {
    let root = owned_test_root();
    let recovery_root = root.join("recovery");
    let project_path = root.join("import.superi");
    let plates = root.join("plates");
    std::fs::create_dir_all(&plates).unwrap();
    for path in [
        root.join("clip.mov"),
        root.join("audio.wav"),
        root.join("logo.png"),
        root.join("overlay.exr"),
        plates.join("shot_0001.png"),
        plates.join("shot_0002.png"),
        plates.join("shot_0003.png"),
    ] {
        std::fs::write(path, b"superi-import-contract").unwrap();
    }

    let mut lifecycle =
        DesktopProjectLifecycle::new(LocalProjectBackend::new(recovery_root), 4).unwrap();
    lifecycle
        .execute(DesktopProjectCommand::Create {
            path: project_path.to_string_lossy().into_owned(),
            project: DesktopProjectCreateRequest {
                project_id: "project:00000000000000000000000000000303".into(),
                project_name: "Import Contract".into(),
                root_timeline_id: "timeline:00000000000000000000000000010303".into(),
                root_timeline_name: "Import Timeline".into(),
                edit_rate_numerator: 24,
                edit_rate_denominator: 1,
            },
        })
        .unwrap();
    assert!(!lifecycle.snapshot().dirty());

    let picker = lifecycle
        .import_media(request(
            0,
            DesktopMediaImportOrigin::Picker,
            vec![root.join("clip.mov")],
        ))
        .unwrap();
    assert_eq!(picker.project_revision(), 1);
    assert_eq!(picker.imported().len(), 1);
    assert_eq!(picker.imported()[0].kind(), DesktopImportedMediaKind::File);
    assert!(lifecycle.snapshot().dirty());

    let dropped = lifecycle
        .import_media(request(
            1,
            DesktopMediaImportOrigin::DragDrop,
            vec![root.join("audio.wav")],
        ))
        .unwrap();
    assert_eq!(dropped.project_revision(), 2);

    let scanned = lifecycle
        .import_media(request(
            2,
            DesktopMediaImportOrigin::FolderScan,
            vec![plates.clone()],
        ))
        .unwrap();
    assert_eq!(scanned.project_revision(), 3);
    assert_eq!(scanned.imported().len(), 1);
    assert_eq!(
        scanned.imported()[0].kind(),
        DesktopImportedMediaKind::ImageSequence
    );
    assert_eq!(scanned.imported()[0].source_count(), 3);
    assert_eq!(scanned.imported()[0].frame_range(), Some((1, 3)));
    assert_eq!(scanned.imported()[0].frame_rate(), Some((24, 1)));

    let api = lifecycle
        .import_media(request(
            3,
            DesktopMediaImportOrigin::Api,
            vec![root.join("logo.png")],
        ))
        .unwrap();
    assert_eq!(api.project_revision(), 4);

    let automation = lifecycle
        .import_media(request(
            4,
            DesktopMediaImportOrigin::Automation,
            vec![root.join("overlay.exr")],
        ))
        .unwrap();
    assert_eq!(automation.project_revision(), 5);
    for result in [&picker, &dropped, &scanned, &api, &automation] {
        assert_eq!(result.command_method(), "superi.project.command.execute");
        assert_eq!(result.event_name(), "superi.project.state.changed");
        assert!(result.event_sequence().is_some());
    }
    assert_eq!(
        automation.automation_method(),
        Some("superi.project.command.execute")
    );

    let duplicate = lifecycle
        .import_media(request(
            5,
            DesktopMediaImportOrigin::Picker,
            vec![root.join("clip.mov")],
        ))
        .unwrap();
    assert_eq!(duplicate.project_revision(), 5);
    assert!(duplicate.imported().is_empty());
    assert_eq!(duplicate.skipped().len(), 1);
    assert_eq!(duplicate.event_sequence(), None);

    lifecycle.execute(DesktopProjectCommand::Close).unwrap();
    assert!(!lifecycle.snapshot().dirty());
    lifecycle
        .execute(DesktopProjectCommand::Open {
            path: project_path.to_string_lossy().into_owned(),
        })
        .unwrap();
    assert_eq!(lifecycle.snapshot().active().unwrap().project_revision(), 5);
    assert!(!lifecycle.snapshot().dirty());
    let reopened_duplicate = lifecycle
        .import_media(request(
            5,
            DesktopMediaImportOrigin::Api,
            vec![root.join("logo.png")],
        ))
        .unwrap();
    assert_eq!(reopened_duplicate.project_revision(), 5);
    assert!(reopened_duplicate.imported().is_empty());

    let _ = std::fs::remove_dir_all(root);
}
