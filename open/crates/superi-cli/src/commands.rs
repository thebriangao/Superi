//! Precise headless command surface for the canonical editorial slice.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::instrumentation::{
    InstrumentationSummary, ProcessMemorySampler, StageInstrumentation, StageProbe,
};
use crate::project_workflows::{parse_workflow_command, run_workflow, WorkflowCommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use superi_api::commands::{ExecuteScenarioTransaction, GetEngineIntegrationValidation};
use superi_api::permissions::{
    ApiFilesystemAccess, ApiFilesystemPath, ApiFilesystemScope, ApiPermissionContext,
    ApiPermissionEffect, ApiPermissionRule,
};
use superi_api::scenario::{ExactFrameRate, ScenarioApi, SliceAction, SliceGraphEffect};
use superi_api::schema::{GetPublicApiSchema, PublicApiSchemaApi, PublicApiSchemaSnapshot};
use superi_api::validation::IntegrationValidationApi;

use crate::expectations::{resolve_expectations, ContractObservations, ExpectationFailureKind};

const SCENARIO_ID: &str = "superi.slice.canonical.v1";
const SCENARIO_REVISION: u32 = 1;
const FIXTURE_ID: &str = "slice/video-cfr";
const FIXTURE_VERSION: u32 = 1;
const PORTABLE_FIXTURE_PATH: &str = "open/test-fixtures/slice/video-cfr/v1/input.webm";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
const EXIT_INVALID_INPUT: i32 = 2;
const EXIT_UNAVAILABLE: i32 = 3;
const EXIT_STAGE_FAILURE: i32 = 4;
const USAGE: &str = "Usage:\n  superi-cli api schema\n  superi-cli project create --project <PROJECT> --request <JSON_OR_->\n  superi-cli project execute --project <PROJECT> --request <JSON_OR_-> [--permissions <JSON>]\n  superi-cli project inspect --project <PROJECT>\n  superi-cli project save-copy --project <PROJECT> --destination <PROJECT> --collision <require-absent|replace-existing>\n  superi-cli project backup --project <PROJECT> --destination <PROJECT>\n  superi-cli project recovery <get|compare|restore|dismiss> --project <PROJECT> --recovery-root <DIRECTORY> --request <JSON_OR_-> [--permissions <JSON>]\n  superi-cli media execute --project <PROJECT> --request <JSON_OR_-> [--permissions <JSON>]\n  superi-cli timeline execute --project <PROJECT> --request <JSON_OR_-> [--permissions <JSON>]\n  superi-cli render inspect --project <PROJECT>\n  superi-cli render configure --project <PROJECT> --request <JSON_OR_->\n  superi-cli inspect <editor|api-schema> [--project <PROJECT>]\n  superi-cli validate <project|engine> [--project <PROJECT>]\n  superi-cli automation run --project <PROJECT> --input <JSONL_OR_-> [--permissions <JSON>]\n  superi-cli slice run --scenario superi.slice.canonical.v1 --artifact-dir <EMPTY_DIRECTORY> --report <REPORT_JSON>\n  superi-cli engine validate\n  superi-cli --help\n  superi-cli --version\n";
const STUB_ARTIFACT_NAME: &str = "canonical.webm.contract-stub";
static TEMPORARY_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

enum Command {
    ApiSchema,
    RunSlice {
        artifact_dir: PathBuf,
        report: PathBuf,
    },
    ValidateEngine,
    Workflow(WorkflowCommand),
    Help,
    Version,
}

pub(crate) struct CliFailure {
    exit: i32,
    category: &'static str,
    recoverability: &'static str,
    stage_id: Option<&'static str>,
    message: String,
    contexts: Vec<CliFailureContext>,
}

#[derive(Serialize)]
struct CliFailureContext {
    component: String,
    operation: String,
    fields: BTreeMap<String, String>,
}

impl CliFailure {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self {
            exit: EXIT_INVALID_INPUT,
            category: "invalid_input",
            recoverability: "user_correctable",
            stage_id: None,
            message: message.into(),
            contexts: Vec::new(),
        }
    }

    pub(crate) fn unavailable(stage_id: &'static str, message: impl Into<String>) -> Self {
        Self {
            exit: EXIT_UNAVAILABLE,
            category: "unavailable",
            recoverability: "user_correctable",
            stage_id: Some(stage_id),
            message: message.into(),
            contexts: Vec::new(),
        }
    }

    pub(crate) fn stage(
        stage_id: &'static str,
        category: &'static str,
        recoverability: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            exit: EXIT_STAGE_FAILURE,
            category,
            recoverability,
            stage_id: Some(stage_id),
            message: message.into(),
            contexts: Vec::new(),
        }
    }

    pub(crate) fn from_error(stage_id: &'static str, error: superi_core::error::Error) -> Self {
        let contexts = safe_error_contexts(error.contexts());
        let mut failure = if error.category() == superi_core::error::ErrorCategory::Unavailable {
            Self {
                exit: EXIT_UNAVAILABLE,
                category: error.category().code(),
                recoverability: error.recoverability().code(),
                stage_id: Some(stage_id),
                message: error.message().to_owned(),
                contexts: Vec::new(),
            }
        } else {
            Self::stage(
                stage_id,
                error.category().code(),
                error.recoverability().code(),
                error.message(),
            )
        };
        failure.contexts = contexts;
        failure
    }

    pub(crate) fn contextualize(mut self, context: impl AsRef<str>) -> Self {
        self.message = format!("{}: {}", context.as_ref(), self.message);
        self
    }
}

