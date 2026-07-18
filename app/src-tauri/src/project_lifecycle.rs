//! Desktop project lifecycle presentation above the durable public API host.
//!
//! This module owns only the active project reference, bounded recent-project presentation,
//! recovery presentation, and actionable failure state. Project contents, persistence, recovery
//! artifacts, settings, and media remain owned below this boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use superi_api::commands::{
    ExecuteProjectSettingsTransaction, GetProjectRecovery, GetProjectSettings,
    RestoreProjectRecovery,
};
use superi_api::local::{
    LocalProjectCollision, LocalProjectCreateRequest, LocalProjectHost, LocalProjectSummary,
};
use superi_api::permissions::{
    ApiDestructiveOperation, ApiPermissionContext, ApiPermissionEffect, ApiPermissionRule,
};
use superi_api::project::{ProjectSettingMutation, ProjectSettingValue, ProjectSettingsSnapshot};
use superi_core::diagnostics::UserSafeError;
use superi_core::error::{Error, ErrorCategory, Recoverability};
use tauri::State;

const MAX_RECENT_PROJECTS: usize = 32;

const TIMELINE_RATE_NUMERATOR_KEY: &str = "superi.project.timeline.default_rate_numerator";
const TIMELINE_RATE_DENOMINATOR_KEY: &str = "superi.project.timeline.default_rate_denominator";
const TIMELINE_TIMECODE_MODE_KEY: &str = "superi.project.timeline.timecode_mode";
const COLOR_MODE_KEY: &str = "superi.project.color.mode";
const COLOR_WORKING_SPACE_KEY: &str = "superi.project.color.working_space";
const COLOR_CONFIG_ID_KEY: &str = "superi.project.color.config_id";
const COLOR_CONFIG_FINGERPRINT_KEY: &str = "superi.project.color.config_fingerprint";
const AUDIO_SAMPLE_RATE_KEY: &str = "superi.project.audio.sample_rate_hz";
const AUDIO_OUTPUT_LAYOUT_KEY: &str = "superi.project.audio.output_layout";
const CACHE_MODE_KEY: &str = "superi.project.cache.mode";
const CACHE_MAX_BYTES_KEY: &str = "superi.project.cache.max_bytes";
const CACHE_MAX_FRAMES_KEY: &str = "superi.project.cache.max_frames";
const PROXY_MODE_KEY: &str = "superi.project.proxy.mode";
const PROXY_QUALITY_KEY: &str = "superi.project.proxy.quality";
const RENDER_EXTENT_MODE_KEY: &str = "superi.project.render.extent_mode";
const RENDER_WIDTH_KEY: &str = "superi.project.render.width";
const RENDER_HEIGHT_KEY: &str = "superi.project.render.height";
const WORKING_FOLDER_KEY: &str = "superi.project.working.folder";
const WORKING_CACHE_FOLDER_KEY: &str = "superi.project.working.cache_folder";
const WORKING_PROXY_FOLDER_KEY: &str = "superi.project.working.proxy_folder";

/// Closed actionable failure classification used by the desktop project lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopProjectFailureClass {
    Retryable,
    Degraded,
    UserCorrectable,
    Terminal,
}

/// Reviewed actionable failure data retained beside the last valid project state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectFailure {
    class: DesktopProjectFailureClass,
    code: String,
    title: String,
    action: String,
    context: BTreeMap<String, String>,
}

impl DesktopProjectFailure {
    #[must_use]
    pub fn new(
        class: DesktopProjectFailureClass,
        code: impl Into<String>,
        title: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            class,
            code: code.into(),
            title: title.into(),
            action: action.into(),
            context: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub const fn class(&self) -> DesktopProjectFailureClass {
        self.class
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    #[must_use]
    pub fn context(&self, key: &str) -> Option<&str> {
        self.context.get(key).map(String::as_str)
    }
}

impl fmt::Display for DesktopProjectFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.title)
    }
}

impl std::error::Error for DesktopProjectFailure {}

/// Durable identity returned only after a backend operation succeeds completely.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectIdentity {
    project_id: String,
    project_revision: u64,
    root_timeline_id: String,
}

impl DesktopProjectIdentity {
    pub fn new(
        project_id: impl Into<String>,
        project_revision: u64,
        root_timeline_id: impl Into<String>,
    ) -> Result<Self, DesktopProjectFailure> {
        let identity = Self {
            project_id: project_id.into(),
            project_revision,
            root_timeline_id: root_timeline_id.into(),
        };
        if identity.project_id.trim().is_empty() || identity.root_timeline_id.trim().is_empty() {
            return Err(user_correctable(
                "project_identity_invalid",
                "Project identity is invalid",
                "Choose a valid Superi project and try again.",
            ));
        }
        Ok(identity)
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
}

/// Strict desktop request for creating one minimal durable project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectCreateRequest {
    pub project_id: String,
    pub project_name: String,
    pub root_timeline_id: String,
    pub root_timeline_name: String,
    pub edit_rate_numerator: u32,
    pub edit_rate_denominator: u32,
}

