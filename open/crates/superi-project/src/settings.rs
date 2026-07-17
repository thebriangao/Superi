//! Authoritative versioned project settings and atomic mutation vocabulary.
//!
//! Settings remain platform-neutral shared values. This module owns the exact
//! project key set, conditional relationships, deterministic defaults, and
//! transaction validation. Engine subsystems resolve these durable values into
//! their existing typed policy contracts without rewriting authored state.

use std::collections::{BTreeMap, BTreeSet};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{SemanticVersion, SettingKey, SettingValue, SettingsSnapshot};
use superi_core::time::Timebase;
use superi_core::timecode::TimecodeFormat;

const COMPONENT: &str = "superi-project.settings";

/// Settings document format revision stored by the project database.
pub const PROJECT_SETTINGS_FORMAT_REVISION: u32 = 1;
/// Maximum mutations accepted in one atomic settings transaction.
pub const MAX_PROJECT_SETTING_MUTATIONS: usize = 64;
/// Maximum bytes accepted in a project-owned symbolic identifier.
pub const MAX_PROJECT_SETTING_IDENTIFIER_BYTES: usize = 128;

/// Exact schema for the project settings key vocabulary.
pub const PROJECT_SETTINGS_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

pub const TIMELINE_RATE_NUMERATOR_KEY: &str = "superi.project.timeline.default_rate_numerator";
pub const TIMELINE_RATE_DENOMINATOR_KEY: &str = "superi.project.timeline.default_rate_denominator";
pub const TIMELINE_TIMECODE_MODE_KEY: &str = "superi.project.timeline.timecode_mode";
pub const COLOR_MODE_KEY: &str = "superi.project.color.mode";
pub const COLOR_WORKING_SPACE_KEY: &str = "superi.project.color.working_space";
pub const COLOR_CONFIG_ID_KEY: &str = "superi.project.color.config_id";
pub const COLOR_CONFIG_FINGERPRINT_KEY: &str = "superi.project.color.config_fingerprint";
pub const AUDIO_SAMPLE_RATE_KEY: &str = "superi.project.audio.sample_rate_hz";
pub const AUDIO_OUTPUT_LAYOUT_KEY: &str = "superi.project.audio.output_layout";
pub const CACHE_MODE_KEY: &str = "superi.project.cache.mode";
pub const CACHE_MAX_BYTES_KEY: &str = "superi.project.cache.max_bytes";
pub const CACHE_MAX_FRAMES_KEY: &str = "superi.project.cache.max_frames";
pub const PROXY_MODE_KEY: &str = "superi.project.proxy.mode";
pub const PROXY_QUALITY_KEY: &str = "superi.project.proxy.quality";
pub const RENDER_FRAME_RATE_MODE_KEY: &str = "superi.project.render.frame_rate_mode";
pub const RENDER_RATE_NUMERATOR_KEY: &str = "superi.project.render.rate_numerator";
pub const RENDER_RATE_DENOMINATOR_KEY: &str = "superi.project.render.rate_denominator";
pub const RENDER_EXTENT_MODE_KEY: &str = "superi.project.render.extent_mode";
pub const RENDER_WIDTH_KEY: &str = "superi.project.render.width";
pub const RENDER_HEIGHT_KEY: &str = "superi.project.render.height";
pub const RENDER_PIXEL_FORMAT_KEY: &str = "superi.project.render.pixel_format";
pub const RENDER_ALPHA_MODE_KEY: &str = "superi.project.render.alpha_mode";
pub const RENDER_COLOR_TARGET_KEY: &str = "superi.project.render.color_target";

