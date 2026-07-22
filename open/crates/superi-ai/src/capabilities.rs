//! Honest process-local capability discovery for the open AI boundary.
//!
//! This module reports implemented runtime availability only. Architectural requirements are
//! exposed separately so callers cannot mistake a required offline, editable-artifact boundary
//! for an executable pipeline.

/// Schema revision for [`AiCapabilities`].
pub const AI_CAPABILITY_SCHEMA_VERSION: u32 = 1;

/// Availability of the process-local inference runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AiRuntimeAvailability {
    /// No audited runtime or bundled model loader is implemented.
    Unavailable,
}

/// One executable local pipeline advertised by the current process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiPipelineCapability {
    id: String,
}

impl AiPipelineCapability {
    /// Returns the stable pipeline identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Immutable discovery state for the open AI boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiCapabilities {
    schema_version: u32,
    runtime: AiRuntimeAvailability,
    local_only: bool,
    requires_editable_artifacts: bool,
    available_pipelines: Vec<AiPipelineCapability>,
}

impl AiCapabilities {
    /// Returns the schema revision for this payload.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns actual runtime availability.
    #[must_use]
    pub const fn runtime(&self) -> AiRuntimeAvailability {
        self.runtime
    }

    /// Returns whether every future runtime path must remain local and offline.
    #[must_use]
    pub const fn local_only(&self) -> bool {
        self.local_only
    }

    /// Returns whether produced state must remain an ordinary editable artifact.
    #[must_use]
    pub const fn requires_editable_artifacts(&self) -> bool {
        self.requires_editable_artifacts
    }

    /// Returns executable pipelines available in this process.
    #[must_use]
    pub fn available_pipelines(&self) -> &[AiPipelineCapability] {
        &self.available_pipelines
    }
}

/// Discovers the currently implemented local AI surface without loading models or starting work.
#[must_use]
pub fn discover_local_capabilities() -> AiCapabilities {
    AiCapabilities {
        schema_version: AI_CAPABILITY_SCHEMA_VERSION,
        runtime: AiRuntimeAvailability::Unavailable,
        local_only: true,
        requires_editable_artifacts: true,
        available_pipelines: Vec::new(),
    }
}
