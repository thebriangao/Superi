//! Persistent referenced-media paths and atomic media commands.
//!
//! Timeline state retains stable media identity and opaque target text. This module owns the
//! project-file context that turns a versioned target into a local path, while leaving unknown
//! locators untouched for interchange and future extensions.

use std::fmt;
use std::path::{Path, PathBuf};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::MediaId;
use superi_timeline::media::RelinkDecision;
use superi_timeline::model::{EditorialProject, LinkedMediaReference};

use crate::document::{ProjectDocument, ProjectDraft, ProjectGraph, ProjectSnapshot};

const COMPONENT: &str = "superi-project.media";
const MEDIA_PATH_TARGET_PREFIX: &str = "superi.media-path.v1:";
const MEDIA_PATH_TARGET_NAMESPACE: &str = "superi.media-path.";

/// Stable target format used for project-owned filesystem references.
pub const MEDIA_PATH_TARGET_FORMAT: &str = "superi.media-path.v1";

/// A canonical relative path that has one syntax on every supported platform.
///
/// Components use `/`. Repeated separators and complete `.` components are removed. Complete
/// `..` components are reduced lexically while leading parent traversal is preserved. Normal
/// components reject the Windows-invalid characters, device names, and trailing forms that would
/// make the reference host-specific.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PortableRelativePath(String);

impl PortableRelativePath {
    /// Parses and canonicalizes a portable project-relative path.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(invalid_path(
                "create_portable_relative_path",
                "portable relative media path must not be empty",
                &value,
            ));
        }
        if value.starts_with('/') {
            return Err(invalid_path(
                "create_portable_relative_path",
                "portable relative media path must not have a root",
                &value,
            ));
        }

        let mut segments: Vec<String> = Vec::new();
        for segment in value.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    if segments.last().is_some_and(|last| last != "..") {
                        segments.pop();
                    } else {
                        segments.push(segment.to_owned());
                    }
                }
                _ => {
                    validate_portable_segment(segment, &value)?;
                    segments.push(segment.to_owned());
                }
            }
        }

        if segments.is_empty() {
            return Err(invalid_path(
                "create_portable_relative_path",
                "portable relative media path must identify a resource",
                &value,
            ));
        }
        Ok(Self(segments.join("/")))
    }

    /// Returns the canonical slash-separated path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PortableRelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The filesystem syntax recorded with a host-absolute media path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MediaPathPlatform {
    /// POSIX-style absolute paths used by macOS and Linux.
    Unix,
    /// Drive, UNC, or device paths used by Windows.
    Windows,
}

impl MediaPathPlatform {
    /// Returns the path syntax of the current build target.
    #[must_use]
    pub const fn current() -> Self {
        #[cfg(windows)]
        {
            Self::Windows
        }
        #[cfg(not(windows))]
        {
            Self::Unix
        }
    }

