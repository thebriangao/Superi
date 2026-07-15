//! Bounded playback prediction over exact frame coordinates.
//!
//! This module owns cache-neutral prediction policy. It separates the nearest playback-critical
//! frames from farther predictive and trailing work, preserves exact signed transport direction,
//! and clips every request to one caller-owned half-open timeline range. Engine orchestration owns
//! worker submission, cancellation, graph evaluation, and concrete cache insertion.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{RationalTime, TimeRange};

const COMPONENT: &str = "superi-cache.prefetch";

/// Hard request-count ceiling for one prediction plan.
pub const MAX_PREFETCH_FRAMES: usize = 512;

/// Signed direction inferred from one transport step.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlaybackPrefetchDirection {
    /// Increasing frame coordinates.
    Forward,
    /// Decreasing frame coordinates.
    Reverse,
}

impl PlaybackPrefetchDirection {
    const fn from_step(step: i64) -> Self {
        if step > 0 {
            Self::Forward
        } else {
            Self::Reverse
        }
    }

    const fn sign(self) -> i32 {
        match self {
            Self::Forward => 1,
            Self::Reverse => -1,
        }
    }
}

/// Playback relevance of one predicted frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlaybackPrefetchUrgency {
    /// Nearest work needed to protect continuous playback.
    PlaybackCritical,
    /// Farther directional or trailing work that remains replaceable.
    Predictive,
}

/// Explicit bounded prediction policy for one playback stream.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlaybackPrefetchConfig {
    playback_critical_frames: usize,
    predictive_frames: usize,
    trailing_frames: usize,
}

impl PlaybackPrefetchConfig {
    /// Creates one finite frame-count policy.
    ///
    /// At least one nearest playback-critical frame is required. Predictive and trailing work may
    /// be disabled independently. The complete possible plan remains bounded by
    /// [`MAX_PREFETCH_FRAMES`].
    pub fn new(
        playback_critical_frames: usize,
        predictive_frames: usize,
        trailing_frames: usize,
    ) -> Result<Self> {
        if playback_critical_frames == 0 {
            return Err(invalid(
                "configure",
                "playback prefetch requires at least one playback-critical frame",
            ));
        }
        let total = playback_critical_frames
            .checked_add(predictive_frames)
            .and_then(|count| count.checked_add(trailing_frames))
            .ok_or_else(|| invalid("configure", "playback prefetch frame count overflows"))?;
        if total > MAX_PREFETCH_FRAMES {
            return Err(invalid(
                "configure",
                "playback prefetch frame count exceeds the bounded plan limit",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "frame_limit")
                    .with_field("requested_frames", total.to_string())
                    .with_field("maximum_frames", MAX_PREFETCH_FRAMES.to_string()),
            ));
        }
        Ok(Self {
            playback_critical_frames,
            predictive_frames,
            trailing_frames,
        })
    }

    /// Returns the nearest frames requested before extended prediction.
    #[must_use]
    pub const fn playback_critical_frames(self) -> usize {
        self.playback_critical_frames
    }

    /// Returns additional frames requested in the active direction.
    #[must_use]
    pub const fn predictive_frames(self) -> usize {
        self.predictive_frames
    }

    /// Returns additional frames requested opposite the active direction.
    #[must_use]
    pub const fn trailing_frames(self) -> usize {
        self.trailing_frames
    }

    /// Builds one deterministic nearest-first prediction plan.
    #[must_use]
    pub fn plan(self, input: PlaybackPrefetchInput) -> PlaybackPrefetchPlan {
        let direction = PlaybackPrefetchDirection::from_step(input.signed_step);
        let primary_count = self.playback_critical_frames + self.predictive_frames;
        let mut requests = Vec::with_capacity(primary_count + self.trailing_frames);

        for ordinal in 1..=primary_count {
            let Some(frame) = predicted_frame(input, i128::from(input.signed_step), ordinal) else {
                break;
            };
            let urgency = if ordinal <= self.playback_critical_frames {
                PlaybackPrefetchUrgency::PlaybackCritical
            } else {
                PlaybackPrefetchUrgency::Predictive
            };
            requests.push(PlaybackPrefetchRequest {
                frame,
                urgency,
                offset_steps: direction.sign()
                    * i32::try_from(ordinal).expect("bounded prefetch ordinal fits i32"),
            });
        }

        let trailing_step = -i128::from(input.signed_step);
        for ordinal in 1..=self.trailing_frames {
            let Some(frame) = predicted_frame(input, trailing_step, ordinal) else {
                break;
            };
            requests.push(PlaybackPrefetchRequest {
                frame,
                urgency: PlaybackPrefetchUrgency::Predictive,
                offset_steps: -direction.sign()
                    * i32::try_from(ordinal).expect("bounded prefetch ordinal fits i32"),
            });
        }

        PlaybackPrefetchPlan {
            playhead: input.playhead,
            direction,
            requests,
        }
    }
}

