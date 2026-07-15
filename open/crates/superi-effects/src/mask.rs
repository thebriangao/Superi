//! Editable animated mask paths and deterministic soft-coverage composition.
//!
//! This module owns authoring state, exact-time sampling, strict persistence, and ordered alpha
//! composition. A runtime factory remains responsible for rasterizing sampled cubic paths and
//! applying feather and expansion geometry before passing one coverage value per mask to
//! [`MaskStackSample::compose_rasterized_coverages`]. No image or GPU value is hidden here.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::{Point2, Vector2};
use superi_core::time::{RationalTime, Timebase};

use crate::keyframe::{AnimationCurve, Interpolation};

const COMPONENT: &str = "superi-effects.mask";
const MASK_VERTEX_COMPONENTS: usize = 6;
const MIN_PATH_VERTICES: usize = 3;
const MAX_PATH_VERTICES: usize = 4_096;
const MAX_MASKS: usize = 256;

/// Current incompatible revision of the standalone mask-stack wire contract.
pub const MASK_STACK_SCHEMA_REVISION: u32 = 1;

/// The winding rule used to determine the inside of one closed mask path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaskFillRule {
    /// Counts signed crossings and fills points with a nonzero winding count.
    NonZero,
    /// Fills points crossed by an odd number of path edges.
    EvenOdd,
}

/// The ordered alpha operation used to combine one mask with prior stack coverage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaskBooleanOperation {
    /// Replaces prior coverage with this mask.
    Replace,
    /// Combines this mask and prior coverage using source-over alpha union.
    Union,
    /// Removes this mask from prior coverage using destination-out alpha.
    Subtract,
    /// Retains only shared soft coverage using source-in alpha.
    Intersect,
    /// Retains coverage present in exactly one side using Porter-Duff XOR alpha.
    Exclude,
}

/// One sampled cubic-path vertex with handles relative to its anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaskVertex {
    anchor: Point2,
    incoming_handle: Vector2,
    outgoing_handle: Vector2,
}

impl MaskVertex {
    /// Creates one finite sampled vertex.
    #[must_use]
    pub const fn new(anchor: Point2, incoming_handle: Vector2, outgoing_handle: Vector2) -> Self {
        Self {
            anchor,
            incoming_handle,
            outgoing_handle,
        }
    }

    /// Returns the sampled anchor position.
    #[must_use]
    pub const fn anchor(&self) -> Point2 {
        self.anchor
    }

    /// Returns the incoming control displacement relative to the anchor.
    #[must_use]
    pub const fn incoming_handle(&self) -> Vector2 {
        self.incoming_handle
    }

    /// Returns the outgoing control displacement relative to the anchor.
    #[must_use]
    pub const fn outgoing_handle(&self) -> Vector2 {
        self.outgoing_handle
    }
}

/// One explicit cubic Bezier segment of a sampled closed mask path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaskCubicSegment {
    start: Point2,
    control_1: Point2,
    control_2: Point2,
    end: Point2,
}

impl MaskCubicSegment {
    /// Returns the segment start anchor.
    #[must_use]
    pub const fn start(&self) -> Point2 {
        self.start
    }

    /// Returns the first absolute cubic control point.
    #[must_use]
    pub const fn control_1(&self) -> Point2 {
        self.control_1
    }

    /// Returns the second absolute cubic control point.
    #[must_use]
    pub const fn control_2(&self) -> Point2 {
        self.control_2
    }

    /// Returns the segment end anchor.
    #[must_use]
    pub const fn end(&self) -> Point2 {
        self.end
    }
}

/// One six-component animation curve for a stable mask vertex slot.
///
/// Components are anchor x, anchor y, incoming handle x, incoming handle y, outgoing handle x,
/// and outgoing handle y. Topology edits remain explicit because vertex correspondence cannot be
/// inferred safely across time.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskVertexAnimation {
    curve: AnimationCurve,
}

impl MaskVertexAnimation {
    /// Creates a vertex animation from one exact six-component curve.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the curve does not contain exactly six components.
    pub fn new(curve: AnimationCurve) -> Result<Self> {
        if curve.keyframes()[0].value().component_count() != MASK_VERTEX_COMPONENTS {
            return Err(mask_error(
                "create_vertex_animation",
                "vertex_component_count",
                "mask vertex animation must contain exactly six components",
            ));
        }
        Ok(Self { curve })
    }

