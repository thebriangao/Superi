use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use superi_fixture_tool::{
    generate_color_baseline, COLOR_BASELINE_IMAGE_COUNT, COLOR_BASELINE_SEQUENCE_FRAME_COUNT,
    COLOR_IMAGE_CATALOG_NAME, COLOR_MANIFEST_NAME, COLOR_PAYLOAD_NAME, COLOR_SEQUENCE_CATALOG_NAME,
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TemporaryOutput(PathBuf);

impl TemporaryOutput {
    fn new(label: &str) -> Self {
        let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "superi-color-fixture-{label}-{}-{suffix}",
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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/color/image-sequences/v1")
}

#[test]
fn generator_reproduces_canonical_color_artifacts_byte_for_byte() {
    let first = TemporaryOutput::new("first");
    let second = TemporaryOutput::new("second");

    let first_report = generate_color_baseline(first.path()).expect("generation must succeed");
    let second_report = generate_color_baseline(second.path()).expect("generation must repeat");

    assert_eq!(first_report.image_count(), COLOR_BASELINE_IMAGE_COUNT);
    assert_eq!(
        first_report.sequence_frame_count(),
        COLOR_BASELINE_SEQUENCE_FRAME_COUNT
    );
    assert!(first_report.payload_bytes() > 0);
    assert!(first_report.payload_bytes() < 4 * 1024);
    assert_eq!(first_report, second_report);

    for name in [
        COLOR_IMAGE_CATALOG_NAME,
        COLOR_SEQUENCE_CATALOG_NAME,
        COLOR_PAYLOAD_NAME,
        COLOR_MANIFEST_NAME,
    ] {
        let first_bytes = fs::read(first.path().join(name)).expect("first artifact must exist");
        let second_bytes = fs::read(second.path().join(name)).expect("second artifact must exist");
        let canonical = fs::read(canonical_fixture().join(name))
            .expect("canonical artifact must exist in the repository");
        assert_eq!(
            first_bytes, second_bytes,
            "repeated {name} bytes must match"
        );
        assert_eq!(first_bytes, canonical, "generated {name} must be canonical");
    }
}

#[test]
fn generator_refuses_to_replace_an_existing_output_directory() {
    let output = TemporaryOutput::new("existing");
    fs::create_dir(output.path()).expect("test output must be created");
    fs::write(output.path().join("sentinel"), "keep\n").expect("sentinel must be written");

    let error = generate_color_baseline(output.path()).expect_err("existing output must fail");

    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(
        fs::read_to_string(output.path().join("sentinel")).expect("sentinel must remain"),
        "keep\n"
    );
}