    const fn code(self) -> &'static str {
        match self {
            Self::Unix => "unix",
            Self::Windows => "windows",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code {
            "unix" => Some(Self::Unix),
            "windows" => Some(Self::Windows),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReferencedMediaPathKind {
    ProjectRelative(PortableRelativePath),
    HostAbsolute {
        platform: MediaPathPlatform,
        path: String,
    },
}

/// One typed filesystem target retained by a linked media reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferencedMediaPath(ReferencedMediaPathKind);

impl ReferencedMediaPath {
    /// Creates a project-relative reference from an already validated portable path.
    #[must_use]
    pub fn project_relative(path: PortableRelativePath) -> Self {
        Self(ReferencedMediaPathKind::ProjectRelative(path))
    }

    /// Creates a current-host absolute reference that can be serialized without data loss.
    pub fn host_absolute(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(invalid_path(
                "create_host_absolute_path",
                "host media path must be absolute",
                &path.display().to_string(),
            ));
        }
        let path = path.to_str().ok_or_else(|| {
            media_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "create_host_absolute_path",
                "host media path must be valid UTF-8 for stable persistence",
            )
        })?;
        validate_absolute_text(
            MediaPathPlatform::current(),
            path,
            "create_host_absolute_path",
        )?;
        Ok(Self(ReferencedMediaPathKind::HostAbsolute {
            platform: MediaPathPlatform::current(),
            path: path.to_owned(),
        }))
    }

    /// Decodes the stable target format or a compatible legacy path target.
    ///
    /// URI and other opaque locators return `None`. A target in a future Superi media-path format
    /// fails explicitly so a caller cannot silently reinterpret its schema.
    pub fn from_target(target: &str) -> Result<Option<Self>> {
        if let Some(encoded) = target.strip_prefix(MEDIA_PATH_TARGET_PREFIX) {
            return decode_current_target(encoded).map(Some);
        }
        if target.starts_with(MEDIA_PATH_TARGET_NAMESPACE) {
            return Err(media_error(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "decode_media_path_target",
                "media path target uses an unsupported format version",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "decode_media_path_target")
                    .with_field("target", target.to_owned()),
            ));
        }

        let legacy_path = Path::new(target);
        if legacy_path.is_absolute() {
            validate_absolute_text(
                MediaPathPlatform::current(),
                target,
                "decode_legacy_absolute_path",
            )?;
            return Ok(Some(Self(ReferencedMediaPathKind::HostAbsolute {
                platform: MediaPathPlatform::current(),
                path: target.to_owned(),
            })));
        }
        if has_uri_scheme(target) {
            return Ok(None);
        }
        Ok(PortableRelativePath::new(target)
            .ok()
            .map(Self::project_relative))
    }

    /// Returns the platform recorded for a host-absolute target.
    #[must_use]
    pub const fn platform(&self) -> Option<MediaPathPlatform> {
        match &self.0 {
            ReferencedMediaPathKind::ProjectRelative(_) => None,
            ReferencedMediaPathKind::HostAbsolute { platform, .. } => Some(*platform),
        }
    }

    /// Returns the portable relative path when this target moves with the project.
    #[must_use]
    pub const fn portable_relative_path(&self) -> Option<&PortableRelativePath> {
        match &self.0 {
            ReferencedMediaPathKind::ProjectRelative(path) => Some(path),
            ReferencedMediaPathKind::HostAbsolute { .. } => None,
        }
    }

    /// Encodes this path as stable target text retained by timeline and project serialization.
    #[must_use]
    pub fn to_target(&self) -> String {
        match &self.0 {
            ReferencedMediaPathKind::ProjectRelative(path) => {
                format!("{MEDIA_PATH_TARGET_PREFIX}relative:{}", path.as_str())
            }
            ReferencedMediaPathKind::HostAbsolute { platform, path } => format!(
                "{MEDIA_PATH_TARGET_PREFIX}absolute:{}:{path}",
                platform.code()
            ),
        }
    }

    /// Resolves this target without consulting the filesystem or process current directory.
    ///
    /// Relative targets require the absolute path of the owning `.superi` file. Host-absolute
    /// targets resolve only when their recorded syntax matches the current platform.
    pub fn resolve(&self, project_file: impl AsRef<Path>) -> Result<PathBuf> {
        match &self.0 {
            ReferencedMediaPathKind::ProjectRelative(path) => {
                let project_file = project_file.as_ref();
                if !project_file.is_absolute() {
                    return Err(invalid_path(
                        "resolve_project_media_path",
                        "project file path must be absolute for deterministic media resolution",
                        &project_file.display().to_string(),
                    ));
                }
                let parent = project_file.parent().ok_or_else(|| {
                    invalid_path(
                        "resolve_project_media_path",
                        "project file path must have a parent directory",
                        &project_file.display().to_string(),
                    )
                })?;
                let mut resolved = parent.to_path_buf();
                for segment in path.as_str().split('/') {
                    resolved.push(segment);
                }
                Ok(resolved)
            }
            ReferencedMediaPathKind::HostAbsolute { platform, path } => {
                if *platform != MediaPathPlatform::current() {
                    return Err(media_error(
                        ErrorCategory::Unsupported,
                        Recoverability::UserCorrectable,
                        "resolve_host_absolute_path",
                        "host-absolute media path belongs to another platform",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "resolve_host_absolute_path")
                            .with_field("stored_platform", platform.code())
                            .with_field("current_platform", MediaPathPlatform::current().code()),
                    ));
                }
                let resolved = PathBuf::from(path);
                if !resolved.is_absolute() {
                    return Err(media_error(
                        ErrorCategory::CorruptData,
                        Recoverability::UserCorrectable,
                        "resolve_host_absolute_path",
                        "stored host media path is not absolute on its declared platform",
                    ));
                }
                Ok(resolved)
            }
        }
    }
}