/// Every supported key in stable domain order.
pub const PROJECT_SETTING_KEYS: &[&str] = &[
    TIMELINE_RATE_NUMERATOR_KEY,
    TIMELINE_RATE_DENOMINATOR_KEY,
    TIMELINE_TIMECODE_MODE_KEY,
    COLOR_MODE_KEY,
    COLOR_WORKING_SPACE_KEY,
    COLOR_CONFIG_ID_KEY,
    COLOR_CONFIG_FINGERPRINT_KEY,
    AUDIO_SAMPLE_RATE_KEY,
    AUDIO_OUTPUT_LAYOUT_KEY,
    CACHE_MODE_KEY,
    CACHE_MAX_BYTES_KEY,
    CACHE_MAX_FRAMES_KEY,
    PROXY_MODE_KEY,
    PROXY_QUALITY_KEY,
    RENDER_FRAME_RATE_MODE_KEY,
    RENDER_RATE_NUMERATOR_KEY,
    RENDER_RATE_DENOMINATOR_KEY,
    RENDER_EXTENT_MODE_KEY,
    RENDER_WIDTH_KEY,
    RENDER_HEIGHT_KEY,
    RENDER_PIXEL_FORMAT_KEY,
    RENDER_ALPHA_MODE_KEY,
    RENDER_COLOR_TARGET_KEY,
];

/// One immutable, validated project settings snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSettings {
    snapshot: SettingsSnapshot,
}

impl ProjectSettings {
    /// Creates deterministic settings from the selected root timeline edit rate.
    pub fn defaults(root_edit_rate: Timebase) -> Result<Self> {
        let values = [
            integer(
                TIMELINE_RATE_NUMERATOR_KEY,
                i64::from(root_edit_rate.numerator()),
            )?,
            integer(
                TIMELINE_RATE_DENOMINATOR_KEY,
                i64::from(root_edit_rate.denominator()),
            )?,
            text(TIMELINE_TIMECODE_MODE_KEY, "non_drop_frame")?,
            text(COLOR_MODE_KEY, "built_in_acescg")?,
            text(COLOR_WORKING_SPACE_KEY, "acescg")?,
            integer(AUDIO_SAMPLE_RATE_KEY, 48_000)?,
            text(AUDIO_OUTPUT_LAYOUT_KEY, "stereo")?,
            text(CACHE_MODE_KEY, "automatic")?,
            text(PROXY_MODE_KEY, "disabled")?,
            text(PROXY_QUALITY_KEY, "half")?,
            text(RENDER_FRAME_RATE_MODE_KEY, "timeline")?,
            text(RENDER_EXTENT_MODE_KEY, "source")?,
            text(RENDER_PIXEL_FORMAT_KEY, "rgba16_float")?,
            text(RENDER_ALPHA_MODE_KEY, "premultiplied")?,
            text(RENDER_COLOR_TARGET_KEY, "project_working")?,
        ];
        Self::from_snapshot(SettingsSnapshot::new(
            PROJECT_SETTINGS_SCHEMA_VERSION.clone(),
            values,
        )?)
    }

    /// Validates a shared settings snapshot as this exact project schema.
    pub fn from_snapshot(snapshot: SettingsSnapshot) -> Result<Self> {
        if snapshot.schema_version() != &PROJECT_SETTINGS_SCHEMA_VERSION {
            return Err(unsupported(
                "validate_schema",
                "project settings schema version is not supported",
            ));
        }
        validate_snapshot(&snapshot)?;
        Ok(Self { snapshot })
    }

    /// Returns the exact shared schema version.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        self.snapshot.schema_version()
    }

    /// Returns the immutable shared settings representation.
    #[must_use]
    pub const fn snapshot(&self) -> &SettingsSnapshot {
        &self.snapshot
    }

    /// Iterates values in canonical key order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&SettingKey, &SettingValue)> {
        self.snapshot.iter()
    }

    /// Returns one value by its permanent key text.
    #[must_use]
    pub fn value(&self, key: &str) -> Option<&SettingValue> {
        SettingKey::new(key)
            .ok()
            .and_then(|key| self.snapshot.get(&key))
    }

    /// Returns one integer without coercion.
    #[must_use]
    pub fn integer(&self, key: &str) -> Option<i64> {
        self.value(key).and_then(SettingValue::as_integer)
    }

    /// Returns one text value without coercion.
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        self.value(key).and_then(SettingValue::as_text)
    }

    pub(crate) fn apply(&self, transaction: ProjectSettingsTransaction) -> Result<Self> {
        let mut candidate = self
            .snapshot
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        for mutation in transaction.mutations {
            match mutation {
                ProjectSettingMutation::Set { key, value } => {
                    candidate.insert(key, value);
                }
                ProjectSettingMutation::Remove { key } => {
                    candidate.remove(&key);
                }
            }
        }
        Self::from_snapshot(SettingsSnapshot::new(
            PROJECT_SETTINGS_SCHEMA_VERSION.clone(),
            candidate,
        )?)
    }
}

