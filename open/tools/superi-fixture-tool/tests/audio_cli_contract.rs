use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TemporaryOutput(PathBuf);

impl TemporaryOutput {
    fn new() -> Self {
        let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("superi-audio-cli-{}-{suffix}", std::process::id()));
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

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_superi-fixture-tool"))
}

#[test]
fn generate_audio_command_reports_success_and_refuses_overwrite() {
    let output = TemporaryOutput::new();

    let generated = command()
        .arg("generate-audio")
        .arg(output.path())
        .output()
        .expect("generator command must run");
    assert!(generated.status.success());
    assert_eq!(
        String::from_utf8(generated.stdout).expect("stdout must be UTF-8"),
        "generated 3 audio cases\n"
    );
    assert!(output.path().join("fixture.json").is_file());

    let repeated = command()
        .arg("generate-audio")
        .arg(output.path())
        .output()
        .expect("repeated command must run");
    assert_eq!(repeated.status.code(), Some(1));
    assert!(String::from_utf8(repeated.stderr)
        .expect("stderr must be UTF-8")
        .contains("already exists"));
}

#[test]
fn invalid_generate_audio_arguments_print_complete_usage() {
    let output = command()
        .arg("generate-audio")
        .output()
        .expect("invalid command must run");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr must be UTF-8"),
        "usage:\n  superi-fixture-tool check [FIXTURE_ROOT]\n  superi-fixture-tool generate-video <OUTPUT_DIRECTORY>\n  superi-fixture-tool generate-audio <OUTPUT_DIRECTORY>\n  superi-fixture-tool generate-timing <OUTPUT_DIRECTORY>\n"
    );
}