    /// Returns the complete editable animation curve.
    #[must_use]
    pub const fn curve(&self) -> &AnimationCurve {
        &self.curve
    }

    /// Returns the exact curve clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.curve.timebase()
    }

    /// Samples one finite vertex at an exact physical time.
    pub fn sample(&self, time: RationalTime) -> Result<MaskVertex> {
        let value = self.curve.evaluate(time)?;
        let components = value.components();
        Ok(MaskVertex::new(
            Point2::new(components[0], components[1])?,
            Vector2::new(components[2], components[3])?,
            Vector2::new(components[4], components[5])?,
        ))
    }
}

/// One editable closed path with stable vertex topology and exact animation.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskPathAnimation {
    fill_rule: MaskFillRule,
    vertices: Vec<MaskVertexAnimation>,
    timebase: Timebase,
}

impl MaskPathAnimation {
    /// Creates a checked closed path.
    ///
    /// # Errors
    ///
    /// Returns invalid input for fewer than three vertices, excessive topology, or mixed clocks.
    pub fn new(
        fill_rule: MaskFillRule,
        vertices: impl IntoIterator<Item = MaskVertexAnimation>,
    ) -> Result<Self> {
        let vertices = vertices.into_iter().collect::<Vec<_>>();
        validate_vertex_count(vertices.len(), "create_path")?;
        let timebase = vertices[0].timebase();
        if let Some(index) = vertices
            .iter()
            .position(|vertex| vertex.timebase() != timebase)
        {
            return Err(mask_error(
                "create_path",
                "vertex_timebase_mismatch",
                "every mask path vertex must use the same timebase",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_path")
                    .with_field("vertex_index", index.to_string()),
            ));
        }
        Ok(Self {
            fill_rule,
            vertices,
            timebase,
        })
    }

    /// Returns the path winding rule.
    #[must_use]
    pub const fn fill_rule(&self) -> MaskFillRule {
        self.fill_rule
    }

    /// Returns vertex animations in stable contour order.
    #[must_use]
    pub fn vertices(&self) -> &[MaskVertexAnimation] {
        &self.vertices
    }

    /// Returns the common animation clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns a new path with a different winding rule and unchanged vertex animations.
    pub fn with_fill_rule(&self, fill_rule: MaskFillRule) -> Result<Self> {
        Self::new(fill_rule, self.vertices.clone())
    }

    /// Samples the closed cubic path at an exact physical time.
    pub fn sample(&self, time: RationalTime) -> Result<MaskPath> {
        MaskPath::new(
            self.fill_rule,
            self.vertices
                .iter()
                .map(|vertex| vertex.sample(time))
                .collect::<Result<Vec<_>>>()?,
        )
    }

    /// Returns a new path with one vertex inserted at a contour-order index.
    pub fn with_vertex_inserted(&self, index: usize, vertex: MaskVertexAnimation) -> Result<Self> {
        if index > self.vertices.len() {
            return Err(index_error("insert_vertex", index, self.vertices.len()));
        }
        let mut vertices = self.vertices.clone();
        vertices.insert(index, vertex);
        Self::new(self.fill_rule, vertices)
    }

    /// Returns a new path with one vertex replaced at a contour-order index.
    pub fn with_vertex_replaced(&self, index: usize, vertex: MaskVertexAnimation) -> Result<Self> {
        if index >= self.vertices.len() {
            return Err(index_error("replace_vertex", index, self.vertices.len()));
        }
        let mut vertices = self.vertices.clone();
        vertices[index] = vertex;
        Self::new(self.fill_rule, vertices)
    }

    /// Returns a new path without one contour-order vertex.
    pub fn with_vertex_removed(&self, index: usize) -> Result<Self> {
        if index >= self.vertices.len() {
            return Err(index_error("remove_vertex", index, self.vertices.len()));
        }
        let mut vertices = self.vertices.clone();
        vertices.remove(index);
        Self::new(self.fill_rule, vertices)
    }
}

/// One sampled, closed cubic mask path ready for caller-owned rasterization.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskPath {
    fill_rule: MaskFillRule,
    vertices: Vec<MaskVertex>,
    segments: Vec<MaskCubicSegment>,
}

