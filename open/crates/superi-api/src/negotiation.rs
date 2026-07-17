//! Stateless public API and durable project version negotiation.

use std::cmp::Ordering;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::SemanticVersion;
use superi_engine::editor::{
    negotiate_project_format, project_format_support, ProjectFormatIdentity, ProjectFormatRelease,
    ProjectVersionDisposition, ProjectVersionNegotiation, ProjectVersionReason,
};

use crate::commands::ApiCommand;
use crate::permissions::{
    ApiPermissionKind, ApiPermissionRequirementMode, ApiPermissionRequirements,
};
use crate::schema::PublicMethodKind;
use crate::version::{
    NEGOTIATE_API_VERSION_METHOD, PUBLIC_API_SCHEMA_RELEASES, VERSION_NEGOTIATION_SCHEMA_VERSION,
};

const COMPONENT: &str = "superi-api.version_negotiation";
const MAX_VERSION_OFFERS: usize = 64;

/// Complete observed durable project format supplied for compatibility evaluation.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFormatDescriptor {
    application_id: u32,
    format: String,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::SemanticVersionBinding)
    )]
    format_version: SemanticVersion,
    primitive_schema_revision: u32,
    schema_revision: u32,
}

impl ProjectFormatDescriptor {
    /// Creates one complete observed project format descriptor.
    #[must_use]
    pub fn new(
        application_id: u32,
        format: impl Into<String>,
        format_version: SemanticVersion,
        primitive_schema_revision: u32,
        schema_revision: u32,
    ) -> Self {
        Self {
            application_id,
            format: format.into(),
            format_version,
            primitive_schema_revision,
            schema_revision,
        }
    }

    /// Returns the observed SQLite application identity.
    #[must_use]
    pub const fn application_id(&self) -> u32 {
        self.application_id
    }

    /// Returns the observed text format identity.
    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
    }

    /// Returns the observed semantic format version.
    #[must_use]
    pub const fn format_version(&self) -> &SemanticVersion {
        &self.format_version
    }

    /// Returns the observed stable primitive revision.
    #[must_use]
    pub const fn primitive_schema_revision(&self) -> u32 {
        self.primitive_schema_revision
    }

    /// Returns the observed project schema revision.
    #[must_use]
    pub const fn schema_revision(&self) -> u32 {
        self.schema_revision
    }

    fn into_engine(self) -> ProjectFormatIdentity {
        ProjectFormatIdentity::new(
            self.application_id,
            self.format,
            self.format_version,
            self.primitive_schema_revision,
            self.schema_revision,
        )
    }
}

/// Strict client offers for public API and optional project format negotiation.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NegotiateApiVersion {
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = Vec<crate::typescript::SemanticVersionBinding>)
    )]
    api_schema_versions: Vec<SemanticVersion>,
    primitive_schema_revisions: Vec<u32>,
    project: Option<ProjectFormatDescriptor>,
}

impl NegotiateApiVersion {
    /// Creates strict nonempty ascending client offers.
    pub fn try_new(
        api_schema_versions: Vec<SemanticVersion>,
        primitive_schema_revisions: Vec<u32>,
        project: Option<ProjectFormatDescriptor>,
    ) -> Result<Self> {
        validate_api_versions(&api_schema_versions)?;
        validate_primitive_revisions(&primitive_schema_revisions)?;
        Ok(Self {
            api_schema_versions,
            primitive_schema_revisions,
            project,
        })
    }

    /// Returns client API schema offers in strictly ascending precedence order.
    #[must_use]
    pub fn api_schema_versions(&self) -> &[SemanticVersion] {
        &self.api_schema_versions
    }

    /// Returns client stable primitive offers in strictly ascending order.
    #[must_use]
    pub fn primitive_schema_revisions(&self) -> &[u32] {
        &self.primitive_schema_revisions
    }

    /// Returns an optional project format to evaluate independently.
    #[must_use]
    pub const fn project(&self) -> Option<&ProjectFormatDescriptor> {
        self.project.as_ref()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NegotiateApiVersionWire {
    api_schema_versions: Vec<SemanticVersion>,
    primitive_schema_revisions: Vec<u32>,
    project: Option<ProjectFormatDescriptor>,
}

impl<'de> Deserialize<'de> for NegotiateApiVersion {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = NegotiateApiVersionWire::deserialize(deserializer)?;
        Self::try_new(
            wire.api_schema_versions,
            wire.primitive_schema_revisions,
            wire.project,
        )
        .map_err(D::Error::custom)
    }
}

impl ApiCommand for NegotiateApiVersion {
    type Response = NegotiateApiVersionResult;