/// One ordered mutation in a project settings transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectSettingMutation {
    /// Insert or replace one exact typed value.
    Set {
        /// Permanent namespaced key.
        key: SettingKey,
        /// Exact value without coercion.
        value: SettingValue,
    },
    /// Remove one conditional key.
    Remove {
        /// Permanent namespaced key.
        key: SettingKey,
    },
}

impl ProjectSettingMutation {
    /// Creates a checked set mutation.
    pub fn set(key: impl AsRef<str>, value: SettingValue) -> Result<Self> {
        Ok(Self::Set {
            key: parse_key(key.as_ref())?,
            value,
        })
    }

    /// Creates a checked remove mutation.
    pub fn remove(key: impl AsRef<str>) -> Result<Self> {
        Ok(Self::Remove {
            key: parse_key(key.as_ref())?,
        })
    }

    /// Returns the affected key.
    #[must_use]
    pub const fn key(&self) -> &SettingKey {
        match self {
            Self::Set { key, .. } | Self::Remove { key } => key,
        }
    }
}

/// One optimistic, ordered, atomic project settings transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSettingsTransaction {
    expected_revision: u64,
    mutations: Vec<ProjectSettingMutation>,
}

impl ProjectSettingsTransaction {
    /// Creates a bounded transaction and rejects duplicate keys.
    pub fn new(expected_revision: u64, mutations: Vec<ProjectSettingMutation>) -> Result<Self> {
        if mutations.is_empty() {
            return Err(invalid(
                "validate_transaction",
                "project settings transaction must contain at least one mutation",
            ));
        }
        if mutations.len() > MAX_PROJECT_SETTING_MUTATIONS {
            return Err(exhausted(
                "validate_transaction",
                "project settings transaction exceeds the mutation bound",
            ));
        }
        let mut keys = BTreeSet::new();
        for mutation in &mutations {
            if !keys.insert(mutation.key().clone()) {
                return Err(invalid(
                    "validate_transaction",
                    "project settings transaction contains a duplicate key",
                )
                .with_context(
                    context("validate_transaction").with_field("key", mutation.key().as_str()),
                ));
            }
        }
        Ok(Self {
            expected_revision,
            mutations,
        })
    }

    /// Returns the exact project revision fence.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Returns ordered mutations.
    #[must_use]
    pub fn mutations(&self) -> &[ProjectSettingMutation] {
        &self.mutations
    }
}

