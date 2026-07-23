#![forbid(unsafe_code)]

//! Portable application and session services preserved outside any presentation host.
//!
//! These modules retain the substantive lifecycle, project, media, crash, capability,
//! engine-link, and public-transport behavior that previously lived behind host wrappers.
//! Canonical authored project and editorial state remain below this crate.

pub mod capabilities;
pub mod crash_diagnostics;
pub mod engine;
pub mod lifecycle;
pub mod migration;
pub mod platform_adapters;
pub mod process_runtime;
pub mod project_lifecycle;
pub mod transport;

use std::path::{Path, PathBuf};

use migration::{LegacySessionMigrator, MigrationReport};
use project_lifecycle::{DesktopProjectFailure, DesktopProjectState};

/// Portable owners initialized for one native or headless application session.
#[derive(Clone)]
pub struct SessionOwners {
    root: PathBuf,
    projects: DesktopProjectState,
}

impl SessionOwners {
    /// Migrates any legacy shell-local state, then initializes portable project services.
    pub fn initialize(
        legacy_root: Option<&Path>,
        session_root: impl Into<PathBuf>,
    ) -> Result<(Self, Option<MigrationReport>), SessionInitializationError> {
        let session_root = session_root.into();
        let migration = legacy_root
            .map(|legacy| LegacySessionMigrator::new(legacy, &session_root).migrate())
            .transpose()?;
        let projects = DesktopProjectState::default();
        projects.initialize(session_root.clone())?;
        Ok((
            Self {
                root: session_root,
                projects,
            },
            migration,
        ))
    }

    /// Returns the portable session storage root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the retained project and media service owner.
    #[must_use]
    pub const fn projects(&self) -> &DesktopProjectState {
        &self.projects
    }
}

/// Initialization failure that never hides whether migration or service startup failed.
#[derive(Debug, thiserror::Error)]
pub enum SessionInitializationError {
    #[error(transparent)]
    Migration(#[from] migration::MigrationError),
    #[error(transparent)]
    Project(#[from] DesktopProjectFailure),
}