    const METHOD: &'static str = NEGOTIATE_API_VERSION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = VERSION_NEGOTIATION_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// One project schema and semantic format pair on the public wire.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFormatReleaseDescriptor {
    schema_revision: u32,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::SemanticVersionBinding)
    )]
    format_version: SemanticVersion,
}

impl ProjectFormatReleaseDescriptor {
    /// Returns the durable project schema revision.
    #[must_use]
    pub const fn schema_revision(&self) -> u32 {
        self.schema_revision
    }

    /// Returns the exact semantic project format version.
    #[must_use]
    pub const fn format_version(&self) -> &SemanticVersion {
        &self.format_version
    }

    fn from_engine(release: ProjectFormatRelease) -> Self {
        Self {
            schema_revision: release.schema_revision(),
            format_version: release
                .format_version()
                .parse()
                .expect("engine project format registry contains valid SemVer"),
        }
    }
}

/// Public projection of the server's project format support.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFormatSupportDescriptor {
    application_id: u32,
    format: String,
    primitive_schema_revision: u32,
    releases: Vec<ProjectFormatReleaseDescriptor>,
}

impl ProjectFormatSupportDescriptor {
    /// Returns the required SQLite application identity.
    #[must_use]
    pub const fn application_id(&self) -> u32 {
        self.application_id
    }

    /// Returns the required text format identity.
    #[must_use]
    pub fn format(&self) -> &str {
        &self.format
    }

    /// Returns the supported stable primitive revision.
    #[must_use]
    pub const fn primitive_schema_revision(&self) -> u32 {
        self.primitive_schema_revision
    }

    /// Returns all released project formats in ascending schema order.
    #[must_use]
    pub fn releases(&self) -> &[ProjectFormatReleaseDescriptor] {
        &self.releases
    }

    fn current() -> Self {
        let support = project_format_support();
        Self {
            application_id: support.application_id(),
            format: support.format().to_owned(),
            primitive_schema_revision: support.primitive_schema_revision(),
            releases: support
                .releases()
                .iter()
                .copied()
                .map(ProjectFormatReleaseDescriptor::from_engine)
                .collect(),
        }
    }
}

/// Server releases available for public API negotiation.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiVersionSupport {
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = Vec<crate::typescript::SemanticVersionBinding>)
    )]
    api_schema_versions: Vec<SemanticVersion>,
    primitive_schema_revisions: Vec<u32>,
    project_format: ProjectFormatSupportDescriptor,
}

impl ApiVersionSupport {
    /// Returns every API catalog release in ascending SemVer precedence order.
    #[must_use]
    pub fn api_schema_versions(&self) -> &[SemanticVersion] {
        &self.api_schema_versions
    }

    /// Returns every supported stable primitive revision in ascending order.
    #[must_use]
    pub fn primitive_schema_revisions(&self) -> &[u32] {
        &self.primitive_schema_revisions
    }

    /// Returns complete durable project format support.
    #[must_use]
    pub const fn project_format(&self) -> &ProjectFormatSupportDescriptor {
        &self.project_format
    }

    fn current() -> Self {
        Self {
            api_schema_versions: PUBLIC_API_SCHEMA_RELEASES.to_vec(),
            primitive_schema_revisions: vec![
                superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION,
            ],
            project_format: ProjectFormatSupportDescriptor::current(),
        }
    }
}

/// Canonical common API schema and primitive revision selected for a client.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiVersionSelection {
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::SemanticVersionBinding)
    )]
    api_schema_version: SemanticVersion,
    primitive_schema_revision: u32,
}

impl ApiVersionSelection {
    /// Returns the canonical selected server API release.
    #[must_use]
    pub const fn api_schema_version(&self) -> &SemanticVersion {
        &self.api_schema_version
    }

    /// Returns the selected stable primitive revision.
    #[must_use]
    pub const fn primitive_schema_revision(&self) -> u32 {
        self.primitive_schema_revision
    }
}

/// One compatibility dimension with no common client and server release.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApiVersionIncompatibility {
    /// No API catalog version has equal SemVer precedence.
    NoCommonApiSchemaVersion,
    /// No exact stable primitive revision is shared.
    NoCommonPrimitiveSchemaRevision,
}

