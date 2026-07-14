//! Stable identifiers and schema versions for the public API.

use superi_core::settings::SemanticVersion;

/// Schema version for media capability snapshots.
pub const MEDIA_CAPABILITIES_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(2, 0, 0);

/// JSON-RPC method for querying the current media capability snapshot.
pub const GET_MEDIA_CAPABILITIES_METHOD: &str = "superi.media.capabilities.get";

/// Ordered event name for media capability snapshot changes.
pub const MEDIA_CAPABILITIES_CHANGED_EVENT: &str = "superi.media.capabilities.changed";

/// Schema version for reference slice scenarios and state snapshots.
pub const SLICE_SCENARIO_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for executing one reference slice scenario action.
pub const EXECUTE_SCENARIO_ACTION_METHOD: &str = "superi.slice.scenario.action.execute";
