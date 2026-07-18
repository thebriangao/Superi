//! Bounded command and ordered event transport above the managed engine connection.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use superi_api::commands::{GetEditorState, GetEditorStateResult};
use superi_api::editor::{ExecuteProjectCommand, ExecuteProjectCommandResult};
use superi_api::events::ProjectStateChanged;
use superi_api::local::LocalProjectExecution;
use superi_api::schema::{PublicApiError, PublicErrorContext};
use superi_api::version::{
    EXECUTE_PROJECT_COMMAND_METHOD, GET_EDITOR_STATE_METHOD, PROJECT_STATE_CHANGED_EVENT,
};
use superi_core::diagnostics::TraceValue;
use superi_core::error::{Error, ErrorCategory, Recoverability};

use crate::engine::EngineConnection;
use crate::project_lifecycle::{
    DesktopProjectFailure, DesktopProjectFailureClass, DesktopProjectState,
};

/// One Tauri event carries every generated public API event.
pub const DESKTOP_API_EVENT: &str = "superi://api-event";

const COMPONENT: &str = "superi.desktop.transport";
const STREAM_ID: &str = "superi.desktop.events.v1";
const INTEGRATION_VALIDATION_METHOD: &str = "superi.engine.integration.validation.get";
const ENGINE_INTROSPECTION_EVENT: &str = "superi.engine.introspection.changed";
const MAX_PENDING_REQUESTS: usize = 32;
const RETAINED_EVENTS: usize = 64;
const ENGINE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

/// Closed command surface accepted by the single desktop API dispatcher.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum DesktopTransportCommand {
    /// Opens a fresh connection generation and requests replay after one cursor.
    Connect { after_sequence: u64 },
    /// Forwards one generated public API request through the managed engine connection.
    Request {
        generation: u64,
        request_id: String,
        method: String,
        request: Value,
    },
    /// Cooperatively abandons one generation-scoped request.
    Cancel { generation: u64, request_id: String },
    /// Closes one connection generation and abandons its pending work.
    Disconnect { generation: u64 },
}

/// Ordered event envelope delivered through the one native Tauri event.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DesktopEventEnvelope {
    generation: u64,
    stream_id: String,
    sequence: u64,
    event: String,
    payload: Value,
}

impl DesktopEventEnvelope {
    /// Returns the connection generation that may accept this delivery.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the process-lifetime stream identity.
    #[must_use]
    pub fn stream_id(&self) -> &str {
        &self.stream_id
    }

    /// Returns the monotonic process-lifetime event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the generated public event name.
    #[must_use]
    pub fn event(&self) -> &str {
        &self.event
    }

    /// Returns the exact generated public event payload.
    #[must_use]
    pub const fn payload(&self) -> &Value {
        &self.payload
    }
}

/// Serializable result variants returned by the single desktop API dispatcher.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DesktopTransportReply {
    /// A new generation is connected with retained replay or an explicit resync barrier.
    Connected {
        generation: u64,
        stream_id: String,
        replay: Vec<DesktopEventEnvelope>,
        resync_required: bool,
    },
    /// One generated request completed with its request identity unchanged.
    Response {
        generation: u64,
        request_id: String,
        response: Value,
    },
    /// One cancellation attempt completed idempotently.
    Cancelled {
        generation: u64,
        request_id: String,
        cancelled: bool,
    },
    /// The requested generation is no longer connected.
    Disconnected { generation: u64 },
}

/// One dispatcher outcome and its optional ordered event side effect.
pub struct DesktopTransportOutcome {
    reply: DesktopTransportReply,
    event: Option<DesktopEventEnvelope>,
}

struct RoutedResponse {
    response: Value,
    event: Option<(String, Value)>,
}

impl DesktopTransportOutcome {
    pub(crate) fn into_parts(self) -> (DesktopTransportReply, Option<DesktopEventEnvelope>) {
        (self.reply, self.event)
    }

    /// Returns the generated response or control reply.
    #[must_use]
    pub const fn reply(&self) -> &DesktopTransportReply {
        &self.reply
    }

    /// Returns the ordered replacement event produced by this request, if any.
    #[must_use]
    pub const fn event(&self) -> Option<&DesktopEventEnvelope> {
        self.event.as_ref()
    }
}

struct DesktopTransportInner {
    generation: u64,
    connected: bool,
    sequence: u64,
    retained: VecDeque<DesktopEventEnvelope>,
    pending: HashMap<(u64, String), Arc<AtomicBool>>,
}