/// One MediaId-keyed mutation available through the project document boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectMediaCommand {
    /// Changes the active path without claiming content verification.
    SetPath {
        /// Stable project media identity.
        media_id: MediaId,
        /// New active filesystem target.
        path: ReferencedMediaPath,
    },
    /// Marks the active path unavailable without changing identity.
    MarkMissing {
        /// Stable project media identity.
        media_id: MediaId,
    },
    /// Checks a candidate path against persistent content identity.
    ConsiderRelink {
        /// Stable project media identity.
        media_id: MediaId,
        /// Candidate filesystem target.
        path: ReferencedMediaPath,
        /// Fingerprint observed from the candidate source.
        observed_fingerprint: String,
    },
}

impl ProjectMediaCommand {
    /// Creates an unverified path-change command.
    #[must_use]
    pub const fn set_path(media_id: MediaId, path: ReferencedMediaPath) -> Self {
        Self::SetPath { media_id, path }
    }

    /// Creates a missing-media command.
    #[must_use]
    pub const fn mark_missing(media_id: MediaId) -> Self {
        Self::MarkMissing { media_id }
    }

    /// Creates a fingerprint-checked relink command.
    #[must_use]
    pub fn consider_relink(
        media_id: MediaId,
        path: ReferencedMediaPath,
        observed_fingerprint: impl Into<String>,
    ) -> Self {
        Self::ConsiderRelink {
            media_id,
            path,
            observed_fingerprint: observed_fingerprint.into(),
        }
    }

    /// Returns the persistent identity addressed by this command.
    #[must_use]
    pub const fn media_id(&self) -> MediaId {
        match self {
            Self::SetPath { media_id, .. }
            | Self::MarkMissing { media_id }
            | Self::ConsiderRelink { media_id, .. } => *media_id,
        }
    }
}

/// The semantic result of one published project media command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectMediaCommandResult {
    /// The active target changed and is awaiting fingerprint verification.
    PathUpdated,
    /// The active target is explicitly missing.
    MarkedMissing,
    /// A fingerprint-checked candidate was accepted or rejected.
    Relink(RelinkDecision),
}

/// One media command result paired with the exact published project snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectMediaCommandOutcome {
    snapshot: ProjectSnapshot,
    result: ProjectMediaCommandResult,
}

impl ProjectMediaCommandOutcome {
    /// Returns the immutable project snapshot published by the command.
    #[must_use]
    pub const fn snapshot(&self) -> &ProjectSnapshot {
        &self.snapshot
    }

    /// Returns the semantic command result.
    #[must_use]
    pub const fn result(&self) -> ProjectMediaCommandResult {
        self.result
    }

    /// Consumes the outcome and returns its published snapshot.
    #[must_use]
    pub fn into_snapshot(self) -> ProjectSnapshot {
        self.snapshot
    }
}

impl ProjectDocument {
    /// Returns one linked media path by stable identity.
    pub fn media_path(&self, media_id: MediaId) -> Result<ReferencedMediaPath> {
        project_media_path(self.editorial_project(), media_id)
    }

