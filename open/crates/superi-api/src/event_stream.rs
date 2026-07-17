//! Bounded ordered delivery for the stable public event vocabulary.

use std::collections::{BTreeSet, VecDeque};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::SemanticVersion;

use crate::commands::ApiCommand;
use crate::events::{
    ApiEvent, AsyncJobsChanged, AudioAutomationChanged, EngineIntrospectionChanged,
    MediaCapabilitiesChanged, ProjectRecoveryChanged, ProjectSettingsChanged, ProjectStateChanged,
    ScenarioStateChanged,
};
use crate::permissions::{
    ApiPermissionKind, ApiPermissionRequirementMode, ApiPermissionRequirements,
};
use crate::schema::{ApiResource, PublicMethodKind};
use crate::version::{
    ASYNC_JOBS_SCHEMA_VERSION, AUDIO_AUTOMATION_SCHEMA_VERSION, CLOSE_EVENT_SUBSCRIPTION_METHOD,
    ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION, ENGINE_INTROSPECTION_SCHEMA_VERSION,
    EVENT_STREAM_SCHEMA_VERSION, EXECUTE_PROJECT_COMMAND_METHOD, EXECUTE_SCENARIO_ACTION_METHOD,
    GET_ASYNC_JOBS_METHOD, GET_AUDIO_AUTOMATION_METHOD, GET_EDITOR_STATE_METHOD,
    GET_ENGINE_INTEGRATION_VALIDATION_METHOD, GET_ENGINE_INTROSPECTION_METHOD,
    GET_MEDIA_CAPABILITIES_METHOD, GET_PROJECT_RECOVERY_METHOD, GET_PROJECT_SETTINGS_METHOD,
    MEDIA_CAPABILITIES_SCHEMA_VERSION, OPEN_EVENT_SUBSCRIPTION_METHOD,
    POLL_EVENT_SUBSCRIPTION_METHOD, PROJECT_EDITOR_SCHEMA_VERSION, PROJECT_HISTORY_RESOURCE,
    PROJECT_RECOVERY_SCHEMA_VERSION, PROJECT_SETTINGS_SCHEMA_VERSION,
    SLICE_SCENARIO_SCHEMA_VERSION,
};

const COMPONENT: &str = "superi-api.event-stream";
const MAX_EVENT_IDENTIFIER_BYTES: usize = 128;

/// Defensive upper bound for retained events, subscriptions, and one poll batch.
pub const MAX_EVENT_STREAM_BOUND: u32 = 4_096;

macro_rules! define_identifier {
    ($(#[$metadata:meta])* $name:ident, $operation:literal) => {
        $(#[$metadata])*
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates one validated stable identifier.
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                validate_identifier($operation, &value)?;
                Ok(Self(value))
            }

            /// Returns the canonical identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(D::Error::custom)
            }
        }
    };
}

define_identifier!(
    /// Identity for one process-lifetime ordered public event stream.
    EventStreamId,
    "validate_stream_id"
);
define_identifier!(
    /// Caller-owned identity for one independent event subscription.
    SubscriptionId,
    "validate_subscription_id"
);

/// Validated bounded storage and delivery limits for one event stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct EventStreamConfig {
    retained_events: u32,
    max_subscriptions: u32,
    max_batch_size: u32,
}

impl EventStreamConfig {
    /// Creates one configuration whose limits are all finite and nonzero.
    pub fn new(retained_events: u32, max_subscriptions: u32, max_batch_size: u32) -> Result<Self> {
        validate_bound("retained_events", retained_events)?;
        validate_bound("max_subscriptions", max_subscriptions)?;
        validate_bound("max_batch_size", max_batch_size)?;
        Ok(Self {
            retained_events,
            max_subscriptions,
            max_batch_size,
        })
    }

    /// Returns the maximum number of complete records retained for replay.
    #[must_use]
    pub const fn retained_events(self) -> u32 {
        self.retained_events
    }

    /// Returns the maximum number of registered subscriber identities.
    #[must_use]
    pub const fn max_subscriptions(self) -> u32 {
        self.max_subscriptions
    }

    /// Returns the server-side maximum records delivered by one poll.
    #[must_use]
    pub const fn max_batch_size(self) -> u32 {
        self.max_batch_size
    }
}

