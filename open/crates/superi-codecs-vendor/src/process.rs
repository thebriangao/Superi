use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::thread;
use std::time::Duration;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::operation::OperationContext;

use crate::protocol::{Envelope, ErrorWire, ProtocolRequest, ProtocolResponse};

const WAIT_SLICE: Duration = Duration::from_millis(10);
const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_MAXIMUM_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

/// Explicit configuration for one separately installed vendor RAW worker executable.
#[derive(Clone, Debug)]
pub struct VendorPluginConfig {
    executable: PathBuf,
    arguments: Vec<OsString>,
    startup_timeout: Duration,
    maximum_message_bytes: usize,
}

impl VendorPluginConfig {
    /// Creates an explicit worker configuration with no arguments or inherited environment.
    #[must_use]
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            arguments: Vec::new(),
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            maximum_message_bytes: DEFAULT_MAXIMUM_MESSAGE_BYTES,
        }
    }

    /// Replaces worker command-line arguments.
    #[must_use]
    pub fn with_arguments<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.arguments = arguments.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the bounded startup and handshake deadline.
    pub fn with_startup_timeout(mut self, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            return Err(invalid_config(
                "set_startup_timeout",
                "vendor plugin startup timeout must be greater than zero",
            ));
        }
        self.startup_timeout = timeout;
        Ok(self)
    }

    /// Sets the maximum serialized request or response line size.
    pub fn with_maximum_message_bytes(mut self, maximum: usize) -> Result<Self> {
        if maximum == 0 {
            return Err(invalid_config(
                "set_maximum_message_bytes",
                "vendor plugin message limit must be greater than zero",
            ));
        }
        self.maximum_message_bytes = maximum;
        Ok(self)
    }

    pub(crate) fn startup_operation(
        &self,
        operation: &OperationContext,
    ) -> Result<OperationContext> {
        let timeout = operation
            .remaining()
            .map(|remaining| remaining.min(self.startup_timeout))
            .unwrap_or(self.startup_timeout);
        OperationContext::new(operation.priority())
            .with_cancellation(operation.cancellation_token().clone())
            .with_timeout(timeout)
    }
}

pub(crate) struct ProcessClient {
    state: Mutex<ProcessState>,
    next_id: AtomicU64,
    maximum_message_bytes: usize,
}

struct ProcessState {
    child: Child,
    writer: SyncSender<WriteCommand>,
    reader: Receiver<ReadEvent>,
    terminated: bool,
}

struct WriteCommand {
    bytes: Vec<u8>,
    completion: Sender<io::Result<()>>,
}

enum ReadEvent {
    Line(Vec<u8>),
    EndOfStream,
    Io(io::Error),
    Unterminated,
    TooLarge,
}

impl ProcessClient {
    pub(crate) fn start(
        config: &VendorPluginConfig,
        operation: &OperationContext,
    ) -> Result<Arc<Self>> {
        operation.check("start_vendor_plugin")?;
        let executable = canonical_executable(&config.executable)?;
        let mut command = Command::new(&executable);
        command
            .args(&config.arguments)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(parent) = executable.parent() {
            command.current_dir(parent);
        }
        let mut child = command.spawn().map_err(|source| {
            Error::with_source(
                ErrorCategory::Unavailable,
                Recoverability::UserCorrectable,
                "vendor plugin executable could not be started",
                source,
            )
            .with_context(
                ErrorContext::new("superi-codecs-vendor.process", "start_vendor_plugin")
                    .with_field("executable", executable.display().to_string()),
            )
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| protocol_error("start_vendor_plugin", "worker stdin was not created"))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            protocol_error("start_vendor_plugin", "worker stdout was not created")
        })?;

        let (writer, writer_commands) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("superi-vendor-plugin-writer".to_owned())
            .spawn(move || writer_loop(stdin, writer_commands))
            .map_err(|source| {
                let _ = child.kill();
                Error::with_source(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::Retryable,
                    "vendor plugin writer thread could not be created",
                    source,
                )
                .with_context(ErrorContext::new(
                    "superi-codecs-vendor.process",
                    "start_vendor_plugin",
                ))
            })?;
        let (read_events, reader) = mpsc::sync_channel(1);
        let maximum_message_bytes = config.maximum_message_bytes;
        thread::Builder::new()
            .name("superi-vendor-plugin-reader".to_owned())
            .spawn(move || reader_loop(BufReader::new(stdout), read_events, maximum_message_bytes))
            .map_err(|source| {
                let _ = child.kill();
                Error::with_source(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::Retryable,
                    "vendor plugin reader thread could not be created",
                    source,
                )
                .with_context(ErrorContext::new(
                    "superi-codecs-vendor.process",
                    "start_vendor_plugin",
                ))
            })?;

