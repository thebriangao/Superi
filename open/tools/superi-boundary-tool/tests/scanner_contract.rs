use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use superi_boundary_tool::scan_open_tree;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct OpenTree(PathBuf);

impl OpenTree {
    fn new() -> Self {
        let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "superi-boundary-policy-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).expect("temporary open tree must be created");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"clean\"\nversion = \"0.0.0\"\n",
        )
        .expect("manifest must be written");
        fs::write(root.join("src/lib.rs"), "pub fn local_only() {}\n")
            .expect("source must be written");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory must be created");
        }
        fs::write(path, contents).expect("test file must be written");
    }
}

impl Drop for OpenTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_violation(root: &Path, code: &str) {
    let violations = scan_open_tree(root).expect_err("open tree must be rejected");
    assert!(
        violations.iter().any(|violation| violation.code() == code),
        "expected {code}, got {violations:?}"
    );
}

#[test]
fn canonical_open_tree_is_part_of_the_workspace_test_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report = scan_open_tree(&root).unwrap_or_else(|violations| {
        panic!(
            "canonical open tree at {} violates repository policy:\n{violations}",
            root.display()
        )
    });

    assert!(report.files_scanned() > 0);
    assert!(report.manifests_scanned() > 0);
}

#[test]
fn cross_platform_ci_runs_the_locked_boundary_command_in_every_build_job() {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow = fs::read_to_string(repository_root.join(".github/workflows/ci.yml"))
        .expect("cross-platform CI workflow must be readable");
    let command = "        run: cargo run --locked -p superi-boundary-tool -- check .";
    let command_count = workflow.lines().filter(|line| *line == command).count();
    let build_count = workflow
        .lines()
        .filter(|line| *line == "        run: cargo build --workspace --locked")
        .count();

    assert_eq!(
        build_count, 2,
        "every declared build job must remain visible"
    );
    assert_eq!(
        command_count, build_count,
        "every cross-platform build job must enforce the open boundary"
    );
}

#[test]
fn rejects_direct_and_renamed_network_client_dependencies() {
    let direct = OpenTree::new();
    direct.write(
        "Cargo.toml",
        "[package]\nname = \"bad\"\nversion = \"0.0.0\"\n[dependencies]\nreqwest = \"0.12\"\n",
    );
    assert_violation(direct.path(), "network.dependency");

    let renamed = OpenTree::new();
    renamed.write(
        "Cargo.toml",
        "[package]\nname = \"bad\"\nversion = \"0.0.0\"\n[target.'cfg(unix)'.dependencies]\ntransport = { package = \"tokio-tungstenite\", version = \"0.24\" }\n",
    );
    assert_violation(renamed.path(), "network.dependency");
}

#[test]
fn rejects_network_apis_in_source_and_build_scripts() {
    let source = OpenTree::new();
    source.write("src/lib.rs", "use std::net::TcpStream;\n");
    assert_violation(source.path(), "network.api");

    let build_script = OpenTree::new();
    build_script.write(
        "build.rs",
        "fn main() { let _ = std::net::UdpSocket::bind(\"0.0.0.0:0\"); }\n",
    );
    assert_violation(build_script.path(), "network.api");

    let nested_use = OpenTree::new();
    nested_use.write(
        "src/lib.rs",
        "use std::{net::TcpStream, path::Path};\nfn borrow<'a>(value: &'a Path) { let _ = TcpStream::connect(\"localhost:9\"); let _ = value; }\n",
    );
    assert_violation(nested_use.path(), "network.api");

    let after_lifetime = OpenTree::new();
    after_lifetime.write(
        "src/lib.rs",
        "fn borrow<'a>(value: &'a str) { let _ = value; let _ = std::net::TcpListener::bind(\"localhost:0\"); }\n",
    );
    assert_violation(after_lifetime.path(), "network.api");
}

#[test]
fn rejects_closed_tree_manifest_paths_and_source_includes() {
    let dependency = OpenTree::new();
    dependency.write(
        "Cargo.toml",
        "[package]\nname = \"bad\"\nversion = \"0.0.0\"\n[dependencies]\nmax = { path = \"../../closed/max\" }\n",
    );
    assert_violation(dependency.path(), "closed.reference");

    let include = OpenTree::new();
    include.write(
        "src/lib.rs",
        "const PRIVATE: &[u8] = include_bytes!(\"../../../closed/model.bin\");\n",
    );
    assert_violation(include.path(), "closed.reference");

    let raw_byte_include = OpenTree::new();
    raw_byte_include.write(
        "src/lib.rs",
        "const NOTE: &[u8] = br#\"raw bytes\"#;\nconst PRIVATE: &[u8] = include_bytes!(r#\"../../../closed/model.bin\"#);\n",
    );
    assert_violation(raw_byte_include.path(), "closed.reference");

    let renamed_package = OpenTree::new();
    renamed_package.write(
        "Cargo.toml",
        "[package]\nname = \"bad\"\nversion = \"0.0.0\"\n[dependencies.max_client]\npackage = \"superi-max-client\"\nversion = \"1\"\n",
    );
    assert_violation(renamed_package.path(), "closed.reference");
}

#[test]
fn ignores_comments_and_strings_that_only_explain_the_policy() {
    let root = OpenTree::new();
    root.write(
        "src/lib.rs",
        "// Never call std::net::TcpStream here.\npub const RULE: &str = \"open must not import closed/\";\n",
    );

    scan_open_tree(root.path()).expect("policy prose must not be treated as executable code");
}

#[cfg(unix)]
#[test]
fn rejects_symlinks_in_the_open_tree() {
    use std::os::unix::fs::symlink;

    let root = OpenTree::new();
    let outside = root.path().parent().unwrap().join(format!(
        "superi-boundary-outside-{}",
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&outside, "outside\n").expect("external file must be written");
    symlink(&outside, root.path().join("src/linked.rs")).expect("symlink must be created");

    assert_violation(root.path(), "path.symlink");
    fs::remove_file(outside).expect("external file must be removed");
}

#[test]
fn command_line_reports_success_and_policy_failures() {
    let clean = OpenTree::new();
    let success = Command::new(env!("CARGO_BIN_EXE_superi-boundary-tool"))
        .arg("check")
        .arg(clean.path())
        .output()
        .expect("boundary command must run");
    assert!(success.status.success());
    assert!(String::from_utf8_lossy(&success.stdout).contains("validated 2 files"));

    let rejected = OpenTree::new();
    rejected.write("src/lib.rs", "use std::net::TcpStream;\n");
    let failure = Command::new(env!("CARGO_BIN_EXE_superi-boundary-tool"))
        .arg("check")
        .arg(rejected.path())
        .output()
        .expect("boundary command must run");
    assert_eq!(failure.status.code(), Some(1));
    let expected_path = Path::new("src").join("lib.rs");
    let expected_diagnostic = format!("network.api: {}:1:", expected_path.display());
    assert!(String::from_utf8_lossy(&failure.stderr).contains(&expected_diagnostic));
}
