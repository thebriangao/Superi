//! Durable local project workflows exposed through the stable public API.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use superi_api::commands::{
    CompareProjectRecovery, DismissProjectRecovery, ExecuteProjectSettingsTransaction,
    GetEditorState, GetProjectRecovery, RestoreProjectRecovery,
};
use superi_api::editor::{ExecuteProjectCommand, GetProjectCommandLog, ProjectCommandLogDetail};
use superi_api::local::{LocalAutomationRequest, LocalProjectCollision, LocalProjectHost};
use superi_api::permissions::{ApiPermissionContext, ApiPermissionRule};
use superi_core::error::Result as CoreResult;

use crate::commands::{run_engine_validation, run_public_api_schema, CliFailure};

const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
const MAX_AUTOMATION_LINE_BYTES: usize = 1024 * 1024;

pub(crate) enum WorkflowCommand {
    ProjectCreate {
        project: PathBuf,
        request: InputSource,
    },
    ProjectExecute {
        project: PathBuf,
        request: InputSource,
        permissions: Option<PathBuf>,
    },
    ProjectInspect {
        project: PathBuf,
    },
    ProjectCommandLog {
        project: PathBuf,
        after_sequence: u64,
        limit: u32,
        detail: ProjectCommandLogDetail,
        permissions: Option<PathBuf>,
    },
    ProjectSaveCopy {
        project: PathBuf,
        destination: PathBuf,
        collision: LocalProjectCollision,
    },
    ProjectBackup {
        project: PathBuf,
        destination: PathBuf,
    },
    ProjectRecovery {
        operation: RecoveryOperation,
        project: PathBuf,
        recovery_root: PathBuf,
        request: InputSource,
        permissions: Option<PathBuf>,
    },
    MediaExecute {
        project: PathBuf,
        request: InputSource,
        permissions: Option<PathBuf>,
    },
    TimelineExecute {
        project: PathBuf,
        request: InputSource,
        permissions: Option<PathBuf>,
    },
    RenderInspect {
        project: PathBuf,
    },
    RenderConfigure {
        project: PathBuf,
        request: InputSource,
    },
    InspectEditor {
        project: PathBuf,
    },
    InspectApiSchema,
    ValidateProject {
        project: PathBuf,
    },
    ValidateEngine,
    AutomationRun {
        project: PathBuf,
        input: InputSource,
        permissions: Option<PathBuf>,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum RecoveryOperation {
    Get,
    Compare,
    Restore,
    Dismiss,
}

pub(crate) enum InputSource {
    Stdin,
    File(PathBuf),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissionPolicy {
    principal: String,
    rules: Vec<ApiPermissionRule>,
}

pub(crate) fn parse_workflow_command(
    arguments: &[OsString],
) -> Result<Option<WorkflowCommand>, CliFailure> {
    let Some(command) = arguments.first().and_then(|value| value.to_str()) else {
        return Ok(None);
    };
    match command {
        "project" => parse_project(&arguments[1..]).map(Some),
        "media" => parse_execute_domain(&arguments[1..], true).map(Some),
        "timeline" => parse_execute_domain(&arguments[1..], false).map(Some),
        "render" => parse_render(&arguments[1..]).map(Some),
        "inspect" => parse_inspect(&arguments[1..]).map(Some),
        "validate" => parse_validate(&arguments[1..]).map(Some),
        "automation" => parse_automation(&arguments[1..]).map(Some),
        _ => Ok(None),
    }
}

pub(crate) fn run_workflow(command: WorkflowCommand) -> Result<Vec<Value>, CliFailure> {
    match command {
        WorkflowCommand::ProjectCreate { project, request } => one_core(
            "project.create",
            LocalProjectHost::create(project, read_json(request, "project create request")?),
        ),
        WorkflowCommand::ProjectExecute {
            project,
            request,
            permissions,
        } => one_core(
            "project.execute",
            LocalProjectHost::execute_project(
                project,
                read_json(request, "project command request")?,
                read_permissions(permissions)?,
            ),
        ),
        WorkflowCommand::ProjectInspect { project } => one_core(
            "project.inspect",
            LocalProjectHost::inspect_editor(project, GetEditorState::new("cli-project-inspect")),
        ),
        WorkflowCommand::ProjectCommandLog {
            project,
            after_sequence,
            limit,
            detail,
            permissions,
        } => one_core(
            "project.command-log",
            LocalProjectHost::inspect_command_log(
                project,
                GetProjectCommandLog::new(after_sequence, limit, detail),
                read_permissions(permissions)?,
            ),
        ),
        WorkflowCommand::ProjectSaveCopy {
            project,
            destination,
            collision,
        } => one_core(
            "project.save-copy",
            LocalProjectHost::save_copy(project, destination, collision),
        ),
        WorkflowCommand::ProjectBackup {
            project,
            destination,
        } => one_core(
            "project.backup",
            LocalProjectHost::backup(project, destination),
        ),
        WorkflowCommand::ProjectRecovery {
            operation,
            project,
            recovery_root,
            request,
            permissions,
        } => {
            let permissions = read_permissions(permissions)?;
            match operation {
                RecoveryOperation::Get => one_core(
                    "project.recovery.get",
                    LocalProjectHost::recovery_get(
                        project,
                        recovery_root,
                        read_json::<GetProjectRecovery>(request, "recovery get request")?,
                        permissions,
                    ),
                ),
                RecoveryOperation::Compare => one_core(
                    "project.recovery.compare",
                    LocalProjectHost::recovery_compare(
                        project,
                        recovery_root,
                        read_json::<CompareProjectRecovery>(request, "recovery compare request")?,
                        permissions,
                    ),
                ),
                RecoveryOperation::Restore => one_core(
                    "project.recovery.restore",
                    LocalProjectHost::recovery_restore(
                        project,
                        recovery_root,
                        read_json::<RestoreProjectRecovery>(request, "recovery restore request")?,
                        permissions,
                    ),
                ),
                RecoveryOperation::Dismiss => one_core(
                    "project.recovery.dismiss",
                    LocalProjectHost::recovery_dismiss(
                        project,
                        recovery_root,
                        read_json::<DismissProjectRecovery>(request, "recovery dismiss request")?,
                        permissions,
                    ),
                ),
            }
        }
        WorkflowCommand::MediaExecute {
            project,
            request,
            permissions,
        } => one_core(
            "media.execute",
            LocalProjectHost::execute_media(
                project,
                read_json::<ExecuteProjectCommand>(request, "media command request")?,
                read_permissions(permissions)?,
            ),
        ),
        WorkflowCommand::TimelineExecute {
            project,
            request,
            permissions,
        } => one_core(
            "timeline.execute",
            LocalProjectHost::execute_timeline(
                project,
                read_json::<ExecuteProjectCommand>(request, "timeline command request")?,
                read_permissions(permissions)?,
            ),
        ),
        WorkflowCommand::RenderInspect { project } => one_core(
            "render.inspect",
            LocalProjectHost::inspect_render(project, GetEditorState::new("cli-render-inspect")),
        ),
        WorkflowCommand::RenderConfigure { project, request } => one_core(
            "render.configure",
            LocalProjectHost::configure_render(
                project,
                read_json::<ExecuteProjectSettingsTransaction>(
                    request,
                    "render configuration request",
                )?,
            ),
        ),
        WorkflowCommand::InspectEditor { project } => one_core(
            "inspect.editor",
            LocalProjectHost::inspect_editor(project, GetEditorState::new("cli-inspect-editor")),
        ),
        WorkflowCommand::InspectApiSchema => {
            one_value("inspect.api-schema", run_public_api_schema()?)
        }
        WorkflowCommand::ValidateProject { project } => {
            one_core("validate.project", LocalProjectHost::validate(project))
        }
        WorkflowCommand::ValidateEngine => one_value("validate.engine", run_engine_validation()?),
        WorkflowCommand::AutomationRun {
            project,
            input,
            permissions,
        } => run_automation(project, input, read_permissions(permissions)?),
    }
}

fn parse_project(arguments: &[OsString]) -> Result<WorkflowCommand, CliFailure> {
    let Some(operation) = arguments.first().and_then(|value| value.to_str()) else {
        return Err(CliFailure::invalid("project requires an operation"));
    };
    match operation {
        "create" => {
            let mut options = Options::parse(&arguments[1..])?;
            let command = WorkflowCommand::ProjectCreate {
                project: options.required_path("--project")?,
                request: options.required_input("--request")?,
            };
            options.finish()?;
            Ok(command)
        }
        "execute" => {
            let mut options = Options::parse(&arguments[1..])?;
            let command = WorkflowCommand::ProjectExecute {
                project: options.required_path("--project")?,
                request: options.required_input("--request")?,
                permissions: options.optional_regular_path("--permissions")?,
            };
            options.finish()?;
            Ok(command)
        }
        "inspect" => {
            let mut options = Options::parse(&arguments[1..])?;
            let command = WorkflowCommand::ProjectInspect {
                project: options.required_path("--project")?,
            };
            options.finish()?;
            Ok(command)
        }
        "command-log" => {
            let mut options = Options::parse(&arguments[1..])?;
            let detail = match options.required_utf8("--detail")?.as_str() {
                "metadata" => ProjectCommandLogDetail::Metadata,
                "replayable" => ProjectCommandLogDetail::Replayable,
                _ => {
                    return Err(CliFailure::invalid(
                        "--detail must be metadata or replayable",
                    ));
                }
            };
            let command = WorkflowCommand::ProjectCommandLog {
                project: options.required_path("--project")?,
                after_sequence: options.required_u64("--after-sequence")?,
                limit: options.required_u32("--limit")?,
                detail,
                permissions: options.optional_regular_path("--permissions")?,
            };
            options.finish()?;
            Ok(command)
        }
        "save-copy" => {
            let mut options = Options::parse(&arguments[1..])?;
            let collision = match options.required_utf8("--collision")?.as_str() {
                "require-absent" => LocalProjectCollision::RequireAbsent,
                "replace-existing" => LocalProjectCollision::ReplaceExisting,
                _ => {
                    return Err(CliFailure::invalid(
                        "--collision must be require-absent or replace-existing",
                    ));
                }
            };
            let command = WorkflowCommand::ProjectSaveCopy {
                project: options.required_path("--project")?,
                destination: options.required_path("--destination")?,
                collision,
            };
            options.finish()?;
            Ok(command)
        }
        "backup" => {
            let mut options = Options::parse(&arguments[1..])?;
            let command = WorkflowCommand::ProjectBackup {
                project: options.required_path("--project")?,
                destination: options.required_path("--destination")?,
            };
            options.finish()?;
            Ok(command)
        }
        "recovery" => parse_recovery(&arguments[1..]),
        _ => Err(CliFailure::invalid("unrecognized project operation")),
    }
}

fn parse_recovery(arguments: &[OsString]) -> Result<WorkflowCommand, CliFailure> {
    let operation = match arguments.first().and_then(|value| value.to_str()) {
        Some("get") => RecoveryOperation::Get,
        Some("compare") => RecoveryOperation::Compare,
        Some("restore") => RecoveryOperation::Restore,
        Some("dismiss") => RecoveryOperation::Dismiss,
        Some(_) => return Err(CliFailure::invalid("unrecognized recovery operation")),
        None => {
            return Err(CliFailure::invalid(
                "project recovery requires an operation",
            ))
        }
    };
    let mut options = Options::parse(&arguments[1..])?;
    let command = WorkflowCommand::ProjectRecovery {
        operation,
        project: options.required_path("--project")?,
        recovery_root: options.required_path("--recovery-root")?,
        request: options.required_input("--request")?,
        permissions: options.optional_regular_path("--permissions")?,
    };
    options.finish()?;
    Ok(command)
}

fn parse_execute_domain(
    arguments: &[OsString],
    media: bool,
) -> Result<WorkflowCommand, CliFailure> {
    if arguments.first().and_then(|value| value.to_str()) != Some("execute") {
        return Err(CliFailure::invalid(if media {
            "media requires the execute operation"
        } else {
            "timeline requires the execute operation"
        }));
    }
    let mut options = Options::parse(&arguments[1..])?;
    let project = options.required_path("--project")?;
    let request = options.required_input("--request")?;
    let permissions = options.optional_regular_path("--permissions")?;
    options.finish()?;
    Ok(if media {
        WorkflowCommand::MediaExecute {
            project,
            request,
            permissions,
        }
    } else {
        WorkflowCommand::TimelineExecute {
            project,
            request,
            permissions,
        }
    })
}

fn parse_render(arguments: &[OsString]) -> Result<WorkflowCommand, CliFailure> {
    let Some(operation) = arguments.first().and_then(|value| value.to_str()) else {
        return Err(CliFailure::invalid("render requires an operation"));
    };
    let mut options = Options::parse(&arguments[1..])?;
    let command = match operation {
        "inspect" => WorkflowCommand::RenderInspect {
            project: options.required_path("--project")?,
        },
        "configure" => WorkflowCommand::RenderConfigure {
            project: options.required_path("--project")?,
            request: options.required_input("--request")?,
        },
        _ => return Err(CliFailure::invalid("unrecognized render operation")),
    };
    options.finish()?;
    Ok(command)
}

fn parse_inspect(arguments: &[OsString]) -> Result<WorkflowCommand, CliFailure> {
    match arguments.first().and_then(|value| value.to_str()) {
        Some("editor") => {
            let mut options = Options::parse(&arguments[1..])?;
            let command = WorkflowCommand::InspectEditor {
                project: options.required_path("--project")?,
            };
            options.finish()?;
            Ok(command)
        }
        Some("api-schema") if arguments.len() == 1 => Ok(WorkflowCommand::InspectApiSchema),
        Some("api-schema") => Err(CliFailure::invalid("inspect api-schema accepts no options")),
        Some(_) => Err(CliFailure::invalid("unrecognized inspect target")),
        None => Err(CliFailure::invalid("inspect requires a target")),
    }
}

fn parse_validate(arguments: &[OsString]) -> Result<WorkflowCommand, CliFailure> {
    match arguments.first().and_then(|value| value.to_str()) {
        Some("project") => {
            let mut options = Options::parse(&arguments[1..])?;
            let command = WorkflowCommand::ValidateProject {
                project: options.required_path("--project")?,
            };
            options.finish()?;
            Ok(command)
        }
        Some("engine") if arguments.len() == 1 => Ok(WorkflowCommand::ValidateEngine),
        Some("engine") => Err(CliFailure::invalid("validate engine accepts no options")),
        Some(_) => Err(CliFailure::invalid("unrecognized validate target")),
        None => Err(CliFailure::invalid("validate requires a target")),
    }
}

fn parse_automation(arguments: &[OsString]) -> Result<WorkflowCommand, CliFailure> {
    if arguments.first().and_then(|value| value.to_str()) != Some("run") {
        return Err(CliFailure::invalid("automation requires the run operation"));
    }
    let mut options = Options::parse(&arguments[1..])?;
    let command = WorkflowCommand::AutomationRun {
        project: options.required_path("--project")?,
        input: options.required_input("--input")?,
        permissions: options.optional_regular_path("--permissions")?,
    };
    options.finish()?;
    Ok(command)
}

fn run_automation(
    project: PathBuf,
    input: InputSource,
    permissions: Arc<ApiPermissionContext>,
) -> Result<Vec<Value>, CliFailure> {
    let bytes = read_input(input, "automation input")?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| CliFailure::invalid("automation input must be UTF-8 JSONL"))?;
    let mut request_count = 0_usize;
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.len() > MAX_AUTOMATION_LINE_BYTES {
            return Err(CliFailure::invalid(format!(
                "automation line {} exceeds the byte bound",
                index + 1
            )));
        }
        let request = serde_json::from_str::<LocalAutomationRequest>(line).map_err(|error| {
            CliFailure::invalid(format!(
                "automation line {} is not a valid typed request: {error}",
                index + 1
            ))
        })?;
        let result =
            LocalProjectHost::execute_automation(&project, request, Arc::clone(&permissions))
                .map_err(|error| {
                    CliFailure::from_error("automation.run", error).contextualize(format!(
                        "automation line {} stopped after {} durable request(s)",
                        index + 1,
                        request_count
                    ))
                })?;
        let output = serde_json::to_vec(&result).map_err(|error| {
            CliFailure::stage(
                "automation.run",
                "internal",
                "terminal",
                format!("automation result serialization failed: {error}"),
            )
        })?;
        stdout.write_all(&output).map_err(|error| {
            CliFailure::unavailable(
                "automation.run",
                format!("automation response could not be written: {error}"),
            )
        })?;
        stdout.write_all(b"\n").map_err(|error| {
            CliFailure::unavailable(
                "automation.run",
                format!("automation response terminator could not be written: {error}"),
            )
        })?;
        stdout.flush().map_err(|error| {
            CliFailure::unavailable(
                "automation.run",
                format!("automation response could not be flushed: {error}"),
            )
        })?;
        request_count += 1;
    }
    if request_count == 0 {
        return Err(CliFailure::invalid(
            "automation input must contain at least one JSON request",
        ));
    }
    Ok(Vec::new())
}

fn read_permissions(path: Option<PathBuf>) -> Result<Arc<ApiPermissionContext>, CliFailure> {
    let Some(path) = path else {
        return Ok(Arc::new(ApiPermissionContext::default()));
    };
    let policy: PermissionPolicy = decode_json(
        &read_regular_file(&path, "permission policy")?,
        "permission policy",
    )?;
    ApiPermissionContext::new(policy.principal, policy.rules)
        .map(Arc::new)
        .map_err(|error| CliFailure::from_error("permissions.load", error))
}

fn read_json<T: DeserializeOwned>(
    source: InputSource,
    description: &'static str,
) -> Result<T, CliFailure> {
    decode_json(&read_input(source, description)?, description)
}

fn decode_json<T: DeserializeOwned>(
    bytes: &[u8],
    description: &'static str,
) -> Result<T, CliFailure> {
    serde_json::from_slice(bytes)
        .map_err(|error| CliFailure::invalid(format!("{description} is invalid JSON: {error}")))
}

fn read_input(source: InputSource, description: &'static str) -> Result<Vec<u8>, CliFailure> {
    match source {
        InputSource::Stdin => read_stdin(description),
        InputSource::File(path) => read_regular_file(&path, description),
    }
}

fn read_stdin(description: &'static str) -> Result<Vec<u8>, CliFailure> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(MAX_JSON_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            CliFailure::invalid(format!("{description} could not be read: {error}"))
        })?;
    if bytes.len() as u64 > MAX_JSON_BYTES {
        return Err(CliFailure::invalid(format!(
            "{description} exceeds the byte bound"
        )));
    }
    Ok(bytes)
}

