//! Stable typed public control of the durable authored project aggregate.

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::SemanticVersion;
use superi_engine::editor as engine;

use crate::commands::{ApiCommand, GetEditorState};
use crate::events::ProjectStateChanged;
use crate::permissions::{
    ApiFilesystemAccess, ApiFilesystemPath, ApiFilesystemPlatform, ApiPermissionContext,
    ApiPermissionKind, ApiPermissionRequirement, ApiPermissionRequirementMode,
    ApiPermissionRequirements, ApiPluginOperation,
};
use crate::schema::{ApiResource, PublicMethodKind};
use crate::scripting::RunProjectScript;
use crate::version::{
    EXECUTE_PROJECT_COMMAND_METHOD, GET_PROJECT_COMMAND_LOG_METHOD, PROJECT_COMMAND_LOG_RESOURCE,
    PROJECT_COMMAND_LOG_SCHEMA_VERSION, PROJECT_EDITOR_SCHEMA_VERSION, PROJECT_HISTORY_RESOURCE,
};

const COMPONENT: &str = "superi-api.editor";
const MAX_PROJECT_COMMAND_LOG_QUERY_RECORDS: u32 = 256;

/// Maximum number of public project actions in one atomic apply command.
pub const MAX_PROJECT_ACTIONS: usize = engine::MAX_COMPOUND_PROJECT_ACTIONS;

/// Stable command classification returned after public project execution.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectCommandKind {
    Apply,
    Undo,
    Redo,
    Inspect,
}

/// One generic project command on the stable public surface.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectCommand {
    /// Apply one bounded ordered compound authored transaction.
    Apply { actions: Vec<ProjectAction> },
    /// Restore the most recent retained before-state.
    Undo {},
    /// Restore the most recent retained after-state.
    Redo {},
    /// Inspect current project history state without mutation or an event.
    Inspect {},
}

impl ProjectCommand {
    #[must_use]
    pub const fn kind(&self) -> ProjectCommandKind {
        match self {
            Self::Apply { .. } => ProjectCommandKind::Apply,
            Self::Undo {} => ProjectCommandKind::Undo,
            Self::Redo {} => ProjectCommandKind::Redo,
            Self::Inspect {} => ProjectCommandKind::Inspect,
        }
    }
}

/// Request for one generic public project command.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteProjectCommand {
    transaction_id: String,
    expected_project_revision: u64,
    command: ProjectCommand,
}

impl ExecuteProjectCommand {
    #[must_use]
    pub fn new(
        transaction_id: impl Into<String>,
        expected_project_revision: u64,
        command: ProjectCommand,
    ) -> Self {
        Self {
            transaction_id: transaction_id.into(),
            expected_project_revision,
            command,
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn expected_project_revision(&self) -> u64 {
        self.expected_project_revision
    }

    #[must_use]
    pub const fn command(&self) -> &ProjectCommand {
        &self.command
    }

    fn into_parts(self) -> (String, u64, ProjectCommand) {
        (
            self.transaction_id,
            self.expected_project_revision,
            self.command,
        )
    }

    pub(crate) fn validate_for_script(&self) -> Result<()> {
        let (transaction_id, _, command) = self.clone().into_parts();
        let _ = engine::EngineTransactionId::new(transaction_id)?;
        if let ProjectCommand::Apply { actions } = command {
            let actions = actions
                .into_iter()
                .map(ProjectAction::into_engine)
                .collect::<Result<Vec<_>>>()?;
            let _ = engine::CompoundProjectTransaction::new(actions)?;
        }
        Ok(())
    }
}

impl ApiCommand for ExecuteProjectCommand {
    type Response = ExecuteProjectCommandResult;
    const METHOD: &'static str = EXECUTE_PROJECT_COMMAND_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_EDITOR_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode =
        ApiPermissionRequirementMode::PayloadDependent;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] =
        &[ApiPermissionKind::Filesystem, ApiPermissionKind::Plugin];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        let ProjectCommand::Apply { actions } = &self.command else {
            return Ok(ApiPermissionRequirements::none());
        };
        let mut requirements = Vec::new();
        for action in actions {
            action.append_permission_requirements(&mut requirements)?;
        }
        ApiPermissionRequirements::new(requirements)
    }
}

/// Stable typed mutation category retained by project history.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectMutationKind {
    ProjectSettings,
    Compound,
    SetMediaPath,
    MarkMediaMissing,
    ConsiderMediaRelink,
    UpsertExtension,
    RemoveExtension,
    SetExtensionLifecycle,
    SetExtensionCapabilities,
    RecordExtensionFailure,
    ClearExtensionFailure,
    ExtensionState,
    Unknown,
}

/// Minimum complete replacement state owned by the generic history command surface.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectHistorySnapshot {
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    schema_version: SemanticVersion,
    project_id: String,
    project_revision: u64,
    root_timeline_id: String,
    capacity: usize,
    undo_depth: usize,
    redo_depth: usize,
    next_undo: Option<ProjectMutationKind>,
    next_redo: Option<ProjectMutationKind>,
    command_log_oldest_sequence: Option<u64>,
    command_log_latest_sequence: u64,
    command_log_retained_records: usize,
}

impl ProjectHistorySnapshot {
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub fn root_timeline_id(&self) -> &str {
        &self.root_timeline_id
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub const fn undo_depth(&self) -> usize {
        self.undo_depth
    }

    #[must_use]
    pub const fn redo_depth(&self) -> usize {
        self.redo_depth
    }

    #[must_use]
    pub const fn next_undo(&self) -> Option<ProjectMutationKind> {
        self.next_undo
    }

    #[must_use]
    pub const fn next_redo(&self) -> Option<ProjectMutationKind> {
        self.next_redo
    }

    #[must_use]
    pub const fn command_log_oldest_sequence(&self) -> Option<u64> {
        self.command_log_oldest_sequence
    }

    #[must_use]
    pub const fn command_log_latest_sequence(&self) -> u64 {
        self.command_log_latest_sequence
    }

    #[must_use]
    pub const fn command_log_retained_records(&self) -> usize {
        self.command_log_retained_records
    }
}

impl ApiResource for ProjectHistorySnapshot {
    const RESOURCE: &'static str = PROJECT_HISTORY_RESOURCE;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_EDITOR_SCHEMA_VERSION;
}

/// Semantic result from one media mutation.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MediaMutationResult {
    PathUpdated,
    MarkedMissing,
    RelinkAccepted,
    RelinkRejected,
    Unknown,
}

/// Stable extension result kind.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExtensionMutationResultKind {
    Upserted,
    Removed,
    LifecycleSet,
    GrantedCapabilitiesSet,
    FailureRecorded,
    FailureCleared,
    Unknown,
}

/// One action result inside an applied compound command.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectActionEvidence {
    RootTimelineSelected {
        timeline_id: String,
    },
    TimelineEdited {
        revision: u64,
        operations: Vec<TimelineEditKind>,
    },
    GraphMutated {
        graph_id: String,
        revision: u64,
    },
    MediaMutated {
        outcome: MediaMutationResult,
    },
    ClipMixMutated {
        revision: u64,
    },
    ExtensionMutated {
        outcome: ExtensionMutationResultKind,
        extension_id: String,
        record_id: String,
        replaced: Option<bool>,
    },
}

/// Stable command evidence returned by execution and attached to its matching event.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectCommandEvidence {
    Applied { actions: Vec<ProjectActionEvidence> },
    AppliedMutation { mutation: ProjectMutationKind },
    Undone { mutation: ProjectMutationKind },
    Redone { mutation: ProjectMutationKind },
    Inspected {},
}

/// Successful generic public project command result.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteProjectCommandResult {
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    schema_version: SemanticVersion,
    transaction_id: String,
    command_sequence: u64,
    command_kind: ProjectCommandKind,
    authored_state_changed: bool,
    command_log_sequence: u64,
    state: ProjectHistorySnapshot,
    evidence: ProjectCommandEvidence,
}

impl ExecuteProjectCommandResult {
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub const fn command_kind(&self) -> ProjectCommandKind {
        self.command_kind
    }

    #[must_use]
    pub const fn authored_state_changed(&self) -> bool {
        self.authored_state_changed
    }

    #[must_use]
    pub const fn command_log_sequence(&self) -> u64 {
        self.command_log_sequence
    }

    #[must_use]
    pub const fn state(&self) -> &ProjectHistorySnapshot {
        &self.state
    }

    #[must_use]
    pub const fn evidence(&self) -> &ProjectCommandEvidence {
        &self.evidence
    }
}

/// Requested detail level for bounded command-log inspection.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectCommandLogDetail {
    /// Return public-safe metadata only.
    Metadata,
    /// Return retained typed requests after reauthorizing each command.
    Replayable,
}

/// Whether exact replay request bytes remain retained for one command.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectCommandPayloadDisposition {
    Retained,
    DigestOnly,
}

/// Strict cursor query for durable project command records.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetProjectCommandLog {
    after_sequence: u64,
    requested_limit: u32,
    detail: ProjectCommandLogDetail,
}

impl GetProjectCommandLog {
    #[must_use]
    pub const fn new(
        after_sequence: u64,
        requested_limit: u32,
        detail: ProjectCommandLogDetail,
    ) -> Self {
        Self {
            after_sequence,
            requested_limit,
            detail,
        }
    }

    #[must_use]
    pub const fn after_sequence(&self) -> u64 {
        self.after_sequence
    }

    #[must_use]
    pub const fn requested_limit(&self) -> u32 {
        self.requested_limit
    }

    #[must_use]
    pub const fn detail(&self) -> ProjectCommandLogDetail {
        self.detail
    }

    pub(crate) fn validate_for_script(&self) -> Result<()> {
        validate_command_log_query(self)
    }
}

impl ApiCommand for GetProjectCommandLog {
    type Response = GetProjectCommandLogResult;
    const METHOD: &'static str = GET_PROJECT_COMMAND_LOG_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_COMMAND_LOG_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// One public-safe durable command record with optional reauthorized typed request.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCommandLogRecord {
    sequence: u64,
    command_sequence: u64,
    transaction_id: String,
    method: String,
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    request_schema_version: SemanticVersion,
    command_kind: ProjectCommandKind,
    expected_project_revision: u64,
    request_byte_length: u64,
    request_sha256: String,
    payload_disposition: ProjectCommandPayloadDisposition,
    before_project_revision: u64,
    after_project_revision: u64,
    before_semantic_hash: String,
    after_semantic_hash: String,
    authored_state_changed: bool,
    replay_request: Option<Box<ExecuteProjectCommand>>,
}

impl ProjectCommandLogRecord {
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn payload_disposition(&self) -> ProjectCommandPayloadDisposition {
        self.payload_disposition
    }

    #[must_use]
    pub fn replay_request(&self) -> Option<&ExecuteProjectCommand> {
        match &self.replay_request {
            Some(request) => Some(request.as_ref()),
            None => None,
        }
    }
}

/// One cursor-safe bounded command-log batch.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCommandLogBatch {
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    schema_version: SemanticVersion,
    project_id: String,
    after_sequence: u64,
    through_sequence: u64,
    oldest_available_sequence: Option<u64>,
    latest_sequence: u64,
    records: Vec<ProjectCommandLogRecord>,
}

impl ProjectCommandLogBatch {
    #[must_use]
    pub fn records(&self) -> &[ProjectCommandLogRecord] {
        &self.records
    }

    #[must_use]
    pub const fn through_sequence(&self) -> u64 {
        self.through_sequence
    }

    #[must_use]
    pub const fn latest_sequence(&self) -> u64 {
        self.latest_sequence
    }
}

impl ApiResource for ProjectCommandLogBatch {
    const RESOURCE: &'static str = PROJECT_COMMAND_LOG_RESOURCE;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_COMMAND_LOG_SCHEMA_VERSION;
}

/// Explicit command-log replay barrier after retained metadata was evicted.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCommandLogGap {
    project_id: String,
    requested_after_sequence: u64,
    oldest_available_sequence: u64,
    latest_sequence: u64,
}

/// Cursor query result that never hides an evicted prefix behind partial records.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "result", rename_all = "snake_case")]
pub enum GetProjectCommandLogResult {
    Records(ProjectCommandLogBatch),
    ResyncRequired(ProjectCommandLogGap),
}

/// Exact public timebase.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactTimebase {
    pub numerator: u32,
    pub denominator: u32,
}

impl ExactTimebase {
    fn into_engine(self) -> Result<engine::Timebase> {
        engine::Timebase::new(self.numerator, self.denominator)
    }
}