fn validate_snapshot(snapshot: &SettingsSnapshot) -> Result<()> {
    for (key, _) in snapshot.iter() {
        if !PROJECT_SETTING_KEYS.contains(&key.as_str()) {
            return Err(
                unsupported("validate_key", "project setting key is not supported")
                    .with_context(context("validate_key").with_field("key", key.as_str())),
            );
        }
    }

    let timeline_rate = superi_core::time::FrameRate::new(
        required_u32(snapshot, TIMELINE_RATE_NUMERATOR_KEY)?,
        required_u32(snapshot, TIMELINE_RATE_DENOMINATOR_KEY)?,
    )?;
    match required_text(snapshot, TIMELINE_TIMECODE_MODE_KEY)? {
        "non_drop_frame" => {}
        "drop_frame" => {
            if TimecodeFormat::drop_frame(timeline_rate).is_err() {
                return Err(invalid(
                    "validate_timeline",
                    "drop-frame timecode is unsupported for the selected timeline rate",
                ));
            }
        }
        _ => {
            return Err(invalid(
                "validate_timeline",
                "project timeline timecode mode is not supported",
            ));
        }
    }

    let color_mode = required_text(snapshot, COLOR_MODE_KEY)?;
    let working_space = required_text(snapshot, COLOR_WORKING_SPACE_KEY)?;
    validate_identifier(working_space, "working_space")?;
    match color_mode {
        "built_in_acescg" => {
            if working_space != "acescg" {
                return Err(invalid(
                    "validate_color",
                    "built-in ACEScg mode requires the acescg working space",
                ));
            }
            reject_present(snapshot, COLOR_CONFIG_ID_KEY, "validate_color")?;
            reject_present(snapshot, COLOR_CONFIG_FINGERPRINT_KEY, "validate_color")?;
        }
        "pinned_config" => {
            validate_identifier(required_text(snapshot, COLOR_CONFIG_ID_KEY)?, "config_id")?;
            validate_fingerprint(required_text(snapshot, COLOR_CONFIG_FINGERPRINT_KEY)?)?;
        }
        _ => {
            return Err(invalid(
                "validate_color",
                "project color mode is not supported",
            ));
        }
    }

    let _ = required_u32(snapshot, AUDIO_SAMPLE_RATE_KEY)?;
    require_code(
        snapshot,
        AUDIO_OUTPUT_LAYOUT_KEY,
        &["mono", "stereo", "quad", "surround_5_1", "surround_7_1"],
        "validate_audio",
    )?;

    match required_text(snapshot, CACHE_MODE_KEY)? {
        "automatic" | "disabled" => {
            reject_present(snapshot, CACHE_MAX_BYTES_KEY, "validate_cache")?;
            reject_present(snapshot, CACHE_MAX_FRAMES_KEY, "validate_cache")?;
        }
        "bounded" => {
            let _ = required_positive(snapshot, CACHE_MAX_BYTES_KEY)?;
            let _ = required_positive(snapshot, CACHE_MAX_FRAMES_KEY)?;
        }
        _ => {
            return Err(invalid(
                "validate_cache",
                "project cache mode is not supported",
            ));
        }
    }

    require_code(
        snapshot,
        PROXY_MODE_KEY,
        &["disabled", "on_demand", "prefer"],
        "validate_proxy",
    )?;
    require_code(
        snapshot,
        PROXY_QUALITY_KEY,
        &["eighth", "quarter", "half", "full"],
        "validate_proxy",
    )?;

    match required_text(snapshot, RENDER_FRAME_RATE_MODE_KEY)? {
        "timeline" => {
            reject_present(snapshot, RENDER_RATE_NUMERATOR_KEY, "validate_render")?;
            reject_present(snapshot, RENDER_RATE_DENOMINATOR_KEY, "validate_render")?;
        }
        "explicit" => {
            let _ = superi_core::time::FrameRate::new(
                required_u32(snapshot, RENDER_RATE_NUMERATOR_KEY)?,
                required_u32(snapshot, RENDER_RATE_DENOMINATOR_KEY)?,
            )?;
        }
        _ => {
            return Err(invalid(
                "validate_render",
                "project render frame-rate mode is not supported",
            ));
        }
    }
    match required_text(snapshot, RENDER_EXTENT_MODE_KEY)? {
        "source" => {
            reject_present(snapshot, RENDER_WIDTH_KEY, "validate_render")?;
            reject_present(snapshot, RENDER_HEIGHT_KEY, "validate_render")?;
        }
        "explicit" => {
            let _ = required_u32(snapshot, RENDER_WIDTH_KEY)?;
            let _ = required_u32(snapshot, RENDER_HEIGHT_KEY)?;
        }
        _ => {
            return Err(invalid(
                "validate_render",
                "project render extent mode is not supported",
            ));
        }
    }
    require_code(
        snapshot,
        RENDER_PIXEL_FORMAT_KEY,
        &["rgba16_float", "rgba32_float"],
        "validate_render",
    )?;
    require_code(
        snapshot,
        RENDER_ALPHA_MODE_KEY,
        &["opaque", "straight", "premultiplied"],
        "validate_render",
    )?;
    require_code(
        snapshot,
        RENDER_COLOR_TARGET_KEY,
        &["project_working", "display", "delivery"],
        "validate_render",
    )?;
    Ok(())
}

