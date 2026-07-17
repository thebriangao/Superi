//! Bounded deterministic local scripting over the stable public project API.

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::SemanticVersion;
use superi_engine::editor::EngineTransactionId;

use crate::commands::{ApiCommand, GetEditorState, GetEditorStateResult};
use crate::editor::{
    ExecuteProjectCommand, ExecuteProjectCommandResult, GetProjectCommandLog,
    GetProjectCommandLogResult, ProjectEditorApi,
};
use crate::permissions::{
    ApiPermissionKind, ApiPermissionRequirementMode, ApiPermissionRequirements,
};
use crate::schema::{
    PublicApiError, PublicErrorContext, PublicMethodKind, PublicResourceReference,
};
use crate::version::EDITOR_STATE_SCHEMA_VERSION;

pub use crate::version::{RUN_PROJECT_SCRIPT_METHOD, SCRIPTING_SCHEMA_VERSION};

const COMPONENT: &str = "superi.api.scripting";

/// Stable local script language identity.
pub const SCRIPT_LANGUAGE: &str = "superi-json";
/// Conventional filename suffix for local script source.
pub const SCRIPT_FILE_EXTENSION: &str = ".superi-script.json";
/// Maximum exact UTF-8 script source size.
pub const MAX_SCRIPT_SOURCE_BYTES: usize = 1_048_576;
/// Maximum number of ordered steps in one program.
pub const MAX_SCRIPT_STEPS: usize = 256;
/// Maximum UTF-8 bytes in one script identity.
pub const MAX_SCRIPT_IDENTIFIER_BYTES: usize = 128;
/// Maximum JSON value nesting depth accepted by the runtime.
pub const MAX_SCRIPT_JSON_DEPTH: usize = 128;

/// Validated canonical identity for one local script.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ScriptId(String);

impl ScriptId {
    /// Creates one bounded lowercase ASCII script identity.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_script_id(&value)?;
        Ok(Self(value))
    }

    /// Returns the canonical script identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ScriptId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// Closed supported step vocabulary for version 1 of `superi-json`.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", deny_unknown_fields)]
pub enum ProjectScriptStep {
    /// Runs one existing typed project command through [`ProjectEditorApi`].
    #[serde(rename = "superi.project.command.execute")]
    ExecuteProjectCommand { params: ExecuteProjectCommand },
    /// Reads one complete existing editor replacement snapshot.
    #[serde(rename = "superi.editor.state.get")]
    GetEditorState { params: GetEditorState },
    /// Reads one bounded cursor-safe batch from the durable project command log.
    #[serde(rename = "superi.project.command_log.get")]
    GetProjectCommandLog { params: GetProjectCommandLog },
}

impl ProjectScriptStep {
    fn validate(&self) -> Result<()> {
        match self {
            Self::ExecuteProjectCommand { params } => params.validate_for_script(),
            Self::GetEditorState { params } => {
                EngineTransactionId::new(params.transaction_id().to_owned()).map(|_| ())
            }
            Self::GetProjectCommandLog { params } => params.validate_for_script(),
        }
    }

    fn append_permission_requirements(
        &self,
        requirements: &mut Vec<crate::permissions::ApiPermissionRequirement>,
    ) -> Result<()> {
        if let Self::ExecuteProjectCommand { params } = self {
            requirements.extend(params.permission_requirements()?.as_slice().iter().cloned());
        }
        Ok(())
    }
}

/// Complete strict source document for `superi-json` version 1.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectScriptProgram {
    language: String,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::SemanticVersionBinding)
    )]
    language_version: SemanticVersion,
    script_id: ScriptId,
    expected_initial_project_revision: u64,
    steps: Vec<ProjectScriptStep>,
}

impl ProjectScriptProgram {
    /// Returns the stable local language identity.
    #[must_use]
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Returns the exact language contract version.
    #[must_use]
    pub const fn language_version(&self) -> &SemanticVersion {
        &self.language_version
    }

    /// Returns the caller-owned script identity.
    #[must_use]
    pub const fn script_id(&self) -> &ScriptId {
        &self.script_id
    }

    /// Returns the required initial project revision fence.
    #[must_use]
    pub const fn expected_initial_project_revision(&self) -> u64 {
        self.expected_initial_project_revision
    }

    /// Returns steps in exact source order.
    #[must_use]
    pub fn steps(&self) -> &[ProjectScriptStep] {
        &self.steps
    }