impl MaskPath {
    fn new(fill_rule: MaskFillRule, vertices: Vec<MaskVertex>) -> Result<Self> {
        validate_vertex_count(vertices.len(), "sample_path")?;
        let mut segments = Vec::with_capacity(vertices.len());
        for index in 0..vertices.len() {
            let start = vertices[index];
            let end = vertices[(index + 1) % vertices.len()];
            segments.push(MaskCubicSegment {
                start: start.anchor,
                control_1: start.anchor.checked_offset(start.outgoing_handle)?,
                control_2: end.anchor.checked_offset(end.incoming_handle)?,
                end: end.anchor,
            });
        }
        Ok(Self {
            fill_rule,
            vertices,
            segments,
        })
    }

    /// Returns the path winding rule.
    #[must_use]
    pub const fn fill_rule(&self) -> MaskFillRule {
        self.fill_rule
    }

    /// Returns sampled vertices in stable contour order.
    #[must_use]
    pub fn vertices(&self) -> &[MaskVertex] {
        &self.vertices
    }

    /// Returns explicit cubic segments, including the closing segment.
    #[must_use]
    pub fn segments(&self) -> &[MaskCubicSegment] {
        &self.segments
    }
}

/// One editable and animatable mask layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    path: MaskPathAnimation,
    feather: AnimationCurve,
    expansion: AnimationCurve,
    opacity: AnimationCurve,
    inversion: AnimationCurve,
    operation: MaskBooleanOperation,
}

impl Mask {
    /// Creates a checked mask with complete editable controls.
    ///
    /// Feather is measured in nonnegative pixels, expansion in signed pixels, opacity in zero
    /// through one, and inversion is a hold-interpolated scalar with exact zero or one values.
    /// Every curve must use the path clock.
    pub fn new(
        path: MaskPathAnimation,
        feather: AnimationCurve,
        expansion: AnimationCurve,
        opacity: AnimationCurve,
        inversion: AnimationCurve,
        operation: MaskBooleanOperation,
    ) -> Result<Self> {
        validate_control_curve(&feather, path.timebase(), MaskControl::Feather)?;
        validate_control_curve(&expansion, path.timebase(), MaskControl::Expansion)?;
        validate_control_curve(&opacity, path.timebase(), MaskControl::Opacity)?;
        validate_control_curve(&inversion, path.timebase(), MaskControl::Inversion)?;
        Ok(Self {
            path,
            feather,
            expansion,
            opacity,
            inversion,
            operation,
        })
    }

    /// Returns the editable animated closed path.
    #[must_use]
    pub const fn path(&self) -> &MaskPathAnimation {
        &self.path
    }

    /// Returns the editable feather-radius animation in pixels.
    #[must_use]
    pub const fn feather(&self) -> &AnimationCurve {
        &self.feather
    }

    /// Returns the editable signed expansion animation in pixels.
    #[must_use]
    pub const fn expansion(&self) -> &AnimationCurve {
        &self.expansion
    }

    /// Returns the editable normalized-opacity animation.
    #[must_use]
    pub const fn opacity(&self) -> &AnimationCurve {
        &self.opacity
    }

    /// Returns the editable hold-interpolated inversion animation.
    #[must_use]
    pub const fn inversion(&self) -> &AnimationCurve {
        &self.inversion
    }

    /// Returns the ordered boolean operation.
    #[must_use]
    pub const fn operation(&self) -> MaskBooleanOperation {
        self.operation
    }

