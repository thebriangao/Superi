use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use superi_api_bindings::{
    canonical_bindings_path, check_path, generate_path, render, CheckStatus, GenerateStatus,
};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock must follow the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "superi-api-bindings-{label}-{}-{nonce}",
        std::process::id()
    ))
}

#[test]
fn generation_is_deterministic_and_idempotent() {
    let directory = temporary_directory("generate");
    let path = directory.join("bindings.ts");

    assert_eq!(generate_path(&path).unwrap(), GenerateStatus::Written);
    let first = fs::read(&path).unwrap();
    assert_eq!(generate_path(&path).unwrap(), GenerateStatus::Unchanged);
    assert_eq!(fs::read(&path).unwrap(), first);
    assert_eq!(first, render().unwrap().as_bytes());

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn check_reports_drift_without_mutating_the_file() {
    let directory = temporary_directory("check");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("bindings.ts");
    fs::write(&path, b"stale bindings\n").unwrap();
    let before = fs::read(&path).unwrap();

    assert_eq!(check_path(&path).unwrap(), CheckStatus::Stale);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(
        check_path(&directory.join("missing.ts")).unwrap(),
        CheckStatus::Missing
    );

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn committed_bindings_match_fresh_output() {
    assert_eq!(
        check_path(&canonical_bindings_path()).unwrap(),
        CheckStatus::Current
    );
}
