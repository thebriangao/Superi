//! Stable identifiers and schema versions for the public API.

use superi_core::settings::SemanticVersion;

/// Schema version for media capability snapshots.
pub const MEDIA_CAPABILITIES_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(2, 0, 0);

/// JSON-RPC method for querying the current media capability snapshot.
pub const GET_MEDIA_CAPABILITIES_METHOD: &str = "superi.media.capabilities.get";

/// Ordered event name for media capability snapshot changes.
pub const MEDIA_CAPABILITIES_CHANGED_EVENT: &str = "superi.media.capabilities.changed";

/// Schema version for complete engine capability and health snapshots.
pub const ENGINE_INTROSPECTION_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for querying complete engine capability and health state.
pub const GET_ENGINE_INTROSPECTION_METHOD: &str = "superi.engine.introspection.get";

/// Ordered event name for complete engine introspection state changes.
pub const ENGINE_INTROSPECTION_CHANGED_EVENT: &str = "superi.engine.introspection.changed";

/// Schema version for reference slice scenarios and state snapshots.
pub const SLICE_SCENARIO_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for executing one reference slice scenario action.
pub const EXECUTE_SCENARIO_ACTION_METHOD: &str = "superi.slice.scenario.action.execute";

/// JSON-RPC method for executing one atomic reference slice transaction.
pub const EXECUTE_SCENARIO_TRANSACTION_METHOD: &str = "superi.slice.scenario.transaction.execute";

/// Ordered event name for complete scenario state changes.
pub const SCENARIO_STATE_CHANGED_EVENT: &str = "superi.slice.scenario.state.changed";
