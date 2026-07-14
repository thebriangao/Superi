use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use superi_fixture_tool::{
    generate_media_error_baseline, MEDIA_ERROR_BASELINE_CASE_COUNT, MEDIA_ERROR_CATALOG_NAME,
    MEDIA_ERROR_MALFORMED_WAVE_NAME, MEDIA_ERROR_MANIFEST_NAME, MEDIA_ERROR_PARTIAL_WAVE_NAME,
    MEDIA_ERROR_TRUNCATED_AIFF_NAME, MEDIA_ERROR_UNSUPPORTED_AIFC_NAME,
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TemporaryOutput(PathBuf);

impl TemporaryOutput {
    fn new(label: &str) -> Self {
        let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "superi-media-error-fixture-{label}-{}-{suffix}",
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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/media/error-cases/v1")
}

#[test]
fn generator_reproduces_canonical_media_error_artifacts_byte_for_byte() {
    let first = TemporaryOutput::new("first");
    let second = TemporaryOutput::new("second");

    let first_report =
        generate_media_error_baseline(first.path()).expect("generation must succeed");
    let second_report =
        generate_media_error_baseline(second.path()).expect("generation must repeat");

    assert_eq!(first_report.case_count(), MEDIA_ERROR_BASELINE_CASE_COUNT);
    assert_eq!(first_report.case_count(), 4);
    assert!(first_report.catalog_bytes() > 0);
    assert!(first_report.catalog_bytes() < 4 * 1024);
    assert!(first_report.payload_bytes() > 0);
    assert!(first_report.payload_bytes() < 4 * 1024);
    assert_eq!(first_report, second_report);

    for name in [
        MEDIA_ERROR_CATALOG_NAME,
        MEDIA_ERROR_MALFORMED_WAVE_NAME,
        MEDIA_ERROR_TRUNCATED_AIFF_NAME,
        MEDIA_ERROR_UNSUPPORTED_AIFC_NAME,
        MEDIA_ERROR_PARTIAL_WAVE_NAME,
        MEDIA_ERROR_MANIFEST_NAME,
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

    let error =
        generate_media_error_baseline(output.path()).expect_err("existing output must fail");

    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(
        fs::read_to_string(output.path().join("sentinel")).expect("sentinel must remain"),
        "keep\n"
    );
}
