//! Canonical settings, capabilities, feature discovery, and version identifiers.
//!
//! These types are platform-neutral values shared by projects, engines,
//! extensions, and automation. Policy, persistence, environment discovery, and
//! subsystem implementation remain with their owning crates.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use crate::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

/// A failure while parsing a canonical namespaced shared identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParseSharedNameError {
    /// The identifier is empty.
    Empty,
    /// The identifier has fewer than two dot-separated segments.
    MissingNamespace,
    /// A segment is empty.
    EmptySegment {
        /// The zero-based byte index where the empty segment begins.
        index: usize,
    },
    /// A segment does not begin with a lowercase ASCII letter.
    InvalidSegmentStart {
        /// The zero-based byte index of the invalid first byte.
        index: usize,
    },
    /// A non-canonical byte appears within a segment.
    InvalidCharacter {
        /// The zero-based byte index of the invalid byte.
        index: usize,
    },
}

impl fmt::Display for ParseSharedNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("shared identifier is empty"),
            Self::MissingNamespace => {
                formatter.write_str("shared identifier must contain at least two segments")
            }
            Self::EmptySegment { index } => {
                write!(
                    formatter,
                    "shared identifier has an empty segment at byte {index}"
                )
            }
            Self::InvalidSegmentStart { index } => write!(
                formatter,
                "shared identifier segment must begin with a lowercase ASCII letter at byte {index}"
            ),
            Self::InvalidCharacter { index } => write!(
                formatter,
                "shared identifier contains a non-canonical character at byte {index}"
            ),
        }
    }
}

impl std::error::Error for ParseSharedNameError {}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct CanonicalName(String);

impl CanonicalName {
    fn parse(input: &str) -> std::result::Result<Self, ParseSharedNameError> {
        if input.is_empty() {
            return Err(ParseSharedNameError::Empty);
        }

        let bytes = input.as_bytes();
        let mut segment_count = 0_usize;
        let mut segment_start = 0_usize;
        for (index, byte) in bytes.iter().copied().enumerate() {
            if byte == b'.' {
                if index == segment_start {
                    return Err(ParseSharedNameError::EmptySegment { index });
                }
                segment_count += 1;
                segment_start = index + 1;
                continue;
            }

            if index == segment_start {
                if !byte.is_ascii_lowercase() {
                    return Err(ParseSharedNameError::InvalidSegmentStart { index });
                }
            } else if !(byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'_' | b'-'))
            {
                return Err(ParseSharedNameError::InvalidCharacter { index });
            }
        }

        if segment_start == bytes.len() {
            return Err(ParseSharedNameError::EmptySegment { index: bytes.len() });
        }
        segment_count += 1;
        if segment_count < 2 {
            return Err(ParseSharedNameError::MissingNamespace);
        }

        Ok(Self(input.to_owned()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

macro_rules! define_shared_name {
    ($name:ident, $summary:literal) => {
        #[doc = $summary]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(CanonicalName);

        impl $name {
            /// Parses a strict canonical namespaced identifier.
            pub fn new(input: impl AsRef<str>) -> std::result::Result<Self, ParseSharedNameError> {
                CanonicalName::parse(input.as_ref()).map(Self)
            }

            /// Returns the canonical identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }

            /// Consumes the identifier and returns its owned canonical text.
            #[must_use]
            pub fn into_string(self) -> String {
                let Self(CanonicalName(value)) = self;
                value
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = ParseSharedNameError;

            fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
                Self::new(input)
            }
        }
    };
}

define_shared_name!(
    SettingKey,
    "A stable namespaced setting key shared by project and process boundaries."
);
define_shared_name!(
    CapabilityId,
    "A stable symbolic capability identifier that never relies on bit positions."
);
define_shared_name!(
    FeatureId,
    "A stable namespaced identifier for a discoverable feature."
);
define_shared_name!(
    ComponentId,
    "A stable namespaced identifier for a versioned component or protocol."
);

/// The section of a semantic version that failed validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum VersionSection {
    /// The major integer.
    Major,
    /// The minor integer.
    Minor,
    /// The patch integer.
    Patch,
    /// Dot-separated pre-release identifiers.
    PreRelease,
    /// Dot-separated build metadata identifiers.
    BuildMetadata,
}

impl fmt::Display for VersionSection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Patch => "patch",
            Self::PreRelease => "pre-release",
            Self::BuildMetadata => "build metadata",
        })
    }
}