/// Shared bounded transport state managed by Tauri above C002's engine handle.
#[derive(Clone)]
pub struct DesktopTransportState {
    inner: Arc<Mutex<DesktopTransportInner>>,
}

impl Default for DesktopTransportState {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopTransportState {
    /// Creates one disconnected process-lifetime transport owner.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(DesktopTransportInner {
                generation: 0,
                connected: false,
                sequence: 0,
                retained: VecDeque::with_capacity(RETAINED_EVENTS),
                pending: HashMap::with_capacity(MAX_PENDING_REQUESTS),
            })),
        }
    }

    /// Executes a nonblocking connection, cancellation, or disconnect control command.
    pub fn dispatch_control(
        &self,
        command: DesktopTransportCommand,
    ) -> Result<DesktopTransportReply, PublicApiError> {
        match command {
            DesktopTransportCommand::Connect { after_sequence } => self.connect(after_sequence),
            DesktopTransportCommand::Cancel {
                generation,
                request_id,
            } => self.cancel(generation, request_id),
            DesktopTransportCommand::Disconnect { generation } => self.disconnect(generation),
            DesktopTransportCommand::Request { .. } => Err(transport_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "request work must run on a blocking-safe transport worker",
                "dispatch_control",
                &[],
            )),
        }
    }

    /// Executes one command on a blocking-safe worker and returns any event to emit.
    pub fn dispatch_blocking(
        &self,
        engine: &EngineConnection,
        projects: &DesktopProjectState,
        command: DesktopTransportCommand,
    ) -> Result<DesktopTransportOutcome, PublicApiError> {
        match command {
            DesktopTransportCommand::Request {
                generation,
                request_id,
                method,
                request,
            } => self.request(engine, projects, generation, request_id, method, request),
            control => self
                .dispatch_control(control)
                .map(|reply| DesktopTransportOutcome { reply, event: None }),
        }
    }

    fn connect(&self, after_sequence: u64) -> Result<DesktopTransportReply, PublicApiError> {
        let mut inner = self.lock("connect")?;
        for token in inner.pending.values() {
            token.store(true, Ordering::Release);
        }
        inner.pending.clear();
        inner.generation = inner.generation.checked_add(1).ok_or_else(|| {
            transport_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "the desktop transport generation was exhausted",
                "connect",
                &[],
            )
        })?;
        inner.connected = true;

        let oldest_sequence = inner
            .retained
            .front()
            .map_or(inner.sequence + 1, |event| event.sequence);
        let resync_required = after_sequence > inner.sequence
            || (after_sequence > 0 && after_sequence.saturating_add(1) < oldest_sequence);
        let replay = if resync_required {
            Vec::new()
        } else {
            inner
                .retained
                .iter()
                .filter(|event| event.sequence > after_sequence)
                .cloned()
                .map(|mut event| {
                    event.generation = inner.generation;
                    event
                })
                .collect()
        };

        Ok(DesktopTransportReply::Connected {
            generation: inner.generation,
            stream_id: STREAM_ID.to_owned(),
            replay,
            resync_required,
        })
    }

    fn cancel(
        &self,
        generation: u64,
        request_id: String,
    ) -> Result<DesktopTransportReply, PublicApiError> {
        let inner = self.lock("cancel")?;
        let cancelled = inner
            .pending
            .get(&(generation, request_id.clone()))
            .is_some_and(|token| {
                token.store(true, Ordering::Release);
                true
            });
        Ok(DesktopTransportReply::Cancelled {
            generation,
            request_id,
            cancelled,
        })
    }

    fn disconnect(&self, generation: u64) -> Result<DesktopTransportReply, PublicApiError> {
        let mut inner = self.lock("disconnect")?;
        if inner.generation == generation {
            inner.connected = false;
            for ((pending_generation, _), token) in &inner.pending {
                if *pending_generation == generation {
                    token.store(true, Ordering::Release);
                }
            }
            inner
                .pending
                .retain(|(pending_generation, _), _| *pending_generation != generation);
        }
        Ok(DesktopTransportReply::Disconnected { generation })
    }

    fn request(
        &self,
        engine: &EngineConnection,
        projects: &DesktopProjectState,
        generation: u64,
        request_id: String,
        method: String,
        request: Value,
    ) -> Result<DesktopTransportOutcome, PublicApiError> {
        self.validate_request(generation, &request_id, &method, &request)?;
        let token = self.admit_request(generation, &request_id, &method)?;
        if token.load(Ordering::Acquire) {
            self.remove_pending(generation, &request_id, &token)?;
            return Err(cancelled_request_error(generation, &request_id, &method));
        }
        let result = self.route_request(engine, projects, &method, request);
        self.remove_pending(generation, &request_id, &token)?;

        if token.load(Ordering::Acquire) && cancellation_wins_after_routing(&method) {
            return Err(cancelled_request_error(generation, &request_id, &method));
        }

        let RoutedResponse { response, event } = result?;
        let event = event
            .map(|(event, payload)| self.publish_event(generation, event, payload))
            .transpose()?;

        Ok(DesktopTransportOutcome {
            reply: DesktopTransportReply::Response {
                generation,
                request_id,
                response,
            },
            event,
        })
    }

    fn route_request(
        &self,
        engine: &EngineConnection,
        projects: &DesktopProjectState,
        method: &str,
        request: Value,
    ) -> Result<RoutedResponse, PublicApiError> {
        match method {
            INTEGRATION_VALIDATION_METHOD => {
                let result = engine
                    .request_integration_validation()
                    .and_then(|pending| pending.wait(ENGINE_RESPONSE_TIMEOUT))
                    .map_err(|error| public_error_from_core(&error, "request_engine", &[]))?;
                Ok(RoutedResponse {
                    response: serialize_response(&result, method)?,
                    event: Some((
                        ENGINE_INTROSPECTION_EVENT.to_owned(),
                        serde_json::json!({ "snapshot": result.snapshot().engine() }),
                    )),
                })
            }
            GET_EDITOR_STATE_METHOD => {
                let request = serde_json::from_value::<GetEditorState>(request).map_err(|_| {
                    transport_error(
                        ErrorCategory::InvalidInput,
                        Recoverability::UserCorrectable,
                        "the editor-state request does not match its generated schema",
                        "decode_request",
                        &[("method", method.into())],
                    )
                })?;
                let result = projects
                    .inspect_editor(request)
                    .map_err(|failure| public_error_from_project(&failure, "inspect_editor"))?;
                Ok(RoutedResponse {
                    response: serialize_response::<GetEditorStateResult>(&result, method)?,
                    event: None,
                })
            }
            EXECUTE_PROJECT_COMMAND_METHOD => {
                let request =
                    serde_json::from_value::<ExecuteProjectCommand>(request).map_err(|_| {
                        transport_error(
                            ErrorCategory::InvalidInput,
                            Recoverability::UserCorrectable,
                            "the project-command request does not match its generated schema",
                            "decode_request",
                            &[("method", method.into())],
                        )
                    })?;
                let execution = projects
                    .execute_timeline(request)
                    .map_err(|failure| public_error_from_project(&failure, "execute_timeline"))?;
                route_project_execution(execution, method)
            }
            _ => Err(transport_error(
                ErrorCategory::Unsupported,
                Recoverability::Degraded,
                "this generated method has no desktop route yet",
                "route_request",
                &[("method", method.into())],
            )),
        }
    }

    fn validate_request(
        &self,
        generation: u64,
        request_id: &str,
        method: &str,
        request: &Value,
    ) -> Result<(), PublicApiError> {
        let inner = self.lock("validate_request")?;
        if !inner.connected || inner.generation != generation {
            return Err(transport_error(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "the requested desktop connection generation is not active",
                "validate_request",
                &[("generation", generation.into()), ("method", method.into())],
            ));
        }
        drop(inner);
        if request_id.trim().is_empty() {
            return Err(transport_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "the desktop request identity must not be empty",
                "validate_request",
                &[("method", method.into())],
            ));
        }
        if !matches!(
            method,
            INTEGRATION_VALIDATION_METHOD
                | GET_EDITOR_STATE_METHOD
                | EXECUTE_PROJECT_COMMAND_METHOD
        ) {
            return Err(transport_error(
                ErrorCategory::Unsupported,
                Recoverability::Degraded,
                "this generated method has no desktop engine route yet",
                "route_request",
                &[("method", method.into())],
            ));
        }
        let valid_request = if method == INTEGRATION_VALIDATION_METHOD {
            request.is_null() || request.as_object().is_some_and(serde_json::Map::is_empty)
        } else {
            request.is_object()
        };
        if !valid_request {
            return Err(transport_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "the generated request has an invalid envelope",
                "decode_request",
                &[("method", method.into())],
            ));
        }
        Ok(())
    }

    fn admit_request(
        &self,
        generation: u64,
        request_id: &str,
        method: &str,
    ) -> Result<Arc<AtomicBool>, PublicApiError> {
        let mut inner = self.lock("admit_request")?;
        if inner.pending.len() >= MAX_PENDING_REQUESTS {
            return Err(transport_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Retryable,
                "the desktop transport request registry is full",
                "admit_request",
                &[("method", method.into())],
            ));
        }
        let key = (generation, request_id.to_owned());
        if inner.pending.contains_key(&key) {
            return Err(transport_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "the desktop request identity is already pending",
                "admit_request",
                &[("method", method.into()), ("request_id", request_id.into())],
            ));
        }
        let token = Arc::new(AtomicBool::new(false));
        inner.pending.insert(key, Arc::clone(&token));
        Ok(token)
    }

    fn remove_pending(
        &self,
        generation: u64,
        request_id: &str,
        token: &Arc<AtomicBool>,
    ) -> Result<(), PublicApiError> {
        let mut inner = self.lock("complete_request")?;
        let key = (generation, request_id.to_owned());
        if inner
            .pending
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            inner.pending.remove(&key);
        }
        Ok(())
    }

    fn publish_event(
        &self,
        generation: u64,
        event_name: String,
        payload: Value,
    ) -> Result<DesktopEventEnvelope, PublicApiError> {
        let mut inner = self.lock("publish_event")?;
        if !inner.connected || inner.generation != generation {
            return Err(transport_error(
                ErrorCategory::Cancelled,
                Recoverability::Retryable,
                "the response belongs to a replaced desktop connection generation",
                "publish_event",
                &[("generation", generation.into())],
            ));
        }
        inner.sequence = inner.sequence.checked_add(1).ok_or_else(|| {
            transport_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "the desktop event sequence was exhausted",
                "publish_event",
                &[],
            )
        })?;
        let event = DesktopEventEnvelope {
            generation,
            stream_id: STREAM_ID.to_owned(),
            sequence: inner.sequence,
            event: event_name,
            payload,
        };
        if inner.retained.len() == RETAINED_EVENTS {
            inner.retained.pop_front();
        }
        inner.retained.push_back(event.clone());
        Ok(event)
    }

    fn lock(
        &self,
        operation: &'static str,
    ) -> Result<MutexGuard<'_, DesktopTransportInner>, PublicApiError> {
        self.inner.lock().map_err(|_| {
            transport_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "the desktop transport state is unavailable",
                operation,
                &[],
            )
        })
    }
}

