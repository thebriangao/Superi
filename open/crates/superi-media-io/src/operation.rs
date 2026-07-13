//! Cooperative cancellation, deadlines, and user-visible media priority.
//!
//! Media implementations poll an [`OperationContext`] before starting work and between bounded
//! units of I/O or codec processing. Platform adapters must also apply [`OperationContext::remaining`]
//! to native calls that can block because a cooperative token cannot preempt an unbounded foreign
//! call by itself.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

/// User-visible scheduling importance for one media operation.
///
/// Natural ordering is scheduling order, with larger values ranked first. The later job scheduler
/// owns queue fairness and stable tie breaking; this value only carries the caller's intent across
/// the media boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
#[non_exhaustive]
pub enum MediaPriority {
    /// Replaceable previews, analysis, cache, or other unattended work.
    Background = 0,
    /// A user-requested offline render or export.
    Export = 1,
    /// Time-sensitive playback and prefetch work.
    Playback = 2,
    /// A direct user action such as ingest, seek, or relink.
    Interactive = 3,
}

impl MediaPriority {
    /// Every priority defined by this version in ascending scheduling order.
    pub const ALL: &'static [Self] = &[
        Self::Background,
        Self::Export,
        Self::Playback,
        Self::Interactive,
    ];

    /// Returns the stable machine code used by diagnostics and future API projection.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Export => "export",
            Self::Playback => "playback",
            Self::Interactive => "interactive",
        }
    }

    /// Returns the stable ascending rank. Schedulers select larger ranks first.
    #[must_use]
    pub const fn rank(self) -> u8 {
        self as u8
    }
}

/// Cloneable one-way cancellation shared between operation owners and workers.
///
/// The atomic flag carries no adjacent data and is not a synchronization barrier, so relaxed
/// ordering is sufficient. Dropping every token ends the operation lifetime; cancellation cannot
/// be reset and accidentally reused for unrelated work.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Creates an active token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation. Calling this more than once is idempotent.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Immutable cooperative state passed to one media operation and its nested work.
#[derive(Clone, Debug)]
pub struct OperationContext {
    priority: MediaPriority,
    cancellation: CancellationToken,
    deadline: Option<Instant>,
}

impl OperationContext {
    /// Creates an operation with no deadline and a fresh cancellation token.
    #[must_use]
    pub fn new(priority: MediaPriority) -> Self {
        Self {
            priority,
            cancellation: CancellationToken::new(),
            deadline: None,
        }
    }

    /// Replaces the cancellation token so related contexts can share one cancellation owner.
    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Sets an absolute in-process monotonic deadline.
    #[must_use]
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Sets a deadline relative to now and rejects values the platform cannot represent.
    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self> {
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "media operation timeout is outside the supported monotonic clock range",
            )
            .with_context(
                ErrorContext::new("superi-media-io.operation", "set_timeout")
                    .with_field("priority", self.priority.code()),
            )
        })?;
        self.deadline = Some(deadline);
        Ok(self)
    }

    /// Returns the user-visible scheduling priority.
    #[must_use]
    pub const fn priority(&self) -> MediaPriority {
        self.priority
    }

    /// Returns the shared cancellation token.
    #[must_use]
    pub const fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation
    }

    /// Returns the opaque in-process deadline when one exists.
    #[must_use]
    pub const fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Returns time remaining, saturated to zero after the deadline.
    #[must_use]
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    /// Checks cancellation and timeout at one bounded operation checkpoint.
    ///
    /// Explicit user cancellation takes precedence if both states are observed together.
    pub fn check(&self, operation: &'static str) -> Result<()> {
        let context = || {
            ErrorContext::new("superi-media-io.operation", operation)
                .with_field("priority", self.priority.code())
        };
        if self.cancellation.is_cancelled() {
            return Err(Error::new(
                ErrorCategory::Cancelled,
                Recoverability::Degraded,
                "media operation was cancelled",
            )
            .with_context(context()));
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(Error::new(
                ErrorCategory::Timeout,
                Recoverability::Retryable,
                "media operation exceeded its deadline",
            )
            .with_context(context()));
        }
        Ok(())
    }
}
