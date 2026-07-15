//! Editable exact-frame rotoscoping state and solver-independent propagation hooks.
//!
//! Authored base masks and corrections are canonical project state. Propagated frames are
//! replaceable, inspectable results that remain inside the same serializable artifact. Engines can
//! implement [`RotoscopePropagator`] without gaining ownership of masks, graph state, or timing.

use std::collections::BTreeSet;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{Error as _, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{RationalTime, TimeRange, Timebase};

const COMPONENT: &str = "superi-effects::rotoscope";

/// Current standalone rotoscope artifact wire revision.
pub const ROTOSCOPE_ARTIFACT_SCHEMA_REVISION: u32 = 1;
/// Maximum number of independent spans in one rotoscope artifact.
pub const MAX_ROTOSCOPE_SPANS: usize = 4_096;
/// Maximum exact frame coordinates represented by one rotoscope span.
pub const MAX_ROTOSCOPE_FRAMES_PER_SPAN: u64 = 100_000;

/// Stable identity for one independently propagated rotoscope span.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RotoscopeSpanId(u64);

impl RotoscopeSpanId {
    /// Creates an identity from its persisted integer representation.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for RotoscopeSpanId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// One complete editable mask payload at an exact frame coordinate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RotoscopeFrame<T> {
    frame: RationalTime,
    payload: T,
}

impl<T> RotoscopeFrame<T> {
    /// Creates a frame sample. Its owning span validates clock and range membership.
    #[must_use]
    pub const fn new(frame: RationalTime, payload: T) -> Self {
        Self { frame, payload }
    }

    /// Returns the exact frame coordinate.
    #[must_use]
    pub const fn frame(&self) -> RationalTime {
        self.frame
    }

    /// Returns the complete editable mask payload.
    #[must_use]
    pub const fn payload(&self) -> &T {
        &self.payload
    }

    /// Consumes the frame sample and returns its complete payload.
    #[must_use]
    pub fn into_payload(self) -> T {
        self.payload
    }
}

/// One half-open exact-time span with an authored base, corrections, and derived samples.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RotoscopeSpan<T> {
    id: RotoscopeSpanId,
    range: TimeRange,
    base: RotoscopeFrame<T>,
    corrections: Vec<RotoscopeFrame<T>>,
    propagated: Vec<RotoscopeFrame<T>>,
}

impl<T> RotoscopeSpan<T> {
    /// Creates a checked span with no corrections or propagated results.
    pub fn new(id: RotoscopeSpanId, range: TimeRange, base: RotoscopeFrame<T>) -> Result<Self> {
        let span = Self {
            id,
            range,
            base,
            corrections: Vec::new(),
            propagated: Vec::new(),
        };
        validate_span(&span, range.timebase(), "create_span")?;
        Ok(span)
    }

    /// Returns the stable span identity.
    #[must_use]
    pub const fn id(&self) -> RotoscopeSpanId {
        self.id
    }

    /// Returns the half-open frame range owned by this span.
    #[must_use]
    pub const fn range(&self) -> TimeRange {
        self.range
    }

    /// Returns the authored base frame and mask payload.
    #[must_use]
    pub const fn base(&self) -> &RotoscopeFrame<T> {
        &self.base
    }

    /// Returns corrections in strictly increasing frame order.
    #[must_use]
    pub fn corrections(&self) -> &[RotoscopeFrame<T>] {
        &self.corrections
    }

    /// Returns derived propagation samples in strictly increasing frame order.
    #[must_use]
    pub fn propagated_frames(&self) -> &[RotoscopeFrame<T>] {
        &self.propagated
    }

    fn from_parts(
        id: RotoscopeSpanId,
        range: TimeRange,
        base: RotoscopeFrame<T>,
        corrections: Vec<RotoscopeFrame<T>>,
        propagated: Vec<RotoscopeFrame<T>>,
    ) -> Result<Self> {
        let span = Self {
            id,
            range,
            base,
            corrections,
            propagated,
        };
        validate_span(&span, range.timebase(), "deserialize_artifact")?;
        Ok(span)
    }
}

/// Direction in which authored anchors influence propagated frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropagationDirection {
    /// Propagate from the base toward increasing frame coordinates.
    Forward,
    /// Propagate from the base toward decreasing frame coordinates.
    Backward,
}