fn read_regular_file(path: &Path, description: &'static str) -> Result<Vec<u8>, CliFailure> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        CliFailure::invalid(format!(
            "{description} metadata could not be read at {}: {error}",
            path.display()
        ))
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(CliFailure::invalid(format!(
            "{description} must be a non-symlink regular file"
        )));
    }
    if metadata.len() > MAX_JSON_BYTES {
        return Err(CliFailure::invalid(format!(
            "{description} exceeds the byte bound"
        )));
    }
    fs::read(path).map_err(|error| {
        CliFailure::invalid(format!(
            "{description} could not be read at {}: {error}",
            path.display()
        ))
    })
}

fn one_core<T: Serialize>(
    stage: &'static str,
    result: CoreResult<T>,
) -> Result<Vec<Value>, CliFailure> {
    let value = result.map_err(|error| CliFailure::from_error(stage, error))?;
    one_value(stage, value)
}

fn one_value<T: Serialize>(stage: &'static str, value: T) -> Result<Vec<Value>, CliFailure> {
    Ok(vec![to_value(stage, value)?])
}

fn to_value<T: Serialize>(stage: &'static str, value: T) -> Result<Value, CliFailure> {
    serde_json::to_value(value).map_err(|error| {
        CliFailure::stage(
            stage,
            "internal",
            "terminal",
            format!("workflow result serialization failed: {error}"),
        )
    })
}

