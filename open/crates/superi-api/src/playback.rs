//! Strict public control for the authoritative interactive playback transport.

use serde::{Deserialize, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::SemanticVersion;
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineTransactionId,
};
use superi_engine::editor as engine;
use superi_engine::transport::{
    PlaybackDirection as EnginePlaybackDirection, PlaybackTransportCommand,
};

use crate::commands::ApiCommand;
use crate::editor::ExactTime;
use crate::permissions::{
    ApiPermissionKind, ApiPermissionRequirementMode, ApiPermissionRequirements,
};
use crate::schema::PublicMethodKind;
use crate::version::{EXECUTE_PLAYBACK_TRANSPORT_METHOD, PLAYBACK_TRANSPORT_SCHEMA_VERSION};

const COMPONENT: &str = "superi-api.playback";

/// Public signed playback traversal direction.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackDirection {
    Forward,
    Reverse,
}

impl PlaybackDirection {
    const fn into_engine(self) -> EnginePlaybackDirection {
        match self {
            Self::Forward => EnginePlaybackDirection::Forward,
            Self::Reverse => EnginePlaybackDirection::Reverse,
        }
    }
}

/// One strict transport action executed by the playback-domain owner.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlaybackTransportAction {
    Inspect {},
    Play {},
    Pause {},
    Stop {},
    Seek { target: ExactTime },
    BeginScrub {},
    ScrubTo { target: ExactTime },
    EndScrub { resume: bool },
    SetLoop { enabled: bool },
    SetRate { numerator: i64, denominator: u64 },
    SetDirection { direction: PlaybackDirection },
    Shuttle { numerator: i64, denominator: u64 },
    StepFrames { delta: i64 },
}

impl PlaybackTransportAction {
    fn into_engine(self) -> Result<PlaybackTransportCommand> {
        Ok(match self {
            Self::Inspect {} => PlaybackTransportCommand::Inspect,
            Self::Play {} => PlaybackTransportCommand::Play,
            Self::Pause {} => PlaybackTransportCommand::Pause,
            Self::Stop {} => PlaybackTransportCommand::Stop,
            Self::Seek { target } => PlaybackTransportCommand::Seek(target.into_engine()?),
            Self::BeginScrub {} => PlaybackTransportCommand::BeginScrub,
            Self::ScrubTo { target } => PlaybackTransportCommand::ScrubTo(target.into_engine()?),
            Self::EndScrub { resume } => PlaybackTransportCommand::EndScrub { resume },
            Self::SetLoop { enabled } => PlaybackTransportCommand::SetLoopToBounds(enabled),
            Self::SetRate {
                numerator,
                denominator,
            } => PlaybackTransportCommand::SetRate(engine::PlaybackRate::new(
                numerator,
                denominator,
            )?),
            Self::SetDirection { direction } => {
                PlaybackTransportCommand::SetDirection(direction.into_engine())
            }
            Self::Shuttle {
                numerator,
                denominator,
            } => PlaybackTransportCommand::Shuttle(engine::PlaybackRate::new(
                numerator,
                denominator,
            )?),
            Self::StepFrames { delta } => PlaybackTransportCommand::StepFrames(delta),
        })
    }
}

/// Caller-correlated strict playback transport request.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutePlaybackTransport {
    transaction_id: String,
    command: PlaybackTransportAction,
}

impl ExecutePlaybackTransport {
    #[must_use]
    pub fn new(transaction_id: impl Into<String>, command: PlaybackTransportAction) -> Self {
        Self {
            transaction_id: transaction_id.into(),
            command,
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn command(&self) -> PlaybackTransportAction {
        self.command
    }

    fn into_parts(self) -> (String, PlaybackTransportAction) {
        (self.transaction_id, self.command)
    }
}

impl ApiCommand for ExecutePlaybackTransport {
    type Response = ExecutePlaybackTransportResult;

    const METHOD: &'static str = EXECUTE_PLAYBACK_TRANSPORT_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = PLAYBACK_TRANSPORT_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Immediate bounded acceptance of one asynchronous transport command.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutePlaybackTransportResult {
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::SemanticVersionBinding)
    )]
    schema_version: SemanticVersion,
    transaction_id: String,
    command_sequence: u64,
    accepted: bool,
    pending_command: bool,
}

impl ExecutePlaybackTransportResult {
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub const fn accepted(&self) -> bool {
        self.accepted
    }

    #[must_use]
    pub const fn pending_command(&self) -> bool {
        self.pending_command
    }
}

pub(crate) fn execute_playback_transport(
    dispatcher: &mut EngineCommandDispatcher,
    request: ExecutePlaybackTransport,
) -> Result<ExecutePlaybackTransportResult> {
    dispatcher.discard_playback_events()?;
    let (transaction_id, command) = request.into_parts();
    let outcome = dispatcher.dispatch(EngineCommandRequest::new(
        EngineTransactionId::new(transaction_id)?,
        EngineCommand::ExecutePlayback(command.into_engine()?),
    ))?;
    let EngineCommandResult::PlaybackAccepted { .. } = outcome.result() else {
        return Err(unexpected_result());
    };
    Ok(ExecutePlaybackTransportResult {
        schema_version: PLAYBACK_TRANSPORT_SCHEMA_VERSION,
        transaction_id: outcome.transaction_id().as_str().to_owned(),
        command_sequence: outcome.command_sequence(),
        accepted: true,
        pending_command: true,
    })
}

fn unexpected_result() -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "playback dispatcher returned an unrelated command result",
    )
    .with_context(ErrorContext::new(COMPONENT, "execute"))
}