    fn validate(&self) -> Result<()> {
        if self.language != SCRIPT_LANGUAGE || self.language_version != SCRIPTING_SCHEMA_VERSION {
            return Err(script_error(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "validate_language",
                "script language identity or version is unsupported",
            ));
        }
        if self.steps.is_empty() {
            return Err(script_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_steps",
                "script must contain at least one step",
            ));
        }
        if self.steps.len() > MAX_SCRIPT_STEPS {
            return Err(script_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "validate_steps",
                "script step count exceeds the stable bound",
            ));
        }
        for step in &self.steps {
            step.validate()?;
        }
        Ok(())
    }

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        let mut requirements = Vec::new();
        for step in &self.steps {
            step.append_permission_requirements(&mut requirements)?;
        }
        ApiPermissionRequirements::new(requirements)
    }
}

/// Request to validate and run exact local script source.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunProjectScript {
    source: String,
    expected_source_sha256: String,
}

impl RunProjectScript {
    /// Creates one exact-source script request.
    #[must_use]
    pub fn new(source: impl Into<String>, expected_source_sha256: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            expected_source_sha256: expected_source_sha256.into(),
        }
    }

    /// Returns exact UTF-8 source bytes as text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the caller's required lowercase SHA-256 digest.
    #[must_use]
    pub fn expected_source_sha256(&self) -> &str {
        &self.expected_source_sha256
    }
}

impl ApiCommand for RunProjectScript {
    type Response = RunProjectScriptResult;

    const METHOD: &'static str = RUN_PROJECT_SCRIPT_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = SCRIPTING_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode =
        ApiPermissionRequirementMode::PayloadDependent;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] =
        &[ApiPermissionKind::Filesystem, ApiPermissionKind::Plugin];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        let parsed = parse_request(self)?;
        parsed.program.permission_requirements()
    }
}

/// Typed successful response from one supported script step.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "response")]
pub enum ProjectScriptStepResponse {
    /// Result from the existing generic project command surface.
    #[serde(rename = "superi.project.command.execute")]
    ExecuteProjectCommand(Box<ExecuteProjectCommandResult>),
    /// Result from the existing complete editor-state query.
    #[serde(rename = "superi.editor.state.get")]
    GetEditorState(Box<GetEditorStateResult>),
    /// Result from the durable project command-log query.
    #[serde(rename = "superi.project.command_log.get")]
    GetProjectCommandLog(Box<GetProjectCommandLogResult>),
}

/// One ordered completed step and its typed public response.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectScriptStepRecord {
    index: u32,
    response: ProjectScriptStepResponse,
}

impl ProjectScriptStepRecord {
    /// Returns the zero-based source step index.
    #[must_use]
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// Returns the typed response from the existing public method.
    #[must_use]
    pub const fn response(&self) -> &ProjectScriptStepResponse {
        &self.response
    }
}

/// Final deterministic interpretation of one script run.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectScriptRunStatus {
    /// Every source step completed.
    Completed,
    /// The initial project revision fence rejected all steps.
    Rejected,
    /// A later step failed after zero or more completed steps.
    Stopped,
}

/// Complete deterministic trace from one local script execution.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunProjectScriptResult {
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::SemanticVersionBinding)
    )]
    runtime_schema_version: SemanticVersion,
    language: String,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::SemanticVersionBinding)
    )]
    language_version: SemanticVersion,
    script_id: ScriptId,
    source_sha256: String,
    project_id: String,
    initial_project_revision: u64,
    initial_project_semantic_hash: String,
    final_project_revision: u64,
    final_project_semantic_hash: String,
    status: ProjectScriptRunStatus,
    completed_steps: Vec<ProjectScriptStepRecord>,
    failed_step_index: Option<u32>,
    failure: Option<PublicApiError>,
    effects_committed: bool,
}

impl RunProjectScriptResult {
    /// Returns the local runtime schema version.
    #[must_use]
    pub const fn runtime_schema_version(&self) -> &SemanticVersion {
        &self.runtime_schema_version
    }

    /// Returns the stable local language identity.
    #[must_use]
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Returns the exact interpreted language version.
    #[must_use]
    pub const fn language_version(&self) -> &SemanticVersion {
        &self.language_version
    }

    /// Returns the caller-owned script identity.
    #[must_use]
    pub const fn script_id(&self) -> &ScriptId {
        &self.script_id
    }

    /// Returns the lowercase SHA-256 of the exact UTF-8 source.
    #[must_use]
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    /// Returns the stable affected project identity.
    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Returns the observed project revision before any source step.
    #[must_use]
    pub const fn initial_project_revision(&self) -> u64 {
        self.initial_project_revision
    }

