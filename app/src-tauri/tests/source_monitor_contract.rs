use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use superi_api::editor::{
    EditorClipSource, EditorClipTimeMap, EditorRetimeSegment, EditorThreePointPlacement,
    EditorTrackItem, EditorialObjectId, ExactDuration, ExactTime, ExactTimeRange, ExactTimebase,
    ExecuteProjectCommand, ProjectAction, ProjectCommand, TimelineEditOperation,
};
use superi_desktop::engine::LinkedEngineProcess;
use superi_desktop::lifecycle::ApplicationLifecycle;
use superi_desktop::project_lifecycle::{
    DesktopMediaImportOrigin, DesktopMediaImportRequest, DesktopProjectCommand,
    DesktopProjectCreateRequest, DesktopProjectState, MediaLibrarySnapshot, MediaSourceScanRequest,
    SourceMonitorEngineState, SourceMonitorLoadRequest, SourceMonitorMarkMutation,
    SourceMonitorMarkUpdate, SourceMonitorSeekRequest, SourceMonitorTime,
    SourceMonitorUnloadRequest,
};
use superi_desktop::transport::{
    DesktopTransportCommand, DesktopTransportReply, DesktopTransportState,
};
use superi_engine::editor as engine;

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

fn create_point_edit_project(state: &DesktopProjectState, root: &Path) -> PathBuf {
    state.initialize(root.join("recovery")).unwrap();
    let project = root.join("source-monitor-point-edit.superi");
    let rate = engine::FrameRate::FPS_24.timebase();
    let root_timeline = engine::TimelineId::from_raw(0x10_314);
    let track_id = engine::TrackId::from_raw(0x20_314);
    let gap = engine::Gap::new(
        engine::GapId::from_raw(0x30_314),
        "Target bed",
        engine::TimeRange::new(
            engine::RationalTime::zero(rate),
            engine::Duration::new(120, rate).unwrap(),
        )
        .unwrap(),
    );
    let mut timeline = engine::Timeline::new(
        root_timeline,
        "Source Monitor Point Edit",
        rate,
        engine::RationalTime::zero(rate),
        vec![engine::Track::new(
            track_id,
            "V1",
            engine::TrackSemantics::Video(engine::VideoTrackSemantics::new(
                engine::FrameRate::FPS_24,
                engine::VideoCompositing::Over,
            )),
            vec![engine::TrackItem::Gap(gap)],
        )],
    );
    timeline.set_track_targeted(track_id, true).unwrap();
    timeline.set_track_sync_locked(track_id, false).unwrap();
    let editorial = engine::EditorialProject::new(
        engine::ProjectId::from_raw(0x314),
        "Source Monitor Point Edit",
        [],
        [timeline],
    )
    .unwrap();
    let document = engine::ProjectDocument::new(editorial, root_timeline).unwrap();
    let mut database = engine::ProjectDatabase::create(&project).unwrap();
    database.replace(&document.snapshot()).unwrap();
    state
        .execute(DesktopProjectCommand::Open {
            path: project.to_string_lossy().into_owned(),
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

fn exact_time(value: i64) -> ExactTime {
    ExactTime {
        value,
        timebase: ExactTimebase {
            numerator: 24,
            denominator: 1,
        },
    }
}

fn exact_duration(value: u64) -> ExactDuration {
    ExactDuration {
        value,
        timebase: ExactTimebase {
            numerator: 24,
            denominator: 1,
        },
    }
}

fn exact_range(start: i64, duration: u64) -> ExactTimeRange {
    ExactTimeRange {
        start: exact_time(start),
        duration: exact_duration(duration),
    }
}

fn point_edit_clip(
    id: &str,
    media_id: &str,
    source_range: ExactTimeRange,
    record_range: ExactTimeRange,
) -> EditorTrackItem {
    EditorTrackItem::Clip {
        id: id.to_owned(),
        name: "Source monitor point edit".to_owned(),
        source: EditorClipSource::Media {
            media_id: media_id.to_owned(),
        },
        source_range,
        record_range,
        time_map: EditorClipTimeMap {
            record_duration: record_range.duration,
            source_timebase: source_range.start.timebase,
            segments: vec![EditorRetimeSegment {
                record_range: exact_range(0, record_range.duration.value),
                source_start: source_range.start,
                rate_numerator: 1,
                rate_denominator: 1,
            }],
        },
    }
}

fn point_edit_command(
    transaction_id: &str,
    revision: u64,
    operation: TimelineEditOperation,
) -> ExecuteProjectCommand {
    ExecuteProjectCommand::new(
        transaction_id,
        revision,
        ProjectCommand::Apply {
            actions: vec![ProjectAction::EditTimeline {
                operations: vec![operation],
            }],
        },
    )
}

fn execute_generated_project_command(
    transport: &DesktopTransportState,
    engine: &LinkedEngineProcess,
    projects: &DesktopProjectState,
    generation: u64,
    request_id: &str,
    command: ExecuteProjectCommand,
) -> serde_json::Value {
    let outcome = transport
        .dispatch_blocking(
            &engine.connection(),
            projects,
            DesktopTransportCommand::Request {
                generation,
                request_id: request_id.to_owned(),
                method: "superi.project.command.execute".to_owned(),
                request: serde_json::to_value(command).unwrap(),
            },
        )
        .unwrap();
    let DesktopTransportReply::Response { response, .. } = outcome.reply() else {
        panic!("point edit returned an unexpected desktop response");
    };
    let event = outcome
        .event()
        .expect("point edit must publish one project replacement event");
    assert_eq!(event.event(), "superi.project.state.changed");
    response.clone()
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
    assert!(workspaces.contains("<SourceMonitor"));
    assert!(workspaces.contains("projectRevision={snapshot?.project.project_revision ?? null}"));
    assert!(workspaces.contains("onSnapshotChange={setSourceMonitor}"));
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

#[test]
fn source_monitor_point_edits_use_the_retained_generated_project_route_and_persist() {
    let root = owned_test_root("point-edit");
    std::fs::create_dir_all(&root).unwrap();
    let paths = (1..=3)
        .map(|frame| root.join(format!("edit_{frame:04}.png")))
        .collect::<Vec<_>>();
    for (index, path) in paths.iter().enumerate() {
        write_png(path, u8::try_from(index * 70).unwrap());
    }

    let projects = DesktopProjectState::default();
    let project = create_point_edit_project(&projects, &root);
    let imported = projects
        .import_media(import_request(0, paths))
        .expect("source media should import into the point-edit project");
    let media = &imported.imported()[0];
    let library = projects.media_library().unwrap();
    let (project_revision, library_revision) = library_fences(&library);
    assert_eq!(project_revision, 1);
    let loaded = projects
        .source_monitor_load(SourceMonitorLoadRequest {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            media_id: media.media_id().to_owned(),
            expected_source_fingerprint: media.content_fingerprint().to_owned(),
        })
        .unwrap();
    let marked_in = projects
        .source_monitor_update_marks(SourceMonitorMarkUpdate {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: loaded.monitor_revision(),
            mutation: SourceMonitorMarkMutation::SetIn,
        })
        .unwrap();
    let (_, library_revision) = library_fences(marked_in.media_library());
    let seeked = projects
        .source_monitor_seek(SourceMonitorSeekRequest {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: marked_in.monitor().monitor_revision(),
            target: SourceMonitorTime {
                value: 2,
                timebase_numerator: 24,
                timebase_denominator: 1,
            },
        })
        .unwrap();
    let marked_out = projects
        .source_monitor_update_marks(SourceMonitorMarkUpdate {
            expected_project_revision: project_revision,
            expected_library_revision: library_revision,
            expected_monitor_revision: seeked.monitor_revision(),
            mutation: SourceMonitorMarkMutation::SetOut,
        })
        .unwrap();
    assert_eq!(marked_out.monitor().marks().in_mark.unwrap().value, 1);
    assert_eq!(marked_out.monitor().marks().out_mark.unwrap().value, 2);

    let lifecycle = ApplicationLifecycle::new().unwrap();
    let engine_process = LinkedEngineProcess::launch(lifecycle.clone()).unwrap();
    let transport = DesktopTransportState::new();
    let DesktopTransportReply::Connected { generation, .. } = transport
        .dispatch_control(DesktopTransportCommand::Connect { after_sequence: 0 })
        .unwrap()
    else {
        panic!("point edit transport returned an unexpected connection reply");
    };

    let source_range = exact_range(1, 2);
    let record_range = exact_range(10, 2);
    let first = TimelineEditOperation::ThreePoint {
        timeline_id: "timeline:00000000000000000000000000010314".to_owned(),
        track_id: "track:00000000000000000000000000020314".to_owned(),
        clip: point_edit_clip(
            "clip:00000000000000000000000000040314",
            media.media_id(),
            source_range,
            record_range,
        ),
        placement: EditorThreePointPlacement::SourceRangeAtRecordStart {
            source_range,
            record_start: record_range.start,
        },
        fragment_ids: vec![EditorialObjectId::Gap {
            id: "gap:00000000000000000000000000050314".to_owned(),
        }],
    };
    let applied = execute_generated_project_command(
        &transport,
        &engine_process,
        &projects,
        generation,
        "point-three-source-range",
        point_edit_command("point-three-source-range", 1, first),
    );
    assert_eq!(applied["state"]["project_revision"], 2);
    assert_eq!(applied["state"]["undo_depth"], 1);

    let undone = execute_generated_project_command(
        &transport,
        &engine_process,
        &projects,
        generation,
        "point-undo",
        ExecuteProjectCommand::new("point-undo", 2, ProjectCommand::Undo {}),
    );
    assert_eq!(undone["state"]["project_revision"], 3);
    assert_eq!(undone["state"]["redo_depth"], 1);
    let redone = execute_generated_project_command(
        &transport,
        &engine_process,
        &projects,
        generation,
        "point-redo",
        ExecuteProjectCommand::new("point-redo", 3, ProjectCommand::Redo {}),
    );
    assert_eq!(redone["state"]["project_revision"], 4);

    let remaining_modes = [
        (
            "point-three-source-start",
            "clip:00000000000000000000000000060314",
            "gap:00000000000000000000000000070314",
            exact_range(30, 2),
            EditorThreePointPlacement::SourceStartOverRecordRange {
                source_start: source_range.start,
                record_range: exact_range(30, 2),
            },
        ),
        (
            "point-three-record-end",
            "clip:00000000000000000000000000080314",
            "gap:00000000000000000000000000090314",
            exact_range(40, 2),
            EditorThreePointPlacement::SourceRangeBacktimedToRecordEnd {
                source_range,
                record_end: exact_time(42),
            },
        ),
        (
            "point-three-source-end",
            "clip:000000000000000000000000000a0314",
            "gap:000000000000000000000000000b0314",
            exact_range(50, 2),
            EditorThreePointPlacement::SourceEndBacktimedOverRecordRange {
                source_end: exact_time(3),
                record_range: exact_range(50, 2),
            },
        ),
    ];
    for (offset, (transaction, clip_id, fragment_id, record_range, placement)) in
        remaining_modes.into_iter().enumerate()
    {
        let expected_revision = 4 + u64::try_from(offset).unwrap();
        let operation = TimelineEditOperation::ThreePoint {
            timeline_id: "timeline:00000000000000000000000000010314".to_owned(),
            track_id: "track:00000000000000000000000000020314".to_owned(),
            clip: point_edit_clip(clip_id, media.media_id(), source_range, record_range),
            placement,
            fragment_ids: vec![EditorialObjectId::Gap {
                id: fragment_id.to_owned(),
            }],
        };
        let result = execute_generated_project_command(
            &transport,
            &engine_process,
            &projects,
            generation,
            transaction,
            point_edit_command(transaction, expected_revision, operation),
        );
        assert_eq!(result["state"]["project_revision"], expected_revision + 1);
    }

    let four_range = exact_range(60, 2);
    let four_point = TimelineEditOperation::FourPoint {
        timeline_id: "timeline:00000000000000000000000000010314".to_owned(),
        track_id: "track:00000000000000000000000000020314".to_owned(),
        clip: point_edit_clip(
            "clip:000000000000000000000000000c0314",
            media.media_id(),
            source_range,
            four_range,
        ),
        source_range,
        record_range: four_range,
        fragment_ids: vec![EditorialObjectId::Gap {
            id: "gap:000000000000000000000000000d0314".to_owned(),
        }],
    };
    let four = execute_generated_project_command(
        &transport,
        &engine_process,
        &projects,
        generation,
        "point-four",
        point_edit_command("point-four", 7, four_point),
    );
    assert_eq!(four["state"]["project_revision"], 8);
    let monitor = projects.source_monitor_snapshot().unwrap();
    assert_eq!(monitor.engine_state(), SourceMonitorEngineState::Ready);
    assert_eq!(
        serde_json::to_value(&monitor).unwrap()["project_revision"],
        8
    );
    assert!(monitor.marks_fresh());

    lifecycle.request_shutdown().unwrap();
    engine_process.join().unwrap();
    drop(projects);
    let reopened = engine::ProjectDatabase::open_read_only(&project)
        .unwrap()
        .load()
        .unwrap();
    assert_eq!(reopened.snapshot().revision(), 8);
    let timeline = reopened
        .editorial_project()
        .timeline(engine::TimelineId::from_raw(0x10_314))
        .unwrap();
    assert!(timeline
        .track(engine::TrackId::from_raw(0x20_314))
        .unwrap()
        .items()
        .iter()
        .any(|item| item.id().to_string() == "clip:000000000000000000000000000c0314"));
    let _ = std::fs::remove_dir_all(root);
}