struct Options {
    values: BTreeMap<String, OsString>,
}

impl Options {
    fn parse(arguments: &[OsString]) -> Result<Self, CliFailure> {
        if arguments.len() % 2 != 0 {
            return Err(CliFailure::invalid(
                "every workflow option requires exactly one value",
            ));
        }
        let mut values = BTreeMap::new();
        for pair in arguments.chunks_exact(2) {
            let key = pair[0]
                .to_str()
                .filter(|value| value.starts_with("--"))
                .ok_or_else(|| CliFailure::invalid("workflow option names must begin with --"))?;
            if values.insert(key.to_owned(), pair[1].clone()).is_some() {
                return Err(CliFailure::invalid(format!(
                    "workflow option {key} was provided more than once"
                )));
            }
        }
        Ok(Self { values })
    }

    fn required_path(&mut self, key: &'static str) -> Result<PathBuf, CliFailure> {
        let value = self.required(key)?;
        if value.is_empty() {
            return Err(CliFailure::invalid(format!("{key} must not be empty")));
        }
        Ok(PathBuf::from(value))
    }

    fn required_input(&mut self, key: &'static str) -> Result<InputSource, CliFailure> {
        let value = self.required(key)?;
        if value == OsStr::new("-") {
            Ok(InputSource::Stdin)
        } else if value.is_empty() {
            Err(CliFailure::invalid(format!("{key} must not be empty")))
        } else {
            Ok(InputSource::File(PathBuf::from(value)))
        }
    }