fn safe_error_contexts(contexts: &[superi_core::error::ErrorContext]) -> Vec<CliFailureContext> {
    contexts
        .iter()
        .rev()
        .take(8)
        .map(|context| CliFailureContext {
            component: bounded_text(context.component(), 128),
            operation: bounded_text(context.operation(), 128),
            fields: context
                .fields()
                .iter()
                .take(8)
                .map(|(key, value)| {
                    let lowered = key.to_ascii_lowercase();
                    let sensitive = ["path", "target", "payload", "secret", "token"]
                        .iter()
                        .any(|term| lowered.contains(term));
                    (
                        bounded_text(key, 128),
                        if sensitive {
                            "[redacted]".to_owned()
                        } else {
                            bounded_text(value, 256)
                        },
                    )
                })
                .collect(),
        })
        .collect()
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[derive(Serialize)]
struct StageRecord {
    stage_id: &'static str,
    implementation: &'static str,
    owner: &'static str,
    implementation_revision: String,
    input: Value,
    output: Value,
    #[serde(flatten)]
    instrumentation: StageInstrumentation,
    success: bool,
    diagnostics: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureManifest {
    schema_version: u32,
    fixture_id: String,
    fixture_version: u32,
    description: String,
    provenance: FixtureProvenance,
    files: Vec<FixtureFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureProvenance {
    kind: String,
    source: String,
    author: String,
    created_on: String,
    license: String,
    rights: String,
    generator: Option<FixtureGenerator>,
    parents: Vec<FixtureParent>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureGenerator {
    name: String,
    version: String,
    command: String,
    seed: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureParent {
    fixture_id: String,
    fixture_version: u32,
    manifest_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureFile {
    path: String,
    media_type: String,
    bytes: u64,
    sha256: String,
}

struct ResolvedFixture {
    payload_path: PathBuf,
    manifest_sha256: String,
    payload_sha256: String,
    payload_bytes: u64,
}

struct RepositoryState {
    commit: String,
    dirty: bool,
}

pub(crate) fn run(arguments: impl IntoIterator<Item = OsString>) -> i32 {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let command = match parse_command(&arguments) {
        Ok(command) => command,
        Err(failure) => return emit_failure(failure),
    };

    match command {
        Command::ApiSchema => match run_public_api_schema() {
            Ok(snapshot) => {
                println!(
                    "{}",
                    serde_json::to_string(&snapshot).expect("public API schema is serializable")
                );
                0
            }
            Err(failure) => emit_failure(failure),
        },
        Command::Help => {
            print!("{USAGE}");
            0
        }
        Command::Version => {
            println!("superi {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Command::RunSlice {
            artifact_dir,
            report,
        } => match run_slice(&artifact_dir, &report) {
            Ok(summary) => {
                println!(
                    "{}",
                    serde_json::to_string(&summary).expect("summary is serializable")
                );
                0
            }
            Err(failure) => emit_failure(failure),
        },
        Command::ValidateEngine => match run_engine_validation() {
            Ok(report) => {
                println!(
                    "{}",
                    serde_json::to_string(&report).expect("validation report is serializable")
                );
                0
            }
            Err(failure) => emit_failure(failure),
        },
        Command::Workflow(command) => match run_workflow(command) {
            Ok(values) => {
                for value in values {
                    println!(
                        "{}",
                        serde_json::to_string(&value).expect("workflow result is serializable")
                    );
                }
                0
            }
            Err(failure) => emit_failure(failure),
        },
    }
}

fn parse_command(arguments: &[OsString]) -> Result<Command, CliFailure> {
    if let Some(command) = parse_workflow_command(arguments)? {
        return Ok(Command::Workflow(command));
    }
    match arguments {
        [] => Ok(Command::Help),
        [argument] if argument == "--help" || argument == "help" => Ok(Command::Help),
        [argument] if argument == "--version" => Ok(Command::Version),
        [api, schema] if api == "api" && schema == "schema" => Ok(Command::ApiSchema),
        [engine, validate] if engine == "engine" && validate == "validate" => {
            Ok(Command::ValidateEngine)
        }
        [slice, run, scenario_option, scenario, artifact_option, artifact_dir, report_option, report]
            if slice == "slice"
                && run == "run"
                && scenario_option == "--scenario"
                && artifact_option == "--artifact-dir"
                && report_option == "--report" =>
        {
            if scenario != SCENARIO_ID {
                return Err(CliFailure::invalid(format!(
                    "unsupported scenario `{}`; expected `{SCENARIO_ID}`",
                    scenario.to_string_lossy()
                )));
            }
            Ok(Command::RunSlice {
                artifact_dir: PathBuf::from(artifact_dir),
                report: PathBuf::from(report),
            })
        }
        _ => Err(CliFailure::invalid(
            "unrecognized command; run `superi-cli --help` for the stable command surface",
        )),
    }
}

pub(crate) fn run_public_api_schema() -> Result<PublicApiSchemaSnapshot, CliFailure> {
    let api = PublicApiSchemaApi::new().map_err(|_| {
        CliFailure::stage(
            "api.schema",
            "internal",
            "terminal",
            "public API catalog failed validation",
        )
    })?;
    Ok(api.execute(GetPublicApiSchema::new()).into_snapshot())
}

pub(crate) fn run_engine_validation(
) -> Result<superi_api::commands::GetEngineIntegrationValidationResult, CliFailure> {
    let api = IntegrationValidationApi::from_fresh_engine().map_err(|failure| {
        CliFailure::stage(
            "engine.validate",
            failure_category(failure.category()),
            failure_recoverability(failure.recoverability()),
            failure.message().to_owned(),
        )
    })?;
    let result = api.execute(GetEngineIntegrationValidation::new());
    if !result.snapshot().is_coherent() {
        return Err(CliFailure::stage(
            "engine.validate",
            "internal",
            "terminal",
            "engine integration validation reported incoherent state",
        ));
    }
    Ok(result)
}

fn run_slice(artifact_dir: &Path, report_path: &Path) -> Result<Value, CliFailure> {
    validate_output_targets(artifact_dir, report_path)?;
    let repository_root = find_repository_root()?;
    let repository = inspect_repository(&repository_root)?;
    let implementation_revision = if repository.dirty {
        format!("{}+dirty", repository.commit)
    } else {
        repository.commit.clone()
    };
    let toolchain = command_text("rustc", &["--version"]).map_err(|message| {
        CliFailure::unavailable(
            "fixture.resolve",
            format!("Rust toolchain unavailable: {message}"),
        )
    })?;

    let mut memory_sampler = ProcessMemorySampler::new().map_err(|message| {
        CliFailure::unavailable(
            "fixture.resolve",
            format!("memory instrumentation unavailable: {message}"),
        )
    })?;

    let fixture_probe = begin_stage(&mut memory_sampler, "fixture.resolve")?;
    let fixture = resolve_fixture(&repository_root)?;
    let mut stages = vec![StageRecord {
        stage_id: "fixture.resolve",
        implementation: "runtime",
        owner: "superi-cli.fixture-resolver",
        implementation_revision: implementation_revision.clone(),
        input: json!({
            "fixture_id": FIXTURE_ID,
            "fixture_version": FIXTURE_VERSION,
            "manifest_path": "open/test-fixtures/slice/video-cfr/v1/fixture.json"
        }),
        output: json!({
            "manifest_sha256": fixture.manifest_sha256,
            "payload_sha256": fixture.payload_sha256,
            "payload_bytes": fixture.payload_bytes,
            "payload_path": "open/test-fixtures/slice/video-cfr/v1/input.webm",
            "validation": "manifest_identity_and_payload_digest"
        }),
        instrumentation: finish_stage(fixture_probe, &mut memory_sampler, "fixture.resolve")?,
        success: true,
        diagnostics: Vec::new(),
    }];

    create_artifact_directory(artifact_dir)?;
    let mut api = scenario_api_for_fixture(&fixture.payload_path)?;

    let import_probe = begin_stage(&mut memory_sampler, "media.import")?;
    execute_action(
        &mut api,
        "media.import",
        SliceAction::ImportClip {
            path: fixture.payload_path.display().to_string(),
            fixture_id: FIXTURE_ID.to_owned(),
            fixture_version: FIXTURE_VERSION,
            manifest_sha256: fixture.manifest_sha256.clone(),
            payload_sha256: fixture.payload_sha256.clone(),
            frame_rate: ExactFrameRate::new(24, 1),
            frame_count: 96,
            width: 96,
            height: 54,
        },
    )?;
    stages.push(StageRecord {
        stage_id: "media.import",
        implementation: "stub",
        owner: "superi-engine.command",
        implementation_revision: implementation_revision.clone(),
        input: json!({
            "fixture_id": FIXTURE_ID,
            "payload_sha256": fixture.payload_sha256,
            "expected_backend": "mkv-webm",
            "expected_codec_backend": "rust-av1"
        }),
        output: json!({
            "media_id": api.snapshot().media().expect("import succeeded").media_id(),
            "frame_rate": {"numerator": 24, "denominator": 1},
            "frame_count": 96,
            "extent": {"width": 96, "height": 54}
        }),
        instrumentation: finish_stage(import_probe, &mut memory_sampler, "media.import")?,
        success: true,
        diagnostics: vec![
            "Typed import state is modeled without opening or decoding the payload.".to_owned(),
        ],
    });

    let edit_probe = begin_stage(&mut memory_sampler, "timeline.edit")?;
    execute_action(
        &mut api,
        "timeline.edit",
        SliceAction::PlaceClip {
            timeline_start_frame: 0,
        },
    )?;
    execute_action(
        &mut api,
        "timeline.edit",
        SliceAction::TrimClip {
            source_start_frame: 24,
            source_end_frame: 72,
        },
    )?;
    stages.push(StageRecord {
        stage_id: "timeline.edit",
        implementation: "stub",
        owner: "superi-engine.command",
        implementation_revision: implementation_revision.clone(),
        input: json!({
            "timeline": "canonical",
            "track": "V1",
            "clip": "clip-1",
            "untrimmed_source_range": [0, 96],
            "trimmed_source_range": [24, 72]
        }),
        output: serde_json::to_value(api.snapshot().timeline()).expect("timeline is serializable"),
        instrumentation: finish_stage(edit_probe, &mut memory_sampler, "timeline.edit")?,
        success: true,
        diagnostics: vec![
            "The command engine owns reversible typed state while the production timeline owner is absent."
                .to_owned(),
        ],
    });

    let compile_probe = begin_stage(&mut memory_sampler, "timeline.compile")?;
    let timeline_value =
        serde_json::to_value(api.snapshot().timeline()).expect("timeline state is serializable");
    let timeline_sha256 = digest_json(&timeline_value);
    stages.push(StageRecord {
        stage_id: "timeline.compile",
        implementation: "stub",
        owner: "superi-cli.contract-runner",
        implementation_revision: implementation_revision.clone(),
        input: timeline_value.clone(),
        output: json!({
            "compiled_identity": "canonical/V1/clip-1",
            "timeline_sha256": timeline_sha256,
            "frame_count": 48
        }),
        instrumentation: finish_stage(compile_probe, &mut memory_sampler, "timeline.compile")?,
        success: true,
        diagnostics: vec![
            "Compilation records the exact public timeline contract without a production compiler."
                .to_owned(),
        ],
    });

    let graph_probe = begin_stage(&mut memory_sampler, "graph.evaluate")?;
    let effected = execute_action(
        &mut api,
        "graph.evaluate",
        SliceAction::ApplyGraphEffect {
            effect: SliceGraphEffect::HorizontalMirror,
        },
    )?;
    let original_final_state = effected["state"].clone();
    let graph_value = original_final_state["graph"].clone();
    let graph_sha256 = digest_json(&graph_value);
    stages.push(StageRecord {
        stage_id: "graph.evaluate",
        implementation: "stub",
        owner: "superi-engine.command",
        implementation_revision: implementation_revision.clone(),
        input: json!({
            "effect_type": "superi.effect.transform",
            "matrix": [-1.0, 0.0, 95.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            "sampling": "nearest",
            "edge_mode": "transparent_black"
        }),
        output: graph_value.clone(),
        instrumentation: finish_stage(graph_probe, &mut memory_sampler, "graph.evaluate")?,
        success: true,
        diagnostics: vec![
            "Graph topology and parameters are complete, but no pixels are evaluated.".to_owned(),
        ],
    });

    let color_probe = begin_stage(&mut memory_sampler, "color.deliver")?;
    stages.push(StageRecord {
        stage_id: "color.deliver",
        implementation: "stub",
        owner: "superi-cli.contract-runner",
        implementation_revision: implementation_revision.clone(),
        input: json!({
            "source": {"range": "limited", "matrix": "bt709"},
            "graph_output": {"width": 96, "height": 54}
        }),
        output: json!({"delivery_color_space": "srgb", "alpha": "opaque"}),
        instrumentation: finish_stage(color_probe, &mut memory_sampler, "color.deliver")?,
        success: true,
        diagnostics: vec![
            "The sRGB delivery boundary is declared without a production color transform."
                .to_owned(),
        ],
    });

    let export_probe = begin_stage(&mut memory_sampler, "media.export")?;
    let artifact_path = artifact_dir.join(STUB_ARTIFACT_NAME);
    let artifact_bytes = contract_stub_bytes();
    publish_create_only(&artifact_path, &artifact_bytes).map_err(|error| {
        CliFailure::stage(
            "media.export",
            "unavailable",
            "user_correctable",
            format!("could not publish contract artifact: {error}"),
        )
    })?;
    let artifact_sha256 = sha256_hex(&artifact_bytes);
    let timestamps = canonical_timestamps();
    stages.push(StageRecord {
        stage_id: "media.export",
        implementation: "stub",
        owner: "superi-cli.contract-runner",
        implementation_revision: implementation_revision.clone(),
        input: json!({
            "target_container": "webm",
            "target_codec": "av1",
            "target_encoder": "rust-av1",
            "frame_count": 48
        }),
        output: json!({
            "artifact_name": STUB_ARTIFACT_NAME,
            "artifact_kind": "contract_stub",
            "bytes": artifact_bytes.len(),
            "sha256": artifact_sha256
        }),
        instrumentation: finish_stage(export_probe, &mut memory_sampler, "media.export")?,
        success: true,
        diagnostics: vec![
            "The published artifact is a non-playable contract stub, not canonical.webm."
                .to_owned(),
        ],
    });

    let verify_probe = begin_stage(&mut memory_sampler, "slice.verify")?;
    execute_action(&mut api, "slice.verify", SliceAction::Undo {})?;
    execute_action(&mut api, "slice.verify", SliceAction::Undo {})?;
    execute_action(&mut api, "slice.verify", SliceAction::Redo {})?;
    execute_action(&mut api, "slice.verify", SliceAction::Redo {})?;
    let replayed_state = serde_json::to_value(api.snapshot()).expect("state is serializable");
    let original_semantic = semantic_state(original_final_state.clone());
    let replayed_semantic = semantic_state(replayed_state.clone());
    if original_semantic != replayed_semantic {
        return Err(CliFailure::stage(
            "slice.verify",
            "internal",
            "terminal",
            "undo and redo did not recover the exact final semantic state",
        ));
    }
    let portable_project_state = portable_project_state(replayed_semantic.clone())?;
    let project_state_sha256 = digest_json(&portable_project_state);
    let operation_log_sha256 = digest_json(&replayed_state["operation_log"]);
    let export_expectation = json!({
        "container": "webm",
        "codec": "av1",
        "encoder": "rust-av1",
        "pixel_format": "yuv420p8",
        "color_space": "srgb",
        "range": "limited",
        "matrix": "bt709",
        "alpha": "opaque",
        "frame_rate": {"numerator": 24, "denominator": 1},
        "time_base": {"numerator": 1, "denominator": 24},
        "width": 96,
        "height": 54,
        "frame_count": 48,
        "audio_streams": 0
    });
    let expectation_evidence = resolve_expectations(
        &repository_root,
        &ContractObservations {
            source_manifest_sha256: &fixture.manifest_sha256,
            source_payload_sha256: &fixture.payload_sha256,
            project_state_sha256: &project_state_sha256,
            timeline_sha256: &timeline_sha256,
            graph_sha256: &graph_sha256,
            operation_log_sha256: &operation_log_sha256,
            undo_redo_recovered: true,
            timestamps: &timestamps,
            export: &export_expectation,
        },
    )
    .map_err(|failure| match failure.kind() {
        ExpectationFailureKind::Unavailable => {
            CliFailure::unavailable("slice.verify", failure.message())
        }
        ExpectationFailureKind::Corrupt => CliFailure::stage(
            "slice.verify",
            "corrupt_data",
            "user_correctable",
            failure.message(),
        ),
        ExpectationFailureKind::Mismatch => {
            CliFailure::stage("slice.verify", "internal", "terminal", failure.message())
        }
    })?;
    stages.push(StageRecord {
        stage_id: "slice.verify",
        implementation: "runtime",
        owner: "superi-cli.contract-verifier",
        implementation_revision: implementation_revision.clone(),
        input: json!({
            "required_stage_count": 8,
            "required_operation_ids": [
                "slice.op.import",
                "slice.op.insert",
                "slice.op.trim",
                "slice.op.effect"
            ],
            "reversal_sequence": ["undo_effect", "undo_trim", "redo_trim", "redo_effect"]
        }),
        output: json!({
            "undo_redo_recovered": true,
            "project_state_sha256": project_state_sha256,
            "expectations_status": "contract_passed"
        }),
        instrumentation: finish_stage(verify_probe, &mut memory_sampler, "slice.verify")?,
        success: true,
        diagnostics: vec![
            "All applicable canonical expectations passed; rendered pixel comparison remains not evaluated while graph, color, and export stages are stubs."
                .to_owned(),
        ],
    });

    let stub_stages = stages
        .iter()
        .filter(|stage| stage.implementation == "stub")
        .map(|stage| stage.stage_id)
        .collect::<Vec<_>>();
    let observed_resident_bytes_max = stages
        .iter()
        .map(|stage| stage.instrumentation.memory.observed_resident_bytes_max())
        .max()
        .expect("the canonical slice contains stages");
    let instrumentation = InstrumentationSummary::new(stages.len(), observed_resident_bytes_max);
    let report = json!({
        "schema_version": "1.1.0",
        "scenario_id": SCENARIO_ID,
        "scenario_revision": SCENARIO_REVISION,
        "success": true,
        "conformance": "contract",
        "repository": {
            "commit": repository.commit,
            "dirty": repository.dirty
        },
        "fixture": {
            "fixture_id": FIXTURE_ID,
            "fixture_version": FIXTURE_VERSION,
            "manifest_path": "open/test-fixtures/slice/video-cfr/v1/fixture.json",
            "manifest_sha256": fixture.manifest_sha256,
            "payload": {
                "path": "open/test-fixtures/slice/video-cfr/v1/input.webm",
                "bytes": fixture.payload_bytes,
                "sha256": fixture.payload_sha256,
                "container": "webm",
                "codec": "av1",
                "pixel_format": "yuv420p8",
                "range": "limited",
                "matrix": "bt709",
                "frame_rate": {"numerator": 24, "denominator": 1},
                "time_base": {"numerator": 1, "denominator": 24},
                "frame_count": 96,
                "width": 96,
                "height": 54,
                "audio_streams": 0,
                "trait_validation": "expected_contract_only"
            }
        },
        "digests": {
            "project_state_sha256": project_state_sha256,
            "timeline_sha256": timeline_sha256,
            "graph_sha256": graph_sha256,
            "operation_log_sha256": operation_log_sha256
        },
        "state": replayed_state,
        "instrumentation": instrumentation,
        "stages": stages,
        "backends": {
            "selected_container_backend": "mkv-webm",
            "selected_codec_backend": "rust-av1",
            "active_runtime_backends": [],
            "features": active_features(),
            "target": format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            "toolchain": toolchain,
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" }
        },
        "export": {
            "path": artifact_path.display().to_string(),
            "artifact_kind": "contract_stub",
            "implementation": "stub",
            "playable": false,
            "bytes": artifact_bytes.len(),
            "sha256": artifact_sha256,
            "target_stream": {
                "container": "webm",
                "codec": "av1",
                "encoder": "rust-av1",
                "pixel_format": "yuv420p8",
                "color_space": "srgb",
                "range": "limited",
                "matrix": "bt709",
                "alpha": "opaque",
                "frame_rate": {"numerator": 24, "denominator": 1},
                "time_base": {"numerator": 1, "denominator": 24},
                "width": 96,
                "height": 54,
                "frame_count": 48,
                "audio_streams": 0,
                "timestamps": timestamps
            }
        },
        "expectations": expectation_evidence,
        "verification": {
            "undo_redo_recovered": true,
            "reproducibility_scope": [
                "deterministic_state",
                "stage_identity",
                "artifact_bytes",
                "expectation_evidence"
            ]
        },
        "stub_stages": stub_stages,
        "diagnostics": [
            "Contract conformance completed with disclosed stubs and is not runtime conformance.",
            "The artifact is non-playable evidence and is not a working editor export."
        ]
    });

    let mut report_bytes = serde_json::to_vec_pretty(&report).expect("report is serializable");
    report_bytes.push(b'\n');
    if let Err(error) = publish_create_only(report_path, &report_bytes) {
        let _ = remove_owned_file(&artifact_path, &artifact_sha256);
        return Err(CliFailure::stage(
            "slice.verify",
            "unavailable",
            "user_correctable",
            format!("could not publish report: {error}"),
        ));
    }

    Ok(json!({
        "scenario_id": SCENARIO_ID,
        "conformance": "contract",
        "success": true,
        "artifact": artifact_path.display().to_string(),
        "report": report_path.display().to_string()
    }))
}

fn execute_action(
    api: &mut ScenarioApi,
    stage_id: &'static str,
    action: SliceAction,
) -> Result<Value, CliFailure> {
    let expected_revision = api.snapshot().revision();
    let transaction_number = expected_revision.checked_add(1).ok_or_else(|| {
        CliFailure::stage(
            stage_id,
            "resource_exhausted",
            "terminal",
            "scenario revision is exhausted before transaction dispatch",
        )
    })?;
    let transaction_id = format!("slice-transaction-{transaction_number}");
    let result = api
        .execute_transaction(ExecuteScenarioTransaction::new(
            transaction_id.clone(),
            expected_revision,
            vec![action],
        ))
        .map_err(|failure| {
            CliFailure::stage(
                stage_id,
                failure_category(failure.category()),
                failure_recoverability(failure.recoverability()),
                failure.message().to_owned(),
            )
        })?;
    let events = api.drain_events();
    let event = match events.as_slice() {
        [event] => event,
        _ => {
            return Err(CliFailure::stage(
                stage_id,
                "internal",
                "terminal",
                "scenario transaction did not publish exactly one state event",
            ));
        }
    };
    if event.transaction_id() != transaction_id
        || event.command_sequence() != result.command_sequence()
        || event.project_revision() != result.state().revision()
        || event.state() != result.state()
    {
        return Err(CliFailure::stage(
            stage_id,
            "internal",
            "terminal",
            "scenario transaction result and state event do not match",
        ));
    }
    Ok(serde_json::to_value(result).expect("API result is serializable"))
}

fn scenario_api_for_fixture(path: &Path) -> Result<ScenarioApi, CliFailure> {
    let target = ApiFilesystemPath::native(path.display().to_string()).map_err(|error| {
        CliFailure::stage(
            "fixture.resolve",
            failure_category(error.category().code()),
            failure_recoverability(error.recoverability().code()),
            "canonical fixture path cannot be represented by API permission policy",
        )
    })?;
    let permissions = ApiPermissionContext::new(
        "superi.cli.slice-runner",
        [ApiPermissionRule::filesystem(
            ApiPermissionEffect::Allow,
            ApiFilesystemAccess::Read,
            ApiFilesystemScope::exact(target),
        )],
    )
    .map_err(|error| {
        CliFailure::stage(
            "fixture.resolve",
            failure_category(error.category().code()),
            failure_recoverability(error.recoverability().code()),
            "canonical fixture permission policy is invalid",
        )
    })?;
    Ok(ScenarioApi::new_with_permissions(Arc::new(permissions)))
}

fn failure_category(value: &str) -> &'static str {
    match value {
        "invalid_input" => "invalid_input",
        "permission_denied" => "permission_denied",
        "not_found" => "not_found",
        "conflict" => "conflict",
        "unsupported" => "unsupported",
        "corrupt_data" => "corrupt_data",
        "resource_exhausted" => "resource_exhausted",
        "unavailable" => "unavailable",
        _ => "internal",
    }
}

fn failure_recoverability(value: &str) -> &'static str {
    match value {
        "user_correctable" => "user_correctable",
        "retryable" => "retryable",
        _ => "terminal",
    }
}

fn validate_output_targets(artifact_dir: &Path, report: &Path) -> Result<(), CliFailure> {
    if artifact_dir.as_os_str().is_empty() || report.as_os_str().is_empty() {
        return Err(CliFailure::invalid(
            "artifact and report paths must not be empty",
        ));
    }
    if report == artifact_dir.join(STUB_ARTIFACT_NAME) {
        return Err(CliFailure::invalid(
            "report path must not collide with the contract artifact path",
        ));
    }
    match fs::symlink_metadata(report) {
        Ok(_) => return Err(CliFailure::invalid("report path already exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CliFailure::invalid(format!(
                "report path could not be inspected: {error}"
            )));
        }
    }
    match fs::symlink_metadata(artifact_dir) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(CliFailure::invalid(
                    "artifact path must be an absent or non-symlink directory",
                ));
            }
            let mut entries = fs::read_dir(artifact_dir).map_err(|error| {
                CliFailure::invalid(format!("artifact directory could not be read: {error}"))
            })?;
            if entries.next().is_some() {
                return Err(CliFailure::invalid("artifact directory must be empty"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CliFailure::invalid(format!(
                "artifact path could not be inspected: {error}"
            )));
        }
    }

    let artifact_parent = artifact_dir
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let artifact_parent_metadata = fs::symlink_metadata(artifact_parent)
        .map_err(|error| CliFailure::invalid(format!("artifact parent is unavailable: {error}")))?;
    if artifact_parent_metadata.file_type().is_symlink() || !artifact_parent_metadata.is_dir() {
        return Err(CliFailure::invalid(
            "artifact parent must be a non-symlink directory",
        ));
    }

    let report_parent = report
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if report_parent != artifact_dir {
        let metadata = fs::symlink_metadata(report_parent).map_err(|error| {
            CliFailure::invalid(format!("report parent is unavailable: {error}"))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(CliFailure::invalid(
                "report parent must be a non-symlink directory",
            ));
        }
    }
    Ok(())
}

fn create_artifact_directory(path: &Path) -> Result<(), CliFailure> {
    if path.is_dir() {
        return Ok(());
    }
    fs::create_dir(path).map_err(|error| {
        CliFailure::stage(
            "fixture.resolve",
            "unavailable",
            "user_correctable",
            format!("artifact directory could not be created: {error}"),
        )
    })
}

fn find_repository_root() -> Result<PathBuf, CliFailure> {
    let current = std::env::current_dir().map_err(|error| {
        CliFailure::unavailable(
            "fixture.resolve",
            format!("working directory unavailable: {error}"),
        )
    })?;
    for candidate in current.ancestors() {
        if candidate.join("open/Cargo.toml").is_file()
            && candidate.join("docs/vertical-slice.md").is_file()
        {
            return Ok(candidate.to_path_buf());
        }
    }
    Err(CliFailure::unavailable(
        "fixture.resolve",
        "Superi repository root could not be located from the working directory",
    ))
}

fn inspect_repository(root: &Path) -> Result<RepositoryState, CliFailure> {
    let root_text = root.to_string_lossy();
    let commit = command_text("git", &["-C", &root_text, "rev-parse", "HEAD"])
        .map_err(|message| CliFailure::unavailable("fixture.resolve", message))?;
    let status = command_text("git", &["-C", &root_text, "status", "--porcelain"])
        .map_err(|message| CliFailure::unavailable("fixture.resolve", message))?;
    Ok(RepositoryState {
        commit,
        dirty: !status.is_empty(),
    })
}

fn command_text(program: &str, arguments: &[&str]) -> Result<String, String> {
    let output = ProcessCommand::new(program)
        .args(arguments)
        .output()
        .map_err(|error| format!("could not execute {program}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn resolve_fixture(root: &Path) -> Result<ResolvedFixture, CliFailure> {
    let version_dir = root.join("open/test-fixtures/slice/video-cfr/v1");
    let manifest_path = version_dir.join("fixture.json");
    let payload_path = version_dir.join("input.webm");
    let manifest_metadata = fs::symlink_metadata(&manifest_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliFailure::unavailable(
                "fixture.resolve",
                "canonical fixture manifest is unavailable",
            )
        } else {
            CliFailure::stage(
                "fixture.resolve",
                "unavailable",
                "user_correctable",
                format!("canonical fixture manifest could not be inspected: {error}"),
            )
        }
    })?;
    if manifest_metadata.file_type().is_symlink() || !manifest_metadata.is_file() {
        return Err(CliFailure::stage(
            "fixture.resolve",
            "invalid_input",
            "user_correctable",
            "canonical fixture manifest must be a non-symlink regular file",
        ));
    }
    let manifest_bytes = read_bounded(&manifest_path, MAX_MANIFEST_BYTES).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliFailure::unavailable(
                "fixture.resolve",
                "canonical fixture manifest is unavailable",
            )
        } else {
            CliFailure::stage(
                "fixture.resolve",
                "corrupt_data",
                "user_correctable",
                format!("canonical fixture manifest could not be read: {error}"),
            )
        }
    })?;
    let manifest: FixtureManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        CliFailure::stage(
            "fixture.resolve",
            "corrupt_data",
            "user_correctable",
            format!("canonical fixture manifest is invalid: {error}"),
        )
    })?;
    validate_manifest(&manifest)?;
    let payload_entry = &manifest.files[0];
    let metadata = fs::symlink_metadata(&payload_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliFailure::unavailable(
                "fixture.resolve",
                "canonical fixture payload is unavailable",
            )
        } else {
            CliFailure::stage(
                "fixture.resolve",
                "unavailable",
                "user_correctable",
                format!("canonical fixture payload could not be inspected: {error}"),
            )
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CliFailure::stage(
            "fixture.resolve",
            "invalid_input",
            "user_correctable",
            "canonical fixture payload must be a non-symlink regular file",
        ));
    }
    if metadata.len() > MAX_PAYLOAD_BYTES {
        return Err(CliFailure::stage(
            "fixture.resolve",
            "resource_exhausted",
            "user_correctable",
            "canonical fixture payload exceeds the runner byte bound",
        ));
    }
    if metadata.len() != payload_entry.bytes {
        return Err(CliFailure::stage(
            "fixture.resolve",
            "corrupt_data",
            "user_correctable",
            "canonical fixture payload byte count does not match its manifest",
        ));
    }
    let payload_bytes = read_bounded(&payload_path, MAX_PAYLOAD_BYTES).map_err(|error| {
        CliFailure::stage(
            "fixture.resolve",
            "unavailable",
            "user_correctable",
            format!("canonical fixture payload could not be read: {error}"),
        )
    })?;
    let payload_sha256 = sha256_hex(&payload_bytes);
    if payload_sha256 != payload_entry.sha256 {
        return Err(CliFailure::stage(
            "fixture.resolve",
            "corrupt_data",
            "user_correctable",
            "canonical fixture payload digest does not match its manifest",
        ));
    }
    Ok(ResolvedFixture {
        payload_path,
        manifest_sha256: sha256_hex(&manifest_bytes),
        payload_sha256,
        payload_bytes: metadata.len(),
    })
}