impl Default for EventStreamConfig {
    fn default() -> Self {
        Self {
            retained_events: 64,
            max_subscriptions: 64,
            max_batch_size: 64,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventStreamConfigWire {
    retained_events: u32,
    max_subscriptions: u32,
    max_batch_size: u32,
}

impl<'de> Deserialize<'de> for EventStreamConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EventStreamConfigWire::deserialize(deserializer)?;
        Self::new(
            wire.retained_events,
            wire.max_subscriptions,
            wire.max_batch_size,
        )
        .map_err(D::Error::custom)
    }
}

/// Nonzero sequence allocated by the public stream independently of engine events.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PublicEventSequence(u64);

impl PublicEventSequence {
    /// Creates one valid nonzero public event sequence.
    pub fn new(value: u64) -> Result<Self> {
        if value == 0 {
            return Err(stream_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_public_event_sequence",
                "public event sequence must be nonzero",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the numeric sequence value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for PublicEventSequence {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u64::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

/// Correlation retained independently from JSON-RPC request identifiers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicEventCorrelation {
    /// A state change produced by one successful typed command.
    Command {
        source_event_sequence: u64,
        command_sequence: u64,
        transaction_id: String,
    },
    /// A state change observed from a revisioned owner without a command transaction.
    Observation { revision: u64 },
}

/// One authoritative state resource and the typed method that refreshes it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResyncResource {
    resource: String,
    method: String,
    method_kind: PublicMethodKind,
    schema_version: SemanticVersion,
}

impl ResyncResource {
    fn new(
        resource: &'static str,
        method: &'static str,
        method_kind: PublicMethodKind,
        schema_version: SemanticVersion,
    ) -> Self {
        Self {
            resource: resource.to_owned(),
            method: method.to_owned(),
            method_kind,
            schema_version,
        }
    }

    /// Returns the permanent replacement resource identity.
    #[must_use]
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// Returns the stable typed method used to read the full replacement state.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns whether replacement state is refreshed through a query or typed inspect command.
    #[must_use]
    pub const fn method_kind(&self) -> PublicMethodKind {
        self.method_kind
    }

    /// Returns the replacement payload schema version.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }
}

/// Complete authoritative replacement manifest used after an explicit stream gap.
#[must_use]
pub fn replacement_resource_manifest() -> Vec<ResyncResource> {
    vec![
        ResyncResource::new(
            "superi.audio.automation",
            GET_AUDIO_AUTOMATION_METHOD,
            PublicMethodKind::Query,
            AUDIO_AUTOMATION_SCHEMA_VERSION,
        ),
        ResyncResource::new(
            "superi.editor.state",
            GET_EDITOR_STATE_METHOD,
            PublicMethodKind::Query,
            crate::version::EDITOR_STATE_SCHEMA_VERSION,
        ),
        ResyncResource::new(
            "superi.engine.integration.validation",
            GET_ENGINE_INTEGRATION_VALIDATION_METHOD,
            PublicMethodKind::Query,
            ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION,
        ),
        ResyncResource::new(
            "superi.engine.introspection",
            GET_ENGINE_INTROSPECTION_METHOD,
            PublicMethodKind::Query,
            ENGINE_INTROSPECTION_SCHEMA_VERSION,
        ),
        ResyncResource::new(
            "superi.jobs",
            GET_ASYNC_JOBS_METHOD,
            PublicMethodKind::Query,
            ASYNC_JOBS_SCHEMA_VERSION,
        ),
        ResyncResource::new(
            "superi.media.capabilities",
            GET_MEDIA_CAPABILITIES_METHOD,
            PublicMethodKind::Query,
            MEDIA_CAPABILITIES_SCHEMA_VERSION,
        ),
        ResyncResource::new(
            PROJECT_HISTORY_RESOURCE,
            EXECUTE_PROJECT_COMMAND_METHOD,
            PublicMethodKind::Command,
            PROJECT_EDITOR_SCHEMA_VERSION,
        ),
        ResyncResource::new(
            "superi.project.recovery",
            GET_PROJECT_RECOVERY_METHOD,
            PublicMethodKind::Query,
            PROJECT_RECOVERY_SCHEMA_VERSION,
        ),
        ResyncResource::new(
            "superi.project.settings",
            GET_PROJECT_SETTINGS_METHOD,
            PublicMethodKind::Query,
            PROJECT_SETTINGS_SCHEMA_VERSION,
        ),
        ResyncResource::new(
            "superi.slice.scenario.state",
            EXECUTE_SCENARIO_ACTION_METHOD,
            PublicMethodKind::Command,
            SLICE_SCENARIO_SCHEMA_VERSION,
        ),
    ]
}

/// The authoritative resource revision replaced by one delivered event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventReplacementResource {
    descriptor: ResyncResource,
    revision: u64,
    project_revision: Option<u64>,
}

impl EventReplacementResource {
    fn new(descriptor: ResyncResource, revision: u64, project_revision: Option<u64>) -> Self {
        Self {
            descriptor,
            revision,
            project_revision,
        }
    }

    /// Returns the resource and refresh method descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> &ResyncResource {
        &self.descriptor
    }

    /// Returns the primary authoritative resource revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns an additional project revision when the resource owns a separate catalog revision.
    #[must_use]
    pub const fn project_revision(&self) -> Option<u64> {
        self.project_revision
    }
}

/// Strict closed union of every event in the public schema catalog.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "event", content = "payload")]
#[non_exhaustive]
pub enum PublicApiEvent {
    #[serde(rename = "superi.project.state.changed")]
    ProjectStateChanged(Box<ProjectStateChanged>),
    #[serde(rename = "superi.jobs.changed")]
    AsyncJobsChanged(Box<AsyncJobsChanged>),
    #[serde(rename = "superi.project.recovery.changed")]
    ProjectRecoveryChanged(Box<ProjectRecoveryChanged>),
    #[serde(rename = "superi.audio.automation.changed")]
    AudioAutomationChanged(Box<AudioAutomationChanged>),
    #[serde(rename = "superi.project.settings.changed")]
    ProjectSettingsChanged(Box<ProjectSettingsChanged>),
    #[serde(rename = "superi.media.capabilities.changed")]
    MediaCapabilitiesChanged(Box<MediaCapabilitiesChanged>),
    #[serde(rename = "superi.engine.introspection.changed")]
    EngineIntrospectionChanged(Box<EngineIntrospectionChanged>),
    #[serde(rename = "superi.slice.scenario.state.changed")]
    ScenarioStateChanged(Box<ScenarioStateChanged>),
}

#[derive(Deserialize)]
#[serde(tag = "event", content = "payload", deny_unknown_fields)]
enum PublicApiEventWire {
    #[serde(rename = "superi.project.state.changed")]
    ProjectState(Box<ProjectStateChanged>),
    #[serde(rename = "superi.jobs.changed")]
    AsyncJobs(Box<AsyncJobsChanged>),
    #[serde(rename = "superi.project.recovery.changed")]
    ProjectRecovery(Box<ProjectRecoveryChanged>),
    #[serde(rename = "superi.audio.automation.changed")]
    AudioAutomation(Box<AudioAutomationChanged>),
    #[serde(rename = "superi.project.settings.changed")]
    ProjectSettings(Box<ProjectSettingsChanged>),
    #[serde(rename = "superi.media.capabilities.changed")]
    MediaCapabilities(Box<MediaCapabilitiesChanged>),
    #[serde(rename = "superi.engine.introspection.changed")]
    EngineIntrospection(Box<EngineIntrospectionChanged>),
    #[serde(rename = "superi.slice.scenario.state.changed")]
    ScenarioState(Box<ScenarioStateChanged>),
}

impl<'de> Deserialize<'de> for PublicApiEvent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let event = match PublicApiEventWire::deserialize(deserializer)? {
            PublicApiEventWire::ProjectState(value) => Self::ProjectStateChanged(value),
            PublicApiEventWire::AsyncJobs(value) => Self::AsyncJobsChanged(value),
            PublicApiEventWire::ProjectRecovery(value) => Self::ProjectRecoveryChanged(value),
            PublicApiEventWire::AudioAutomation(value) => Self::AudioAutomationChanged(value),
            PublicApiEventWire::ProjectSettings(value) => Self::ProjectSettingsChanged(value),
            PublicApiEventWire::MediaCapabilities(value) => Self::MediaCapabilitiesChanged(value),
            PublicApiEventWire::EngineIntrospection(value) => {
                Self::EngineIntrospectionChanged(value)
            }
            PublicApiEventWire::ScenarioState(value) => Self::ScenarioStateChanged(value),
        };
        event.validate().map_err(D::Error::custom)?;
        Ok(event)
    }
}