/// One opaque recovery candidate projected for user selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopRecoveryCandidate {
    candidate_id: String,
    project_revision: u64,
    action: String,
}

impl DesktopRecoveryCandidate {
    pub fn new(
        candidate_id: impl Into<String>,
        project_revision: u64,
        action: impl Into<String>,
    ) -> Result<Self, DesktopProjectFailure> {
        let candidate = Self {
            candidate_id: candidate_id.into(),
            project_revision,
            action: action.into(),
        };
        if candidate.candidate_id.trim().is_empty() || candidate.action.trim().is_empty() {
            return Err(user_correctable(
                "recovery_candidate_invalid",
                "Recovery candidate is invalid",
                "Refresh recovery options and choose another candidate.",
            ));
        }
        Ok(candidate)
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }
}

/// Complete revision-fenced recovery presentation for the active project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopRecoveryCatalog {
    catalog_revision: u64,
    candidates: Vec<DesktopRecoveryCandidate>,
}

impl DesktopRecoveryCatalog {
    pub fn new(
        catalog_revision: u64,
        candidates: Vec<DesktopRecoveryCandidate>,
    ) -> Result<Self, DesktopProjectFailure> {
        if catalog_revision == 0 {
            return Err(user_correctable(
                "recovery_catalog_invalid",
                "Recovery catalog is invalid",
                "Refresh recovery options before continuing.",
            ));
        }
        let mut identities = BTreeSet::new();
        if candidates
            .iter()
            .any(|candidate| !identities.insert(candidate.candidate_id.clone()))
        {
            return Err(user_correctable(
                "recovery_catalog_duplicate",
                "Recovery catalog is ambiguous",
                "Refresh recovery options before continuing.",
            ));
        }
        Ok(Self {
            catalog_revision,
            candidates,
        })
    }

    #[must_use]
    pub const fn catalog_revision(&self) -> u64 {
        self.catalog_revision
    }

    #[must_use]
    pub fn candidates(&self) -> &[DesktopRecoveryCandidate] {
        &self.candidates
    }
}

/// Active or recent project presentation. The path is explicit user-selected display state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectRecord {
    path: String,
    identity: DesktopProjectIdentity,
}

impl DesktopProjectRecord {
    fn new(path: String, identity: DesktopProjectIdentity) -> Self {
        Self { path, identity }
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        self.identity.project_id()
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.identity.project_revision()
    }

    #[must_use]
    pub fn root_timeline_id(&self) -> &str {
        self.identity.root_timeline_id()
    }
}

/// Complete replacement state for desktop project lifecycle presentation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectSnapshot {
    revision: u64,
    active: Option<DesktopProjectRecord>,
    recent: Vec<DesktopProjectRecord>,
    recovery: Option<DesktopRecoveryCatalog>,
    failure: Option<DesktopProjectFailure>,
}

impl DesktopProjectSnapshot {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn active(&self) -> Option<&DesktopProjectRecord> {
        self.active.as_ref()
    }

    #[must_use]
    pub fn recent(&self) -> &[DesktopProjectRecord] {
        &self.recent
    }

    #[must_use]
    pub const fn recovery(&self) -> Option<&DesktopRecoveryCatalog> {
        self.recovery.as_ref()
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&DesktopProjectFailure> {
        self.failure.as_ref()
    }
}

/// Complete editable project settings presented by the desktop lifecycle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectSettings {
    project_revision: u64,
    frame_rate_numerator: u32,
    frame_rate_denominator: u32,
    timecode_mode: String,
    resolution_width: Option<u32>,
    resolution_height: Option<u32>,
    color_mode: String,
    color_working_space: String,
    color_config_id: Option<String>,
    color_config_fingerprint: Option<String>,
    audio_sample_rate_hz: u32,
    audio_output_layout: String,
    cache_mode: String,
    cache_max_bytes: Option<u64>,
    cache_max_frames: Option<u64>,
    proxy_mode: String,
    proxy_quality: String,
    working_folder: Option<String>,
    cache_folder: Option<String>,
    proxy_folder: Option<String>,
}