/// One exact playback observation used to predict future cache work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackPrefetchInput {
    playhead: RationalTime,
    bounds: TimeRange,
    signed_step: i64,
}

impl PlaybackPrefetchInput {
    /// Creates one frame-coordinate observation.
    ///
    /// `signed_step` is the predicted frame-coordinate change for each next playback presentation.
    /// Positive values move forward, negative values move in reverse, and magnitudes above one
    /// represent faster transport. The playhead and range must share one exact timebase, and the
    /// playhead must be inside the half-open range.
    pub fn new(playhead: RationalTime, bounds: TimeRange, signed_step: i64) -> Result<Self> {
        if signed_step == 0 {
            return Err(invalid("observe", "playback prefetch step must be nonzero"));
        }
        if playhead.timebase() != bounds.timebase() {
            return Err(invalid(
                "observe",
                "playback prefetch playhead and bounds must use one timebase",
            ));
        }
        if !bounds.contains(playhead)? {
            return Err(invalid(
                "observe",
                "playback prefetch playhead must be inside the timeline bounds",
            ));
        }
        Ok(Self {
            playhead,
            bounds,
            signed_step,
        })
    }

    /// Returns the exact current frame coordinate.
    #[must_use]
    pub const fn playhead(self) -> RationalTime {
        self.playhead
    }

    /// Returns the half-open frame range available to prediction.
    #[must_use]
    pub const fn bounds(self) -> TimeRange {
        self.bounds
    }

    /// Returns the signed predicted coordinate change per presentation.
    #[must_use]
    pub const fn signed_step(self) -> i64 {
        self.signed_step
    }
}

/// One exact predicted frame and its policy meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackPrefetchRequest {
    frame: RationalTime,
    urgency: PlaybackPrefetchUrgency,
    offset_steps: i32,
}

impl PlaybackPrefetchRequest {
    /// Returns the exact frame coordinate to populate.
    #[must_use]
    pub const fn frame(self) -> RationalTime {
        self.frame
    }

    /// Returns the playback relevance assigned by the bounded policy.
    #[must_use]
    pub const fn urgency(self) -> PlaybackPrefetchUrgency {
        self.urgency
    }

    /// Returns the signed ordinal distance from the playhead.
    ///
    /// This counts prediction steps rather than raw coordinate units. Forward primary work is
    /// positive, reverse primary work is negative, and trailing work has the opposite sign.
    #[must_use]
    pub const fn offset_steps(self) -> i32 {
        self.offset_steps
    }
}

/// Immutable ordered cache work predicted from one playback observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackPrefetchPlan {
    playhead: RationalTime,
    direction: PlaybackPrefetchDirection,
    requests: Vec<PlaybackPrefetchRequest>,
}

impl PlaybackPrefetchPlan {
    /// Returns the playhead that produced this plan.
    #[must_use]
    pub const fn playhead(&self) -> RationalTime {
        self.playhead
    }

    /// Returns the active prediction direction.
    #[must_use]
    pub const fn direction(&self) -> PlaybackPrefetchDirection {
        self.direction
    }

    /// Returns requests in execution order.
    ///
    /// Nearest playback-critical frames precede farther directional prediction, followed by
    /// trailing work in nearest-first order.
    #[must_use]
    pub fn requests(&self) -> &[PlaybackPrefetchRequest] {
        &self.requests
    }

    /// Returns the exact number of predicted requests.
    #[must_use]
    pub fn len(&self) -> usize {
        self.requests.len()
    }

    /// Returns whether timeline clipping left no frame to populate.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }
}

fn predicted_frame(
    input: PlaybackPrefetchInput,
    signed_step: i128,
    ordinal: usize,
) -> Option<RationalTime> {
    let offset = signed_step.checked_mul(i128::try_from(ordinal).ok()?)?;
    let value = i128::from(input.playhead.value()).checked_add(offset)?;
    let value = i64::try_from(value).ok()?;
    let frame = RationalTime::new(value, input.playhead.timebase());
    let end = input
        .bounds
        .end_exclusive()
        .expect("validated time range has a representable end");
    (frame >= input.bounds.start() && frame < end).then_some(frame)
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