    /// Returns the shared animation clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.path.timebase()
    }

    /// Returns a new checked mask with a replacement animated path.
    pub fn with_path(&self, path: MaskPathAnimation) -> Result<Self> {
        Self::new(
            path,
            self.feather.clone(),
            self.expansion.clone(),
            self.opacity.clone(),
            self.inversion.clone(),
            self.operation,
        )
    }

    /// Returns a new checked mask with replacement feather animation.
    pub fn with_feather(&self, feather: AnimationCurve) -> Result<Self> {
        Self::new(
            self.path.clone(),
            feather,
            self.expansion.clone(),
            self.opacity.clone(),
            self.inversion.clone(),
            self.operation,
        )
    }

    /// Returns a new checked mask with replacement expansion animation.
    pub fn with_expansion(&self, expansion: AnimationCurve) -> Result<Self> {
        Self::new(
            self.path.clone(),
            self.feather.clone(),
            expansion,
            self.opacity.clone(),
            self.inversion.clone(),
            self.operation,
        )
    }

    /// Returns a new checked mask with replacement opacity animation.
    pub fn with_opacity(&self, opacity: AnimationCurve) -> Result<Self> {
        Self::new(
            self.path.clone(),
            self.feather.clone(),
            self.expansion.clone(),
            opacity,
            self.inversion.clone(),
            self.operation,
        )
    }

    /// Returns a new checked mask with replacement inversion animation.
    pub fn with_inversion(&self, inversion: AnimationCurve) -> Result<Self> {
        Self::new(
            self.path.clone(),
            self.feather.clone(),
            self.expansion.clone(),
            self.opacity.clone(),
            inversion,
            self.operation,
        )
    }

    /// Returns a new checked mask with a replacement ordered boolean operation.
    pub fn with_operation(&self, operation: MaskBooleanOperation) -> Result<Self> {
        Self::new(
            self.path.clone(),
            self.feather.clone(),
            self.expansion.clone(),
            self.opacity.clone(),
            self.inversion.clone(),
            operation,
        )
    }

    /// Samples every path and control at one exact physical time.
    ///
    /// Sample-time validation prevents easing or expression overshoot from publishing an illegal
    /// feather, opacity, or inversion value.
    pub fn sample(&self, time: RationalTime) -> Result<MaskSample> {
        let feather_pixels = evaluate_control(&self.feather, time, MaskControl::Feather)?;
        let expansion_pixels = evaluate_control(&self.expansion, time, MaskControl::Expansion)?;
        let opacity = evaluate_control(&self.opacity, time, MaskControl::Opacity)?;
        let inversion = evaluate_control(&self.inversion, time, MaskControl::Inversion)?;
        let is_inverted = match inversion {
            0.0 => false,
            1.0 => true,
            _ => {
                return Err(control_error(
                    "sample_mask",
                    MaskControl::Inversion,
                    "sampled_toggle_out_of_range",
                    "sampled mask inversion must be exactly zero or one",
                ));
            }
        };
        Ok(MaskSample {
            path: self.path.sample(time)?,
            feather_pixels,
            expansion_pixels,
            opacity,
            is_inverted,
            operation: self.operation,
        })
    }
}

/// One exact-time mask sample with no hidden raster or image state.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskSample {
    path: MaskPath,
    feather_pixels: f64,
    expansion_pixels: f64,
    opacity: f64,
    is_inverted: bool,
    operation: MaskBooleanOperation,
}

impl MaskSample {
    /// Returns the sampled closed path for caller-owned rasterization.
    #[must_use]
    pub const fn path(&self) -> &MaskPath {
        &self.path
    }

    /// Returns the sampled nonnegative feather radius in pixels.
    #[must_use]
    pub const fn feather_pixels(&self) -> f64 {
        self.feather_pixels
    }

    /// Returns the sampled signed expansion in pixels.
    #[must_use]
    pub const fn expansion_pixels(&self) -> f64 {
        self.expansion_pixels
    }

    /// Returns sampled normalized mask opacity.
    #[must_use]
    pub const fn opacity(&self) -> f64 {
        self.opacity
    }

    /// Returns the sampled inversion toggle.
    #[must_use]
    pub const fn is_inverted(&self) -> bool {
        self.is_inverted
    }

    /// Returns the ordered boolean operation.
    #[must_use]
    pub const fn operation(&self) -> MaskBooleanOperation {
        self.operation
    }
}

/// One bounded ordered mask artifact reusable as ordinary graph parameter state.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskStack {
    masks: Vec<Mask>,
    timebase: Option<Timebase>,
}