impl DesktopProjectSettings {
    fn from_snapshot(snapshot: &ProjectSettingsSnapshot) -> Result<Self, DesktopProjectFailure> {
        let resolution = match setting_text(snapshot, RENDER_EXTENT_MODE_KEY)?.as_str() {
            "source" => None,
            "explicit" => Some((
                setting_u32(snapshot, RENDER_WIDTH_KEY)?,
                setting_u32(snapshot, RENDER_HEIGHT_KEY)?,
            )),
            _ => return Err(settings_snapshot_invalid(RENDER_EXTENT_MODE_KEY)),
        };
        let (resolution_width, resolution_height) = resolution
            .map(|(width, height)| (Some(width), Some(height)))
            .unwrap_or((None, None));
        let cache_mode = setting_text(snapshot, CACHE_MODE_KEY)?;
        let (cache_max_bytes, cache_max_frames) = if cache_mode == "bounded" {
            (
                Some(setting_u64(snapshot, CACHE_MAX_BYTES_KEY)?),
                Some(setting_u64(snapshot, CACHE_MAX_FRAMES_KEY)?),
            )
        } else {
            (None, None)
        };
        Ok(Self {
            project_revision: snapshot.project_revision(),
            frame_rate_numerator: setting_u32(snapshot, TIMELINE_RATE_NUMERATOR_KEY)?,
            frame_rate_denominator: setting_u32(snapshot, TIMELINE_RATE_DENOMINATOR_KEY)?,
            timecode_mode: setting_text(snapshot, TIMELINE_TIMECODE_MODE_KEY)?,
            resolution_width,
            resolution_height,
            color_mode: setting_text(snapshot, COLOR_MODE_KEY)?,
            color_working_space: setting_text(snapshot, COLOR_WORKING_SPACE_KEY)?,
            color_config_id: optional_setting_text(snapshot, COLOR_CONFIG_ID_KEY)?,
            color_config_fingerprint: optional_setting_text(
                snapshot,
                COLOR_CONFIG_FINGERPRINT_KEY,
            )?,
            audio_sample_rate_hz: setting_u32(snapshot, AUDIO_SAMPLE_RATE_KEY)?,
            audio_output_layout: setting_text(snapshot, AUDIO_OUTPUT_LAYOUT_KEY)?,
            cache_mode,
            cache_max_bytes,
            cache_max_frames,
            proxy_mode: setting_text(snapshot, PROXY_MODE_KEY)?,
            proxy_quality: setting_text(snapshot, PROXY_QUALITY_KEY)?,
            working_folder: optional_setting_text(snapshot, WORKING_FOLDER_KEY)?,
            cache_folder: optional_setting_text(snapshot, WORKING_CACHE_FOLDER_KEY)?,
            proxy_folder: optional_setting_text(snapshot, WORKING_PROXY_FOLDER_KEY)?,
        })
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub const fn frame_rate(&self) -> (u32, u32) {
        (self.frame_rate_numerator, self.frame_rate_denominator)
    }

    #[must_use]
    pub const fn resolution(&self) -> Option<(u32, u32)> {
        match (self.resolution_width, self.resolution_height) {
            (Some(width), Some(height)) => Some((width, height)),
            _ => None,
        }
    }

    #[must_use]
    pub fn color_mode(&self) -> &str {
        &self.color_mode
    }

    #[must_use]
    pub fn color_working_space(&self) -> &str {
        &self.color_working_space
    }

    #[must_use]
    pub const fn audio_sample_rate_hz(&self) -> u32 {
        self.audio_sample_rate_hz
    }

    #[must_use]
    pub fn audio_output_layout(&self) -> &str {
        &self.audio_output_layout
    }

    #[must_use]
    pub fn cache_mode(&self) -> &str {
        &self.cache_mode
    }

    #[must_use]
    pub const fn cache_budget(&self) -> Option<(u64, u64)> {
        match (self.cache_max_bytes, self.cache_max_frames) {
            (Some(bytes), Some(frames)) => Some((bytes, frames)),
            _ => None,
        }
    }

    #[must_use]
    pub fn proxy_mode(&self) -> &str {
        &self.proxy_mode
    }

    #[must_use]
    pub fn proxy_quality(&self) -> &str {
        &self.proxy_quality
    }

    #[must_use]
    pub fn working_folder(&self) -> Option<&str> {
        self.working_folder.as_deref()
    }

    #[must_use]
    pub fn cache_folder(&self) -> Option<&str> {
        self.cache_folder.as_deref()
    }

    #[must_use]
    pub fn proxy_folder(&self) -> Option<&str> {
        self.proxy_folder.as_deref()
    }
}

/// One optimistic full replacement of the desktop-owned project settings surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectSettingsUpdate {
    pub expected_project_revision: u64,
    pub frame_rate_numerator: u32,
    pub frame_rate_denominator: u32,
    pub timecode_mode: String,
    pub resolution_width: Option<u32>,
    pub resolution_height: Option<u32>,
    pub color_mode: String,
    pub color_working_space: String,
    pub color_config_id: Option<String>,
    pub color_config_fingerprint: Option<String>,
    pub audio_sample_rate_hz: u32,
    pub audio_output_layout: String,
    pub cache_mode: String,
    pub cache_max_bytes: Option<u64>,
    pub cache_max_frames: Option<u64>,
    pub proxy_mode: String,
    pub proxy_quality: String,
    pub working_folder: Option<String>,
    pub cache_folder: Option<String>,
    pub proxy_folder: Option<String>,
}

