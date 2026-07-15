//! Editable, deterministic parameter animation over exact project time.
//!
//! Curves retain authored keyframes, timing intent, interpolation, easing, tangents, and expression
//! source. Derived roving times are recomputed from authored state and never serialized as a second
//! source of truth.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{RationalTime, TimeRounding, Timebase};
use superi_graph::expr::{ExpressionVariableName, ScalarExpression};

const COMPONENT: &str = "superi-effects.keyframe";
const MAX_COMPONENTS: usize = 64;
const MAX_KEYFRAMES: usize = 100_000;
const BEZIER_BISECTION_STEPS: usize = 52;

/// Current incompatible revision of the standalone animation-curve wire contract.
pub const ANIMATION_CURVE_SCHEMA_REVISION: u32 = 1;

/// One finite, nonempty multi-component property value.
///
/// Components are deliberately semantic-neutral. Node parameter schemas decide whether they mean a
/// scalar, position, color, transform control, or another animatable value.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationValue(Vec<f64>);

impl AnimationValue {
    /// Creates one bounded finite animation value.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the value is empty, too wide, or contains a nonfinite component.
    pub fn new(components: impl IntoIterator<Item = f64>) -> Result<Self> {
        let components = components.into_iter().collect::<Vec<_>>();
        validate_components(&components, "create_value", "value")?;
        Ok(Self(components))
    }

    /// Creates one scalar value.
    ///
    /// # Errors
    ///
    /// Returns invalid input when `component` is nonfinite.
    pub fn scalar(component: f64) -> Result<Self> {
        Self::new([component])
    }

    /// Returns the components in stable property order.
    #[must_use]
    pub fn components(&self) -> &[f64] {
        &self.0
    }

    /// Returns the number of independently animated components.
    #[must_use]
    pub fn component_count(&self) -> usize {
        self.0.len()
    }
}

/// Independent graph-editor slope components measured in value units per second.
#[derive(Clone, Debug, PartialEq)]
pub struct ValueTangent(Vec<f64>);

impl ValueTangent {
    /// Creates one finite tangent vector.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the vector is empty, too wide, or contains a nonfinite component.
    pub fn new(components_per_second: impl IntoIterator<Item = f64>) -> Result<Self> {
        let components = components_per_second.into_iter().collect::<Vec<_>>();
        validate_components(&components, "create_tangent", "tangent")?;
        Ok(Self(components))
    }

    /// Returns slope components in stable property order.
    #[must_use]
    pub fn components_per_second(&self) -> &[f64] {
        &self.0
    }

    fn scaled(&self, factor: f64) -> Result<Self> {
        Self::new(self.0.iter().map(|component| component * factor))
    }
}

/// Authored placement of one keyframe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyframeTiming {
    /// One exact locked time on the curve clock.
    Fixed(RationalTime),
    /// An interior time derived from neighboring fixed anchors and value distance.
    Roving,
}

/// The interpolation used by the segment leaving one keyframe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Interpolation {
    /// Interpolates directly between adjacent values.
    Linear,
    /// Uses cubic Hermite interpolation with independent outgoing and incoming value tangents.
    Cubic,
    /// Holds the left value until the next exact key time.
    Hold,
}

/// One cubic Bezier time-easing function with fixed `(0, 0)` and `(1, 1)` endpoints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicEasing {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl CubicEasing {
    /// Creates a finite cubic Bezier easing function.
    ///
    /// The x controls must be in `[0, 1]` so the input progress has one deterministic mapping. The
    /// y controls may be any finite values, preserving intentional overshoot.
    ///
    /// # Errors
    ///
    /// Returns invalid input for nonfinite controls or x controls outside `[0, 1]`.
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Self> {
        if [x1, y1, x2, y2]
            .into_iter()
            .any(|component| !component.is_finite())
        {
            return Err(animation_error(
                "create_easing",
                "nonfinite_easing_control",
                "cubic easing controls must be finite",
            ));
        }
        if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
            return Err(animation_error(
                "create_easing",
                "easing_x_out_of_range",
                "cubic easing x controls must be between zero and one",
            ));
        }
        Ok(Self { x1, y1, x2, y2 })
    }

    /// Returns `[x1, y1, x2, y2]` for graph-editor inspection.
    #[must_use]
    pub const fn control_points(self) -> [f64; 4] {
        [self.x1, self.y1, self.x2, self.y2]
    }

    /// Maps normalized input progress through the easing curve.
    ///
    /// Input is clamped to `[0, 1]`. Endpoint results are exact, while interior x is solved through
    /// a fixed-iteration bisection for deterministic behavior.
    #[must_use]
    pub fn map(self, progress: f64) -> f64 {
        if progress <= 0.0 {
            return 0.0;
        }
        if progress >= 1.0 {
            return 1.0;
        }
        let mut low = 0.0;
        let mut high = 1.0;
        for _ in 0..BEZIER_BISECTION_STEPS {
            let parameter = (low + high) * 0.5;
            if cubic_bezier_coordinate(parameter, self.x1, self.x2) < progress {
                low = parameter;
            } else {
                high = parameter;
            }
        }
        cubic_bezier_coordinate((low + high) * 0.5, self.y1, self.y2)
    }
}