/// A strict Semantic Versioning 2.0.0 parse failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParseSemanticVersionError {
    /// Fewer than three core numeric components were supplied.
    MissingCoreComponent,
    /// More than three core numeric components were supplied.
    TooManyCoreComponents,
    /// A core or numeric pre-release identifier has a forbidden leading zero.
    LeadingZero {
        /// The affected version section.
        section: VersionSection,
    },
    /// A required numeric core component contains a non-digit or is empty.
    InvalidNumber {
        /// The affected version section.
        section: VersionSection,
    },
    /// A numeric core component exceeds the fixed-width public representation.
    NumericOverflow {
        /// The affected version section.
        section: VersionSection,
    },
    /// A pre-release or build identifier is empty.
    EmptyIdentifier {
        /// The affected version section.
        section: VersionSection,
        /// The zero-based identifier position within the section.
        identifier: usize,
    },
    /// An identifier contains a byte outside ASCII letters, digits, and hyphen.
    InvalidIdentifier {
        /// The affected version section.
        section: VersionSection,
        /// The zero-based byte index within the whole section.
        index: usize,
    },
    /// More than one build metadata separator was supplied.
    MultipleBuildSeparators,
}

impl fmt::Display for ParseSemanticVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCoreComponent => {
                formatter.write_str("semantic version requires major, minor, and patch numbers")
            }
            Self::TooManyCoreComponents => {
                formatter.write_str("semantic version contains more than three core numbers")
            }
            Self::LeadingZero { section } => {
                write!(
                    formatter,
                    "semantic version {section} contains a leading zero"
                )
            }
            Self::InvalidNumber { section } => {
                write!(
                    formatter,
                    "semantic version {section} is not a decimal integer"
                )
            }
            Self::NumericOverflow { section } => write!(
                formatter,
                "semantic version {section} exceeds the supported 64-bit range"
            ),
            Self::EmptyIdentifier {
                section,
                identifier,
            } => write!(
                formatter,
                "semantic version {section} identifier {identifier} is empty"
            ),
            Self::InvalidIdentifier { section, index } => write!(
                formatter,
                "semantic version {section} contains an invalid character at byte {index}"
            ),
            Self::MultipleBuildSeparators => {
                formatter.write_str("semantic version contains multiple build separators")
            }
        }
    }
}

impl std::error::Error for ParseSemanticVersionError {}

/// A complete Semantic Versioning 2.0.0 identifier.
///
/// Structural equality includes build metadata. Use [`Self::precedence_cmp`]
/// when comparing release precedence because build metadata does not affect
/// precedence under the specification.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SemanticVersion {
    major: u64,
    minor: u64,
    patch: u64,
    pre_release: Option<String>,
    build_metadata: Option<String>,
}

impl SemanticVersion {
    /// Constructs a normal release version without pre-release or build fields.
    #[must_use]
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre_release: None,
            build_metadata: None,
        }
    }

    /// Returns the major version number.
    #[must_use]
    pub const fn major(&self) -> u64 {
        self.major
    }

    /// Returns the minor version number.
    #[must_use]
    pub const fn minor(&self) -> u64 {
        self.minor
    }

    /// Returns the patch version number.
    #[must_use]
    pub const fn patch(&self) -> u64 {
        self.patch
    }

    /// Returns dot-separated pre-release identifiers when present.
    #[must_use]
    pub fn pre_release(&self) -> Option<&str> {
        self.pre_release.as_deref()
    }

    /// Returns dot-separated build metadata identifiers when present.
    #[must_use]
    pub fn build_metadata(&self) -> Option<&str> {
        self.build_metadata.as_deref()
    }

    /// Compares release precedence according to Semantic Versioning 2.0.0.
    #[must_use]
    pub fn precedence_cmp(&self, other: &Self) -> Ordering {
        match (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)) {
            Ordering::Equal => {
                compare_pre_release(self.pre_release.as_deref(), other.pre_release.as_deref())
            }
            ordering => ordering,
        }
    }
}

impl fmt::Display for SemanticVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre_release) = &self.pre_release {
            write!(formatter, "-{pre_release}")?;
        }
        if let Some(build_metadata) = &self.build_metadata {
            write!(formatter, "+{build_metadata}")?;
        }
        Ok(())
    }
}

impl FromStr for SemanticVersion {
    type Err = ParseSemanticVersionError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        let (without_build, build_metadata) = match input.split_once('+') {
            Some((left, right)) => {
                if right.contains('+') {
                    return Err(ParseSemanticVersionError::MultipleBuildSeparators);
                }
                validate_identifiers(right, VersionSection::BuildMetadata, false)?;
                (left, Some(right.to_owned()))
            }
            None => (input, None),
        };

