//! Deterministic generation and drift checks for committed public API bindings.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Canonical path beneath the open workspace root.
pub const CANONICAL_BINDINGS_PATH: &str = "bindings/typescript/superi-api.ts";

/// Result of comparing one file with freshly rendered bindings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckStatus {
    /// The file exactly matches fresh deterministic output.
    Current,
    /// The file does not exist.
    Missing,
    /// The file exists but differs from fresh deterministic output.
    Stale,
}

/// Result of publishing generated bindings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerateStatus {
    /// The existing file already matched and was not rewritten.
    Unchanged,
    /// Fresh deterministic output was written.
    Written,
}

/// Generator or filesystem failure.
#[derive(Debug)]
pub struct BindingToolError {
    message: String,
}

impl BindingToolError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BindingToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BindingToolError {}

/// Resolves the committed artifact independently of the caller's working directory.
#[must_use]
pub fn canonical_bindings_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(CANONICAL_BINDINGS_PATH)
}

/// Renders the complete bindings without filesystem access.
pub fn render() -> Result<String, BindingToolError> {
    superi_api::typescript::render_typescript_bindings()
        .map_err(|error| BindingToolError::new(error.to_string()))
}

/// Checks one path without creating, replacing, or otherwise mutating it.
pub fn check_path(path: &Path) -> Result<CheckStatus, BindingToolError> {
    let expected = render()?;
    match fs::read(path) {
        Ok(actual) if actual == expected.as_bytes() => Ok(CheckStatus::Current),
        Ok(_) => Ok(CheckStatus::Stale),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(CheckStatus::Missing),
        Err(error) => Err(BindingToolError::new(format!(
            "failed to read {}: {error}",
            path.display()
        ))),
    }
}

/// Publishes fresh deterministic output, avoiding a rewrite when bytes are current.
pub fn generate_path(path: &Path) -> Result<GenerateStatus, BindingToolError> {
    let expected = render()?;
    match fs::read(path) {
        Ok(actual) if actual == expected.as_bytes() => return Ok(GenerateStatus::Unchanged),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(BindingToolError::new(format!(
                "failed to read {}: {error}",
                path.display()
            )))
        }
    }

    let parent = path.parent().ok_or_else(|| {
        BindingToolError::new(format!("{} has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        BindingToolError::new(format!("failed to create {}: {error}", parent.display()))
    })?;
    fs::write(path, expected).map_err(|error| {
        BindingToolError::new(format!("failed to write {}: {error}", path.display()))
    })?;
    Ok(GenerateStatus::Written)
}