/// Normalized time mapping for the segment leaving one keyframe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    /// Uses normalized segment time unchanged.
    Linear,
    /// Maps normalized segment time through one inspectable cubic Bezier function.
    CubicBezier(CubicEasing),
}

impl Easing {
    fn map(self, progress: f64) -> f64 {
        match self {
            Self::Linear => progress,
            Self::CubicBezier(easing) => easing.map(progress),
        }
    }
}

/// One directly editable authored keyframe.
#[derive(Clone, Debug, PartialEq)]
pub struct Keyframe {
    timing: KeyframeTiming,
    value: AnimationValue,
    outgoing: Interpolation,
    easing: Easing,
    incoming_tangent: Option<ValueTangent>,
    outgoing_tangent: Option<ValueTangent>,
}

impl Keyframe {
    /// Creates one checked keyframe with independent incoming and outgoing value tangents.
    ///
    /// # Errors
    ///
    /// Returns invalid input when either tangent has a different component count from the value.
    pub fn new(
        timing: KeyframeTiming,
        value: AnimationValue,
        outgoing: Interpolation,
        easing: Easing,
        incoming_tangent: Option<ValueTangent>,
        outgoing_tangent: Option<ValueTangent>,
    ) -> Result<Self> {
        for tangent in [incoming_tangent.as_ref(), outgoing_tangent.as_ref()]
            .into_iter()
            .flatten()
        {
            if tangent.0.len() != value.0.len() {
                return Err(animation_error(
                    "create_keyframe",
                    "tangent_component_count_mismatch",
                    "keyframe tangent and value component counts must match",
                ));
            }
        }
        Ok(Self {
            timing,
            value,
            outgoing,
            easing,
            incoming_tangent,
            outgoing_tangent,
        })
    }

    /// Returns the authored fixed or roving timing intent.
    #[must_use]
    pub const fn timing(&self) -> KeyframeTiming {
        self.timing
    }

    /// Returns the exact authored value.
    #[must_use]
    pub const fn value(&self) -> &AnimationValue {
        &self.value
    }

    /// Returns the interpolation for the segment leaving this key.
    #[must_use]
    pub const fn outgoing_interpolation(&self) -> Interpolation {
        self.outgoing
    }

    /// Returns the time easing for the segment leaving this key.
    #[must_use]
    pub const fn easing(&self) -> Easing {
        self.easing
    }

    /// Returns the independent incoming value tangent, when authored.
    #[must_use]
    pub const fn incoming_tangent(&self) -> Option<&ValueTangent> {
        self.incoming_tangent.as_ref()
    }

    /// Returns the independent outgoing value tangent, when authored.
    #[must_use]
    pub const fn outgoing_tangent(&self) -> Option<&ValueTangent> {
        self.outgoing_tangent.as_ref()
    }
}

/// One bounded pure arithmetic expression over `time` and base `value`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeExpression(ScalarExpression);

impl TimeExpression {
    /// Compiles editable expression source through the shared graph expression language.
    ///
    /// `time` is the current exact evaluation time converted to physical seconds. `value` is the
    /// interpolated base component before expression application. Either variable may be unused.
    ///
    /// # Errors
    ///
    /// Returns the shared bounded expression compiler's classified error.
    pub fn compile(source: &str) -> Result<Self> {
        ScalarExpression::compile(
            source,
            [
                ExpressionVariableName::new("time")?,
                ExpressionVariableName::new("value")?,
            ],
        )
        .map(Self)
    }

    /// Returns the editable source after outer whitespace trimming.
    #[must_use]
    pub fn source(&self) -> &str {
        self.0.source()
    }