        let (core, pre_release) = match without_build.split_once('-') {
            Some((left, right)) => {
                validate_identifiers(right, VersionSection::PreRelease, true)?;
                (left, Some(right.to_owned()))
            }
            None => (without_build, None),
        };

        let mut components = core.split('.');
        let major = parse_core_number(
            components
                .next()
                .ok_or(ParseSemanticVersionError::MissingCoreComponent)?,
            VersionSection::Major,
        )?;
        let minor = parse_core_number(
            components
                .next()
                .ok_or(ParseSemanticVersionError::MissingCoreComponent)?,
            VersionSection::Minor,
        )?;
        let patch = parse_core_number(
            components
                .next()
                .ok_or(ParseSemanticVersionError::MissingCoreComponent)?,
            VersionSection::Patch,
        )?;
        if components.next().is_some() {
            return Err(ParseSemanticVersionError::TooManyCoreComponents);
        }

        Ok(Self {
            major,
            minor,
            patch,
            pre_release,
            build_metadata,
        })
    }
}

fn parse_core_number(
    input: &str,
    section: VersionSection,
) -> std::result::Result<u64, ParseSemanticVersionError> {
    if input.is_empty() || !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseSemanticVersionError::InvalidNumber { section });
    }
    if input.len() > 1 && input.starts_with('0') {
        return Err(ParseSemanticVersionError::LeadingZero { section });
    }
    input
        .parse()
        .map_err(|_| ParseSemanticVersionError::NumericOverflow { section })
}

fn validate_identifiers(
    input: &str,
    section: VersionSection,
    reject_numeric_leading_zero: bool,
) -> std::result::Result<(), ParseSemanticVersionError> {
    let mut start = 0_usize;
    for (identifier, value) in input.split('.').enumerate() {
        if value.is_empty() {
            return Err(ParseSemanticVersionError::EmptyIdentifier {
                section,
                identifier,
            });
        }
        for (offset, byte) in value.bytes().enumerate() {
            if !(byte.is_ascii_alphanumeric() || byte == b'-') {
                return Err(ParseSemanticVersionError::InvalidIdentifier {
                    section,
                    index: start + offset,
                });
            }
        }
        if reject_numeric_leading_zero
            && value.len() > 1
            && value.bytes().all(|byte| byte.is_ascii_digit())
            && value.starts_with('0')
        {
            return Err(ParseSemanticVersionError::LeadingZero { section });
        }
        start += value.len() + 1;
    }
    Ok(())
}

fn compare_pre_release(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => {
            let mut left_parts = left.split('.');
            let mut right_parts = right.split('.');
            loop {
                match (left_parts.next(), right_parts.next()) {
                    (None, None) => return Ordering::Equal,
                    (None, Some(_)) => return Ordering::Less,
                    (Some(_), None) => return Ordering::Greater,
                    (Some(left_part), Some(right_part)) => {
                        match compare_pre_release_identifier(left_part, right_part) {
                            Ordering::Equal => {}
                            ordering => return ordering,
                        }
                    }
                }
            }
        }
    }
}

fn compare_pre_release_identifier(left: &str, right: &str) -> Ordering {
    let left_numeric = left.bytes().all(|byte| byte.is_ascii_digit());
    let right_numeric = right.bytes().all(|byte| byte.is_ascii_digit());
    match (left_numeric, right_numeric) {
        (true, true) => left.len().cmp(&right.len()).then_with(|| left.cmp(right)),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => left.cmp(right),
    }
}

/// A failure while parsing a component-qualified version identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParseVersionIdentifierError {
    /// The `@` separator between component and version is absent.
    MissingSeparator,
    /// The component name is not canonical.
    Component(ParseSharedNameError),
    /// The semantic version is invalid.
    Version(ParseSemanticVersionError),
}

impl fmt::Display for ParseVersionIdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSeparator => {
                formatter.write_str("version identifier is missing its component separator")
            }
            Self::Component(error) => write!(formatter, "invalid version component: {error}"),
            Self::Version(error) => write!(formatter, "invalid component version: {error}"),
        }
    }
}

impl std::error::Error for ParseVersionIdentifierError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MissingSeparator => None,
            Self::Component(error) => Some(error),
            Self::Version(error) => Some(error),
        }
    }
}

/// A component-qualified semantic version used at process and project boundaries.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct VersionIdentifier {
    component: ComponentId,
    version: SemanticVersion,
}

impl VersionIdentifier {
    /// Creates a component-qualified version.
    #[must_use]
    pub const fn new(component: ComponentId, version: SemanticVersion) -> Self {
        Self { component, version }
    }

