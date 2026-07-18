use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use superi_desktop::project_lifecycle::{
    DesktopMediaImportOrigin, DesktopMediaImportRequest, DesktopProjectCommand,
    DesktopProjectCreateRequest, DesktopProjectState, MediaPreviewRequest,
};

const AUDIO_FRAME_COUNT: u32 = 262_145;

fn owned_test_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "superi-media-preview-{}-{nonce}",
        std::process::id()
    ))
}

fn create_project(state: &DesktopProjectState, root: &Path) {
    state.initialize(root.join("recovery")).unwrap();
    state
        .execute(DesktopProjectCommand::Create {
            path: root.join("preview.superi").to_string_lossy().into_owned(),
            project: DesktopProjectCreateRequest {
                project_id: "project:00000000000000000000000000000310".into(),
                project_name: "Preview Contract".into(),
                root_timeline_id: "timeline:00000000000000000000000000010310".into(),
                root_timeline_name: "Preview Timeline".into(),
                edit_rate_numerator: 24,
                edit_rate_denominator: 1,
            },
        })
        .unwrap();
}

fn import_request(
    expected_project_revision: u64,
    paths: Vec<PathBuf>,
) -> DesktopMediaImportRequest {
    DesktopMediaImportRequest {
        expected_project_revision,
        origin: DesktopMediaImportOrigin::Api,
        paths: paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        recursive: false,
        detect_image_sequences: true,
    }
}