    /// Returns only variables actually referenced by the source in canonical order.
    #[must_use]
    pub fn variables(&self) -> Vec<&str> {
        self.0
            .variables()
            .iter()
            .map(ExpressionVariableName::as_str)
            .collect()
    }

    fn evaluate(&self, time_seconds: f64, value: f64) -> Result<f64> {
        self.0.evaluate_with(|name| match name.as_str() {
            "time" => Ok(time_seconds),
            "value" => Ok(value),
            _ => Err(animation_error(
                "evaluate_expression",
                "unknown_time_expression_variable",
                "checked time expression contains an unknown variable",
            )),
        })
    }
}

/// One validated immutable animation curve and its derived roving times.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationCurve {
    timebase: Timebase,
    keyframes: Vec<Keyframe>,
    resolved_times: Vec<RationalTime>,
    expression: Option<TimeExpression>,
}

impl AnimationCurve {
    /// Creates a checked animation curve from authored state.
    ///
    /// The first and last keys must be fixed. All fixed keys use `timebase` and increase strictly.
    /// Roving groups receive distinct integer ticks based on cumulative L1 component distance.
    ///
    /// # Errors
    ///
    /// Returns invalid input for empty or oversized curves, mismatched values, invalid timing,
    /// unrepresentable roving placement, or nonfinite path distance.
    pub fn new(
        timebase: Timebase,
        keyframes: impl IntoIterator<Item = Keyframe>,
        expression: Option<TimeExpression>,
    ) -> Result<Self> {
        let keyframes = keyframes.into_iter().collect::<Vec<_>>();
        if keyframes.is_empty() {
            return Err(animation_error(
                "create_curve",
                "empty_curve",
                "animation curve must contain at least one keyframe",
            ));
        }
        if keyframes.len() > MAX_KEYFRAMES {
            return Err(animation_error(
                "create_curve",
                "keyframe_limit",
                "animation curve exceeds the supported keyframe count",
            ));
        }
        if !matches!(
            keyframes.first().map(Keyframe::timing),
            Some(KeyframeTiming::Fixed(_))
        ) || !matches!(
            keyframes.last().map(Keyframe::timing),
            Some(KeyframeTiming::Fixed(_))
        ) {
            return Err(animation_error(
                "create_curve",
                "roving_endpoint",
                "the first and last animation keyframes must be fixed",
            ));
        }

        let component_count = keyframes[0].value.component_count();
        let mut previous_fixed = None;
        for (index, keyframe) in keyframes.iter().enumerate() {
            if keyframe.value.component_count() != component_count {
                return Err(animation_error(
                    "create_curve",
                    "component_count_mismatch",
                    "every animation keyframe must use the same component count",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "create_curve")
                        .with_field("keyframe_index", index.to_string()),
                ));
            }
            validate_keyframe_tangent_dimensions(keyframe, component_count, index)?;
            if let KeyframeTiming::Fixed(time) = keyframe.timing {
                if time.timebase() != timebase {
                    return Err(animation_error(
                        "create_curve",
                        "timebase_mismatch",
                        "fixed keyframe time must use the curve timebase",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "create_curve")
                            .with_field("keyframe_index", index.to_string()),
                    ));
                }
                if previous_fixed.is_some_and(|previous| time.value() <= previous) {
                    return Err(animation_error(
                        "create_curve",
                        "fixed_time_not_increasing",
                        "fixed keyframe times must increase strictly",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "create_curve")
                            .with_field("keyframe_index", index.to_string()),
                    ));
                }
                previous_fixed = Some(time.value());
            }
        }

        let resolved_times = resolve_roving_times(timebase, &keyframes)?;
        Ok(Self {
            timebase,
            keyframes,
            resolved_times,
            expression,
        })
    }

    /// Returns the exact discrete clock used by authored and derived key times.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns authored keyframes in direct editing order.
    #[must_use]
    pub fn keyframes(&self) -> &[Keyframe] {
        &self.keyframes
    }

    /// Returns resolved fixed and derived roving times in keyframe order.
    #[must_use]
    pub fn resolved_times(&self) -> &[RationalTime] {
        &self.resolved_times
    }

    /// Returns the optional editable time expression.
    #[must_use]
    pub const fn expression(&self) -> Option<&TimeExpression> {
        self.expression.as_ref()
    }

    /// Evaluates this curve at one exact physical time.
    ///
    /// Exact key times return exact authored values. Outside the authored range, base animation
    /// clamps to the nearest endpoint. An expression is applied component by component afterward.
    ///
    /// # Errors
    ///
    /// Returns classified invalid input for arithmetic overflow, nonfinite interpolation, or an
    /// expression runtime failure.
    pub fn evaluate(&self, time: RationalTime) -> Result<AnimationValue> {
        let base = self.evaluate_base(time)?;
        let Some(expression) = &self.expression else {
            return Ok(base);
        };
        let time_seconds = rational_time_seconds(time);
        AnimationValue::new(
            base.components()
                .iter()
                .map(|component| expression.evaluate(time_seconds, *component))
                .collect::<Result<Vec<_>>>()?,
        )
    }

    /// Returns a new curve with one keyframe inserted at an authored-order index.
    ///
    /// # Errors
    ///
    /// Returns invalid input for an out-of-range index or any invalid resulting curve state.
    pub fn with_inserted(&self, index: usize, keyframe: Keyframe) -> Result<Self> {
        if index > self.keyframes.len() {
            return Err(index_error("insert_keyframe", index, self.keyframes.len()));
        }
        let mut keyframes = self.keyframes.clone();
        keyframes.insert(index, keyframe);
        Self::new(self.timebase, keyframes, self.expression.clone())
    }

    /// Returns a new curve with one keyframe replaced at an authored-order index.
    ///
    /// # Errors
    ///
    /// Returns invalid input for an out-of-range index or any invalid resulting curve state.
    pub fn with_replaced(&self, index: usize, keyframe: Keyframe) -> Result<Self> {
        if index >= self.keyframes.len() {
            return Err(index_error("replace_keyframe", index, self.keyframes.len()));
        }
        let mut keyframes = self.keyframes.clone();
        keyframes[index] = keyframe;
        Self::new(self.timebase, keyframes, self.expression.clone())
    }

    /// Returns a new curve without the keyframe at an authored-order index.
    ///
    /// # Errors
    ///
    /// Returns invalid input for an out-of-range index or any invalid resulting curve state.
    pub fn without(&self, index: usize) -> Result<Self> {
        if index >= self.keyframes.len() {
            return Err(index_error("remove_keyframe", index, self.keyframes.len()));
        }
        let mut keyframes = self.keyframes.clone();
        keyframes.remove(index);
        Self::new(self.timebase, keyframes, self.expression.clone())
    }

    /// Returns a new curve with replaced or cleared editable expression source.
    ///
    /// # Errors
    ///
    /// Returns invalid input if reconstruction discovers invalid authored curve state.
    pub fn with_expression(&self, expression: Option<TimeExpression>) -> Result<Self> {
        Self::new(self.timebase, self.keyframes.clone(), expression)
    }

    /// Returns a uniformly retimed curve between two new fixed endpoints.
    ///
    /// Every fixed key must map to an exact integer tick. Roving state is retained and recomputed,
    /// while value-per-second tangents scale inversely with duration so the normalized value graph
    /// is preserved.
    ///
    /// # Errors
    ///
    /// Returns invalid input for a single-key curve, unordered endpoints, an inexact fixed-key map,
    /// a nonfinite scaled tangent, or an invalid resulting curve.
    pub fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        if self.keyframes.len() < 2 {
            return Err(animation_error(
                "retime_curve",
                "retime_requires_range",
                "retiming requires at least two keyframes",
            ));
        }
        let new_start = rescale_retime_endpoint(new_start, self.timebase, "start")?;
        let new_end = rescale_retime_endpoint(new_end, self.timebase, "end")?;
        if new_end.value() <= new_start.value() {
            return Err(animation_error(
                "retime_curve",
                "invalid_retime_range",
                "retime end must occur after retime start",
            ));
        }

        let old_start = self.resolved_times[0].value();
        let old_end = self.resolved_times[self.resolved_times.len() - 1].value();
        let old_span = i128::from(old_end) - i128::from(old_start);
        let new_span = i128::from(new_end.value()) - i128::from(new_start.value());
        let tangent_factor = old_span as f64 / new_span as f64;

        let mut keyframes = Vec::with_capacity(self.keyframes.len());
        for keyframe in &self.keyframes {
            let timing = match keyframe.timing {
                KeyframeTiming::Fixed(time) => {
                    let offset = i128::from(time.value()) - i128::from(old_start);
                    let numerator = offset.checked_mul(new_span).ok_or_else(|| {
                        animation_error(
                            "retime_curve",
                            "retime_overflow",
                            "fixed keyframe retime arithmetic overflowed",
                        )
                    })?;
                    if numerator % old_span != 0 {
                        return Err(animation_error(
                            "retime_curve",
                            "inexact_retime",
                            "fixed keyframe cannot map exactly to the curve clock",
                        ));
                    }
                    let mapped = i128::from(new_start.value()) + numerator / old_span;
                    let mapped = i64::try_from(mapped).map_err(|_| {
                        animation_error(
                            "retime_curve",
                            "retime_overflow",
                            "fixed keyframe retime exceeds the supported time range",
                        )
                    })?;
                    KeyframeTiming::Fixed(RationalTime::new(mapped, self.timebase))
                }
                KeyframeTiming::Roving => KeyframeTiming::Roving,
            };
            keyframes.push(Keyframe::new(
                timing,
                keyframe.value.clone(),
                keyframe.outgoing,
                keyframe.easing,
                keyframe
                    .incoming_tangent
                    .as_ref()
                    .map(|tangent| tangent.scaled(tangent_factor))
                    .transpose()?,
                keyframe
                    .outgoing_tangent
                    .as_ref()
                    .map(|tangent| tangent.scaled(tangent_factor))
                    .transpose()?,
            )?);
        }
        Self::new(self.timebase, keyframes, self.expression.clone())
    }

    fn evaluate_base(&self, time: RationalTime) -> Result<AnimationValue> {
        if time <= self.resolved_times[0] {
            return Ok(self.keyframes[0].value.clone());
        }
        let last = self.resolved_times.len() - 1;
        if time >= self.resolved_times[last] {
            return Ok(self.keyframes[last].value.clone());
        }

        for index in 1..self.resolved_times.len() {
            if time == self.resolved_times[index] {
                return Ok(self.keyframes[index].value.clone());
            }
            if time < self.resolved_times[index] {
                return self.evaluate_segment(index - 1, time);
            }
        }
        Ok(self.keyframes[last].value.clone())
    }

    fn evaluate_segment(&self, left_index: usize, time: RationalTime) -> Result<AnimationValue> {
        let right_index = left_index + 1;
        let left = &self.keyframes[left_index];
        let right = &self.keyframes[right_index];
        if left.outgoing == Interpolation::Hold {
            return Ok(left.value.clone());
        }

        let progress = normalized_progress(
            time,
            self.resolved_times[left_index],
            self.resolved_times[right_index],
        )?;
        let eased = left.easing.map(progress);
        let duration_seconds = duration_seconds(
            self.resolved_times[left_index],
            self.resolved_times[right_index],
        );
        let components: Vec<f64> = match left.outgoing {
            Interpolation::Linear => left
                .value
                .0
                .iter()
                .zip(&right.value.0)
                .map(|(left, right)| left + (right - left) * eased)
                .collect(),
            Interpolation::Cubic => left
                .value
                .0
                .iter()
                .zip(&right.value.0)
                .enumerate()
                .map(|(component, (left_value, right_value))| {
                    let secant = (right_value - left_value) / duration_seconds;
                    let outgoing = left
                        .outgoing_tangent
                        .as_ref()
                        .map_or(secant, |tangent| tangent.0[component]);
                    let incoming = right
                        .incoming_tangent
                        .as_ref()
                        .map_or(secant, |tangent| tangent.0[component]);
                    cubic_hermite(
                        *left_value,
                        *right_value,
                        outgoing * duration_seconds,
                        incoming * duration_seconds,
                        eased,
                    )
                })
                .collect(),
            Interpolation::Hold => unreachable!("hold segment returned before interpolation"),
        };
        AnimationValue::new(components)
    }
}