impl MaskStack {
    /// Creates a checked ordered mask stack.
    ///
    /// Empty stacks are valid and mean full unmasked coverage. Nonempty stacks require one shared
    /// clock so every workflow samples the same temporal state.
    pub fn new(masks: impl IntoIterator<Item = Mask>) -> Result<Self> {
        let masks = masks.into_iter().collect::<Vec<_>>();
        if masks.len() > MAX_MASKS {
            return Err(mask_error(
                "create_stack",
                "mask_limit",
                "mask stack exceeds the supported mask count",
            ));
        }
        let timebase = masks.first().map(Mask::timebase);
        if let Some(expected) = timebase {
            if let Some(index) = masks.iter().position(|mask| mask.timebase() != expected) {
                return Err(mask_error(
                    "create_stack",
                    "mask_timebase_mismatch",
                    "every mask in a stack must use the same timebase",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "create_stack")
                        .with_field("mask_index", index.to_string()),
                ));
            }
        }
        Ok(Self { masks, timebase })
    }

    /// Returns masks in canonical composition order.
    #[must_use]
    pub fn masks(&self) -> &[Mask] {
        &self.masks
    }

    /// Returns the shared clock, or `None` for an empty stack.
    #[must_use]
    pub const fn timebase(&self) -> Option<Timebase> {
        self.timebase
    }

    /// Samples the complete stack at one exact physical time.
    pub fn sample(&self, time: RationalTime) -> Result<MaskStackSample> {
        Ok(MaskStackSample {
            time,
            masks: self
                .masks
                .iter()
                .map(|mask| mask.sample(time))
                .collect::<Result<Vec<_>>>()?,
        })
    }

    /// Returns a new stack with one mask inserted at a composition-order index.
    pub fn with_mask_inserted(&self, index: usize, mask: Mask) -> Result<Self> {
        if index > self.masks.len() {
            return Err(index_error("insert_mask", index, self.masks.len()));
        }
        let mut masks = self.masks.clone();
        masks.insert(index, mask);
        Self::new(masks)
    }

    /// Returns a new stack with one mask replaced at a composition-order index.
    pub fn with_mask_replaced(&self, index: usize, mask: Mask) -> Result<Self> {
        if index >= self.masks.len() {
            return Err(index_error("replace_mask", index, self.masks.len()));
        }
        let mut masks = self.masks.clone();
        masks[index] = mask;
        Self::new(masks)
    }

    /// Returns a new stack without one composition-order mask.
    pub fn with_mask_removed(&self, index: usize) -> Result<Self> {
        if index >= self.masks.len() {
            return Err(index_error("remove_mask", index, self.masks.len()));
        }
        let mut masks = self.masks.clone();
        masks.remove(index);
        Self::new(masks)
    }
}

/// One exact-time ordered mask stack awaiting caller-owned path rasterization.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskStackSample {
    time: RationalTime,
    masks: Vec<MaskSample>,
}

impl MaskStackSample {
    /// Returns the exact requested sample time.
    #[must_use]
    pub const fn time(&self) -> RationalTime {
        self.time
    }

    /// Returns sampled masks in canonical composition order.
    #[must_use]
    pub fn masks(&self) -> &[MaskSample] {
        &self.masks
    }

    /// Composes one already-rasterized path coverage per sampled mask.
    ///
    /// Each input must be normalized coverage after the caller applies this sample's path,
    /// fill rule, expansion, and feather. This method applies inversion, opacity, and the ordered
    /// boolean operation using deterministic Porter-Duff alpha equations. An empty stack returns
    /// full coverage because it imposes no mask.
    pub fn compose_rasterized_coverages(&self, coverages: &[f64]) -> Result<f64> {
        if coverages.len() != self.masks.len() {
            return Err(mask_error(
                "compose_coverages",
                "coverage_count_mismatch",
                "rasterized coverage count must match sampled mask count",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compose_coverages")
                    .with_field("coverage_count", coverages.len().to_string())
                    .with_field("mask_count", self.masks.len().to_string()),
            ));
        }
        if self.masks.is_empty() {
            return Ok(1.0);
        }

        let mut destination = 0.0;
        for (index, (mask, coverage)) in self.masks.iter().zip(coverages).enumerate() {
            if !coverage.is_finite() || !(0.0..=1.0).contains(coverage) {
                return Err(mask_error(
                    "compose_coverages",
                    "coverage_out_of_range",
                    "rasterized mask coverage must be finite and between zero and one",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "compose_coverages")
                        .with_field("mask_index", index.to_string()),
                ));
            }
            let source = if mask.is_inverted {
                1.0 - coverage
            } else {
                *coverage
            } * mask.opacity;
            destination = match mask.operation {
                MaskBooleanOperation::Replace => source,
                MaskBooleanOperation::Union => source + destination * (1.0 - source),
                MaskBooleanOperation::Subtract => destination * (1.0 - source),
                MaskBooleanOperation::Intersect => destination * source,
                MaskBooleanOperation::Exclude => {
                    source * (1.0 - destination) + destination * (1.0 - source)
                }
            }
            .clamp(0.0, 1.0);
        }
        Ok(destination)
    }
}

