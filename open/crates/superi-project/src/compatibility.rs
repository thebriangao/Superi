//! Authoritative project format identity and compatibility negotiation.

use std::cmp::Ordering;

use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::SemanticVersion;

/// SQLite application identity for every Superi project database.
pub const PROJECT_APPLICATION_ID: u32 = 0x5355_5052;
/// Oldest project schema with a registered reader and forward migration.
pub const PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION: u32 = 0;
/// Current durable project schema revision.
pub const PROJECT_SCHEMA_REVISION: u32 = 5;
/// Stable text identity for the durable project format.
pub const PROJECT_FORMAT: &str = "superi.project";
/// Current semantic project format version.
pub const PROJECT_FORMAT_VERSION: &str = "1.4.0";

pub(crate) const PROJECT_FORMAT_VERSION_SCHEMA_ZERO: &str = "0.9.0";
pub(crate) const PROJECT_FORMAT_VERSION_SCHEMA_ONE: &str = "1.0.0";
pub(crate) const PROJECT_FORMAT_VERSION_SCHEMA_TWO: &str = "1.1.0";
pub(crate) const PROJECT_FORMAT_VERSION_SCHEMA_THREE: &str = "1.2.0";
pub(crate) const PROJECT_FORMAT_VERSION_SCHEMA_FOUR: &str = "1.3.0";

/// One released schema and semantic project format pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectFormatRelease {
    schema_revision: u32,
    format_version: &'static str,
}

impl ProjectFormatRelease {
    const fn new(schema_revision: u32, format_version: &'static str) -> Self {
        Self {
            schema_revision,
            format_version,
        }
    }

    /// Returns the durable database schema revision.
    #[must_use]
    pub const fn schema_revision(self) -> u32 {
        self.schema_revision
    }

    /// Returns the exact semantic format version stored by this release.
    #[must_use]
    pub const fn format_version(self) -> &'static str {
        self.format_version
    }
}

const PROJECT_FORMAT_RELEASES: [ProjectFormatRelease; 6] = [
    ProjectFormatRelease::new(0, PROJECT_FORMAT_VERSION_SCHEMA_ZERO),
    ProjectFormatRelease::new(1, PROJECT_FORMAT_VERSION_SCHEMA_ONE),
    ProjectFormatRelease::new(2, PROJECT_FORMAT_VERSION_SCHEMA_TWO),
    ProjectFormatRelease::new(3, PROJECT_FORMAT_VERSION_SCHEMA_THREE),
    ProjectFormatRelease::new(4, PROJECT_FORMAT_VERSION_SCHEMA_FOUR),
    ProjectFormatRelease::new(5, PROJECT_FORMAT_VERSION),
];

/// Complete compatibility support published by this application build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectFormatSupport {
    application_id: u32,
    format: &'static str,
    primitive_schema_revision: u32,
    releases: &'static [ProjectFormatRelease],
}

impl ProjectFormatSupport {
    /// Returns the required SQLite application identity.
    #[must_use]
    pub const fn application_id(self) -> u32 {
        self.application_id
    }

    /// Returns the required text format identity.
    #[must_use]
    pub const fn format(self) -> &'static str {
        self.format
    }

    /// Returns the supported stable primitive wire revision.
    #[must_use]
    pub const fn primitive_schema_revision(self) -> u32 {
        self.primitive_schema_revision
    }

    /// Returns every released project format in ascending schema order.
    #[must_use]
    pub const fn releases(self) -> &'static [ProjectFormatRelease] {
        self.releases
    }

    /// Returns the current project format release.
    #[must_use]
    pub fn current(self) -> &'static ProjectFormatRelease {
        self.releases
            .last()
            .expect("project format support always contains a current release")
    }
}

/// Returns the single authoritative project format compatibility table.
#[must_use]
pub const fn project_format_support() -> ProjectFormatSupport {
    ProjectFormatSupport {
        application_id: PROJECT_APPLICATION_ID,
        format: PROJECT_FORMAT,
        primitive_schema_revision: STABLE_PRIMITIVE_SCHEMA_REVISION,
        releases: &PROJECT_FORMAT_RELEASES,
    }
}

/// Complete observed identity of one durable project format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectFormatIdentity {
    application_id: u32,
    format: String,
    format_version: SemanticVersion,
    primitive_schema_revision: u32,
    schema_revision: u32,
}

impl ProjectFormatIdentity {
    /// Creates one complete observed format identity.
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

    /// Returns the observed database schema revision.
    #[must_use]
    pub const fn schema_revision(&self) -> u32 {
        self.schema_revision
    }
}

/// Coarse compatibility outcome for one observed project identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectVersionDisposition {
    /// The project is already at the current supported release.
    Current,
    /// A registered forward migration reaches the current release.
    MigrationRequired,
    /// The project declares a version newer than this application supports.
    RequiresNewerApplication,
    /// The project belongs to a different application or durable format.
    Unsupported,
    /// The project combines fields that do not form a registered release.
    Invalid,
}