    /// Applies one path or relink command as an atomic whole-project revision.
    ///
    /// Timeline graph documents are retained byte-for-byte because media availability and target
    /// paths are not processing parameters. Checked compilation provenance is regenerated around
    /// those graphs so direct graph edits survive the editorial revision change.
    pub fn execute_media_command(
        &mut self,
        expected_revision: u64,
        command: ProjectMediaCommand,
    ) -> Result<ProjectMediaCommandOutcome> {
        let media_id = command.media_id();
        self.check_revision("execute_media_command", expected_revision)?;
        let current_media = self
            .editorial_project()
            .media_reference(media_id)
            .ok_or_else(|| missing_media_reference(media_id))?;
        let mut preview = current_media.clone();
        let preview_result = apply_media_command(&mut preview, &command)?;
        if preview == *current_media {
            return Ok(ProjectMediaCommandOutcome {
                snapshot: self.snapshot(),
                result: preview_result,
            });
        }
        let mut command_result = None;
        let snapshot = self.edit(expected_revision, |draft| {
            command_result = Some(draft.execute_media_command(&command)?);
            Ok(())
        })?;

        let result = command_result.ok_or_else(|| {
            media_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "execute_media_command",
                "published media command did not retain its semantic result",
            )
        })?;
        Ok(ProjectMediaCommandOutcome { snapshot, result })
    }
}

impl ProjectDraft<'_> {
    /// Applies one media command inside an existing whole-project transaction.
    pub fn execute_media_command(
        &mut self,
        command: &ProjectMediaCommand,
    ) -> Result<ProjectMediaCommandResult> {
        let media_id = command.media_id();
        let current_media = self
            .editorial_project()
            .media_reference(media_id)
            .ok_or_else(|| missing_media_reference(media_id))?;
        let mut preview = current_media.clone();
        let result = apply_media_command(&mut preview, command)?;
        if preview == *current_media {
            return Ok(result);
        }

        let retained_timeline_graphs = self
            .graphs()
            .filter_map(|graph| {
                graph
                    .root_timeline_id()
                    .map(|root| (root, graph.graph().clone()))
            })
            .collect::<Vec<_>>();
        let editorial_revision = self.editorial_project().revision();
        self.editorial_project_mut()
            .edit(editorial_revision, |editorial| {
                let media = editorial.media_reference_mut(media_id)?;
                apply_media_command(media, command).map(|_| ())
            })?;

        for (root_timeline_id, graph) in retained_timeline_graphs {
            let refreshed =
                ProjectGraph::restore_timeline(self.editorial_project(), root_timeline_id, graph)?;
            self.replace_graph(refreshed);
        }
        Ok(result)
    }
}

fn apply_media_command(
    media: &mut LinkedMediaReference,
    command: &ProjectMediaCommand,
) -> Result<ProjectMediaCommandResult> {
    Ok(match command {
        ProjectMediaCommand::SetPath { path, .. } => {
            media.set_target(path.to_target());
            ProjectMediaCommandResult::PathUpdated
        }
        ProjectMediaCommand::MarkMissing { .. } => {
            media.mark_missing();
            ProjectMediaCommandResult::MarkedMissing
        }
        ProjectMediaCommand::ConsiderRelink {
            path,
            observed_fingerprint,
            ..
        } => ProjectMediaCommandResult::Relink(
            media.consider_relink(path.to_target(), observed_fingerprint.clone())?,
        ),
    })
}

impl ProjectSnapshot {
    /// Returns one linked media path by stable identity.
    pub fn media_path(&self, media_id: MediaId) -> Result<ReferencedMediaPath> {
        project_media_path(self.editorial_project(), media_id)
    }
}

