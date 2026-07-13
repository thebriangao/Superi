use std::io::{self, Cursor, Read};
use std::time::{Duration as StdDuration, Instant};

use superi_core::error::{ErrorCategory, Recoverability};
use superi_media_io::demux::StreamId;
use superi_media_io::operation::{CancellationToken, MediaPriority, OperationContext};
use superi_media_io::read::{
    read_exact_interruptible, CorruptionKind, CorruptionReport, ReadOutcome,
};

#[test]
fn media_priorities_have_stable_user_visible_order() {
    assert_eq!(MediaPriority::Background.code(), "background");
    assert_eq!(MediaPriority::Export.code(), "export");
    assert_eq!(MediaPriority::Playback.code(), "playback");
    assert_eq!(MediaPriority::Interactive.code(), "interactive");
    assert!(MediaPriority::Interactive > MediaPriority::Playback);
    assert!(MediaPriority::Playback > MediaPriority::Export);
    assert!(MediaPriority::Export > MediaPriority::Background);
    assert_eq!(
        MediaPriority::ALL
            .iter()
            .map(|priority| priority.rank())
            .collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
}

#[test]
fn cancellation_and_deadlines_return_actionable_shared_errors() {
    let operation = OperationContext::new(MediaPriority::Interactive);
    let token = operation.cancellation_token().clone();
    std::thread::spawn(move || token.cancel()).join().unwrap();

    let cancelled = operation.check("open_source").unwrap_err();
    assert_eq!(cancelled.category(), ErrorCategory::Cancelled);
    assert_eq!(cancelled.recoverability(), Recoverability::Degraded);
    assert_eq!(cancelled.contexts()[0].operation(), "open_source");
    assert_eq!(
        cancelled.contexts()[0].field("priority"),
        Some("interactive")
    );

    let expired = OperationContext::new(MediaPriority::Playback).with_deadline(Instant::now());
    let timeout = expired.check("read_packet").unwrap_err();
    assert_eq!(timeout.category(), ErrorCategory::Timeout);
    assert_eq!(timeout.recoverability(), Recoverability::Retryable);
    assert_eq!(timeout.contexts()[0].operation(), "read_packet");
    assert_eq!(timeout.contexts()[0].field("priority"), Some("playback"));
    assert_eq!(expired.remaining(), Some(StdDuration::ZERO));

    let overflow = OperationContext::new(MediaPriority::Background)
        .with_timeout(StdDuration::MAX)
        .unwrap_err();
    assert_eq!(overflow.category(), ErrorCategory::InvalidInput);
}

struct OneByteReader {
    bytes: Vec<u8>,
    position: usize,
}

impl OneByteReader {
    fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            position: 0,
        }
    }
}

impl Read for OneByteReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.position == self.bytes.len() {
            return Ok(0);
        }
        buffer[0] = self.bytes[self.position];
        self.position += 1;
        Ok(1)
    }
}

struct InterruptedOnce<R> {
    inner: R,
    interrupted: bool,
}

impl<R> InterruptedOnce<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            interrupted: false,
        }
    }
}

impl<R: Read> Read for InterruptedOnce<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::Error::new(io::ErrorKind::Interrupted, "retry me"));
        }
        self.inner.read(buffer)
    }
}

#[test]
fn short_and_interrupted_reads_fill_the_requested_buffer() {
    let operation = OperationContext::new(MediaPriority::Playback);
    let mut short = OneByteReader::new([1, 2, 3, 4]);
    let mut buffer = [0_u8; 4];
    let outcome = read_exact_interruptible(&mut short, &mut buffer, 40, &operation).unwrap();
    assert_eq!(outcome, ReadOutcome::Complete(4));
    assert_eq!(buffer, [1, 2, 3, 4]);

    let mut interrupted = InterruptedOnce::new(Cursor::new([9_u8, 8, 7]));
    let mut buffer = [0_u8; 3];
    let outcome = read_exact_interruptible(&mut interrupted, &mut buffer, 90, &operation).unwrap();
    assert_eq!(outcome, ReadOutcome::Complete(3));
    assert_eq!(buffer, [9, 8, 7]);
}

