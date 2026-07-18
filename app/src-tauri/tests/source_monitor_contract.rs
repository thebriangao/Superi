use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use superi_desktop::project_lifecycle::{
    DesktopMediaImportOrigin, DesktopMediaImportRequest, DesktopProjectCommand,
    DesktopProjectCreateRequest, DesktopProjectState, MediaLibrarySnapshot, MediaSourceScanRequest,
    SourceMonitorEngineState, SourceMonitorLoadRequest, SourceMonitorMarkMutation,
    SourceMonitorMarkUpdate, SourceMonitorSeekRequest, SourceMonitorTime,
    SourceMonitorUnloadRequest,
};

fn owned_test_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "superi-source-monitor-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn create_project(state: &DesktopProjectState, root: &Path) -> PathBuf {
    let recovery = root.join("recovery");
    state.initialize(recovery).unwrap();
    let project = root.join("source-monitor.superi");
    state
        .execute(DesktopProjectCommand::Create {
            path: project.to_string_lossy().into_owned(),
            project: DesktopProjectCreateRequest {
                project_id: "project:00000000000000000000000000000314".into(),
                project_name: "Source Monitor Contract".into(),
                root_timeline_id: "timeline:00000000000000000000000000010314".into(),
                root_timeline_name: "Source Monitor Timeline".into(),
                edit_rate_numerator: 24,
                edit_rate_denominator: 1,
            },
        })
        .unwrap();
    project
}

fn import_request(project_revision: u64, paths: Vec<PathBuf>) -> DesktopMediaImportRequest {
    DesktopMediaImportRequest {
        expected_project_revision: project_revision,
        origin: DesktopMediaImportOrigin::Api,
        paths: paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        recursive: false,
        detect_image_sequences: true,
    }
}

fn library_fences(library: &MediaLibrarySnapshot) -> (u64, u64) {
    let value = serde_json::to_value(library).unwrap();
    (
        value["project_revision"].as_u64().unwrap(),
        value["revision"].as_u64().unwrap(),
    )
}

