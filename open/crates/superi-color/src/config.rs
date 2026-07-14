//! Immutable, versioned color-management configuration.
//!
//! This module owns bounded configuration-file loading, named scene-linear
//! working spaces, aliases, roles, deterministic semantic identity, and the
//! project value that pins one working space to one exact config. Display,
//! view, look, file-rule, and transform-graph execution remain separate color
//! pipeline stages and are never approximated here.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::working_space::WorkingSpace;

const COMPONENT: &str = "superi-color.config";
const CONFIG_SCHEMA: &str = "superi.color-config";
const CONFIG_VERSION: u32 = 1;
const PROJECT_SETTINGS_VERSION: u32 = 1;

/// One validated named working-space declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigWorkingSpace {
    id: String,
    aliases: Vec<String>,
    space: WorkingSpace,
}

impl ConfigWorkingSpace {
    /// Returns the stable config-local identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns alternate identifiers in deterministic order.
    #[must_use]
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    /// Returns the exact validated scene-linear working space.
    #[must_use]
    pub const fn space(&self) -> WorkingSpace {
        self.space
    }
}

/// Immutable runtime form of one validated color-management file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColorManagementConfig {
    id: String,
    default_working_space: String,
    working_spaces: BTreeMap<String, ConfigWorkingSpace>,
    aliases: BTreeMap<String, String>,
    roles: BTreeMap<String, String>,
    fingerprint: String,
}

impl ColorManagementConfig {
    /// Maximum accepted encoded config size.
    pub const MAX_FILE_BYTES: usize = 1024 * 1024;
    /// Maximum accepted number of named working spaces.
    pub const MAX_WORKING_SPACES: usize = 64;

    /// Loads and validates a UTF-8 JSON color-management file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|error| io_error(path, error))?;
        let metadata = file.metadata().map_err(|error| io_error(path, error))?;
        let length = usize::try_from(metadata.len()).map_err(|_| {
            exhausted(
                "load_config",
                "color config file size cannot be represented",
            )
        })?;
        validate_source_size(length)?;
        let mut bytes = Vec::with_capacity(length.min(Self::MAX_FILE_BYTES));
        file.take((Self::MAX_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| io_error(path, error))?;
        Self::from_json(&bytes).map_err(|error| {
            error.with_context(context("load_config").with_field("path", path.to_string_lossy()))
        })
    }

    /// Parses and validates a bounded JSON color-management artifact.
    pub fn from_json(source: &[u8]) -> Result<Self> {
        validate_source_size(source.len())?;
        let raw: RawConfig = serde_json::from_slice(source).map_err(|error| {
            invalid("parse_config", "color config is not valid strict JSON").with_context(
                context("parse_config")
                    .with_field("line", error.line().to_string())
                    .with_field("column", error.column().to_string()),
            )
        })?;
        Self::validate(raw)
    }

    fn validate(raw: RawConfig) -> Result<Self> {
        if raw.schema != CONFIG_SCHEMA {
            return Err(unsupported(
                "validate_schema",
                "color config schema is not supported",
            ));
        }
        if raw.version != CONFIG_VERSION {
            return Err(
                unsupported("validate_version", "color config version is not supported")
                    .with_context(
                        context("validate_version").with_field("version", raw.version.to_string()),
                    ),
            );
        }
        validate_name("config_id", &raw.id)?;
        validate_name("default_working_space", &raw.default_working_space)?;
        if raw.working_spaces.is_empty() {
            return Err(invalid(
                "validate_working_spaces",
                "color config must declare at least one working space",
            ));
        }
        if raw.working_spaces.len() > Self::MAX_WORKING_SPACES {
            return Err(exhausted(
                "validate_working_spaces",
                "color config exceeds the working-space limit",
            ));
        }

        let mut working_spaces = BTreeMap::new();
        let mut aliases = BTreeMap::new();
        for raw_space in raw.working_spaces {
            validate_name("working_space_id", &raw_space.id)?;
            if working_spaces.contains_key(&raw_space.id) {
                return Err(invalid(
                    "validate_working_spaces",
                    "working-space identifiers must be unique",
                )
                .with_context(context("validate_working_spaces").with_field("id", raw_space.id)));
            }
            let color_space = parse_color_space(&raw_space)?;
            let space = WorkingSpace::new(color_space).map_err(|error| {
                error.with_context(
                    context("validate_working_space").with_field("id", raw_space.id.clone()),
                )
            })?;
            let mut declared_aliases = raw_space.aliases;
            declared_aliases.sort();
            let mut unique_aliases = BTreeSet::new();
            for alias in &declared_aliases {
                validate_name("working_space_alias", alias)?;
                if alias == &raw_space.id
                    || !unique_aliases.insert(alias.clone())
                    || aliases.contains_key(alias)
                {
                    return Err(invalid(
                        "validate_aliases",
                        "working-space aliases must be globally unique",
                    )
                    .with_context(context("validate_aliases").with_field("alias", alias)));
                }
                aliases.insert(alias.clone(), raw_space.id.clone());
            }
            working_spaces.insert(
                raw_space.id.clone(),
                ConfigWorkingSpace {
                    id: raw_space.id,
                    aliases: declared_aliases,
                    space,
                },
            );
        }
        for id in working_spaces.keys() {
            if aliases.contains_key(id) {
                return Err(invalid(
                    "validate_aliases",
                    "an alias must not collide with a working-space identifier",
                )
                .with_context(context("validate_aliases").with_field("id", id)));
            }
        }
        if !working_spaces.contains_key(&raw.default_working_space) {
            return Err(invalid(
                "validate_default_working_space",
                "default working space does not exist",
            ));
        }

        let mut roles = BTreeMap::new();
        for (role, target) in raw.roles {
            validate_name("role", &role)?;
            validate_name("role_target", &target)?;
            let resolved = resolve_id(&working_spaces, &aliases, &target).ok_or_else(|| {
                invalid("validate_roles", "color role target does not exist")
                    .with_context(context("validate_roles").with_field("role", role.clone()))
            })?;
            roles.insert(role, resolved.to_owned());
        }

        let fingerprint =
            semantic_fingerprint(&raw.id, &raw.default_working_space, &working_spaces, &roles);
        Ok(Self {
            id: raw.id,
            default_working_space: raw.default_working_space,
            working_spaces,
            aliases,
            roles,
            fingerprint,
        })
    }