impl Serialize for MaskStack {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        MaskStackWire::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MaskStack {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        MaskStackWire::deserialize(deserializer)?
            .into_stack()
            .map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaskStackWire {
    schema_revision: u32,
    masks: Vec<MaskWire>,
}

impl MaskStackWire {
    fn into_stack(self) -> Result<MaskStack> {
        if self.schema_revision != MASK_STACK_SCHEMA_REVISION {
            return Err(Error::new(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "mask stack schema revision is not supported",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "deserialize_stack")
                    .with_field("reason", "unsupported_schema_revision")
                    .with_field("schema_revision", self.schema_revision.to_string())
                    .with_field(
                        "supported_schema_revision",
                        MASK_STACK_SCHEMA_REVISION.to_string(),
                    ),
            ));
        }
        MaskStack::new(
            self.masks
                .into_iter()
                .map(MaskWire::into_mask)
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

impl From<&MaskStack> for MaskStackWire {
    fn from(stack: &MaskStack) -> Self {
        Self {
            schema_revision: MASK_STACK_SCHEMA_REVISION,
            masks: stack.masks.iter().map(MaskWire::from).collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaskWire {
    path: MaskPathWire,
    feather: AnimationCurve,
    expansion: AnimationCurve,
    opacity: AnimationCurve,
    inversion: AnimationCurve,
    operation: MaskBooleanOperationWire,
}

impl MaskWire {
    fn into_mask(self) -> Result<Mask> {
        Mask::new(
            self.path.into_path()?,
            self.feather,
            self.expansion,
            self.opacity,
            self.inversion,
            self.operation.into_operation(),
        )
    }
}

impl From<&Mask> for MaskWire {
    fn from(mask: &Mask) -> Self {
        Self {
            path: MaskPathWire::from(&mask.path),
            feather: mask.feather.clone(),
            expansion: mask.expansion.clone(),
            opacity: mask.opacity.clone(),
            inversion: mask.inversion.clone(),
            operation: MaskBooleanOperationWire::from(mask.operation),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaskPathWire {
    fill_rule: MaskFillRuleWire,
    vertices: Vec<AnimationCurve>,
}

impl MaskPathWire {
    fn into_path(self) -> Result<MaskPathAnimation> {
        MaskPathAnimation::new(
            self.fill_rule.into_fill_rule(),
            self.vertices
                .into_iter()
                .map(MaskVertexAnimation::new)
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

impl From<&MaskPathAnimation> for MaskPathWire {
    fn from(path: &MaskPathAnimation) -> Self {
        Self {
            fill_rule: MaskFillRuleWire::from(path.fill_rule),
            vertices: path
                .vertices
                .iter()
                .map(|vertex| vertex.curve.clone())
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MaskFillRuleWire {
    NonZero,
    EvenOdd,
}

impl MaskFillRuleWire {
    const fn into_fill_rule(self) -> MaskFillRule {
        match self {
            Self::NonZero => MaskFillRule::NonZero,
            Self::EvenOdd => MaskFillRule::EvenOdd,
        }
    }
}

impl From<MaskFillRule> for MaskFillRuleWire {
    fn from(fill_rule: MaskFillRule) -> Self {
        match fill_rule {
            MaskFillRule::NonZero => Self::NonZero,
            MaskFillRule::EvenOdd => Self::EvenOdd,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MaskBooleanOperationWire {
    Replace,
    Union,
    Subtract,
    Intersect,
    Exclude,
}

impl MaskBooleanOperationWire {
    const fn into_operation(self) -> MaskBooleanOperation {
        match self {
            Self::Replace => MaskBooleanOperation::Replace,
            Self::Union => MaskBooleanOperation::Union,
            Self::Subtract => MaskBooleanOperation::Subtract,
            Self::Intersect => MaskBooleanOperation::Intersect,
            Self::Exclude => MaskBooleanOperation::Exclude,
        }
    }
}

impl From<MaskBooleanOperation> for MaskBooleanOperationWire {
    fn from(operation: MaskBooleanOperation) -> Self {
        match operation {
            MaskBooleanOperation::Replace => Self::Replace,
            MaskBooleanOperation::Union => Self::Union,
            MaskBooleanOperation::Subtract => Self::Subtract,
            MaskBooleanOperation::Intersect => Self::Intersect,
            MaskBooleanOperation::Exclude => Self::Exclude,
        }
    }
}

#[derive(Clone, Copy)]
enum MaskControl {
    Feather,
    Expansion,
    Opacity,
    Inversion,
}

impl MaskControl {
    const fn name(self) -> &'static str {
        match self {
            Self::Feather => "feather",
            Self::Expansion => "expansion",
            Self::Opacity => "opacity",
            Self::Inversion => "inversion",
        }
    }
}

fn validate_control_curve(
    curve: &AnimationCurve,
    timebase: Timebase,
    control: MaskControl,
) -> Result<()> {
    if curve.timebase() != timebase {
        return Err(control_error(
            "create_mask",
            control,
            "control_timebase_mismatch",
            "mask control curve must use the path timebase",
        ));
    }
    if curve.keyframes()[0].value().component_count() != 1 {
        return Err(control_error(
            "create_mask",
            control,
            "control_component_count",
            "mask control curve must contain exactly one component",
        ));
    }
    for (index, keyframe) in curve.keyframes().iter().enumerate() {
        let value = keyframe.value().components()[0];
        validate_control_value(value, control, "authored")?;
        if matches!(control, MaskControl::Inversion)
            && keyframe.outgoing_interpolation() != Interpolation::Hold
        {
            return Err(control_error(
                "create_mask",
                control,
                "toggle_interpolation",
                "mask inversion animation must use hold interpolation",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_mask")
                    .with_field("keyframe_index", index.to_string()),
            ));
        }
    }
    Ok(())
}

fn evaluate_control(
    curve: &AnimationCurve,
    time: RationalTime,
    control: MaskControl,
) -> Result<f64> {
    let value = curve.evaluate(time)?;
    let component = value.components()[0];
    validate_control_value(component, control, "sampled")?;
    Ok(component)
}

fn validate_control_value(value: f64, control: MaskControl, stage: &'static str) -> Result<()> {
    let valid = match control {
        MaskControl::Feather => value >= 0.0,
        MaskControl::Expansion => true,
        MaskControl::Opacity => (0.0..=1.0).contains(&value),
        MaskControl::Inversion => value == 0.0 || value == 1.0,
    };
    if valid {
        return Ok(());
    }
    Err(control_error(
        if stage == "authored" {
            "create_mask"
        } else {
            "sample_mask"
        },
        control,
        if stage == "authored" {
            "authored_control_out_of_range"
        } else {
            "sampled_control_out_of_range"
        },
        match control {
            MaskControl::Feather => "mask feather must be nonnegative",
            MaskControl::Expansion => "mask expansion must be finite",
            MaskControl::Opacity => "mask opacity must be between zero and one",
            MaskControl::Inversion => "mask inversion must be exactly zero or one",
        },
    ))
}

fn validate_vertex_count(count: usize, operation: &'static str) -> Result<()> {
    if count < MIN_PATH_VERTICES {
        return Err(mask_error(
            operation,
            "path_too_short",
            "closed mask path must contain at least three vertices",
        ));
    }
    if count > MAX_PATH_VERTICES {
        return Err(mask_error(
            operation,
            "vertex_limit",
            "mask path exceeds the supported vertex count",
        ));
    }
    Ok(())
}

fn index_error(operation: &'static str, index: usize, len: usize) -> Error {
    mask_error(
        operation,
        "index_out_of_range",
        "mask edit index is outside the authored order",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("index", index.to_string())
            .with_field("len", len.to_string()),
    )
}

fn control_error(
    operation: &'static str,
    control: MaskControl,
    reason: &'static str,
    message: &'static str,
) -> Error {
    mask_error(operation, reason, message)
        .with_context(ErrorContext::new(COMPONENT, operation).with_field("control", control.name()))
}

fn mask_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