/// Exact public rational coordinate.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactTime {
    pub value: i64,
    pub timebase: ExactTimebase,
}

impl ExactTime {
    fn into_engine(self) -> Result<engine::RationalTime> {
        Ok(engine::RationalTime::new(
            self.value,
            self.timebase.into_engine()?,
        ))
    }
}

/// Exact public nonnegative duration.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactDuration {
    pub value: u64,
    pub timebase: ExactTimebase,
}

impl ExactDuration {
    fn into_engine(self) -> Result<engine::Duration> {
        engine::Duration::new(self.value, self.timebase.into_engine()?)
    }
}

/// Exact public half-open range.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactTimeRange {
    pub start: ExactTime,
    pub duration: ExactDuration,
}

impl ExactTimeRange {
    fn into_engine(self) -> Result<engine::TimeRange> {
        engine::TimeRange::new(self.start.into_engine()?, self.duration.into_engine()?)
    }
}

/// Stable identity for any timed editorial object.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorialObjectId {
    Clip { id: String },
    Gap { id: String },
    Transition { id: String },
    Generator { id: String },
    Caption { id: String },
}

impl EditorialObjectId {
    fn into_engine(self) -> Result<engine::EditorialObjectId> {
        match self {
            Self::Clip { id } => Ok(engine::EditorialObjectId::Clip(parse_id(&id, "clip_id")?)),
            Self::Gap { id } => Ok(engine::EditorialObjectId::Gap(parse_id(&id, "gap_id")?)),
            Self::Transition { id } => Ok(engine::EditorialObjectId::Transition(parse_id(
                &id,
                "transition_id",
            )?)),
            Self::Generator { id } => Ok(engine::EditorialObjectId::Generator(parse_id(
                &id,
                "generator_id",
            )?)),
            Self::Caption { id } => Ok(engine::EditorialObjectId::Caption(parse_id(
                &id,
                "caption_id",
            )?)),
        }
    }
}

/// Source relationship for a public clip value.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorClipSource {
    Media { media_id: String },
    Timeline { timeline_id: String },
}

impl EditorClipSource {
    fn into_engine(self) -> Result<engine::ClipSource> {
        match self {
            Self::Media { media_id } => {
                Ok(engine::ClipSource::Media(parse_id(&media_id, "media_id")?))
            }
            Self::Timeline { timeline_id } => Ok(engine::ClipSource::Timeline(parse_id(
                &timeline_id,
                "timeline_id",
            )?)),
        }
    }
}

/// One exact piecewise-linear retime segment.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorRetimeSegment {
    pub record_range: ExactTimeRange,
    pub source_start: ExactTime,
    pub rate_numerator: i64,
    pub rate_denominator: u64,
}

impl EditorRetimeSegment {
    fn into_engine(self) -> Result<engine::RetimeSegment> {
        engine::RetimeSegment::new(
            self.record_range.into_engine()?,
            self.source_start.into_engine()?,
            engine::PlaybackRate::new(self.rate_numerator, self.rate_denominator)?,
        )
    }
}

/// Complete clip-local record-to-source timing map.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorClipTimeMap {
    pub record_duration: ExactDuration,
    pub source_timebase: ExactTimebase,
    pub segments: Vec<EditorRetimeSegment>,
}

impl EditorClipTimeMap {
    fn into_engine(self) -> Result<engine::ClipTimeMap> {
        let duration = self.record_duration.into_engine()?;
        let source_timebase = self.source_timebase.into_engine()?;
        let segments = self
            .segments
            .into_iter()
            .map(EditorRetimeSegment::into_engine)
            .collect::<Result<Vec<_>>>()?;
        engine::ClipTimeMap::new(duration, source_timebase, segments)
    }
}

/// Complete typed material accepted by timeline edit operations.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorTrackItem {
    Clip {
        id: String,
        name: String,
        source: EditorClipSource,
        source_range: ExactTimeRange,
        record_range: ExactTimeRange,
        time_map: EditorClipTimeMap,
    },
    Gap {
        id: String,
        name: String,
        record_range: ExactTimeRange,
    },
    Transition {
        id: String,
        name: String,
        from: EditorialObjectId,
        to: EditorialObjectId,
        from_offset: ExactDuration,
        to_offset: ExactDuration,
    },
    Generator {
        id: String,
        name: String,
        generator_kind: String,
        parameters: BTreeMap<String, String>,
        record_range: ExactTimeRange,
    },
    Caption {
        id: String,
        name: String,
        text: String,
        language: Option<String>,
        record_range: ExactTimeRange,
    },
}

impl EditorTrackItem {
    fn into_engine(self) -> Result<engine::TrackItem> {
        match self {
            Self::Clip {
                id,
                name,
                source,
                source_range,
                record_range,
                time_map,
            } => {
                let mut clip = engine::Clip::new(
                    parse_id(&id, "clip_id")?,
                    name,
                    source.into_engine()?,
                    source_range.into_engine()?,
                    record_range.into_engine()?,
                )?;
                clip.set_time_map(time_map.into_engine()?)?;
                Ok(engine::TrackItem::Clip(clip))
            }
            Self::Gap {
                id,
                name,
                record_range,
            } => Ok(engine::TrackItem::Gap(engine::Gap::new(
                parse_id(&id, "gap_id")?,
                name,
                record_range.into_engine()?,
            ))),
            Self::Transition {
                id,
                name,
                from,
                to,
                from_offset,
                to_offset,
            } => Ok(engine::TrackItem::Transition(engine::Transition::new(
                parse_id(&id, "transition_id")?,
                name,
                from.into_engine()?,
                to.into_engine()?,
                from_offset.into_engine()?,
                to_offset.into_engine()?,
            ))),
            Self::Generator {
                id,
                name,
                generator_kind,
                parameters,
                record_range,
            } => Ok(engine::TrackItem::Generator(engine::Generator::new(
                parse_id(&id, "generator_id")?,
                name,
                generator_kind,
                parameters,
                record_range.into_engine()?,
            ))),
            Self::Caption {
                id,
                name,
                text,
                language,
                record_range,
            } => Ok(engine::TrackItem::Caption(engine::Caption::new(
                parse_id(&id, "caption_id")?,
                name,
                text,
                language,
                record_range.into_engine()?,
            ))),
        }
    }
}

/// Public edit-edge selection.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorEditSide {
    Start,
    End,
}

impl EditorEditSide {
    const fn into_engine(self) -> engine::EditSide {
        match self {
            Self::Start => engine::EditSide::Start,
            Self::End => engine::EditSide::End,
        }
    }
}

/// Public extend behavior.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorExtendMode {
    Ripple,
    Roll,
}

impl EditorExtendMode {
    const fn into_engine(self) -> engine::ExtendMode {
        match self {
            Self::Ripple => engine::ExtendMode::Ripple,
            Self::Roll => engine::ExtendMode::Roll,
        }
    }
}

/// Companion-track identities for synchronized ripple and extend behavior.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorRippleSyncAdjustment {
    pub track_id: String,
    pub gap_id: String,
    pub fragment_ids: Vec<EditorialObjectId>,
}

impl EditorRippleSyncAdjustment {
    fn into_engine(self) -> Result<engine::RippleSyncAdjustment> {
        Ok(engine::RippleSyncAdjustment::new(
            parse_id(&self.track_id, "track_id")?,
            parse_id(&self.gap_id, "gap_id")?,
            self.fragment_ids
                .into_iter()
                .map(EditorialObjectId::into_engine)
                .collect::<Result<Vec<_>>>()?,
        ))
    }
}

/// One of the four exact three-point placement rules.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "placement", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorThreePointPlacement {
    SourceRangeAtRecordStart {
        source_range: ExactTimeRange,
        record_start: ExactTime,
    },
    SourceStartOverRecordRange {
        source_start: ExactTime,
        record_range: ExactTimeRange,
    },
    SourceRangeBacktimedToRecordEnd {
        source_range: ExactTimeRange,
        record_end: ExactTime,
    },
    SourceEndBacktimedOverRecordRange {
        source_end: ExactTime,
        record_range: ExactTimeRange,
    },
}

impl EditorThreePointPlacement {
    fn into_engine(self) -> Result<engine::ThreePointPlacement> {
        match self {
            Self::SourceRangeAtRecordStart {
                source_range,
                record_start,
            } => Ok(engine::ThreePointPlacement::SourceRangeAtRecordStart {
                source_range: source_range.into_engine()?,
                record_start: record_start.into_engine()?,
            }),
            Self::SourceStartOverRecordRange {
                source_start,
                record_range,
            } => Ok(engine::ThreePointPlacement::SourceStartOverRecordRange {
                source_start: source_start.into_engine()?,
                record_range: record_range.into_engine()?,
            }),
            Self::SourceRangeBacktimedToRecordEnd {
                source_range,
                record_end,
            } => Ok(
                engine::ThreePointPlacement::SourceRangeBacktimedToRecordEnd {
                    source_range: source_range.into_engine()?,
                    record_end: record_end.into_engine()?,
                },
            ),
            Self::SourceEndBacktimedOverRecordRange {
                source_end,
                record_range,
            } => Ok(
                engine::ThreePointPlacement::SourceEndBacktimedOverRecordRange {
                    source_end: source_end.into_engine()?,
                    record_range: record_range.into_engine()?,
                },
            ),
        }
    }
}

/// Stable public timeline operation kind.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TimelineEditKind {
    Insert,
    Overwrite,
    Append,
    Replace,
    Lift,
    Extract,
    Ripple,
    Roll,
    Slip,
    Slide,
    Razor,
    Trim,
    Extend,
    ThreePoint,
    FourPoint,
}

/// Every current directly executable timeline edit operation.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimelineEditOperation {
    Insert {
        timeline_id: String,
        track_id: String,
        at: ExactTime,
        material: EditorTrackItem,
        fragment_ids: Vec<EditorialObjectId>,
    },
    Overwrite {
        timeline_id: String,
        track_id: String,
        at: ExactTime,
        material: EditorTrackItem,
        fragment_ids: Vec<EditorialObjectId>,
    },
    Append {
        timeline_id: String,
        track_id: String,
        material: EditorTrackItem,
    },
    Replace {
        timeline_id: String,
        track_id: String,
        target_id: EditorialObjectId,
        material: EditorTrackItem,
    },
    Lift {
        timeline_id: String,
        track_id: String,
        range: ExactTimeRange,
        gap_id: String,
        gap_name: String,
        fragment_ids: Vec<EditorialObjectId>,
    },
    Extract {
        timeline_id: String,
        track_id: String,
        range: ExactTimeRange,
        fragment_ids: Vec<EditorialObjectId>,
    },
    Ripple {
        timeline_id: String,
        track_id: String,
        target_id: EditorialObjectId,
        side: EditorEditSide,
        to: ExactTime,
        sync_adjustments: Vec<EditorRippleSyncAdjustment>,
    },
    Roll {
        timeline_id: String,
        track_id: String,
        left_id: EditorialObjectId,
        right_id: EditorialObjectId,
        to: ExactTime,
    },
    Slip {
        timeline_id: String,
        track_id: String,
        clip_id: String,
        source_start: ExactTime,
    },
    Slide {
        timeline_id: String,
        track_id: String,
        clip_id: String,
        to: ExactTime,
    },
    Razor {
        timeline_id: String,
        track_id: String,
        target_id: EditorialObjectId,
        at: ExactTime,
        fragment_id: EditorialObjectId,
    },
    Trim {
        timeline_id: String,
        track_id: String,
        target_id: EditorialObjectId,
        side: EditorEditSide,
        to: ExactTime,
        gap_id: Option<String>,
    },
    Extend {
        timeline_id: String,
        track_id: String,
        target_id: EditorialObjectId,
        side: EditorEditSide,
        to: ExactTime,
        mode: EditorExtendMode,
        sync_adjustments: Vec<EditorRippleSyncAdjustment>,
    },
    ThreePoint {
        timeline_id: String,
        track_id: String,
        clip: EditorTrackItem,
        placement: EditorThreePointPlacement,
        fragment_ids: Vec<EditorialObjectId>,
    },
    FourPoint {
        timeline_id: String,
        track_id: String,
        clip: EditorTrackItem,
        source_range: ExactTimeRange,
        record_range: ExactTimeRange,
        fragment_ids: Vec<EditorialObjectId>,
    },
}

