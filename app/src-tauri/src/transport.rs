//! Bounded command and ordered event transport above the managed engine connection.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use superi_api::commands::{ApiCommand, GetEditorState, GetEngineIntegrationValidation};
use superi_api::editor::ExecuteProjectCommand;
use superi_api::events::{ApiEvent, ProjectStateChanged};
use superi_api::playback::ExecutePlaybackTransport;
use superi_api::schema::{PublicApiError, PublicErrorContext};
use superi_core::diagnostics::TraceValue;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability};

use crate::engine::EngineConnection;
use crate::project_lifecycle::{
    DesktopProjectFailure, DesktopProjectFailureClass, DesktopProjectState,
};

/// One Tauri event carries every generated public API event.
pub const DESKTOP_API_EVENT: &str = "superi://api-event";

const COMPONENT: &str = "superi.desktop.transport";
const STREAM_ID: &str = "superi.desktop.events.v1";
const ENGINE_INTROSPECTION_EVENT: &str = "superi.engine.introspection.changed";
const MAX_PENDING_REQUESTS: usize = 32;
const RETAINED_EVENTS: usize = 64;
const ENGINE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const EDITOR_STATE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

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

/// One dispatcher outcome and its ordered event side effects.
pub struct DesktopTransportOutcome {
    reply: DesktopTransportReply,
    events: Vec<DesktopTransportEmission>,
}

impl DesktopTransportOutcome {
    #[cfg(test)]
    pub(crate) fn into_parts(self) -> (DesktopTransportReply, Vec<DesktopEventEnvelope>) {
        (
            self.reply,
            self.events
                .into_iter()
                .map(|emission| emission.envelope)
                .collect(),
        )
    }

    pub(crate) fn into_targeted_parts(
        self,
    ) -> (DesktopTransportReply, Vec<DesktopTransportEmission>) {
        (self.reply, self.events)
    }

    /// Returns the generated response or control reply.
    #[must_use]
    pub const fn reply(&self) -> &DesktopTransportReply {
        &self.reply
    }

    /// Returns the ordered replacement event produced by this request, if any.
    #[must_use]
    pub fn event(&self) -> Option<&DesktopEventEnvelope> {
        self.events.first().map(|emission| &emission.envelope)
    }
}

/// One ordered event projected for one connected webview generation.
pub struct DesktopTransportEmission {
    client_id: String,
    envelope: DesktopEventEnvelope,
}

impl DesktopTransportEmission {
    pub(crate) fn client_id(&self) -> &str {
        &self.client_id
    }

    pub(crate) const fn envelope(&self) -> &DesktopEventEnvelope {
        &self.envelope
    }
}

struct RoutedResponse {
    response: Value,
    events: Vec<(String, Value)>,
}

struct DesktopRequestDispatch {
    generation: u64,
    request_id: String,
    method: String,
    request: Value,
}

#[derive(Clone, Debug)]
struct RetainedDesktopEvent {
    sequence: u64,
    event: String,
    payload: Value,
}

