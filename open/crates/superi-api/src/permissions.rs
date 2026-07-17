//! Host-injected permission policy for the stable public API surface.
//!
//! Permission contexts are process-owned authority and are deliberately not serializable. Rules,
//! scopes, and derived requirements are typed values so hosts can inspect, persist, and test policy
//! without exposing engine ownership or relying on unstructured permission strings.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{CapabilityId, ComponentId};

use crate::commands::ApiCommand;

const COMPONENT: &str = "superi-api.permissions";

/// The protected subsystem addressed by one public API permission requirement.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApiPermissionKind {
    /// Filesystem access to one typed path.
    Filesystem,
    /// Durable plugin state, lifecycle, or capability delegation.
    Plugin,
    /// An explicitly destructive public operation.
    Destructive,
}

impl ApiPermissionKind {
    /// Every permission kind in stable schema order.
    pub const ALL: &'static [Self] = &[Self::Filesystem, Self::Plugin, Self::Destructive];

    /// Returns the permanent public code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Filesystem => "filesystem",
            Self::Plugin => "plugin",
            Self::Destructive => "destructive",
        }
    }
}

/// How a method obtains its permission requirements.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApiPermissionRequirementMode {
    /// The method has no protected operation in this API version.
    None,
    /// The method always requires the same typed operation.
    Static,
    /// The complete request payload determines the exact requirement set.
    PayloadDependent,
}

impl ApiPermissionRequirementMode {
    /// Every requirement mode in stable schema order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Static, Self::PayloadDependent];
}

/// Whether a matching policy rule grants or explicitly rejects authority.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApiPermissionEffect {
    /// Grants the matching requirement when no deny rule also matches.
    Allow,
    /// Rejects the matching requirement even when another rule allows it.
    Deny,
}

impl ApiPermissionEffect {
    /// Every rule effect in stable schema order.
    pub const ALL: &'static [Self] = &[Self::Allow, Self::Deny];
}

/// Filesystem operation requested by a public command.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApiFilesystemAccess {
    /// Read or inspect one target.
    Read,
    /// Create or replace one target.
    Write,
    /// Delete one target.
    Delete,
}

impl ApiFilesystemAccess {
    /// Every filesystem access code in stable schema order.
    pub const ALL: &'static [Self] = &[Self::Read, Self::Write, Self::Delete];

    #[must_use]
    const fn code(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Delete => "delete",
        }
    }
}

/// Filesystem syntax carried by an absolute path.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApiFilesystemPlatform {
    /// POSIX path syntax used by macOS and Linux.
    Unix,
    /// Drive or UNC syntax used by Windows.
    Windows,
}

impl ApiFilesystemPlatform {
    /// Every declared platform in stable schema order.
    pub const ALL: &'static [Self] = &[Self::Unix, Self::Windows];

    /// Returns the syntax of the current build target.
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
}

#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ApiFilesystemPathValue {
    ProjectRelative {
        path: String,
    },
    Absolute {
        platform: ApiFilesystemPlatform,
        path: String,
    },
}

/// A canonical project-relative or declared-platform absolute filesystem target.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiFilesystemPath(ApiFilesystemPathValue);

impl ApiFilesystemPath {
    /// Creates a canonical portable relative path and rejects traversal above its root.
    pub fn project_relative(path: impl Into<String>) -> Result<Self> {
        let canonical = normalize_project_relative(&path.into())?;
        Ok(Self(ApiFilesystemPathValue::ProjectRelative {
            path: canonical,
        }))
    }

    /// Creates a canonical absolute path using the declared syntax.
    pub fn absolute(platform: ApiFilesystemPlatform, path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        let canonical = normalize_absolute(platform, &path)?.canonical;
        Ok(Self(ApiFilesystemPathValue::Absolute {
            platform,
            path: canonical,
        }))
    }

    /// Classifies one native host path as absolute or portable relative input.
    pub fn native(path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        let platform = ApiFilesystemPlatform::current();
        if is_absolute_for(platform, &path) {
            Self::absolute(platform, path)
        } else {
            #[cfg(windows)]
            let path = path.replace('\\', "/");
            Self::project_relative(path)
        }
    }