#[test]
fn partial_and_empty_reads_are_explicit_and_recoverable() {
    let operation = OperationContext::new(MediaPriority::Background);
    let mut truncated = Cursor::new([5_u8, 6]);
    let mut buffer = [0_u8; 4];
    let outcome = read_exact_interruptible(&mut truncated, &mut buffer, 128, &operation).unwrap();
    let ReadOutcome::Partial { value, report } = outcome else {
        panic!("truncated input must return an explicit partial result")
    };
    assert_eq!(value, 2);
    assert_eq!(&buffer[..value], [5, 6]);
    assert_eq!(report.kind(), CorruptionKind::Truncated);
    assert_eq!(report.byte_offset(), Some(128));
    assert_eq!(report.expected_bytes(), Some(4));
    assert_eq!(report.actual_bytes(), Some(2));
    assert_eq!(report.recoverability(), Recoverability::Degraded);

    let error = report.to_error("read_packet");
    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert_eq!(error.contexts()[0].field("kind"), Some("truncated"));
    assert_eq!(error.contexts()[0].field("byte_offset"), Some("128"));
    assert_eq!(error.contexts()[0].field("expected_bytes"), Some("4"));
    assert_eq!(error.contexts()[0].field("actual_bytes"), Some("2"));

    let mut empty = Cursor::new([]);
    let mut buffer = [0_u8; 1];
    assert_eq!(
        read_exact_interruptible(&mut empty, &mut buffer, 0, &operation).unwrap(),
        ReadOutcome::EndOfStream
    );
}

struct CancellingReader {
    token: CancellationToken,
    calls: usize,
}

impl Read for CancellingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        buffer[0] = 77;
        self.token.cancel();
        Ok(1)
    }
}

#[test]
fn cancellation_stops_between_bounded_reads_and_preserves_progress() {
    let operation = OperationContext::new(MediaPriority::Interactive);
    let mut reader = CancellingReader {
        token: operation.cancellation_token().clone(),
        calls: 0,
    };
    let mut buffer = [0_u8; 4];
    let error = read_exact_interruptible(&mut reader, &mut buffer, 200, &operation).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(reader.calls, 1);
    assert_eq!(buffer[0], 77);
    let progress = error.contexts().last().unwrap();
    assert_eq!(progress.field("byte_offset"), Some("200"));
    assert_eq!(progress.field("expected_bytes"), Some("4"));
    assert_eq!(progress.field("actual_bytes"), Some("1"));
}

struct LyingReader;

impl Read for LyingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        Ok(buffer.len() + 1)
    }
}

#[test]
fn impossible_reader_counts_fail_without_panicking() {
    let operation = OperationContext::new(MediaPriority::Export);
    let mut reader = LyingReader;
    let mut buffer = [0_u8; 2];
    let error = read_exact_interruptible(&mut reader, &mut buffer, 0, &operation).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Internal);
    assert_eq!(error.recoverability(), Recoverability::Terminal);
}

#[test]
fn corruption_reports_preserve_stream_and_recovery_context() {
    let report = CorruptionReport::new(
        CorruptionKind::ChecksumMismatch,
        Recoverability::UserCorrectable,
    )
    .with_stream(StreamId::new(7))
    .with_byte_progress(512, 32, 32)
    .unwrap();
    let error = report.to_error("decode_packet");
    assert_eq!(report.kind().code(), "checksum_mismatch");
    assert_eq!(report.stream_id(), Some(StreamId::new(7)));
    assert_eq!(error.contexts()[0].field("stream_id"), Some("7"));
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
}

#[test]
fn operation_and_read_values_cross_thread_boundaries_safely() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<CancellationToken>();
    assert_send_sync::<OperationContext>();
    assert_send_sync::<CorruptionReport>();
    assert_send_sync::<ReadOutcome<Vec<u8>>>();
}