impl TimelineEditOperation {
    fn into_engine(self) -> Result<engine::EditOperation> {
        match self {
            Self::Insert {
                timeline_id,
                track_id,
                at,
                material,
                fragment_ids,
            } => Ok(engine::EditOperation::Insert {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                at: at.into_engine()?,
                material: material.into_engine()?,
                fragment_ids: object_ids(fragment_ids)?,
            }),
            Self::Overwrite {
                timeline_id,
                track_id,
                at,
                material,
                fragment_ids,
            } => Ok(engine::EditOperation::Overwrite {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                at: at.into_engine()?,
                material: material.into_engine()?,
                fragment_ids: object_ids(fragment_ids)?,
            }),
            Self::Append {
                timeline_id,
                track_id,
                material,
            } => Ok(engine::EditOperation::Append {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                material: material.into_engine()?,
            }),
            Self::Replace {
                timeline_id,
                track_id,
                target_id,
                material,
            } => Ok(engine::EditOperation::Replace {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                target_id: target_id.into_engine()?,
                material: material.into_engine()?,
            }),
            Self::Lift {
                timeline_id,
                track_id,
                range,
                gap_id,
                gap_name,
                fragment_ids,
            } => {
                let range = range.into_engine()?;
                Ok(engine::EditOperation::Lift {
                    timeline_id: parse_id(&timeline_id, "timeline_id")?,
                    track_id: parse_id(&track_id, "track_id")?,
                    range,
                    gap: engine::Gap::new(parse_id(&gap_id, "gap_id")?, gap_name, range),
                    fragment_ids: object_ids(fragment_ids)?,
                })
            }
            Self::Extract {
                timeline_id,
                track_id,
                range,
                fragment_ids,
            } => Ok(engine::EditOperation::Extract {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                range: range.into_engine()?,
                fragment_ids: object_ids(fragment_ids)?,
            }),
            Self::Ripple {
                timeline_id,
                track_id,
                target_id,
                side,
                to,
                sync_adjustments,
            } => Ok(engine::EditOperation::Ripple {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                target_id: target_id.into_engine()?,
                side: side.into_engine(),
                to: to.into_engine()?,
                sync_adjustments: sync_adjustments
                    .into_iter()
                    .map(EditorRippleSyncAdjustment::into_engine)
                    .collect::<Result<Vec<_>>>()?,
            }),
            Self::Roll {
                timeline_id,
                track_id,
                left_id,
                right_id,
                to,
            } => Ok(engine::EditOperation::Roll {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                left_id: left_id.into_engine()?,
                right_id: right_id.into_engine()?,
                to: to.into_engine()?,
            }),
            Self::Slip {
                timeline_id,
                track_id,
                clip_id,
                source_start,
            } => Ok(engine::EditOperation::Slip {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                clip_id: parse_id(&clip_id, "clip_id")?,
                source_start: source_start.into_engine()?,
            }),
            Self::Slide {
                timeline_id,
                track_id,
                clip_id,
                to,
            } => Ok(engine::EditOperation::Slide {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                clip_id: parse_id(&clip_id, "clip_id")?,
                to: to.into_engine()?,
            }),
            Self::Razor {
                timeline_id,
                track_id,
                target_id,
                at,
                fragment_id,
            } => Ok(engine::EditOperation::Razor {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                target_id: target_id.into_engine()?,
                at: at.into_engine()?,
                fragment_id: fragment_id.into_engine()?,
            }),
            Self::Trim {
                timeline_id,
                track_id,
                target_id,
                side,
                to,
                gap_id,
            } => Ok(engine::EditOperation::Trim {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                target_id: target_id.into_engine()?,
                side: side.into_engine(),
                to: to.into_engine()?,
                gap_id: gap_id
                    .as_deref()
                    .map(|value| parse_id(value, "gap_id"))
                    .transpose()?,
            }),
            Self::Extend {
                timeline_id,
                track_id,
                target_id,
                side,
                to,
                mode,
                sync_adjustments,
            } => Ok(engine::EditOperation::Extend {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                target_id: target_id.into_engine()?,
                side: side.into_engine(),
                to: to.into_engine()?,
                mode: mode.into_engine(),
                sync_adjustments: sync_adjustments
                    .into_iter()
                    .map(EditorRippleSyncAdjustment::into_engine)
                    .collect::<Result<Vec<_>>>()?,
            }),
            Self::ThreePoint {
                timeline_id,
                track_id,
                clip,
                placement,
                fragment_ids,
            } => Ok(engine::EditOperation::ThreePoint {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                clip: into_clip(clip)?,
                placement: placement.into_engine()?,
                fragment_ids: object_ids(fragment_ids)?,
            }),
            Self::FourPoint {
                timeline_id,
                track_id,
                clip,
                source_range,
                record_range,
                fragment_ids,
            } => Ok(engine::EditOperation::FourPoint {
                timeline_id: parse_id(&timeline_id, "timeline_id")?,
                track_id: parse_id(&track_id, "track_id")?,
                clip: into_clip(clip)?,
                source_range: source_range.into_engine()?,
                record_range: record_range.into_engine()?,
                fragment_ids: object_ids(fragment_ids)?,
            }),
        }
    }
}

/// One complete graph endpoint.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorGraphEndpoint {
    pub node_id: String,
    pub port_id: String,
}

impl EditorGraphEndpoint {
    fn into_engine(self) -> Result<engine::GraphEndpoint> {
        Ok(engine::GraphEndpoint::new(
            parse_id(&self.node_id, "node_id")?,
            parse_id(&self.port_id, "port_id")?,
        ))
    }
}

/// One complete stable directed graph edge.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorGraphEdge {
    pub edge_id: String,
    pub source: EditorGraphEndpoint,
    pub destination: EditorGraphEndpoint,
}

impl EditorGraphEdge {
    fn into_engine(self) -> Result<engine::GraphEdge> {
        Ok(engine::GraphEdge::new(
            parse_id(&self.edge_id, "edge_id")?,
            self.source.into_engine()?,
            self.destination.into_engine()?,
        ))
    }
}

/// Complete color interpretation used by a graph schema.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorColorSpace {
    pub primaries: String,
    pub transfer: String,
    pub matrix: String,
    pub range: String,
}

impl EditorColorSpace {
    fn into_engine(self) -> Result<engine::ColorSpace> {
        Ok(engine::ColorSpace::new(
            engine::ColorPrimaries::from_code(&self.primaries)
                .ok_or_else(|| invalid("color_space", "unknown color primaries"))?,
            engine::TransferFunction::from_code(&self.transfer)
                .ok_or_else(|| invalid("color_space", "unknown transfer function"))?,
            engine::MatrixCoefficients::from_code(&self.matrix)
                .ok_or_else(|| invalid("color_space", "unknown matrix coefficients"))?,
            engine::ColorRange::from_code(&self.range)
                .ok_or_else(|| invalid("color_space", "unknown color range"))?,
        ))
    }
}

/// Graph port cardinality.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorPortCardinality {
    Single,
    Optional,
    Variadic,
}

impl EditorPortCardinality {
    const fn into_engine(self) -> engine::PortCardinality {
        match self {
            Self::Single => engine::PortCardinality::Single,
            Self::Optional => engine::PortCardinality::Optional,
            Self::Variadic => engine::PortCardinality::Variadic,
        }
    }
}

/// One typed port declaration.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorPortSchema {
    pub name: String,
    pub value_type: String,
    pub cardinality: EditorPortCardinality,
}

impl EditorPortSchema {
    fn into_engine(self) -> Result<engine::PortSchema> {
        Ok(engine::PortSchema::new(
            engine::PortName::new(self.name)
                .map_err(|_| invalid("port_schema", "invalid graph port name"))?,
            engine::ValueTypeId::new(self.value_type)
                .map_err(|_| invalid("port_schema", "invalid graph value type"))?,
            self.cardinality.into_engine(),
        ))
    }
}

/// One typed editable parameter declaration.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorParameterSchema {
    pub name: String,
    pub value_type: String,
    pub animatable: bool,
}

impl EditorParameterSchema {
    fn into_engine(self) -> Result<engine::ParameterSchema> {
        Ok(engine::ParameterSchema::new(
            engine::ParameterName::new(self.name)
                .map_err(|_| invalid("parameter_schema", "invalid graph parameter name"))?,
            engine::ValueTypeId::new(self.value_type)
                .map_err(|_| invalid("parameter_schema", "invalid graph value type"))?,
            self.animatable,
        ))
    }
}

/// Graph evaluation time dependency.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorTimeBehavior {
    Invariant {},
    CurrentFrame {},
    FrameWindow {
        frames_before: u32,
        frames_after: u32,
    },
    Unbounded {},
}

impl EditorTimeBehavior {
    const fn into_engine(self) -> engine::TimeBehavior {
        match self {
            Self::Invariant {} => engine::TimeBehavior::Invariant,
            Self::CurrentFrame {} => engine::TimeBehavior::CurrentFrame,
            Self::FrameWindow {
                frames_before,
                frames_after,
            } => engine::TimeBehavior::FrameWindow {
                frames_before,
                frames_after,
            },
            Self::Unbounded {} => engine::TimeBehavior::Unbounded,
        }
    }
}

/// Graph region-of-interest behavior.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorRoiBehavior {
    FullFrame {},
    InputBounds {},
    Expanded { pixels: u32 },
    Custom {},
}

impl EditorRoiBehavior {
    const fn into_engine(self) -> engine::RoiBehavior {
        match self {
            Self::FullFrame {} => engine::RoiBehavior::FullFrame,
            Self::InputBounds {} => engine::RoiBehavior::InputBounds,
            Self::Expanded { pixels } => engine::RoiBehavior::Expanded { pixels },
            Self::Custom {} => engine::RoiBehavior::Custom,
        }
    }
}

/// Graph color requirement.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorColorRequirements {
    NotApplicable {},
    Tagged {},
    Exact { color_space: EditorColorSpace },
}

impl EditorColorRequirements {
    fn into_engine(self) -> Result<engine::ColorRequirements> {
        match self {
            Self::NotApplicable {} => Ok(engine::ColorRequirements::NotApplicable),
            Self::Tagged {} => Ok(engine::ColorRequirements::Tagged),
            Self::Exact { color_space } => {
                Ok(engine::ColorRequirements::Exact(color_space.into_engine()?))
            }
        }
    }
}

/// Graph determinism declaration.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorDeterminism {
    Deterministic,
    Seeded,
    NonDeterministic,
}

impl EditorDeterminism {
    const fn into_engine(self) -> engine::Determinism {
        match self {
            Self::Deterministic => engine::Determinism::Deterministic,
            Self::Seeded => engine::Determinism::Seeded,
            Self::NonDeterministic => engine::Determinism::NonDeterministic,
        }
    }
}

/// Graph cache declaration.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorCachePolicy {
    Disabled,
    Static,
    PerFrame,
    PerRegion,
}

impl EditorCachePolicy {
    const fn into_engine(self) -> engine::CachePolicy {
        match self {
            Self::Disabled => engine::CachePolicy::Disabled,
            Self::Static => engine::CachePolicy::Static,
            Self::PerFrame => engine::CachePolicy::PerFrame,
            Self::PerRegion => engine::CachePolicy::PerRegion,
        }
    }
}

/// Complete graph node behavior declaration.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorNodeBehavior {
    pub time: EditorTimeBehavior,
    pub roi: EditorRoiBehavior,
    pub color: EditorColorRequirements,
    pub determinism: EditorDeterminism,
    pub cache_policy: EditorCachePolicy,
}

impl EditorNodeBehavior {
    fn into_engine(self) -> Result<engine::NodeBehavior> {
        Ok(engine::NodeBehavior::new(
            self.time.into_engine(),
            self.roi.into_engine(),
            self.color.into_engine()?,
            self.determinism.into_engine(),
            self.cache_policy.into_engine(),
        ))
    }
}

/// One complete versioned graph node schema.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorNodeSchema {
    pub node_type: String,
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    pub schema_version: SemanticVersion,
    pub inputs: Vec<EditorPortSchema>,
    pub outputs: Vec<EditorPortSchema>,
    pub parameters: Vec<EditorParameterSchema>,
    pub behavior: EditorNodeBehavior,
    pub required_capabilities: Vec<String>,
}

impl EditorNodeSchema {
    fn into_engine(self) -> Result<engine::NodeSchema> {
        let node_type = engine::NodeTypeId::new(self.node_type)
            .map_err(|_| invalid("node_schema", "invalid graph node type"))?;
        let inputs = self
            .inputs
            .into_iter()
            .map(EditorPortSchema::into_engine)
            .collect::<Result<Vec<_>>>()?;
        let outputs = self
            .outputs
            .into_iter()
            .map(EditorPortSchema::into_engine)
            .collect::<Result<Vec<_>>>()?;
        let parameters = self
            .parameters
            .into_iter()
            .map(EditorParameterSchema::into_engine)
            .collect::<Result<Vec<_>>>()?;
        let capabilities = self
            .required_capabilities
            .into_iter()
            .map(|value| {
                engine::CapabilityId::new(value)
                    .map_err(|_| invalid("node_schema", "invalid capability identifier"))
            })
            .collect::<Result<Vec<_>>>()?;
        engine::NodeSchema::new(
            engine::NodeSchemaId::new(node_type, self.schema_version),
            inputs,
            outputs,
            parameters,
            self.behavior.into_engine()?,
            engine::CapabilitySet::new(capabilities),
        )
    }
}

