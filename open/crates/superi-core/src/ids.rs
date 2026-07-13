//! Strongly typed identifiers shared by every Superi subsystem.
//!
//! Each concrete type carries one opaque 128-bit value. The Rust type preserves
//! the object's domain, while the canonical text includes the same domain as a
//! permanent prefix. Allocation and derivation policies belong to the subsystem
//! that owns the identified state.
//!
//! Domain types cannot be substituted accidentally:
//!
//! ```compile_fail
//! use superi_core::ids::{MediaId, ProjectId};
//!
//! fn open_project(_project_id: ProjectId) {}
//!
//! open_project(MediaId::from_raw(1));
//! ```

use std::fmt;
use std::str::FromStr;

/// A stable description of the object domain carried by an identifier.
///
/// Concrete identifier newtypes retain the static distinction. This enum is
/// intended for generic inspection and boundary validation, not as a substitute
/// for those concrete types.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum IdentifierKind {
    /// A saved or in-memory project document.
    Project,
    /// An imported or generated media resource.
    Media,
    /// An editorial track.
    Track,
    /// An editorial clip.
    Clip,
    /// A processing graph node.
    Node,
    /// An editable node or operation parameter.
    Parameter,
    /// A scheduled unit of work.
    Job,
    /// A replaceable derived-data cache.
    Cache,
    /// A processing or presentation device.
    Device,
}

impl IdentifierKind {
    /// Every identifier domain defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::Project,
        Self::Media,
        Self::Track,
        Self::Clip,
        Self::Node,
        Self::Parameter,
        Self::Job,
        Self::Cache,
        Self::Device,
    ];

    /// Returns the permanent lowercase code for this domain.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Media => "media",
            Self::Track => "track",
            Self::Clip => "clip",
            Self::Node => "node",
            Self::Parameter => "parameter",
            Self::Job => "job",
            Self::Cache => "cache",
            Self::Device => "device",
        }
    }

    /// Looks up an identifier domain by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "project" => Some(Self::Project),
            "media" => Some(Self::Media),
            "track" => Some(Self::Track),
            "clip" => Some(Self::Clip),
            "node" => Some(Self::Node),
            "parameter" => Some(Self::Parameter),
            "job" => Some(Self::Job),
            "cache" => Some(Self::Cache),
            "device" => Some(Self::Device),
            _ => None,
        }
    }
}

impl fmt::Display for IdentifierKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// A failure while parsing a canonical typed identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParseIdentifierError {
    /// The required separator between kind and payload is absent.
    MissingSeparator,
    /// The kind prefix is not defined by this version of `superi-core`.
    UnknownKind,
    /// The prefix names a different concrete identifier domain.
    UnexpectedKind {
        /// The domain required by the requested concrete type.
        expected: IdentifierKind,
        /// The domain found in the input.
        actual: IdentifierKind,
    },
    /// The hexadecimal payload does not contain exactly 32 characters.
    InvalidLength {
        /// The required payload length.
        expected: usize,
        /// The input payload length.
        actual: usize,
    },
    /// The payload contains a byte outside lowercase hexadecimal syntax.
    InvalidHex {
        /// The zero-based byte index within the hexadecimal payload.
        index: usize,
    },
}

impl fmt::Display for ParseIdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSeparator => {
                formatter.write_str("identifier is missing its kind separator")
            }
            Self::UnknownKind => formatter.write_str("identifier has an unknown kind"),
            Self::UnexpectedKind { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} identifier, found {actual} identifier"
                )
            }
            Self::InvalidLength { expected, actual } => write!(
                formatter,
                "identifier payload must contain {expected} hexadecimal characters, found {actual}"
            ),
            Self::InvalidHex { index } => write!(
                formatter,
                "identifier payload contains invalid hexadecimal at byte {index}"
            ),
        }
    }
}

impl std::error::Error for ParseIdentifierError {}

mod private {
    pub trait Sealed {}
}