/// Immutable bounded input passed to a propagation engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropagationRequest<T> {
    source_revision: u64,
    span_id: RotoscopeSpanId,
    direction: PropagationDirection,
    span_range: TimeRange,
    anchors: Vec<RotoscopeFrame<T>>,
    target_frames: Vec<RationalTime>,
}

impl<T> PropagationRequest<T> {
    /// Returns the artifact revision from which this request was built.
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }

    /// Returns the span being propagated.
    #[must_use]
    pub const fn span_id(&self) -> RotoscopeSpanId {
        self.span_id
    }

    /// Returns the requested traversal direction.
    #[must_use]
    pub const fn direction(&self) -> PropagationDirection {
        self.direction
    }

    /// Returns the complete half-open span range.
    #[must_use]
    pub const fn span_range(&self) -> TimeRange {
        self.span_range
    }

    /// Returns the base followed by directional correction anchors in traversal order.
    #[must_use]
    pub fn anchors(&self) -> &[RotoscopeFrame<T>] {
        &self.anchors
    }

    /// Returns every non-authored output frame in traversal order.
    #[must_use]
    pub fn target_frames(&self) -> &[RationalTime] {
        &self.target_frames
    }
}

/// A complete propagation result tied to one exact request and source revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropagationResult<T> {
    source_revision: u64,
    span_id: RotoscopeSpanId,
    direction: PropagationDirection,
    span_range: TimeRange,
    anchor_frames: Vec<RationalTime>,
    target_frames: Vec<RationalTime>,
    frames: Vec<RotoscopeFrame<T>>,
}

impl<T> PropagationResult<T> {
    /// Creates a result only when it covers the request's exact target sequence once.
    pub fn new(
        request: &PropagationRequest<T>,
        frames: impl IntoIterator<Item = RotoscopeFrame<T>>,
    ) -> Result<Self> {
        let expected_frames = request.target_frames.len();
        let mut incoming = frames.into_iter();
        let mut frames = Vec::with_capacity(expected_frames);
        for _ in 0..expected_frames {
            let Some(frame) = incoming.next() else {
                break;
            };
            frames.push(frame);
        }
        let has_extra_frame = incoming.next().is_some();
        if frames.len() != expected_frames || has_extra_frame {
            let actual_frames = if has_extra_frame {
                format!(">{expected_frames}")
            } else {
                frames.len().to_string()
            };
            return Err(rotoscope_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "create_propagation_result",
                "result_frame_set_mismatch",
                "propagation result must cover every requested target frame exactly once",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_propagation_result")
                    .with_field("expected_frames", expected_frames.to_string())
                    .with_field("actual_frames", actual_frames),
            ));
        }
        for (index, (frame, target)) in frames.iter().zip(request.target_frames.iter()).enumerate()
        {
            ensure_timebase(
                frame.frame,
                request.span_range.timebase(),
                "create_propagation_result",
            )?;
            if frame.frame != *target {
                return Err(rotoscope_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "create_propagation_result",
                    "result_frame_set_mismatch",
                    "propagation result frames must match the requested traversal order",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "create_propagation_result")
                        .with_field("result_index", index.to_string())
                        .with_field("expected_frame", target.value().to_string())
                        .with_field("actual_frame", frame.frame.value().to_string()),
                ));
            }
        }

        Ok(Self {
            source_revision: request.source_revision,
            span_id: request.span_id,
            direction: request.direction,
            span_range: request.span_range,
            anchor_frames: request.anchors.iter().map(RotoscopeFrame::frame).collect(),
            target_frames: request.target_frames.clone(),
            frames,
        })
    }

    /// Returns the artifact revision used to compute this result.
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }

    /// Returns the span identity used to compute this result.
    #[must_use]
    pub const fn span_id(&self) -> RotoscopeSpanId {
        self.span_id
    }

    /// Returns the direction used to compute this result.
    #[must_use]
    pub const fn direction(&self) -> PropagationDirection {
        self.direction
    }

    /// Returns result samples in request traversal order.
    #[must_use]
    pub fn frames(&self) -> &[RotoscopeFrame<T>] {
        &self.frames
    }
}

