//! Stable identifiers and schema versions for the public API.

use superi_core::settings::SemanticVersion;

/// Schema version for the complete public API catalog.
pub const PUBLIC_API_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 9, 0);

/// Every released public API catalog schema in ascending SemVer precedence order.
pub const PUBLIC_API_SCHEMA_RELEASES: &[SemanticVersion] = &[
    SemanticVersion::new(1, 0, 0),
    SemanticVersion::new(1, 1, 0),
    SemanticVersion::new(1, 2, 0),
    SemanticVersion::new(1, 3, 0),
    SemanticVersion::new(1, 4, 0),
    SemanticVersion::new(1, 5, 0),
    SemanticVersion::new(1, 6, 0),
    SemanticVersion::new(1, 7, 0),
    SemanticVersion::new(1, 8, 0),
    SemanticVersion::new(1, 9, 0),
];

/// Independent request and response schema for version negotiation.
pub const VERSION_NEGOTIATION_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// Stateless JSON-RPC query for API and optional project version negotiation.
pub const NEGOTIATE_API_VERSION_METHOD: &str = "superi.api.version.negotiate";

/// Schema version for structured public API failures.
pub const PUBLIC_ERROR_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for retrieving the complete public API catalog.
pub const GET_PUBLIC_API_SCHEMA_METHOD: &str = "superi.api.schema.get";

/// Schema version for ordered bounded public event delivery.
pub const EVENT_STREAM_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC command for registering one independent event subscriber.
pub const OPEN_EVENT_SUBSCRIPTION_METHOD: &str = "superi.events.subscription.open";

/// JSON-RPC command for removing one independent event subscriber.
pub const CLOSE_EVENT_SUBSCRIPTION_METHOD: &str = "superi.events.subscription.close";

/// JSON-RPC query for non-destructive bounded event replay.
pub const POLL_EVENT_SUBSCRIPTION_METHOD: &str = "superi.events.subscription.poll";

/// Schema version for the generic authored project command surface.
pub const PROJECT_EDITOR_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 7, 0);

/// JSON-RPC method for one generic authored project command.
pub const EXECUTE_PROJECT_COMMAND_METHOD: &str = "superi.project.command.execute";

/// Schema version for durable project command-log inspection.
pub const PROJECT_COMMAND_LOG_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC query for bounded project command-log inspection.
pub const GET_PROJECT_COMMAND_LOG_METHOD: &str = "superi.project.command_log.get";

/// Replacement resource name for durable project command-log state.
pub const PROJECT_COMMAND_LOG_RESOURCE: &str = "superi.project.command_log";

/// Schema version for the bounded local scripting language and execution trace.
pub const SCRIPTING_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for validating and running exact local script source.
pub const RUN_PROJECT_SCRIPT_METHOD: &str = "superi.project.script.run";

/// Ordered event name for generic authored project state changes.
pub const PROJECT_STATE_CHANGED_EVENT: &str = "superi.project.state.changed";

/// Replacement resource name for generic project history state.
pub const PROJECT_HISTORY_RESOURCE: &str = "superi.project.history";

/// Schema version for extension registration and capability discovery.
pub const EXTENSIONS_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// Read-only JSON-RPC query for the complete process-lifetime extension registry.
pub const GET_EXTENSIONS_METHOD: &str = "superi.extensions.get";

/// Full replacement event for process-lifetime extension registry changes.
pub const EXTENSIONS_CHANGED_EVENT: &str = "superi.extensions.changed";

/// Complete process-lifetime extension replacement resource.
pub const EXTENSIONS_RESOURCE: &str = "superi.extensions";

/// Schema version for public asynchronous job replacement snapshots.
pub const ASYNC_JOBS_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for querying every retained asynchronous job.
pub const GET_ASYNC_JOBS_METHOD: &str = "superi.jobs.get";

/// JSON-RPC method for cooperatively pausing one asynchronous job.
pub const PAUSE_ASYNC_JOB_METHOD: &str = "superi.jobs.pause";

/// JSON-RPC method for resuming one fully paused asynchronous job.
pub const RESUME_ASYNC_JOB_METHOD: &str = "superi.jobs.resume";

/// JSON-RPC method for retrying one nonterminal failed asynchronous job.
pub const RETRY_ASYNC_JOB_METHOD: &str = "superi.jobs.retry";

/// JSON-RPC method for cooperatively cancelling one asynchronous job.
pub const CANCEL_ASYNC_JOB_METHOD: &str = "superi.jobs.cancel";

/// JSON-RPC method for cooperatively cancelling every unfinished asynchronous job.
pub const CANCEL_ALL_ASYNC_JOBS_METHOD: &str = "superi.jobs.cancel_all";

/// JSON-RPC method for removing one finalized asynchronous job.
pub const REMOVE_ASYNC_JOB_METHOD: &str = "superi.jobs.remove";

/// Ordered event name for complete asynchronous job replacement state.
pub const ASYNC_JOBS_CHANGED_EVENT: &str = "superi.jobs.changed";

/// Schema version for complete editor replacement state.
pub const EDITOR_STATE_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC method for querying complete editor replacement state.
pub const GET_EDITOR_STATE_METHOD: &str = "superi.editor.state.get";

/// Schema version for strict interactive playback transport control.
pub const PLAYBACK_TRANSPORT_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// JSON-RPC command for the authoritative interactive playback transport.
pub const EXECUTE_PLAYBACK_TRANSPORT_METHOD: &str = "superi.playback.transport.execute";

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