fn validate_manifest(manifest: &FixtureManifest) -> Result<(), CliFailure> {
    if manifest.schema_version != 1
        || manifest.fixture_id != FIXTURE_ID
        || manifest.fixture_version != FIXTURE_VERSION
        || manifest.files.len() != 1
    {
        return Err(CliFailure::stage(
            "fixture.resolve",
            "corrupt_data",
            "user_correctable",
            "canonical fixture manifest identity or inventory is incorrect",
        ));
    }
    let file = &manifest.files[0];
    if file.path != "input.webm"
        || file.media_type != "video/webm"
        || !is_lower_sha256(&file.sha256)
    {
        return Err(CliFailure::stage(
            "fixture.resolve",
            "corrupt_data",
            "user_correctable",
            "canonical fixture payload declaration is incorrect",
        ));
    }
    let provenance = &manifest.provenance;
    let required_text = [
        manifest.description.as_str(),
        provenance.kind.as_str(),
        provenance.source.as_str(),
        provenance.author.as_str(),
        provenance.created_on.as_str(),
        provenance.license.as_str(),
        provenance.rights.as_str(),
    ];
    if required_text.iter().any(|value| value.trim().is_empty()) {
        return Err(CliFailure::stage(
            "fixture.resolve",
            "corrupt_data",
            "user_correctable",
            "canonical fixture provenance contains an empty required field",
        ));
    }
    let generator = provenance.generator.as_ref().ok_or_else(|| {
        CliFailure::stage(
            "fixture.resolve",
            "corrupt_data",
            "user_correctable",
            "canonical generated fixture lacks generator provenance",
        )
    })?;
    if [
        generator.name.as_str(),
        generator.version.as_str(),
        generator.command.as_str(),
        generator.seed.as_str(),
    ]
    .iter()
    .any(|value| value.trim().is_empty())
    {
        return Err(CliFailure::stage(
            "fixture.resolve",
            "corrupt_data",
            "user_correctable",
            "canonical fixture generator provenance is incomplete",
        ));
    }
    for parent in &provenance.parents {
        if parent.fixture_id.is_empty()
            || parent.fixture_version == 0
            || !is_lower_sha256(&parent.manifest_sha256)
        {
            return Err(CliFailure::stage(
                "fixture.resolve",
                "corrupt_data",
                "user_correctable",
                "canonical fixture parent provenance is invalid",
            ));
        }
    }
    Ok(())
}

fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file exceeds bounded read limit",
        ));
    }
    let file = fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file grew beyond bounded read limit",
        ));
    }
    Ok(bytes)
}

fn contract_stub_bytes() -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(&json!({
        "schema_version": "1.0.0",
        "scenario_id": SCENARIO_ID,
        "artifact_kind": "contract_stub",
        "implementation": "stub",
        "playable": false,
        "missing_runtime_owners": [
            "media.import",
            "timeline.edit",
            "timeline.compile",
            "graph.evaluate",
            "color.deliver",
            "media.export"
        ],
        "planned_target": {
            "path": "canonical.webm",
            "container": "webm",
            "codec": "av1",
            "encoder": "rust-av1",
            "width": 96,
            "height": 54,
            "frame_rate": {"numerator": 24, "denominator": 1},
            "frame_count": 48,
            "color_space": "srgb"
        }
    }))
    .expect("contract stub is serializable");
    bytes.push(b'\n');
    bytes
}

fn canonical_timestamps() -> Vec<Value> {
    (0_u64..48)
        .map(|frame| {
            json!({
                "value": frame,
                "time_base": {"numerator": 1, "denominator": 24}
            })
        })
        .collect()
}

fn active_features() -> Vec<&'static str> {
    let mut features = vec!["default"];
    if cfg!(feature = "os-codecs") {
        features.push("os-codecs");
    }
    features
}