/// Common value access for official concrete Superi identifier types.
///
/// This trait is sealed so extensions cannot claim a custom value is one of the
/// core domains. Generic inspection may use it without erasing the concrete type
/// from APIs that own or mutate state.
pub trait TypedId:
    private::Sealed + Clone + Copy + Eq + Ord + std::hash::Hash + Send + Sync + 'static
{
    /// The object domain carried by this concrete type.
    const KIND: IdentifierKind;

    /// Constructs the typed identifier from its complete opaque value.
    #[must_use]
    fn from_raw(raw: u128) -> Self;

    /// Returns the complete opaque value.
    #[must_use]
    fn raw(self) -> u128;

    /// Constructs the identifier from platform-independent big-endian bytes.
    #[must_use]
    fn from_bytes(bytes: [u8; 16]) -> Self {
        Self::from_raw(u128::from_be_bytes(bytes))
    }

    /// Returns platform-independent big-endian bytes.
    #[must_use]
    fn to_bytes(self) -> [u8; 16] {
        self.raw().to_be_bytes()
    }
}

fn parse_identifier(input: &str, expected: IdentifierKind) -> Result<u128, ParseIdentifierError> {
    let (kind_code, payload) = input
        .split_once(':')
        .ok_or(ParseIdentifierError::MissingSeparator)?;
    let actual = IdentifierKind::from_code(kind_code).ok_or(ParseIdentifierError::UnknownKind)?;
    if actual != expected {
        return Err(ParseIdentifierError::UnexpectedKind { expected, actual });
    }
    if payload.len() != 32 {
        return Err(ParseIdentifierError::InvalidLength {
            expected: 32,
            actual: payload.len(),
        });
    }

    let mut raw = 0_u128;
    for (index, byte) in payload.bytes().enumerate() {
        let nibble = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => return Err(ParseIdentifierError::InvalidHex { index }),
        };
        raw = (raw << 4) | u128::from(nibble);
    }
    Ok(raw)
}

macro_rules! define_identifier {
    ($name:ident, $kind:ident, $summary:literal) => {
        #[doc = $summary]
        #[repr(transparent)]
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u128);

        impl $name {
            /// The object domain carried by this type.
            pub const KIND: IdentifierKind = IdentifierKind::$kind;

            /// Constructs the typed identifier from its complete opaque value.
            ///
            /// Every 128-bit value is representable. The owning subsystem decides
            /// how values are allocated or derived and whether zero has meaning in
            /// its own state model.
            #[must_use]
            pub const fn from_raw(raw: u128) -> Self {
                Self(raw)
            }

            /// Returns the complete opaque value.
            #[must_use]
            pub const fn raw(self) -> u128 {
                self.0
            }

            /// Constructs the identifier from platform-independent big-endian bytes.
            #[must_use]
            pub const fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(u128::from_be_bytes(bytes))
            }

            /// Returns platform-independent big-endian bytes.
            #[must_use]
            pub const fn to_bytes(self) -> [u8; 16] {
                self.0.to_be_bytes()
            }
        }

        impl private::Sealed for $name {}

        impl TypedId for $name {
            const KIND: IdentifierKind = Self::KIND;

            fn from_raw(raw: u128) -> Self {
                Self::from_raw(raw)
            }

            fn raw(self) -> u128 {
                self.raw()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}:{:032x}", Self::KIND, self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}({})", stringify!($name), self)
            }
        }

        impl FromStr for $name {
            type Err = ParseIdentifierError;

            fn from_str(input: &str) -> Result<Self, Self::Err> {
                parse_identifier(input, Self::KIND).map(Self::from_raw)
            }
        }
    };
}

define_identifier!(ProjectId, Project, "A strongly typed project identifier.");
define_identifier!(MediaId, Media, "A strongly typed media identifier.");
define_identifier!(
    TrackId,
    Track,
    "A strongly typed editorial track identifier."
);
define_identifier!(ClipId, Clip, "A strongly typed editorial clip identifier.");
define_identifier!(NodeId, Node, "A strongly typed processing node identifier.");
define_identifier!(
    ParameterId,
    Parameter,
    "A strongly typed editable parameter identifier."
);
define_identifier!(JobId, Job, "A strongly typed scheduled job identifier.");
define_identifier!(CacheId, Cache, "A strongly typed derived cache identifier.");
define_identifier!(
    DeviceId,
    Device,
    "A strongly typed processing device identifier."
);