/// Solver-independent hook for tracking, local inference, or another propagation engine.
pub trait RotoscopePropagator<T>: Send + Sync {
    /// Computes every requested target while preserving the request identity and revision.
    fn propagate(&self, request: &PropagationRequest<T>) -> Result<PropagationResult<T>>;
}

/// Provenance for the currently resolved payload at one frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RotoscopeFrameSource {
    /// The span's canonical base payload.
    Base,
    /// An artist-authored per-frame correction.
    Correction,
    /// A replaceable propagated result.
    Propagation,
}

/// Inspectable resolved frame that retains its source-layer provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedRotoscopeFrame<'a, T> {
    frame: RationalTime,
    source: RotoscopeFrameSource,
    payload: &'a T,
}

impl<'a, T> ResolvedRotoscopeFrame<'a, T> {
    /// Returns the exact resolved frame coordinate.
    #[must_use]
    pub const fn frame(&self) -> RationalTime {
        self.frame
    }

    /// Returns whether the payload is a base, correction, or propagation result.
    #[must_use]
    pub const fn source(&self) -> RotoscopeFrameSource {
        self.source
    }

    /// Returns the complete resolved mask payload.
    #[must_use]
    pub const fn payload(&self) -> &'a T {
        self.payload
    }
}

/// Complete editable rotoscope state for one exact frame clock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RotoscopeArtifact<T> {
    timebase: Timebase,
    content_revision: u64,
    spans: Vec<RotoscopeSpan<T>>,
}

impl<T> RotoscopeArtifact<T> {
    /// Creates canonical checked state from independently identified non-overlapping spans.
    pub fn new(
        timebase: Timebase,
        spans: impl IntoIterator<Item = RotoscopeSpan<T>>,
    ) -> Result<Self> {
        let mut incoming = spans.into_iter();
        let mut spans = Vec::with_capacity(incoming.size_hint().0.min(MAX_ROTOSCOPE_SPANS));
        for _ in 0..=MAX_ROTOSCOPE_SPANS {
            let Some(span) = incoming.next() else {
                break;
            };
            spans.push(span);
        }
        Self::from_parts(timebase, 0, spans)
    }

    /// Returns the exact frame clock shared by every span and sample.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns the monotonic revision used to fence asynchronous propagation.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.content_revision
    }

    /// Returns spans in canonical increasing range order.
    #[must_use]
    pub fn spans(&self) -> &[RotoscopeSpan<T>] {
        &self.spans
    }

    /// Finds one span by stable identity.
    #[must_use]
    pub fn span(&self, id: RotoscopeSpanId) -> Option<&RotoscopeSpan<T>> {
        self.spans.iter().find(|span| span.id == id)
    }

    /// Resolves a frame with authored base and correction state above derived propagation.
    #[must_use]
    pub fn resolved_frame(
        &self,
        span_id: RotoscopeSpanId,
        frame: RationalTime,
    ) -> Option<ResolvedRotoscopeFrame<'_, T>> {
        if frame.timebase() != self.timebase {
            return None;
        }
        let span = self.span(span_id)?;
        if frame == span.base.frame {
            return Some(ResolvedRotoscopeFrame {
                frame,
                source: RotoscopeFrameSource::Base,
                payload: &span.base.payload,
            });
        }
        if let Ok(index) = span
            .corrections
            .binary_search_by_key(&frame.value(), |candidate| candidate.frame.value())
        {
            return Some(ResolvedRotoscopeFrame {
                frame,
                source: RotoscopeFrameSource::Correction,
                payload: &span.corrections[index].payload,
            });
        }
        span.propagated
            .binary_search_by_key(&frame.value(), |candidate| candidate.frame.value())
            .ok()
            .map(|index| ResolvedRotoscopeFrame {
                frame,
                source: RotoscopeFrameSource::Propagation,
                payload: &span.propagated[index].payload,
            })
    }

    fn from_parts(
        timebase: Timebase,
        content_revision: u64,
        mut spans: Vec<RotoscopeSpan<T>>,
    ) -> Result<Self> {
        if spans.len() > MAX_ROTOSCOPE_SPANS {
            return Err(rotoscope_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "create_artifact",
                "span_limit_exceeded",
                "rotoscope artifact contains more spans than the supported bound",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_artifact")
                    .with_field("span_count", spans.len().to_string())
                    .with_field("span_limit", MAX_ROTOSCOPE_SPANS.to_string()),
            ));
        }
        for span in &spans {
            validate_span(span, timebase, "create_artifact")?;
        }
        spans.sort_by_key(|span| (span.range.start().value(), span.id));

        let mut identities = BTreeSet::new();
        for span in &spans {
            if !identities.insert(span.id) {
                return Err(rotoscope_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "create_artifact",
                    "duplicate_span_id",
                    "rotoscope span identity must be unique inside an artifact",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "create_artifact")
                        .with_field("span_id", span.id.to_string()),
                ));
            }
        }
        for pair in spans.windows(2) {
            let previous_end = pair[0].range.end_exclusive()?;
            if previous_end.value() > pair[1].range.start().value() {
                return Err(rotoscope_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "create_artifact",
                    "overlapping_spans",
                    "rotoscope spans must not overlap on the artifact frame clock",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "create_artifact")
                        .with_field("left_span_id", pair[0].id.to_string())
                        .with_field("right_span_id", pair[1].id.to_string()),
                ));
            }
        }
        Ok(Self {
            timebase,
            content_revision,
            spans,
        })
    }
}