    /// Returns the canonical portable path when this target is project-relative.
    #[must_use]
    pub fn as_project_relative(&self) -> Option<&str> {
        match &self.0 {
            ApiFilesystemPathValue::ProjectRelative { path } => Some(path),
            ApiFilesystemPathValue::Absolute { .. } => None,
        }
    }

    /// Returns the declared platform and canonical path when this target is absolute.
    #[must_use]
    pub fn as_absolute(&self) -> Option<(ApiFilesystemPlatform, &str)> {
        match &self.0 {
            ApiFilesystemPathValue::Absolute { platform, path } => Some((*platform, path)),
            ApiFilesystemPathValue::ProjectRelative { .. } => None,
        }
    }

    fn normalized(&self) -> Result<NormalizedPath> {
        match &self.0 {
            ApiFilesystemPathValue::ProjectRelative { path } => Ok(NormalizedPath {
                root: NormalizedRoot::Project,
                components: normalize_relative_components(path)?,
                canonical: normalize_project_relative(path)?,
            }),
            ApiFilesystemPathValue::Absolute { platform, path } => {
                normalize_absolute(*platform, path)
            }
        }
    }

    fn validate(&self) -> Result<()> {
        let canonical = self.normalized()?.canonical;
        let retained = match &self.0 {
            ApiFilesystemPathValue::ProjectRelative { path }
            | ApiFilesystemPathValue::Absolute { path, .. } => path,
        };
        if canonical != *retained {
            return Err(invalid(
                "validate_filesystem_path",
                "permission path is not canonical",
            ));
        }
        Ok(())
    }
}

/// Exact or recursive filesystem policy scope.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApiFilesystemScope {
    /// Matches only the canonical target itself.
    Exact { path: ApiFilesystemPath },
    /// Matches the canonical root and descendants at component boundaries.
    Recursive { root: ApiFilesystemPath },
}

impl ApiFilesystemScope {
    /// Creates one exact target scope.
    #[must_use]
    pub const fn exact(path: ApiFilesystemPath) -> Self {
        Self::Exact { path }
    }

    /// Creates one root-and-descendants scope.
    #[must_use]
    pub const fn recursive(root: ApiFilesystemPath) -> Self {
        Self::Recursive { root }
    }

    fn matches(&self, target: &ApiFilesystemPath) -> Result<bool> {
        let target = target.normalized()?;
        let (scope, recursive) = match self {
            Self::Exact { path } => (path.normalized()?, false),
            Self::Recursive { root } => (root.normalized()?, true),
        };
        if scope.root != target.root {
            return Ok(false);
        }
        if recursive {
            Ok(target.components.starts_with(&scope.components))
        } else {
            Ok(scope.components == target.components)
        }
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::Exact { path } => path.validate(),
            Self::Recursive { root } => root.validate(),
        }
    }
}

/// Protected operation on durable plugin state or delegated authority.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApiPluginOperation {
    /// Create, replace, remove, or record state and failure evidence.
    ManageState,
    /// Change plugin lifecycle state.
    ManageLifecycle,
    /// Grant a capability subset to plugin-owned state.
    DelegateCapabilities,
}

impl ApiPluginOperation {
    /// Every plugin operation in stable schema order.
    pub const ALL: &'static [Self] = &[
        Self::ManageState,
        Self::ManageLifecycle,
        Self::DelegateCapabilities,
    ];

    #[must_use]
    const fn code(self) -> &'static str {
        match self {
            Self::ManageState => "manage_state",
            Self::ManageLifecycle => "manage_lifecycle",
            Self::DelegateCapabilities => "delegate_capabilities",
        }
    }
}

/// Exact plugin identity or all validated plugin identities.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApiPluginScope {
    /// One exact canonical extension identity.
    Exact { extension_id: String },
    /// Every canonical extension identity.
    All {},
}

impl ApiPluginScope {
    /// Creates one exact validated extension scope.
    pub fn exact(extension_id: impl Into<String>) -> Result<Self> {
        let extension_id = extension_id.into();
        validate_component("validate_plugin_scope", &extension_id)?;
        Ok(Self::Exact { extension_id })
    }