impl PublicApiEvent {
    /// Every permanent event name represented by this closed union.
    pub const NAMES: &'static [&'static str] = &[
        ProjectStateChanged::NAME,
        AsyncJobsChanged::NAME,
        ProjectRecoveryChanged::NAME,
        AudioAutomationChanged::NAME,
        ProjectSettingsChanged::NAME,
        MediaCapabilitiesChanged::NAME,
        EngineIntrospectionChanged::NAME,
        ScenarioStateChanged::NAME,
    ];

    /// Returns the permanent event name from the public catalog.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ProjectStateChanged(_) => ProjectStateChanged::NAME,
            Self::AsyncJobsChanged(_) => AsyncJobsChanged::NAME,
            Self::ProjectRecoveryChanged(_) => ProjectRecoveryChanged::NAME,
            Self::AudioAutomationChanged(_) => AudioAutomationChanged::NAME,
            Self::ProjectSettingsChanged(_) => ProjectSettingsChanged::NAME,
            Self::MediaCapabilitiesChanged(_) => MediaCapabilitiesChanged::NAME,
            Self::EngineIntrospectionChanged(_) => EngineIntrospectionChanged::NAME,
            Self::ScenarioStateChanged(_) => ScenarioStateChanged::NAME,
        }
    }

    /// Returns the exact payload schema version for this event variant.
    #[must_use]
    pub fn schema_version(&self) -> SemanticVersion {
        match self {
            Self::ProjectStateChanged(_) => ProjectStateChanged::SCHEMA_VERSION,
            Self::AsyncJobsChanged(_) => AsyncJobsChanged::SCHEMA_VERSION,
            Self::ProjectRecoveryChanged(_) => ProjectRecoveryChanged::SCHEMA_VERSION,
            Self::AudioAutomationChanged(_) => AudioAutomationChanged::SCHEMA_VERSION,
            Self::ProjectSettingsChanged(_) => ProjectSettingsChanged::SCHEMA_VERSION,
            Self::MediaCapabilitiesChanged(_) => MediaCapabilitiesChanged::SCHEMA_VERSION,
            Self::EngineIntrospectionChanged(_) => EngineIntrospectionChanged::SCHEMA_VERSION,
            Self::ScenarioStateChanged(_) => ScenarioStateChanged::SCHEMA_VERSION,
        }
    }

    /// Returns command or observation correlation carried by this event.
    #[must_use]
    pub fn correlation(&self) -> PublicEventCorrelation {
        match self {
            Self::ProjectStateChanged(value) => command_correlation(
                value.sequence(),
                value.command_sequence(),
                value.transaction_id(),
            ),
            Self::AsyncJobsChanged(value) => command_correlation(
                value.sequence(),
                value.command_sequence(),
                value.transaction_id(),
            ),
            Self::ProjectRecoveryChanged(value) => command_correlation(
                value.sequence(),
                value.command_sequence(),
                value.transaction_id(),
            ),
            Self::AudioAutomationChanged(value) => command_correlation(
                value.sequence(),
                value.command_sequence(),
                value.transaction_id(),
            ),
            Self::ProjectSettingsChanged(value) => command_correlation(
                value.sequence(),
                value.command_sequence(),
                value.transaction_id(),
            ),
            Self::MediaCapabilitiesChanged(value) => PublicEventCorrelation::Observation {
                revision: value.snapshot().revision(),
            },
            Self::EngineIntrospectionChanged(value) => PublicEventCorrelation::Observation {
                revision: value.snapshot().revision(),
            },
            Self::ScenarioStateChanged(value) => command_correlation(
                value.sequence(),
                value.command_sequence(),
                value.transaction_id(),
            ),
        }
    }

    /// Returns the full state resource replaced by this event.
    #[must_use]
    pub fn replacement_resource(&self) -> EventReplacementResource {
        match self {
            Self::ProjectStateChanged(value) => EventReplacementResource::new(
                ResyncResource::new(
                    PROJECT_HISTORY_RESOURCE,
                    EXECUTE_PROJECT_COMMAND_METHOD,
                    PublicMethodKind::Command,
                    PROJECT_EDITOR_SCHEMA_VERSION,
                ),
                value.project_revision(),
                None,
            ),
            Self::AsyncJobsChanged(value) => EventReplacementResource::new(
                ResyncResource::new(
                    "superi.jobs",
                    GET_ASYNC_JOBS_METHOD,
                    PublicMethodKind::Query,
                    ASYNC_JOBS_SCHEMA_VERSION,
                ),
                value.jobs_revision(),
                None,
            ),
            Self::ProjectRecoveryChanged(value) => EventReplacementResource::new(
                ResyncResource::new(
                    "superi.project.recovery",
                    GET_PROJECT_RECOVERY_METHOD,
                    PublicMethodKind::Query,
                    PROJECT_RECOVERY_SCHEMA_VERSION,
                ),
                value.catalog_revision(),
                Some(value.project_revision()),
            ),
            Self::AudioAutomationChanged(value) => EventReplacementResource::new(
                ResyncResource::new(
                    "superi.audio.automation",
                    GET_AUDIO_AUTOMATION_METHOD,
                    PublicMethodKind::Query,
                    AUDIO_AUTOMATION_SCHEMA_VERSION,
                ),
                value.audio_automation_revision(),
                None,
            ),
            Self::ProjectSettingsChanged(value) => EventReplacementResource::new(
                ResyncResource::new(
                    "superi.project.settings",
                    GET_PROJECT_SETTINGS_METHOD,
                    PublicMethodKind::Query,
                    PROJECT_SETTINGS_SCHEMA_VERSION,
                ),
                value.project_revision(),
                None,
            ),
            Self::MediaCapabilitiesChanged(value) => EventReplacementResource::new(
                ResyncResource::new(
                    "superi.media.capabilities",
                    GET_MEDIA_CAPABILITIES_METHOD,
                    PublicMethodKind::Query,
                    MEDIA_CAPABILITIES_SCHEMA_VERSION,
                ),
                value.snapshot().revision(),
                None,
            ),
            Self::EngineIntrospectionChanged(value) => EventReplacementResource::new(
                ResyncResource::new(
                    "superi.engine.introspection",
                    GET_ENGINE_INTROSPECTION_METHOD,
                    PublicMethodKind::Query,
                    ENGINE_INTROSPECTION_SCHEMA_VERSION,
                ),
                value.snapshot().revision(),
                None,
            ),
            Self::ScenarioStateChanged(value) => EventReplacementResource::new(
                ResyncResource::new(
                    "superi.slice.scenario.state",
                    EXECUTE_SCENARIO_ACTION_METHOD,
                    PublicMethodKind::Command,
                    SLICE_SCENARIO_SCHEMA_VERSION,
                ),
                value.project_revision(),
                None,
            ),
        }
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::ProjectStateChanged(value) => {
                validate_command_event(
                    value.sequence(),
                    value.command_sequence(),
                    value.transaction_id(),
                )?;
                validate_revision(
                    "validate_project_state_event",
                    value.project_revision(),
                    value.state().project_revision(),
                )?;
                validate_schema(
                    "validate_project_state_event",
                    value.state().schema_version(),
                    &PROJECT_EDITOR_SCHEMA_VERSION,
                )
            }
            Self::AsyncJobsChanged(value) => {
                validate_command_event(
                    value.sequence(),
                    value.command_sequence(),
                    value.transaction_id(),
                )?;
                validate_revision(
                    "validate_async_jobs_event",
                    value.jobs_revision(),
                    value.snapshot().revision(),
                )?;
                validate_schema(
                    "validate_async_jobs_event",
                    value.snapshot().schema_version(),
                    &ASYNC_JOBS_SCHEMA_VERSION,
                )
            }
            Self::ProjectRecoveryChanged(value) => {
                validate_command_event(
                    value.sequence(),
                    value.command_sequence(),
                    value.transaction_id(),
                )?;
                validate_revision(
                    "validate_project_recovery_event",
                    value.project_revision(),
                    value.snapshot().project_revision(),
                )?;
                validate_revision(
                    "validate_project_recovery_event",
                    value.catalog_revision(),
                    value.snapshot().catalog_revision(),
                )?;
                validate_schema(
                    "validate_project_recovery_event",
                    value.snapshot().schema_version(),
                    &PROJECT_RECOVERY_SCHEMA_VERSION,
                )
            }
            Self::AudioAutomationChanged(value) => {
                validate_command_event(
                    value.sequence(),
                    value.command_sequence(),
                    value.transaction_id(),
                )?;
                validate_revision(
                    "validate_audio_automation_event",
                    value.audio_automation_revision(),
                    value.snapshot().revision(),
                )?;
                validate_schema(
                    "validate_audio_automation_event",
                    value.snapshot().schema_version(),
                    &AUDIO_AUTOMATION_SCHEMA_VERSION,
                )
            }
            Self::ProjectSettingsChanged(value) => {
                validate_command_event(
                    value.sequence(),
                    value.command_sequence(),
                    value.transaction_id(),
                )?;
                validate_revision(
                    "validate_project_settings_event",
                    value.project_revision(),
                    value.snapshot().project_revision(),
                )?;
                validate_schema(
                    "validate_project_settings_event",
                    value.snapshot().schema_version(),
                    &PROJECT_SETTINGS_SCHEMA_VERSION,
                )
            }
            Self::MediaCapabilitiesChanged(value) => validate_schema(
                "validate_media_capabilities_event",
                value.snapshot().schema_version(),
                &MEDIA_CAPABILITIES_SCHEMA_VERSION,
            ),
            Self::EngineIntrospectionChanged(value) => validate_schema(
                "validate_engine_introspection_event",
                value.snapshot().schema_version(),
                &ENGINE_INTROSPECTION_SCHEMA_VERSION,
            ),
            Self::ScenarioStateChanged(value) => {
                validate_command_event(
                    value.sequence(),
                    value.command_sequence(),
                    value.transaction_id(),
                )?;
                validate_revision(
                    "validate_scenario_state_event",
                    value.project_revision(),
                    value.state().revision(),
                )?;
                validate_schema(
                    "validate_scenario_state_event",
                    value.state().schema_version(),
                    &SLICE_SCENARIO_SCHEMA_VERSION,
                )
            }
        }
    }
}