impl DesktopProjectSettingsUpdate {
    fn into_transaction(self) -> Result<ExecuteProjectSettingsTransaction, DesktopProjectFailure> {
        let mut mutations = vec![
            set_integer(
                TIMELINE_RATE_NUMERATOR_KEY,
                i64::from(self.frame_rate_numerator),
            ),
            set_integer(
                TIMELINE_RATE_DENOMINATOR_KEY,
                i64::from(self.frame_rate_denominator),
            ),
            set_text(TIMELINE_TIMECODE_MODE_KEY, self.timecode_mode),
            set_text(COLOR_MODE_KEY, self.color_mode),
            set_text(COLOR_WORKING_SPACE_KEY, self.color_working_space),
            set_integer(AUDIO_SAMPLE_RATE_KEY, i64::from(self.audio_sample_rate_hz)),
            set_text(AUDIO_OUTPUT_LAYOUT_KEY, self.audio_output_layout),
            set_text(CACHE_MODE_KEY, self.cache_mode),
            set_text(PROXY_MODE_KEY, self.proxy_mode),
            set_text(PROXY_QUALITY_KEY, self.proxy_quality),
        ];
        match (self.resolution_width, self.resolution_height) {
            (Some(width), Some(height)) => {
                mutations.push(set_text(RENDER_EXTENT_MODE_KEY, "explicit"));
                mutations.push(set_integer(RENDER_WIDTH_KEY, i64::from(width)));
                mutations.push(set_integer(RENDER_HEIGHT_KEY, i64::from(height)));
            }
            (None, None) => {
                mutations.push(set_text(RENDER_EXTENT_MODE_KEY, "source"));
                mutations.push(remove_setting(RENDER_WIDTH_KEY));
                mutations.push(remove_setting(RENDER_HEIGHT_KEY));
            }
            _ => {
                return Err(user_correctable(
                    "project_settings_resolution_incomplete",
                    "Project resolution is incomplete",
                    "Provide both resolution dimensions or clear both values.",
                ));
            }
        }
        replace_optional_text(&mut mutations, COLOR_CONFIG_ID_KEY, self.color_config_id);
        replace_optional_text(
            &mut mutations,
            COLOR_CONFIG_FINGERPRINT_KEY,
            self.color_config_fingerprint,
        );
        replace_optional_integer(&mut mutations, CACHE_MAX_BYTES_KEY, self.cache_max_bytes)?;
        replace_optional_integer(&mut mutations, CACHE_MAX_FRAMES_KEY, self.cache_max_frames)?;
        replace_optional_text(&mut mutations, WORKING_FOLDER_KEY, self.working_folder);
        replace_optional_text(&mut mutations, WORKING_CACHE_FOLDER_KEY, self.cache_folder);
        replace_optional_text(&mut mutations, WORKING_PROXY_FOLDER_KEY, self.proxy_folder);
        Ok(ExecuteProjectSettingsTransaction::new(
            format!(
                "superi-desktop-project-settings-{}",
                self.expected_project_revision
            ),
            self.expected_project_revision,
            mutations,
        ))
    }
}

fn setting_text(
    snapshot: &ProjectSettingsSnapshot,
    key: &'static str,
) -> Result<String, DesktopProjectFailure> {
    match snapshot.value(key) {
        Some(ProjectSettingValue::Text(value)) => Ok(value.clone()),
        _ => Err(settings_snapshot_invalid(key)),
    }
}

fn optional_setting_text(
    snapshot: &ProjectSettingsSnapshot,
    key: &'static str,
) -> Result<Option<String>, DesktopProjectFailure> {
    match snapshot.value(key) {
        None => Ok(None),
        Some(ProjectSettingValue::Text(value)) => Ok(Some(value.clone())),
        _ => Err(settings_snapshot_invalid(key)),
    }
}

fn setting_u64(
    snapshot: &ProjectSettingsSnapshot,
    key: &'static str,
) -> Result<u64, DesktopProjectFailure> {
    match snapshot.value(key) {
        Some(ProjectSettingValue::Integer(value)) => {
            u64::try_from(*value).map_err(|_| settings_snapshot_invalid(key))
        }
        _ => Err(settings_snapshot_invalid(key)),
    }
}

fn setting_u32(
    snapshot: &ProjectSettingsSnapshot,
    key: &'static str,
) -> Result<u32, DesktopProjectFailure> {
    setting_u64(snapshot, key)
        .and_then(|value| u32::try_from(value).map_err(|_| settings_snapshot_invalid(key)))
}

fn settings_snapshot_invalid(key: &'static str) -> DesktopProjectFailure {
    DesktopProjectFailure::new(
        DesktopProjectFailureClass::Degraded,
        "project_settings_snapshot_invalid",
        "Project settings could not be displayed",
        "Close and reopen the project before editing its settings.",
    )
    .with_context("key", key)
}

fn set_text(key: &'static str, value: impl Into<String>) -> ProjectSettingMutation {
    ProjectSettingMutation::Set {
        key: key.to_owned(),
        value: ProjectSettingValue::Text(value.into()),
    }
}

fn set_integer(key: &'static str, value: i64) -> ProjectSettingMutation {
    ProjectSettingMutation::Set {
        key: key.to_owned(),
        value: ProjectSettingValue::Integer(value),
    }
}

fn remove_setting(key: &'static str) -> ProjectSettingMutation {
    ProjectSettingMutation::Remove {
        key: key.to_owned(),
    }
}

fn replace_optional_text(
    mutations: &mut Vec<ProjectSettingMutation>,
    key: &'static str,
    value: Option<String>,
) {
    mutations.push(match value {
        Some(value) => set_text(key, value),
        None => remove_setting(key),
    });
}