/// Public project compatibility outcome.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectCompatibilityDisposition {
    /// The project is at the current supported release.
    Current,
    /// A complete registered forward migration is available.
    MigrationRequired,
    /// A newer application is required.
    RequiresNewerApplication,
    /// The project belongs to another application or format.
    Unsupported,
    /// The fields do not form a released project format.
    Invalid,
}

/// Public typed reason for one project compatibility outcome.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectCompatibilityReason {
    /// A complete registered migration path is available.
    RegisteredMigration,
    /// The SQLite application identity is foreign.
    ForeignApplicationIdentity,
    /// The text format identity is foreign.
    ForeignFormatIdentity,
    /// The schema revision is newer than supported.
    FutureSchemaRevision,
    /// The semantic format is newer than supported.
    FutureSemanticFormat,
    /// The primitive wire revision is newer than supported.
    FuturePrimitiveRevision,
    /// The schema revision has no released registration.
    UnregisteredSchemaRevision,
    /// The semantic version does not match its schema revision.
    InconsistentSchemaFormat,
    /// The primitive revision is inconsistent with a released format.
    InconsistentPrimitiveRevision,
    /// A future engine reason is not yet known to this API schema.
    Unknown,
}

/// Complete public compatibility projection for one project descriptor.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCompatibilityResult {
    disposition: ProjectCompatibilityDisposition,
    reasons: Vec<ProjectCompatibilityReason>,
    target: ProjectFormatReleaseDescriptor,
    migration_path: Vec<ProjectFormatReleaseDescriptor>,
}

impl ProjectCompatibilityResult {
    /// Returns the coarse project compatibility outcome.
    #[must_use]
    pub const fn disposition(&self) -> ProjectCompatibilityDisposition {
        self.disposition
    }

    /// Returns exact reasons in deterministic field order.
    #[must_use]
    pub fn reasons(&self) -> &[ProjectCompatibilityReason] {
        &self.reasons
    }

    /// Returns the current project release targeted by this application.
    #[must_use]
    pub const fn target(&self) -> &ProjectFormatReleaseDescriptor {
        &self.target
    }

    /// Returns every successor release in the registered migration path.
    #[must_use]
    pub fn migration_path(&self) -> &[ProjectFormatReleaseDescriptor] {
        &self.migration_path
    }

    fn from_engine(value: &ProjectVersionNegotiation) -> Self {
        Self {
            disposition: public_project_disposition(value.disposition()),
            reasons: value
                .reasons()
                .iter()
                .copied()
                .map(public_project_reason)
                .collect(),
            target: ProjectFormatReleaseDescriptor::from_engine(value.target()),
            migration_path: value
                .migration_path()
                .iter()
                .copied()
                .map(ProjectFormatReleaseDescriptor::from_engine)
                .collect(),
        }
    }
}

/// Complete deterministic API and optional project negotiation response.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NegotiateApiVersionResult {
    support: ApiVersionSupport,
    selection: Option<ApiVersionSelection>,
    incompatibilities: Vec<ApiVersionIncompatibility>,
    project: Option<ProjectCompatibilityResult>,
}

impl NegotiateApiVersionResult {
    /// Returns all releases supported by the server.
    #[must_use]
    pub const fn support(&self) -> &ApiVersionSupport {
        &self.support
    }

    /// Returns the common selection only when both dimensions are compatible.
    #[must_use]
    pub const fn selection(&self) -> Option<&ApiVersionSelection> {
        self.selection.as_ref()
    }

    /// Returns every missing compatibility dimension in stable order.
    #[must_use]
    pub fn incompatibilities(&self) -> &[ApiVersionIncompatibility] {
        &self.incompatibilities
    }

    /// Returns an independently evaluated project result when requested.
    #[must_use]
    pub const fn project(&self) -> Option<&ProjectCompatibilityResult> {
        self.project.as_ref()
    }
}

/// Stateless owner for public API and project version negotiation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VersionNegotiationApi;