macro_rules! impl_public_event_conversion {
    ($source:ty, $variant:ident) => {
        impl TryFrom<$source> for PublicApiEvent {
            type Error = Error;

            fn try_from(value: $source) -> Result<Self> {
                let event = Self::$variant(Box::new(value));
                event.validate()?;
                Ok(event)
            }
        }
    };
}

impl_public_event_conversion!(ProjectStateChanged, ProjectStateChanged);
impl_public_event_conversion!(AsyncJobsChanged, AsyncJobsChanged);
impl_public_event_conversion!(ProjectRecoveryChanged, ProjectRecoveryChanged);
impl_public_event_conversion!(AudioAutomationChanged, AudioAutomationChanged);
impl_public_event_conversion!(ProjectSettingsChanged, ProjectSettingsChanged);
impl_public_event_conversion!(MediaCapabilitiesChanged, MediaCapabilitiesChanged);
impl_public_event_conversion!(EngineIntrospectionChanged, EngineIntrospectionChanged);
impl_public_event_conversion!(ScenarioStateChanged, ScenarioStateChanged);

/// One immutable retained record with public ordering and exact replacement metadata.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PublicEventRecord {
    stream_id: EventStreamId,
    sequence: PublicEventSequence,
    event_name: String,
    schema_version: SemanticVersion,
    correlation: PublicEventCorrelation,
    replacement_resource: EventReplacementResource,
    event: PublicApiEvent,
}