/// One semantic audio channel position.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorChannelPosition {
    FrontLeft {},
    FrontRight {},
    FrontCenter {},
    LowFrequency {},
    BackLeft {},
    BackRight {},
    FrontLeftOfCenter {},
    FrontRightOfCenter {},
    BackCenter {},
    SideLeft {},
    SideRight {},
    TopCenter {},
    TopFrontLeft {},
    TopFrontCenter {},
    TopFrontRight {},
    TopBackLeft {},
    TopBackCenter {},
    TopBackRight {},
    Discrete { index: u16 },
}

impl EditorChannelPosition {
    const fn into_engine(self) -> engine::ChannelPosition {
        match self {
            Self::FrontLeft {} => engine::ChannelPosition::FrontLeft,
            Self::FrontRight {} => engine::ChannelPosition::FrontRight,
            Self::FrontCenter {} => engine::ChannelPosition::FrontCenter,
            Self::LowFrequency {} => engine::ChannelPosition::LowFrequency,
            Self::BackLeft {} => engine::ChannelPosition::BackLeft,
            Self::BackRight {} => engine::ChannelPosition::BackRight,
            Self::FrontLeftOfCenter {} => engine::ChannelPosition::FrontLeftOfCenter,
            Self::FrontRightOfCenter {} => engine::ChannelPosition::FrontRightOfCenter,
            Self::BackCenter {} => engine::ChannelPosition::BackCenter,
            Self::SideLeft {} => engine::ChannelPosition::SideLeft,
            Self::SideRight {} => engine::ChannelPosition::SideRight,
            Self::TopCenter {} => engine::ChannelPosition::TopCenter,
            Self::TopFrontLeft {} => engine::ChannelPosition::TopFrontLeft,
            Self::TopFrontCenter {} => engine::ChannelPosition::TopFrontCenter,
            Self::TopFrontRight {} => engine::ChannelPosition::TopFrontRight,
            Self::TopBackLeft {} => engine::ChannelPosition::TopBackLeft,
            Self::TopBackCenter {} => engine::ChannelPosition::TopBackCenter,
            Self::TopBackRight {} => engine::ChannelPosition::TopBackRight,
            Self::Discrete { index } => engine::ChannelPosition::Discrete(index),
        }
    }
}

fn channel_layout(values: Vec<EditorChannelPosition>) -> Result<engine::ChannelLayout> {
    engine::ChannelLayout::new(values.into_iter().map(EditorChannelPosition::into_engine))
}

/// Graph-domain track semantics.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorTrackSemantics {
    Video {
        frame_rate_numerator: u32,
        frame_rate_denominator: u32,
        compositing: EditorVideoCompositing,
    },
    Audio {
        sample_rate: u32,
        channel_layout: Vec<EditorChannelPosition>,
        destination: EditorAudioDestination,
        destination_layout: Vec<EditorChannelPosition>,
        routes: Vec<EditorAudioChannelRoute>,
    },
    Caption {
        timebase: ExactTimebase,
        language: String,
        purpose: EditorCaptionPurpose,
    },
    Data {
        timebase: ExactTimebase,
        scheme_id_uri: String,
        value: Option<String>,
    },
}

/// Video compositing behavior.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorVideoCompositing {
    Over,
    Replace,
}

/// Audio route destination.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorAudioDestination {
    Main {},
    Track { track_id: String },
}

/// Audio channel route target.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorAudioChannelTarget {
    Channel { position: EditorChannelPosition },
    Muted {},
}

/// One exact audio channel routing decision.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorAudioChannelRoute {
    pub source: EditorChannelPosition,
    pub target: EditorAudioChannelTarget,
}

/// Timed text purpose.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorCaptionPurpose {
    Captions,
    Subtitles,
    Descriptions,
    Chapters,
}

impl EditorTrackSemantics {
    fn into_engine(self) -> Result<engine::TrackSemantics> {
        match self {
            Self::Video {
                frame_rate_numerator,
                frame_rate_denominator,
                compositing,
            } => Ok(engine::TrackSemantics::Video(
                engine::VideoTrackSemantics::new(
                    engine::FrameRate::new(frame_rate_numerator, frame_rate_denominator)?,
                    match compositing {
                        EditorVideoCompositing::Over => engine::VideoCompositing::Over,
                        EditorVideoCompositing::Replace => engine::VideoCompositing::Replace,
                    },
                ),
            )),
            Self::Audio {
                sample_rate,
                channel_layout: source_layout,
                destination,
                destination_layout,
                routes,
            } => {
                let source_layout = channel_layout(source_layout)?;
                let destination_layout = channel_layout(destination_layout)?;
                let destination = match destination {
                    EditorAudioDestination::Main {} => engine::AudioRouteDestination::Main,
                    EditorAudioDestination::Track { track_id } => {
                        engine::AudioRouteDestination::Track(parse_id(&track_id, "track_id")?)
                    }
                };
                let routes = routes
                    .into_iter()
                    .map(|route| {
                        engine::AudioChannelRoute::new(
                            route.source.into_engine(),
                            match route.target {
                                EditorAudioChannelTarget::Channel { position } => {
                                    engine::AudioChannelTarget::Channel(position.into_engine())
                                }
                                EditorAudioChannelTarget::Muted {} => {
                                    engine::AudioChannelTarget::Muted
                                }
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                let routing = engine::AudioRouting::new(destination, destination_layout, routes)?;
                Ok(engine::TrackSemantics::Audio(
                    engine::AudioTrackSemantics::new(sample_rate, source_layout, routing)?,
                ))
            }
            Self::Caption {
                timebase,
                language,
                purpose,
            } => Ok(engine::TrackSemantics::Caption(
                engine::CaptionTrackSemantics::new(
                    timebase.into_engine()?,
                    engine::LanguageTag::new(language)?,
                    match purpose {
                        EditorCaptionPurpose::Captions => engine::CaptionPurpose::Captions,
                        EditorCaptionPurpose::Subtitles => engine::CaptionPurpose::Subtitles,
                        EditorCaptionPurpose::Descriptions => engine::CaptionPurpose::Descriptions,
                        EditorCaptionPurpose::Chapters => engine::CaptionPurpose::Chapters,
                    },
                ),
            )),
            Self::Data {
                timebase,
                scheme_id_uri,
                value,
            } => Ok(engine::TrackSemantics::Data(
                engine::DataTrackSemantics::new(
                    timebase.into_engine()?,
                    engine::DataSchema::new(scheme_id_uri, value.as_deref())?,
                ),
            )),
        }
    }
}

/// One recursively typed timeline metadata value.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum EditorMetadataValue {
    Null,
    Boolean(bool),
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Text(String),
    Time(ExactTime),
    Range(ExactTimeRange),
    List(Vec<EditorMetadataValue>),
    Map(BTreeMap<String, EditorMetadataValue>),
}

impl EditorMetadataValue {
    fn into_engine(self) -> Result<engine::MetadataValue> {
        match self {
            Self::Null => Ok(engine::MetadataValue::Null),
            Self::Boolean(value) => Ok(engine::MetadataValue::Boolean(value)),
            Self::Signed(value) => Ok(engine::MetadataValue::Signed(value)),
            Self::Unsigned(value) => Ok(engine::MetadataValue::Unsigned(value)),
            Self::Float(value) => Ok(engine::MetadataValue::Float(
                engine::DiagnosticFiniteF64::new(value)?,
            )),
            Self::Text(value) => Ok(engine::MetadataValue::Text(value)),
            Self::Time(value) => Ok(engine::MetadataValue::Time(value.into_engine()?)),
            Self::Range(value) => Ok(engine::MetadataValue::Range(value.into_engine()?)),
            Self::List(values) => Ok(engine::MetadataValue::List(
                values
                    .into_iter()
                    .map(EditorMetadataValue::into_engine)
                    .collect::<Result<Vec<_>>>()?,
            )),
            Self::Map(values) => Ok(engine::MetadataValue::Map(metadata_map(values)?)),
        }
    }
}

fn metadata_map(values: BTreeMap<String, EditorMetadataValue>) -> Result<engine::TimelineMetadata> {
    let values = values
        .into_iter()
        .map(|(key, value)| Ok((engine::MetadataKey::new(key)?, value.into_engine()?)))
        .collect::<Result<Vec<_>>>()?;
    Ok(engine::TimelineMetadata::from_entries(values))
}

/// Multicam synchronization provenance.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorMulticamSyncMethod {
    Manual {},
    Timecode {},
    InPoints {},
    OutPoints {},
    ClipMarker { name: String },
    Audio {},
}

impl EditorMulticamSyncMethod {
    fn into_engine(self) -> engine::MulticamSyncMethod {
        match self {
            Self::Manual {} => engine::MulticamSyncMethod::Manual,
            Self::Timecode {} => engine::MulticamSyncMethod::Timecode,
            Self::InPoints {} => engine::MulticamSyncMethod::InPoints,
            Self::OutPoints {} => engine::MulticamSyncMethod::OutPoints,
            Self::ClipMarker { name } => engine::MulticamSyncMethod::ClipMarker(name),
            Self::Audio {} => engine::MulticamSyncMethod::Audio,
        }
    }
}

/// One complete multicam angle.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorMulticamAngle {
    pub angle_id: String,
    pub name: String,
    pub camera_label: String,
    pub enabled: bool,
    pub metadata: BTreeMap<String, EditorMetadataValue>,
    pub source_clip_ids: Vec<String>,
}

impl EditorMulticamAngle {
    fn into_engine(self) -> Result<engine::MulticamAngle> {
        let mut angle = engine::MulticamAngle::new(
            parse_id(&self.angle_id, "multicam_angle_id")?,
            self.name,
            self.camera_label,
            self.source_clip_ids
                .iter()
                .map(|value| parse_id(value, "clip_id"))
                .collect::<Result<Vec<_>>>()?,
        )?;
        angle.set_enabled(self.enabled);
        angle.set_metadata(metadata_map(self.metadata)?);
        Ok(angle)
    }
}

/// Complete synchronized multicam source state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorMulticamSource {
    pub sync_method: EditorMulticamSyncMethod,
    pub angles: Vec<EditorMulticamAngle>,
}

impl EditorMulticamSource {
    fn into_engine(self) -> Result<engine::MulticamSource> {
        engine::MulticamSource::new(
            self.sync_method.into_engine(),
            self.angles
                .into_iter()
                .map(EditorMulticamAngle::into_engine)
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

/// Multicam audio selection policy.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorMulticamAudioPolicy {
    FollowVideo {},
    Fixed { angle_id: String },
    AllAngles {},
}

impl EditorMulticamAudioPolicy {
    fn into_engine(self) -> Result<engine::MulticamAudioPolicy> {
        match self {
            Self::FollowVideo {} => Ok(engine::MulticamAudioPolicy::FollowVideo),
            Self::Fixed { angle_id } => Ok(engine::MulticamAudioPolicy::Fixed(parse_id(
                &angle_id,
                "multicam_angle_id",
            )?)),
            Self::AllAngles {} => Ok(engine::MulticamAudioPolicy::AllAngles),
        }
    }
}

/// One exact multicam switch interval.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorMulticamSwitch {
    pub source_range: ExactTimeRange,
    pub angle_id: String,
}

/// Complete clip-local multicam state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorMulticamClip {
    pub clip_id: String,
    pub switches: Vec<EditorMulticamSwitch>,
    pub audio_policy: EditorMulticamAudioPolicy,
}

impl EditorMulticamClip {
    fn into_engine(self) -> Result<engine::MulticamClip> {
        let mut switches = self.switches.into_iter();
        let first = switches
            .next()
            .ok_or_else(|| invalid("multicam_clip", "multicam switch list is empty"))?;
        let first_range = first.source_range.into_engine()?;
        let mut parsed = vec![(first_range, parse_id(&first.angle_id, "multicam_angle_id")?)];
        for switch in switches {
            parsed.push((
                switch.source_range.into_engine()?,
                parse_id(&switch.angle_id, "multicam_angle_id")?,
            ));
        }
        let coverage = engine::TimeRange::from_start_end(
            parsed[0].0.start(),
            parsed
                .last()
                .expect("parsed multicam switch remains present")
                .0
                .end_exclusive()?,
        )?;
        let mut clip = engine::MulticamClip::new(
            parse_id(&self.clip_id, "clip_id")?,
            coverage,
            parsed[0].1,
            self.audio_policy.into_engine()?,
        )?;
        for (range, angle_id) in parsed {
            clip.switch_range(range, angle_id)?;
        }
        Ok(clip)
    }
}