fn cancellation_wins_after_routing(method: &str) -> bool {
    // A durable authored command cannot be abandoned after commit. Its response and
    // replacement event must win a late cancellation so the caller can reconcile.
    method != EXECUTE_PROJECT_COMMAND_METHOD
}

fn cancelled_request_error(generation: u64, request_id: &str, method: &str) -> PublicApiError {
    transport_error(
        ErrorCategory::Cancelled,
        Recoverability::Retryable,
        "the desktop no longer needs this engine response",
        "request",
        &[
            ("generation", generation.into()),
            ("method", method.into()),
            ("request_id", request_id.into()),
        ],
    )
}

/// Creates the public failure returned when a transport worker cannot complete.
pub fn transport_task_error(operation: &'static str) -> PublicApiError {
    transport_error(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "the desktop transport worker could not complete",
        operation,
        &[],
    )
}

/// Creates the retryable public failure returned when native event emission fails.
pub fn event_emission_error() -> PublicApiError {
    transport_error(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "the desktop event channel is temporarily unavailable",
        "emit_event",
        &[],
    )
}

fn public_error_from_core(
    error: &Error,
    operation: &'static str,
    fields: &[(&str, TraceValue)],
) -> PublicApiError {
    let context = public_context(operation, fields);
    PublicApiError::from_error(error, vec![context], None)
        .expect("fixed desktop transport public error context should be valid")
}