    /// Returns the component identity.
    #[must_use]
    pub const fn component(&self) -> &ComponentId {
        &self.component
    }

    /// Returns the complete semantic version.
    #[must_use]
    pub const fn version(&self) -> &SemanticVersion {
        &self.version
    }
}

impl fmt::Display for VersionIdentifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.component, self.version)
    }
}

impl FromStr for VersionIdentifier {
    type Err = ParseVersionIdentifierError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        let (component, version) = input
            .split_once('@')
            .ok_or(ParseVersionIdentifierError::MissingSeparator)?;
        Ok(Self::new(
            ComponentId::from_str(component).map_err(ParseVersionIdentifierError::Component)?,
            SemanticVersion::from_str(version).map_err(ParseVersionIdentifierError::Version)?,
        ))
    }
}

/// The stable kind tag for a shared setting value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum SettingValueKind {
    /// A boolean value.
    Boolean,
    /// A signed 64-bit integer value.
    Integer,
    /// Owned UTF-8 text.
    Text,
}

impl SettingValueKind {
    /// Every setting value kind defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[Self::Boolean, Self::Integer, Self::Text];

    /// Returns the permanent lowercase code for this value kind.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Text => "text",
        }
    }

    /// Looks up a setting value kind by permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "boolean" => Some(Self::Boolean),
            "integer" => Some(Self::Integer),
            "text" => Some(Self::Text),
            _ => None,
        }
    }
}

impl fmt::Display for SettingValueKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// A platform-neutral shared setting value.
///
/// Floating point is deliberately absent so NaN payloads, signed zero, and
/// platform or language conversion rules cannot alter shared meaning.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum SettingValue {
    /// A boolean flag.
    Boolean(bool),
    /// A fixed-width signed integer.
    Integer(i64),
    /// Owned UTF-8 text.
    Text(String),
}

impl SettingValue {
    /// Returns the stable value kind.
    #[must_use]
    pub const fn kind(&self) -> SettingValueKind {
        match self {
            Self::Boolean(_) => SettingValueKind::Boolean,
            Self::Integer(_) => SettingValueKind::Integer,
            Self::Text(_) => SettingValueKind::Text,
        }
    }

    /// Returns a boolean value without coercion.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns an integer value without coercion.
    #[must_use]
    pub const fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns a text value without coercion.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }
}

/// An immutable, explicitly versioned snapshot of shared settings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsSnapshot {
    schema_version: SemanticVersion,
    values: BTreeMap<SettingKey, SettingValue>,
}

impl SettingsSnapshot {
    /// Builds a deterministic snapshot and rejects duplicate keys.
    pub fn new(
        schema_version: SemanticVersion,
        values: impl IntoIterator<Item = (SettingKey, SettingValue)>,
    ) -> Result<Self> {
        let mut indexed = BTreeMap::new();
        for (key, value) in values {
            if indexed.insert(key.clone(), value).is_some() {
                return Err(validation_error(
                    "create_settings_snapshot",
                    "settings snapshot contains a duplicate key",
                    [("key", key.as_str())],
                ));
            }
        }
        Ok(Self {
            schema_version,
            values: indexed,
        })
    }

    /// Returns the schema used to interpret this snapshot.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns the exact stored value for a key without coercion.
    #[must_use]
    pub fn get(&self, key: &SettingKey) -> Option<&SettingValue> {
        self.values.get(key)
    }

    /// Returns entries in canonical key order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&SettingKey, &SettingValue)> {
        self.values.iter()
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether the snapshot has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// A deterministic set of symbolic capabilities.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilitySet {
    values: BTreeSet<CapabilityId>,
}

impl CapabilitySet {
    /// Builds a set from identifiers. Duplicate declarations are idempotent.
    #[must_use]
    pub fn new(values: impl IntoIterator<Item = CapabilityId>) -> Self {
        Self {
            values: values.into_iter().collect(),
        }
    }

    /// Returns whether a capability is present.
    #[must_use]
    pub fn contains(&self, capability: &CapabilityId) -> bool {
        self.values.contains(capability)
    }

    /// Returns whether every required capability is present.
    #[must_use]
    pub fn contains_all(&self, required: &Self) -> bool {
        required.values.is_subset(&self.values)
    }

    /// Returns capabilities in canonical identifier order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &CapabilityId> {
        self.values.iter()
    }

    /// Returns the number of distinct capabilities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns whether no capabilities are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Whether a known feature can be used in a discovery snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum FeatureAvailability {
    /// The producer can execute the feature now.
    Available,
    /// The feature exists but an explicit setting or policy disables it.
    Disabled,
    /// The producer knows the feature but cannot support it in this build or environment.
    Unsupported,
    /// A temporary runtime condition prevents use.
    Unavailable,
}