    /// Creates an all-extensions scope.
    #[must_use]
    pub const fn all() -> Self {
        Self::All {}
    }

    fn matches(&self, extension_id: &str) -> bool {
        match self {
            Self::Exact {
                extension_id: expected,
            } => expected == extension_id,
            Self::All {} => true,
        }
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::Exact { extension_id } => {
                validate_component("validate_plugin_scope", extension_id)
            }
            Self::All {} => Ok(()),
        }
    }
}

/// Explicitly destructive operation exposed by the current public API.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApiDestructiveOperation {
    /// Cooperatively cancel one or all asynchronous jobs.
    CancelAsyncJob,
    /// Remove one finalized asynchronous job record.
    RemoveAsyncJob,
    /// Restore a project recovery candidate over current authored state.
    RestoreProjectRecovery,
    /// Durably dismiss a managed project recovery candidate.
    DismissProjectRecovery,
    /// Remove an authored audio automation lane or keyframe.
    RemoveAudioAutomation,
}

impl ApiDestructiveOperation {
    /// Every destructive operation in stable schema order.
    pub const ALL: &'static [Self] = &[
        Self::CancelAsyncJob,
        Self::RemoveAsyncJob,
        Self::RestoreProjectRecovery,
        Self::DismissProjectRecovery,
        Self::RemoveAudioAutomation,
    ];

    #[must_use]
    const fn code(self) -> &'static str {
        match self {
            Self::CancelAsyncJob => "cancel_async_job",
            Self::RemoveAsyncJob => "remove_async_job",
            Self::RestoreProjectRecovery => "restore_project_recovery",
            Self::DismissProjectRecovery => "dismiss_project_recovery",
            Self::RemoveAudioAutomation => "remove_audio_automation",
        }
    }
}

/// One exact permission requirement derived from a complete typed command payload.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApiPermissionRequirement {
    /// Filesystem access to one canonical target.
    Filesystem {
        access: ApiFilesystemAccess,
        target: ApiFilesystemPath,
    },
    /// Plugin state, lifecycle, or delegated capability authority.
    Plugin {
        operation: ApiPluginOperation,
        extension_id: String,
        delegated_capabilities: Vec<String>,
    },
    /// One explicitly destructive semantic operation.
    Destructive { operation: ApiDestructiveOperation },
}

impl ApiPermissionRequirement {
    /// Creates one filesystem requirement.
    #[must_use]
    pub const fn filesystem(access: ApiFilesystemAccess, target: ApiFilesystemPath) -> Self {
        Self::Filesystem { access, target }
    }

    /// Creates a nondelegating plugin state or lifecycle requirement.
    pub fn plugin(operation: ApiPluginOperation, extension_id: impl Into<String>) -> Result<Self> {
        if operation == ApiPluginOperation::DelegateCapabilities {
            return Err(invalid(
                "create_plugin_requirement",
                "delegation requirements must declare their capability set",
            ));
        }
        let extension_id = extension_id.into();
        validate_component("create_plugin_requirement", &extension_id)?;
        Ok(Self::Plugin {
            operation,
            extension_id,
            delegated_capabilities: Vec::new(),
        })
    }

    /// Creates one capability delegation requirement.
    pub fn plugin_delegation<I, S>(extension_id: impl Into<String>, capabilities: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let extension_id = extension_id.into();
        validate_component("create_plugin_requirement", &extension_id)?;
        let delegated_capabilities = canonical_capabilities(capabilities)?;
        Ok(Self::Plugin {
            operation: ApiPluginOperation::DelegateCapabilities,
            extension_id,
            delegated_capabilities,
        })
    }

    /// Creates one destructive operation requirement.
    #[must_use]
    pub const fn destructive(operation: ApiDestructiveOperation) -> Self {
        Self::Destructive { operation }
    }

    /// Returns the protected permission kind.
    #[must_use]
    pub const fn kind(&self) -> ApiPermissionKind {
        match self {
            Self::Filesystem { .. } => ApiPermissionKind::Filesystem,
            Self::Plugin { .. } => ApiPermissionKind::Plugin,
            Self::Destructive { .. } => ApiPermissionKind::Destructive,
        }
    }