impl VersionNegotiationApi {
    /// Creates the stateless negotiation owner.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Selects canonical common releases without mutating runtime or project state.
    #[must_use]
    pub fn execute(self, request: NegotiateApiVersion) -> NegotiateApiVersionResult {
        let support = ApiVersionSupport::current();
        let api_schema_version = support
            .api_schema_versions
            .iter()
            .rev()
            .find(|server| {
                request
                    .api_schema_versions
                    .iter()
                    .any(|client| client.precedence_cmp(server) == Ordering::Equal)
            })
            .cloned();
        let primitive_schema_revision = support
            .primitive_schema_revisions
            .iter()
            .rev()
            .find(|server| {
                request
                    .primitive_schema_revisions
                    .binary_search(server)
                    .is_ok()
            })
            .copied();

        let mut incompatibilities = Vec::new();
        if api_schema_version.is_none() {
            incompatibilities.push(ApiVersionIncompatibility::NoCommonApiSchemaVersion);
        }
        if primitive_schema_revision.is_none() {
            incompatibilities.push(ApiVersionIncompatibility::NoCommonPrimitiveSchemaRevision);
        }
        let selection = api_schema_version.zip(primitive_schema_revision).map(
            |(api_schema_version, primitive_schema_revision)| ApiVersionSelection {
                api_schema_version,
                primitive_schema_revision,
            },
        );
        let project = request.project.map(|project| {
            let result = negotiate_project_format(project.into_engine());
            ProjectCompatibilityResult::from_engine(&result)
        });

        NegotiateApiVersionResult {
            support,
            selection,
            incompatibilities,
            project,
        }
    }
}

fn validate_api_versions(versions: &[SemanticVersion]) -> Result<()> {
    if versions.is_empty() || versions.len() > MAX_VERSION_OFFERS {
        return Err(invalid(
            "validate_api_version_offers",
            "API schema offers must contain between 1 and 64 versions",
        ));
    }
    if versions
        .windows(2)
        .any(|pair| pair[0].precedence_cmp(&pair[1]) != Ordering::Less)
    {
        return Err(invalid(
            "validate_api_version_offers",
            "API schema offers must have unique strictly ascending SemVer precedence",
        ));
    }
    Ok(())
}

fn validate_primitive_revisions(revisions: &[u32]) -> Result<()> {
    if revisions.is_empty() || revisions.len() > MAX_VERSION_OFFERS {
        return Err(invalid(
            "validate_primitive_version_offers",
            "primitive schema offers must contain between 1 and 64 revisions",
        ));
    }
    if revisions[0] == 0 || revisions.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid(
            "validate_primitive_version_offers",
            "primitive schema offers must be nonzero, unique, and strictly ascending",
        ));
    }
    Ok(())
}

const fn public_project_disposition(
    value: ProjectVersionDisposition,
) -> ProjectCompatibilityDisposition {
    match value {
        ProjectVersionDisposition::Current => ProjectCompatibilityDisposition::Current,
        ProjectVersionDisposition::MigrationRequired => {
            ProjectCompatibilityDisposition::MigrationRequired
        }
        ProjectVersionDisposition::RequiresNewerApplication => {
            ProjectCompatibilityDisposition::RequiresNewerApplication
        }
        ProjectVersionDisposition::Unsupported => ProjectCompatibilityDisposition::Unsupported,
        ProjectVersionDisposition::Invalid => ProjectCompatibilityDisposition::Invalid,
        _ => ProjectCompatibilityDisposition::Invalid,
    }
}

const fn public_project_reason(value: ProjectVersionReason) -> ProjectCompatibilityReason {
    match value {
        ProjectVersionReason::RegisteredMigration => {
            ProjectCompatibilityReason::RegisteredMigration
        }
        ProjectVersionReason::ForeignApplicationIdentity => {
            ProjectCompatibilityReason::ForeignApplicationIdentity
        }
        ProjectVersionReason::ForeignFormatIdentity => {
            ProjectCompatibilityReason::ForeignFormatIdentity
        }
        ProjectVersionReason::FutureSchemaRevision => {
            ProjectCompatibilityReason::FutureSchemaRevision
        }
        ProjectVersionReason::FutureSemanticFormat => {
            ProjectCompatibilityReason::FutureSemanticFormat
        }
        ProjectVersionReason::FuturePrimitiveRevision => {
            ProjectCompatibilityReason::FuturePrimitiveRevision
        }
        ProjectVersionReason::UnregisteredSchemaRevision => {
            ProjectCompatibilityReason::UnregisteredSchemaRevision
        }
        ProjectVersionReason::InconsistentSchemaFormat => {
            ProjectCompatibilityReason::InconsistentSchemaFormat
        }
        ProjectVersionReason::InconsistentPrimitiveRevision => {
            ProjectCompatibilityReason::InconsistentPrimitiveRevision
        }
        _ => ProjectCompatibilityReason::Unknown,
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