impl FeatureAvailability {
    /// Every availability state defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::Available,
        Self::Disabled,
        Self::Unsupported,
        Self::Unavailable,
    ];

    /// Returns the permanent lowercase code for this state.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Disabled => "disabled",
            Self::Unsupported => "unsupported",
            Self::Unavailable => "unavailable",
        }
    }

    /// Looks up an availability state by permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "available" => Some(Self::Available),
            "disabled" => Some(Self::Disabled),
            "unsupported" => Some(Self::Unsupported),
            "unavailable" => Some(Self::Unavailable),
            _ => None,
        }
    }
}

impl fmt::Display for FeatureAvailability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// A complete declaration for one discoverable feature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeatureDescriptor {
    id: FeatureId,
    version: SemanticVersion,
    availability: FeatureAvailability,
    required_capabilities: CapabilitySet,
}

impl FeatureDescriptor {
    /// Creates a feature declaration without consulting global state.
    #[must_use]
    pub const fn new(
        id: FeatureId,
        version: SemanticVersion,
        availability: FeatureAvailability,
        required_capabilities: CapabilitySet,
    ) -> Self {
        Self {
            id,
            version,
            availability,
            required_capabilities,
        }
    }

    /// Returns the feature identity.
    #[must_use]
    pub const fn id(&self) -> &FeatureId {
        &self.id
    }

    /// Returns the feature contract version.
    #[must_use]
    pub const fn version(&self) -> &SemanticVersion {
        &self.version
    }

    /// Returns the declared availability.
    #[must_use]
    pub const fn availability(&self) -> FeatureAvailability {
        self.availability
    }

    /// Returns the capabilities required to execute the feature.
    #[must_use]
    pub const fn required_capabilities(&self) -> &CapabilitySet {
        &self.required_capabilities
    }
}

/// An immutable, validated feature discovery snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeatureDiscovery {
    schema_version: SemanticVersion,
    producer: VersionIdentifier,
    capabilities: CapabilitySet,
    features: BTreeMap<FeatureId, FeatureDescriptor>,
}

impl FeatureDiscovery {
    /// Builds a discovery snapshot with internally consistent available features.
    pub fn new(
        schema_version: SemanticVersion,
        producer: VersionIdentifier,
        capabilities: CapabilitySet,
        features: impl IntoIterator<Item = FeatureDescriptor>,
    ) -> Result<Self> {
        let mut indexed = BTreeMap::new();
        for feature in features {
            if feature.availability == FeatureAvailability::Available {
                if let Some(missing) = feature
                    .required_capabilities
                    .iter()
                    .find(|capability| !capabilities.contains(capability))
                {
                    return Err(validation_error(
                        "create_feature_discovery",
                        "available feature requires a capability absent from the snapshot",
                        [
                            ("feature", feature.id.as_str()),
                            ("capability", missing.as_str()),
                        ],
                    ));
                }
            }

            let id = feature.id.clone();
            if indexed.insert(id.clone(), feature).is_some() {
                return Err(validation_error(
                    "create_feature_discovery",
                    "feature discovery contains a duplicate feature",
                    [("feature", id.as_str())],
                ));
            }
        }

        Ok(Self {
            schema_version,
            producer,
            capabilities,
            features: indexed,
        })
    }

    /// Returns the schema used to interpret this snapshot.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns the component that produced the snapshot.
    #[must_use]
    pub const fn producer(&self) -> &VersionIdentifier {
        &self.producer
    }

    /// Returns all declared capabilities.
    #[must_use]
    pub const fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    /// Returns one known feature by stable identity.
    #[must_use]
    pub fn feature(&self, id: &FeatureId) -> Option<&FeatureDescriptor> {
        self.features.get(id)
    }

    /// Returns whether a known feature is currently available.
    #[must_use]
    pub fn is_available(&self, id: &FeatureId) -> bool {
        self.feature(id)
            .is_some_and(|feature| feature.availability == FeatureAvailability::Available)
    }

    /// Returns feature declarations in canonical identifier order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &FeatureDescriptor> {
        self.features.values()
    }

    /// Returns the number of known features.
    #[must_use]
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Returns whether no features are declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

fn validation_error<'a>(
    operation: &'static str,
    message: &'static str,
    fields: impl IntoIterator<Item = (&'static str, &'a str)>,
) -> Error {
    let mut context = ErrorContext::new("superi-core.settings", operation);
    for (key, value) in fields {
        context = context.with_field(key, value);
    }
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context)
}