fn required_value<'a>(snapshot: &'a SettingsSnapshot, key: &str) -> Result<&'a SettingValue> {
    let key_value = parse_key(key)?;
    snapshot.get(&key_value).ok_or_else(|| {
        invalid(
            "validate_required_key",
            "project settings snapshot is missing a required key",
        )
        .with_context(context("validate_required_key").with_field("key", key))
    })
}

fn required_text<'a>(snapshot: &'a SettingsSnapshot, key: &str) -> Result<&'a str> {
    required_value(snapshot, key)?.as_text().ok_or_else(|| {
        invalid(
            "validate_value_kind",
            "project setting requires a text value",
        )
        .with_context(context("validate_value_kind").with_field("key", key))
    })
}

fn required_positive(snapshot: &SettingsSnapshot, key: &str) -> Result<i64> {
    let value = required_value(snapshot, key)?.as_integer().ok_or_else(|| {
        invalid(
            "validate_value_kind",
            "project setting requires an integer value",
        )
        .with_context(context("validate_value_kind").with_field("key", key))
    })?;
    if value <= 0 {
        return Err(invalid(
            "validate_integer",
            "project setting integer must be greater than zero",
        )
        .with_context(context("validate_integer").with_field("key", key)));
    }
    Ok(value)
}

fn required_u32(snapshot: &SettingsSnapshot, key: &str) -> Result<u32> {
    u32::try_from(required_positive(snapshot, key)?).map_err(|_| {
        invalid(
            "validate_integer",
            "project setting integer exceeds the supported 32-bit range",
        )
        .with_context(context("validate_integer").with_field("key", key))
    })
}

fn require_code(
    snapshot: &SettingsSnapshot,
    key: &str,
    supported: &[&str],
    operation: &'static str,
) -> Result<()> {
    let value = required_text(snapshot, key)?;
    if supported.contains(&value) {
        return Ok(());
    }
    Err(
        invalid(operation, "project setting code is not supported").with_context(
            context(operation)
                .with_field("key", key)
                .with_field("value", value),
        ),
    )
}

fn reject_present(snapshot: &SettingsSnapshot, key: &str, operation: &'static str) -> Result<()> {
    let key_value = parse_key(key)?;
    if snapshot.get(&key_value).is_some() {
        return Err(invalid(
            operation,
            "project setting is forbidden by the selected mode",
        )
        .with_context(context(operation).with_field("key", key)));
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &'static str) -> Result<()> {
    let bytes = value.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= MAX_PROJECT_SETTING_IDENTIFIER_BYTES
        && bytes[0].is_ascii_lowercase()
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.')
        });
    if valid {
        return Ok(());
    }
    Err(invalid(
        "validate_identifier",
        "project setting identifier is not canonical lowercase ASCII",
    )
    .with_context(context("validate_identifier").with_field("field", field)))
}

fn validate_fingerprint(value: &str) -> Result<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(invalid(
        "validate_color_fingerprint",
        "project color config fingerprint must be 64 lowercase hexadecimal bytes",
    ))
}

fn parse_key(value: &str) -> Result<SettingKey> {
    SettingKey::new(value).map_err(|_| {
        invalid(
            "parse_setting_key",
            "project setting key is not a canonical namespaced identifier",
        )
    })
}

fn integer(key: &str, value: i64) -> Result<(SettingKey, SettingValue)> {
    Ok((parse_key(key)?, SettingValue::Integer(value)))
}

fn text(key: &str, value: &str) -> Result<(SettingKey, SettingValue)> {
    Ok((parse_key(key)?, SettingValue::Text(value.to_owned())))
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