    /// Returns the semantic project hash before any source step.
    #[must_use]
    pub fn initial_project_semantic_hash(&self) -> &str {
        &self.initial_project_semantic_hash
    }

    /// Returns the observed project revision after execution stopped.
    #[must_use]
    pub const fn final_project_revision(&self) -> u64 {
        self.final_project_revision
    }

    /// Returns the semantic project hash after execution stopped.
    #[must_use]
    pub fn final_project_semantic_hash(&self) -> &str {
        &self.final_project_semantic_hash
    }

    /// Returns the final execution status.
    #[must_use]
    pub const fn status(&self) -> ProjectScriptRunStatus {
        self.status
    }

    /// Returns every completed step in exact source order.
    #[must_use]
    pub fn completed_steps(&self) -> &[ProjectScriptStepRecord] {
        &self.completed_steps
    }

    /// Returns the first failed zero-based source index, if execution reached a step failure.
    #[must_use]
    pub const fn failed_step_index(&self) -> Option<u32> {
        self.failed_step_index
    }

    /// Returns the structured user-safe failure, if execution was rejected or stopped.
    #[must_use]
    pub const fn failure(&self) -> Option<&PublicApiError> {
        self.failure.as_ref()
    }

    /// Returns whether the final authored revision differs from the initial revision.
    #[must_use]
    pub const fn effects_committed(&self) -> bool {
        self.effects_committed
    }
}

struct ParsedRequest {
    program: ProjectScriptProgram,
    source_sha256: String,
}

fn parse_request(request: &RunProjectScript) -> Result<ParsedRequest> {
    if request.source.len() > MAX_SCRIPT_SOURCE_BYTES {
        return Err(script_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            "validate_source_size",
            "script source exceeds the stable byte bound",
        ));
    }
    validate_digest(&request.expected_source_sha256)?;
    let source_sha256 = format!("{:x}", Sha256::digest(request.source.as_bytes()));
    if source_sha256 != request.expected_source_sha256 {
        return Err(script_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "validate_source_digest",
            "script source does not match its required SHA-256 digest",
        ));
    }

    validate_json_structure(&request.source)?;
    let program: ProjectScriptProgram =
        serde_json::from_str(&request.source).map_err(|source| {
            Error::with_source(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "script source does not match the supported strict program schema",
                source,
            )
            .with_context(ErrorContext::new(COMPONENT, "decode_program"))
        })?;
    program.validate()?;
    Ok(ParsedRequest {
        program,
        source_sha256,
    })
}

pub(crate) fn execute_project_script(
    api: &mut ProjectEditorApi,
    request: RunProjectScript,
) -> Result<RunProjectScriptResult> {
    let ParsedRequest {
        program,
        source_sha256,
    } = parse_request(&request)?;
    let initial = api.execute(GetEditorState::new(format!(
        "script:{}:state:initial",
        program.script_id.as_str()
    )))?;
    let initial_project = initial.snapshot().project();
    let project_id = initial_project.project_id().to_owned();
    let initial_project_revision = initial_project.project_revision();
    let initial_project_semantic_hash = initial_project.semantic_hash().to_owned();

    if initial_project_revision != program.expected_initial_project_revision {
        let failure = public_failure(
            &script_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "validate_initial_revision",
                "script initial project revision does not match current state",
            ),
            "superi.project.script.validate-initial-revision",
            &project_id,
            initial_project_revision,
        )?;
        return Ok(RunProjectScriptResult {
            runtime_schema_version: SCRIPTING_SCHEMA_VERSION,
            language: SCRIPT_LANGUAGE.to_owned(),
            language_version: program.language_version,
            script_id: program.script_id,
            source_sha256,
            project_id,
            initial_project_revision,
            initial_project_semantic_hash: initial_project_semantic_hash.clone(),
            final_project_revision: initial_project_revision,
            final_project_semantic_hash: initial_project_semantic_hash,
            status: ProjectScriptRunStatus::Rejected,
            completed_steps: Vec::new(),
            failed_step_index: None,
            failure: Some(failure),
            effects_committed: false,
        });
    }

    let language_version = program.language_version;
    let script_id = program.script_id;
    let mut completed_steps = Vec::with_capacity(program.steps.len());
    let mut failed = None;
    for (index, step) in program.steps.into_iter().enumerate() {
        let response = match step {
            ProjectScriptStep::ExecuteProjectCommand { params } => {
                api.execute(params).map(|response| {
                    ProjectScriptStepResponse::ExecuteProjectCommand(Box::new(response))
                })
            }
            ProjectScriptStep::GetEditorState { params } => api
                .execute(params)
                .map(|response| ProjectScriptStepResponse::GetEditorState(Box::new(response))),
            ProjectScriptStep::GetProjectCommandLog { params } => {
                api.execute(params).map(|response| {
                    ProjectScriptStepResponse::GetProjectCommandLog(Box::new(response))
                })
            }
        };
        match response {
            Ok(response) => completed_steps.push(ProjectScriptStepRecord {
                index: u32::try_from(index).expect("script step count is bounded by u32"),
                response,
            }),
            Err(error) => {
                failed = Some((
                    u32::try_from(index).expect("script step count is bounded by u32"),
                    error,
                ));
                break;
            }
        }
    }

    let final_state = api.execute(GetEditorState::new(format!(
        "script:{}:state:final",
        script_id.as_str()
    )))?;
    let final_project = final_state.snapshot().project();
    let final_project_revision = final_project.project_revision();
    let final_project_semantic_hash = final_project.semantic_hash().to_owned();
    let effects_committed = final_project_revision != initial_project_revision;
    let (status, failed_step_index, failure) = match failed {
        Some((index, error)) => (
            ProjectScriptRunStatus::Stopped,
            Some(index),
            Some(public_failure(
                &error,
                "superi.project.script.execute-step",
                &project_id,
                final_project_revision,
            )?),
        ),
        None => (ProjectScriptRunStatus::Completed, None, None),
    };

    Ok(RunProjectScriptResult {
        runtime_schema_version: SCRIPTING_SCHEMA_VERSION,
        language: SCRIPT_LANGUAGE.to_owned(),
        language_version,
        script_id,
        source_sha256,
        project_id,
        initial_project_revision,
        initial_project_semantic_hash,
        final_project_revision,
        final_project_semantic_hash,
        status,
        completed_steps,
        failed_step_index,
        failure,
        effects_committed,
    })
}

