use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use superi_fixture_tool::{
    generate_video_baseline, VIDEO_BASELINE_CASE_COUNT, VIDEO_CATALOG_NAME, VIDEO_MANIFEST_NAME,
    VIDEO_PAYLOAD_NAME,
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TemporaryOutput(PathBuf);

impl TemporaryOutput {
    fn new() -> Self {
        let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "superi-video-fixture-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryOutput {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn canonical_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/video/pixel-formats/v1")
}

#[test]
fn generator_reproduces_every_canonical_artifact_byte_for_byte() {
    let output = TemporaryOutput::new();

    let report = generate_video_baseline(output.path()).expect("generation must succeed");

    assert_eq!(report.case_count(), VIDEO_BASELINE_CASE_COUNT);
    assert!(report.payload_bytes() > 0);
    assert!(report.payload_bytes() < 64 * 1024);

    for name in [VIDEO_CATALOG_NAME, VIDEO_PAYLOAD_NAME, VIDEO_MANIFEST_NAME] {
        let generated = fs::read(output.path().join(name)).expect("generated artifact must exist");
        let canonical = fs::read(canonical_fixture().join(name))
            .expect("canonical artifact must exist in the repository");
        assert_eq!(generated, canonical, "generated {name} must be canonical");
    }
}

#[test]
fn generator_refuses_to_replace_an_existing_output_directory() {
    let output = TemporaryOutput::new();
    fs::create_dir(output.path()).expect("test output must be created");
    fs::write(output.path().join("sentinel"), "keep\n").expect("sentinel must be written");

    let error = generate_video_baseline(output.path()).expect_err("existing output must fail");

    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(
        fs::read_to_string(output.path().join("sentinel")).expect("sentinel must remain"),
        "keep\n"
    );
}