fn replace_optional_integer(
    mutations: &mut Vec<ProjectSettingMutation>,
    key: &'static str,
    value: Option<u64>,
) -> Result<(), DesktopProjectFailure> {
    mutations.push(match value {
        Some(value) => set_integer(
            key,
            i64::try_from(value).map_err(|_| {
                user_correctable(
                    "project_settings_integer_too_large",
                    "Project setting is too large",
                    "Choose a smaller cache budget.",
                )
                .with_context("key", key)
            })?,
        ),
        None => remove_setting(key),
    });
    Ok(())
}

/// Durable operation boundary implemented by the existing API-owned local project host.
pub trait DesktopProjectBackend {
    fn create(
        &mut self,
        path: &Path,
        request: &DesktopProjectCreateRequest,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure>;

    fn open(&mut self, path: &Path) -> Result<DesktopProjectIdentity, DesktopProjectFailure>;

    fn save(&mut self, path: &Path) -> Result<DesktopProjectIdentity, DesktopProjectFailure>;

    fn save_as(
        &mut self,
        path: &Path,
        destination: &Path,
        replace_existing: bool,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure>;

    fn discover_recovery(
        &mut self,
        path: &Path,
    ) -> Result<DesktopRecoveryCatalog, DesktopProjectFailure>;

    fn restore_recovery(
        &mut self,
        path: &Path,
        catalog_revision: u64,
        candidate_id: &str,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure>;
}

/// Concrete durable backend over the existing API-owned local project host.
pub struct LocalProjectBackend {
    recovery_root: PathBuf,
}

impl LocalProjectBackend {
    #[must_use]
    pub fn new(recovery_root: PathBuf) -> Self {
        Self { recovery_root }
    }

    fn identity(
        summary: &LocalProjectSummary,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        DesktopProjectIdentity::new(
            summary.project_id(),
            summary.project_revision(),
            summary.root_timeline_id(),
        )
    }

    fn validate(
        path: &Path,
        operation: &'static str,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        let validation =
            LocalProjectHost::validate(path).map_err(|error| safe_failure(operation, error))?;
        Self::identity(validation.project())
    }

    fn inspect_settings(
        &self,
        path: &Path,
    ) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
        let response = LocalProjectHost::inspect_settings(path, GetProjectSettings::new())
            .map_err(|error| safe_failure("inspect_settings", error))?;
        DesktopProjectSettings::from_snapshot(response.snapshot())
    }

    fn update_settings(
        &mut self,
        path: &Path,
        update: DesktopProjectSettingsUpdate,
    ) -> Result<(DesktopProjectSettings, DesktopProjectIdentity), DesktopProjectFailure> {
        let response = LocalProjectHost::execute_settings(
            path,
            update.into_transaction()?,
            Arc::new(ApiPermissionContext::default()),
        )
        .map_err(|error| safe_failure("update_settings", error))?;
        let settings = DesktopProjectSettings::from_snapshot(response.result().snapshot())?;
        let identity = Self::validate(path, "update_settings")?;
        Ok((settings, identity))
    }

    fn restore_permissions() -> Result<Arc<ApiPermissionContext>, DesktopProjectFailure> {
        ApiPermissionContext::new(
            "superi.desktop.project",
            [ApiPermissionRule::destructive(
                ApiPermissionEffect::Allow,
                ApiDestructiveOperation::RestoreProjectRecovery,
            )],
        )
        .map(Arc::new)
        .map_err(|error| safe_failure("restore_recovery", error))
    }
}

impl DesktopProjectBackend for LocalProjectBackend {
    fn create(
        &mut self,
        path: &Path,
        request: &DesktopProjectCreateRequest,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        let summary = LocalProjectHost::create(
            path,
            LocalProjectCreateRequest {
                project_id: request.project_id.clone(),
                project_name: request.project_name.clone(),
                root_timeline_id: request.root_timeline_id.clone(),
                root_timeline_name: request.root_timeline_name.clone(),
                edit_rate_numerator: request.edit_rate_numerator,
                edit_rate_denominator: request.edit_rate_denominator,
            },
        )
        .map_err(|error| safe_failure("create", error))?;
        Self::identity(&summary)
    }

    fn open(&mut self, path: &Path) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        Self::validate(path, "open")
    }

    fn save(&mut self, path: &Path) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        LocalProjectHost::save(path).map_err(|error| safe_failure("save", error))?;
        Self::validate(path, "save")
    }

    fn save_as(
        &mut self,
        path: &Path,
        destination: &Path,
        replace_existing: bool,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        let collision = if replace_existing {
            LocalProjectCollision::ReplaceExisting
        } else {
            LocalProjectCollision::RequireAbsent
        };
        LocalProjectHost::save_as(path, destination, collision)
            .map_err(|error| safe_failure("save_as", error))?;
        Self::validate(destination, "save_as")
    }