impl Serialize for AnimationCurve {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        AnimationCurveWire::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AnimationCurve {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        AnimationCurveWire::deserialize(deserializer)?
            .into_curve()
            .map_err(D::Error::custom)
    }
}

impl Serialize for TimeExpression {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TimeExpressionWire {
            source: self.source().to_owned(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TimeExpression {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TimeExpressionWire::deserialize(deserializer)?;
        Self::compile(&wire.source).map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnimationCurveWire {
    schema_revision: u32,
    timebase: Timebase,
    keyframes: Vec<KeyframeWire>,
    expression: Option<TimeExpression>,
}

impl AnimationCurveWire {
    fn into_curve(self) -> Result<AnimationCurve> {
        if self.schema_revision != ANIMATION_CURVE_SCHEMA_REVISION {
            return Err(Error::new(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "animation curve schema revision is not supported",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "deserialize_curve")
                    .with_field("reason", "unsupported_schema_revision")
                    .with_field("schema_revision", self.schema_revision.to_string())
                    .with_field(
                        "supported_schema_revision",
                        ANIMATION_CURVE_SCHEMA_REVISION.to_string(),
                    ),
            ));
        }
        AnimationCurve::new(
            self.timebase,
            self.keyframes
                .into_iter()
                .map(KeyframeWire::into_keyframe)
                .collect::<Result<Vec<_>>>()?,
            self.expression,
        )
    }
}

impl From<&AnimationCurve> for AnimationCurveWire {
    fn from(curve: &AnimationCurve) -> Self {
        Self {
            schema_revision: ANIMATION_CURVE_SCHEMA_REVISION,
            timebase: curve.timebase,
            keyframes: curve.keyframes.iter().map(KeyframeWire::from).collect(),
            expression: curve.expression.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyframeWire {
    timing: KeyframeTimingWire,
    value: Vec<f64>,
    outgoing_interpolation: InterpolationWire,
    easing: EasingWire,
    incoming_tangent: Option<Vec<f64>>,
    outgoing_tangent: Option<Vec<f64>>,
}

impl KeyframeWire {
    fn into_keyframe(self) -> Result<Keyframe> {
        Keyframe::new(
            self.timing.into_timing(),
            AnimationValue::new(self.value)?,
            self.outgoing_interpolation.into_interpolation(),
            self.easing.into_easing()?,
            self.incoming_tangent.map(ValueTangent::new).transpose()?,
            self.outgoing_tangent.map(ValueTangent::new).transpose()?,
        )
    }
}

impl From<&Keyframe> for KeyframeWire {
    fn from(keyframe: &Keyframe) -> Self {
        Self {
            timing: KeyframeTimingWire::from(keyframe.timing),
            value: keyframe.value.0.clone(),
            outgoing_interpolation: InterpolationWire::from(keyframe.outgoing),
            easing: EasingWire::from(keyframe.easing),
            incoming_tangent: keyframe
                .incoming_tangent
                .as_ref()
                .map(|tangent| tangent.0.clone()),
            outgoing_tangent: keyframe
                .outgoing_tangent
                .as_ref()
                .map(|tangent| tangent.0.clone()),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum KeyframeTimingWire {
    Fixed { time: RationalTime },
    Roving,
}

impl KeyframeTimingWire {
    const fn into_timing(self) -> KeyframeTiming {
        match self {
            Self::Fixed { time } => KeyframeTiming::Fixed(time),
            Self::Roving => KeyframeTiming::Roving,
        }
    }
}

impl From<KeyframeTiming> for KeyframeTimingWire {
    fn from(timing: KeyframeTiming) -> Self {
        match timing {
            KeyframeTiming::Fixed(time) => Self::Fixed { time },
            KeyframeTiming::Roving => Self::Roving,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum InterpolationWire {
    Linear,
    Cubic,
    Hold,
}

impl InterpolationWire {
    const fn into_interpolation(self) -> Interpolation {
        match self {
            Self::Linear => Interpolation::Linear,
            Self::Cubic => Interpolation::Cubic,
            Self::Hold => Interpolation::Hold,
        }
    }
}

impl From<Interpolation> for InterpolationWire {
    fn from(interpolation: Interpolation) -> Self {
        match interpolation {
            Interpolation::Linear => Self::Linear,
            Interpolation::Cubic => Self::Cubic,
            Interpolation::Hold => Self::Hold,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum EasingWire {
    Linear,
    CubicBezier { x1: f64, y1: f64, x2: f64, y2: f64 },
}

impl EasingWire {
    fn into_easing(self) -> Result<Easing> {
        match self {
            Self::Linear => Ok(Easing::Linear),
            Self::CubicBezier { x1, y1, x2, y2 } => {
                CubicEasing::new(x1, y1, x2, y2).map(Easing::CubicBezier)
            }
        }
    }
}

impl From<Easing> for EasingWire {
    fn from(easing: Easing) -> Self {
        match easing {
            Easing::Linear => Self::Linear,
            Easing::CubicBezier(easing) => {
                let [x1, y1, x2, y2] = easing.control_points();
                Self::CubicBezier { x1, y1, x2, y2 }
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeExpressionWire {
    source: String,
}

fn validate_components(
    components: &[f64],
    operation: &'static str,
    subject: &'static str,
) -> Result<()> {
    if components.is_empty() {
        return Err(animation_error(
            operation,
            if subject == "value" {
                "empty_value"
            } else {
                "empty_tangent"
            },
            "animation components cannot be empty",
        ));
    }
    if components.len() > MAX_COMPONENTS {
        return Err(animation_error(
            operation,
            "component_limit",
            "animation components exceed the supported width",
        ));
    }
    if let Some(index) = components
        .iter()
        .position(|component| !component.is_finite())
    {
        return Err(animation_error(
            operation,
            if subject == "value" {
                "nonfinite_value"
            } else {
                "nonfinite_tangent"
            },
            "animation components must be finite",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation).with_field("component", index.to_string()),
        ));
    }
    Ok(())
}

fn validate_keyframe_tangent_dimensions(
    keyframe: &Keyframe,
    component_count: usize,
    keyframe_index: usize,
) -> Result<()> {
    for tangent in [
        keyframe.incoming_tangent.as_ref(),
        keyframe.outgoing_tangent.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if tangent.0.len() != component_count {
            return Err(animation_error(
                "create_curve",
                "tangent_component_count_mismatch",
                "keyframe tangent and curve component counts must match",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_curve")
                    .with_field("keyframe_index", keyframe_index.to_string()),
            ));
        }
    }
    Ok(())
}

fn resolve_roving_times(timebase: Timebase, keyframes: &[Keyframe]) -> Result<Vec<RationalTime>> {
    let mut resolved = vec![RationalTime::zero(timebase); keyframes.len()];
    let mut left = 0;
    while left < keyframes.len() {
        let KeyframeTiming::Fixed(left_time) = keyframes[left].timing else {
            return Err(animation_error(
                "resolve_roving",
                "roving_without_left_anchor",
                "roving keyframe has no fixed left anchor",
            ));
        };
        resolved[left] = left_time;
        if left + 1 == keyframes.len() {
            break;
        }
        let right = (left + 1..keyframes.len())
            .find(|index| matches!(keyframes[*index].timing, KeyframeTiming::Fixed(_)))
            .ok_or_else(|| {
                animation_error(
                    "resolve_roving",
                    "roving_without_right_anchor",
                    "roving keyframe has no fixed right anchor",
                )
            })?;
        let KeyframeTiming::Fixed(right_time) = keyframes[right].timing else {
            unreachable!("fixed-anchor search returned a fixed keyframe");
        };
        resolved[right] = right_time;
        let intervals = right - left;
        if intervals > 1 {
            allocate_roving_span(
                &keyframes[left..=right],
                &mut resolved[left..=right],
                left_time,
                right_time,
            )?;
        }
        left = right;
    }
    Ok(resolved)
}

fn allocate_roving_span(
    keyframes: &[Keyframe],
    resolved: &mut [RationalTime],
    start: RationalTime,
    end: RationalTime,
) -> Result<()> {
    let intervals = keyframes.len() - 1;
    let span = end.value().checked_sub(start.value()).ok_or_else(|| {
        animation_error(
            "resolve_roving",
            "roving_span_overflow",
            "roving fixed-anchor span exceeds the supported time range",
        )
    })?;
    if span < i64::try_from(intervals).unwrap_or(i64::MAX) {
        return Err(animation_error(
            "resolve_roving",
            "roving_span_too_narrow",
            "roving span has too few integer ticks for distinct keyframes",
        ));
    }

    let mut weights = Vec::with_capacity(intervals);
    for pair in keyframes.windows(2) {
        let mut distance = 0.0;
        for (left, right) in pair[0].value.0.iter().zip(&pair[1].value.0) {
            distance += (right - left).abs();
        }
        if !distance.is_finite() {
            return Err(animation_error(
                "resolve_roving",
                "nonfinite_roving_distance",
                "roving value distance must remain finite",
            ));
        }
        weights.push(distance);
    }
    let total = weights.iter().sum::<f64>();
    if !total.is_finite() {
        return Err(animation_error(
            "resolve_roving",
            "nonfinite_roving_distance",
            "roving total value distance must remain finite",
        ));
    }

    let mut cumulative = 0.0;
    let mut previous = start.value();
    for offset in 1..intervals {
        cumulative += weights[offset - 1];
        let ratio = if total == 0.0 {
            offset as f64 / intervals as f64
        } else {
            cumulative / total
        };
        let relative = (span as f64 * ratio).round().clamp(0.0, span as f64) as i64;
        let remaining = i64::try_from(intervals - offset).unwrap_or(i64::MAX);
        let minimum = previous.checked_add(1).ok_or_else(|| {
            animation_error(
                "resolve_roving",
                "roving_span_overflow",
                "roving allocation exceeded the supported time range",
            )
        })?;
        let maximum = end.value().checked_sub(remaining).ok_or_else(|| {
            animation_error(
                "resolve_roving",
                "roving_span_overflow",
                "roving allocation exceeded the supported time range",
            )
        })?;
        let candidate = start
            .value()
            .checked_add(relative)
            .ok_or_else(|| {
                animation_error(
                    "resolve_roving",
                    "roving_span_overflow",
                    "roving allocation exceeded the supported time range",
                )
            })?
            .clamp(minimum, maximum);
        resolved[offset] = RationalTime::new(candidate, start.timebase());
        previous = candidate;
    }
    Ok(())
}

fn normalized_progress(time: RationalTime, left: RationalTime, right: RationalTime) -> Result<f64> {
    let target_numerator = i128::from(time.value())
        .checked_mul(i128::from(time.timebase().denominator()))
        .ok_or_else(evaluation_time_overflow)?;
    let target_denominator = i128::from(time.timebase().numerator());
    let left_numerator = i128::from(left.value())
        .checked_mul(i128::from(left.timebase().denominator()))
        .ok_or_else(evaluation_time_overflow)?;
    let left_denominator = i128::from(left.timebase().numerator());
    let delta_numerator = target_numerator
        .checked_mul(left_denominator)
        .and_then(|value| {
            left_numerator
                .checked_mul(target_denominator)
                .and_then(|left_value| value.checked_sub(left_value))
        })
        .ok_or_else(evaluation_time_overflow)?;
    let delta_denominator = target_denominator
        .checked_mul(left_denominator)
        .ok_or_else(evaluation_time_overflow)?;
    let segment_seconds = duration_seconds(left, right);
    let progress = (delta_numerator as f64 / delta_denominator as f64) / segment_seconds;
    if !progress.is_finite() {
        return Err(evaluation_time_overflow());
    }
    Ok(progress.clamp(0.0, 1.0))
}

fn duration_seconds(left: RationalTime, right: RationalTime) -> f64 {
    (i128::from(right.value()) - i128::from(left.value())) as f64
        * f64::from(left.timebase().denominator())
        / f64::from(left.timebase().numerator())
}

fn rational_time_seconds(time: RationalTime) -> f64 {
    time.value() as f64 * f64::from(time.timebase().denominator())
        / f64::from(time.timebase().numerator())
}

fn cubic_bezier_coordinate(parameter: f64, first: f64, second: f64) -> f64 {
    let complement = 1.0 - parameter;
    3.0 * complement * complement * parameter * first
        + 3.0 * complement * parameter * parameter * second
        + parameter * parameter * parameter
}

fn cubic_hermite(left: f64, right: f64, left_slope: f64, right_slope: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    (2.0 * t3 - 3.0 * t2 + 1.0) * left
        + (t3 - 2.0 * t2 + t) * left_slope
        + (-2.0 * t3 + 3.0 * t2) * right
        + (t3 - t2) * right_slope
}

fn rescale_retime_endpoint(
    time: RationalTime,
    timebase: Timebase,
    endpoint: &'static str,
) -> Result<RationalTime> {
    time.checked_rescale(timebase, TimeRounding::Exact)
        .map_err(|source| {
            Error::with_source(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "retime endpoint cannot be represented exactly on the curve clock",
                source,
            )
            .with_context(
                ErrorContext::new(COMPONENT, "retime_curve")
                    .with_field("reason", "inexact_retime_endpoint")
                    .with_field("endpoint", endpoint),
            )
        })
}

fn index_error(operation: &'static str, index: usize, length: usize) -> Error {
    animation_error(
        operation,
        "keyframe_index_out_of_range",
        "keyframe edit index is outside the authored curve",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("index", index.to_string())
            .with_field("keyframe_count", length.to_string()),
    )
}

fn evaluation_time_overflow() -> Error {
    animation_error(
        "evaluate_curve",
        "evaluation_time_overflow",
        "animation evaluation time arithmetic overflowed",
    )
}

fn animation_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
