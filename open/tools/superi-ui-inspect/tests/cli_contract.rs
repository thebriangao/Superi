use std::fs;
use std::process::Command;

#[test]
fn private_controller_exposes_the_complete_foundation_command_set() {
    let output = Command::new(env!("CARGO_BIN_EXE_superi-ui-inspect"))
        .arg("--help")
        .output()
        .expect("run private controller help");
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).expect("UTF-8 help");
    for command in [
        "render", "inspect", "click", "key", "type", "crop", "compare",
    ] {
        assert!(help.contains(command), "help omits `{command}`");
    }
}

#[test]
fn render_emits_pixels_semantics_transcript_manifest_and_state() {
    let temporary = tempfile::tempdir().expect("temporary capture root");
    let output = Command::new(env!("CARGO_BIN_EXE_superi-ui-inspect"))
        .args([
            "render",
            "--output",
            temporary.path().to_str().expect("UTF-8 path"),
            "--width",
            "960",
            "--height",
            "640",
        ])
        .output()
        .expect("run private capture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for artifact in [
        "surface.png",
        "semantics.json",
        "transcript.json",
        "manifest.json",
        "state.json",
    ] {
        assert!(
            temporary.path().join(artifact).is_file(),
            "{artifact} missing"
        );
    }
}

#[test]
fn click_records_and_reloads_a_retained_interaction() {
    let temporary = tempfile::tempdir().expect("temporary capture root");
    let baseline = temporary.path().join("baseline");
    let selected = temporary.path().join("selected");
    let render = Command::new(env!("CARGO_BIN_EXE_superi-ui-inspect"))
        .args([
            "render",
            "--output",
            baseline.to_str().expect("UTF-8 baseline path"),
        ])
        .output()
        .expect("render baseline");
    assert!(
        render.status.success(),
        "{}",
        String::from_utf8_lossy(&render.stderr)
    );

    let click = Command::new(env!("CARGO_BIN_EXE_superi-ui-inspect"))
        .args([
            "click",
            "--output",
            selected.to_str().expect("UTF-8 selected path"),
            "--node",
            "foundation.semantics",
            "--state",
            baseline
                .join("state.json")
                .to_str()
                .expect("UTF-8 state path"),
        ])
        .output()
        .expect("activate semantic probe");
    assert!(
        click.status.success(),
        "{}",
        String::from_utf8_lossy(&click.stderr)
    );

    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(selected.join("state.json")).expect("selected state"))
            .expect("valid selected state");
    assert_eq!(state["state"]["selected_probe"], "semantics");
    assert_eq!(state["transcript"][0]["event"]["kind"], "activate");
    assert_eq!(
        state["transcript"][0]["event"]["value"],
        "foundation.semantics"
    );
}