    fn operation_code(&self) -> &'static str {
        match self {
            Self::Filesystem { access, .. } => access.code(),
            Self::Plugin { operation, .. } => operation.code(),
            Self::Destructive { operation } => operation.code(),
        }
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::Filesystem { target, .. } => target.validate(),
            Self::Plugin {
                operation,
                extension_id,
                delegated_capabilities,
            } => {
                validate_component("validate_plugin_requirement", extension_id)?;
                validate_canonical_capabilities(delegated_capabilities)?;
                match operation {
                    ApiPluginOperation::DelegateCapabilities => Ok(()),
                    _ if delegated_capabilities.is_empty() => Ok(()),
                    _ => Err(invalid(
                        "validate_plugin_requirement",
                        "nondelegating plugin requirement contains capabilities",
                    )),
                }
            }
            Self::Destructive { .. } => Ok(()),
        }
    }
}

/// A canonical deduplicated set of permission requirements.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiPermissionRequirements(Vec<ApiPermissionRequirement>);

impl ApiPermissionRequirements {
    /// Builds a deterministic requirement set and validates every target and identity.
    pub fn new(requirements: impl IntoIterator<Item = ApiPermissionRequirement>) -> Result<Self> {
        let requirements = requirements.into_iter().collect::<BTreeSet<_>>();
        for requirement in &requirements {
            requirement.validate()?;
        }
        Ok(Self(requirements.into_iter().collect()))
    }

    /// Returns an empty requirement set.
    #[must_use]
    pub const fn none() -> Self {
        Self(Vec::new())
    }

    /// Returns requirements in stable semantic order.
    #[must_use]
    pub fn as_slice(&self) -> &[ApiPermissionRequirement] {
        &self.0
    }

    /// Returns the number of distinct requirements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether no protected operation is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn validate(&self) -> Result<()> {
        let canonical = Self::new(self.0.clone())?;
        if canonical != *self {
            return Err(invalid(
                "validate_permission_requirements",
                "permission requirements are not canonical",
            ));
        }
        Ok(())
    }
}

/// One allow or deny rule in a host-owned policy context.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApiPermissionRule {
    /// Filesystem rule over one exact or recursive target scope.
    Filesystem {
        effect: ApiPermissionEffect,
        access: ApiFilesystemAccess,
        scope: ApiFilesystemScope,
    },
    /// Plugin rule over one exact identity or all identities.
    Plugin {
        effect: ApiPermissionEffect,
        operation: ApiPluginOperation,
        scope: ApiPluginScope,
        delegation_ceiling: Option<Vec<String>>,
    },
    /// Destructive operation rule.
    Destructive {
        effect: ApiPermissionEffect,
        operation: ApiDestructiveOperation,
    },
}

impl ApiPermissionRule {
    /// Creates one filesystem rule.
    #[must_use]
    pub const fn filesystem(
        effect: ApiPermissionEffect,
        access: ApiFilesystemAccess,
        scope: ApiFilesystemScope,
    ) -> Self {
        Self::Filesystem {
            effect,
            access,
            scope,
        }
    }

    /// Creates a nondelegating plugin state or lifecycle rule.
    pub fn plugin(
        effect: ApiPermissionEffect,
        operation: ApiPluginOperation,
        scope: ApiPluginScope,
    ) -> Result<Self> {
        if operation == ApiPluginOperation::DelegateCapabilities {
            return Err(invalid(
                "create_plugin_rule",
                "delegation rules must declare a capability ceiling",
            ));
        }
        scope.validate()?;
        Ok(Self::Plugin {
            effect,
            operation,
            scope,
            delegation_ceiling: None,
        })
    }

    /// Creates a plugin delegation rule bounded to a capability ceiling.
    pub fn plugin_delegation<I, S>(
        effect: ApiPermissionEffect,
        scope: ApiPluginScope,
        capabilities: I,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        scope.validate()?;
        let capabilities = canonical_capabilities(capabilities)?;
        Ok(Self::Plugin {
            effect,
            operation: ApiPluginOperation::DelegateCapabilities,
            scope,
            delegation_ceiling: Some(capabilities),
        })
    }