fn write_mono_wave(path: &Path, frames: u32) {
    let data_len = frames * 2;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&48_000_u32.to_le_bytes());
    bytes.extend_from_slice(&(48_000_u32 * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for frame in 0..frames {
        let sample = i16::try_from(frame % 512).unwrap() - 256;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}

fn write_png(path: &Path, red: u8) {
    let image =
        image::RgbaImage::from_raw(2, 2, [red, 32, 255_u8.saturating_sub(red), 255].repeat(4))
            .unwrap();
    image
        .save_with_format(path, image::ImageFormat::Png)
        .unwrap();
}

#[test]
fn source_monitor_uses_one_retained_engine_session_and_state_free_workspace_projection() {
    let tauri_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repository_root = tauri_root.parent().unwrap().parent().unwrap();
    let app_root = tauri_root.parent().unwrap();

    let engine_media =
        std::fs::read_to_string(repository_root.join("open/crates/superi-engine/src/media.rs"))
            .unwrap();
    let lifecycle = std::fs::read_to_string(tauri_root.join("src/project_lifecycle.rs")).unwrap();
    let source_monitor =
        std::fs::read_to_string(tauri_root.join("src/project_lifecycle/source_monitor.rs"))
            .unwrap();
    let host = std::fs::read_to_string(tauri_root.join("src/lib.rs")).unwrap();
    let bridge = std::fs::read_to_string(app_root.join("src/project-lifecycle.ts")).unwrap();
    let viewport = std::fs::read_to_string(app_root.join("src/native-viewport.tsx")).unwrap();
    let workspaces = std::fs::read_to_string(app_root.join("src/editor-workspaces.tsx")).unwrap();

    assert!(engine_media.contains("pub fn source_backend_registry()"));
    assert!(lifecycle.contains("mod source_monitor;"));
    assert!(lifecycle.contains("source_monitor_marks"));
    assert!(source_monitor.contains("pub struct SourceMonitorSnapshot"));
    assert!(source_monitor.contains("source_backend_registry()"));
    assert!(source_monitor.contains("SeekMode::Exact"));
    assert!(source_monitor.contains("marks_fresh"));
    for command in [
        "desktop_source_monitor_snapshot",
        "desktop_source_monitor_load",
        "desktop_source_monitor_seek",
        "desktop_source_monitor_update_marks",
        "desktop_source_monitor_unload",
    ] {
        assert!(host.contains(command), "missing Tauri command {command}");
        assert!(bridge.contains(command), "missing bridge command {command}");
    }
    assert!(viewport.contains("export function SourceMonitor"));
    assert!(viewport.contains("Source session"));
    assert!(viewport.contains("native GPU viewer"));
    assert!(workspaces.contains("<SourceMonitor />"));
    assert!(!workspaces.contains("useState"));
    assert!(!workspaces.contains("useSuperiApi"));
}

#[test]
fn wave_source_load_seek_marks_persist_and_unload_with_exact_revision_fences() {
    let root = owned_test_root("wave");
    std::fs::create_dir_all(&root).unwrap();
    let wave = root.join("dialog.wav");
    write_mono_wave(&wave, 480);

    let state = DesktopProjectState::default();
    let project = create_project(&state, &root);
    let imported = state
        .import_media(import_request(0, vec![wave.clone()]))
        .expect("wave source should import");
    let media = &imported.imported()[0];
    let mut library = state.media_library().unwrap();
    let (project_revision, library_revision) = library_fences(&library);
    let loaded = state
        .source_monitor_load(SourceMonitorLoadRequest {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            media_id: media.media_id().to_owned(),
            expected_source_fingerprint: media.content_fingerprint().to_owned(),
        })
        .expect("wave source should open through the engine source registry");
    assert_eq!(loaded.engine_state(), SourceMonitorEngineState::Ready);
    assert_eq!(
        loaded.current(),
        Some(SourceMonitorTime {
            value: 0,
            timebase_numerator: 48_000,
            timebase_denominator: 1,
        })
    );

    let seeked = state
        .source_monitor_seek(SourceMonitorSeekRequest {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: loaded.monitor_revision(),
            target: SourceMonitorTime {
                value: 48,
                timebase_numerator: 48_000,
                timebase_denominator: 1,
            },
        })
        .expect("wave source should seek exactly");
    assert_eq!(seeked.current().unwrap().value, 48);

    let marked_in = state
        .source_monitor_update_marks(SourceMonitorMarkUpdate {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: seeked.monitor_revision(),
            mutation: SourceMonitorMarkMutation::SetIn,
        })
        .expect("in mark should publish atomically");
    assert_eq!(marked_in.monitor().marks().in_mark.unwrap().value, 48);
    assert!(marked_in.monitor().marks_fresh());
    library = marked_in.media_library().clone();
    let (_, library_revision) = library_fences(&library);

    let seeked = state
        .source_monitor_seek(SourceMonitorSeekRequest {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: marked_in.monitor().monitor_revision(),
            target: SourceMonitorTime {
                value: 96,
                timebase_numerator: 48_000,
                timebase_denominator: 1,
            },
        })
        .expect("wave source should retain the open session across mark publication");
    let marked_out = state
        .source_monitor_update_marks(SourceMonitorMarkUpdate {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: seeked.monitor_revision(),
            mutation: SourceMonitorMarkMutation::SetOut,
        })
        .expect("out mark should publish atomically");
    assert_eq!(marked_out.monitor().marks().out_mark.unwrap().value, 96);
    library = marked_out.media_library().clone();
    let (_, library_revision) = library_fences(&library);

    let seeked = state
        .source_monitor_seek(SourceMonitorSeekRequest {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: marked_out.monitor().monitor_revision(),
            target: SourceMonitorTime {
                value: 120,
                timebase_numerator: 48_000,
                timebase_denominator: 1,
            },
        })
        .unwrap();
    let reversed = state
        .source_monitor_update_marks(SourceMonitorMarkUpdate {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: seeked.monitor_revision(),
            mutation: SourceMonitorMarkMutation::SetIn,
        })
        .expect_err("in after out must not publish");
    assert_eq!(reversed.code(), "source_monitor_marks_reversed");
    assert_eq!(
        library_fences(&state.media_library().unwrap()).1,
        library_revision
    );

    write_mono_wave(&wave, 481);
    let scanned = state
        .scan_media_sources(MediaSourceScanRequest {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            media_ids: vec![media.media_id().to_owned()],
            verify_content: true,
        })
        .expect("changed source bytes should publish scanner evidence");
    let (_, scanned_library_revision) = library_fences(&scanned);
    assert_eq!(
        state.source_monitor_snapshot().unwrap().engine_state(),
        SourceMonitorEngineState::Stale
    );
    let changed_seek = state
        .source_monitor_seek(SourceMonitorSeekRequest {
            expected_project_revision: project_revision,
            expected_library_revision: scanned_library_revision,
            expected_monitor_revision: seeked.monitor_revision(),
            target: SourceMonitorTime {
                value: 96,
                timebase_numerator: 48_000,
                timebase_denominator: 1,
            },
        })
        .expect_err("changed source evidence must fence the retained session");
    assert_eq!(changed_seek.code(), "source_monitor_stale");

    let unloaded = state
        .source_monitor_unload(SourceMonitorUnloadRequest {
            expected_monitor_revision: seeked.monitor_revision(),
        })
        .expect("unload should release the retained source");
    assert_eq!(unloaded.engine_state(), SourceMonitorEngineState::Empty);
    drop(state);

    let reopened = DesktopProjectState::default();
    reopened.initialize(root.join("recovery")).unwrap();
    reopened
        .execute(DesktopProjectCommand::Open {
            path: project.to_string_lossy().into_owned(),
        })
        .unwrap();
    let persisted = serde_json::to_value(reopened.media_library().unwrap()).unwrap();
    assert_eq!(
        persisted["items"][0]["source_monitor_marks"]["in_mark"]["value"],
        48
    );
    assert_eq!(
        persisted["items"][0]["source_monitor_marks"]["out_mark"]["value"],
        96
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn image_sequence_source_uses_the_exact_project_range_and_rejects_overrun() {
    let root = owned_test_root("sequence");
    std::fs::create_dir_all(&root).unwrap();
    let paths = (1..=3)
        .map(|frame| root.join(format!("plate_{frame:04}.png")))
        .collect::<Vec<_>>();
    for (index, path) in paths.iter().enumerate() {
        write_png(path, u8::try_from(index * 80).unwrap());
    }

    let state = DesktopProjectState::default();
    create_project(&state, &root);
    let imported = state
        .import_media(import_request(0, paths))
        .expect("image sequence should import");
    let media = &imported.imported()[0];
    let library = state.media_library().unwrap();
    let (project_revision, library_revision) = library_fences(&library);
    let loaded = state
        .source_monitor_load(SourceMonitorLoadRequest {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            media_id: media.media_id().to_owned(),
            expected_source_fingerprint: media.content_fingerprint().to_owned(),
        })
        .unwrap();
    assert_eq!(loaded.current().unwrap().value, 1);
    assert_eq!(loaded.current().unwrap().timebase_numerator, 24);

    let end = state
        .source_monitor_seek(SourceMonitorSeekRequest {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: loaded.monitor_revision(),
            target: SourceMonitorTime {
                value: 3,
                timebase_numerator: 24,
                timebase_denominator: 1,
            },
        })
        .expect("inclusive last sequence frame should seek");
    assert_eq!(end.current().unwrap().value, 3);
    let overrun = state
        .source_monitor_seek(SourceMonitorSeekRequest {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: end.monitor_revision(),
            target: SourceMonitorTime {
                value: 4,
                timebase_numerator: 24,
                timebase_denominator: 1,
            },
        })
        .expect_err("sequence seek beyond the exact range must fail");
    assert_eq!(overrun.code(), "source_monitor_seek_invalid");
    assert_eq!(
        state.source_monitor_snapshot().unwrap().monitor_revision(),
        end.monitor_revision()
    );

    let _ = std::fs::remove_dir_all(root);
}