impl<T: Clone> RotoscopeArtifact<T> {
    /// Adds a new span and advances the artifact revision.
    pub fn with_span(&self, span: RotoscopeSpan<T>) -> Result<Self> {
        if self.span(span.id).is_some() {
            return Err(rotoscope_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "add_span",
                "duplicate_span_id",
                "a rotoscope span with this identity already exists",
            ));
        }
        let mut spans = self.spans.clone();
        spans.push(span);
        self.rebuild(spans)
    }

    /// Replaces one complete span, preserving its stable identity.
    pub fn with_replaced_span(&self, replacement: RotoscopeSpan<T>) -> Result<Self> {
        let index = self.span_index(replacement.id, "replace_span")?;
        let mut spans = self.spans.clone();
        spans[index] = replacement;
        self.rebuild(spans)
    }

    /// Removes one span and all of its authored and derived frame state.
    pub fn without_span(&self, span_id: RotoscopeSpanId) -> Result<Self> {
        let index = self.span_index(span_id, "remove_span")?;
        let mut spans = self.spans.clone();
        spans.remove(index);
        self.rebuild(spans)
    }

    /// Replaces a span's authored base and invalidates every derived frame in that span.
    pub fn with_base(&self, span_id: RotoscopeSpanId, base: RotoscopeFrame<T>) -> Result<Self> {
        ensure_timebase(base.frame, self.timebase, "replace_base")?;
        let index = self.span_index(span_id, "replace_base")?;
        let mut spans = self.spans.clone();
        let span = &mut spans[index];
        ensure_in_range(span.range, base.frame, "replace_base")?;
        if span
            .corrections
            .binary_search_by_key(&base.frame.value(), |candidate| candidate.frame.value())
            .is_ok()
        {
            return Err(rotoscope_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "replace_base",
                "base_conflicts_with_correction",
                "remove the correction at the new base frame before replacing the base",
            ));
        }
        span.base = base;
        span.propagated.clear();
        self.rebuild(spans)
    }

    /// Adds or replaces one complete per-frame correction and invalidates its directional tail.
    pub fn with_correction(
        &self,
        span_id: RotoscopeSpanId,
        correction: RotoscopeFrame<T>,
    ) -> Result<Self> {
        ensure_timebase(correction.frame, self.timebase, "edit_correction")?;
        let index = self.span_index(span_id, "edit_correction")?;
        let mut spans = self.spans.clone();
        let span = &mut spans[index];
        ensure_in_range(span.range, correction.frame, "edit_correction")?;
        if correction.frame == span.base.frame {
            return Err(rotoscope_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "edit_correction",
                "correction_on_base_frame",
                "a per-frame correction cannot replace the span's distinct base frame",
            ));
        }
        invalidate_from(span, correction.frame);
        match span
            .corrections
            .binary_search_by_key(&correction.frame.value(), |candidate| {
                candidate.frame.value()
            }) {
            Ok(correction_index) => span.corrections[correction_index] = correction,
            Err(correction_index) => span.corrections.insert(correction_index, correction),
        }
        self.rebuild(spans)
    }

    /// Removes one correction and invalidates the frames that correction could influence.
    pub fn without_correction(
        &self,
        span_id: RotoscopeSpanId,
        frame: RationalTime,
    ) -> Result<Self> {
        ensure_timebase(frame, self.timebase, "remove_correction")?;
        let index = self.span_index(span_id, "remove_correction")?;
        let mut spans = self.spans.clone();
        let span = &mut spans[index];
        let correction_index = span
            .corrections
            .binary_search_by_key(&frame.value(), |candidate| candidate.frame.value())
            .map_err(|_| {
                rotoscope_error(
                    ErrorCategory::NotFound,
                    Recoverability::UserCorrectable,
                    "remove_correction",
                    "unknown_correction",
                    "no authored correction exists at the requested frame",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "remove_correction")
                        .with_field("span_id", span_id.to_string())
                        .with_field("frame", frame.value().to_string()),
                )
            })?;
        invalidate_from(span, frame);
        span.corrections.remove(correction_index);
        self.rebuild(spans)
    }

    /// Clears replaceable propagation in one direction without changing authored frame state.
    pub fn without_propagation(
        &self,
        span_id: RotoscopeSpanId,
        direction: PropagationDirection,
    ) -> Result<Self> {
        let index = self.span_index(span_id, "clear_propagation")?;
        let mut spans = self.spans.clone();
        let span = &mut spans[index];
        let base_value = span.base.frame.value();
        span.propagated.retain(|sample| match direction {
            PropagationDirection::Forward => sample.frame.value() < base_value,
            PropagationDirection::Backward => sample.frame.value() > base_value,
        });
        self.rebuild(spans)
    }

    /// Builds one bounded request with authored anchors and exact target frames.
    pub fn propagation_request(
        &self,
        span_id: RotoscopeSpanId,
        direction: PropagationDirection,
    ) -> Result<PropagationRequest<T>> {
        let span = self
            .span(span_id)
            .ok_or_else(|| unknown_span("create_propagation_request", span_id))?;
        let base_value = span.base.frame.value();
        let mut anchors = Vec::with_capacity(span.corrections.len() + 1);
        anchors.push(span.base.clone());
        match direction {
            PropagationDirection::Forward => anchors.extend(
                span.corrections
                    .iter()
                    .filter(|correction| correction.frame.value() > base_value)
                    .cloned(),
            ),
            PropagationDirection::Backward => anchors.extend(
                span.corrections
                    .iter()
                    .rev()
                    .filter(|correction| correction.frame.value() < base_value)
                    .cloned(),
            ),
        }

        let end = span.range.end_exclusive()?.value();
        let start = span.range.start().value();
        let is_correction = |value: i64| {
            span.corrections
                .binary_search_by_key(&value, |correction| correction.frame.value())
                .is_ok()
        };
        let target_frames = match direction {
            PropagationDirection::Forward => ((base_value + 1)..end)
                .filter(|value| !is_correction(*value))
                .map(|value| RationalTime::new(value, self.timebase))
                .collect(),
            PropagationDirection::Backward => (start..base_value)
                .rev()
                .filter(|value| !is_correction(*value))
                .map(|value| RationalTime::new(value, self.timebase))
                .collect(),
        };
        Ok(PropagationRequest {
            source_revision: self.content_revision,
            span_id,
            direction,
            span_range: span.range,
            anchors,
            target_frames,
        })
    }

    /// Applies an exact result atomically when its request still matches current state.
    pub fn apply_propagation(&self, result: PropagationResult<T>) -> Result<Self> {
        if result.source_revision != self.content_revision {
            return Err(rotoscope_error(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "apply_propagation",
                "stale_propagation_result",
                "rotoscope state changed after the propagation request was created",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "apply_propagation")
                    .with_field("source_revision", result.source_revision.to_string())
                    .with_field("current_revision", self.content_revision.to_string()),
            ));
        }
        self.span_index(result.span_id, "apply_propagation")?;
        let current_request = self.propagation_request(result.span_id, result.direction)?;
        let current_anchor_frames: Vec<_> = current_request
            .anchors
            .iter()
            .map(RotoscopeFrame::frame)
            .collect();
        if result.span_range != current_request.span_range
            || result.anchor_frames != current_anchor_frames
            || result.target_frames != current_request.target_frames
        {
            return Err(rotoscope_error(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "apply_propagation",
                "propagation_request_mismatch",
                "propagation result does not describe the current span request",
            ));
        }

        let index = self.span_index(result.span_id, "apply_propagation")?;
        let mut spans = self.spans.clone();
        let span = &mut spans[index];
        let base_value = span.base.frame.value();
        span.propagated.retain(|sample| match result.direction {
            PropagationDirection::Forward => sample.frame.value() < base_value,
            PropagationDirection::Backward => sample.frame.value() > base_value,
        });
        span.propagated.extend(result.frames);
        span.propagated.sort_by_key(|sample| sample.frame.value());
        self.rebuild(spans)
    }

    /// Runs a propagation hook and atomically applies its checked result.
    pub fn propagate_with<P>(
        &self,
        span_id: RotoscopeSpanId,
        direction: PropagationDirection,
        propagator: &P,
    ) -> Result<Self>
    where
        P: RotoscopePropagator<T> + ?Sized,
    {
        let request = self.propagation_request(span_id, direction)?;
        let result = propagator.propagate(&request)?;
        self.apply_propagation(result)
    }

    fn span_index(&self, span_id: RotoscopeSpanId, operation: &'static str) -> Result<usize> {
        self.spans
            .iter()
            .position(|span| span.id == span_id)
            .ok_or_else(|| unknown_span(operation, span_id))
    }

    fn rebuild(&self, spans: Vec<RotoscopeSpan<T>>) -> Result<Self> {
        let content_revision = self.content_revision.checked_add(1).ok_or_else(|| {
            rotoscope_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "edit_artifact",
                "revision_overflow",
                "rotoscope artifact revision cannot advance beyond its integer bound",
            )
        })?;
        Self::from_parts(self.timebase, content_revision, spans)
    }
}