impl PublicEventRecord {
    /// Returns the process-lifetime stream identity.
    #[must_use]
    pub const fn stream_id(&self) -> &EventStreamId {
        &self.stream_id
    }

    /// Returns the independent public delivery sequence.
    #[must_use]
    pub const fn sequence(&self) -> PublicEventSequence {
        self.sequence
    }

    /// Returns the permanent event name.
    #[must_use]
    pub fn event_name(&self) -> &str {
        &self.event_name
    }

    /// Returns the exact event payload schema version.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns typed transaction or observation correlation.
    #[must_use]
    pub const fn correlation(&self) -> &PublicEventCorrelation {
        &self.correlation
    }

    /// Returns the resource this event fully replaces.
    #[must_use]
    pub const fn replacement_resource(&self) -> &EventReplacementResource {
        &self.replacement_resource
    }

    /// Returns the closed typed event payload.
    #[must_use]
    pub const fn event(&self) -> &PublicApiEvent {
        &self.event
    }

    fn validate(&self) -> Result<()> {
        self.event.validate()?;
        if self.event_name != self.event.name()
            || self.schema_version != self.event.schema_version()
            || self.correlation != self.event.correlation()
            || self.replacement_resource != self.event.replacement_resource()
        {
            return Err(stream_error(
                ErrorCategory::CorruptData,
                Recoverability::Terminal,
                "validate_event_record",
                "public event record metadata does not match its typed payload",
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicEventRecordWire {
    stream_id: EventStreamId,
    sequence: PublicEventSequence,
    event_name: String,
    schema_version: SemanticVersion,
    correlation: PublicEventCorrelation,
    replacement_resource: EventReplacementResource,
    event: PublicApiEvent,
}

impl<'de> Deserialize<'de> for PublicEventRecord {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PublicEventRecordWire::deserialize(deserializer)?;
        let value = Self {
            stream_id: wire.stream_id,
            sequence: wire.sequence,
            event_name: wire.event_name,
            schema_version: wire.schema_version,
            correlation: wire.correlation,
            replacement_resource: wire.replacement_resource,
            event: wire.event,
        };
        value.validate().map_err(D::Error::custom)?;
        Ok(value)
    }
}

/// Where a newly registered subscriber begins reading retained state changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStart {
    /// Begin after the latest sequence present when the subscription opens.
    Latest,
    /// Begin before the oldest complete record still available for replay.
    EarliestAvailable,
}

/// Strict command for registering one independent subscriber identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenEventSubscription {
    subscription_id: SubscriptionId,
    start: SubscriptionStart,
}

impl OpenEventSubscription {
    /// Creates one registration command.
    #[must_use]
    pub const fn new(subscription_id: SubscriptionId, start: SubscriptionStart) -> Self {
        Self {
            subscription_id,
            start,
        }
    }

    /// Returns the caller-owned subscriber identity.
    #[must_use]
    pub const fn subscription_id(&self) -> &SubscriptionId {
        &self.subscription_id
    }

    /// Returns the requested initial cursor policy.
    #[must_use]
    pub const fn start(&self) -> SubscriptionStart {
        self.start
    }
}

impl ApiCommand for OpenEventSubscription {
    type Response = OpenEventSubscriptionResult;
    const METHOD: &'static str = OPEN_EVENT_SUBSCRIPTION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = EVENT_STREAM_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful subscriber registration and its caller-held initial cursor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenEventSubscriptionResult {
    stream_id: EventStreamId,
    subscription_id: SubscriptionId,
    initial_after_sequence: u64,
    snapshot: EventStreamSnapshot,
}

impl OpenEventSubscriptionResult {
    /// Returns the current process-lifetime stream identity.
    #[must_use]
    pub const fn stream_id(&self) -> &EventStreamId {
        &self.stream_id
    }

    /// Returns the registered subscriber identity.
    #[must_use]
    pub const fn subscription_id(&self) -> &SubscriptionId {
        &self.subscription_id
    }

    /// Returns the cursor to provide on the first poll.
    #[must_use]
    pub const fn initial_after_sequence(&self) -> u64 {
        self.initial_after_sequence
    }

    /// Returns complete subscription control state after registration.
    #[must_use]
    pub const fn snapshot(&self) -> &EventStreamSnapshot {
        &self.snapshot
    }
}

/// Strict command for removing one subscriber without affecting any other cursor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloseEventSubscription {
    stream_id: EventStreamId,
    subscription_id: SubscriptionId,
}

impl CloseEventSubscription {
    /// Creates one close command bound to a stream lifetime.
    #[must_use]
    pub const fn new(stream_id: EventStreamId, subscription_id: SubscriptionId) -> Self {
        Self {
            stream_id,
            subscription_id,
        }
    }

    /// Returns the stream identity last observed by the caller.
    #[must_use]
    pub const fn stream_id(&self) -> &EventStreamId {
        &self.stream_id
    }

    /// Returns the subscriber to remove.
    #[must_use]
    pub const fn subscription_id(&self) -> &SubscriptionId {
        &self.subscription_id
    }
}

impl ApiCommand for CloseEventSubscription {
    type Response = CloseEventSubscriptionResult;
    const METHOD: &'static str = CLOSE_EVENT_SUBSCRIPTION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = EVENT_STREAM_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful subscriber removal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloseEventSubscriptionResult {
    stream_id: EventStreamId,
    subscription_id: SubscriptionId,
    snapshot: EventStreamSnapshot,
}

impl CloseEventSubscriptionResult {
    /// Returns the current stream identity.
    #[must_use]
    pub const fn stream_id(&self) -> &EventStreamId {
        &self.stream_id
    }

