use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use superi_desktop::file_associations::{
    open_project_file, project_paths_from_arguments, project_paths_from_urls,
};
use superi_desktop::project_lifecycle::{
    DesktopProjectCommand, DesktopProjectCreateRequest, DesktopProjectState,
};
use tauri::Url;

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock must follow the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "superi-file-associations-{}-{nonce}",
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

fn create_request(seed: &str) -> DesktopProjectCreateRequest {
    DesktopProjectCreateRequest {
        project_id: "project:00000000000000000000000000000603".into(),
        project_name: format!("Project {seed}"),
        root_timeline_id: "timeline:00000000000000000000000000010603".into(),
        root_timeline_name: format!("Timeline {seed}"),
        edit_rate_numerator: 24,
        edit_rate_denominator: 1,
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
fn association_open_uses_the_durable_owner_and_retains_state_on_failure() {
    let root = TestRoot::new();
    let project = root.path().join("owned.superi");
    let state = DesktopProjectState::default();
    state.initialize(root.path().join("recovery")).unwrap();

    state
        .execute(DesktopProjectCommand::Create {
            path: project.to_string_lossy().into_owned(),
            project: create_request("owned"),
        })
        .unwrap();
    state.execute(DesktopProjectCommand::Close).unwrap();

    let opened = open_project_file(&state, &project).unwrap();
    let active = opened
        .active()
        .expect("association must activate the project");
    assert_eq!(
        active.project_id(),
        "project:00000000000000000000000000000603"
    );
    assert_eq!(active.path(), project.to_string_lossy());
    assert_eq!(opened.recent().len(), 1);
    assert!(opened.failure().is_none());

    let missing = root.path().join("missing.superi");
    assert!(open_project_file(&state, &missing).is_err());
    let retained = state.snapshot().unwrap();
    assert_eq!(
        retained
            .active()
            .expect("valid project must survive")
            .project_id(),
        "project:00000000000000000000000000000603"
    );
    assert_eq!(retained.recent(), opened.recent());
    assert!(retained.failure().is_some());
}