impl<T: Serialize> Serialize for RotoscopeArtifact<T> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RotoscopeArtifactWireRef::from(self).serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for RotoscopeArtifact<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        RotoscopeArtifactWire::deserialize(deserializer)?
            .into_artifact()
            .map_err(D::Error::custom)
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RotoscopeArtifactWireRef<'a, T> {
    schema_revision: u32,
    timebase: Timebase,
    content_revision: u64,
    spans: Vec<RotoscopeSpanWireRef<'a, T>>,
}

impl<'a, T> From<&'a RotoscopeArtifact<T>> for RotoscopeArtifactWireRef<'a, T> {
    fn from(artifact: &'a RotoscopeArtifact<T>) -> Self {
        Self {
            schema_revision: ROTOSCOPE_ARTIFACT_SCHEMA_REVISION,
            timebase: artifact.timebase,
            content_revision: artifact.content_revision,
            spans: artifact
                .spans
                .iter()
                .map(RotoscopeSpanWireRef::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RotoscopeSpanWireRef<'a, T> {
    id: u64,
    range: TimeRange,
    base: RotoscopeFrameWireRef<'a, T>,
    corrections: Vec<RotoscopeFrameWireRef<'a, T>>,
    propagated: Vec<RotoscopeFrameWireRef<'a, T>>,
}

impl<'a, T> From<&'a RotoscopeSpan<T>> for RotoscopeSpanWireRef<'a, T> {
    fn from(span: &'a RotoscopeSpan<T>) -> Self {
        Self {
            id: span.id.get(),
            range: span.range,
            base: RotoscopeFrameWireRef::from(&span.base),
            corrections: span
                .corrections
                .iter()
                .map(RotoscopeFrameWireRef::from)
                .collect(),
            propagated: span
                .propagated
                .iter()
                .map(RotoscopeFrameWireRef::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RotoscopeFrameWireRef<'a, T> {
    frame: RationalTime,
    payload: &'a T,
}

impl<'a, T> From<&'a RotoscopeFrame<T>> for RotoscopeFrameWireRef<'a, T> {
    fn from(frame: &'a RotoscopeFrame<T>) -> Self {
        Self {
            frame: frame.frame,
            payload: &frame.payload,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
struct RotoscopeArtifactWire<T> {
    schema_revision: u32,
    timebase: Timebase,
    content_revision: u64,
    #[serde(deserialize_with = "deserialize_bounded_spans")]
    spans: Vec<RotoscopeSpanWire<T>>,
}

impl<T> RotoscopeArtifactWire<T> {
    fn into_artifact(self) -> Result<RotoscopeArtifact<T>> {
        if self.schema_revision != ROTOSCOPE_ARTIFACT_SCHEMA_REVISION {
            return Err(rotoscope_error(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "deserialize_artifact",
                "unsupported_schema_revision",
                "rotoscope artifact schema revision is not supported",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "deserialize_artifact")
                    .with_field("schema_revision", self.schema_revision.to_string())
                    .with_field(
                        "supported_schema_revision",
                        ROTOSCOPE_ARTIFACT_SCHEMA_REVISION.to_string(),
                    ),
            ));
        }
        if self.spans.len() > MAX_ROTOSCOPE_SPANS {
            return Err(rotoscope_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "deserialize_artifact",
                "span_limit_exceeded",
                "serialized rotoscope artifact exceeds the supported span bound",
            ));
        }
        let spans = self
            .spans
            .into_iter()
            .map(RotoscopeSpanWire::into_span)
            .collect::<Result<Vec<_>>>()?;
        RotoscopeArtifact::from_parts(self.timebase, self.content_revision, spans)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
struct RotoscopeSpanWire<T> {
    id: u64,
    range: TimeRange,
    base: RotoscopeFrameWire<T>,
    #[serde(deserialize_with = "deserialize_bounded_frames")]
    corrections: Vec<RotoscopeFrameWire<T>>,
    #[serde(deserialize_with = "deserialize_bounded_frames")]
    propagated: Vec<RotoscopeFrameWire<T>>,
}

impl<T> RotoscopeSpanWire<T> {
    fn into_span(self) -> Result<RotoscopeSpan<T>> {
        let corrections = self
            .corrections
            .into_iter()
            .map(RotoscopeFrameWire::into_frame)
            .collect();
        let propagated = self
            .propagated
            .into_iter()
            .map(RotoscopeFrameWire::into_frame)
            .collect();
        RotoscopeSpan::from_parts(
            RotoscopeSpanId::from_raw(self.id),
            self.range,
            self.base.into_frame(),
            corrections,
            propagated,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RotoscopeFrameWire<T> {
    frame: RationalTime,
    payload: T,
}

impl<T> RotoscopeFrameWire<T> {
    fn into_frame(self) -> RotoscopeFrame<T> {
        RotoscopeFrame::new(self.frame, self.payload)
    }
}

fn deserialize_bounded_spans<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Vec<RotoscopeSpanWire<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::new(
        MAX_ROTOSCOPE_SPANS,
        "rotoscope spans",
    ))
}

fn deserialize_bounded_frames<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Vec<RotoscopeFrameWire<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let limit = usize::try_from(MAX_ROTOSCOPE_FRAMES_PER_SPAN)
        .map_err(|_| D::Error::custom("rotoscope frame bound is not representable"))?;
    deserializer.deserialize_seq(BoundedVecVisitor::new(limit, "rotoscope frame samples"))
}

struct BoundedVecVisitor<T> {
    limit: usize,
    description: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> BoundedVecVisitor<T> {
    const fn new(limit: usize, description: &'static str) -> Self {
        Self {
            limit,
            description,
            marker: PhantomData,
        }
    }
}

impl<'de, T> Visitor<'de> for BoundedVecVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "at most {} {}", self.limit, self.description)
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        if sequence.size_hint().is_some_and(|hint| hint > self.limit) {
            return Err(A::Error::custom(format_args!(
                "{} exceeds the supported bound of {}",
                self.description, self.limit
            )));
        }
        let capacity = sequence.size_hint().unwrap_or(0).min(self.limit);
        let mut values = Vec::with_capacity(capacity);
        loop {
            if values.len() == self.limit {
                if sequence.next_element::<IgnoredAny>()?.is_some() {
                    return Err(A::Error::custom(format_args!(
                        "{} exceeds the supported bound of {}",
                        self.description, self.limit
                    )));
                }
                return Ok(values);
            }
            let Some(value) = sequence.next_element()? else {
                return Ok(values);
            };
            values.push(value);
        }
    }
}

fn validate_span<T>(
    span: &RotoscopeSpan<T>,
    expected_timebase: Timebase,
    operation: &'static str,
) -> Result<()> {
    if span.range.timebase() != expected_timebase {
        return Err(timebase_error(
            operation,
            expected_timebase,
            span.range.timebase(),
        ));
    }
    if span.range.is_empty() {
        return Err(rotoscope_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "empty_span",
            "rotoscope span must contain at least one exact frame",
        ));
    }
    if span.range.duration().value() > MAX_ROTOSCOPE_FRAMES_PER_SPAN {
        return Err(rotoscope_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            operation,
            "span_frame_limit_exceeded",
            "rotoscope span exceeds the supported exact-frame bound",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("span_id", span.id.to_string())
                .with_field("frame_count", span.range.duration().value().to_string())
                .with_field("frame_limit", MAX_ROTOSCOPE_FRAMES_PER_SPAN.to_string()),
        ));
    }
    ensure_timebase(span.base.frame, expected_timebase, operation)?;
    ensure_in_range(span.range, span.base.frame, operation)?;
    validate_frame_sequence(
        &span.corrections,
        span,
        expected_timebase,
        "correction_frame_not_increasing",
        operation,
    )?;
    validate_frame_sequence(
        &span.propagated,
        span,
        expected_timebase,
        "propagation_frame_not_increasing",
        operation,
    )?;
    for propagated in &span.propagated {
        if span
            .corrections
            .binary_search_by_key(&propagated.frame.value(), |frame| frame.frame.value())
            .is_ok()
        {
            return Err(rotoscope_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                operation,
                "propagation_overlaps_correction",
                "derived propagation cannot occupy an authored correction frame",
            ));
        }
    }
    Ok(())
}

fn validate_frame_sequence<T>(
    frames: &[RotoscopeFrame<T>],
    span: &RotoscopeSpan<T>,
    expected_timebase: Timebase,
    unordered_reason: &'static str,
    operation: &'static str,
) -> Result<()> {
    let maximum = usize::try_from(span.range.duration().value()).map_err(|_| {
        rotoscope_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            operation,
            "span_frame_limit_exceeded",
            "rotoscope span frame count cannot be represented on this platform",
        )
    })?;
    if frames.len() > maximum {
        return Err(rotoscope_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            operation,
            "frame_collection_limit_exceeded",
            "rotoscope frame collection exceeds its owning span",
        ));
    }
    let mut previous = None;
    for frame in frames {
        ensure_timebase(frame.frame, expected_timebase, operation)?;
        ensure_in_range(span.range, frame.frame, operation)?;
        if frame.frame == span.base.frame {
            return Err(rotoscope_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                operation,
                "frame_overlaps_base",
                "correction and propagation samples cannot occupy the base frame",
            ));
        }
        if previous.is_some_and(|value| frame.frame.value() <= value) {
            return Err(rotoscope_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                operation,
                unordered_reason,
                "rotoscope frame samples must be unique and strictly increasing",
            ));
        }
        previous = Some(frame.frame.value());
    }
    Ok(())
}

