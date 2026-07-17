//! Stable identifiers and schema versions for the public API.

use superi_core::settings::SemanticVersion;

/// Schema version for the complete public API catalog.
pub const PUBLIC_API_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// Schema version for structured public API failures.
pub const PUBLIC_ERROR_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for retrieving the complete public API catalog.
pub const GET_PUBLIC_API_SCHEMA_METHOD: &str = "superi.api.schema.get";

/// Schema version for the generic authored project command surface.
pub const PROJECT_EDITOR_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for one generic authored project command.
pub const EXECUTE_PROJECT_COMMAND_METHOD: &str = "superi.project.command.execute";

/// Ordered event name for generic authored project state changes.
pub const PROJECT_STATE_CHANGED_EVENT: &str = "superi.project.state.changed";

/// Replacement resource name for generic project history state.
pub const PROJECT_HISTORY_RESOURCE: &str = "superi.project.history";

/// Schema version for authored audio automation replacement snapshots.
pub const AUDIO_AUTOMATION_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for querying authored audio automation.
pub const GET_AUDIO_AUTOMATION_METHOD: &str = "superi.audio.automation.get";

/// JSON-RPC method for one atomic authored audio automation transaction.
pub const EXECUTE_AUDIO_AUTOMATION_TRANSACTION_METHOD: &str =
    "superi.audio.automation.transaction.execute";

/// Ordered event name for authored audio automation changes.
pub const AUDIO_AUTOMATION_CHANGED_EVENT: &str = "superi.audio.automation.changed";

/// Schema version for project crash recovery state and comparison payloads.
pub const PROJECT_RECOVERY_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for refreshing project crash recovery state.
pub const GET_PROJECT_RECOVERY_METHOD: &str = "superi.project.recovery.get";

/// JSON-RPC method for comparing one recovery candidate with current state.
pub const COMPARE_PROJECT_RECOVERY_METHOD: &str = "superi.project.recovery.compare";

/// JSON-RPC method for restoring one recovery candidate.
pub const RESTORE_PROJECT_RECOVERY_METHOD: &str = "superi.project.recovery.restore";

/// JSON-RPC method for durably dismissing one recovery candidate.
pub const DISMISS_PROJECT_RECOVERY_METHOD: &str = "superi.project.recovery.dismiss";

/// Ordered event name for complete project recovery replacement state.
pub const PROJECT_RECOVERY_CHANGED_EVENT: &str = "superi.project.recovery.changed";

/// Schema version for authoritative project settings replacement snapshots.
pub const PROJECT_SETTINGS_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for querying authoritative project settings.
pub const GET_PROJECT_SETTINGS_METHOD: &str = "superi.project.settings.get";

/// JSON-RPC method for one atomic project settings transaction.
pub const EXECUTE_PROJECT_SETTINGS_TRANSACTION_METHOD: &str =
    "superi.project.settings.transaction.execute";

/// Ordered event name for authoritative project settings changes.
pub const PROJECT_SETTINGS_CHANGED_EVENT: &str = "superi.project.settings.changed";

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

/// Schema version for coherent engine integration validation snapshots.
pub const ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION: SemanticVersion =
    SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for querying coherent engine integration state.
pub const GET_ENGINE_INTEGRATION_VALIDATION_METHOD: &str =
    "superi.engine.integration.validation.get";