fn write_stereo_wave(path: &Path) {
    let data_len = AUDIO_FRAME_COUNT * 4;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&48_000_u32.to_le_bytes());
    bytes.extend_from_slice(&(48_000_u32 * 4).to_le_bytes());
    bytes.extend_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for frame in 0..AUDIO_FRAME_COUNT {
        let left = if frame % 2 == 0 { i16::MIN } else { i16::MAX };
        let right = if frame % 3 == 0 { i16::MAX } else { -16_384 };
        bytes.extend_from_slice(&left.to_le_bytes());
        bytes.extend_from_slice(&right.to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}

fn write_png(path: &Path, red: u8) {
    let mut pixels = Vec::new();
    for _ in 0..4 {
        pixels.extend_from_slice(&[red, 32, 255_u8.saturating_sub(red), 255]);
    }
    let image = image::RgbaImage::from_raw(2, 2, pixels).unwrap();
    image
        .save_with_format(path, image::ImageFormat::Png)
        .unwrap();
}

fn request(
    project_revision: u64,
    library_revision: u64,
    media_id: &str,
    freshness: &str,
) -> MediaPreviewRequest {
    MediaPreviewRequest {
        expected_project_revision: project_revision,
        expected_library_revision: library_revision,
        media_id: media_id.to_owned(),
        expected_freshness: freshness.to_owned(),
    }
}

fn assert_ready_png(product: &Value) {
    assert_eq!(product["status"], "ready");
    let artifact = &product["artifact"];
    assert!(artifact["width"].as_u64().unwrap() > 0);
    assert!(artifact["height"].as_u64().unwrap() > 0);
    assert!(artifact["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));
}

#[test]
fn real_stills_sequences_and_wave_audio_generate_bounded_semantic_previews() {
    let root = owned_test_root();
    let plates = root.join("plates");
    std::fs::create_dir_all(&plates).unwrap();
    let plate_paths: Vec<_> = (1..=3)
        .map(|frame| plates.join(format!("shot_{frame:04}.png")))
        .collect();
    for (index, path) in plate_paths.iter().enumerate() {
        write_png(path, u8::try_from(index * 80).unwrap());
    }
    let still_path = root.join("poster.png");
    write_png(&still_path, 192);
    let wave_path = root.join("dialog.wav");
    write_stereo_wave(&wave_path);

    let state = DesktopProjectState::default();
    create_project(&state, &root);
    let plates = state
        .import_media(import_request(0, plate_paths))
        .expect("image sequence should import");
    let still = state
        .import_media(import_request(1, vec![still_path]))
        .expect("still image should import");
    let audio = state
        .import_media(import_request(2, vec![wave_path]))
        .expect("wave audio should import");
    assert_eq!(plates.imported().len(), 1);
    assert_eq!(still.imported().len(), 1);
    assert_eq!(audio.imported().len(), 1);

    let plate = &plates.imported()[0];
    let plate_bundle = state
        .generate_media_preview(request(3, 3, plate.media_id(), plate.content_fingerprint()))
        .expect("fresh image sequence should generate previews");
    let plate_json = serde_json::to_value(plate_bundle).unwrap();
    assert_eq!(plate_json["media_id"], plate.media_id());
    assert_eq!(plate_json["freshness"], plate.content_fingerprint());
    assert_ready_png(&plate_json["thumbnail"]);
    assert_ready_png(&plate_json["preview"]);
    assert_eq!(plate_json["waveform"]["status"], "unavailable");
    assert_eq!(plate_json["filmstrip"]["status"], "ready");
    let frames = plate_json["filmstrip"]["artifact"]["frames"]
        .as_array()
        .unwrap();
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0]["source_index"], 0);
    assert_eq!(frames[1]["source_index"], 1);
    assert_eq!(frames[2]["source_index"], 2);
    for frame in frames {
        assert!(frame["data_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }

    let still = &still.imported()[0];
    let still_bundle = state
        .generate_media_preview(request(3, 3, still.media_id(), still.content_fingerprint()))
        .expect("fresh still should generate previews");
    let still_json = serde_json::to_value(still_bundle).unwrap();
    assert_ready_png(&still_json["thumbnail"]);
    assert_ready_png(&still_json["preview"]);
    assert_eq!(
        still_json["filmstrip"]["artifact"]["frames"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(still_json["waveform"]["status"], "unavailable");

    let audio = &audio.imported()[0];
    let audio_bundle = state
        .generate_media_preview(request(3, 3, audio.media_id(), audio.content_fingerprint()))
        .expect("fresh wave audio should generate a semantic waveform");
    let audio_json = serde_json::to_value(audio_bundle).unwrap();
    assert_eq!(audio_json["thumbnail"]["status"], "unavailable");
    assert_eq!(audio_json["filmstrip"]["status"], "unavailable");
    assert_ready_png(&audio_json["preview"]);
    assert_eq!(audio_json["waveform"]["status"], "ready");
    let waveform = &audio_json["waveform"]["artifact"];
    assert_eq!(waveform["start_sample"], 0);
    assert_eq!(waveform["sample_rate"], 48_000);
    assert_eq!(waveform["frame_count"], AUDIO_FRAME_COUNT);
    assert_eq!(
        waveform["channel_layout"],
        serde_json::json!(["front_left", "front_right"])
    );
    assert!(waveform["image"]["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));

    let stale = state
        .generate_media_preview(request(3, 2, audio.media_id(), audio.content_fingerprint()))
        .expect_err("stale library revisions must not read source content");
    assert_eq!(stale.code(), "media_preview_revision_stale");

    let stale_freshness = state
        .generate_media_preview(request(3, 3, audio.media_id(), "sha256:stale"))
        .expect_err("stale source identity must not read source content");
    assert_eq!(stale_freshness.code(), "media_preview_freshness_stale");

    let unsupported_path = root.join("camera.mp4");
    std::fs::write(&unsupported_path, b"unsupported-video-container").unwrap();
    let unsupported = state
        .import_media(import_request(3, vec![unsupported_path]))
        .expect("unsupported preview formats should remain importable");
    let unsupported = &unsupported.imported()[0];
    let unsupported_bundle = state
        .generate_media_preview(request(
            4,
            4,
            unsupported.media_id(),
            unsupported.content_fingerprint(),
        ))
        .expect("unsupported preview formats should return explicit product state");
    let unsupported_json = serde_json::to_value(unsupported_bundle).unwrap();
    for product in ["thumbnail", "filmstrip", "waveform", "preview"] {
        assert_eq!(unsupported_json[product]["status"], "unavailable");
        assert!(!unsupported_json[product]["reason"]
            .as_str()
            .unwrap()
            .is_empty());
    }

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn production_command_bridge_and_inspector_consume_all_four_preview_products() {
    let tauri_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let app_root = tauri_root.parent().unwrap();
    let host = std::fs::read_to_string(tauri_root.join("src/lib.rs")).unwrap();
    let lifecycle = std::fs::read_to_string(tauri_root.join("src/project_lifecycle.rs")).unwrap();
    let bridge = std::fs::read_to_string(app_root.join("src/project-lifecycle.ts")).unwrap();
    let app = std::fs::read_to_string(app_root.join("src/App.tsx")).unwrap();

    assert!(host.contains("project_lifecycle::desktop_generate_media_preview"));
    assert!(lifecycle.contains("pub async fn desktop_generate_media_preview"));
    assert!(lifecycle.contains("spawn_blocking(move || state.generate_media_preview(request))"));
    assert!(bridge.contains("export interface MediaPreviewBundle"));
    assert!(bridge.contains("export async function generateProjectMediaPreview"));
    assert!(bridge.contains("desktop_generate_media_preview"));
    assert!(app.contains("generateProjectMediaPreview"));
    assert!(app.contains("media-preview-filmstrip"));
    assert!(app.contains("media-preview-waveform"));
    assert!(app.contains("channel_layout"));
    assert!(app.contains("frame_count"));
}