    /// Creates an explicit deny rule for all capability delegation in one plugin scope.
    pub fn deny_all_plugin_delegation(scope: ApiPluginScope) -> Result<Self> {
        scope.validate()?;
        Ok(Self::Plugin {
            effect: ApiPermissionEffect::Deny,
            operation: ApiPluginOperation::DelegateCapabilities,
            scope,
            delegation_ceiling: None,
        })
    }

    /// Creates one destructive operation rule.
    #[must_use]
    pub const fn destructive(
        effect: ApiPermissionEffect,
        operation: ApiDestructiveOperation,
    ) -> Self {
        Self::Destructive { effect, operation }
    }

    #[must_use]
    const fn effect(&self) -> ApiPermissionEffect {
        match self {
            Self::Filesystem { effect, .. }
            | Self::Plugin { effect, .. }
            | Self::Destructive { effect, .. } => *effect,
        }
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::Filesystem { scope, .. } => scope.validate(),
            Self::Plugin {
                effect,
                operation,
                scope,
                delegation_ceiling,
            } => {
                scope.validate()?;
                if let Some(capabilities) = delegation_ceiling {
                    validate_canonical_capabilities(capabilities)?;
                }
                match operation {
                    ApiPluginOperation::DelegateCapabilities => {
                        if *effect == ApiPermissionEffect::Allow && delegation_ceiling.is_none() {
                            Err(invalid(
                                "validate_plugin_rule",
                                "allow delegation rule has no capability ceiling",
                            ))
                        } else {
                            Ok(())
                        }
                    }
                    _ if delegation_ceiling.is_none() => Ok(()),
                    _ => Err(invalid(
                        "validate_plugin_rule",
                        "nondelegating plugin rule has a capability ceiling",
                    )),
                }
            }
            Self::Destructive { .. } => Ok(()),
        }
    }

    fn matches(&self, requirement: &ApiPermissionRequirement) -> Result<bool> {
        match (self, requirement) {
            (
                Self::Filesystem { access, scope, .. },
                ApiPermissionRequirement::Filesystem {
                    access: required,
                    target,
                },
            ) => Ok(access == required && scope.matches(target)?),
            (
                Self::Plugin {
                    operation,
                    scope,
                    delegation_ceiling,
                    ..
                },
                ApiPermissionRequirement::Plugin {
                    operation: required,
                    extension_id,
                    delegated_capabilities,
                },
            ) => {
                if operation != required || !scope.matches(extension_id) {
                    return Ok(false);
                }
                if *operation != ApiPluginOperation::DelegateCapabilities {
                    return Ok(true);
                }
                Ok(match delegation_ceiling {
                    None => self.effect() == ApiPermissionEffect::Deny,
                    Some(ceiling) => contains_all(ceiling, delegated_capabilities),
                })
            }
            (
                Self::Destructive { operation, .. },
                ApiPermissionRequirement::Destructive {
                    operation: required,
                },
            ) => Ok(operation == required),
            _ => Ok(false),
        }
    }
}

/// Nonserializable host authority bound to one public API facade.
#[derive(Clone, Eq, PartialEq)]
pub struct ApiPermissionContext {
    principal: ComponentId,
    rules: Vec<ApiPermissionRule>,
}

impl ApiPermissionContext {
    /// Creates one validated context. No rules means deny all protected operations.
    pub fn new<I>(principal: impl AsRef<str>, rules: I) -> Result<Self>
    where
        I: IntoIterator<Item = ApiPermissionRule>,
    {
        let principal = ComponentId::new(principal.as_ref()).map_err(|_| {
            invalid(
                "create_permission_context",
                "permission context principal is not a canonical component identity",
            )
        })?;
        let rules = rules.into_iter().collect::<Vec<_>>();
        for rule in &rules {
            rule.validate()?;
        }
        Ok(Self { principal, rules })
    }

