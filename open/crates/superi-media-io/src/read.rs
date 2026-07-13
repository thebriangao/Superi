//! Explicit corruption reports and interruption-aware byte reads.

use std::io::{self, Read};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::demux::StreamId;
use crate::operation::OperationContext;

/// Stable codec-neutral classification for damaged or inconsistent media data.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CorruptionKind {
    /// The source ended after only part of a required structure was read.
    Truncated,
    /// Bytes do not satisfy the relevant container or codec grammar.
    Malformed,
    /// Stored and calculated integrity values disagree.
    ChecksumMismatch,
    /// Related metadata fields contradict one another.
    InconsistentMetadata,
}

impl CorruptionKind {
    /// Returns the stable machine code used in diagnostics.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Truncated => "truncated",
            Self::Malformed => "malformed",
            Self::ChecksumMismatch => "checksum_mismatch",
            Self::InconsistentMetadata => "inconsistent_metadata",
        }
    }
}

/// Structured evidence for one damaged source region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorruptionReport {
    kind: CorruptionKind,
    recoverability: Recoverability,
    stream_id: Option<StreamId>,
    byte_offset: Option<u64>,
    expected_bytes: Option<usize>,
    actual_bytes: Option<usize>,
}

impl CorruptionReport {
    /// Creates a report with explicit workflow recovery behavior.
    #[must_use]
    pub const fn new(kind: CorruptionKind, recoverability: Recoverability) -> Self {
        Self {
            kind,
            recoverability,
            stream_id: None,
            byte_offset: None,
            expected_bytes: None,
            actual_bytes: None,
        }
    }

    /// Creates a recoverable truncation report with exact byte progress.
    pub fn truncated(byte_offset: u64, expected: usize, actual: usize) -> Result<Self> {
        if actual >= expected {
            return Err(invalid_report(
                "truncated data must contain fewer actual bytes than expected bytes",
            ));
        }
        Self::new(CorruptionKind::Truncated, Recoverability::Degraded).with_byte_progress(
            byte_offset,
            expected,
            actual,
        )
    }

    /// Adds a source-local stream identifier.
    #[must_use]
    pub fn with_stream(mut self, stream_id: StreamId) -> Self {
        self.stream_id = Some(stream_id);
        self
    }

    /// Adds a checked byte range and progress count.
    pub fn with_byte_progress(
        mut self,
        byte_offset: u64,
        expected: usize,
        actual: usize,
    ) -> Result<Self> {
        if actual > expected {
            return Err(invalid_report(
                "actual bytes must not exceed expected bytes",
            ));
        }
        let actual_u64 = u64::try_from(actual).map_err(|_| {
            invalid_report("actual byte count cannot be represented as a file offset")
        })?;
        byte_offset
            .checked_add(actual_u64)
            .ok_or_else(|| invalid_report("byte progress exceeds the source offset range"))?;
        self.byte_offset = Some(byte_offset);
        self.expected_bytes = Some(expected);
        self.actual_bytes = Some(actual);
        Ok(self)
    }

    /// Returns the stable corruption classification.
    #[must_use]
    pub const fn kind(&self) -> CorruptionKind {
        self.kind
    }

    /// Returns the declared workflow recovery behavior.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    /// Returns the source-local stream when known.
    #[must_use]
    pub const fn stream_id(&self) -> Option<StreamId> {
        self.stream_id
    }

    /// Returns the starting source byte offset when known.
    #[must_use]
    pub const fn byte_offset(&self) -> Option<u64> {
        self.byte_offset
    }

    /// Returns the required byte count when known.
    #[must_use]
    pub const fn expected_bytes(&self) -> Option<usize> {
        self.expected_bytes
    }

    /// Returns the successfully read byte count when known.
    #[must_use]
    pub const fn actual_bytes(&self) -> Option<usize> {
        self.actual_bytes
    }