/// Exact reason contributing to one compatibility outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectVersionReason {
    /// A complete registered migration path is available.
    RegisteredMigration,
    /// The SQLite application identity is not Superi's project identity.
    ForeignApplicationIdentity,
    /// The text format identity is not Superi's project format.
    ForeignFormatIdentity,
    /// The database schema is newer than this application supports.
    FutureSchemaRevision,
    /// The semantic project format is newer than this application supports.
    FutureSemanticFormat,
    /// The primitive wire revision is newer than this application supports.
    FuturePrimitiveRevision,
    /// The database schema has no released project format registration.
    UnregisteredSchemaRevision,
    /// The semantic format version does not match its registered schema.
    InconsistentSchemaFormat,
    /// The primitive wire revision is not valid for a registered release.
    InconsistentPrimitiveRevision,
}

/// Deterministic compatibility result for one observed project identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectVersionNegotiation {
    observed: ProjectFormatIdentity,
    target: ProjectFormatRelease,
    disposition: ProjectVersionDisposition,
    reasons: Vec<ProjectVersionReason>,
    migration_path: Vec<ProjectFormatRelease>,
}

impl ProjectVersionNegotiation {
    /// Returns the complete observed identity.
    #[must_use]
    pub const fn observed(&self) -> &ProjectFormatIdentity {
        &self.observed
    }

    /// Returns the current format targeted by this application.
    #[must_use]
    pub const fn target(&self) -> ProjectFormatRelease {
        self.target
    }

    /// Returns the coarse compatibility outcome.
    #[must_use]
    pub const fn disposition(&self) -> ProjectVersionDisposition {
        self.disposition
    }

    /// Returns exact reasons in deterministic field order.
    #[must_use]
    pub fn reasons(&self) -> &[ProjectVersionReason] {
        &self.reasons
    }

    /// Returns each successor release needed to reach the current format.
    #[must_use]
    pub fn migration_path(&self) -> &[ProjectFormatRelease] {
        &self.migration_path
    }
}

/// Negotiates one observed project identity without opening or mutating a project.
#[must_use]
pub fn negotiate_project_format(observed: ProjectFormatIdentity) -> ProjectVersionNegotiation {
    let support = project_format_support();
    let target = *support.current();
    let mut reasons = Vec::new();

    if observed.application_id != support.application_id {
        reasons.push(ProjectVersionReason::ForeignApplicationIdentity);
    }
    if observed.format != support.format {
        reasons.push(ProjectVersionReason::ForeignFormatIdentity);
    }
    if !reasons.is_empty() {
        return ProjectVersionNegotiation {
            observed,
            target,
            disposition: ProjectVersionDisposition::Unsupported,
            reasons,
            migration_path: Vec::new(),
        };
    }

    if observed.schema_revision > target.schema_revision {
        reasons.push(ProjectVersionReason::FutureSchemaRevision);
    }
    let target_version = parse_registered_version(target);
    if observed.format_version.precedence_cmp(&target_version) == Ordering::Greater {
        reasons.push(ProjectVersionReason::FutureSemanticFormat);
    }
    if observed.primitive_schema_revision > support.primitive_schema_revision {
        reasons.push(ProjectVersionReason::FuturePrimitiveRevision);
    }
    if !reasons.is_empty() {
        return ProjectVersionNegotiation {
            observed,
            target,
            disposition: ProjectVersionDisposition::RequiresNewerApplication,
            reasons,
            migration_path: Vec::new(),
        };
    }

    let Some(position) = support
        .releases
        .iter()
        .position(|release| release.schema_revision == observed.schema_revision)
    else {
        return ProjectVersionNegotiation {
            observed,
            target,
            disposition: ProjectVersionDisposition::Invalid,
            reasons: vec![ProjectVersionReason::UnregisteredSchemaRevision],
            migration_path: Vec::new(),
        };
    };
    let registered = support.releases[position];
    if observed.format_version != parse_registered_version(registered) {
        reasons.push(ProjectVersionReason::InconsistentSchemaFormat);
    }
    if observed.primitive_schema_revision != support.primitive_schema_revision {
        reasons.push(ProjectVersionReason::InconsistentPrimitiveRevision);
    }
    if !reasons.is_empty() {
        return ProjectVersionNegotiation {
            observed,
            target,
            disposition: ProjectVersionDisposition::Invalid,
            reasons,
            migration_path: Vec::new(),
        };
    }

    let migration_path = support.releases[(position + 1)..].to_vec();
    let disposition = if migration_path.is_empty() {
        ProjectVersionDisposition::Current
    } else {
        reasons.push(ProjectVersionReason::RegisteredMigration);
        ProjectVersionDisposition::MigrationRequired
    };
    ProjectVersionNegotiation {
        observed,
        target,
        disposition,
        reasons,
        migration_path,
    }
}

pub(crate) fn project_format_release(
    schema_revision: u32,
) -> Option<&'static ProjectFormatRelease> {
    PROJECT_FORMAT_RELEASES
        .iter()
        .find(|release| release.schema_revision == schema_revision)
}

fn parse_registered_version(release: ProjectFormatRelease) -> SemanticVersion {
    release
        .format_version
        .parse()
        .expect("built-in project format version is valid SemVer")
}