fn project_media_path(
    project: &EditorialProject,
    media_id: MediaId,
) -> Result<ReferencedMediaPath> {
    let media = project.media_reference(media_id).ok_or_else(|| {
        media_error(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "resolve_project_media_reference",
            "linked media identity was not found in the project",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "resolve_project_media_reference")
                .with_field("media_id", media_id.to_string()),
        )
    })?;
    ReferencedMediaPath::from_target(media.target())?.ok_or_else(|| {
        media_error(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "resolve_project_media_reference",
            "linked media target is not a supported filesystem path",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "resolve_project_media_reference")
                .with_field("media_id", media_id.to_string())
                .with_field("target", media.target().to_owned()),
        )
    })
}

fn missing_media_reference(media_id: MediaId) -> Error {
    media_error(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        "execute_media_command",
        "linked media identity was not found in the project",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "execute_media_command")
            .with_field("media_id", media_id.to_string()),
    )
}

fn decode_current_target(encoded: &str) -> Result<ReferencedMediaPath> {
    if let Some(path) = encoded.strip_prefix("relative:") {
        return PortableRelativePath::new(path).map(ReferencedMediaPath::project_relative);
    }
    if let Some(encoded) = encoded.strip_prefix("absolute:") {
        let (platform, path) = encoded.split_once(':').ok_or_else(|| {
            corrupt_target(
                "absolute media path target is missing its platform or path",
                encoded,
            )
        })?;
        let platform = MediaPathPlatform::from_code(platform).ok_or_else(|| {
            corrupt_target(
                "absolute media path target has an unknown platform",
                encoded,
            )
        })?;
        validate_absolute_text(platform, path, "decode_media_path_target")?;
        return Ok(ReferencedMediaPath(ReferencedMediaPathKind::HostAbsolute {
            platform,
            path: path.to_owned(),
        }));
    }
    Err(corrupt_target(
        "media path target has an unknown version-1 representation",
        encoded,
    ))
}

fn validate_portable_segment(segment: &str, complete_path: &str) -> Result<()> {
    if segment.chars().any(|character| {
        character.is_control()
            || matches!(character, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*')
    }) {
        return Err(invalid_path(
            "create_portable_relative_path",
            "portable media path contains a platform-specific or control character",
            complete_path,
        ));
    }
    if segment.ends_with(' ') || segment.ends_with('.') {
        return Err(invalid_path(
            "create_portable_relative_path",
            "portable media path component must not end with a space or period",
            complete_path,
        ));
    }
    let device_name = segment
        .split_once('.')
        .map_or(segment, |(base, _)| base)
        .to_ascii_uppercase();
    if is_reserved_windows_device_name(&device_name) {
        return Err(invalid_path(
            "create_portable_relative_path",
            "portable media path contains a reserved device name",
            complete_path,
        ));
    }
    Ok(())
}

fn is_reserved_windows_device_name(value: &str) -> bool {
    matches!(value, "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            value.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        })
}

fn validate_absolute_text(
    platform: MediaPathPlatform,
    path: &str,
    operation: &'static str,
) -> Result<()> {
    if path.is_empty() || path.chars().any(char::is_control) {
        return Err(invalid_path(
            operation,
            "absolute media path must be nonempty and contain no control characters",
            path,
        ));
    }
    let valid = match platform {
        MediaPathPlatform::Unix => path.starts_with('/'),
        MediaPathPlatform::Windows => is_windows_absolute(path),
    };
    if !valid {
        return Err(invalid_path(
            operation,
            "absolute media path does not match its declared platform",
            path,
        ));
    }
    Ok(())
}

fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    let unc_absolute = path.starts_with("\\\\") || path.starts_with("//");
    drive_absolute || unc_absolute
}

fn has_uri_scheme(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    let mut characters = scheme.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
}

fn corrupt_target(message: &'static str, target: &str) -> Error {
    media_error(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        "decode_media_path_target",
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "decode_media_path_target")
            .with_field("target", target.to_owned()),
    )
}

fn invalid_path(operation: &'static str, message: &'static str, path: &str) -> Error {
    media_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("path", path.to_owned()))
}

fn media_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