    /// Returns the removed subscriber identity.
    #[must_use]
    pub const fn subscription_id(&self) -> &SubscriptionId {
        &self.subscription_id
    }

    /// Returns complete subscription control state after removal.
    #[must_use]
    pub const fn snapshot(&self) -> &EventStreamSnapshot {
        &self.snapshot
    }
}

/// Strict retryable query using a caller-held replay cursor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PollEvents {
    stream_id: EventStreamId,
    subscription_id: SubscriptionId,
    after_sequence: u64,
    requested_limit: u32,
}

impl PollEvents {
    /// Creates one bounded non-destructive poll query.
    pub fn new(
        stream_id: EventStreamId,
        subscription_id: SubscriptionId,
        after_sequence: u64,
        requested_limit: u32,
    ) -> Result<Self> {
        validate_bound("requested_limit", requested_limit)?;
        Ok(Self {
            stream_id,
            subscription_id,
            after_sequence,
            requested_limit,
        })
    }

    /// Returns the stream identity last observed by the caller.
    #[must_use]
    pub const fn stream_id(&self) -> &EventStreamId {
        &self.stream_id
    }

    /// Returns the registered subscriber identity.
    #[must_use]
    pub const fn subscription_id(&self) -> &SubscriptionId {
        &self.subscription_id
    }

    /// Returns the last sequence already durably handled by the caller.
    #[must_use]
    pub const fn after_sequence(&self) -> u64 {
        self.after_sequence
    }

    /// Returns the caller-side batch limit.
    #[must_use]
    pub const fn requested_limit(&self) -> u32 {
        self.requested_limit
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PollEventsWire {
    stream_id: EventStreamId,
    subscription_id: SubscriptionId,
    after_sequence: u64,
    requested_limit: u32,
}

impl<'de> Deserialize<'de> for PollEvents {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PollEventsWire::deserialize(deserializer)?;
        Self::new(
            wire.stream_id,
            wire.subscription_id,
            wire.after_sequence,
            wire.requested_limit,
        )
        .map_err(D::Error::custom)
    }
}

impl ApiCommand for PollEvents {
    type Response = PollEventsResult;
    const METHOD: &'static str = POLL_EVENT_SUBSCRIPTION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = EVENT_STREAM_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// One successful bounded replay result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventBatch {
    stream_id: EventStreamId,
    subscription_id: SubscriptionId,
    after_sequence: u64,
    through_sequence: u64,
    records: Vec<PublicEventRecord>,
}

impl EventBatch {
    /// Returns the active stream identity.
    #[must_use]
    pub const fn stream_id(&self) -> &EventStreamId {
        &self.stream_id
    }

    /// Returns the subscriber for which this batch was produced.
    #[must_use]
    pub const fn subscription_id(&self) -> &SubscriptionId {
        &self.subscription_id
    }

    /// Returns the caller cursor used to produce this batch.
    #[must_use]
    pub const fn after_sequence(&self) -> u64 {
        self.after_sequence
    }

    /// Returns the cursor to persist after completely handling this batch.
    #[must_use]
    pub const fn through_sequence(&self) -> u64 {
        self.through_sequence
    }

    /// Returns complete immutable records in strict public sequence order.
    #[must_use]
    pub fn records(&self) -> &[PublicEventRecord] {
        &self.records
    }
}

/// Why a caller must refresh complete state instead of replaying an incomplete suffix.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventGapReason {
    /// The caller references a different process-lifetime stream identity.
    StreamRestarted,
    /// Required records were evicted from the bounded replay buffer.
    CursorEvicted,
}

/// Explicit recovery barrier and complete replacement-state manifest after a gap.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventGap {
    reason: EventGapReason,
    requested_stream_id: EventStreamId,
    current_stream_id: EventStreamId,
    subscription_id: SubscriptionId,
    requested_after_sequence: u64,
    oldest_available_sequence: Option<u64>,
    latest_sequence: u64,
    reset_barrier: u64,
    replacement_resources: Vec<ResyncResource>,
}

impl EventGap {
    /// Returns the exact replay failure mode.
    #[must_use]
    pub const fn reason(&self) -> EventGapReason {
        self.reason
    }

    /// Returns the stream identity supplied by the caller.
    #[must_use]
    pub const fn requested_stream_id(&self) -> &EventStreamId {
        &self.requested_stream_id
    }

    /// Returns the current stream identity to persist after recovery.
    #[must_use]
    pub const fn current_stream_id(&self) -> &EventStreamId {
        &self.current_stream_id
    }

    /// Returns the affected subscriber identity.
    #[must_use]
    pub const fn subscription_id(&self) -> &SubscriptionId {
        &self.subscription_id
    }

    /// Returns the unusable cursor supplied by the caller.
    #[must_use]
    pub const fn requested_after_sequence(&self) -> u64 {
        self.requested_after_sequence
    }

    /// Returns the oldest complete record still retained, when any exists.
    #[must_use]
    pub const fn oldest_available_sequence(&self) -> Option<u64> {
        self.oldest_available_sequence
    }

    /// Returns the latest sequence allocated before this response.
    #[must_use]
    pub const fn latest_sequence(&self) -> u64 {
        self.latest_sequence
    }

    /// Returns the cursor barrier established after every listed resource is refreshed.
    #[must_use]
    pub const fn reset_barrier(&self) -> u64 {
        self.reset_barrier
    }

    /// Returns every authoritative state resource required for complete recovery.
    #[must_use]
    pub fn replacement_resources(&self) -> &[ResyncResource] {
        &self.replacement_resources
    }
}

/// Poll result that never hides a replay gap behind partial delivery.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "result", rename_all = "snake_case")]
pub enum PollEventsResult {
    /// A complete ordered suffix, possibly empty.
    Events(EventBatch),
    /// No records were delivered because complete state refresh is required.
    ResyncRequired(EventGap),
}