    /// Returns the stable config identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the deterministic SHA-256 identity of validated config semantics.
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Returns the configured default working-space identifier.
    #[must_use]
    pub fn default_working_space_id(&self) -> &str {
        &self.default_working_space
    }

    /// Returns the configured default working space.
    #[must_use]
    pub fn default_working_space(&self) -> WorkingSpace {
        self.working_spaces[&self.default_working_space].space
    }

    /// Resolves a working-space identifier or alias.
    #[must_use]
    pub fn working_space(&self, name: &str) -> Option<WorkingSpace> {
        resolve_id(&self.working_spaces, &self.aliases, name)
            .and_then(|id| self.working_spaces.get(id))
            .map(|entry| entry.space)
    }

    /// Resolves a task role to its working space.
    #[must_use]
    pub fn role(&self, role: &str) -> Option<WorkingSpace> {
        self.roles
            .get(role)
            .and_then(|id| self.working_spaces.get(id))
            .map(|entry| entry.space)
    }

    /// Iterates canonical working-space identifiers in deterministic order.
    pub fn working_space_ids(&self) -> impl Iterator<Item = &str> {
        self.working_spaces.keys().map(String::as_str)
    }
}

/// Project-owned working-space selection pinned to one exact config artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectColorSettings {
    version: u32,
    config_id: String,
    config_fingerprint: String,
    working_space: String,
}

impl ProjectColorSettings {
    /// Pins one named working space from a validated config.
    pub fn new(config: &ColorManagementConfig, working_space: &str) -> Result<Self> {
        let canonical = resolve_id(&config.working_spaces, &config.aliases, working_space)
            .ok_or_else(|| not_found("select_project_working_space", working_space))?;
        Ok(Self {
            version: PROJECT_SETTINGS_VERSION,
            config_id: config.id.clone(),
            config_fingerprint: config.fingerprint.clone(),
            working_space: canonical.to_owned(),
        })
    }

    /// Returns the serialized project-settings schema version.
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Returns the pinned color-config identifier.
    #[must_use]
    pub fn config_id(&self) -> &str {
        &self.config_id
    }

    /// Returns the pinned semantic color-config fingerprint.
    #[must_use]
    pub fn config_fingerprint(&self) -> &str {
        &self.config_fingerprint
    }

    /// Returns the canonical configured working-space identifier.
    #[must_use]
    pub fn working_space_id(&self) -> &str {
        &self.working_space
    }

    /// Resolves this selection only against the exact config it pins.
    pub fn resolve(&self, config: &ColorManagementConfig) -> Result<WorkingSpace> {
        if self.version != PROJECT_SETTINGS_VERSION {
            return Err(unsupported(
                "resolve_project_settings",
                "project color-settings version is not supported",
            ));
        }
        if self.config_id != config.id || self.config_fingerprint != config.fingerprint {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "project color config identity does not match the loaded config",
            )
            .with_context(
                context("resolve_project_settings")
                    .with_field("expected_config_id", &self.config_id)
                    .with_field("actual_config_id", &config.id),
            ));
        }
        config
            .working_spaces
            .get(&self.working_space)
            .map(|entry| entry.space)
            .ok_or_else(|| not_found("resolve_project_working_space", &self.working_space))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    schema: String,
    version: u32,
    id: String,
    default_working_space: String,
    #[serde(default)]
    roles: BTreeMap<String, String>,
    working_spaces: Vec<RawWorkingSpace>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkingSpace {
    id: String,
    #[serde(default)]
    aliases: Vec<String>,
    primaries: String,
    transfer: String,
    matrix: String,
    range: String,
}