/// Every exact timeline-domain value accepted by a compiled graph parameter.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorTimelineGraphValue {
    ProjectId { project_id: String },
    TimelineId { timeline_id: String },
    TrackId { track_id: String },
    EditorialObjectId { object_id: EditorialObjectId },
    Text { value: String },
    OptionalText { value: Option<String> },
    Timebase { value: ExactTimebase },
    RationalTime { value: ExactTime },
    TimeRange { value: ExactTimeRange },
    Duration { value: ExactDuration },
    ClipSource { value: EditorClipSource },
    ClipTimeMap { value: EditorClipTimeMap },
    OptionalMulticamSource { value: Option<EditorMulticamSource> },
    OptionalMulticamClip { value: Option<EditorMulticamClip> },
    TrackSemantics { value: EditorTrackSemantics },
    TrackOrder { track_ids: Vec<String> },
    ObjectOrder { object_ids: Vec<EditorialObjectId> },
    StringMap { values: BTreeMap<String, String> },
}

impl EditorTimelineGraphValue {
    fn into_engine(self) -> Result<engine::TimelineGraphValue> {
        match self {
            Self::ProjectId { project_id } => Ok(engine::TimelineGraphValue::ProjectId(parse_id(
                &project_id,
                "project_id",
            )?)),
            Self::TimelineId { timeline_id } => Ok(engine::TimelineGraphValue::TimelineId(
                parse_id(&timeline_id, "timeline_id")?,
            )),
            Self::TrackId { track_id } => Ok(engine::TimelineGraphValue::TrackId(parse_id(
                &track_id, "track_id",
            )?)),
            Self::EditorialObjectId { object_id } => Ok(
                engine::TimelineGraphValue::EditorialObjectId(object_id.into_engine()?),
            ),
            Self::Text { value } => Ok(engine::TimelineGraphValue::Text(value)),
            Self::OptionalText { value } => Ok(engine::TimelineGraphValue::OptionalText(value)),
            Self::Timebase { value } => {
                Ok(engine::TimelineGraphValue::Timebase(value.into_engine()?))
            }
            Self::RationalTime { value } => Ok(engine::TimelineGraphValue::RationalTime(
                value.into_engine()?,
            )),
            Self::TimeRange { value } => {
                Ok(engine::TimelineGraphValue::TimeRange(value.into_engine()?))
            }
            Self::Duration { value } => {
                Ok(engine::TimelineGraphValue::Duration(value.into_engine()?))
            }
            Self::ClipSource { value } => {
                Ok(engine::TimelineGraphValue::ClipSource(value.into_engine()?))
            }
            Self::ClipTimeMap { value } => Ok(engine::TimelineGraphValue::ClipTimeMap(
                value.into_engine()?,
            )),
            Self::OptionalMulticamSource { value } => {
                Ok(engine::TimelineGraphValue::OptionalMulticamSource(
                    value.map(EditorMulticamSource::into_engine).transpose()?,
                ))
            }
            Self::OptionalMulticamClip { value } => {
                Ok(engine::TimelineGraphValue::OptionalMulticamClip(
                    value.map(EditorMulticamClip::into_engine).transpose()?,
                ))
            }
            Self::TrackSemantics { value } => Ok(engine::TimelineGraphValue::TrackSemantics(
                value.into_engine()?,
            )),
            Self::TrackOrder { track_ids } => Ok(engine::TimelineGraphValue::TrackOrder(
                track_ids
                    .iter()
                    .map(|value| parse_id(value, "track_id"))
                    .collect::<Result<Vec<_>>>()?,
            )),
            Self::ObjectOrder { object_ids: values } => {
                Ok(engine::TimelineGraphValue::ObjectOrder(object_ids(values)?))
            }
            Self::StringMap { values } => Ok(engine::TimelineGraphValue::StringMap(values)),
        }
    }
}

/// Every processing or timeline-domain graph value on the editor surface.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorGraphValue {
    Domain { value: EditorTimelineGraphValue },
    Scalar { value: f64 },
    Vec2 { value: [f64; 2] },
    Vec3 { value: [f64; 3] },
    Color { value: [f64; 4] },
    Matrix3 { value: [f64; 9] },
    Boolean { value: bool },
    Choice { value: String },
}

impl EditorGraphValue {
    fn into_engine(self) -> Result<engine::CompiledTimelineGraphValue> {
        match self {
            Self::Domain { value } => Ok(engine::GraphValue::domain(value.into_engine()?)),
            Self::Scalar { value } => engine::GraphValue::scalar(value),
            Self::Vec2 { value } => engine::GraphValue::vec2(value),
            Self::Vec3 { value } => engine::GraphValue::vec3(value),
            Self::Color { value } => engine::GraphValue::color(value),
            Self::Matrix3 { value } => engine::GraphValue::matrix3(value),
            Self::Boolean { value } => Ok(engine::GraphValue::boolean(value)),
            Self::Choice { value } => engine::GraphValue::choice(value),
        }
    }
}

/// One schema-tagged graph parameter value.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorTypedParameterValue {
    pub value_type: String,
    pub value: EditorGraphValue,
}

impl EditorTypedParameterValue {
    fn into_engine(
        self,
    ) -> Result<engine::TypedParameterValue<engine::CompiledTimelineGraphValue>> {
        Ok(engine::TypedParameterValue::new(
            engine::ValueTypeId::new(self.value_type)
                .map_err(|_| invalid("parameter_value", "invalid graph value type"))?,
            self.value.into_engine()?,
        ))
    }
}

/// One stable instance port binding.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorInstancePort {
    pub port_id: String,
    pub name: String,
}

impl EditorInstancePort {
    fn into_engine(self) -> Result<engine::InstancePort> {
        Ok(engine::InstancePort::new(
            parse_id(&self.port_id, "port_id")?,
            engine::PortName::new(self.name)
                .map_err(|_| invalid("instance_port", "invalid graph port name"))?,
        ))
    }
}

/// One complete editable parameter binding.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorEditableParameter {
    pub parameter_id: String,
    pub name: String,
    pub value: EditorTypedParameterValue,
}

impl EditorEditableParameter {
    fn into_engine(self) -> Result<engine::EditableParameter<engine::CompiledTimelineGraphValue>> {
        Ok(engine::EditableParameter::new(
            parse_id(&self.parameter_id, "parameter_id")?,
            engine::ParameterName::new(self.name)
                .map_err(|_| invalid("editable_parameter", "invalid graph parameter name"))?,
            self.value.into_engine()?,
        ))
    }
}

/// One complete editable graph node instance.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorEditableNode {
    pub schema: EditorNodeSchema,
    pub inputs: Vec<EditorInstancePort>,
    pub outputs: Vec<EditorInstancePort>,
    pub parameters: Vec<EditorEditableParameter>,
}

impl EditorEditableNode {
    fn into_engine(self) -> Result<engine::EditableNode<engine::CompiledTimelineGraphValue>> {
        let schema = Arc::new(self.schema.into_engine()?);
        engine::EditableNode::new(
            schema,
            self.inputs
                .into_iter()
                .map(EditorInstancePort::into_engine)
                .collect::<Result<Vec<_>>>()?,
            self.outputs
                .into_iter()
                .map(EditorInstancePort::into_engine)
                .collect::<Result<Vec<_>>>()?,
            self.parameters
                .into_iter()
                .map(EditorEditableParameter::into_engine)
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

/// Stable graph parameter address.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorParameterAddress {
    pub node_id: String,
    pub parameter_id: String,
}

impl EditorParameterAddress {
    fn into_engine(self) -> Result<engine::ParameterAddress> {
        Ok(engine::ParameterAddress::new(
            parse_id(&self.node_id, "node_id")?,
            parse_id(&self.parameter_id, "parameter_id")?,
        ))
    }
}

/// One typed expression or link dependency.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorParameterReference {
    pub address: EditorParameterAddress,
    pub value_type: String,
}

impl EditorParameterReference {
    fn into_engine(self) -> Result<engine::ParameterReference> {
        Ok(engine::ParameterReference::new(
            self.address.into_engine()?,
            engine::ValueTypeId::new(self.value_type)
                .map_err(|_| invalid("parameter_reference", "invalid graph value type"))?,
        ))
    }
}

/// One explicit expression variable binding.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorExpressionBinding {
    pub name: String,
    pub reference: EditorParameterReference,
}

/// Complete direct-link or bounded expression driver state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorParameterDriver {
    Link {
        value_type: String,
        source: EditorParameterReference,
    },
    Expression {
        value_type: String,
        source: String,
        bindings: Vec<EditorExpressionBinding>,
    },
}

impl EditorParameterDriver {
    fn into_engine(self) -> Result<engine::ParameterDriver> {
        match self {
            Self::Link { value_type, source } => Ok(engine::ParameterDriver::link(
                engine::ValueTypeId::new(value_type)
                    .map_err(|_| invalid("parameter_driver", "invalid graph value type"))?,
                source.into_engine()?,
            )),
            Self::Expression {
                value_type,
                source,
                bindings,
            } => {
                let bindings = bindings
                    .into_iter()
                    .map(|binding| {
                        Ok((
                            engine::ExpressionVariableName::new(binding.name)?,
                            binding.reference.into_engine()?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(engine::ParameterDriver::expression(
                    engine::ValueTypeId::new(value_type)
                        .map_err(|_| invalid("parameter_driver", "invalid graph value type"))?,
                    engine::ParameterExpression::compile(&source, bindings)?,
                ))
            }
        }
    }
}

/// Every current graph mutation on the public editor surface.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorGraphMutation {
    Add {
        node_id: String,
        node: Box<EditorEditableNode>,
        position: usize,
    },
    Remove {
        node_id: String,
    },
    Connect {
        edge: EditorGraphEdge,
    },
    Disconnect {
        edge_id: String,
    },
    Reorder {
        node_id: String,
        position: usize,
    },
    SetParameter {
        node_id: String,
        parameter_id: String,
        value: EditorTypedParameterValue,
    },
    SetParameterDriver {
        target: EditorParameterAddress,
        driver: EditorParameterDriver,
    },
    ClearParameterDriver {
        target: EditorParameterAddress,
    },
}

impl EditorGraphMutation {
    fn into_engine(self) -> Result<engine::GraphMutation<engine::CompiledTimelineGraphValue>> {
        match self {
            Self::Add {
                node_id,
                node,
                position,
            } => Ok(engine::GraphMutation::Add {
                node_id: parse_id(&node_id, "node_id")?,
                node: (*node).into_engine()?,
                position,
            }),
            Self::Remove { node_id } => Ok(engine::GraphMutation::Remove {
                node_id: parse_id(&node_id, "node_id")?,
            }),
            Self::Connect { edge } => Ok(engine::GraphMutation::Connect {
                edge: edge.into_engine()?,
            }),
            Self::Disconnect { edge_id } => Ok(engine::GraphMutation::Disconnect {
                edge_id: parse_id(&edge_id, "edge_id")?,
            }),
            Self::Reorder { node_id, position } => Ok(engine::GraphMutation::Reorder {
                node_id: parse_id(&node_id, "node_id")?,
                position,
            }),
            Self::SetParameter {
                node_id,
                parameter_id,
                value,
            } => Ok(engine::GraphMutation::SetParameter {
                node_id: parse_id(&node_id, "node_id")?,
                parameter_id: parse_id(&parameter_id, "parameter_id")?,
                value: value.into_engine()?,
            }),
            Self::SetParameterDriver { target, driver } => {
                Ok(engine::GraphMutation::SetParameterDriver {
                    target: target.into_engine()?,
                    driver: driver.into_engine()?,
                })
            }
            Self::ClearParameterDriver { target } => {
                Ok(engine::GraphMutation::ClearParameterDriver {
                    target: target.into_engine()?,
                })
            }
        }
    }
}

/// Portable or declared-platform filesystem target for referenced media.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorMediaPath {
    ProjectRelative {
        path: String,
    },
    Absolute {
        platform: EditorMediaPathPlatform,
        path: String,
    },
}

/// Filesystem syntax retained for an absolute media path.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorMediaPathPlatform {
    Unix,
    Windows,
}

impl EditorMediaPath {
    fn permission_path(&self) -> Result<ApiFilesystemPath> {
        match self {
            Self::ProjectRelative { path } => ApiFilesystemPath::project_relative(path.clone()),
            Self::Absolute { platform, path } => ApiFilesystemPath::absolute(
                match platform {
                    EditorMediaPathPlatform::Unix => ApiFilesystemPlatform::Unix,
                    EditorMediaPathPlatform::Windows => ApiFilesystemPlatform::Windows,
                },
                path.clone(),
            ),
        }
    }