fn public_failure(
    error: &Error,
    operation: &'static str,
    project_id: &str,
    project_revision: u64,
) -> Result<PublicApiError> {
    let context = PublicErrorContext::reviewed(COMPONENT, operation)?;
    let resource = PublicResourceReference::new(
        "superi.editor.state",
        EDITOR_STATE_SCHEMA_VERSION,
        project_id,
        project_revision,
    )?;
    PublicApiError::from_error(error, vec![context], Some(resource))
}

fn validate_script_id(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.len() > MAX_SCRIPT_IDENTIFIER_BYTES
        || !bytes[0].is_ascii_lowercase()
        || bytes.iter().copied().any(|byte| {
            !(byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-' | b':'))
        })
    {
        return Err(script_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "validate_script_id",
            "script identity must use bounded canonical lowercase ASCII",
        ));
    }
    Ok(())
}

fn validate_digest(value: &str) -> Result<()> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    {
        return Err(script_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "validate_source_digest",
            "script source digest must be 64 lowercase hexadecimal characters",
        ));
    }
    Ok(())
}

fn validate_json_structure(source: &str) -> Result<()> {
    let mut deserializer = serde_json::Deserializer::from_str(source);
    JsonStructureSeed { depth: 1 }
        .deserialize(&mut deserializer)
        .and_then(|()| deserializer.end())
        .map_err(|source| {
            Error::with_source(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "script source is not bounded duplicate-free JSON",
                source,
            )
            .with_context(ErrorContext::new(COMPONENT, "validate_json_structure"))
        })
}

#[derive(Clone, Copy)]
struct JsonStructureSeed {
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for JsonStructureSeed {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        if self.depth > MAX_SCRIPT_JSON_DEPTH {
            return Err(D::Error::custom(
                "script JSON exceeds the maximum nesting depth",
            ));
        }
        deserializer.deserialize_any(JsonStructureVisitor { depth: self.depth })
    }
}

struct JsonStructureVisitor {
    depth: usize,
}

impl<'de> Visitor<'de> for JsonStructureVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bounded duplicate-free JSON")
    }

    fn visit_bool<E>(self, _value: bool) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_string<E>(self, _value: String) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence
            .next_element_seed(JsonStructureSeed {
                depth: self.depth + 1,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut names = BTreeSet::new();
        while let Some(name) = map.next_key::<String>()? {
            if !names.insert(name) {
                return Err(A::Error::custom(
                    "script JSON contains a duplicate object name",
                ));
            }
            map.next_value_seed(JsonStructureSeed {
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
}

fn script_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