impl RetainedDesktopEvent {
    fn envelope(&self, generation: u64) -> DesktopEventEnvelope {
        DesktopEventEnvelope {
            generation,
            stream_id: STREAM_ID.to_owned(),
            sequence: self.sequence,
            event: self.event.clone(),
            payload: self.payload.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DesktopClientConnection {
    generation: u64,
    connected: bool,
}

struct DesktopTransportInner {
    next_generation: u64,
    sequence: u64,
    retained: VecDeque<RetainedDesktopEvent>,
    clients: HashMap<String, DesktopClientConnection>,
    pending: HashMap<(String, u64, String), Arc<AtomicBool>>,
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
                next_generation: 0,
                sequence: 0,
                retained: VecDeque::with_capacity(RETAINED_EVENTS),
                clients: HashMap::new(),
                pending: HashMap::with_capacity(MAX_PENDING_REQUESTS),
            })),
        }
    }

    /// Executes a nonblocking connection, cancellation, or disconnect control command.
    pub fn dispatch_control(
        &self,
        command: DesktopTransportCommand,
    ) -> Result<DesktopTransportReply, PublicApiError> {
        self.dispatch_control_for("main", command)
    }

    /// Executes one control command for the invoking editor webview.
    pub fn dispatch_control_for(
        &self,
        client_id: &str,
        command: DesktopTransportCommand,
    ) -> Result<DesktopTransportReply, PublicApiError> {
        validate_client_id(client_id)?;
        match command {
            DesktopTransportCommand::Connect { after_sequence } => {
                self.connect(client_id, after_sequence)
            }
            DesktopTransportCommand::Cancel {
                generation,
                request_id,
            } => self.cancel(client_id, generation, request_id),
            DesktopTransportCommand::Disconnect { generation } => {
                self.disconnect(client_id, generation)
            }
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
        self.dispatch_blocking_for("main", engine, projects, command)
    }

    /// Executes one command for the invoking editor webview on a blocking-safe worker.
    pub fn dispatch_blocking_for(
        &self,
        client_id: &str,
        engine: &EngineConnection,
        projects: &DesktopProjectState,
        command: DesktopTransportCommand,
    ) -> Result<DesktopTransportOutcome, PublicApiError> {
        validate_client_id(client_id)?;
        match command {
            DesktopTransportCommand::Request {
                generation,
                request_id,
                method,
                request,
            } => self.request(
                client_id,
                projects,
                engine,
                DesktopRequestDispatch {
                    generation,
                    request_id,
                    method,
                    request,
                },
            ),
            control => {
                self.dispatch_control_for(client_id, control)
                    .map(|reply| DesktopTransportOutcome {
                        reply,
                        events: Vec::new(),
                    })
            }
        }
    }

    fn connect(
        &self,
        client_id: &str,
        after_sequence: u64,
    ) -> Result<DesktopTransportReply, PublicApiError> {
        let mut inner = self.lock("connect")?;
        for ((pending_client, _, _), token) in &inner.pending {
            if pending_client == client_id {
                token.store(true, Ordering::Release);
            }
        }
        inner
            .pending
            .retain(|(pending_client, _, _), _| pending_client != client_id);
        inner.next_generation = inner.next_generation.checked_add(1).ok_or_else(|| {
            transport_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "the desktop transport generation was exhausted",
                "connect",
                &[],
            )
        })?;
        let generation = inner.next_generation;
        inner.clients.insert(
            client_id.to_owned(),
            DesktopClientConnection {
                generation,
                connected: true,
            },
        );

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
                .map(|event| event.envelope(generation))
                .collect()
        };

        Ok(DesktopTransportReply::Connected {
            generation,
            stream_id: STREAM_ID.to_owned(),
            replay,
            resync_required,
        })
    }

    fn cancel(
        &self,
        client_id: &str,
        generation: u64,
        request_id: String,
    ) -> Result<DesktopTransportReply, PublicApiError> {
        let inner = self.lock("cancel")?;
        let cancelled = inner
            .pending
            .get(&(client_id.to_owned(), generation, request_id.clone()))
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

    fn disconnect(
        &self,
        client_id: &str,
        generation: u64,
    ) -> Result<DesktopTransportReply, PublicApiError> {
        let mut inner = self.lock("disconnect")?;
        if inner
            .clients
            .get(client_id)
            .is_some_and(|client| client.generation == generation)
        {
            if let Some(client) = inner.clients.get_mut(client_id) {
                client.connected = false;
            }
            for ((pending_client, pending_generation, _), token) in &inner.pending {
                if pending_client == client_id && *pending_generation == generation {
                    token.store(true, Ordering::Release);
                }
            }
            inner
                .pending
                .retain(|(pending_client, pending_generation, _), _| {
                    pending_client != client_id || *pending_generation != generation
                });
        }
        Ok(DesktopTransportReply::Disconnected { generation })
    }

    /// Removes a closing editor webview and cancels only its pending work.
    pub fn disconnect_client(&self, client_id: &str) -> Result<(), PublicApiError> {
        validate_client_id(client_id)?;
        let mut inner = self.lock("disconnect_client")?;
        if let Some(client) = inner.clients.remove(client_id) {
            for ((pending_client, pending_generation, _), token) in &inner.pending {
                if pending_client == client_id && *pending_generation == client.generation {
                    token.store(true, Ordering::Release);
                }
            }
            inner
                .pending
                .retain(|(pending_client, _, _), _| pending_client != client_id);
        }
        Ok(())
    }

    #[cfg(test)]
    fn client_is_connected(&self, client_id: &str, generation: u64) -> bool {
        self.inner.lock().is_ok_and(|inner| {
            inner
                .clients
                .get(client_id)
                .is_some_and(|client| client.connected && client.generation == generation)
        })
    }

    fn request(
        &self,
        client_id: &str,
        project: &DesktopProjectState,
        engine: &EngineConnection,
        dispatch: DesktopRequestDispatch,
    ) -> Result<DesktopTransportOutcome, PublicApiError> {
        let DesktopRequestDispatch {
            generation,
            request_id,
            method,
            request,
        } = dispatch;
        self.validate_request(client_id, generation, &request_id, &method, &request)?;
        let token = self.admit_request(client_id, generation, &request_id, &method)?;
        if token.load(Ordering::Acquire) {
            self.remove_pending(client_id, generation, &request_id, &token)?;
            return Err(cancelled_request_error(generation, &request_id, &method));
        }
        let result = self.route_request(project, engine, &method, request);
        self.remove_pending(client_id, generation, &request_id, &token)?;

        if token.load(Ordering::Acquire) && cancellation_wins_after_routing(&method) {
            return Err(cancelled_request_error(generation, &request_id, &method));
        }

        let routed = result.map_err(|error| {
            public_error_from_core(
                &error,
                "request",
                &[
                    ("generation", generation.into()),
                    ("method", method.as_str().into()),
                    ("request_id", request_id.as_str().into()),
                ],
            )
        })?;
        let mut events = Vec::with_capacity(routed.events.len());
        for (event, payload) in routed.events {
            events.extend(self.publish_event(client_id, event, payload)?);
        }
        Ok(DesktopTransportOutcome {
            reply: DesktopTransportReply::Response {
                generation,
                request_id,
                response: routed.response,
            },
            events,
        })
    }

    fn route_request(
        &self,
        project: &DesktopProjectState,
        engine: &EngineConnection,
        method: &str,
        request: Value,
    ) -> Result<RoutedResponse, Error> {
        match method {
            GetEngineIntegrationValidation::METHOD => {
                let result = engine
                    .request_integration_validation()?
                    .wait(ENGINE_RESPONSE_TIMEOUT)?;
                Ok(RoutedResponse {
                    response: serialize_generated_response(&result)?,
                    events: vec![(
                        ENGINE_INTROSPECTION_EVENT.to_owned(),
                        serde_json::json!({ "snapshot": result.snapshot().engine() }),
                    )],
                })
            }
            GetEditorState::METHOD => {
                let request = decode_generated_request::<GetEditorState>(request)?;
                let lease = project
                    .begin_editor_request()
                    .map_err(project_failure_error)?;
                let route = lease.route().clone();
                let result = engine
                    .request_editor_state(route, request)?
                    .wait(EDITOR_STATE_RESPONSE_TIMEOUT)?;
                drop(lease);
                Ok(RoutedResponse {
                    response: serialize_generated_response(&result)?,
                    events: Vec::new(),
                })
            }
            ExecuteProjectCommand::METHOD => {
                let request = decode_generated_request::<ExecuteProjectCommand>(request)?;
                let lease = project
                    .begin_editor_request()
                    .map_err(project_failure_error)?;
                let route = lease.route().clone();
                let output = engine.request_project_command(route, request)?.wait()?;
                lease
                    .accept(output.identity().clone())
                    .map_err(project_failure_error)?;
                let events = output
                    .events()
                    .iter()
                    .map(|event| {
                        Ok((
                            ProjectStateChanged::NAME.to_owned(),
                            serialize_generated_response(event)?,
                        ))
                    })
                    .collect::<std::result::Result<Vec<_>, Error>>()?;
                Ok(RoutedResponse {
                    response: serialize_generated_response(output.result())?,
                    events,
                })
            }
            ExecutePlaybackTransport::METHOD => {
                let request = decode_generated_request::<ExecutePlaybackTransport>(request)?;
                let lease = project
                    .begin_editor_request()
                    .map_err(project_failure_error)?;
                let route = lease.route().clone();
                let result = engine
                    .request_playback_transport(route, request)?
                    .wait(EDITOR_STATE_RESPONSE_TIMEOUT)?;
                drop(lease);
                Ok(RoutedResponse {
                    response: serialize_generated_response(&result)?,
                    events: Vec::new(),
                })
            }
            _ => Err(engine_transport_error(
                ErrorCategory::Unsupported,
                Recoverability::Degraded,
                "this generated method has no desktop engine route yet",
                "route_request",
            )),
        }
    }

    fn validate_request(
        &self,
        client_id: &str,
        generation: u64,
        request_id: &str,
        method: &str,
        request: &Value,
    ) -> Result<(), PublicApiError> {
        let inner = self.lock("validate_request")?;
        if !inner
            .clients
            .get(client_id)
            .is_some_and(|client| client.connected && client.generation == generation)
        {
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
        if method != GetEngineIntegrationValidation::METHOD
            && method != GetEditorState::METHOD
            && method != ExecuteProjectCommand::METHOD
            && method != ExecutePlaybackTransport::METHOD
        {
            return Err(transport_error(
                ErrorCategory::Unsupported,
                Recoverability::Degraded,
                "this generated method has no desktop engine route yet",
                "route_request",
                &[("method", method.into())],
            ));
        }
        let valid_request = if method == GetEngineIntegrationValidation::METHOD {
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
        client_id: &str,
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
        let key = (client_id.to_owned(), generation, request_id.to_owned());
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
        client_id: &str,
        generation: u64,
        request_id: &str,
        token: &Arc<AtomicBool>,
    ) -> Result<(), PublicApiError> {
        let mut inner = self.lock("complete_request")?;
        let key = (client_id.to_owned(), generation, request_id.to_owned());
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
        client_id: &str,
        event_name: String,
        payload: Value,
    ) -> Result<Vec<DesktopTransportEmission>, PublicApiError> {
        let mut inner = self.lock("publish_event")?;
        inner.sequence = inner.sequence.checked_add(1).ok_or_else(|| {
            transport_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "the desktop event sequence was exhausted",
                "publish_event",
                &[],
            )
        })?;
        let event = RetainedDesktopEvent {
            sequence: inner.sequence,
            event: event_name,
            payload,
        };
        if inner.retained.len() == RETAINED_EVENTS {
            inner.retained.pop_front();
        }
        inner.retained.push_back(event.clone());
        let mut clients = inner
            .clients
            .iter()
            .filter(|(_, client)| client.connected)
            .map(|(target, client)| (target.clone(), client.generation))
            .collect::<Vec<_>>();
        clients.sort_by(|left, right| {
            let left_order = usize::from(left.0 != client_id);
            let right_order = usize::from(right.0 != client_id);
            (left_order, left.0.as_str()).cmp(&(right_order, right.0.as_str()))
        });
        Ok(clients
            .into_iter()
            .map(|(target, target_generation)| DesktopTransportEmission {
                client_id: target,
                envelope: event.envelope(target_generation),
            })
            .collect())
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

fn validate_client_id(client_id: &str) -> Result<(), PublicApiError> {
    if client_id.is_empty()
        || client_id.len() > 128
        || !client_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
    {
        return Err(transport_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "the desktop transport client identity is invalid",
            "validate_client",
            &[],
        ));
    }
    Ok(())
}

fn cancellation_wins_after_routing(method: &str) -> bool {
    // A durable authored command cannot be abandoned after commit. Its response and
    // replacement event must win a late cancellation so the caller can reconcile.
    method != ExecuteProjectCommand::METHOD && method != ExecutePlaybackTransport::METHOD
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

fn decode_generated_request<T: DeserializeOwned>(request: Value) -> Result<T, Error> {
    serde_json::from_value(request).map_err(|source| {
        Error::with_source(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "the generated desktop request payload could not be decoded",
            source,
        )
        .with_context(ErrorContext::new(COMPONENT, "decode_request"))
    })
}

fn serialize_generated_response<T: Serialize>(response: &T) -> Result<Value, Error> {
    serde_json::to_value(response).map_err(|source| {
        Error::with_source(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "the generated desktop response could not be serialized",
            source,
        )
        .with_context(ErrorContext::new(COMPONENT, "serialize_response"))
    })
}

fn project_failure_error(failure: DesktopProjectFailure) -> Error {
    let (category, recoverability) = match failure.class() {
        DesktopProjectFailureClass::Retryable => {
            (ErrorCategory::Unavailable, Recoverability::Retryable)
        }
        DesktopProjectFailureClass::Degraded => {
            (ErrorCategory::Unavailable, Recoverability::Degraded)
        }
        DesktopProjectFailureClass::UserCorrectable => {
            (ErrorCategory::Conflict, Recoverability::UserCorrectable)
        }
        DesktopProjectFailureClass::Terminal => (ErrorCategory::Internal, Recoverability::Terminal),
    };
    Error::new(category, recoverability, failure.title().to_owned()).with_context(
        ErrorContext::new(COMPONENT, "project_route")
            .with_field("code", failure.code().to_owned())
            .with_field("action", failure.action().to_owned()),
    )
}

fn engine_transport_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
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
    let operation = format!("{COMPONENT}.{operation}");
    let mut context = PublicErrorContext::reviewed(COMPONENT, operation)
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
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use superi_api::commands::GetEditorStateResult;
    use superi_api::editor::{
        EditorTrackItem, ExactDuration, ExactTime, ExactTimeRange, ExecuteProjectCommandResult,
        ProjectAction, ProjectCommand, TimelineEditOperation,
    };
    use superi_api::playback::{
        ExecutePlaybackTransport, ExecutePlaybackTransportResult, PlaybackTransportAction,
    };
    use superi_engine::editor;
    use superi_engine::editor::ProjectDatabase;

    use crate::engine::LinkedEngineProcess;
    use crate::lifecycle::{ApplicationLifecycle, ApplicationLifecyclePhase};
    use crate::project_lifecycle::{DesktopProjectCommand, DesktopProjectState};

    use super::*;

    const PROJECT: editor::ProjectId = editor::ProjectId::from_raw(0xe001);
    const ROOT: editor::TimelineId = editor::TimelineId::from_raw(0xe002);
    const TRACK: editor::TrackId = editor::TrackId::from_raw(0xe003);
    const INITIAL_GAP: editor::GapId = editor::GapId::from_raw(0xe004);
    const APPENDED_GAP: editor::GapId = editor::GapId::from_raw(0xe005);

    struct TemporaryTransportProject {
        root: PathBuf,
        path: PathBuf,
    }

    impl TemporaryTransportProject {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time is available")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "superi-desktop-transport-{}-{nonce}",
                std::process::id()
            ));
            Self {
                path: root.join("transport.superi"),
                root,
            }
        }
    }

    impl Drop for TemporaryTransportProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn exact_range(start: i64, duration: u64) -> editor::TimeRange {
        let timebase = editor::FrameRate::FPS_24.timebase();
        editor::TimeRange::new(
            editor::RationalTime::new(start, timebase),
            editor::Duration::new(duration, timebase).unwrap(),
        )
        .unwrap()
    }

    fn project_document() -> editor::ProjectDocument {
        let timebase = editor::FrameRate::FPS_24.timebase();
        let timeline = editor::Timeline::new(
            ROOT,
            "Transport timeline",
            timebase,
            editor::RationalTime::zero(timebase),
            vec![editor::Track::new(
                TRACK,
                "V1",
                editor::TrackSemantics::Video(editor::VideoTrackSemantics::new(
                    editor::FrameRate::FPS_24,
                    editor::VideoCompositing::Over,
                )),
                vec![editor::TrackItem::Gap(editor::Gap::new(
                    INITIAL_GAP,
                    "Initial gap",
                    exact_range(0, 24),
                ))],
            )],
        );
        let editorial = editor::EditorialProject::new(
            PROJECT,
            "Transport project",
            std::iter::empty::<editor::LinkedMediaReference>(),
            [timeline],
        )
        .unwrap();
        editor::ProjectDocument::new(editorial, ROOT).unwrap()
    }

    fn public_range(start: i64, duration: u64) -> ExactTimeRange {
        let timebase = superi_api::editor::ExactTimebase {
            numerator: 24,
            denominator: 1,
        };
        ExactTimeRange {
            start: ExactTime {
                value: start,
                timebase,
            },
            duration: ExactDuration {
                value: duration,
                timebase,
            },
        }
    }

    fn wait_for_running_generation(lifecycle: &ApplicationLifecycle) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = lifecycle.snapshot();
            if snapshot.application_phase() == ApplicationLifecyclePhase::Running
                && snapshot.engine_generation() == 1
            {
                return;
            }
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("linked engine lifecycle transition timed out");
            lifecycle
                .wait_for_change(snapshot.revision(), remaining)
                .expect("linked engine lifecycle wait should succeed");
        }
    }

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
            .admit_request(
                "main",
                generation,
                "request-1",
                GetEngineIntegrationValidation::METHOD,
            )
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
    fn webview_clients_keep_independent_generations_and_receive_one_ordered_event() {
        let transport = DesktopTransportState::new();
        let DesktopTransportReply::Connected {
            generation: main_generation,
            ..
        } = transport
            .dispatch_control_for(
                "main",
                DesktopTransportCommand::Connect { after_sequence: 0 },
            )
            .unwrap()
        else {
            panic!("main connect returned an unexpected reply");
        };
        let DesktopTransportReply::Connected {
            generation: workspace_generation,
            ..
        } = transport
            .dispatch_control_for(
                "workspace-1",
                DesktopTransportCommand::Connect { after_sequence: 0 },
            )
            .unwrap()
        else {
            panic!("workspace connect returned an unexpected reply");
        };

        assert_ne!(main_generation, workspace_generation);
        assert!(transport.client_is_connected("main", main_generation));
        assert!(transport.client_is_connected("workspace-1", workspace_generation));
        let emissions = transport
            .publish_event(
                "main",
                "superi.project.state.changed".to_owned(),
                serde_json::json!({"project_revision": 9}),
            )
            .unwrap();
        assert_eq!(emissions.len(), 2);
        assert_eq!(emissions[0].client_id, "main");
        assert_eq!(emissions[0].envelope.generation(), main_generation);
        assert_eq!(emissions[1].client_id, "workspace-1");
        assert_eq!(emissions[1].envelope.generation(), workspace_generation);
        assert_eq!(
            emissions[0].envelope.sequence(),
            emissions[1].envelope.sequence()
        );

        let main_token = transport
            .admit_request(
                "main",
                main_generation,
                "shared-request",
                GetEngineIntegrationValidation::METHOD,
            )
            .unwrap();
        let workspace_token = transport
            .admit_request(
                "workspace-1",
                workspace_generation,
                "shared-request",
                GetEngineIntegrationValidation::METHOD,
            )
            .unwrap();
        transport.disconnect_client("workspace-1").unwrap();
        assert!(transport.client_is_connected("main", main_generation));
        assert!(!transport.client_is_connected("workspace-1", workspace_generation));
        assert!(!main_token.load(Ordering::Acquire));
        assert!(workspace_token.load(Ordering::Acquire));

        let after_close = transport
            .publish_event(
                "workspace-1",
                "superi.project.state.changed".to_owned(),
                serde_json::json!({"project_revision": 10}),
            )
            .unwrap();
        assert_eq!(after_close.len(), 1);
        assert_eq!(after_close[0].client_id, "main");
        assert_eq!(
            after_close[0].envelope.sequence(),
            emissions[0].envelope.sequence() + 1
        );
        transport
            .remove_pending("main", main_generation, "shared-request", &main_token)
            .unwrap();
    }

    #[test]
    fn late_cancellation_cannot_hide_a_durable_project_commit() {
        assert!(!cancellation_wins_after_routing(
            ExecuteProjectCommand::METHOD
        ));
        assert!(!cancellation_wins_after_routing(
            ExecutePlaybackTransport::METHOD
        ));
        assert!(cancellation_wins_after_routing(
            GetEngineIntegrationValidation::METHOD
        ));
        assert!(cancellation_wins_after_routing(GetEditorState::METHOD));
    }

    #[test]
    fn desktop_transport_routes_editor_state_and_durable_edit_events_end_to_end() {
        let temporary = TemporaryTransportProject::new();
        fs::create_dir_all(&temporary.root).unwrap();
        let initial = project_document().snapshot();
        let mut database = ProjectDatabase::create(&temporary.path).unwrap();
        database.replace(&initial).unwrap();
        drop(database);

        let project = DesktopProjectState::default();
        project.initialize(temporary.root.join("recovery")).unwrap();
        project
            .execute(DesktopProjectCommand::Open {
                path: temporary.path.to_string_lossy().into_owned(),
            })
            .unwrap();

        let lifecycle = ApplicationLifecycle::new().unwrap();
        let process = LinkedEngineProcess::launch(lifecycle.clone()).unwrap();
        wait_for_running_generation(&lifecycle);
        let connection = process.connection();
        let transport = DesktopTransportState::new();
        let DesktopTransportReply::Connected { generation, .. } = transport
            .dispatch_control(DesktopTransportCommand::Connect { after_sequence: 0 })
            .unwrap()
        else {
            panic!("connect returned an unexpected reply");
        };

        let state_outcome = transport
            .dispatch_blocking(
                &connection,
                &project,
                DesktopTransportCommand::Request {
                    generation,
                    request_id: "state-0".to_owned(),
                    method: GetEditorState::METHOD.to_owned(),
                    request: serde_json::to_value(GetEditorState::new("state-0")).unwrap(),
                },
            )
            .unwrap();
        let (state_reply, state_events) = state_outcome.into_parts();
        assert!(state_events.is_empty());
        let DesktopTransportReply::Response { response, .. } = state_reply else {
            panic!("editor state returned an unexpected reply");
        };
        let state: GetEditorStateResult = serde_json::from_value(response).unwrap();
        assert_eq!(state.snapshot().project().project_revision(), 0);

        let playback_outcome = transport
            .dispatch_blocking(
                &connection,
                &project,
                DesktopTransportCommand::Request {
                    generation,
                    request_id: "play-1".to_owned(),
                    method: ExecutePlaybackTransport::METHOD.to_owned(),
                    request: serde_json::to_value(ExecutePlaybackTransport::new(
                        "play-1",
                        PlaybackTransportAction::Play {},
                    ))
                    .unwrap(),
                },
            )
            .unwrap();
        assert!(playback_outcome.event().is_none());
        let DesktopTransportReply::Response { response, .. } = playback_outcome.reply() else {
            panic!("playback transport returned an unexpected reply")
        };
        let accepted: ExecutePlaybackTransportResult =
            serde_json::from_value(response.clone()).unwrap();
        assert!(accepted.accepted());
        assert!(accepted.pending_command());

        let mut observed_playback = None;
        for attempt in 0..100 {
            let request_id = format!("state-playback-{attempt}");
            let outcome = transport
                .dispatch_blocking(
                    &connection,
                    &project,
                    DesktopTransportCommand::Request {
                        generation,
                        request_id: request_id.clone(),
                        method: GetEditorState::METHOD.to_owned(),
                        request: serde_json::to_value(GetEditorState::new(request_id)).unwrap(),
                    },
                )
                .unwrap();
            let DesktopTransportReply::Response { response, .. } = outcome.reply() else {
                panic!("playback state returned an unexpected reply")
            };
            let value = response.clone();
            if value["snapshot"]["playback"]["pending_command"] == false {
                observed_playback = Some(value);
                break;
            }
            std::thread::yield_now();
        }
        let observed_playback = observed_playback.expect("playback command must complete");
        assert_eq!(
            observed_playback["snapshot"]["playback"]["latest"]["mode"],
            "playing"
        );
        assert!(
            observed_playback["snapshot"]["playback"]["latest"]["degradation"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "viewport_output_unavailable")
        );

        let command = ExecuteProjectCommand::new(
            "append-through-transport",
            0,
            ProjectCommand::Apply {
                actions: vec![ProjectAction::EditTimeline {
                    operations: vec![TimelineEditOperation::Append {
                        timeline_id: ROOT.to_string(),
                        track_id: TRACK.to_string(),
                        material: EditorTrackItem::Gap {
                            id: APPENDED_GAP.to_string(),
                            name: "Appended through transport".to_owned(),
                            record_range: public_range(0, 12),
                        },
                    }],
                }],
            },
        );
        let command_outcome = transport
            .dispatch_blocking(
                &connection,
                &project,
                DesktopTransportCommand::Request {
                    generation,
                    request_id: "append-1".to_owned(),
                    method: ExecuteProjectCommand::METHOD.to_owned(),
                    request: serde_json::to_value(command).unwrap(),
                },
            )
            .unwrap();
        let (command_reply, command_events) = command_outcome.into_parts();
        let DesktopTransportReply::Response { response, .. } = command_reply else {
            panic!("project command returned an unexpected reply");
        };
        let result: ExecuteProjectCommandResult = serde_json::from_value(response).unwrap();
        assert_eq!(result.state().project_revision(), 1);
        assert_eq!(result.state().undo_depth(), 1);
        assert_eq!(command_events.len(), 1);
        assert_eq!(command_events[0].event(), ProjectStateChanged::NAME);
        let event: ProjectStateChanged =
            serde_json::from_value(command_events[0].payload().clone()).unwrap();
        assert_eq!(event.transaction_id(), "append-through-transport");
        assert_eq!(event.project_revision(), 1);
        assert_eq!(
            project
                .snapshot()
                .unwrap()
                .active()
                .unwrap()
                .project_revision(),
            1
        );

        let reopened = ProjectDatabase::open_read_only(&temporary.path)
            .unwrap()
            .load()
            .unwrap();
        assert_eq!(
            reopened
                .editorial_project()
                .timeline(ROOT)
                .unwrap()
                .track(TRACK)
                .unwrap()
                .items()
                .len(),
            2
        );

        lifecycle.request_shutdown().unwrap();
        process.join().unwrap();
    }
}