    fn into_engine(self) -> Result<engine::ReferencedMediaPath> {
        let target = match self {
            Self::ProjectRelative { path } => {
                format!("superi.media-path.v1:relative:{path}")
            }
            Self::Absolute { platform, path } => format!(
                "superi.media-path.v1:absolute:{}:{path}",
                match platform {
                    EditorMediaPathPlatform::Unix => "unix",
                    EditorMediaPathPlatform::Windows => "windows",
                }
            ),
        };
        engine::ReferencedMediaPath::from_target(&target)?.ok_or_else(|| {
            invalid(
                "media_path",
                "public media path did not decode as a filesystem target",
            )
        })
    }
}

/// Every current referenced-media mutation.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorMediaMutation {
    SetPath {
        media_id: String,
        path: EditorMediaPath,
    },
    MarkMissing {
        media_id: String,
    },
    ConsiderRelink {
        media_id: String,
        path: EditorMediaPath,
        observed_fingerprint: String,
    },
}

impl EditorMediaMutation {
    fn into_engine(self) -> Result<engine::ProjectMediaCommand> {
        match self {
            Self::SetPath { media_id, path } => Ok(engine::ProjectMediaCommand::set_path(
                parse_id(&media_id, "media_id")?,
                path.into_engine()?,
            )),
            Self::MarkMissing { media_id } => Ok(engine::ProjectMediaCommand::mark_missing(
                parse_id(&media_id, "media_id")?,
            )),
            Self::ConsiderRelink {
                media_id,
                path,
                observed_fingerprint,
            } => Ok(engine::ProjectMediaCommand::consider_relink(
                parse_id(&media_id, "media_id")?,
                path.into_engine()?,
                observed_fingerprint,
            )),
        }
    }
}

/// One complete authored source-to-destination audio route.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorChannelMap {
    pub source: EditorChannelPosition,
    pub destination: EditorChannelPosition,
    pub gain: f32,
}

/// Complete durable mix controls for one clip.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorClipMixControls {
    pub input_layout: Vec<EditorChannelPosition>,
    pub output_layout: Vec<EditorChannelPosition>,
    pub channel_map: Vec<EditorChannelMap>,
    pub gain: f32,
    pub fade_in_frames: u64,
    pub fade_out_frames: u64,
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
    pub phase_inverted: Vec<EditorChannelPosition>,
}

impl EditorClipMixControls {
    fn into_engine(self) -> Result<engine::ClipMixControls> {
        let controls = engine::ClipMixControls::new(
            channel_layout(self.input_layout)?,
            channel_layout(self.output_layout)?,
            self.channel_map
                .into_iter()
                .map(|route| {
                    engine::ChannelMap::new(
                        route.source.into_engine(),
                        route.destination.into_engine(),
                        route.gain,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
        )?
        .with_gain(self.gain)?
        .with_fades(self.fade_in_frames, self.fade_out_frames)?
        .with_pan(self.pan)?
        .with_muted(self.muted)
        .with_solo(self.solo)
        .with_phase_inverted(
            self.phase_inverted
                .into_iter()
                .map(EditorChannelPosition::into_engine),
        )?;
        Ok(controls)
    }
}

/// Every current durable clip-mix mutation.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorClipMixMutation {
    Set {
        clip_id: String,
        controls: EditorClipMixControls,
    },
    Inherit {
        original: String,
        created: String,
    },
    Transfer {
        removed: String,
        inserted: String,
    },
    Remove {
        clip_id: String,
    },
}

impl EditorClipMixMutation {
    fn into_engine(self) -> Result<engine::ClipMixMutation> {
        match self {
            Self::Set { clip_id, controls } => Ok(engine::ClipMixMutation::set(
                parse_id(&clip_id, "clip_id")?,
                controls.into_engine()?,
            )),
            Self::Inherit { original, created } => Ok(engine::ClipMixMutation::inherit(
                parse_id(&original, "clip_id")?,
                parse_id(&created, "clip_id")?,
            )),
            Self::Transfer { removed, inserted } => Ok(engine::ClipMixMutation::transfer(
                parse_id(&removed, "clip_id")?,
                parse_id(&inserted, "clip_id")?,
            )),
            Self::Remove { clip_id } => Ok(engine::ClipMixMutation::remove(parse_id(
                &clip_id, "clip_id",
            )?)),
        }
    }
}

/// Stable compound extension record identity.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorExtensionKey {
    pub extension_id: String,
    pub record_id: String,
}

impl EditorExtensionKey {
    fn into_engine(self) -> Result<engine::ProjectExtensionKey> {
        Ok(engine::ProjectExtensionKey::new(
            engine::ComponentId::new(self.extension_id)
                .map_err(|_| invalid("extension_key", "invalid extension identifier"))?,
            engine::ProjectExtensionRecordId::new(self.record_id)?,
        ))
    }
}

/// Durable extension lifecycle.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorExtensionLifecycle {
    Enabled,
    Disabled,
    Quarantined,
}

impl EditorExtensionLifecycle {
    const fn into_engine(self) -> engine::ProjectExtensionLifecycle {
        match self {
            Self::Enabled => engine::ProjectExtensionLifecycle::Enabled,
            Self::Disabled => engine::ProjectExtensionLifecycle::Disabled,
            Self::Quarantined => engine::ProjectExtensionLifecycle::Quarantined,
        }
    }
}

/// One retained extension failure context.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorFailureContext {
    pub component: String,
    pub operation: String,
    pub fields: BTreeMap<String, String>,
}

impl EditorFailureContext {
    fn into_engine(self) -> engine::ErrorContext {
        let mut context = engine::ErrorContext::new(self.component, self.operation);
        for (key, value) in self.fields {
            context = context.with_field(key, value);
        }
        context
    }
}

/// Complete durable extension failure evidence.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorExtensionFailure {
    pub category: String,
    pub recoverability: String,
    pub message: String,
    pub contexts: Vec<EditorFailureContext>,
    pub total_failures: u64,
    pub consecutive_failures: u32,
}

impl EditorExtensionFailure {
    fn into_engine(self) -> Result<engine::ProjectExtensionFailure> {
        engine::ProjectExtensionFailure::new(
            engine::ErrorCategory::from_code(&self.category)
                .ok_or_else(|| invalid("extension_failure", "unknown error category"))?,
            engine::Recoverability::from_code(&self.recoverability)
                .ok_or_else(|| invalid("extension_failure", "unknown recoverability"))?,
            self.message,
            self.contexts
                .into_iter()
                .map(EditorFailureContext::into_engine),
            self.total_failures,
            self.consecutive_failures,
        )
    }
}

/// Complete opaque extension record envelope.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorExtensionRecord {
    pub extension_id: String,
    pub record_id: String,
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    pub extension_version: SemanticVersion,
    pub extension_kind: String,
    pub payload_schema: String,
    pub requested_capabilities: Vec<String>,
    pub granted_capabilities: Vec<String>,
    pub lifecycle: EditorExtensionLifecycle,
    pub failure: Option<EditorExtensionFailure>,
    pub payload: Vec<u8>,
}

impl EditorExtensionRecord {
    fn into_engine(self) -> Result<engine::ProjectExtensionRecord> {
        let extension_id = engine::ComponentId::new(self.extension_id)
            .map_err(|_| invalid("extension_record", "invalid extension identifier"))?;
        let record_id = engine::ProjectExtensionRecordId::new(self.record_id)?;
        let kind = engine::ProjectExtensionKind::new(
            engine::ComponentId::new(self.extension_kind)
                .map_err(|_| invalid("extension_record", "invalid extension kind"))?,
        );
        let payload_schema = engine::VersionIdentifier::from_str(&self.payload_schema)
            .map_err(|_| invalid("extension_record", "invalid payload schema identifier"))?;
        engine::ProjectExtensionRecord::new(
            extension_id,
            record_id,
            self.extension_version,
            kind,
            payload_schema,
            capability_set(self.requested_capabilities)?,
            capability_set(self.granted_capabilities)?,
            self.lifecycle.into_engine(),
            self.failure
                .map(EditorExtensionFailure::into_engine)
                .transpose()?,
            self.payload,
        )
    }
}

/// Every current durable extension mutation.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditorExtensionMutation {
    Upsert {
        record: EditorExtensionRecord,
    },
    Remove {
        key: EditorExtensionKey,
    },
    SetLifecycle {
        key: EditorExtensionKey,
        lifecycle: EditorExtensionLifecycle,
    },
    SetGrantedCapabilities {
        key: EditorExtensionKey,
        capabilities: Vec<String>,
    },
    RecordFailure {
        key: EditorExtensionKey,
        failure: EditorExtensionFailure,
        quarantine: bool,
    },
    ClearFailure {
        key: EditorExtensionKey,
    },
}

impl EditorExtensionMutation {
    fn append_permission_requirements(
        &self,
        requirements: &mut Vec<ApiPermissionRequirement>,
    ) -> Result<()> {
        match self {
            Self::Upsert { record } => {
                requirements.push(ApiPermissionRequirement::plugin(
                    ApiPluginOperation::ManageState,
                    record.extension_id.clone(),
                )?);
                requirements.push(ApiPermissionRequirement::plugin(
                    ApiPluginOperation::ManageLifecycle,
                    record.extension_id.clone(),
                )?);
                if !record.granted_capabilities.is_empty() {
                    requirements.push(ApiPermissionRequirement::plugin_delegation(
                        record.extension_id.clone(),
                        record.granted_capabilities.clone(),
                    )?);
                }
            }
            Self::Remove { key } | Self::RecordFailure { key, .. } | Self::ClearFailure { key } => {
                requirements.push(ApiPermissionRequirement::plugin(
                    ApiPluginOperation::ManageState,
                    key.extension_id.clone(),
                )?);
            }
            Self::SetLifecycle { key, .. } => {
                requirements.push(ApiPermissionRequirement::plugin(
                    ApiPluginOperation::ManageLifecycle,
                    key.extension_id.clone(),
                )?);
            }
            Self::SetGrantedCapabilities {
                key, capabilities, ..
            } => {
                requirements.push(ApiPermissionRequirement::plugin_delegation(
                    key.extension_id.clone(),
                    capabilities.clone(),
                )?);
            }
        }
        Ok(())
    }

    fn into_engine(self) -> Result<engine::ProjectExtensionCommand> {
        match self {
            Self::Upsert { record } => Ok(engine::ProjectExtensionCommand::upsert(
                record.into_engine()?,
            )),
            Self::Remove { key } => Ok(engine::ProjectExtensionCommand::remove(key.into_engine()?)),
            Self::SetLifecycle { key, lifecycle } => {
                Ok(engine::ProjectExtensionCommand::set_lifecycle(
                    key.into_engine()?,
                    lifecycle.into_engine(),
                ))
            }
            Self::SetGrantedCapabilities { key, capabilities } => {
                Ok(engine::ProjectExtensionCommand::set_granted_capabilities(
                    key.into_engine()?,
                    capability_set(capabilities)?,
                ))
            }
            Self::RecordFailure {
                key,
                failure,
                quarantine,
            } => Ok(engine::ProjectExtensionCommand::record_failure(
                key.into_engine()?,
                failure.into_engine()?,
                quarantine,
            )),
            Self::ClearFailure { key } => Ok(engine::ProjectExtensionCommand::clear_failure(
                key.into_engine()?,
            )),
        }
    }
}