fn parse_color_space(raw: &RawWorkingSpace) -> Result<ColorSpace> {
    let primaries = ColorPrimaries::from_code(&raw.primaries)
        .ok_or_else(|| unsupported_code("primaries", &raw.primaries))?;
    let transfer = TransferFunction::from_code(&raw.transfer)
        .ok_or_else(|| unsupported_code("transfer", &raw.transfer))?;
    let matrix = MatrixCoefficients::from_code(&raw.matrix)
        .ok_or_else(|| unsupported_code("matrix", &raw.matrix))?;
    let range =
        ColorRange::from_code(&raw.range).ok_or_else(|| unsupported_code("range", &raw.range))?;
    Ok(ColorSpace::new(primaries, transfer, matrix, range))
}

fn resolve_id<'a>(
    spaces: &'a BTreeMap<String, ConfigWorkingSpace>,
    aliases: &'a BTreeMap<String, String>,
    name: &str,
) -> Option<&'a str> {
    spaces
        .get_key_value(name)
        .map(|(id, _)| id.as_str())
        .or_else(|| aliases.get(name).map(String::as_str))
}

fn semantic_fingerprint(
    id: &str,
    default: &str,
    spaces: &BTreeMap<String, ConfigWorkingSpace>,
    roles: &BTreeMap<String, String>,
) -> String {
    let mut digest = Sha256::new();
    hash_text(&mut digest, "schema");
    hash_text(&mut digest, CONFIG_SCHEMA);
    digest.update(CONFIG_VERSION.to_be_bytes());
    hash_text(&mut digest, "identity");
    hash_text(&mut digest, id);
    hash_text(&mut digest, default);
    hash_text(&mut digest, "working_spaces");
    digest.update((spaces.len() as u64).to_be_bytes());
    for (space_id, entry) in spaces {
        hash_text(&mut digest, space_id);
        digest.update((entry.aliases.len() as u64).to_be_bytes());
        for alias in &entry.aliases {
            hash_text(&mut digest, alias);
        }
        let color = entry.space.color_space();
        hash_text(&mut digest, color.primaries().code());
        hash_text(&mut digest, color.transfer().code());
        hash_text(&mut digest, color.matrix().code());
        hash_text(&mut digest, color.range().code());
    }
    hash_text(&mut digest, "roles");
    digest.update((roles.len() as u64).to_be_bytes());
    for (role, target) in roles {
        hash_text(&mut digest, role);
        hash_text(&mut digest, target);
    }
    let mut encoded = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn hash_text(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

fn validate_source_size(length: usize) -> Result<()> {
    if length == 0 {
        return Err(invalid("parse_config", "color config file is empty"));
    }
    if length > ColorManagementConfig::MAX_FILE_BYTES {
        return Err(exhausted(
            "parse_config",
            "color config exceeds the fixed byte limit",
        ));
    }
    Ok(())
}

fn validate_name(field: &'static str, value: &str) -> Result<()> {
    let mut bytes = value.bytes();
    let valid = value.len() <= 128
        && bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        });
    if valid {
        Ok(())
    } else {
        Err(invalid(
            "validate_identifier",
            "color config identifiers must use bounded lowercase ASCII",
        )
        .with_context(context("validate_identifier").with_field("field", field)))
    }
}

fn unsupported_code(field: &'static str, code: &str) -> Error {
    unsupported("parse_color_space", "color-space code is not supported")
        .with_context(context("parse_color_space").with_field(field, code))
}

fn not_found(operation: &'static str, name: &str) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        "configured working space does not exist",
    )
    .with_context(context(operation).with_field("name", name))
}

fn io_error(path: &Path, error: std::io::Error) -> Error {
    let category = match error.kind() {
        std::io::ErrorKind::NotFound => ErrorCategory::NotFound,
        std::io::ErrorKind::PermissionDenied => ErrorCategory::PermissionDenied,
        _ => ErrorCategory::Unavailable,
    };
    Error::new(
        category,
        Recoverability::UserCorrectable,
        "color config file could not be read",
    )
    .with_context(
        context("read_config_file")
            .with_field("path", path.to_string_lossy())
            .with_field("io_kind", format!("{:?}", error.kind())),
    )
}

fn context(operation: &'static str) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context(operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context(operation))
}

fn exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context(operation))
}
