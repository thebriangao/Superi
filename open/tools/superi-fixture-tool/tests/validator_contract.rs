use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use superi_fixture_tool::validate_root;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
const HELLO_SHA256: &str = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";

struct FixtureRoot(PathBuf);

impl FixtureRoot {
    fn new() -> Self {
        let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "superi-fixture-policy-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("temporary fixture root must be created");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write_valid_fixture(&self) -> PathBuf {
        let version = self.0.join("policy/hello/v1");
        fs::create_dir_all(&version).expect("fixture version directory must be created");
        fs::write(version.join("hello.txt"), "hello\n").expect("payload must be written");
        fs::write(version.join("fixture.json"), valid_manifest())
            .expect("manifest must be written");
        version
    }
}

impl Drop for FixtureRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn valid_manifest() -> String {
    format!(
        r#"{{
  "schema_version": 1,
  "fixture_id": "policy/hello",
  "fixture_version": 1,
  "description": "Deterministic UTF-8 payload for validator contracts.",
  "provenance": {{
    "kind": "synthetic",
    "source": "Authored directly in the Superi repository.",
    "author": "Superi contributors",
    "created_on": "2026-07-13",
    "license": "CC0-1.0",
    "rights": "Original synthetic content approved for unrestricted redistribution.",
    "generator": {{
      "name": "POSIX printf",
      "version": "IEEE Std 1003.1-2024",
      "command": "printf 'hello\\n' > hello.txt",
      "seed": "not-applicable"
    }},
    "parents": []
  }},
  "files": [
    {{
      "path": "hello.txt",
      "media_type": "text/plain; charset=utf-8",
      "bytes": 6,
      "sha256": "{HELLO_SHA256}"
    }}
  ]
}}"#
    )
}

fn assert_error(root: &Path, code: &str) {
    let errors = validate_root(root).expect_err("fixture root must be rejected");
    assert!(
        errors.iter().any(|error| error.code() == code),
        "expected {code}, got {errors:?}"
    );
}

#[test]
fn canonical_fixture_root_is_part_of_the_workspace_test_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures");

    let report = validate_root(&root).unwrap_or_else(|errors| {
        panic!(
            "canonical fixtures at {} violate repository policy:\n{errors}",
            root.display()
        )
    });

    assert!(
        report.fixture_count() > 0,
        "canonical fixtures must not be empty"
    );
    assert!(
        report.payload_count() > 0,
        "canonical payloads must not be empty"
    );
}

#[test]
fn accepts_complete_versioned_fixture_and_reports_inventory() {
    let root = FixtureRoot::new();
    root.write_valid_fixture();

    let report = validate_root(root.path()).expect("valid fixture root must pass");

    assert_eq!(report.fixture_count(), 1);
    assert_eq!(report.payload_count(), 1);
}

#[test]
fn rejects_content_drift_and_unlisted_payloads() {
    let drifted = FixtureRoot::new();
    let version = drifted.write_valid_fixture();
    fs::write(version.join("hello.txt"), "changed\n").expect("payload must be changed");
    assert_error(drifted.path(), "payload.size");

    let unlisted = FixtureRoot::new();
    let version = unlisted.write_valid_fixture();
    fs::write(version.join("extra.txt"), "extra\n").expect("extra payload must be written");
    assert_error(unlisted.path(), "payload.unlisted");
}

#[test]
fn rejects_directory_identity_and_version_mismatches() {
    let identity = FixtureRoot::new();
    let version = identity.write_valid_fixture();
    let manifest = valid_manifest().replace("policy/hello", "policy/other");
    fs::write(version.join("fixture.json"), manifest).expect("manifest must be changed");
    assert_error(identity.path(), "fixture.id");

    let fixture_version = FixtureRoot::new();
    let version = fixture_version.write_valid_fixture();
    let manifest = valid_manifest().replace("\"fixture_version\": 1", "\"fixture_version\": 2");
    fs::write(version.join("fixture.json"), manifest).expect("manifest must be changed");
    assert_error(fixture_version.path(), "fixture.version");
}

#[test]
fn rejects_incomplete_provenance_and_derived_fixtures_without_lineage() {
    let incomplete = FixtureRoot::new();
    let version = incomplete.write_valid_fixture();
    let manifest = valid_manifest().replace("\"license\": \"CC0-1.0\"", "\"license\": \"\"");
    fs::write(version.join("fixture.json"), manifest).expect("manifest must be changed");
    assert_error(incomplete.path(), "provenance.license");

    let derived = FixtureRoot::new();
    let version = derived.write_valid_fixture();
    let manifest = valid_manifest().replace("\"kind\": \"synthetic\"", "\"kind\": \"derived\"");
    fs::write(version.join("fixture.json"), manifest).expect("manifest must be changed");
    assert_error(derived.path(), "provenance.parents");
}

#[test]
fn rejects_unsafe_payload_paths() {
    let root = FixtureRoot::new();
    let version = root.write_valid_fixture();
    let manifest =
        valid_manifest().replace("\"path\": \"hello.txt\"", "\"path\": \"../hello.txt\"");
    fs::write(version.join("fixture.json"), manifest).expect("manifest must be changed");

    assert_error(root.path(), "payload.path");

    let unnormalized = FixtureRoot::new();
    let version = unnormalized.write_valid_fixture();
    let manifest = valid_manifest().replace("\"path\": \"hello.txt\"", "\"path\": \"a//b\"");
    fs::write(version.join("fixture.json"), manifest).expect("manifest must be changed");
    assert_error(unnormalized.path(), "payload.path");
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_payloads() {
    use std::os::unix::fs::symlink;

    let root = FixtureRoot::new();
    let version = root.write_valid_fixture();
    fs::remove_file(version.join("hello.txt")).expect("payload must be removed");
    let external = root.path().join("external.txt");
    fs::write(&external, "hello\n").expect("external payload must be written");
    symlink(external, version.join("hello.txt")).expect("symlink must be created");

    assert_error(root.path(), "payload.symlink");
}
