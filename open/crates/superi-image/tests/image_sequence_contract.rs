use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use superi_core::error::{ErrorCategory, Recoverability};
use superi_image::sequence::{
    ImageSequenceManifest, ImageSequencePattern, MissingFramePolicy, SequenceSubstitution,
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new(test_name: &str) -> Self {
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "superi-image-sequence-{}-{test_name}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn touch(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, name.as_bytes()).unwrap();
        path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn pattern_parsing_preserves_signed_numbering_padding_and_surrounding_name() {
    let directory = TemporaryDirectory::new("parse");
    let selected = directory.path().join("shot.v2.beauty.-0001.left.exr");

    let parsed = ImageSequencePattern::parse_frame_path(&selected).unwrap();
    assert_eq!(parsed.frame_number(), -1);
    assert_eq!(parsed.pattern().directory(), directory.path());
    assert_eq!(parsed.pattern().prefix(), "shot.v2.beauty.");
    assert_eq!(parsed.pattern().suffix(), ".left.exr");
    assert_eq!(parsed.pattern().zero_padding(), 4);
    assert_eq!(
        parsed.pattern().path_for_frame(-12).unwrap(),
        directory.path().join("shot.v2.beauty.-0012.left.exr")
    );
    assert_eq!(
        parsed.pattern().path_for_frame(10_000).unwrap(),
        directory.path().join("shot.v2.beauty.10000.left.exr")
    );
}

#[test]
fn malformed_or_unnumbered_paths_fail_with_actionable_shared_errors() {
    for path in [
        Path::new("plate.exr"),
        Path::new(""),
        Path::new("plate.999999999999999999999.exr"),
    ] {
        let error = ImageSequencePattern::parse_frame_path(path).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert!(!error.contexts().is_empty());
    }

    assert_eq!(
        ImageSequencePattern::new(PathBuf::from("plates"), "plate.", ".exr", 0)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn discovery_is_deterministic_and_keeps_explicit_logical_and_file_numbering() {
    let directory = TemporaryDirectory::new("discover");
    directory.touch("plate.1005.exr");
    let selected = directory.touch("plate.1001.exr");
    directory.touch("plate.1003.exr");
    directory.touch("plate.01002.exr");
    directory.touch("plate.1003.png");
    directory.touch("other.1003.exr");
    fs::create_dir(directory.path().join("plate.1007.exr")).unwrap();

    let manifest = ImageSequenceManifest::discover(&selected, 2).unwrap();
    assert_eq!(manifest.first_frame_number(), 1001);
    assert_eq!(manifest.last_frame_number(), 1005);
    assert_eq!(manifest.frame_step(), 2);
    assert_eq!(manifest.logical_frame_count(), 3);
    assert_eq!(manifest.available_frame_count(), 3);
    assert!(manifest.missing_frame_numbers().is_empty());
    assert_eq!(
        manifest
            .frames()
            .map(|frame| (
                frame.image_number(),
                frame.file_frame_number(),
                frame.path().map(Path::to_path_buf),
            ))
            .collect::<Vec<_>>(),
        vec![
            (0, 1001, Some(directory.path().join("plate.1001.exr"))),
            (1, 1003, Some(directory.path().join("plate.1003.exr"))),
            (2, 1005, Some(directory.path().join("plate.1005.exr"))),
        ]
    );

    let rediscovered = ImageSequenceManifest::discover(&selected, 2).unwrap();
    assert_eq!(manifest, rediscovered);
}

#[test]
fn discovery_reports_gaps_instead_of_inferring_a_larger_frame_step() {
    let directory = TemporaryDirectory::new("gaps");
    let selected = directory.touch("render.0001.exr");
    directory.touch("render.0003.exr");
    directory.touch("render.0004.exr");

    let manifest = ImageSequenceManifest::discover(&selected, 1).unwrap();
    assert_eq!(manifest.logical_frame_count(), 4);
    assert_eq!(manifest.available_frame_count(), 3);
    assert_eq!(manifest.missing_frame_numbers(), &[2]);
    assert_eq!(manifest.frame(1).unwrap().file_frame_number(), 2);
    assert!(manifest.frame(1).unwrap().path().is_none());

    let error = ImageSequenceManifest::discover(&selected, 2).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(error
        .contexts()
        .iter()
        .any(|context| context.field("file_frame") == Some("4")));
}

#[test]
fn missing_frame_policy_distinguishes_error_hold_and_black_resolution() {
    let directory = TemporaryDirectory::new("missing-policy");
    let selected = directory.touch("comp.1001.exr");
    directory.touch("comp.1003.exr");
    directory.touch("comp.1004.exr");
    let manifest = ImageSequenceManifest::discover(&selected, 1).unwrap();

    let error = manifest.resolve(1, MissingFramePolicy::Error).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(error
        .contexts()
        .iter()
        .any(|context| context.field("file_frame") == Some("1002")));

    let held = manifest.resolve(1, MissingFramePolicy::Hold).unwrap();
    assert_eq!(held.requested().image_number(), 1);
    assert_eq!(held.requested().file_frame_number(), 1002);
    assert_eq!(held.source_frame_number(), Some(1001));
    assert_eq!(held.substitution(), SequenceSubstitution::Hold);
    assert_eq!(
        held.read_path(),
        Some(directory.path().join("comp.1001.exr").as_path())
    );

    let black = manifest.resolve(1, MissingFramePolicy::Black).unwrap();
    assert_eq!(black.requested().file_frame_number(), 1002);
    assert_eq!(black.source_frame_number(), None);
    assert_eq!(black.substitution(), SequenceSubstitution::Black);
    assert_eq!(
        black.reference_path(),
        Some(directory.path().join("comp.1001.exr").as_path())
    );

    let exact = manifest.resolve(2, MissingFramePolicy::Error).unwrap();
    assert_eq!(exact.requested().file_frame_number(), 1003);
    assert_eq!(exact.source_frame_number(), Some(1003));
    assert_eq!(exact.substitution(), SequenceSubstitution::None);
    assert_eq!(exact.reference_path(), None);
}

#[test]
fn leading_missing_frames_cannot_hold_but_can_reference_the_next_frame_for_black() {
    let directory = TemporaryDirectory::new("leading-gap");
    let selected = directory.touch("comp.1001.exr");
    directory.touch("comp.1003.exr");
    let pattern = ImageSequencePattern::parse_frame_path(&selected)
        .unwrap()
        .into_pattern();
    let manifest = ImageSequenceManifest::discover_range(pattern, 1000, 1003, 1).unwrap();

    let hold_error = manifest.resolve(0, MissingFramePolicy::Hold).unwrap_err();
    assert_eq!(hold_error.category(), ErrorCategory::NotFound);

    let black = manifest.resolve(0, MissingFramePolicy::Black).unwrap();
    assert_eq!(black.requested().file_frame_number(), 1000);
    assert_eq!(black.source_frame_number(), None);
    assert_eq!(
        black.reference_path(),
        Some(directory.path().join("comp.1001.exr").as_path())
    );
}

#[test]
fn discovery_rejects_zero_steps_missing_directories_and_out_of_range_addresses() {
    let directory = TemporaryDirectory::new("invalid-discovery");
    let selected = directory.touch("plate.0001.exr");

    assert_eq!(
        ImageSequenceManifest::discover(&selected, 0)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        ImageSequenceManifest::discover(directory.path().join("missing/plate.0001.exr"), 1)
            .unwrap_err()
            .category(),
        ErrorCategory::NotFound
    );

    let manifest = ImageSequenceManifest::discover(&selected, 1).unwrap();
    assert_eq!(
        manifest.frame(1).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        manifest
            .resolve(1, MissingFramePolicy::Error)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}