    fn discover_recovery(
        &mut self,
        path: &Path,
    ) -> Result<DesktopRecoveryCatalog, DesktopProjectFailure> {
        let response = LocalProjectHost::recovery_get(
            path,
            &self.recovery_root,
            GetProjectRecovery::new("superi-desktop-recovery-get"),
            Arc::new(ApiPermissionContext::default()),
        )
        .map_err(|error| safe_failure("discover_recovery", error))?;
        let snapshot = response.result().snapshot();
        let candidates = snapshot
            .candidates()
            .iter()
            .map(|candidate| {
                DesktopRecoveryCandidate::new(
                    candidate.candidate_id(),
                    candidate.captured_project_revision(),
                    format!(
                        "Recover autosaved project revision {}.",
                        candidate.captured_project_revision()
                    ),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        DesktopRecoveryCatalog::new(snapshot.catalog_revision(), candidates)
    }

    fn restore_recovery(
        &mut self,
        path: &Path,
        catalog_revision: u64,
        candidate_id: &str,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        let current = Self::validate(path, "restore_recovery")?;
        let response = LocalProjectHost::recovery_restore(
            path,
            &self.recovery_root,
            RestoreProjectRecovery::new(
                "superi-desktop-recovery-restore",
                catalog_revision,
                current.project_revision(),
                candidate_id,
            ),
            Self::restore_permissions()?,
        )
        .map_err(|error| safe_failure("restore_recovery", error))?;
        if !response.result().restored() {
            return Err(DesktopProjectFailure::new(
                DesktopProjectFailureClass::Degraded,
                "recovery_not_restored",
                "Project recovery did not complete",
                "Refresh recovery options and try another recovery point.",
            )
            .with_context("operation", "restore_recovery"));
        }
        Self::validate(path, "restore_recovery")
    }
}

/// Tauri-managed concrete project lifecycle state.
#[derive(Clone, Default)]
pub struct DesktopProjectState(Arc<Mutex<Option<DesktopProjectLifecycle<LocalProjectBackend>>>>);

impl DesktopProjectState {
    pub fn initialize(&self, recovery_root: PathBuf) -> Result<(), DesktopProjectFailure> {
        std::fs::create_dir_all(&recovery_root).map_err(|source| {
            safe_failure(
                "initialize_recovery",
                Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "desktop recovery storage could not be initialized",
                    source,
                ),
            )
        })?;
        let lifecycle = DesktopProjectLifecycle::new(LocalProjectBackend::new(recovery_root), 12)?;
        *self.lock("initialize")? = Some(lifecycle);
        Ok(())
    }

    pub fn snapshot(&self) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
        self.lock("snapshot")?
            .as_ref()
            .map(|lifecycle| lifecycle.snapshot().clone())
            .ok_or_else(not_initialized)
    }

    pub fn execute(
        &self,
        command: DesktopProjectCommand,
    ) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
        self.lock("execute")?
            .as_mut()
            .ok_or_else(not_initialized)?
            .execute(command)
    }

    pub fn settings(&self) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
        self.lock("settings")?
            .as_ref()
            .ok_or_else(not_initialized)?
            .inspect_settings()
    }

    pub fn update_settings(
        &self,
        update: DesktopProjectSettingsUpdate,
    ) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
        self.lock("update_settings")?
            .as_mut()
            .ok_or_else(not_initialized)?
            .update_settings(update)
    }

    fn lock(
        &self,
        operation: &'static str,
    ) -> Result<
        std::sync::MutexGuard<'_, Option<DesktopProjectLifecycle<LocalProjectBackend>>>,
        DesktopProjectFailure,
    > {
        self.0.lock().map_err(|_| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "project_lifecycle_poisoned",
                "Project lifecycle cannot continue",
                "Restart Superi before continuing.",
            )
            .with_context("operation", operation)
        })
    }
}

#[tauri::command]
pub fn desktop_project_snapshot(
    state: State<'_, DesktopProjectState>,
) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
    state.snapshot()
}

#[tauri::command]
pub async fn desktop_project_execute(
    command: DesktopProjectCommand,
    state: State<'_, DesktopProjectState>,
) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.execute(command))
        .await
        .map_err(|_| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "project_task_failed",
                "Project operation could not finish",
                "Restart Superi before continuing.",
            )
            .with_context("operation", "join")
        })?
}

#[tauri::command]
pub async fn desktop_project_settings(
    state: State<'_, DesktopProjectState>,
) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.settings())
        .await
        .map_err(|_| project_task_failed("inspect_settings"))?
}

#[tauri::command]
pub async fn desktop_project_settings_update(
    update: DesktopProjectSettingsUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.update_settings(update))
        .await
        .map_err(|_| project_task_failed("update_settings"))?
}

/// Closed desktop command set for project lifecycle behavior only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DesktopProjectCommand {
    Create {
        path: String,
        project: DesktopProjectCreateRequest,
    },
    Open {
        path: String,
    },
    OpenRecent {
        path: String,
    },
    Save,
    SaveAs {
        destination: String,
        replace_existing: bool,
    },
    Close,
    DiscoverRecovery,
    RestoreRecovery {
        catalog_revision: u64,
        candidate_id: String,
    },
}