        let client = Arc::new(Self {
            state: Mutex::new(ProcessState {
                child,
                writer,
                reader,
                terminated: false,
            }),
            next_id: AtomicU64::new(1),
            maximum_message_bytes,
        });
        Ok(client)
    }

    pub(crate) fn request(
        &self,
        payload: ProtocolRequest,
        operation: &OperationContext,
    ) -> Result<ProtocolResponse> {
        operation.check("call_vendor_plugin")?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if id == u64::MAX {
            return Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "vendor plugin request identifier space is exhausted",
            )
            .with_context(ErrorContext::new(
                "superi-codecs-vendor.process",
                "call_vendor_plugin",
            )));
        }
        let envelope = Envelope { id, payload };
        let mut bytes = serde_json::to_vec(&envelope).map_err(|source| {
            Error::with_source(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "vendor plugin request could not be serialized",
                source,
            )
            .with_context(ErrorContext::new(
                "superi-codecs-vendor.process",
                "serialize_vendor_request",
            ))
        })?;
        if bytes.len() > self.maximum_message_bytes {
            return Err(message_too_large("serialize_vendor_request", bytes.len()));
        }
        bytes.push(b'\n');

        let mut state = self.lock_state(operation)?;
        if state.terminated {
            return Err(unavailable(
                "call_vendor_plugin",
                "vendor plugin worker is not running",
            ));
        }
        let (completion, written) = mpsc::channel();
        if state
            .writer
            .send(WriteCommand { bytes, completion })
            .is_err()
        {
            state.terminate();
            return Err(unavailable(
                "write_vendor_request",
                "vendor plugin writer stopped",
            ));
        }
        match wait_for(&written, operation, "write_vendor_request") {
            Ok(Ok(())) => {}
            Ok(Err(source)) => {
                state.terminate();
                return Err(Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "vendor plugin request could not be written",
                    source,
                )
                .with_context(ErrorContext::new(
                    "superi-codecs-vendor.process",
                    "write_vendor_request",
                )));
            }
            Err(error) => {
                state.terminate();
                return Err(error);
            }
        }

        let event = match wait_for(&state.reader, operation, "read_vendor_response") {
            Ok(event) => event,
            Err(error) => {
                state.terminate();
                return Err(error);
            }
        };
        let line = match event {
            ReadEvent::Line(line) => line,
            ReadEvent::EndOfStream => {
                state.terminate();
                return Err(unavailable(
                    "read_vendor_response",
                    "vendor plugin closed its protocol stream",
                ));
            }
            ReadEvent::Io(source) => {
                state.terminate();
                return Err(Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "vendor plugin response could not be read",
                    source,
                )
                .with_context(ErrorContext::new(
                    "superi-codecs-vendor.process",
                    "read_vendor_response",
                )));
            }
            ReadEvent::Unterminated => {
                state.terminate();
                return Err(protocol_error(
                    "read_vendor_response",
                    "vendor plugin response ended before its newline delimiter",
                ));
            }
            ReadEvent::TooLarge => {
                state.terminate();
                return Err(message_too_large(
                    "read_vendor_response",
                    self.maximum_message_bytes + 1,
                ));
            }
        };
        let response: Envelope<ProtocolResponse> = match serde_json::from_slice(&line) {
            Ok(response) => response,
            Err(source) => {
                state.terminate();
                return Err(Error::with_source(
                    ErrorCategory::CorruptData,
                    Recoverability::Terminal,
                    "vendor plugin returned invalid protocol JSON",
                    source,
                )
                .with_context(ErrorContext::new(
                    "superi-codecs-vendor.process",
                    "parse_vendor_response",
                )));
            }
        };
        if response.id != id {
            state.terminate();
            return Err(protocol_error(
                "parse_vendor_response",
                "vendor plugin response identifier does not match its request",
            ));
        }
        match response.payload {
            ProtocolResponse::Failure { error } => match worker_error(error) {
                Ok(error) => Err(error),
                Err(error) => {
                    state.terminate();
                    Err(error)
                }
            },
            response => Ok(response),
        }
    }

    fn lock_state(&self, operation: &OperationContext) -> Result<MutexGuard<'_, ProcessState>> {
        loop {
            operation.check("wait_for_vendor_plugin")?;
            match self.state.try_lock() {
                Ok(state) => return Ok(state),
                Err(TryLockError::WouldBlock) => thread::sleep(WAIT_SLICE),
                Err(TryLockError::Poisoned(_)) => {
                    return Err(Error::new(
                        ErrorCategory::Internal,
                        Recoverability::Terminal,
                        "vendor plugin process lock is poisoned",
                    )
                    .with_context(ErrorContext::new(
                        "superi-codecs-vendor.process",
                        "wait_for_vendor_plugin",
                    )))
                }
            }
        }
    }
}