fn ensure_timebase(frame: RationalTime, expected: Timebase, operation: &'static str) -> Result<()> {
    if frame.timebase() != expected {
        return Err(timebase_error(operation, expected, frame.timebase()));
    }
    Ok(())
}

fn timebase_error(operation: &'static str, expected: Timebase, actual: Timebase) -> Error {
    rotoscope_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        operation,
        "timebase_mismatch",
        "rotoscope frame and span state must use the artifact's exact timebase",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("expected_timebase", expected.to_string())
            .with_field("actual_timebase", actual.to_string()),
    )
}

fn ensure_in_range(range: TimeRange, frame: RationalTime, operation: &'static str) -> Result<()> {
    if !range.contains(frame)? {
        return Err(rotoscope_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "frame_outside_span",
            "rotoscope frame must lie inside its owning half-open span",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation).with_field("frame", frame.value().to_string()),
        ));
    }
    Ok(())
}

fn invalidate_from<T>(span: &mut RotoscopeSpan<T>, correction: RationalTime) {
    let base = span.base.frame.value();
    let correction = correction.value();
    span.propagated.retain(|sample| {
        if correction > base {
            sample.frame.value() < correction
        } else {
            sample.frame.value() > correction
        }
    });
}

fn unknown_span(operation: &'static str, span_id: RotoscopeSpanId) -> Error {
    rotoscope_error(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        operation,
        "unknown_span",
        "rotoscope span identity is absent from the artifact",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("span_id", span_id.to_string()),
    )
}

fn rotoscope_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
