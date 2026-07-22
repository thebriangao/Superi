use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use superi_desktop::file_associations::{
    project_paths_from_arguments, project_paths_from_urls, DesktopProjectAssociationState,
    ProjectFileOpenSource,
};
use tauri::Url;

struct TestRoot(PathBuf);

static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(0);

impl TestRoot {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock must follow the Unix epoch")
            .as_nanos();
        let ordinal = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "superi-file-associations-{}-{nonce}-{ordinal}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("test root must be creatable");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn startup_arguments_and_file_urls_select_only_unique_superi_projects() {
    let root = TestRoot::new();
    let first = root.path().join("first.superi");
    let second = root.path().join("SECOND.SUPERI");
    let unrelated = root.path().join("notes.txt");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    std::fs::write(&unrelated, b"notes").unwrap();

    let arguments = project_paths_from_arguments(
        [
            OsString::from("--inspect"),
            OsString::from("first.superi"),
            first.as_os_str().to_owned(),
            OsString::from("SECOND.SUPERI"),
            OsString::from("notes.txt"),
        ],
        root.path(),
    );
    assert_eq!(
        arguments,
        vec![
            std::fs::canonicalize(&first).unwrap(),
            std::fs::canonicalize(&second).unwrap(),
        ]
    );

    let urls = project_paths_from_urls([
        Url::from_file_path(&first).unwrap(),
        Url::parse("https://example.com/not-local.superi").unwrap(),
        Url::from_file_path(&unrelated).unwrap(),
        Url::from_file_path(&second).unwrap(),
        Url::from_file_path(&first).unwrap(),
    ]);
    assert_eq!(
        urls,
        vec![
            std::fs::canonicalize(&first).unwrap(),
            std::fs::canonicalize(&second).unwrap(),
        ]
    );
}

#[test]
fn association_requests_are_bounded_ordered_and_explicitly_resolved() {
    let root = TestRoot::new();
    let first = root.path().join("first.superi");
    let second = root.path().join("second.superi");
    let state = DesktopProjectAssociationState::default();

    let requests = state
        .enqueue(
            vec![first.clone(), second.clone(), first.clone()],
            ProjectFileOpenSource::OperatingSystem,
        )
        .unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].sequence(), 1);
    assert_eq!(requests[1].sequence(), 2);
    assert_eq!(requests[0].source(), ProjectFileOpenSource::OperatingSystem);
    assert_eq!(requests[0].path(), first.to_string_lossy());
    assert_eq!(state.pending().unwrap(), requests);

    assert!(state.resolve(1).unwrap());
    assert!(!state.resolve(1).unwrap());
    assert_eq!(state.pending().unwrap(), &requests[1..]);

    let overflow = DesktopProjectAssociationState::default();
    let paths = (0..33)
        .map(|index| root.path().join(format!("overflow-{index}.superi")))
        .collect();
    let failure = overflow
        .enqueue(paths, ProjectFileOpenSource::StartupArgument)
        .unwrap_err();
    assert_eq!(failure.code(), "project_open_queue_full");
    assert!(overflow.pending().unwrap().is_empty());
}