/// Sole mutable desktop project lifecycle owner.
pub struct DesktopProjectLifecycle<B> {
    backend: B,
    recent_capacity: usize,
    snapshot: DesktopProjectSnapshot,
}

impl<B: DesktopProjectBackend> DesktopProjectLifecycle<B> {
    pub fn new(backend: B, recent_capacity: usize) -> Result<Self, DesktopProjectFailure> {
        if recent_capacity == 0 || recent_capacity > MAX_RECENT_PROJECTS {
            return Err(user_correctable(
                "recent_project_capacity_invalid",
                "Recent-project capacity is invalid",
                "Use a recent-project capacity between 1 and 32.",
            ));
        }
        Ok(Self {
            backend,
            recent_capacity,
            snapshot: DesktopProjectSnapshot {
                revision: 0,
                active: None,
                recent: Vec::new(),
                recovery: None,
                failure: None,
            },
        })
    }

    #[must_use]
    pub const fn snapshot(&self) -> &DesktopProjectSnapshot {
        &self.snapshot
    }

    pub fn execute(
        &mut self,
        command: DesktopProjectCommand,
    ) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
        let result = match command {
            DesktopProjectCommand::Create { path, project } => {
                let path = validate_path(path, "create")?;
                self.backend
                    .create(Path::new(&path), &project)
                    .map(|identity| ProjectTransition::Activate { path, identity })
            }
            DesktopProjectCommand::Open { path } => {
                let path = validate_path(path, "open")?;
                self.backend
                    .open(Path::new(&path))
                    .map(|identity| ProjectTransition::Activate { path, identity })
            }
            DesktopProjectCommand::OpenRecent { path } => {
                let path = validate_path(path, "open_recent")?;
                if !self.snapshot.recent.iter().any(|entry| entry.path == path) {
                    Err(user_correctable(
                        "recent_project_missing",
                        "Recent project is unavailable",
                        "Choose a project that is still present in the recent-project list.",
                    ))
                } else {
                    self.backend
                        .open(Path::new(&path))
                        .map(|identity| ProjectTransition::Activate { path, identity })
                }
            }
            DesktopProjectCommand::Save => {
                let active = self.active_record("save")?.clone();
                self.backend
                    .save(Path::new(&active.path))
                    .map(|identity| ProjectTransition::RefreshActive { identity })
            }
            DesktopProjectCommand::SaveAs {
                destination,
                replace_existing,
            } => {
                let active = self.active_record("save_as")?.clone();
                let destination = validate_path(destination, "save_as")?;
                self.backend
                    .save_as(
                        Path::new(&active.path),
                        Path::new(&destination),
                        replace_existing,
                    )
                    .map(|identity| ProjectTransition::Activate {
                        path: destination,
                        identity,
                    })
            }
            DesktopProjectCommand::Close => Ok(ProjectTransition::Close),
            DesktopProjectCommand::DiscoverRecovery => {
                let active = self.active_record("discover_recovery")?.clone();
                self.backend
                    .discover_recovery(Path::new(&active.path))
                    .map(ProjectTransition::Recovery)
            }
            DesktopProjectCommand::RestoreRecovery {
                catalog_revision,
                candidate_id,
            } => {
                let active = self.active_record("restore_recovery")?.clone();
                let catalog = self.snapshot.recovery.as_ref().ok_or_else(|| {
                    user_correctable(
                        "recovery_catalog_missing",
                        "Recovery options are unavailable",
                        "Refresh recovery options before restoring a project.",
                    )
                })?;
                if catalog.catalog_revision != catalog_revision
                    || !catalog
                        .candidates
                        .iter()
                        .any(|candidate| candidate.candidate_id == candidate_id)
                {
                    Err(user_correctable(
                        "recovery_selection_stale",
                        "Recovery selection is stale",
                        "Refresh recovery options and choose the candidate again.",
                    ))
                } else {
                    self.backend
                        .restore_recovery(Path::new(&active.path), catalog_revision, &candidate_id)
                        .map(|identity| ProjectTransition::Restored { identity })
                }
            }
        };