    /// Converts this report into the canonical shared corruption error.
    #[must_use]
    pub fn to_error(&self, operation: &'static str) -> Error {
        let message = match self.kind {
            CorruptionKind::Truncated => "media data is truncated",
            CorruptionKind::Malformed => "media data is malformed",
            CorruptionKind::ChecksumMismatch => "media data failed its integrity check",
            CorruptionKind::InconsistentMetadata => "media metadata is inconsistent",
        };
        let mut context = ErrorContext::new("superi-media-io.read", operation)
            .with_field("kind", self.kind.code());
        if let Some(stream_id) = self.stream_id {
            context.insert_field("stream_id", stream_id.value().to_string());
        }
        if let Some(byte_offset) = self.byte_offset {
            context.insert_field("byte_offset", byte_offset.to_string());
        }
        if let Some(expected) = self.expected_bytes {
            context.insert_field("expected_bytes", expected.to_string());
        }
        if let Some(actual) = self.actual_bytes {
            context.insert_field("actual_bytes", actual.to_string());
        }
        Error::new(ErrorCategory::CorruptData, self.recoverability, message).with_context(context)
    }
}

/// One explicit result from a bounded source read.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReadOutcome<T> {
    /// The requested value is complete.
    Complete(T),
    /// A usable partial value plus exact corruption evidence.
    Partial {
        /// The usable incomplete value.
        value: T,
        /// Why the value is incomplete or damaged.
        report: CorruptionReport,
    },
    /// No bytes or value remain at the requested source boundary.
    EndOfStream,
}

/// Fills a caller-provided buffer while preserving short-read and interruption behavior.
///
/// Ordinary short reads are accumulated and `Interrupted` is retried. Initial EOF returns
/// [`ReadOutcome::EndOfStream`], while EOF after progress returns a partial byte count and
/// truncation report. Cancellation and deadlines are checked before and after every bounded read.
/// The supplied reader must itself provide bounded calls or apply the operation's remaining time
/// to its platform API because [`Read::read`] does not guarantee that a call will not block.
pub fn read_exact_interruptible<R: Read + ?Sized>(
    reader: &mut R,
    buffer: &mut [u8],
    byte_offset: u64,
    operation: &OperationContext,
) -> Result<ReadOutcome<usize>> {
    let expected = buffer.len();
    let mut actual = 0;
    check_with_progress(operation, byte_offset, expected, actual)?;
    if expected == 0 {
        return Ok(ReadOutcome::Complete(0));
    }

    loop {
        let available = expected - actual;
        match reader.read(&mut buffer[actual..]) {
            Ok(0) if actual == 0 => return Ok(ReadOutcome::EndOfStream),
            Ok(0) => {
                return Ok(ReadOutcome::Partial {
                    value: actual,
                    report: CorruptionReport::truncated(byte_offset, expected, actual)?,
                });
            }
            Ok(read) if read > available => {
                return Err(Error::new(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "media reader reported more bytes than the supplied buffer can hold",
                )
                .with_context(
                    progress_context(byte_offset, expected, actual)
                        .with_field("reported_bytes", read.to_string()),
                ));
            }
            Ok(read) => {
                actual += read;
                check_with_progress(operation, byte_offset, expected, actual)?;
                if actual == expected {
                    return Ok(ReadOutcome::Complete(actual));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                check_with_progress(operation, byte_offset, expected, actual)?;
            }
            Err(error) => {
                return Err(Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "media input could not be read",
                    error,
                )
                .with_context(progress_context(byte_offset, expected, actual)));
            }
        }
    }
}

fn check_with_progress(
    operation: &OperationContext,
    byte_offset: u64,
    expected: usize,
    actual: usize,
) -> Result<()> {
    operation
        .check("read_exact_interruptible")
        .map_err(|mut error| {
            error.push_context(progress_context(byte_offset, expected, actual));
            error
        })
}

fn progress_context(byte_offset: u64, expected: usize, actual: usize) -> ErrorContext {
    ErrorContext::new("superi-media-io.read", "read_exact_interruptible")
        .with_field("byte_offset", byte_offset.to_string())
        .with_field("expected_bytes", expected.to_string())
        .with_field("actual_bytes", actual.to_string())
}

fn invalid_report(message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(
        "superi-media-io.read",
        "create_corruption_report",
    ))
}