    /// Returns the canonical host principal identity.
    #[must_use]
    pub const fn principal(&self) -> &ComponentId {
        &self.principal
    }

    /// Derives and authorizes one complete typed command before conversion or dispatch.
    pub fn authorize_command<C: ApiCommand>(&self, command: &C) -> Result<()> {
        let requirements = command.permission_requirements()?;
        self.authorize(C::METHOD, &requirements)
    }

    /// Authorizes every exact requirement with deny precedence and fail-closed defaults.
    pub fn authorize(
        &self,
        method: &'static str,
        requirements: &ApiPermissionRequirements,
    ) -> Result<()> {
        requirements.validate()?;
        for requirement in requirements.as_slice() {
            let mut allowed = false;
            for rule in &self.rules {
                if !rule.matches(requirement)? {
                    continue;
                }
                match rule.effect() {
                    ApiPermissionEffect::Deny => {
                        return Err(permission_denied(self, method, requirement));
                    }
                    ApiPermissionEffect::Allow => allowed = true,
                }
            }
            if !allowed {
                return Err(permission_denied(self, method, requirement));
            }
        }
        Ok(())
    }
}

impl Default for ApiPermissionContext {
    fn default() -> Self {
        Self {
            principal: ComponentId::new("superi.host.untrusted")
                .expect("built-in permission principal is canonical"),
            rules: Vec::new(),
        }
    }
}

impl fmt::Debug for ApiPermissionContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiPermissionContext")
            .field("principal", &self.principal)
            .field("rule_count", &self.rules.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NormalizedPath {
    root: NormalizedRoot,
    components: Vec<String>,
    canonical: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NormalizedRoot {
    Project,
    Unix,
    WindowsDrive(String),
    WindowsUnc { server: String, share: String },
}

fn normalize_project_relative(path: &str) -> Result<String> {
    Ok(normalize_relative_components(path)?.join("/"))
}

fn normalize_relative_components(path: &str) -> Result<Vec<String>> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.chars().any(char::is_control)
    {
        return Err(invalid(
            "normalize_filesystem_path",
            "project-relative permission path has invalid syntax",
        ));
    }
    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(invalid(
                        "normalize_filesystem_path",
                        "project-relative permission path traverses above its root",
                    ));
                }
            }
            value => components.push(value.to_owned()),
        }
    }
    if components.is_empty() {
        return Err(invalid(
            "normalize_filesystem_path",
            "permission path must identify a filesystem target",
        ));
    }
    Ok(components)
}

fn normalize_absolute(platform: ApiFilesystemPlatform, path: &str) -> Result<NormalizedPath> {
    if path.is_empty() || path.chars().any(char::is_control) {
        return Err(invalid(
            "normalize_filesystem_path",
            "absolute permission path is empty or contains control characters",
        ));
    }
    match platform {
        ApiFilesystemPlatform::Unix => normalize_unix_absolute(path),
        ApiFilesystemPlatform::Windows => normalize_windows_absolute(path),
    }
}

fn normalize_unix_absolute(path: &str) -> Result<NormalizedPath> {
    if !path.starts_with('/') {
        return Err(invalid(
            "normalize_filesystem_path",
            "absolute permission path does not match Unix syntax",
        ));
    }
    let components = normalize_absolute_components(path.split('/').skip(1), false)?;
    let canonical = if components.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", components.join("/"))
    };
    Ok(NormalizedPath {
        root: NormalizedRoot::Unix,
        components,
        canonical,
    })
}