        match result {
            Ok(transition) => {
                self.commit(transition)?;
                Ok(self.snapshot.clone())
            }
            Err(failure) => {
                self.record_failure(failure.clone());
                Err(failure)
            }
        }
    }

    fn active_record(
        &self,
        operation: &'static str,
    ) -> Result<&DesktopProjectRecord, DesktopProjectFailure> {
        self.snapshot.active.as_ref().ok_or_else(|| {
            user_correctable(
                "project_not_open",
                "No project is open",
                "Create or open a project before continuing.",
            )
            .with_context("operation", operation)
        })
    }

    fn commit(&mut self, transition: ProjectTransition) -> Result<(), DesktopProjectFailure> {
        match transition {
            ProjectTransition::Activate { path, identity } => {
                let record = DesktopProjectRecord::new(path, identity);
                self.snapshot.active = Some(record.clone());
                self.snapshot.recovery = None;
                self.push_recent(record);
            }
            ProjectTransition::RefreshActive { identity } => {
                let path = self.active_record("save")?.path.clone();
                let record = DesktopProjectRecord::new(path, identity);
                self.snapshot.active = Some(record.clone());
                if let Some(recent) = self
                    .snapshot
                    .recent
                    .iter_mut()
                    .find(|recent| recent.path == record.path)
                {
                    *recent = record;
                }
            }
            ProjectTransition::Close => {
                self.snapshot.active = None;
                self.snapshot.recovery = None;
            }
            ProjectTransition::Recovery(catalog) => {
                self.snapshot.recovery = Some(catalog);
            }
            ProjectTransition::Restored { identity } => {
                let path = self.active_record("restore_recovery")?.path.clone();
                let record = DesktopProjectRecord::new(path, identity);
                self.snapshot.active = Some(record.clone());
                self.snapshot.recovery = None;
                if let Some(recent) = self
                    .snapshot
                    .recent
                    .iter_mut()
                    .find(|recent| recent.path == record.path)
                {
                    *recent = record;
                }
            }
        }
        self.snapshot.failure = None;
        self.advance_revision()
    }

    fn push_recent(&mut self, record: DesktopProjectRecord) {
        self.snapshot
            .recent
            .retain(|recent| recent.path != record.path);
        self.snapshot.recent.insert(0, record);
        self.snapshot.recent.truncate(self.recent_capacity);
    }

    fn record_failure(&mut self, failure: DesktopProjectFailure) {
        self.snapshot.failure = Some(failure);
        if self.advance_revision().is_err() {
            self.snapshot.failure = Some(DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "project_lifecycle_revision_exhausted",
                "Project lifecycle cannot continue",
                "Restart Superi before continuing.",
            ));
        }
    }

    fn advance_revision(&mut self) -> Result<(), DesktopProjectFailure> {
        self.snapshot.revision = self.snapshot.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "project_lifecycle_revision_exhausted",
                "Project lifecycle cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        Ok(())
    }
}

impl DesktopProjectLifecycle<LocalProjectBackend> {
    /// Inspects settings for the currently active durable project.
    pub fn inspect_settings(&self) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
        let active = self.active_record("inspect_settings")?;
        self.backend.inspect_settings(Path::new(active.path()))
    }

    /// Applies one optimistic settings replacement and refreshes lifecycle revision identity.
    pub fn update_settings(
        &mut self,
        update: DesktopProjectSettingsUpdate,
    ) -> Result<DesktopProjectSettings, DesktopProjectFailure> {
        let path = self.active_record("update_settings")?.path().to_owned();
        match self.backend.update_settings(Path::new(&path), update) {
            Ok((settings, identity)) => {
                self.commit(ProjectTransition::RefreshActive { identity })?;
                Ok(settings)
            }
            Err(failure) => {
                self.record_failure(failure.clone());
                Err(failure)
            }
        }
    }
}

enum ProjectTransition {
    Activate {
        path: String,
        identity: DesktopProjectIdentity,
    },
    RefreshActive {
        identity: DesktopProjectIdentity,
    },
    Close,
    Recovery(DesktopRecoveryCatalog),
    Restored {
        identity: DesktopProjectIdentity,
    },
}

fn validate_path(path: String, operation: &'static str) -> Result<String, DesktopProjectFailure> {
    if path.trim().is_empty() || path.contains('\0') {
        return Err(user_correctable(
            "project_path_invalid",
            "Project path is invalid",
            "Choose a valid Superi project file.",
        )
        .with_context("operation", operation));
    }
    Ok(path)
}

fn user_correctable(
    code: &'static str,
    title: &'static str,
    action: &'static str,
) -> DesktopProjectFailure {
    DesktopProjectFailure::new(
        DesktopProjectFailureClass::UserCorrectable,
        code,
        title,
        action,
    )
}

fn safe_failure(operation: &'static str, error: Error) -> DesktopProjectFailure {
    let safe = UserSafeError::from_error(&error);
    let class = match safe.recoverability() {
        Recoverability::Retryable => DesktopProjectFailureClass::Retryable,
        Recoverability::Degraded => DesktopProjectFailureClass::Degraded,
        Recoverability::UserCorrectable => DesktopProjectFailureClass::UserCorrectable,
        Recoverability::Terminal => DesktopProjectFailureClass::Terminal,
        _ => DesktopProjectFailureClass::Terminal,
    };
    DesktopProjectFailure::new(class, safe.code(), safe.title(), safe.action())
        .with_context("operation", operation)
}

fn not_initialized() -> DesktopProjectFailure {
    DesktopProjectFailure::new(
        DesktopProjectFailureClass::Terminal,
        "project_lifecycle_not_initialized",
        "Project lifecycle is not ready",
        "Restart Superi before continuing.",
    )
}

fn project_task_failed(operation: &'static str) -> DesktopProjectFailure {
    DesktopProjectFailure::new(
        DesktopProjectFailureClass::Terminal,
        "project_task_failed",
        "Project operation could not finish",
        "Restart Superi before continuing.",
    )
    .with_context("operation", operation)
}