impl PollEventsResult {
    /// Returns a replay batch when no gap occurred.
    #[must_use]
    pub const fn batch(&self) -> Option<&EventBatch> {
        match self {
            Self::Events(batch) => Some(batch),
            Self::ResyncRequired(_) => None,
        }
    }

    /// Returns explicit recovery state when replay cannot be complete.
    #[must_use]
    pub const fn gap(&self) -> Option<&EventGap> {
        match self {
            Self::Events(_) => None,
            Self::ResyncRequired(gap) => Some(gap),
        }
    }
}

/// Complete inspectable state for the event subscription control resource.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventStreamSnapshot {
    schema_version: SemanticVersion,
    stream_id: EventStreamId,
    config: EventStreamConfig,
    oldest_available_sequence: Option<u64>,
    latest_sequence: u64,
    retained_event_count: u32,
    active_subscription_count: u32,
}

impl EventStreamSnapshot {
    /// Returns the event stream schema version.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns the process-lifetime stream identity.
    #[must_use]
    pub const fn stream_id(&self) -> &EventStreamId {
        &self.stream_id
    }

    /// Returns the validated finite stream limits.
    #[must_use]
    pub const fn config(&self) -> EventStreamConfig {
        self.config
    }

    /// Returns the oldest complete retained public sequence.
    #[must_use]
    pub const fn oldest_available_sequence(&self) -> Option<u64> {
        self.oldest_available_sequence
    }

    /// Returns the latest allocated sequence, or zero before the first event.
    #[must_use]
    pub const fn latest_sequence(&self) -> u64 {
        self.latest_sequence
    }

    /// Returns the number of complete event records currently retained.
    #[must_use]
    pub const fn retained_event_count(&self) -> u32 {
        self.retained_event_count
    }

    /// Returns the number of registered subscriber identities.
    #[must_use]
    pub const fn active_subscription_count(&self) -> u32 {
        self.active_subscription_count
    }
}

impl ApiResource for EventStreamSnapshot {
    const RESOURCE: &'static str = "superi.events.subscription";
    const SCHEMA_VERSION: SemanticVersion = EVENT_STREAM_SCHEMA_VERSION;
}

/// API-owned bounded replay broker with caller-held non-destructive cursors.
#[derive(Clone, Debug)]
pub struct EventStreamApi {
    stream_id: EventStreamId,
    config: EventStreamConfig,
    last_sequence: u64,
    records: VecDeque<PublicEventRecord>,
    subscriptions: BTreeSet<SubscriptionId>,
}

impl EventStreamApi {
    /// Creates one empty process-lifetime stream.
    #[must_use]
    pub fn new(stream_id: EventStreamId, config: EventStreamConfig) -> Self {
        Self {
            stream_id,
            config,
            last_sequence: 0,
            records: VecDeque::new(),
            subscriptions: BTreeSet::new(),
        }
    }

    /// Returns complete inspectable subscription control state.
    #[must_use]
    pub fn snapshot(&self) -> EventStreamSnapshot {
        EventStreamSnapshot {
            schema_version: EVENT_STREAM_SCHEMA_VERSION,
            stream_id: self.stream_id.clone(),
            config: self.config,
            oldest_available_sequence: self.oldest_sequence(),
            latest_sequence: self.last_sequence,
            retained_event_count: u32::try_from(self.records.len())
                .expect("retained records are bounded by a u32 configuration"),
            active_subscription_count: u32::try_from(self.subscriptions.len())
                .expect("subscriptions are bounded by a u32 configuration"),
        }
    }