fn capability_set(values: Vec<String>) -> Result<engine::CapabilitySet> {
    let values = values
        .into_iter()
        .map(|value| {
            engine::CapabilityId::new(value)
                .map_err(|_| invalid("capability_set", "invalid capability identifier"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(engine::CapabilitySet::new(values))
}

/// Every authored action integrated by the production project transaction owner.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectAction {
    SelectRootTimeline {
        timeline_id: String,
    },
    EditTimeline {
        operations: Vec<TimelineEditOperation>,
    },
    MutateGraph {
        graph_id: String,
        mutations: Vec<EditorGraphMutation>,
    },
    MutateMedia {
        mutation: EditorMediaMutation,
    },
    MutateClipMix {
        mutations: Vec<EditorClipMixMutation>,
    },
    MutateExtension {
        mutation: Box<EditorExtensionMutation>,
    },
}

impl ProjectAction {
    fn append_permission_requirements(
        &self,
        requirements: &mut Vec<ApiPermissionRequirement>,
    ) -> Result<()> {
        match self {
            Self::MutateMedia { mutation } => match mutation {
                EditorMediaMutation::SetPath { path, .. }
                | EditorMediaMutation::ConsiderRelink { path, .. } => {
                    requirements.push(ApiPermissionRequirement::filesystem(
                        ApiFilesystemAccess::Read,
                        path.permission_path()?,
                    ));
                }
                EditorMediaMutation::MarkMissing { .. } => {}
            },
            Self::MutateExtension { mutation } => {
                mutation.append_permission_requirements(requirements)?;
            }
            Self::SelectRootTimeline { .. }
            | Self::EditTimeline { .. }
            | Self::MutateGraph { .. }
            | Self::MutateClipMix { .. } => {}
        }
        Ok(())
    }

    fn into_engine(self) -> Result<engine::CompoundProjectAction> {
        match self {
            Self::SelectRootTimeline { timeline_id } => {
                Ok(engine::CompoundProjectAction::SelectRootTimeline(parse_id(
                    &timeline_id,
                    "timeline_id",
                )?))
            }
            Self::EditTimeline { operations } => Ok(engine::CompoundProjectAction::edit_timeline(
                operations
                    .into_iter()
                    .map(TimelineEditOperation::into_engine)
                    .collect::<Result<Vec<_>>>()?,
            )),
            Self::MutateGraph {
                graph_id,
                mutations,
            } => Ok(engine::CompoundProjectAction::mutate_graph(
                parse_id(&graph_id, "graph_id")?,
                mutations
                    .into_iter()
                    .map(EditorGraphMutation::into_engine)
                    .collect::<Result<Vec<_>>>()?,
            )),
            Self::MutateMedia { mutation } => Ok(engine::CompoundProjectAction::MutateMedia(
                mutation.into_engine()?,
            )),
            Self::MutateClipMix { mutations } => {
                Ok(engine::CompoundProjectAction::mutate_clip_mix(
                    mutations
                        .into_iter()
                        .map(EditorClipMixMutation::into_engine)
                        .collect::<Result<Vec<_>>>()?,
                ))
            }
            Self::MutateExtension { mutation } => Ok(
                engine::CompoundProjectAction::mutate_extension((*mutation).into_engine()?),
            ),
        }
    }
}

/// Mutable public facade around one dispatcher with an attached authoritative project history.
pub struct ProjectEditorApi {
    dispatcher: engine::EngineCommandDispatcher,
    pending_event_evidence: BTreeMap<u64, ProjectCommandEvidence>,
    permissions: Arc<ApiPermissionContext>,
}

mod request_sealed {
    pub trait Sealed {}
}

/// One request accepted by the single stable project editor facade.
pub trait ProjectEditorRequest: request_sealed::Sealed + ApiCommand {
    #[doc(hidden)]
    fn dispatch(self, api: &mut ProjectEditorApi) -> Result<<Self as ApiCommand>::Response>;
}

impl request_sealed::Sealed for ExecuteProjectCommand {}

impl ProjectEditorRequest for ExecuteProjectCommand {
    fn dispatch(self, api: &mut ProjectEditorApi) -> Result<<Self as ApiCommand>::Response> {
        api.execute_project_command(self)
    }
}

impl request_sealed::Sealed for GetProjectCommandLog {}

impl ProjectEditorRequest for GetProjectCommandLog {
    fn dispatch(self, api: &mut ProjectEditorApi) -> Result<<Self as ApiCommand>::Response> {
        api.get_project_command_log(self)
    }
}

impl request_sealed::Sealed for GetEditorState {}

impl ProjectEditorRequest for GetEditorState {
    fn dispatch(self, api: &mut ProjectEditorApi) -> Result<<Self as ApiCommand>::Response> {
        crate::state::execute_editor_state(&mut api.dispatcher, self)
    }
}

impl request_sealed::Sealed for RunProjectScript {}

impl ProjectEditorRequest for RunProjectScript {
    fn dispatch(self, api: &mut ProjectEditorApi) -> Result<<Self as ApiCommand>::Response> {
        crate::scripting::execute_project_script(api, self)
    }
}

impl ProjectEditorApi {
    /// Takes ownership of one dispatcher with an attached authoritative project.
    pub fn new(dispatcher: engine::EngineCommandDispatcher) -> Result<Self> {
        Self::new_with_permissions(dispatcher, Arc::new(ApiPermissionContext::default()))
    }

    /// Takes ownership of one dispatcher and binds explicit host permission authority.
    pub fn new_with_permissions(
        dispatcher: engine::EngineCommandDispatcher,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<Self> {
        let _ = dispatcher.project_history_state()?;
        Ok(Self {
            dispatcher,
            pending_event_evidence: BTreeMap::new(),
            permissions,
        })
    }

    /// Executes one stable authored command or complete replacement-state query.
    pub fn execute<R: ProjectEditorRequest>(
        &mut self,
        request: R,
    ) -> Result<<R as ApiCommand>::Response> {
        self.permissions.authorize_command(&request)?;
        request.dispatch(self)
    }

    /// Executes one stable public project command through the engine dispatcher.
    ///
    /// Every public value is converted and checked before dispatch. A conversion or dispatch
    /// failure leaves project state, history, command sequencing, and events unchanged.
    fn execute_project_command(
        &mut self,
        command: ExecuteProjectCommand,
    ) -> Result<ExecuteProjectCommandResult> {
        let serialized_request = serde_json::to_vec(&command).map_err(|source| {
            Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "project command request could not be serialized deterministically",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "serialize_project_command")
                    .with_field("source", source.to_string()),
            )
        })?;
        let (transaction_id, expected_revision, command) = command.into_parts();
        let kind = command.kind();
        let record = engine::ProjectCommandRecordDraft::from_serialized_request(
            transaction_id.clone(),
            EXECUTE_PROJECT_COMMAND_METHOD,
            PROJECT_EDITOR_SCHEMA_VERSION,
            engine_record_kind(kind),
            expected_revision,
            &serialized_request,
        )?;
        let transaction_id = engine::EngineTransactionId::new(transaction_id)?;
        let recorded = match command {
            ProjectCommand::Apply { actions } => {
                let actions = actions
                    .into_iter()
                    .map(ProjectAction::into_engine)
                    .collect::<Result<Vec<_>>>()?;
                let transaction = engine::CompoundProjectTransaction::new(actions)?;
                engine::RecordedProjectCommand::history(
                    engine::ProjectHistoryCommand::apply(
                        expected_revision,
                        engine::ProjectMutation::compound(transaction),
                    ),
                    record,
                )?
            }
            ProjectCommand::Undo {} => engine::RecordedProjectCommand::history(
                engine::ProjectHistoryCommand::undo(expected_revision),
                record,
            )?,
            ProjectCommand::Redo {} => engine::RecordedProjectCommand::history(
                engine::ProjectHistoryCommand::redo(expected_revision),
                record,
            )?,
            ProjectCommand::Inspect {} => {
                engine::RecordedProjectCommand::inspect(expected_revision, record)?
            }
        };
        let dispatched = self.dispatcher.dispatch(engine::EngineCommandRequest::new(
            transaction_id,
            engine::EngineCommand::ExecuteRecordedProjectCommand(Box::new(recorded)),
        ))?;
        let engine::EngineCommandResult::ProjectHistory(outcome) = dispatched.result() else {
            return Err(unexpected_result("execute_project_command"));
        };
        let evidence = public_evidence(outcome)?;
        let command_log_sequence = outcome
            .command_record()
            .ok_or_else(|| unexpected_result("execute_recorded_project_command"))?
            .sequence();
        self.pending_event_evidence
            .insert(dispatched.command_sequence(), evidence.clone());
        Ok(ExecuteProjectCommandResult {
            schema_version: PROJECT_EDITOR_SCHEMA_VERSION,
            transaction_id: dispatched.transaction_id().as_str().to_owned(),
            command_sequence: dispatched.command_sequence(),
            command_kind: kind,
            authored_state_changed: outcome.authored_state_changed(),
            command_log_sequence,
            state: public_history_state(outcome.state()),
            evidence,
        })
    }

    fn get_project_command_log(
        &self,
        request: GetProjectCommandLog,
    ) -> Result<GetProjectCommandLogResult> {
        validate_command_log_query(&request)?;
        let state = self.dispatcher.project_history_state()?;
        let snapshot = state.snapshot();
        let log = snapshot.command_log();
        let latest_sequence = log.latest_sequence();
        if request.after_sequence > latest_sequence {
            return Err(invalid(
                "get_project_command_log",
                "command-log cursor is newer than the selected project",
            ));
        }
        let oldest_available_sequence = log.oldest_sequence();
        if oldest_available_sequence.is_some_and(|oldest| {
            request.after_sequence < latest_sequence
                && request.after_sequence.saturating_add(1) < oldest
        }) {
            return Ok(GetProjectCommandLogResult::ResyncRequired(
                ProjectCommandLogGap {
                    project_id: snapshot.project_id().to_string(),
                    requested_after_sequence: request.after_sequence,
                    oldest_available_sequence: oldest_available_sequence
                        .expect("the gap predicate observed an oldest sequence"),
                    latest_sequence,
                },
            ));
        }

        let mut records = Vec::new();
        for record in log
            .records()
            .iter()
            .filter(|record| record.sequence() > request.after_sequence)
            .take(request.requested_limit as usize)
        {
            let replay_request = if request.detail == ProjectCommandLogDetail::Replayable {
                record
                    .replay_request()
                    .map(|bytes| {
                        let request = serde_json::from_slice::<ExecuteProjectCommand>(bytes)
                            .map_err(|source| {
                                Error::new(
                                    ErrorCategory::CorruptData,
                                    Recoverability::UserCorrectable,
                                    "retained project command request is not decodable",
                                )
                                .with_context(
                                    ErrorContext::new(COMPONENT, "decode_retained_project_command")
                                        .with_field("source", source.to_string()),
                                )
                            })?;
                        self.permissions.authorize_command(&request)?;
                        Ok::<Box<ExecuteProjectCommand>, Error>(Box::new(request))
                    })
                    .transpose()?
            } else {
                None
            };
            records.push(ProjectCommandLogRecord {
                sequence: record.sequence(),
                command_sequence: record.command_sequence(),
                transaction_id: record.transaction_id().to_owned(),
                method: record.method().to_owned(),
                request_schema_version: record.request_schema_version().clone(),
                command_kind: public_record_kind(record.command_kind()),
                expected_project_revision: record.expected_project_revision(),
                request_byte_length: record.request_byte_length(),
                request_sha256: digest_hex(record.request_sha256()),
                payload_disposition: match record.payload_disposition() {
                    engine::ProjectCommandPayloadDisposition::Retained => {
                        ProjectCommandPayloadDisposition::Retained
                    }
                    engine::ProjectCommandPayloadDisposition::DigestOnly => {
                        ProjectCommandPayloadDisposition::DigestOnly
                    }
                    _ => ProjectCommandPayloadDisposition::DigestOnly,
                },
                before_project_revision: record.before_project_revision(),
                after_project_revision: record.after_project_revision(),
                before_semantic_hash: digest_hex(record.before_semantic_hash()),
                after_semantic_hash: digest_hex(record.after_semantic_hash()),
                authored_state_changed: record.authored_state_changed(),
                replay_request,
            });
        }
        let through_sequence = records
            .last()
            .map_or(request.after_sequence, ProjectCommandLogRecord::sequence);
        Ok(GetProjectCommandLogResult::Records(
            ProjectCommandLogBatch {
                schema_version: PROJECT_COMMAND_LOG_SCHEMA_VERSION,
                project_id: snapshot.project_id().to_string(),
                after_sequence: request.after_sequence,
                through_sequence,
                oldest_available_sequence,
                latest_sequence,
                records,
            },
        ))
    }

    /// Drains this facade's ordered generic project replacement events.
    pub fn drain_events(&mut self) -> Result<Vec<ProjectStateChanged>> {
        self.dispatcher
            .drain_events()?
            .into_iter()
            .map(|envelope| {
                let engine::EngineEvent::ProjectStateChanged(state) = envelope.event() else {
                    return Err(unexpected_result("drain_project_events"));
                };
                let evidence = self
                    .pending_event_evidence
                    .remove(&envelope.command_sequence())
                    .ok_or_else(|| {
                        Error::new(
                            ErrorCategory::Internal,
                            Recoverability::Terminal,
                            "project event is missing its public command evidence",
                        )
                        .with_context(ErrorContext::new(COMPONENT, "drain_project_events"))
                    })?;
                Ok(ProjectStateChanged::new(
                    envelope.sequence(),
                    envelope.command_sequence(),
                    envelope.transaction_id().as_str().to_owned(),
                    state.snapshot().revision(),
                    state.snapshot().command_log().latest_sequence(),
                    public_history_state(state),
                    evidence,
                ))
            })
            .collect()
    }

    /// Borrows the selected immutable project snapshot for persistence and in-process consumers.
    pub fn project_snapshot(&self) -> Result<engine::ProjectSnapshot> {
        self.dispatcher.project_snapshot()
    }
}

fn validate_command_log_query(request: &GetProjectCommandLog) -> Result<()> {
    if request.requested_limit == 0
        || request.requested_limit > MAX_PROJECT_COMMAND_LOG_QUERY_RECORDS
    {
        return Err(invalid(
            "get_project_command_log",
            "requested command-log limit is outside the supported bound",
        ));
    }
    Ok(())
}

impl std::fmt::Debug for ProjectEditorApi {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectEditorApi")
            .field("pending_event_evidence", &self.pending_event_evidence.len())
            .finish_non_exhaustive()
    }
}

fn public_history_state(state: &engine::ProjectHistoryState) -> ProjectHistorySnapshot {
    ProjectHistorySnapshot {
        schema_version: PROJECT_EDITOR_SCHEMA_VERSION,
        project_id: state.snapshot().project_id().to_string(),
        project_revision: state.snapshot().revision(),
        root_timeline_id: state.snapshot().root_timeline_id().to_string(),
        capacity: state.capacity(),
        undo_depth: state.undo_depth(),
        redo_depth: state.redo_depth(),
        next_undo: state.next_undo().map(public_mutation_kind),
        next_redo: state.next_redo().map(public_mutation_kind),
        command_log_oldest_sequence: state.snapshot().command_log().oldest_sequence(),
        command_log_latest_sequence: state.snapshot().command_log().latest_sequence(),
        command_log_retained_records: state.snapshot().command_log().len(),
    }
}

const fn engine_record_kind(kind: ProjectCommandKind) -> engine::ProjectCommandRecordKind {
    match kind {
        ProjectCommandKind::Apply => engine::ProjectCommandRecordKind::Apply,
        ProjectCommandKind::Undo => engine::ProjectCommandRecordKind::Undo,
        ProjectCommandKind::Redo => engine::ProjectCommandRecordKind::Redo,
        ProjectCommandKind::Inspect => engine::ProjectCommandRecordKind::Inspect,
    }
}

const fn public_record_kind(kind: engine::ProjectCommandRecordKind) -> ProjectCommandKind {
    match kind {
        engine::ProjectCommandRecordKind::Apply => ProjectCommandKind::Apply,
        engine::ProjectCommandRecordKind::Undo => ProjectCommandKind::Undo,
        engine::ProjectCommandRecordKind::Redo => ProjectCommandKind::Redo,
        engine::ProjectCommandRecordKind::Inspect => ProjectCommandKind::Inspect,
        _ => ProjectCommandKind::Inspect,
    }
}

fn digest_hex(digest: &[u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to a string cannot fail");
    }
    encoded
}

fn public_evidence(outcome: &engine::ProjectHistoryOutcome) -> Result<ProjectCommandEvidence> {
    match outcome.action_result() {
        None => Ok(ProjectCommandEvidence::Inspected {}),
        Some(engine::ProjectHistoryActionResult::AppliedCompound { actions }) => {
            Ok(ProjectCommandEvidence::Applied {
                actions: actions
                    .iter()
                    .map(public_action_evidence)
                    .collect::<Result<Vec<_>>>()?,
            })
        }
        Some(engine::ProjectHistoryActionResult::Applied { mutation, .. })
        | Some(engine::ProjectHistoryActionResult::AppliedExtension { mutation, .. }) => {
            Ok(ProjectCommandEvidence::AppliedMutation {
                mutation: public_mutation_kind(*mutation),
            })
        }
        Some(engine::ProjectHistoryActionResult::Undone(mutation)) => {
            Ok(ProjectCommandEvidence::Undone {
                mutation: public_mutation_kind(*mutation),
            })
        }
        Some(engine::ProjectHistoryActionResult::Redone(mutation)) => {
            Ok(ProjectCommandEvidence::Redone {
                mutation: public_mutation_kind(*mutation),
            })
        }
        Some(_) => Err(unexpected_result("project_history_evidence")),
    }
}

fn public_action_evidence(
    action: &engine::CompoundProjectActionResult,
) -> Result<ProjectActionEvidence> {
    match action {
        engine::CompoundProjectActionResult::RootTimelineSelected(timeline_id) => {
            Ok(ProjectActionEvidence::RootTimelineSelected {
                timeline_id: timeline_id.to_string(),
            })
        }
        engine::CompoundProjectActionResult::TimelineEdited(result) => {
            Ok(ProjectActionEvidence::TimelineEdited {
                revision: result.revision(),
                operations: result
                    .outcomes()
                    .iter()
                    .map(|outcome| public_edit_kind(outcome.kind()))
                    .collect::<Result<Vec<_>>>()?,
            })
        }
        engine::CompoundProjectActionResult::GraphMutated { graph_id, revision } => {
            Ok(ProjectActionEvidence::GraphMutated {
                graph_id: graph_id.to_string(),
                revision: *revision,
            })
        }
        engine::CompoundProjectActionResult::MediaMutated(result) => {
            Ok(ProjectActionEvidence::MediaMutated {
                outcome: public_media_result(*result),
            })
        }
        engine::CompoundProjectActionResult::ClipMixMutated(revision) => {
            Ok(ProjectActionEvidence::ClipMixMutated {
                revision: *revision,
            })
        }
        engine::CompoundProjectActionResult::ExtensionMutated(result) => {
            public_extension_result(result)
        }
        _ => Err(unexpected_result("compound_action_evidence")),
    }
}

fn public_extension_result(
    result: &engine::ProjectExtensionCommandResult,
) -> Result<ProjectActionEvidence> {
    let (outcome, key, replaced) = match result {
        engine::ProjectExtensionCommandResult::Upserted { key, replaced } => {
            (ExtensionMutationResultKind::Upserted, key, Some(*replaced))
        }
        engine::ProjectExtensionCommandResult::Removed(key) => {
            (ExtensionMutationResultKind::Removed, key, None)
        }
        engine::ProjectExtensionCommandResult::LifecycleSet(key) => {
            (ExtensionMutationResultKind::LifecycleSet, key, None)
        }
        engine::ProjectExtensionCommandResult::GrantedCapabilitiesSet(key) => (
            ExtensionMutationResultKind::GrantedCapabilitiesSet,
            key,
            None,
        ),
        engine::ProjectExtensionCommandResult::FailureRecorded(key) => {
            (ExtensionMutationResultKind::FailureRecorded, key, None)
        }
        engine::ProjectExtensionCommandResult::FailureCleared(key) => {
            (ExtensionMutationResultKind::FailureCleared, key, None)
        }
        _ => return Err(unexpected_result("extension_action_evidence")),
    };
    Ok(ProjectActionEvidence::ExtensionMutated {
        outcome,
        extension_id: key.extension_id().to_string(),
        record_id: key.record_id().to_string(),
        replaced,
    })
}

fn public_media_result(result: engine::ProjectMediaCommandResult) -> MediaMutationResult {
    match result {
        engine::ProjectMediaCommandResult::PathUpdated => MediaMutationResult::PathUpdated,
        engine::ProjectMediaCommandResult::MarkedMissing => MediaMutationResult::MarkedMissing,
        engine::ProjectMediaCommandResult::Relink(engine::RelinkDecision::Accepted) => {
            MediaMutationResult::RelinkAccepted
        }
        engine::ProjectMediaCommandResult::Relink(
            engine::RelinkDecision::RejectedFingerprintMismatch,
        ) => MediaMutationResult::RelinkRejected,
        _ => MediaMutationResult::Unknown,
    }
}

fn public_mutation_kind(value: engine::ProjectMutationKind) -> ProjectMutationKind {
    match value {
        engine::ProjectMutationKind::ProjectSettings => ProjectMutationKind::ProjectSettings,
        engine::ProjectMutationKind::Compound => ProjectMutationKind::Compound,
        engine::ProjectMutationKind::SetMediaPath => ProjectMutationKind::SetMediaPath,
        engine::ProjectMutationKind::MarkMediaMissing => ProjectMutationKind::MarkMediaMissing,
        engine::ProjectMutationKind::ConsiderMediaRelink => {
            ProjectMutationKind::ConsiderMediaRelink
        }
        engine::ProjectMutationKind::UpsertExtension => ProjectMutationKind::UpsertExtension,
        engine::ProjectMutationKind::RemoveExtension => ProjectMutationKind::RemoveExtension,
        engine::ProjectMutationKind::SetExtensionLifecycle => {
            ProjectMutationKind::SetExtensionLifecycle
        }
        engine::ProjectMutationKind::SetExtensionCapabilities => {
            ProjectMutationKind::SetExtensionCapabilities
        }
        engine::ProjectMutationKind::RecordExtensionFailure => {
            ProjectMutationKind::RecordExtensionFailure
        }
        engine::ProjectMutationKind::ClearExtensionFailure => {
            ProjectMutationKind::ClearExtensionFailure
        }
        engine::ProjectMutationKind::ExtensionState => ProjectMutationKind::ExtensionState,
        _ => ProjectMutationKind::Unknown,
    }
}

fn public_edit_kind(value: engine::EditKind) -> Result<TimelineEditKind> {
    match value {
        engine::EditKind::Insert => Ok(TimelineEditKind::Insert),
        engine::EditKind::Overwrite => Ok(TimelineEditKind::Overwrite),
        engine::EditKind::Append => Ok(TimelineEditKind::Append),
        engine::EditKind::Replace => Ok(TimelineEditKind::Replace),
        engine::EditKind::Lift => Ok(TimelineEditKind::Lift),
        engine::EditKind::Extract => Ok(TimelineEditKind::Extract),
        engine::EditKind::Ripple => Ok(TimelineEditKind::Ripple),
        engine::EditKind::Roll => Ok(TimelineEditKind::Roll),
        engine::EditKind::Slip => Ok(TimelineEditKind::Slip),
        engine::EditKind::Slide => Ok(TimelineEditKind::Slide),
        engine::EditKind::Razor => Ok(TimelineEditKind::Razor),
        engine::EditKind::Trim => Ok(TimelineEditKind::Trim),
        engine::EditKind::Extend => Ok(TimelineEditKind::Extend),
        engine::EditKind::ThreePoint => Ok(TimelineEditKind::ThreePoint),
        engine::EditKind::FourPoint => Ok(TimelineEditKind::FourPoint),
        _ => Err(unexpected_result("timeline_edit_evidence")),
    }
}

fn parse_id<T>(value: &str, field: &'static str) -> Result<T>
where
    T: FromStr,
{
    T::from_str(value).map_err(|_| {
        invalid(
            "parse_identifier",
            match field {
                "project_id" => "invalid project identifier",
                "timeline_id" => "invalid timeline identifier",
                "track_id" => "invalid track identifier",
                "clip_id" => "invalid clip identifier",
                "gap_id" => "invalid gap identifier",
                "transition_id" => "invalid transition identifier",
                "generator_id" => "invalid generator identifier",
                "caption_id" => "invalid caption identifier",
                "media_id" => "invalid media identifier",
                "multicam_angle_id" => "invalid multicam angle identifier",
                "graph_id" => "invalid graph identifier",
                "node_id" => "invalid node identifier",
                "port_id" => "invalid port identifier",
                "parameter_id" => "invalid parameter identifier",
                "edge_id" => "invalid edge identifier",
                _ => "invalid typed identifier",
            },
        )
    })
}

fn object_ids(values: Vec<EditorialObjectId>) -> Result<Vec<engine::EditorialObjectId>> {
    values
        .into_iter()
        .map(EditorialObjectId::into_engine)
        .collect()
}

fn into_clip(value: EditorTrackItem) -> Result<engine::Clip> {
    match value.into_engine()? {
        engine::TrackItem::Clip(clip) => Ok(clip),
        _ => Err(invalid(
            "timeline_edit",
            "three-point and four-point operations require clip material",
        )),
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unexpected_result(operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "project editor dispatcher returned an unrelated or unsupported result",
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