fn semantic_state(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.remove("revision");
    }
    value
}

fn portable_project_state(mut value: Value) -> Result<Value, CliFailure> {
    match value.pointer_mut("/media/path") {
        Some(Value::String(path)) if !path.is_empty() => {
            *path = PORTABLE_FIXTURE_PATH.to_owned();
            Ok(value)
        }
        _ => Err(CliFailure::stage(
            "slice.verify",
            "internal",
            "terminal",
            "canonical project state lacks its imported fixture path",
        )),
    }
}

fn digest_json(value: &Value) -> String {
    sha256_hex(&serde_json::to_vec(value).expect("value is serializable"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn publish_create_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let counter = TEMPORARY_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("report");
    let temporary = parent.join(format!(
        ".{name}.superi-{}-{counter}.tmp",
        std::process::id()
    ));
    let result: std::io::Result<()> = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::hard_link(&temporary, path)?;
        Ok(())
    })();
    let cleanup = fs::remove_file(&temporary);
    result?;
    cleanup
}

fn remove_owned_file(path: &Path, expected_sha256: &str) -> std::io::Result<()> {
    let bytes = fs::read(path)?;
    if sha256_hex(&bytes) != expected_sha256 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "owned file changed before cleanup",
        ));
    }
    fs::remove_file(path)
}