fn normalize_windows_absolute(path: &str) -> Result<NormalizedPath> {
    let replaced = path.replace('\\', "/");
    if let Some(rest) = replaced.strip_prefix("//") {
        let raw = rest.split('/').collect::<Vec<_>>();
        if raw.len() < 2 || raw[0].is_empty() || raw[1].is_empty() {
            return Err(invalid(
                "normalize_filesystem_path",
                "Windows UNC permission path lacks a server or share",
            ));
        }
        let server = raw[0].to_ascii_lowercase();
        let share = raw[1].to_ascii_lowercase();
        let components = normalize_absolute_components(raw.into_iter().skip(2), true)?;
        let suffix = if components.is_empty() {
            String::new()
        } else {
            format!("/{}", components.join("/"))
        };
        return Ok(NormalizedPath {
            root: NormalizedRoot::WindowsUnc {
                server: server.clone(),
                share: share.clone(),
            },
            components,
            canonical: format!("//{server}/{share}{suffix}"),
        });
    }
    let bytes = replaced.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'/' {
        return Err(invalid(
            "normalize_filesystem_path",
            "absolute permission path does not match Windows syntax",
        ));
    }
    let drive = (bytes[0] as char).to_ascii_uppercase().to_string();
    let components = normalize_absolute_components(replaced[3..].split('/'), true)?;
    let suffix = if components.is_empty() {
        String::new()
    } else {
        components.join("/")
    };
    Ok(NormalizedPath {
        root: NormalizedRoot::WindowsDrive(drive.clone()),
        components,
        canonical: format!("{drive}:/{suffix}"),
    })
}

fn normalize_absolute_components<'a>(
    values: impl IntoIterator<Item = &'a str>,
    ascii_case_fold: bool,
) -> Result<Vec<String>> {
    let mut components = Vec::new();
    for component in values {
        match component {
            "" | "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(invalid(
                        "normalize_filesystem_path",
                        "absolute permission path traverses above its root",
                    ));
                }
            }
            value => components.push(if ascii_case_fold {
                value.to_ascii_lowercase()
            } else {
                value.to_owned()
            }),
        }
    }
    Ok(components)
}

fn is_absolute_for(platform: ApiFilesystemPlatform, path: &str) -> bool {
    match platform {
        ApiFilesystemPlatform::Unix => path.starts_with('/'),
        ApiFilesystemPlatform::Windows => {
            let bytes = path.as_bytes();
            path.starts_with("\\\\")
                || path.starts_with("//")
                || (bytes.len() >= 3
                    && bytes[0].is_ascii_alphabetic()
                    && bytes[1] == b':'
                    && matches!(bytes[2], b'/' | b'\\'))
        }
    }
}

fn canonical_capabilities<I, S>(capabilities: I) -> Result<Vec<String>>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let values = capabilities
        .into_iter()
        .map(Into::into)
        .collect::<BTreeSet<_>>();
    validate_capabilities(values.iter().map(String::as_str))?;
    Ok(values.into_iter().collect())
}

fn validate_capabilities<'a>(capabilities: impl IntoIterator<Item = &'a str>) -> Result<()> {
    for capability in capabilities {
        CapabilityId::new(capability).map_err(|_| {
            invalid(
                "validate_plugin_capabilities",
                "plugin permission contains a noncanonical capability identity",
            )
        })?;
    }
    Ok(())
}

fn validate_canonical_capabilities(capabilities: &[String]) -> Result<()> {
    validate_capabilities(capabilities.iter().map(String::as_str))?;
    let canonical = capabilities.iter().cloned().collect::<BTreeSet<_>>();
    if canonical.len() != capabilities.len()
        || !canonical
            .iter()
            .zip(capabilities)
            .all(|(left, right)| left == right)
    {
        return Err(invalid(
            "validate_plugin_capabilities",
            "plugin permission capabilities are not canonical",
        ));
    }
    Ok(())
}

fn contains_all(ceiling: &[String], required: &[String]) -> bool {
    required
        .iter()
        .all(|capability| ceiling.binary_search(capability).is_ok())
}

fn validate_component(operation: &'static str, value: &str) -> Result<()> {
    ComponentId::new(value)
        .map(|_| ())
        .map_err(|_| invalid(operation, "plugin permission identity is not canonical"))
}

fn permission_denied(
    context: &ApiPermissionContext,
    method: &'static str,
    requirement: &ApiPermissionRequirement,
) -> Error {
    Error::new(
        ErrorCategory::PermissionDenied,
        Recoverability::UserCorrectable,
        "API permission was not granted for this operation",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "authorize")
            .with_field("principal", context.principal.as_str())
            .with_field("method", method)
            .with_field("permission_kind", requirement.kind().code())
            .with_field("operation", requirement.operation_code()),
    )
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