fn route_project_execution(
    execution: LocalProjectExecution<ExecuteProjectCommandResult, ProjectStateChanged>,
    method: &str,
) -> Result<RoutedResponse, PublicApiError> {
    if execution.events().len() > 1 {
        return Err(transport_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "one project command produced more than one replacement event",
            "route_project_event",
            &[("method", method.into())],
        ));
    }
    let event = execution
        .events()
        .first()
        .map(|event| {
            serialize_response(event, PROJECT_STATE_CHANGED_EVENT)
                .map(|payload| (PROJECT_STATE_CHANGED_EVENT.to_owned(), payload))
        })
        .transpose()?;
    Ok(RoutedResponse {
        response: serialize_response(execution.result(), method)?,
        event,
    })
}

fn serialize_response<T: Serialize>(value: &T, method: &str) -> Result<Value, PublicApiError> {
    serde_json::to_value(value).map_err(|_| {
        transport_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "the generated response could not be serialized",
            "serialize_response",
            &[("method", method.into())],
        )
    })
}

fn public_error_from_project(
    failure: &DesktopProjectFailure,
    operation: &'static str,
) -> PublicApiError {
    let (category, recoverability) = match failure.class() {
        DesktopProjectFailureClass::Retryable => {
            (ErrorCategory::Unavailable, Recoverability::Retryable)
        }
        DesktopProjectFailureClass::Degraded => {
            (ErrorCategory::Unavailable, Recoverability::Degraded)
        }
        DesktopProjectFailureClass::UserCorrectable => {
            (ErrorCategory::InvalidInput, Recoverability::UserCorrectable)
        }
        DesktopProjectFailureClass::Terminal => (ErrorCategory::Internal, Recoverability::Terminal),
    };
    let error = Error::new(category, recoverability, failure.title().to_owned());
    public_error_from_core(
        &error,
        operation,
        &[
            ("code", failure.code().into()),
            ("action", failure.action().into()),
        ],
    )
}

