//! Fail-closed migration of the two authored stores previously owned by the retired host.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const LEGACY_FILES: [&str; 2] = ["project-session.json", "media-library-views.json"];

/// Overall migration result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationOutcome {
    Migrated,
    AlreadyCurrent,
    NoLegacyState,
}

/// One byte-preserving migrated file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigratedFile {
    name: String,
    bytes: u64,
    sha256: String,
    source: PathBuf,
    destination: PathBuf,
}

/// Inspectable report for migration and repeated startup.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationReport {
    outcome: MigrationOutcome,
    files: Vec<MigratedFile>,
}

impl MigrationReport {
    /// Returns overall outcome.
    #[must_use]
    pub const fn outcome(&self) -> MigrationOutcome {
        self.outcome
    }

    /// Returns every discovered file in stable migration order.
    #[must_use]
    pub fn files(&self) -> &[MigratedFile] {
        &self.files
    }
}

/// Source-preserving migrator from the legacy host storage root.
#[derive(Clone, Debug)]
pub struct LegacySessionMigrator {
    legacy_root: PathBuf,
    session_root: PathBuf,
}

impl LegacySessionMigrator {
    /// Creates an explicit source and destination migration.
    #[must_use]
    pub fn new(legacy_root: impl Into<PathBuf>, session_root: impl Into<PathBuf>) -> Self {
        Self {
            legacy_root: legacy_root.into(),
            session_root: session_root.into(),
        }
    }

    /// Validates every source and destination before publishing any new file.
    pub fn migrate(&self) -> Result<MigrationReport, MigrationError> {
        let mut prepared = Vec::new();
        for name in LEGACY_FILES {
            let source = self.legacy_root.join(name);
            if !source.exists() {
                continue;
            }
            let bytes = fs::read(&source).map_err(|error| MigrationError::Io {
                operation: "read legacy source",
                path: source.clone(),
                source: error,
            })?;
            serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|error| {
                MigrationError::InvalidJson {
                    path: source.clone(),
                    detail: error.to_string(),
                }
            })?;
            let destination = self.session_root.join(name);
            let digest = sha256(&bytes);
            let destination_current = if destination.exists() {
                let existing = fs::read(&destination).map_err(|error| MigrationError::Io {
                    operation: "read migration destination",
                    path: destination.clone(),
                    source: error,
                })?;
                if existing != bytes {
                    return Err(MigrationError::Conflict {
                        source,
                        destination,
                        source_sha256: digest,
                        destination_sha256: sha256(&existing),
                    });
                }
                true
            } else {
                false
            };
            prepared.push(PreparedFile {
                report: MigratedFile {
                    name: name.to_owned(),
                    bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                    sha256: digest,
                    source,
                    destination,
                },
                bytes,
                destination_current,
            });
        }
        if prepared.is_empty() {
            return Ok(MigrationReport {
                outcome: MigrationOutcome::NoLegacyState,
                files: Vec::new(),
            });
        }
        if prepared.iter().all(|file| file.destination_current) {
            return Ok(MigrationReport {
                outcome: MigrationOutcome::AlreadyCurrent,
                files: prepared.into_iter().map(|file| file.report).collect(),
            });
        }

        fs::create_dir_all(&self.session_root).map_err(|error| MigrationError::Io {
            operation: "create portable session root",
            path: self.session_root.clone(),
            source: error,
        })?;
        for file in &prepared {
            if file.destination_current {
                continue;
            }
            publish_new_file(&file.report.destination, &file.bytes, &file.report.sha256)?;
        }
        for file in &prepared {
            let published =
                fs::read(&file.report.destination).map_err(|error| MigrationError::Io {
                    operation: "verify migrated destination",
                    path: file.report.destination.clone(),
                    source: error,
                })?;
            if published != file.bytes {
                return Err(MigrationError::Verification {
                    path: file.report.destination.clone(),
                    expected_sha256: file.report.sha256.clone(),
                    actual_sha256: sha256(&published),
                });
            }
        }
        Ok(MigrationReport {
            outcome: MigrationOutcome::Migrated,
            files: prepared.into_iter().map(|file| file.report).collect(),
        })
    }
}

struct PreparedFile {
    report: MigratedFile,
    bytes: Vec<u8>,
    destination_current: bool,
}

fn publish_new_file(destination: &Path, bytes: &[u8], digest: &str) -> Result<(), MigrationError> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session-state");
    let temporary = destination.with_file_name(format!(".{name}.{digest}.migrating"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| MigrationError::Io {
            operation: "create migration temporary",
            path: temporary.clone(),
            source: error,
        })?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(MigrationError::Io {
            operation: "write migration temporary",
            path: temporary,
            source: error,
        });
    }
    drop(file);
    if let Err(error) = fs::hard_link(&temporary, destination) {
        let _ = fs::remove_file(&temporary);
        return Err(MigrationError::Io {
            operation: "publish migration destination",
            path: destination.to_path_buf(),
            source: error,
        });
    }
    fs::remove_file(&temporary).map_err(|error| MigrationError::Io {
        operation: "remove migration temporary",
        path: temporary,
        source: error,
    })
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Exact fail-closed migration error.
#[derive(Debug)]
pub enum MigrationError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    InvalidJson {
        path: PathBuf,
        detail: String,
    },
    Conflict {
        source: PathBuf,
        destination: PathBuf,
        source_sha256: String,
        destination_sha256: String,
    },
    Verification {
        path: PathBuf,
        expected_sha256: String,
        actual_sha256: String,
    },
}

impl fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(formatter, "{operation} failed for {}: {source}", path.display()),
            Self::InvalidJson { path, detail } => {
                write!(formatter, "legacy state {} contains invalid JSON: {detail}", path.display())
            }
            Self::Conflict {
                source,
                destination,
                source_sha256,
                destination_sha256,
            } => write!(
                formatter,
                "legacy state {} conflicts with {}: source {source_sha256}, destination {destination_sha256}",
                source.display(),
                destination.display()
            ),
            Self::Verification {
                path,
                expected_sha256,
                actual_sha256,
            } => write!(
                formatter,
                "migrated state {} failed verification: expected {expected_sha256}, actual {actual_sha256}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for MigrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
