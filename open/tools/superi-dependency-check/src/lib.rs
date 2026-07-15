//! Automated dependency-direction checks for the open workspace.

use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

/// Summary produced after a dependency graph passes validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckReport {
    /// Runtime crates covered by the architecture policy.
    pub checked_packages: usize,
    /// Internal dependency declarations covered by the check.
    pub checked_internal_edges: usize,
}

/// A metadata, policy, or Cargo invocation failure.
#[derive(Debug)]
pub enum CheckError {
    Metadata(String),
    Cargo(String),
    Violations(Vec<String>),
}

impl fmt::Display for CheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metadata(message) | Self::Cargo(message) => formatter.write_str(message),
            Self::Violations(violations) => {
                writeln!(formatter, "open-tree dependency direction check failed:")?;
                for violation in violations {
                    writeln!(formatter, "- {violation}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for CheckError {}

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    manifest_path: PathBuf,
    dependencies: Vec<Dependency>,
}

#[derive(Debug, Deserialize)]
struct Dependency {
    name: String,
    kind: Option<String>,
    path: Option<PathBuf>,
}

/// Runs Cargo's stable metadata command without resolving or fetching dependencies.
pub fn check_workspace(workspace: &Path) -> Result<CheckReport, CheckError> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(workspace)
        .args([
            "metadata",
            "--manifest-path",
            "Cargo.toml",
            "--no-deps",
            "--format-version",
            "1",
            "--locked",
            "--offline",
        ])
        .output()
        .map_err(|error| CheckError::Cargo(format!("failed to execute cargo metadata: {error}")))?;

    if !output.status.success() {
        return Err(CheckError::Cargo(format!(
            "cargo metadata failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let metadata = String::from_utf8(output.stdout)
        .map_err(|error| CheckError::Metadata(format!("cargo metadata was not UTF-8: {error}")))?;
    validate_metadata(&metadata)
}

/// Validates Cargo metadata against the reviewed open runtime dependency policy.
pub fn validate_metadata(metadata: &str) -> Result<CheckReport, CheckError> {
    let metadata: Metadata = serde_json::from_str(metadata)
        .map_err(|error| CheckError::Metadata(format!("invalid cargo metadata: {error}")))?;
    let package_names: HashSet<&str> = metadata
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut violations = BTreeSet::new();
    let mut checked_packages = 0;
    let mut checked_internal_edges = 0;

    for package in &metadata.packages {
        let is_runtime_crate = package
            .manifest_path
            .components()
            .any(|component| component.as_os_str() == "crates");
        if !is_runtime_crate {
            continue;
        }

        checked_packages += 1;
        let Some(policy) = policy_for(&package.name) else {
            violations.insert(format!("unknown runtime crate {}", package.name));
            continue;
        };

        for dependency in &package.dependencies {
            if dependency.path.is_none() || !package_names.contains(dependency.name.as_str()) {
                continue;
            }

            checked_internal_edges += 1;
            let kind = dependency.kind.as_deref().unwrap_or("normal");
            let allowed = if kind == "dev" {
                policy.dev.contains(&dependency.name.as_str())
            } else {
                policy.runtime.contains(&dependency.name.as_str())
            };

            if !allowed {
                violations.insert(format!(
                    "unauthorized {kind} dependency {} -> {}",
                    package.name, dependency.name
                ));
            }
        }
    }

    if violations.is_empty() {
        Ok(CheckReport {
            checked_packages,
            checked_internal_edges,
        })
    } else {
        Err(CheckError::Violations(violations.into_iter().collect()))
    }
}

struct Policy {
    runtime: &'static [&'static str],
    dev: &'static [&'static str],
}

fn policy_for(package: &str) -> Option<Policy> {
    let policy = match package {
        "superi-core" => Policy::new(&[], &[]),
        "superi-image" => Policy::new(&["superi-core"], &[]),
        "superi-gpu" => Policy::new(&["superi-core", "superi-image"], &[]),
        "superi-concurrency" => Policy::new(&["superi-core", "superi-gpu"], &["superi-media-io"]),
        "superi-media-io" => Policy::new(&["superi-core", "superi-image"], &[]),
        "superi-codecs-rs" | "superi-codecs-platform" | "superi-codecs-vendor" => {
            Policy::new(&["superi-core", "superi-image", "superi-media-io"], &[])
        }
        "superi-graph" => Policy::new(
            &[
                "superi-core",
                "superi-gpu",
                "superi-image",
                "superi-concurrency",
            ],
            &[],
        ),
        "superi-cache" => Policy::new(
            &[
                "superi-core",
                "superi-gpu",
                "superi-image",
                "superi-graph",
                "superi-concurrency",
            ],
            &[],
        ),
        "superi-color" | "superi-effects" => Policy::new(
            &["superi-core", "superi-gpu", "superi-image", "superi-graph"],
            &[],
        ),
        "superi-timeline" => Policy::new(&["superi-core", "superi-graph"], &[]),
        "superi-audio" => Policy::new(&["superi-core", "superi-concurrency"], &[]),
        "superi-ai" => Policy::new(&["superi-core", "superi-image", "superi-graph"], &[]),
        "superi-project" => Policy::new(&["superi-core", "superi-graph", "superi-timeline"], &[]),
        "superi-engine" => Policy::new(
            &[
                "superi-core",
                "superi-image",
                "superi-gpu",
                "superi-concurrency",
                "superi-media-io",
                "superi-codecs-rs",
                "superi-graph",
                "superi-cache",
                "superi-color",
                "superi-effects",
                "superi-timeline",
                "superi-audio",
                "superi-ai",
                "superi-project",
                "superi-codecs-platform",
                "superi-codecs-vendor",
            ],
            &[],
        ),
        "superi-api" => Policy::new(&["superi-core", "superi-engine"], &["superi-media-io"]),
        "superi-cli" => Policy::new(&["superi-core", "superi-api"], &[]),
        _ => return None,
    };
    Some(policy)
}

impl Policy {
    const fn new(runtime: &'static [&'static str], dev: &'static [&'static str]) -> Self {
        Self { runtime, dev }
    }
}