fn transport_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
    fields: &[(&str, TraceValue)],
) -> PublicApiError {
    let error = Error::new(category, recoverability, message);
    public_error_from_core(&error, operation, fields)
}

fn public_context(operation: &'static str, fields: &[(&str, TraceValue)]) -> PublicErrorContext {
    let mut context = PublicErrorContext::reviewed(COMPONENT, format!("{COMPONENT}.{operation}"))
        .expect("fixed desktop transport context should be valid");
    for (name, value) in fields {
        context = context
            .with_field(*name, value.clone())
            .expect("fixed desktop transport field should be valid");
    }
    context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_error_projection_preserves_every_recoverability_and_safe_context() {
        for recoverability in Recoverability::ALL {
            let error = transport_error(
                ErrorCategory::Unavailable,
                *recoverability,
                "classification proof",
                "classify",
                &[("generation", 7_u64.into())],
            );
            assert_eq!(error.data().recoverability(), *recoverability);
            assert_eq!(error.data().contexts()[0].component(), COMPONENT);
            assert_eq!(
                error.data().contexts()[0].operation(),
                "superi.desktop.transport.classify"
            );
            assert_eq!(
                error.data().contexts()[0].fields().get("generation"),
                Some(&TraceValue::Unsigned(7))
            );
        }
    }

    #[test]
    fn cancellation_is_generation_scoped_and_idempotent() {
        let transport = DesktopTransportState::new();
        let DesktopTransportReply::Connected { generation, .. } = transport
            .dispatch_control(DesktopTransportCommand::Connect { after_sequence: 0 })
            .unwrap()
        else {
            panic!("connect returned an unexpected reply");
        };
        let token = transport
            .admit_request(generation, "request-1", INTEGRATION_VALIDATION_METHOD)
            .unwrap();

        let first = transport
            .dispatch_control(DesktopTransportCommand::Cancel {
                generation,
                request_id: "request-1".to_owned(),
            })
            .unwrap();
        let second = transport
            .dispatch_control(DesktopTransportCommand::Cancel {
                generation,
                request_id: "request-1".to_owned(),
            })
            .unwrap();

        assert!(token.load(Ordering::Acquire));
        assert!(matches!(
            first,
            DesktopTransportReply::Cancelled {
                cancelled: true,
                ..
            }
        ));
        assert!(matches!(
            second,
            DesktopTransportReply::Cancelled {
                cancelled: true,
                ..
            }
        ));
    }

    #[test]
    fn late_cancellation_cannot_hide_a_durable_project_commit() {
        assert!(!cancellation_wins_after_routing(
            EXECUTE_PROJECT_COMMAND_METHOD
        ));
        assert!(cancellation_wins_after_routing(
            INTEGRATION_VALIDATION_METHOD
        ));
        assert!(cancellation_wins_after_routing(GET_EDITOR_STATE_METHOD));
    }
}
