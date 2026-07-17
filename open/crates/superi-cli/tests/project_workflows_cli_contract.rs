use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

const PROJECT_ID: &str = "project:000000000000000000000000c0230101";
const ROOT_ID: &str = "timeline:000000000000000000000000c0230102";

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-cli-workflows-{}-{}",
            std::process::id(),
            NEXT_PATH.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn stable_workflow_families_publish_before_return_and_reopen_cleanly() {
    let directory = TempDirectory::new();
    let project = directory.join("active.superi");
    let copy = directory.join("copy.superi");
    let backup = directory.join("backup.superi");
    let recovery_root = directory.join("recovery");
    fs::create_dir(&recovery_root).unwrap();

    let create_request = directory.join("create.json");
    write_json(
        &create_request,
        &json!({
            "project_id": PROJECT_ID,
            "project_name": "CLI durable project",
            "root_timeline_id": ROOT_ID,
            "root_timeline_name": "CLI durable timeline",
            "edit_rate_numerator": 24,
            "edit_rate_denominator": 1
        }),
    );
    let created = one_success(&[
        os("project"),
        os("create"),
        os("--project"),
        os(&project),
        os("--request"),
        os(&create_request),
    ]);
    assert_eq!(created["project_id"], PROJECT_ID);
    assert_eq!(created["project_revision"], 0);

    let inspected = one_success(&[os("project"), os("inspect"), os("--project"), os(&project)]);
    assert_eq!(inspected["transaction_id"], "cli-project-inspect");
    assert_eq!(inspected["snapshot"]["project"]["project_revision"], 0);

    let generic_request = directory.join("generic.json");
    write_json(
        &generic_request,
        &json!({
            "transaction_id": "cli-project-command",
            "expected_project_revision": 0,
            "command": {"command": "inspect"}
        }),
    );
    let generic = one_success(&[
        os("project"),
        os("execute"),
        os("--project"),
        os(&project),
        os("--request"),
        os(&generic_request),
    ]);
    assert_eq!(generic["result"]["transaction_id"], "cli-project-command");
    assert_eq!(generic["result"]["authored_state_changed"], false);
    assert_eq!(generic["result"]["command_log_sequence"], 1);
    assert_eq!(generic["events"].as_array().unwrap().len(), 1);

    let timeline_request = directory.join("timeline.json");
    write_json(
        &timeline_request,
        &json!({
            "transaction_id": "cli-timeline",
            "expected_project_revision": 0,
            "command": {
                "command": "apply",
                "actions": [{
                    "action": "select_root_timeline",
                    "timeline_id": ROOT_ID
                }]
            }
        }),
    );
    let timeline = one_success(&[
        os("timeline"),
        os("execute"),
        os("--project"),
        os(&project),
        os("--request"),
        os(&timeline_request),
    ]);
    assert_eq!(timeline["result"]["authored_state_changed"], false);
    assert_eq!(timeline["result"]["command_log_sequence"], 2);
    assert_eq!(timeline["events"].as_array().unwrap().len(), 1);

    let command_log = one_success(&[
        os("project"),
        os("command-log"),
        os("--project"),
        os(&project),
        os("--after-sequence"),
        os("0"),
        os("--limit"),
        os("16"),
        os("--detail"),
        os("replayable"),
    ]);
    assert_eq!(command_log["status"], "records");
    assert_eq!(command_log["result"]["latest_sequence"], 2);
    assert_eq!(
        command_log["result"]["records"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        command_log["result"]["records"][0]["replay_request"]["transaction_id"],
        "cli-project-command"
    );

    let media = run(&[
        os("media"),
        os("execute"),
        os("--project"),
        os(&project),
        os("--request"),
        os(&timeline_request),
    ]);
    assert_eq!(media.status.code(), Some(4));
    let failure: Value = serde_json::from_slice(&media.stderr).unwrap();
    assert_eq!(failure["category"], "invalid_input");
    assert_eq!(failure["stage_id"], "media.execute");

    let render_request = directory.join("render.json");
    write_json(
        &render_request,
        &json!({
            "transaction_id": "cli-render",
            "expected_revision": 0,
            "mutations": [{
                "operation": "set",
                "key": "superi.project.render.color_target",
                "value": {"kind": "text", "value": "delivery"}
            }]
        }),
    );
    let render = one_success(&[
        os("render"),
        os("configure"),
        os("--project"),
        os(&project),
        os("--request"),
        os(&render_request),
    ]);
    assert_eq!(render["result"]["snapshot"]["project_revision"], 1);
    assert_eq!(render["events"].as_array().unwrap().len(), 1);

    let render = one_success(&[os("render"), os("inspect"), os("--project"), os(&project)]);
    assert_eq!(
        render["editor"]["snapshot"]["project"]["project_revision"],
        1
    );
    assert_eq!(render["settings"]["snapshot"]["project_revision"], 1);
    assert_eq!(
        render["settings"]["snapshot"]["values"]["superi.project.render.color_target"]["value"],
        "delivery"
    );

    let editor = one_success(&[os("inspect"), os("editor"), os("--project"), os(&project)]);
    assert_eq!(editor["snapshot"]["project"]["project_revision"], 1);

    let automation = directory.join("automation.jsonl");
    let script_source = serde_json::to_string(&json!({
        "language": "superi-json",
        "language_version": "1.0.0",
        "script_id": "cli.automation.script",
        "expected_initial_project_revision": 2,
        "steps": [{
            "method": "superi.editor.state.get",
            "params": {"transaction_id": "cli-automation-script-editor"}
        }]
    }))
    .unwrap();
    let script_digest = format!("{:x}", Sha256::digest(script_source.as_bytes()));
    fs::write(
        &automation,
        format!(
            "{}\n{}\n{}\n{}\n",
            json!({
                "jsonrpc": "2.0",
                "id": "automation-render-id",
                "method": "superi.project.settings.transaction.execute",
                "params": {
                    "transaction_id": "automation-render",
                    "expected_revision": 1,
                    "mutations": [{
                        "operation": "set",
                        "key": "superi.project.render.color_target",
                        "value": {"kind": "text", "value": "display"}
                    }]
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 23,
                "method": "superi.editor.state.get",
                "params": {"transaction_id": "automation-editor"}
            }),
            json!({
                "jsonrpc": "2.0",
                "id": "automation-script-id",
                "method": "superi.project.script.run",
                "params": {
                    "source": script_source,
                    "expected_source_sha256": script_digest
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": "automation-command-log-id",
                "method": "superi.project.command_log.get",
                "params": {
                    "after_sequence": 0,
                    "requested_limit": 16,
                    "detail": "metadata"
                }
            })
        ),
    )
    .unwrap();
    let automation = success_lines(&[
        os("automation"),
        os("run"),
        os("--project"),
        os(&project),
        os("--input"),
        os(&automation),
    ]);
    assert_eq!(automation.len(), 4);
    assert_eq!(automation[0]["jsonrpc"], "2.0");
    assert_eq!(automation[0]["id"], "automation-render-id");
    assert_eq!(
        automation[0]["result"]["result"]["snapshot"]["project_revision"],
        2
    );
    assert_eq!(automation[1]["jsonrpc"], "2.0");
    assert_eq!(automation[1]["id"], 23);
    assert_eq!(
        automation[1]["result"]["snapshot"]["project"]["project_revision"],
        2
    );
    assert_eq!(automation[2]["jsonrpc"], "2.0");
    assert_eq!(automation[2]["id"], "automation-script-id");
    assert_eq!(automation[2]["result"]["result"]["status"], "completed");
    assert_eq!(
        automation[2]["result"]["result"]["final_project_revision"],
        2
    );
    assert!(automation[2]["result"]["events"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(automation[3]["id"], "automation-command-log-id");
    assert_eq!(automation[3]["result"]["status"], "records");
    assert_eq!(automation[3]["result"]["result"]["latest_sequence"], 2);

    let failing_automation = directory.join("failing-automation.jsonl");
    fs::write(
        &failing_automation,
        format!(
            "{}\n{}\n",
            json!({
                "jsonrpc": "2.0",
                "id": "acknowledged-before-failure",
                "method": "superi.project.settings.get",
                "params": {}
            }),
            json!({
                "jsonrpc": "2.0",
                "id": "stale-mutation",
                "method": "superi.project.settings.transaction.execute",
                "params": {
                    "transaction_id": "stale-automation",
                    "expected_revision": 0,
                    "mutations": [{
                        "operation": "set",
                        "key": "superi.project.render.color_target",
                        "value": {"kind": "text", "value": "project_working"}
                    }]
                }
            })
        ),
    )
    .unwrap();
    let failure = run(&[
        os("automation"),
        os("run"),
        os("--project"),
        os(&project),
        os("--input"),
        os(&failing_automation),
    ]);
    assert_eq!(failure.status.code(), Some(4));
    let acknowledged = String::from_utf8(failure.stdout).unwrap();
    let acknowledged = acknowledged.lines().collect::<Vec<_>>();
    assert_eq!(acknowledged.len(), 1);
    let acknowledged: Value = serde_json::from_str(acknowledged[0]).unwrap();
    assert_eq!(acknowledged["id"], "acknowledged-before-failure");
    let failure_body: Value = serde_json::from_slice(&failure.stderr).unwrap();
    assert_eq!(failure_body["category"], "conflict");
    assert!(failure_body["message"]
        .as_str()
        .unwrap()
        .contains("stopped after 1 durable request(s)"));

    let validation = one_success(&[os("validate"), os("project"), os("--project"), os(&project)]);
    assert_eq!(validation["valid"], true);
    assert_eq!(validation["project"]["project_revision"], 2);

    let protected_media_request = directory.join("protected-media.json");
    write_json(
        &protected_media_request,
        &json!({
            "transaction_id": "protected-media",
            "expected_project_revision": 2,
            "command": {
                "command": "apply",
                "actions": [{
                    "action": "mutate_media",
                    "mutation": {
                        "operation": "set_path",
                        "media_id": "media:000000000000000000000000c023ffff",
                        "path": {
                            "kind": "absolute",
                            "platform": "unix",
                            "path": "/private/super-secret-media.mov"
                        }
                    }
                }]
            }
        }),
    );
    let denied = run(&[
        os("media"),
        os("execute"),
        os("--project"),
        os(&project),
        os("--request"),
        os(&protected_media_request),
    ]);
    assert_eq!(denied.status.code(), Some(4));
    let denied_text = String::from_utf8(denied.stderr).unwrap();
    assert!(!denied_text.contains("super-secret-media"));
    let denied: Value = serde_json::from_str(&denied_text).unwrap();
    assert_eq!(denied["category"], "permission_denied");
    assert!(denied["contexts"].is_array());

    let copy_result = one_success(&[
        os("project"),
        os("save-copy"),
        os("--project"),
        os(&project),
        os("--destination"),
        os(&copy),
        os("--collision"),
        os("require-absent"),
    ]);
    assert_eq!(copy_result["operation"], "save_copy");
    let backup_result = one_success(&[
        os("project"),
        os("backup"),
        os("--project"),
        os(&project),
        os("--destination"),
        os(&backup),
    ]);
    assert_eq!(backup_result["operation"], "backup");
    for saved in [&copy, &backup] {
        let saved = one_success(&[os("validate"), os("project"), os("--project"), os(saved)]);
        assert_eq!(saved["project"]["project_revision"], 2);
    }

    let recovery_request = directory.join("recovery.json");
    write_json(
        &recovery_request,
        &json!({"transaction_id": "cli-recovery"}),
    );
    let recovery = one_success(&[
        os("project"),
        os("recovery"),
        os("get"),
        os("--project"),
        os(&project),
        os("--recovery-root"),
        os(&recovery_root),
        os("--request"),
        os(&recovery_request),
    ]);
    assert_eq!(recovery["result"]["transaction_id"], "cli-recovery");

    let schema = one_success(&[os("inspect"), os("api-schema")]);
    assert_eq!(schema["json_rpc_version"], "2.0");
    let engine = one_success(&[os("validate"), os("engine")]);
    assert_eq!(engine["snapshot"]["coherent"], true);
}

#[test]
fn workflow_parser_rejects_duplicates_unknowns_and_permission_stdin() {
    let directory = TempDirectory::new();
    let project = directory.join("active.superi");
    let duplicate = run(&[
        os("project"),
        os("inspect"),
        os("--project"),
        os(&project),
        os("--project"),
        os(&project),
    ]);
    assert_invalid(&duplicate);

    let unknown = run(&[
        os("render"),
        os("inspect"),
        os("--project"),
        os(&project),
        os("--mystery"),
        os("value"),
    ]);
    assert_invalid(&unknown);

    let request = directory.join("request.json");
    write_json(
        &request,
        &json!({
            "transaction_id": "permission-stdin",
            "expected_project_revision": 0,
            "command": {"command": "inspect"}
        }),
    );
    let permission_stdin = run(&[
        os("project"),
        os("execute"),
        os("--project"),
        os(&project),
        os("--request"),
        os(&request),
        os("--permissions"),
        os("-"),
    ]);
    assert_invalid(&permission_stdin);
}

fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

fn os(value: impl AsRef<OsStr>) -> OsString {
    value.as_ref().to_owned()
}

fn run(arguments: &[OsString]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_superi-cli"))
        .args(arguments)
        .output()
        .unwrap()
}

fn one_success(arguments: &[OsString]) -> Value {
    let mut values = success_lines(arguments);
    assert_eq!(values.len(), 1);
    values.remove(0)
}

fn success_lines(arguments: &[OsString]) -> Vec<Value> {
    let output = run(arguments);
    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn assert_invalid(output: &Output) {
    assert_eq!(output.status.code(), Some(2));
    let failure: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(failure["category"], "invalid_input");
}