impl ProcessState {
    fn terminate(&mut self) {
        if !self.terminated {
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.terminated = true;
        }
    }
}

impl Drop for ProcessState {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn canonical_executable(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).map_err(|source| {
        Error::with_source(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "vendor plugin executable was not found",
            source,
        )
        .with_context(
            ErrorContext::new("superi-codecs-vendor.process", "resolve_vendor_plugin")
                .with_field("executable", path.display().to_string()),
        )
    })
}

fn writer_loop(mut stdin: ChildStdin, commands: Receiver<WriteCommand>) {
    for command in commands {
        let result = stdin.write_all(&command.bytes).and_then(|()| stdin.flush());
        let failed = result.is_err();
        let _ = command.completion.send(result);
        if failed {
            break;
        }
    }
}

fn reader_loop(
    mut stdout: impl BufRead,
    events: SyncSender<ReadEvent>,
    maximum_message_bytes: usize,
) {
    loop {
        match read_bounded_line(&mut stdout, maximum_message_bytes) {
            Ok(Some(line)) => {
                if events.send(ReadEvent::Line(line)).is_err() {
                    break;
                }
            }
            Ok(None) => {
                let _ = events.send(ReadEvent::EndOfStream);
                break;
            }
            Err(BoundedReadError::Io(source)) => {
                let _ = events.send(ReadEvent::Io(source));
                break;
            }
            Err(BoundedReadError::Unterminated) => {
                let _ = events.send(ReadEvent::Unterminated);
                break;
            }
            Err(BoundedReadError::TooLarge) => {
                let _ = events.send(ReadEvent::TooLarge);
                break;
            }
        }
    }
}

enum BoundedReadError {
    Io(io::Error),
    Unterminated,
    TooLarge,
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    maximum_message_bytes: usize,
) -> std::result::Result<Option<Vec<u8>>, BoundedReadError> {
    let mut output = Vec::new();
    loop {
        let available = reader.fill_buf().map_err(BoundedReadError::Io)?;
        if available.is_empty() {
            return if output.is_empty() {
                Ok(None)
            } else {
                Err(BoundedReadError::Unterminated)
            };
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        let payload = if available.get(consumed - 1) == Some(&b'\n') {
            &available[..consumed - 1]
        } else {
            &available[..consumed]
        };
        if output.len().saturating_add(payload.len()) > maximum_message_bytes {
            return Err(BoundedReadError::TooLarge);
        }
        output.extend_from_slice(payload);
        let complete = available.get(consumed - 1) == Some(&b'\n');
        reader.consume(consumed);
        if complete {
            return Ok(Some(output));
        }
    }
}

fn wait_for<T>(
    receiver: &Receiver<T>,
    operation: &OperationContext,
    operation_name: &'static str,
) -> Result<T> {
    loop {
        operation.check(operation_name)?;
        let wait = operation
            .remaining()
            .map(|remaining| remaining.min(WAIT_SLICE))
            .unwrap_or(WAIT_SLICE);
        match receiver.recv_timeout(wait) {
            Ok(value) => return Ok(value),
            Err(RecvTimeoutError::Timeout) => operation.check(operation_name)?,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(unavailable(
                    operation_name,
                    "vendor plugin protocol channel stopped",
                ))
            }
        }
    }
}

fn worker_error(error: ErrorWire) -> Result<Error> {
    if error.message.trim().is_empty() {
        return Err(protocol_error(
            "vendor_plugin_failure",
            "vendor plugin returned an empty failure message",
        ));
    }
    let category = ErrorCategory::from_code(&error.category).ok_or_else(|| {
        protocol_error(
            "vendor_plugin_failure",
            "vendor plugin returned an unknown failure category",
        )
    })?;
    let recoverability = Recoverability::from_code(&error.recoverability).ok_or_else(|| {
        protocol_error(
            "vendor_plugin_failure",
            "vendor plugin returned unknown failure recoverability",
        )
    })?;
    Ok(
        Error::new(category, recoverability, error.message).with_context(
            ErrorContext::new("superi-codecs-vendor.process", "vendor_plugin_failure")
                .with_field("reported_category", error.category)
                .with_field("reported_recoverability", error.recoverability),
        ),
    )
}

pub(crate) fn protocol_error(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Terminal,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-vendor.process", operation))
}

fn unavailable(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-vendor.process", operation))
}

fn message_too_large(operation: &'static str, observed: usize) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        "vendor plugin protocol message exceeds its configured bound",
    )
    .with_context(
        ErrorContext::new("superi-codecs-vendor.process", operation)
            .with_field("observed_bytes", observed.to_string()),
    )
}

fn invalid_config(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-vendor.process", operation))
}