    /// Registers one caller-owned subscription without allocating a server-side cursor.
    pub fn open(&mut self, command: OpenEventSubscription) -> Result<OpenEventSubscriptionResult> {
        let subscription_id = command.subscription_id;
        if self.subscriptions.contains(&subscription_id) {
            return Err(stream_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "open_subscription",
                "event subscription identity is already registered",
            ));
        }
        if self.subscriptions.len()
            >= usize::try_from(self.config.max_subscriptions)
                .expect("u32 subscription bounds fit the current platform")
        {
            return Err(stream_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Retryable,
                "open_subscription",
                "event subscription registry reached its configured bound",
            ));
        }

        let initial_after_sequence = match command.start {
            SubscriptionStart::Latest => self.last_sequence,
            SubscriptionStart::EarliestAvailable => self
                .oldest_sequence()
                .map_or(self.last_sequence, |sequence| sequence.saturating_sub(1)),
        };
        self.subscriptions.insert(subscription_id.clone());
        Ok(OpenEventSubscriptionResult {
            stream_id: self.stream_id.clone(),
            subscription_id,
            initial_after_sequence,
            snapshot: self.snapshot(),
        })
    }

    /// Removes one subscriber without mutating retained records or another subscriber.
    pub fn close(
        &mut self,
        command: CloseEventSubscription,
    ) -> Result<CloseEventSubscriptionResult> {
        if command.stream_id != self.stream_id {
            return Err(stream_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "close_subscription",
                "event stream identity changed before the subscription was closed",
            ));
        }
        if !self.subscriptions.remove(&command.subscription_id) {
            return Err(stream_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "close_subscription",
                "event subscription identity is not registered",
            ));
        }
        Ok(CloseEventSubscriptionResult {
            stream_id: self.stream_id.clone(),
            subscription_id: command.subscription_id,
            snapshot: self.snapshot(),
        })
    }

    /// Publishes one validated closed-union event and returns its immutable retained record.
    pub fn publish(&mut self, event: PublicApiEvent) -> Result<PublicEventRecord> {
        event.validate()?;
        let sequence = self.next_sequence()?;
        let record = PublicEventRecord {
            stream_id: self.stream_id.clone(),
            sequence,
            event_name: event.name().to_owned(),
            schema_version: event.schema_version(),
            correlation: event.correlation(),
            replacement_resource: event.replacement_resource(),
            event,
        };
        record.validate()?;
        self.last_sequence = sequence.get();
        self.records.push_back(record.clone());
        while self.records.len()
            > usize::try_from(self.config.retained_events)
                .expect("u32 retention bounds fit the current platform")
        {
            self.records.pop_front();
        }
        Ok(record)
    }

    /// Validates, converts, and publishes one existing typed API event.
    pub fn publish_typed<T>(&mut self, event: T) -> Result<PublicEventRecord>
    where
        PublicApiEvent: TryFrom<T, Error = Error>,
    {
        self.publish(PublicApiEvent::try_from(event)?)
    }

    /// Returns a deterministic suffix or an explicit complete-state recovery barrier.
    pub fn poll(&self, query: PollEvents) -> Result<PollEventsResult> {
        if query.stream_id != self.stream_id {
            return Ok(PollEventsResult::ResyncRequired(self.gap(
                EventGapReason::StreamRestarted,
                query.stream_id,
                query.subscription_id,
                query.after_sequence,
            )));
        }
        if !self.subscriptions.contains(&query.subscription_id) {
            return Err(stream_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "poll_events",
                "event subscription identity is not registered",
            ));
        }
        if query.after_sequence > self.last_sequence {
            return Err(stream_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "poll_events",
                "event cursor is newer than the current stream",
            ));
        }
        if let Some(oldest) = self.oldest_sequence() {
            if query.after_sequence.saturating_add(1) < oldest {
                return Ok(PollEventsResult::ResyncRequired(self.gap(
                    EventGapReason::CursorEvicted,
                    query.stream_id,
                    query.subscription_id,
                    query.after_sequence,
                )));
            }
        }

        let limit = query.requested_limit.min(self.config.max_batch_size);
        let limit = usize::try_from(limit).expect("u32 batch bounds fit the current platform");
        let records = self
            .records
            .iter()
            .filter(|record| record.sequence.get() > query.after_sequence)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let through_sequence = records
            .last()
            .map_or(query.after_sequence, |record| record.sequence.get());
        Ok(PollEventsResult::Events(EventBatch {
            stream_id: self.stream_id.clone(),
            subscription_id: query.subscription_id,
            after_sequence: query.after_sequence,
            through_sequence,
            records,
        }))
    }

    fn next_sequence(&self) -> Result<PublicEventSequence> {
        let next = self.last_sequence.checked_add(1).ok_or_else(|| {
            stream_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "allocate_event_sequence",
                "public event sequence space is exhausted",
            )
        })?;
        PublicEventSequence::new(next)
    }

    fn oldest_sequence(&self) -> Option<u64> {
        self.records.front().map(|record| record.sequence.get())
    }

    fn gap(
        &self,
        reason: EventGapReason,
        requested_stream_id: EventStreamId,
        subscription_id: SubscriptionId,
        requested_after_sequence: u64,
    ) -> EventGap {
        EventGap {
            reason,
            requested_stream_id,
            current_stream_id: self.stream_id.clone(),
            subscription_id,
            requested_after_sequence,
            oldest_available_sequence: self.oldest_sequence(),
            latest_sequence: self.last_sequence,
            reset_barrier: self.last_sequence,
            replacement_resources: replacement_resource_manifest(),
        }
    }
}

fn validate_identifier(operation: &'static str, value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.is_empty()
        || bytes.len() > MAX_EVENT_IDENTIFIER_BYTES
        || !bytes[0].is_ascii_lowercase()
        || bytes.iter().copied().any(|byte| {
            !(byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-' | b':'))
        })
    {
        return Err(stream_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "event stream identifiers must use bounded canonical lowercase ASCII",
        ));
    }
    Ok(())
}

fn validate_bound(field: &'static str, value: u32) -> Result<()> {
    if value == 0 || value > MAX_EVENT_STREAM_BOUND {
        return Err(stream_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "validate_stream_bound",
            "event stream bounds must be nonzero and within the defensive maximum",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_stream_bound").with_field("field", field),
        ));
    }
    Ok(())
}

fn validate_command_event(
    source_event_sequence: u64,
    command_sequence: u64,
    transaction_id: &str,
) -> Result<()> {
    if source_event_sequence == 0 || command_sequence == 0 || transaction_id.trim().is_empty() {
        return Err(stream_error(
            ErrorCategory::CorruptData,
            Recoverability::Terminal,
            "validate_command_correlation",
            "command event correlation must contain nonzero sequences and a transaction identity",
        ));
    }
    Ok(())
}

fn validate_revision(operation: &'static str, event: u64, snapshot: u64) -> Result<()> {
    if event != snapshot {
        return Err(stream_error(
            ErrorCategory::CorruptData,
            Recoverability::Terminal,
            operation,
            "event revision does not match its complete replacement snapshot",
        ));
    }
    Ok(())
}

fn validate_schema(
    operation: &'static str,
    actual: &SemanticVersion,
    expected: &SemanticVersion,
) -> Result<()> {
    if actual != expected {
        return Err(stream_error(
            ErrorCategory::CorruptData,
            Recoverability::Terminal,
            operation,
            "event replacement snapshot uses an incompatible schema version",
        ));
    }
    Ok(())
}

fn command_correlation(
    source_event_sequence: u64,
    command_sequence: u64,
    transaction_id: &str,
) -> PublicEventCorrelation {
    PublicEventCorrelation::Command {
        source_event_sequence,
        command_sequence,
        transaction_id: transaction_id.to_owned(),
    }
}

fn stream_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

#[cfg(test)]
mod tests {
    use super::{EventStreamApi, EventStreamConfig, EventStreamId};
    use superi_core::error::{ErrorCategory, Recoverability};

    #[test]
    fn sequence_exhaustion_is_explicit_and_terminal() {
        let mut stream = EventStreamApi::new(
            EventStreamId::new("stream-exhausted").unwrap(),
            EventStreamConfig::default(),
        );
        stream.last_sequence = u64::MAX;

        let error = stream.next_sequence().unwrap_err();
        assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
        assert_eq!(error.recoverability(), Recoverability::Terminal);
    }
}
