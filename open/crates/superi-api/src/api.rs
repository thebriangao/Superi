//! Transport-neutral public media capability API.
//!
//! UI, scripting, extensions, CLI clients, and automation execute the same
//! typed query and consume the same full replacement event. Bulk media never
//! crosses this boundary.

use serde::{Deserialize, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::SemanticVersion;
use superi_engine::introspection::{
    MediaBackendTier as EngineMediaBackendTier, MediaCapabilities as EngineMediaCapabilities,
    MediaOperation as EngineMediaOperation,
};

use crate::commands::{GetMediaCapabilities, GetMediaCapabilitiesResult};
use crate::events::MediaCapabilitiesChanged;
use crate::version::MEDIA_CAPABILITIES_SCHEMA_VERSION;

/// The stable public policy tier for one media backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MediaBackendTier {
    /// A normal backend available without fallback permission.
    Primary,
    /// A backend available only under explicit fallback policy.
    Fallback,
}

/// One stable media action exposed to API consumers.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum MediaOperation {
    /// Open a media source for ingest, seeking, playback, or relinking.
    Source,
    /// Decode packets for a named codec.
    Decode {
        /// Stable codec identifier.
        codec: String,
    },
    /// Encode frames or audio blocks for a named codec.
    Encode {
        /// Stable codec identifier.
        codec: String,
    },
}

/// Public capability declaration for one registered backend.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaBackendCapabilities {
    id: String,
    display_name: String,
    priority: u16,
    tier: MediaBackendTier,
    capabilities: Vec<MediaOperation>,
}

impl MediaBackendCapabilities {
    /// Returns stable backend identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the diagnostic display name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Returns the selection priority. Larger values rank first.
    #[must_use]
    pub const fn priority(&self) -> u16 {
        self.priority
    }

    /// Returns the backend's primary or fallback policy tier.
    #[must_use]
    pub const fn tier(&self) -> MediaBackendTier {
        self.tier
    }

    /// Returns declared actions in stable order.
    #[must_use]
    pub fn capabilities(&self) -> &[MediaOperation] {
        &self.capabilities
    }
}

/// Effective ranked backend support for one media action.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaOperationSupport {
    operation: MediaOperation,
    primary_backends: Vec<String>,
    fallback_backends: Vec<String>,
}

impl MediaOperationSupport {
    /// Returns the reported media action.
    #[must_use]
    pub const fn operation(&self) -> &MediaOperation {
        &self.operation
    }

    /// Returns normal candidates in actual selection order.
    #[must_use]
    pub fn primary_backends(&self) -> &[String] {
        &self.primary_backends
    }

    /// Returns candidates requiring explicit fallback permission.
    #[must_use]
    pub fn fallback_backends(&self) -> &[String] {
        &self.fallback_backends
    }
}

/// Strict, versioned, point-in-time media capability state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaCapabilitiesSnapshot {
    schema_version: SemanticVersion,
    revision: u64,
    backends: Vec<MediaBackendCapabilities>,
    operations: Vec<MediaOperationSupport>,
}

impl MediaCapabilitiesSnapshot {
    /// Returns the schema that defines this payload.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns the monotonic API-local state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns backend declarations in stable identifier order.
    #[must_use]
    pub fn backends(&self) -> &[MediaBackendCapabilities] {
        &self.backends
    }

    /// Returns supported actions in stable source, decode, and encode order.
    #[must_use]
    pub fn operations(&self) -> &[MediaOperationSupport] {
        &self.operations
    }

    fn same_state(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version
            && self.backends == other.backends
            && self.operations == other.operations
    }
}

/// API state that serves media capability commands and produces change events.
#[derive(Clone, Debug)]
pub struct MediaCapabilitiesApi {
    snapshot: MediaCapabilitiesSnapshot,
}

impl MediaCapabilitiesApi {
    /// Creates API state from an engine capability snapshot.
    #[must_use]
    pub fn new(engine: &EngineMediaCapabilities) -> Self {
        Self {
            snapshot: public_snapshot(engine, 0),
        }
    }

    /// Executes the same typed query used by UI and automation clients.
    #[must_use]
    pub fn execute(&self, _command: GetMediaCapabilities) -> GetMediaCapabilitiesResult {
        GetMediaCapabilitiesResult::new(self.snapshot.clone())
    }

    /// Synchronizes engine state and emits one full replacement event on change.
    ///
    /// # Errors
    ///
    /// Returns a terminal error if the monotonic revision space is exhausted.
    pub fn synchronize(
        &mut self,
        engine: &EngineMediaCapabilities,
    ) -> Result<Option<MediaCapabilitiesChanged>> {
        let mut next = public_snapshot(engine, self.snapshot.revision);
        if self.snapshot.same_state(&next) {
            return Ok(None);
        }

        next.revision = self.snapshot.revision.checked_add(1).ok_or_else(|| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "media capability revision is exhausted",
            )
            .with_context(ErrorContext::new(
                "superi-api.media_capabilities",
                "synchronize",
            ))
        })?;
        self.snapshot = next.clone();
        Ok(Some(MediaCapabilitiesChanged::new(next)))
    }
}

fn public_snapshot(engine: &EngineMediaCapabilities, revision: u64) -> MediaCapabilitiesSnapshot {
    MediaCapabilitiesSnapshot {
        schema_version: MEDIA_CAPABILITIES_SCHEMA_VERSION.clone(),
        revision,
        backends: engine
            .backends()
            .iter()
            .map(|backend| MediaBackendCapabilities {
                id: backend.id().to_owned(),
                display_name: backend.display_name().to_owned(),
                priority: backend.priority(),
                tier: public_tier(backend.tier()),
                capabilities: backend
                    .capabilities()
                    .iter()
                    .map(public_operation)
                    .collect(),
            })
            .collect(),
        operations: engine
            .operations()
            .iter()
            .map(|support| MediaOperationSupport {
                operation: public_operation(support.operation()),
                primary_backends: support.primary_backends().to_vec(),
                fallback_backends: support.fallback_backends().to_vec(),
            })
            .collect(),
    }
}

fn public_tier(tier: EngineMediaBackendTier) -> MediaBackendTier {
    match tier {
        EngineMediaBackendTier::Primary => MediaBackendTier::Primary,
        EngineMediaBackendTier::Fallback => MediaBackendTier::Fallback,
    }
}

fn public_operation(operation: &EngineMediaOperation) -> MediaOperation {
    match operation {
        EngineMediaOperation::Source => MediaOperation::Source,
        EngineMediaOperation::Decode { codec } => MediaOperation::Decode {
            codec: codec.clone(),
        },
        EngineMediaOperation::Encode { codec } => MediaOperation::Encode {
            codec: codec.clone(),
        },
    }
}