    fn required_utf8(&mut self, key: &'static str) -> Result<String, CliFailure> {
        self.required(key)?
            .into_string()
            .map_err(|_| CliFailure::invalid(format!("{key} must be valid UTF-8")))
    }

    fn required_u64(&mut self, key: &'static str) -> Result<u64, CliFailure> {
        self.required_utf8(key)?
            .parse()
            .map_err(|_| CliFailure::invalid(format!("{key} must be an unsigned 64-bit integer")))
    }

    fn required_u32(&mut self, key: &'static str) -> Result<u32, CliFailure> {
        self.required_utf8(key)?
            .parse()
            .map_err(|_| CliFailure::invalid(format!("{key} must be an unsigned 32-bit integer")))
    }

    fn optional_regular_path(&mut self, key: &'static str) -> Result<Option<PathBuf>, CliFailure> {
        let Some(value) = self.values.remove(key) else {
            return Ok(None);
        };
        if value.is_empty() || value == OsStr::new("-") {
            return Err(CliFailure::invalid(format!(
                "{key} must name a regular file and cannot use stdin"
            )));
        }
        Ok(Some(PathBuf::from(value)))
    }

    fn required(&mut self, key: &'static str) -> Result<OsString, CliFailure> {
        self.values
            .remove(key)
            .ok_or_else(|| CliFailure::invalid(format!("missing required option {key}")))
    }

    fn finish(self) -> Result<(), CliFailure> {
        if let Some(key) = self.values.keys().next() {
            return Err(CliFailure::invalid(format!(
                "unrecognized workflow option {key}"
            )));
        }
        Ok(())
    }
}