fn begin_stage(
    sampler: &mut ProcessMemorySampler,
    stage_id: &'static str,
) -> Result<StageProbe, CliFailure> {
    sampler.begin_stage().map_err(|message| {
        CliFailure::unavailable(
            stage_id,
            format!("memory instrumentation unavailable: {message}"),
        )
    })
}

fn finish_stage(
    probe: StageProbe,
    sampler: &mut ProcessMemorySampler,
    stage_id: &'static str,
) -> Result<StageInstrumentation, CliFailure> {
    probe.finish(sampler).map_err(|message| {
        CliFailure::unavailable(
            stage_id,
            format!("memory instrumentation unavailable: {message}"),
        )
    })
}

fn emit_failure(failure: CliFailure) -> i32 {
    let mut value = json!({
        "record": "error",
        "category": failure.category,
        "recoverability": failure.recoverability,
        "message": failure.message
    });
    if let Some(stage_id) = failure.stage_id {
        value["stage_id"] = Value::String(stage_id.to_owned());
    }
    if !failure.contexts.is_empty() {
        value["contexts"] =
            serde_json::to_value(failure.contexts).expect("failure contexts are serializable");
    }
    eprintln!(
        "{}",
        serde_json::to_string(&value).expect("failure is serializable")
    );
    failure.exit
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        digest_json, execute_action, portable_project_state, scenario_api_for_fixture,
        ExactFrameRate, SliceAction, PORTABLE_FIXTURE_PATH, TEMPORARY_FILE_COUNTER,
    };
    use std::sync::atomic::Ordering;

    #[test]
    fn portable_project_digest_does_not_depend_on_the_checkout_path() {
        let first = portable_project_state(json!({
            "media": {"path": "/checkout/one/open/test-fixtures/slice/video-cfr/v1/input.webm"},
            "phase": "effected"
        }))
        .unwrap_or_else(|_| panic!("first canonical state must normalize"));
        let second = portable_project_state(json!({
            "media": {"path": "/checkout/two/open/test-fixtures/slice/video-cfr/v1/input.webm"},
            "phase": "effected"
        }))
        .unwrap_or_else(|_| panic!("second canonical state must normalize"));

        assert_eq!(first["media"]["path"], PORTABLE_FIXTURE_PATH);
        assert_eq!(first, second);
        assert_eq!(digest_json(&first), digest_json(&second));
    }

    #[test]
    fn action_helper_uses_revision_fenced_transactions_and_consumes_the_matching_event() {
        let counter = TEMPORARY_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let source = std::env::temp_dir().join(format!(
            "superi-cli-dispatcher-test-{}-{counter}.webm",
            std::process::id()
        ));
        let bytes = b"cli dispatcher fixture";
        std::fs::write(&source, bytes).unwrap();
        let mut api = scenario_api_for_fixture(&source)
            .unwrap_or_else(|failure| panic!("fixture policy failed: {}", failure.message));

        let value = execute_action(
            &mut api,
            "dispatcher.test",
            SliceAction::ImportClip {
                path: source.display().to_string(),
                fixture_id: "slice/video-cfr".to_owned(),
                fixture_version: 1,
                manifest_sha256: "1d2b28b5f44c7f86dce50d67b718b0fad967d267d9016961e3d71bb9dab94419"
                    .to_owned(),
                payload_sha256: super::sha256_hex(bytes),
                frame_rate: ExactFrameRate::new(24, 1),
                frame_count: 96,
                width: 96,
                height: 54,
            },
        )
        .unwrap_or_else(|failure| panic!("transaction helper failed: {}", failure.message));

        assert_eq!(value["transaction_id"], "slice-transaction-1");
        assert_eq!(value["command_sequence"], 1);
        assert_eq!(value["state"]["revision"], 1);
        assert!(api.drain_events().is_empty());
        std::fs::remove_file(source).unwrap();
    }
}
