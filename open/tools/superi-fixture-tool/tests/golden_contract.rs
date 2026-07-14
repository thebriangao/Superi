use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;
use superi_fixture_tool::golden::{
    verify_audio, verify_frame, verify_project, verify_timeline, AudioGolden, FrameGolden,
    ProjectGolden, TimelineGolden,
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempGolden(PathBuf);

impl TempGolden {
    fn new(name: &str) -> Self {
        let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "superi-golden-{}-{suffix}-{name}.json",
            std::process::id()
        ));
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, bytes: &[u8]) {
        fs::write(&self.0, bytes).expect("golden file must be written");
    }
}

impl Drop for TempGolden {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn frame() -> FrameGolden {
    FrameGolden::new(
        2,
        1,
        16,
        "rgba16f-le",
        vec!["R", "G", "B", "A"],
        "linear-rec2020",
        "premultiplied",
        (0_u8..16).collect(),
    )
    .expect("frame must be valid")
}

fn audio() -> AudioGolden {
    AudioGolden::new(
        48_000,
        -960,
        2,
        vec!["L", "R"],
        "f32-le",
        true,
        vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x80, 0x3f, 0, 0, 0x80, 0xbf],
    )
    .expect("audio must be valid")
}

#[test]
fn frame_harness_is_byte_exact_and_layout_aware() {
    let expected = TempGolden::new("frame");
    let actual = frame();
    expected.write(&actual.to_golden_bytes().expect("frame must encode"));

    verify_frame(expected.path(), &actual).expect("identical frame must pass");

    let changed = FrameGolden::new(
        2,
        1,
        8,
        "rgba16f-le",
        vec!["R", "G", "B", "A"],
        "linear-rec2020",
        "premultiplied",
        (0_u8..16).collect(),
    )
    .expect_err("short row stride must be rejected before comparison");
    assert_eq!(changed.code(), "frame.row_stride");

    let mut changed_bytes = (0_u8..16).collect::<Vec<_>>();
    changed_bytes[9] ^= 1;
    let changed = FrameGolden::new(
        2,
        1,
        16,
        "rgba16f-le",
        vec!["R", "G", "B", "A"],
        "linear-rec2020",
        "premultiplied",
        changed_bytes,
    )
    .expect("changed frame remains structurally valid");
    let mismatch = verify_frame(expected.path(), &changed).expect_err("drift must fail");
    assert_eq!(mismatch.code(), "golden.mismatch");
    assert_ne!(mismatch.expected_sha256(), mismatch.actual_sha256());
}

#[test]
fn audio_harness_preserves_sample_timing_channels_and_payload() {
    let expected = TempGolden::new("audio");
    let actual = audio();
    expected.write(&actual.to_golden_bytes().expect("audio must encode"));

    verify_audio(expected.path(), &actual).expect("identical audio must pass");

    let retimed = AudioGolden::new(
        48_000,
        -959,
        2,
        vec!["L", "R"],
        "f32-le",
        true,
        actual.payload().to_vec(),
    )
    .expect("retimed audio remains structurally valid");
    assert_eq!(
        verify_audio(expected.path(), &retimed)
            .expect_err("sample origin drift must fail")
            .code(),
        "golden.mismatch"
    );

    let swapped = AudioGolden::new(
        48_000,
        -960,
        2,
        vec!["R", "L"],
        "f32-le",
        true,
        actual.payload().to_vec(),
    )
    .expect("channel reorder remains structurally valid");
    assert_eq!(
        verify_audio(expected.path(), &swapped)
            .expect_err("channel meaning drift must fail")
            .code(),
        "golden.mismatch"
    );

    let malformed = AudioGolden::new(48_000, 0, 2, vec!["L", "R"], "f32-le", true, vec![0; 8])
        .expect_err("payload must contain every declared sample");
    assert_eq!(malformed.code(), "audio.payload_length");
}

#[test]
fn timeline_and_project_harnesses_canonicalize_nested_object_order() {
    let timeline_path = TempGolden::new("timeline");
    let expected_timeline = TimelineGolden::new(
        "otio.1",
        json!({"tracks": [{"clips": ["a", "b"], "kind": "video"}], "rate": [24_000, 1_001]}),
    )
    .expect("timeline must be valid");
    timeline_path.write(
        &expected_timeline
            .to_golden_bytes()
            .expect("timeline must encode"),
    );
    let reordered_timeline = TimelineGolden::new(
        "otio.1",
        serde_json::from_str(
            r#"{"rate":[24000,1001],"tracks":[{"kind":"video","clips":["a","b"]}]}"#,
        )
        .expect("JSON must parse"),
    )
    .expect("timeline must be valid");
    verify_timeline(timeline_path.path(), &reordered_timeline)
        .expect("object insertion order must not affect a golden");

    let project_path = TempGolden::new("project");
    let expected_project = ProjectGolden::new(
        "superi-project.1",
        json!({"timeline": {"id": "main"}, "settings": {"audio_rate": 48_000}}),
    )
    .expect("project must be valid");
    project_path.write(
        &expected_project
            .to_golden_bytes()
            .expect("project must encode"),
    );
    let reordered_project = ProjectGolden::new(
        "superi-project.1",
        serde_json::from_str(r#"{"settings":{"audio_rate":48000},"timeline":{"id":"main"}}"#)
            .expect("JSON must parse"),
    )
    .expect("project must be valid");
    verify_project(project_path.path(), &reordered_project)
        .expect("nested object order must not affect a golden");

    let changed = ProjectGolden::new(
        "superi-project.1",
        json!({"timeline": {"id": "main"}, "settings": {"audio_rate": 44_100}}),
    )
    .expect("changed project remains structurally valid");
    assert_eq!(
        verify_project(project_path.path(), &changed)
            .expect_err("semantic project drift must fail")
            .code(),
        "golden.mismatch"
    );
}

#[test]
fn verification_never_creates_or_rewrites_expected_files() {
    let missing = TempGolden::new("missing");
    let error = verify_frame(missing.path(), &frame()).expect_err("missing golden must fail");
    assert_eq!(error.code(), "golden.read");
    assert!(
        !missing.path().exists(),
        "verification must not bless output"
    );

    let expected = TempGolden::new("readonly");
    expected.write(b"not valid JSON\n");
    let before = fs::read(expected.path()).expect("golden must be readable");
    assert_eq!(
        verify_frame(expected.path(), &frame())
            .expect_err("invalid golden must fail")
            .code(),
        "golden.parse"
    );
    assert_eq!(
        fs::read(expected.path()).expect("golden must remain readable"),
        before,
        "verification must leave expected bytes untouched"
    );
}

#[test]
fn canonical_workspace_goldens_exercise_all_four_harnesses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/golden/harness/v1");

    verify_frame(&root.join("frame.json"), &frame()).expect("canonical frame must match");
    verify_audio(&root.join("audio.json"), &audio()).expect("canonical audio must match");
    verify_timeline(
        &root.join("timeline.json"),
        &TimelineGolden::new(
            "otio.1",
            json!({"tracks": [{"kind": "video", "clips": ["a", "b"]}], "rate": [24_000, 1_001]}),
        )
        .expect("timeline must be valid"),
    )
    .expect("canonical timeline must match");
    verify_project(
        &root.join("project.json"),
        &ProjectGolden::new(
            "superi-project.1",
            json!({"timeline": {"id": "main"}, "settings": {"audio_rate": 48_000}}),
        )
        .expect("project must be valid"),
    )
    .expect("canonical project must match");
}
