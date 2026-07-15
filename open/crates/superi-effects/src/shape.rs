//! Editable vector-shape authoring and exact-time sampling.
//!
//! This module retains renderer-neutral vector state. Stable cubic vertex slots keep path
//! interpolation directly editable, while sampled artifacts expose explicit segments for timeline,
//! graph, script, and runtime consumers. Image allocation, antialiasing, alpha association, and GPU
//! execution remain outside this authoring boundary.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::{Matrix3, Point2, Vector2};
use superi_core::time::{RationalTime, Timebase};

use crate::keyframe::{AnimationCurve, Interpolation};

const COMPONENT: &str = "superi-effects.shape";
const PATH_VERTEX_COMPONENTS: usize = 6;
const MIN_OPEN_PATH_VERTICES: usize = 2;
const MIN_CLOSED_PATH_VERTICES: usize = 3;
const MAX_PATH_VERTICES: usize = 4_096;
const MAX_REPEATER_COPIES: u32 = 4_096;
const MAX_VECTOR_SHAPES: usize = 4_096;

/// Current incompatible revision of the standalone vector-shape document contract.
pub const VECTOR_SHAPE_DOCUMENT_SCHEMA_REVISION: u32 = 1;

/// One sampled cubic-path vertex with handles relative to its anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathVertex {
    anchor: Point2,
    incoming_handle: Vector2,
    outgoing_handle: Vector2,
}

impl PathVertex {
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

/// One explicit cubic Bezier segment from a sampled vector path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathCubicSegment {
    start: Point2,
    control_1: Point2,
    control_2: Point2,
    end: Point2,
}

impl PathCubicSegment {
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

/// One stable animated vertex slot.
///
/// Components are anchor x and y, incoming-handle x and y, and outgoing-handle x and y. Handles
/// remain relative to the anchor so direct edits and interpolation preserve the authored topology.
#[derive(Clone, Debug, PartialEq)]
pub struct PathVertexAnimation {
    curve: AnimationCurve,
}

impl PathVertexAnimation {
    /// Creates one vertex animation from an exact six-component curve.
    pub fn new(curve: AnimationCurve) -> Result<Self> {
        require_curve_components(&curve, PATH_VERTEX_COMPONENTS, "create_path_vertex")?;
        Ok(Self { curve })
    }

    /// Returns the directly editable component curve.
    #[must_use]
    pub const fn curve(&self) -> &AnimationCurve {
        &self.curve
    }

    /// Returns the exact authored clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.curve.timebase()
    }

    /// Samples the complete vertex at one exact project time.
    pub fn sample(&self, time: RationalTime) -> Result<PathVertex> {
        let value = self.curve.evaluate(time)?;
        let components = value.components();
        Ok(PathVertex::new(
            Point2::new(components[0], components[1])?,
            Vector2::new(components[2], components[3])?,
            Vector2::new(components[4], components[5])?,
        ))
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(self.curve.retimed(new_start, new_end)?)
    }
}

/// One directly editable open or closed cubic path with stable animated topology.
#[derive(Clone, Debug, PartialEq)]
pub struct PathAnimation {
    closed: bool,
    vertices: Vec<PathVertexAnimation>,
    timebase: Timebase,
}

impl PathAnimation {
    /// Creates one checked path and requires every vertex slot to use one exact clock.
    pub fn new(
        closed: bool,
        vertices: impl IntoIterator<Item = PathVertexAnimation>,
    ) -> Result<Self> {
        let vertices = vertices.into_iter().collect::<Vec<_>>();
        let minimum = if closed {
            MIN_CLOSED_PATH_VERTICES
        } else {
            MIN_OPEN_PATH_VERTICES
        };
        if vertices.len() < minimum {
            return Err(shape_error(
                "create_path",
                "too_few_path_vertices",
                "vector path does not contain enough vertices for its closure mode",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_path")
                    .with_field("closed", closed.to_string())
                    .with_field("vertex_count", vertices.len().to_string())
                    .with_field("minimum", minimum.to_string()),
            ));
        }
        if vertices.len() > MAX_PATH_VERTICES {
            return Err(shape_error(
                "create_path",
                "path_vertex_limit",
                "vector path exceeds the supported vertex count",
            ));
        }
        let timebase = vertices[0].timebase();
        require_common_timebase(
            timebase,
            vertices.iter().map(PathVertexAnimation::timebase),
            "create_path",
        )?;
        Ok(Self {
            closed,
            vertices,
            timebase,
        })
    }

    /// Returns whether the final vertex connects back to the first.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Returns stable vertex slots in direct editing order.
    #[must_use]
    pub fn vertices(&self) -> &[PathVertexAnimation] {
        &self.vertices
    }

    /// Returns the shared exact clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns an immutable copy with a different closure mode.
    pub fn with_closed(&self, closed: bool) -> Result<Self> {
        Self::new(closed, self.vertices.clone())
    }

    /// Inserts one stable vertex slot without changing this path.
    pub fn with_vertex_inserted(&self, index: usize, vertex: PathVertexAnimation) -> Result<Self> {
        if index > self.vertices.len() {
            return Err(index_error(
                "insert_path_vertex",
                index,
                self.vertices.len(),
            ));
        }
        let mut vertices = self.vertices.clone();
        vertices.insert(index, vertex);
        Self::new(self.closed, vertices)
    }

    /// Replaces one stable vertex slot without changing this path.
    pub fn with_vertex_replaced(&self, index: usize, vertex: PathVertexAnimation) -> Result<Self> {
        if index >= self.vertices.len() {
            return Err(index_error(
                "replace_path_vertex",
                index,
                self.vertices.len(),
            ));
        }
        let mut vertices = self.vertices.clone();
        vertices[index] = vertex;
        Self::new(self.closed, vertices)
    }

    /// Removes one stable vertex slot without changing this path.
    pub fn with_vertex_removed(&self, index: usize) -> Result<Self> {
        if index >= self.vertices.len() {
            return Err(index_error(
                "remove_path_vertex",
                index,
                self.vertices.len(),
            ));
        }
        let mut vertices = self.vertices.clone();
        vertices.remove(index);
        Self::new(self.closed, vertices)
    }

    /// Retimes every vertex curve over one exact interval while preserving topology.
    pub fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        let vertices = self
            .vertices
            .iter()
            .map(|vertex| vertex.retimed(new_start, new_end))
            .collect::<Result<Vec<_>>>()?;
        Self::new(self.closed, vertices)
    }

    /// Samples all stable slots and constructs explicit cubic segments.
    pub fn sample(&self, time: RationalTime) -> Result<PathSample> {
        let vertices = self
            .vertices
            .iter()
            .map(|vertex| vertex.sample(time))
            .collect::<Result<Vec<_>>>()?;
        PathSample::new(self.closed, vertices)
    }
}

/// One renderer-ready sampled cubic path.
#[derive(Clone, Debug, PartialEq)]
pub struct PathSample {
    closed: bool,
    vertices: Vec<PathVertex>,
    segments: Vec<PathCubicSegment>,
}

impl PathSample {
    fn new(closed: bool, vertices: Vec<PathVertex>) -> Result<Self> {
        let segment_count = if closed {
            vertices.len()
        } else {
            vertices.len().saturating_sub(1)
        };
        let mut segments = Vec::with_capacity(segment_count);
        for index in 0..segment_count {
            let start = vertices[index];
            let end = vertices[(index + 1) % vertices.len()];
            segments.push(PathCubicSegment {
                start: start.anchor,
                control_1: start.anchor.checked_offset(start.outgoing_handle)?,
                control_2: end.anchor.checked_offset(end.incoming_handle)?,
                end: end.anchor,
            });
        }
        Ok(Self {
            closed,
            vertices,
            segments,
        })
    }

    /// Returns whether this path includes its closing cubic segment.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Returns sampled vertices in stable authoring order.
    #[must_use]
    pub fn vertices(&self) -> &[PathVertex] {
        &self.vertices
    }

    /// Returns explicit cubic segments in drawing order.
    #[must_use]
    pub fn segments(&self) -> &[PathCubicSegment] {
        &self.segments
    }
}

/// One sampled straight scene-linear ACEScg RGBA color.
///
/// RGB components may retain finite scene-referred values outside zero to one. Alpha is always in
/// zero to one. Premultiplication remains an explicit image-composition operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeColor([f64; 4]);

impl ShapeColor {
    fn new(components: [f64; 4], operation: &'static str) -> Result<Self> {
        if !(0.0..=1.0).contains(&components[3]) {
            return Err(shape_error(
                operation,
                "color_alpha_out_of_range",
                "shape color alpha must be between zero and one",
            ));
        }
        Ok(Self(components))
    }

    /// Returns straight scene-linear red, green, blue, and alpha components.
    #[must_use]
    pub const fn components(self) -> [f64; 4] {
        self.0
    }

    fn interpolated(self, other: Self, progress: f64) -> Result<Self> {
        let mut components = [0.0; 4];
        for (index, value) in components.iter_mut().enumerate() {
            *value = self.0[index] + (other.0[index] - self.0[index]) * progress;
        }
        if components.iter().any(|component| !component.is_finite()) {
            return Err(shape_error(
                "sample_gradient",
                "nonfinite_gradient_color",
                "gradient interpolation produced a nonfinite color",
            ));
        }
        Self::new(components, "sample_gradient")
    }
}

/// One directly editable four-component scene-linear color animation.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeColorAnimation {
    curve: AnimationCurve,
}

impl ShapeColorAnimation {
    /// Creates one color animation and validates every authored alpha value.
    pub fn new(curve: AnimationCurve) -> Result<Self> {
        require_curve_components(&curve, 4, "create_color_animation")?;
        require_keyframe_component_range(
            &curve,
            3,
            0.0,
            1.0,
            "create_color_animation",
            "color_alpha_out_of_range",
            "shape color alpha must be between zero and one",
        )?;
        Ok(Self { curve })
    }

    /// Returns the directly editable RGBA curve.
    #[must_use]
    pub const fn curve(&self) -> &AnimationCurve {
        &self.curve
    }

    /// Returns the exact authored clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.curve.timebase()
    }

    /// Samples straight scene-linear RGBA at one exact time.
    pub fn sample(&self, time: RationalTime) -> Result<ShapeColor> {
        let value = self.curve.evaluate(time)?;
        ShapeColor::new(
            value
                .components()
                .try_into()
                .expect("color curve has four components"),
            "sample_color_animation",
        )
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(self.curve.retimed(new_start, new_end)?)
    }
}

/// How gradient parameters outside the authored zero-to-one interval are mapped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GradientSpread {
    /// Clamp to the nearest endpoint stop.
    Pad,
    /// Repeat the stop interval in the same direction.
    Repeat,
    /// Repeat the stop interval while reversing every second interval.
    Reflect,
}

/// One stable animated gradient stop.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientStopAnimation {
    offset: AnimationCurve,
    color: ShapeColorAnimation,
}

impl GradientStopAnimation {
    /// Creates one stop with an animated normalized offset and animated color.
    pub fn new(offset: AnimationCurve, color: ShapeColorAnimation) -> Result<Self> {
        require_curve_components(&offset, 1, "create_gradient_stop")?;
        require_all_keyframe_components(
            &offset,
            |value| (0.0..=1.0).contains(&value),
            "create_gradient_stop",
            "gradient_stop_out_of_range",
            "gradient stop offsets must be between zero and one",
        )?;
        require_common_timebase(
            offset.timebase(),
            [color.timebase()],
            "create_gradient_stop",
        )?;
        Ok(Self { offset, color })
    }

    /// Returns the directly editable offset curve.
    #[must_use]
    pub const fn offset(&self) -> &AnimationCurve {
        &self.offset
    }

    /// Returns the directly editable color curve.
    #[must_use]
    pub const fn color(&self) -> &ShapeColorAnimation {
        &self.color
    }

    /// Returns the shared exact clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.offset.timebase()
    }

    /// Returns an immutable copy with a replacement offset curve.
    pub fn with_offset(&self, offset: AnimationCurve) -> Result<Self> {
        Self::new(offset, self.color.clone())
    }

    /// Returns an immutable copy with a replacement color curve.
    pub fn with_color(&self, color: ShapeColorAnimation) -> Result<Self> {
        Self::new(self.offset.clone(), color)
    }

    fn sample(&self, time: RationalTime) -> Result<GradientStopSample> {
        let offset = sample_scalar(&self.offset, time, "sample_gradient_stop")?;
        if !(0.0..=1.0).contains(&offset) {
            return Err(shape_error(
                "sample_gradient_stop",
                "gradient_stop_out_of_range",
                "sampled gradient stop offset must be between zero and one",
            ));
        }
        Ok(GradientStopSample {
            offset,
            color: self.color.sample(time)?,
        })
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            self.offset.retimed(new_start, new_end)?,
            self.color.retimed(new_start, new_end)?,
        )
    }
}

/// One sampled ordered gradient stop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStopSample {
    offset: f64,
    color: ShapeColor,
}

impl GradientStopSample {
    /// Returns the sampled normalized stop position.
    #[must_use]
    pub const fn offset(self) -> f64 {
        self.offset
    }

    /// Returns the sampled straight scene-linear color.
    #[must_use]
    pub const fn color(self) -> ShapeColor {
        self.color
    }
}

#[derive(Clone, Debug, PartialEq)]
enum GradientGeometryKind {
    Linear {
        start: AnimationCurve,
        end: AnimationCurve,
    },
    Radial {
        center: AnimationCurve,
        radius: AnimationCurve,
    },
}

/// Editable linear or radial gradient geometry under one exact clock.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientGeometry {
    kind: GradientGeometryKind,
    timebase: Timebase,
}

impl GradientGeometry {
    /// Creates animated linear-gradient endpoints.
    pub fn linear(start: AnimationCurve, end: AnimationCurve) -> Result<Self> {
        require_curve_components(&start, 2, "create_linear_gradient")?;
        require_curve_components(&end, 2, "create_linear_gradient")?;
        require_common_timebase(start.timebase(), [end.timebase()], "create_linear_gradient")?;
        Ok(Self {
            timebase: start.timebase(),
            kind: GradientGeometryKind::Linear { start, end },
        })
    }

    /// Creates an animated radial-gradient center and positive radius.
    pub fn radial(center: AnimationCurve, radius: AnimationCurve) -> Result<Self> {
        require_curve_components(&center, 2, "create_radial_gradient")?;
        require_curve_components(&radius, 1, "create_radial_gradient")?;
        require_all_keyframe_components(
            &radius,
            |value| value > 0.0,
            "create_radial_gradient",
            "nonpositive_gradient_radius",
            "radial gradient radius must be greater than zero",
        )?;
        require_common_timebase(
            center.timebase(),
            [radius.timebase()],
            "create_radial_gradient",
        )?;
        Ok(Self {
            timebase: center.timebase(),
            kind: GradientGeometryKind::Radial { center, radius },
        })
    }

    /// Returns the exact shared clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    fn sample(&self, time: RationalTime) -> Result<GradientGeometrySample> {
        match &self.kind {
            GradientGeometryKind::Linear { start, end } => Ok(GradientGeometrySample::Linear {
                start: sample_point(start, time, "sample_linear_gradient")?,
                end: sample_point(end, time, "sample_linear_gradient")?,
            }),
            GradientGeometryKind::Radial { center, radius } => {
                let radius = sample_scalar(radius, time, "sample_radial_gradient")?;
                if radius <= 0.0 {
                    return Err(shape_error(
                        "sample_radial_gradient",
                        "nonpositive_gradient_radius",
                        "sampled radial gradient radius must be greater than zero",
                    ));
                }
                Ok(GradientGeometrySample::Radial {
                    center: sample_point(center, time, "sample_radial_gradient")?,
                    radius,
                })
            }
        }
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        match &self.kind {
            GradientGeometryKind::Linear { start, end } => Self::linear(
                start.retimed(new_start, new_end)?,
                end.retimed(new_start, new_end)?,
            ),
            GradientGeometryKind::Radial { center, radius } => Self::radial(
                center.retimed(new_start, new_end)?,
                radius.retimed(new_start, new_end)?,
            ),
        }
    }
}

/// Sampled renderer-ready gradient geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientGeometrySample {
    /// A linear gradient from start to end.
    Linear {
        /// Sampled start point.
        start: Point2,
        /// Sampled end point.
        end: Point2,
    },
    /// A radial gradient from its center through a positive radius.
    Radial {
        /// Sampled center point.
        center: Point2,
        /// Sampled positive radius.
        radius: f64,
    },
}

/// One editable gradient paint.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientPaint {
    geometry: GradientGeometry,
    stops: Vec<GradientStopAnimation>,
    spread: GradientSpread,
    timebase: Timebase,
}

impl GradientPaint {
    /// Creates one checked gradient with stable ordered stop slots.
    pub fn new(
        geometry: GradientGeometry,
        stops: impl IntoIterator<Item = GradientStopAnimation>,
        spread: GradientSpread,
    ) -> Result<Self> {
        let stops = stops.into_iter().collect::<Vec<_>>();
        if stops.len() < 2 {
            return Err(shape_error(
                "create_gradient",
                "too_few_gradient_stops",
                "gradient paint requires at least two stops",
            ));
        }
        if stops.len() > 256 {
            return Err(shape_error(
                "create_gradient",
                "gradient_stop_limit",
                "gradient paint exceeds the supported stop count",
            ));
        }
        let timebase = geometry.timebase();
        require_common_timebase(
            timebase,
            stops.iter().map(GradientStopAnimation::timebase),
            "create_gradient",
        )?;
        Ok(Self {
            geometry,
            stops,
            spread,
            timebase,
        })
    }

    /// Returns editable gradient geometry.
    #[must_use]
    pub const fn geometry(&self) -> &GradientGeometry {
        &self.geometry
    }

    /// Returns stable stop slots in authoring order.
    #[must_use]
    pub fn stops(&self) -> &[GradientStopAnimation] {
        &self.stops
    }

    /// Returns the explicit out-of-range spread behavior.
    #[must_use]
    pub const fn spread(&self) -> GradientSpread {
        self.spread
    }

    /// Returns the shared exact clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns an immutable copy with replacement gradient geometry.
    pub fn with_geometry(&self, geometry: GradientGeometry) -> Result<Self> {
        Self::new(geometry, self.stops.clone(), self.spread)
    }

    /// Returns an immutable copy with replacement spread behavior.
    pub fn with_spread(&self, spread: GradientSpread) -> Result<Self> {
        Self::new(self.geometry.clone(), self.stops.clone(), spread)
    }

    /// Inserts one stable gradient-stop slot without changing this paint.
    pub fn with_stop_inserted(&self, index: usize, stop: GradientStopAnimation) -> Result<Self> {
        if index > self.stops.len() {
            return Err(gradient_stop_index_error(
                "insert_gradient_stop",
                index,
                self.stops.len(),
            ));
        }
        let mut stops = self.stops.clone();
        stops.insert(index, stop);
        Self::new(self.geometry.clone(), stops, self.spread)
    }

    /// Replaces one stable gradient-stop slot without changing this paint.
    pub fn with_stop_replaced(&self, index: usize, stop: GradientStopAnimation) -> Result<Self> {
        if index >= self.stops.len() {
            return Err(gradient_stop_index_error(
                "replace_gradient_stop",
                index,
                self.stops.len(),
            ));
        }
        let mut stops = self.stops.clone();
        stops[index] = stop;
        Self::new(self.geometry.clone(), stops, self.spread)
    }

    /// Removes one stable gradient-stop slot without changing this paint.
    pub fn with_stop_removed(&self, index: usize) -> Result<Self> {
        if index >= self.stops.len() {
            return Err(gradient_stop_index_error(
                "remove_gradient_stop",
                index,
                self.stops.len(),
            ));
        }
        let mut stops = self.stops.clone();
        stops.remove(index);
        Self::new(self.geometry.clone(), stops, self.spread)
    }

    /// Samples gradient geometry and ordered stops at one exact time.
    pub fn sample(&self, time: RationalTime) -> Result<GradientPaintSample> {
        let stops = self
            .stops
            .iter()
            .map(|stop| stop.sample(time))
            .collect::<Result<Vec<_>>>()?;
        if stops.windows(2).any(|pair| pair[0].offset > pair[1].offset) {
            return Err(shape_error(
                "sample_gradient",
                "gradient_stop_order",
                "sampled gradient stop offsets must remain nondecreasing",
            ));
        }
        Ok(GradientPaintSample {
            geometry: self.geometry.sample(time)?,
            stops,
            spread: self.spread,
        })
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            self.geometry.retimed(new_start, new_end)?,
            self.stops
                .iter()
                .map(|stop| stop.retimed(new_start, new_end))
                .collect::<Result<Vec<_>>>()?,
            self.spread,
        )
    }
}

/// One sampled gradient with executable spread and component interpolation.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientPaintSample {
    geometry: GradientGeometrySample,
    stops: Vec<GradientStopSample>,
    spread: GradientSpread,
}

impl GradientPaintSample {
    /// Returns sampled geometry.
    #[must_use]
    pub const fn geometry(&self) -> GradientGeometrySample {
        self.geometry
    }

    /// Returns sampled ordered stops.
    #[must_use]
    pub fn stops(&self) -> &[GradientStopSample] {
        &self.stops
    }

    /// Returns the explicit spread behavior.
    #[must_use]
    pub const fn spread(&self) -> GradientSpread {
        self.spread
    }

    /// Evaluates straight scene-linear color at one finite point.
    pub fn color_at(&self, point: Point2) -> Result<ShapeColor> {
        let parameter = match self.geometry {
            GradientGeometrySample::Linear { start, end } => {
                let direction = start.checked_vector_to(end)?;
                let from_start = start.checked_vector_to(point)?;
                let length_squared = direction.x() * direction.x() + direction.y() * direction.y();
                if length_squared == 0.0 {
                    1.0
                } else {
                    (from_start.x() * direction.x() + from_start.y() * direction.y())
                        / length_squared
                }
            }
            GradientGeometrySample::Radial { center, radius } => {
                let displacement = center.checked_vector_to(point)?;
                (displacement.x() * displacement.x() + displacement.y() * displacement.y()).sqrt()
                    / radius
            }
        };
        if !parameter.is_finite() {
            return Err(shape_error(
                "sample_gradient",
                "nonfinite_gradient_parameter",
                "gradient geometry produced a nonfinite parameter",
            ));
        }
        let parameter = match self.spread {
            GradientSpread::Pad => parameter.clamp(0.0, 1.0),
            GradientSpread::Repeat => parameter.rem_euclid(1.0),
            GradientSpread::Reflect => {
                let period = parameter.rem_euclid(2.0);
                if period <= 1.0 {
                    period
                } else {
                    2.0 - period
                }
            }
        };
        sample_gradient_stops(&self.stops, parameter)
    }
}

/// Editable solid or gradient paint.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// One animated straight scene-linear color.
    Solid(ShapeColorAnimation),
    /// One animated linear or radial gradient.
    Gradient(GradientPaint),
}

impl Paint {
    /// Returns the exact authored clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        match self {
            Self::Solid(color) => color.timebase(),
            Self::Gradient(gradient) => gradient.timebase(),
        }
    }

    /// Samples renderer-ready paint at one exact time.
    pub fn sample(&self, time: RationalTime) -> Result<PaintSample> {
        match self {
            Self::Solid(color) => Ok(PaintSample::Solid(color.sample(time)?)),
            Self::Gradient(gradient) => Ok(PaintSample::Gradient(gradient.sample(time)?)),
        }
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        match self {
            Self::Solid(color) => Ok(Self::Solid(color.retimed(new_start, new_end)?)),
            Self::Gradient(gradient) => Ok(Self::Gradient(gradient.retimed(new_start, new_end)?)),
        }
    }
}

/// Sampled solid or gradient paint.
#[derive(Clone, Debug, PartialEq)]
pub enum PaintSample {
    /// One straight scene-linear color.
    Solid(ShapeColor),
    /// One executable sampled gradient.
    Gradient(GradientPaintSample),
}

impl PaintSample {
    /// Borrows the sampled gradient when present.
    #[must_use]
    pub const fn gradient(&self) -> Option<&GradientPaintSample> {
        match self {
            Self::Gradient(gradient) => Some(gradient),
            Self::Solid(_) => None,
        }
    }

    /// Evaluates this paint at one finite point.
    pub fn color_at(&self, point: Point2) -> Result<ShapeColor> {
        match self {
            Self::Solid(color) => Ok(*color),
            Self::Gradient(gradient) => gradient.color_at(point),
        }
    }
}

/// The winding rule retained for closed fill geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FillRule {
    /// Fill points with a nonzero winding count.
    NonZero,
    /// Fill points crossed by an odd number of path edges.
    EvenOdd,
}

/// One editable fill operation.
#[derive(Clone, Debug, PartialEq)]
pub struct FillStyle {
    rule: FillRule,
    paint: Paint,
    opacity: AnimationCurve,
    timebase: Timebase,
}

impl FillStyle {
    /// Creates a fill with animated paint and normalized opacity.
    pub fn new(rule: FillRule, paint: Paint, opacity: AnimationCurve) -> Result<Self> {
        validate_unit_scalar_curve(&opacity, "create_fill", "fill_opacity_out_of_range")?;
        require_common_timebase(paint.timebase(), [opacity.timebase()], "create_fill")?;
        Ok(Self {
            rule,
            timebase: paint.timebase(),
            paint,
            opacity,
        })
    }

    /// Returns the editable winding rule.
    #[must_use]
    pub const fn rule(&self) -> FillRule {
        self.rule
    }

    /// Returns editable paint.
    #[must_use]
    pub const fn paint(&self) -> &Paint {
        &self.paint
    }

    /// Returns the animated normalized opacity curve.
    #[must_use]
    pub const fn opacity(&self) -> &AnimationCurve {
        &self.opacity
    }

    /// Returns the shared exact clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns an immutable copy with a replacement winding rule.
    pub fn with_rule(&self, rule: FillRule) -> Result<Self> {
        Self::new(rule, self.paint.clone(), self.opacity.clone())
    }

    /// Returns an immutable copy with replacement paint.
    pub fn with_paint(&self, paint: Paint) -> Result<Self> {
        Self::new(self.rule, paint, self.opacity.clone())
    }

    /// Returns an immutable copy with replacement opacity animation.
    pub fn with_opacity(&self, opacity: AnimationCurve) -> Result<Self> {
        Self::new(self.rule, self.paint.clone(), opacity)
    }

    /// Samples this fill at one exact time.
    pub fn sample(&self, time: RationalTime) -> Result<FillStyleSample> {
        Ok(FillStyleSample {
            rule: self.rule,
            paint: self.paint.sample(time)?,
            opacity: sample_unit_scalar(
                &self.opacity,
                time,
                "sample_fill",
                "fill_opacity_out_of_range",
            )?,
        })
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            self.rule,
            self.paint.retimed(new_start, new_end)?,
            self.opacity.retimed(new_start, new_end)?,
        )
    }
}

/// One sampled renderer-ready fill.
#[derive(Clone, Debug, PartialEq)]
pub struct FillStyleSample {
    rule: FillRule,
    paint: PaintSample,
    opacity: f64,
}

impl FillStyleSample {
    /// Returns the sampled winding rule.
    #[must_use]
    pub const fn rule(&self) -> FillRule {
        self.rule
    }

    /// Returns sampled paint.
    #[must_use]
    pub const fn paint(&self) -> &PaintSample {
        &self.paint
    }

    /// Returns sampled normalized opacity.
    #[must_use]
    pub const fn opacity(&self) -> f64 {
        self.opacity
    }
}

/// Stroke end-cap geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineCap {
    /// Stop at the endpoint.
    Butt,
    /// Add a semicircle centered at the endpoint.
    Round,
    /// Add a half-width square beyond the endpoint.
    Square,
}

/// Stroke corner geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineJoin {
    /// Extend edges until the miter limit, then bevel.
    Miter,
    /// Join with a circular arc.
    Round,
    /// Join with a straight bevel edge.
    Bevel,
}

/// One editable stroke operation.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    paint: Paint,
    opacity: AnimationCurve,
    width: AnimationCurve,
    cap: LineCap,
    join: LineJoin,
    miter_limit: AnimationCurve,
    dash_pattern: Vec<AnimationCurve>,
    dash_offset: AnimationCurve,
    timebase: Timebase,
}

impl StrokeStyle {
    /// Creates one checked stroke with animated geometry and paint state.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        paint: Paint,
        opacity: AnimationCurve,
        width: AnimationCurve,
        cap: LineCap,
        join: LineJoin,
        miter_limit: AnimationCurve,
        dash_pattern: impl IntoIterator<Item = AnimationCurve>,
        dash_offset: AnimationCurve,
    ) -> Result<Self> {
        validate_unit_scalar_curve(&opacity, "create_stroke", "stroke_opacity_out_of_range")?;
        validate_nonnegative_scalar_curve(
            &width,
            "create_stroke",
            "negative_stroke_width",
            "stroke width must be nonnegative",
        )?;
        validate_scalar_curve_minimum(
            &miter_limit,
            1.0,
            "create_stroke",
            "miter_limit_out_of_range",
            "stroke miter limit must be at least one",
        )?;
        require_curve_components(&dash_offset, 1, "create_stroke")?;
        let dash_pattern = dash_pattern.into_iter().collect::<Vec<_>>();
        if dash_pattern.len() > 128 {
            return Err(shape_error(
                "create_stroke",
                "dash_pattern_limit",
                "stroke dash pattern exceeds the supported element count",
            ));
        }
        for dash in &dash_pattern {
            validate_nonnegative_scalar_curve(
                dash,
                "create_stroke",
                "negative_dash_length",
                "stroke dash lengths must be nonnegative",
            )?;
        }
        let timebase = paint.timebase();
        require_common_timebase(
            timebase,
            [
                opacity.timebase(),
                width.timebase(),
                miter_limit.timebase(),
                dash_offset.timebase(),
            ]
            .into_iter()
            .chain(dash_pattern.iter().map(AnimationCurve::timebase)),
            "create_stroke",
        )?;
        Ok(Self {
            paint,
            opacity,
            width,
            cap,
            join,
            miter_limit,
            dash_pattern,
            dash_offset,
            timebase,
        })
    }

    /// Returns editable paint.
    #[must_use]
    pub const fn paint(&self) -> &Paint {
        &self.paint
    }

    /// Returns animated normalized opacity.
    #[must_use]
    pub const fn opacity(&self) -> &AnimationCurve {
        &self.opacity
    }

    /// Returns animated nonnegative width.
    #[must_use]
    pub const fn width(&self) -> &AnimationCurve {
        &self.width
    }

    /// Returns editable line-cap geometry.
    #[must_use]
    pub const fn cap(&self) -> LineCap {
        self.cap
    }

    /// Returns editable line-join geometry.
    #[must_use]
    pub const fn join(&self) -> LineJoin {
        self.join
    }

    /// Returns animated miter-limit state.
    #[must_use]
    pub const fn miter_limit(&self) -> &AnimationCurve {
        &self.miter_limit
    }

    /// Returns editable dash and gap element curves.
    #[must_use]
    pub fn dash_pattern(&self) -> &[AnimationCurve] {
        &self.dash_pattern
    }

    /// Returns the animated dash offset.
    #[must_use]
    pub const fn dash_offset(&self) -> &AnimationCurve {
        &self.dash_offset
    }

    /// Returns the shared exact clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns an immutable copy with replacement paint.
    pub fn with_paint(&self, paint: Paint) -> Result<Self> {
        Self::new(
            paint,
            self.opacity.clone(),
            self.width.clone(),
            self.cap,
            self.join,
            self.miter_limit.clone(),
            self.dash_pattern.clone(),
            self.dash_offset.clone(),
        )
    }

    /// Returns an immutable copy with replacement opacity animation.
    pub fn with_opacity(&self, opacity: AnimationCurve) -> Result<Self> {
        Self::new(
            self.paint.clone(),
            opacity,
            self.width.clone(),
            self.cap,
            self.join,
            self.miter_limit.clone(),
            self.dash_pattern.clone(),
            self.dash_offset.clone(),
        )
    }

    /// Returns an immutable copy with replacement width animation.
    pub fn with_width(&self, width: AnimationCurve) -> Result<Self> {
        Self::new(
            self.paint.clone(),
            self.opacity.clone(),
            width,
            self.cap,
            self.join,
            self.miter_limit.clone(),
            self.dash_pattern.clone(),
            self.dash_offset.clone(),
        )
    }

    /// Returns an immutable copy with replacement cap geometry.
    pub fn with_cap(&self, cap: LineCap) -> Result<Self> {
        Self::new(
            self.paint.clone(),
            self.opacity.clone(),
            self.width.clone(),
            cap,
            self.join,
            self.miter_limit.clone(),
            self.dash_pattern.clone(),
            self.dash_offset.clone(),
        )
    }

    /// Returns an immutable copy with replacement join geometry.
    pub fn with_join(&self, join: LineJoin) -> Result<Self> {
        Self::new(
            self.paint.clone(),
            self.opacity.clone(),
            self.width.clone(),
            self.cap,
            join,
            self.miter_limit.clone(),
            self.dash_pattern.clone(),
            self.dash_offset.clone(),
        )
    }

    /// Returns an immutable copy with replacement miter-limit animation.
    pub fn with_miter_limit(&self, miter_limit: AnimationCurve) -> Result<Self> {
        Self::new(
            self.paint.clone(),
            self.opacity.clone(),
            self.width.clone(),
            self.cap,
            self.join,
            miter_limit,
            self.dash_pattern.clone(),
            self.dash_offset.clone(),
        )
    }

    /// Returns an immutable copy with replacement animated dash and gap slots.
    pub fn with_dash_pattern(
        &self,
        dash_pattern: impl IntoIterator<Item = AnimationCurve>,
    ) -> Result<Self> {
        Self::new(
            self.paint.clone(),
            self.opacity.clone(),
            self.width.clone(),
            self.cap,
            self.join,
            self.miter_limit.clone(),
            dash_pattern,
            self.dash_offset.clone(),
        )
    }

    /// Returns an immutable copy with replacement dash-offset animation.
    pub fn with_dash_offset(&self, dash_offset: AnimationCurve) -> Result<Self> {
        Self::new(
            self.paint.clone(),
            self.opacity.clone(),
            self.width.clone(),
            self.cap,
            self.join,
            self.miter_limit.clone(),
            self.dash_pattern.clone(),
            dash_offset,
        )
    }

    /// Samples renderer-ready stroke state at one exact time.
    pub fn sample(&self, time: RationalTime) -> Result<StrokeStyleSample> {
        let opacity = sample_unit_scalar(
            &self.opacity,
            time,
            "sample_stroke",
            "stroke_opacity_out_of_range",
        )?;
        let width = sample_scalar(&self.width, time, "sample_stroke")?;
        if width < 0.0 {
            return Err(shape_error(
                "sample_stroke",
                "negative_stroke_width",
                "sampled stroke width must be nonnegative",
            ));
        }
        let miter_limit = sample_scalar(&self.miter_limit, time, "sample_stroke")?;
        if miter_limit < 1.0 {
            return Err(shape_error(
                "sample_stroke",
                "miter_limit_out_of_range",
                "sampled stroke miter limit must be at least one",
            ));
        }
        let mut dash_pattern = self
            .dash_pattern
            .iter()
            .map(|curve| sample_scalar(curve, time, "sample_stroke"))
            .collect::<Result<Vec<_>>>()?;
        if dash_pattern.iter().any(|length| *length < 0.0) {
            return Err(shape_error(
                "sample_stroke",
                "negative_dash_length",
                "sampled stroke dash lengths must be nonnegative",
            ));
        }
        if dash_pattern.len() % 2 == 1 {
            let repeated = dash_pattern.clone();
            dash_pattern.extend(repeated);
        }
        Ok(StrokeStyleSample {
            paint: self.paint.sample(time)?,
            opacity,
            width,
            cap: self.cap,
            join: self.join,
            miter_limit,
            dash_pattern,
            dash_offset: sample_scalar(&self.dash_offset, time, "sample_stroke")?,
        })
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            self.paint.retimed(new_start, new_end)?,
            self.opacity.retimed(new_start, new_end)?,
            self.width.retimed(new_start, new_end)?,
            self.cap,
            self.join,
            self.miter_limit.retimed(new_start, new_end)?,
            self.dash_pattern
                .iter()
                .map(|curve| curve.retimed(new_start, new_end))
                .collect::<Result<Vec<_>>>()?,
            self.dash_offset.retimed(new_start, new_end)?,
        )
    }
}

/// One sampled renderer-ready stroke.
#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyleSample {
    paint: PaintSample,
    opacity: f64,
    width: f64,
    cap: LineCap,
    join: LineJoin,
    miter_limit: f64,
    dash_pattern: Vec<f64>,
    dash_offset: f64,
}

impl StrokeStyleSample {
    /// Returns sampled paint.
    #[must_use]
    pub const fn paint(&self) -> &PaintSample {
        &self.paint
    }

    /// Returns sampled normalized opacity.
    #[must_use]
    pub const fn opacity(&self) -> f64 {
        self.opacity
    }

    /// Returns sampled nonnegative width.
    #[must_use]
    pub const fn width(&self) -> f64 {
        self.width
    }

    /// Returns sampled line-cap geometry.
    #[must_use]
    pub const fn cap(&self) -> LineCap {
        self.cap
    }

    /// Returns sampled line-join geometry.
    #[must_use]
    pub const fn join(&self) -> LineJoin {
        self.join
    }

    /// Returns sampled miter limit.
    #[must_use]
    pub const fn miter_limit(&self) -> f64 {
        self.miter_limit
    }

    /// Returns the effective even-length dash and gap pattern.
    #[must_use]
    pub fn dash_pattern(&self) -> &[f64] {
        &self.dash_pattern
    }

    /// Returns sampled dash offset.
    #[must_use]
    pub const fn dash_offset(&self) -> f64 {
        self.dash_offset
    }

    /// Returns whether a nonnegative path distance lies in a painted dash.
    pub fn is_dash_on(&self, distance: f64) -> Result<bool> {
        if !distance.is_finite() || distance < 0.0 {
            return Err(shape_error(
                "sample_stroke_dash",
                "invalid_path_distance",
                "stroke dash query distance must be finite and nonnegative",
            ));
        }
        let total = self.dash_pattern.iter().sum::<f64>();
        if self.dash_pattern.is_empty() || total == 0.0 {
            return Ok(true);
        }
        if !total.is_finite() {
            return Err(shape_error(
                "sample_stroke_dash",
                "nonfinite_dash_pattern",
                "stroke dash pattern length is nonfinite",
            ));
        }
        let mut phase = (distance + self.dash_offset).rem_euclid(total);
        for (index, length) in self.dash_pattern.iter().copied().enumerate() {
            if phase < length {
                return Ok(index % 2 == 0);
            }
            phase -= length;
        }
        Ok(true)
    }
}

/// Placement of virtual repeater copies relative to their source shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepeaterComposite {
    /// Composite virtual copies above the source shape.
    Above,
    /// Composite virtual copies below the source shape.
    Below,
}

/// Editable transform applied at each repeater exponent.
#[derive(Clone, Debug, PartialEq)]
pub struct RepeaterTransformAnimation {
    anchor: AnimationCurve,
    position: AnimationCurve,
    scale: AnimationCurve,
    rotation_degrees: AnimationCurve,
    timebase: Timebase,
}

impl RepeaterTransformAnimation {
    /// Creates animated anchor, position, positive scale factors, and rotation in degrees.
    pub fn new(
        anchor: AnimationCurve,
        position: AnimationCurve,
        scale: AnimationCurve,
        rotation_degrees: AnimationCurve,
    ) -> Result<Self> {
        require_curve_components(&anchor, 2, "create_repeater_transform")?;
        require_curve_components(&position, 2, "create_repeater_transform")?;
        require_curve_components(&scale, 2, "create_repeater_transform")?;
        require_all_keyframe_components(
            &scale,
            |value| value > 0.0,
            "create_repeater_transform",
            "nonpositive_repeater_scale",
            "repeater scale factors must be greater than zero",
        )?;
        require_curve_components(&rotation_degrees, 1, "create_repeater_transform")?;
        let timebase = anchor.timebase();
        require_common_timebase(
            timebase,
            [
                position.timebase(),
                scale.timebase(),
                rotation_degrees.timebase(),
            ],
            "create_repeater_transform",
        )?;
        Ok(Self {
            anchor,
            position,
            scale,
            rotation_degrees,
            timebase,
        })
    }

    /// Returns the animated anchor point.
    #[must_use]
    pub const fn anchor(&self) -> &AnimationCurve {
        &self.anchor
    }

    /// Returns the animated per-exponent displacement.
    #[must_use]
    pub const fn position(&self) -> &AnimationCurve {
        &self.position
    }

    /// Returns animated positive per-exponent scale factors.
    #[must_use]
    pub const fn scale(&self) -> &AnimationCurve {
        &self.scale
    }

    /// Returns animated per-exponent rotation in degrees.
    #[must_use]
    pub const fn rotation_degrees(&self) -> &AnimationCurve {
        &self.rotation_degrees
    }

    /// Returns the shared exact clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns an immutable copy with replacement anchor animation.
    pub fn with_anchor(&self, anchor: AnimationCurve) -> Result<Self> {
        Self::new(
            anchor,
            self.position.clone(),
            self.scale.clone(),
            self.rotation_degrees.clone(),
        )
    }

    /// Returns an immutable copy with replacement position animation.
    pub fn with_position(&self, position: AnimationCurve) -> Result<Self> {
        Self::new(
            self.anchor.clone(),
            position,
            self.scale.clone(),
            self.rotation_degrees.clone(),
        )
    }

    /// Returns an immutable copy with replacement scale animation.
    pub fn with_scale(&self, scale: AnimationCurve) -> Result<Self> {
        Self::new(
            self.anchor.clone(),
            self.position.clone(),
            scale,
            self.rotation_degrees.clone(),
        )
    }

    /// Returns an immutable copy with replacement rotation animation.
    pub fn with_rotation_degrees(&self, rotation_degrees: AnimationCurve) -> Result<Self> {
        Self::new(
            self.anchor.clone(),
            self.position.clone(),
            self.scale.clone(),
            rotation_degrees,
        )
    }

    fn sample(&self, time: RationalTime) -> Result<RepeaterTransformSample> {
        let scale = sample_vector(&self.scale, time, "sample_repeater_transform")?;
        if scale.x() <= 0.0 || scale.y() <= 0.0 {
            return Err(shape_error(
                "sample_repeater_transform",
                "nonpositive_repeater_scale",
                "sampled repeater scale factors must be greater than zero",
            ));
        }
        Ok(RepeaterTransformSample {
            anchor: sample_point(&self.anchor, time, "sample_repeater_transform")?,
            position: sample_vector(&self.position, time, "sample_repeater_transform")?,
            scale,
            rotation_degrees: sample_scalar(
                &self.rotation_degrees,
                time,
                "sample_repeater_transform",
            )?,
        })
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            self.anchor.retimed(new_start, new_end)?,
            self.position.retimed(new_start, new_end)?,
            self.scale.retimed(new_start, new_end)?,
            self.rotation_degrees.retimed(new_start, new_end)?,
        )
    }
}

/// Sampled per-exponent repeater transform state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RepeaterTransformSample {
    anchor: Point2,
    position: Vector2,
    scale: Vector2,
    rotation_degrees: f64,
}

impl RepeaterTransformSample {
    /// Returns the sampled anchor point.
    #[must_use]
    pub const fn anchor(self) -> Point2 {
        self.anchor
    }

    /// Returns the sampled per-exponent displacement.
    #[must_use]
    pub const fn position(self) -> Vector2 {
        self.position
    }

    /// Returns sampled positive per-exponent scale factors.
    #[must_use]
    pub const fn scale(self) -> Vector2 {
        self.scale
    }

    /// Returns sampled per-exponent rotation in degrees.
    #[must_use]
    pub const fn rotation_degrees(self) -> f64 {
        self.rotation_degrees
    }

    fn transform_at(self, exponent: f64) -> Result<Matrix3> {
        if !exponent.is_finite() {
            return Err(shape_error(
                "sample_repeater",
                "nonfinite_repeater_exponent",
                "repeater copy exponent must be finite",
            ));
        }
        let scale = Vector2::new(self.scale.x().powf(exponent), self.scale.y().powf(exponent))?;
        let radians = self.rotation_degrees.to_radians() * exponent;
        if !radians.is_finite() {
            return Err(shape_error(
                "sample_repeater",
                "nonfinite_repeater_rotation",
                "repeater copy rotation must be finite",
            ));
        }
        let sine = radians.sin();
        let cosine = radians.cos();
        let rotation =
            Matrix3::from_rows([[cosine, -sine, 0.0], [sine, cosine, 0.0], [0.0, 0.0, 1.0]])?;
        let moved = self.position.checked_scale(exponent)?;
        Matrix3::translation(Vector2::new(-self.anchor.x(), -self.anchor.y())?)
            .checked_then(Matrix3::scale(scale))?
            .checked_then(rotation)?
            .checked_then(Matrix3::translation(Vector2::new(
                self.anchor.x() + moved.x(),
                self.anchor.y() + moved.y(),
            )?))
    }
}

/// One explicit sampled virtual copy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeCopy {
    index: u32,
    exponent: f64,
    transform: Matrix3,
    opacity: f64,
}

impl ShapeCopy {
    /// Returns the zero-based copy index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the transform exponent, including the authored offset.
    #[must_use]
    pub const fn exponent(self) -> f64 {
        self.exponent
    }

    /// Returns the complete sampled affine transform.
    #[must_use]
    pub const fn transform(self) -> Matrix3 {
        self.transform
    }

    /// Returns normalized copy opacity.
    #[must_use]
    pub const fn opacity(self) -> f64 {
        self.opacity
    }
}

/// One bounded editable virtual-copy operation.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeRepeater {
    copies: AnimationCurve,
    offset: AnimationCurve,
    transform: RepeaterTransformAnimation,
    start_opacity: AnimationCurve,
    end_opacity: AnimationCurve,
    composite: RepeaterComposite,
    timebase: Timebase,
}

impl ShapeRepeater {
    /// Creates a checked repeater with held integer copy-count animation.
    pub fn new(
        copies: AnimationCurve,
        offset: AnimationCurve,
        transform: RepeaterTransformAnimation,
        start_opacity: AnimationCurve,
        end_opacity: AnimationCurve,
        composite: RepeaterComposite,
    ) -> Result<Self> {
        validate_copy_count_curve(&copies)?;
        require_curve_components(&offset, 1, "create_repeater")?;
        validate_unit_scalar_curve(
            &start_opacity,
            "create_repeater",
            "repeater_opacity_out_of_range",
        )?;
        validate_unit_scalar_curve(
            &end_opacity,
            "create_repeater",
            "repeater_opacity_out_of_range",
        )?;
        let timebase = copies.timebase();
        require_common_timebase(
            timebase,
            [
                offset.timebase(),
                transform.timebase(),
                start_opacity.timebase(),
                end_opacity.timebase(),
            ],
            "create_repeater",
        )?;
        Ok(Self {
            copies,
            offset,
            transform,
            start_opacity,
            end_opacity,
            composite,
            timebase,
        })
    }

    /// Returns held integer copy-count animation.
    #[must_use]
    pub const fn copies(&self) -> &AnimationCurve {
        &self.copies
    }

    /// Returns animated fractional transform offset.
    #[must_use]
    pub const fn offset(&self) -> &AnimationCurve {
        &self.offset
    }

    /// Returns animated per-exponent transform state.
    #[must_use]
    pub const fn transform(&self) -> &RepeaterTransformAnimation {
        &self.transform
    }

    /// Returns animated first-copy opacity.
    #[must_use]
    pub const fn start_opacity(&self) -> &AnimationCurve {
        &self.start_opacity
    }

    /// Returns animated last-copy opacity.
    #[must_use]
    pub const fn end_opacity(&self) -> &AnimationCurve {
        &self.end_opacity
    }

    /// Returns editable composite placement.
    #[must_use]
    pub const fn composite(&self) -> RepeaterComposite {
        self.composite
    }

    /// Returns the shared exact clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns an immutable copy with replacement held copy-count animation.
    pub fn with_copies(&self, copies: AnimationCurve) -> Result<Self> {
        Self::new(
            copies,
            self.offset.clone(),
            self.transform.clone(),
            self.start_opacity.clone(),
            self.end_opacity.clone(),
            self.composite,
        )
    }

    /// Returns an immutable copy with replacement offset animation.
    pub fn with_offset(&self, offset: AnimationCurve) -> Result<Self> {
        Self::new(
            self.copies.clone(),
            offset,
            self.transform.clone(),
            self.start_opacity.clone(),
            self.end_opacity.clone(),
            self.composite,
        )
    }

    /// Returns an immutable copy with replacement transform animation.
    pub fn with_transform(&self, transform: RepeaterTransformAnimation) -> Result<Self> {
        Self::new(
            self.copies.clone(),
            self.offset.clone(),
            transform,
            self.start_opacity.clone(),
            self.end_opacity.clone(),
            self.composite,
        )
    }

    /// Returns an immutable copy with replacement start and end opacity animation.
    pub fn with_opacity(
        &self,
        start_opacity: AnimationCurve,
        end_opacity: AnimationCurve,
    ) -> Result<Self> {
        Self::new(
            self.copies.clone(),
            self.offset.clone(),
            self.transform.clone(),
            start_opacity,
            end_opacity,
            self.composite,
        )
    }

    /// Returns an immutable copy with replacement composite placement.
    pub fn with_composite(&self, composite: RepeaterComposite) -> Result<Self> {
        Self::new(
            self.copies.clone(),
            self.offset.clone(),
            self.transform.clone(),
            self.start_opacity.clone(),
            self.end_opacity.clone(),
            composite,
        )
    }

    /// Samples and expands bounded virtual copies at one exact time.
    pub fn sample(&self, time: RationalTime) -> Result<ShapeRepeaterSample> {
        let copies = sample_scalar(&self.copies, time, "sample_repeater")?;
        if copies.fract() != 0.0 || !(0.0..=f64::from(MAX_REPEATER_COPIES)).contains(&copies) {
            return Err(shape_error(
                "sample_repeater",
                "invalid_repeater_copy_count",
                "sampled repeater copy count must be a bounded nonnegative integer",
            ));
        }
        let copy_count = copies as u32;
        let offset = sample_scalar(&self.offset, time, "sample_repeater")?;
        let transform = self.transform.sample(time)?;
        let start_opacity = sample_unit_scalar(
            &self.start_opacity,
            time,
            "sample_repeater",
            "repeater_opacity_out_of_range",
        )?;
        let end_opacity = sample_unit_scalar(
            &self.end_opacity,
            time,
            "sample_repeater",
            "repeater_opacity_out_of_range",
        )?;
        let mut expanded = Vec::with_capacity(copy_count as usize);
        for index in 0..copy_count {
            let exponent = offset + f64::from(index);
            if !exponent.is_finite() {
                return Err(shape_error(
                    "sample_repeater",
                    "nonfinite_repeater_exponent",
                    "repeater copy exponent must be finite",
                ));
            }
            let progress = if copy_count <= 1 {
                0.0
            } else {
                f64::from(index) / f64::from(copy_count - 1)
            };
            let opacity = start_opacity + (end_opacity - start_opacity) * progress;
            expanded.push(ShapeCopy {
                index,
                exponent,
                transform: transform.transform_at(exponent)?,
                opacity,
            });
        }
        Ok(ShapeRepeaterSample {
            copy_count,
            offset,
            transform,
            start_opacity,
            end_opacity,
            composite: self.composite,
            copies: expanded,
        })
    }

    /// Retimes every nested continuous property over one exact interval.
    pub fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            self.copies.retimed(new_start, new_end)?,
            self.offset.retimed(new_start, new_end)?,
            self.transform.retimed(new_start, new_end)?,
            self.start_opacity.retimed(new_start, new_end)?,
            self.end_opacity.retimed(new_start, new_end)?,
            self.composite,
        )
    }
}

/// One sampled repeater and its explicit virtual copies.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeRepeaterSample {
    copy_count: u32,
    offset: f64,
    transform: RepeaterTransformSample,
    start_opacity: f64,
    end_opacity: f64,
    composite: RepeaterComposite,
    copies: Vec<ShapeCopy>,
}

impl ShapeRepeaterSample {
    /// Returns the sampled bounded copy count.
    #[must_use]
    pub const fn copy_count(&self) -> u32 {
        self.copy_count
    }

    /// Returns the sampled fractional offset.
    #[must_use]
    pub const fn offset(&self) -> f64 {
        self.offset
    }

    /// Returns sampled per-exponent transform state.
    #[must_use]
    pub const fn transform(&self) -> RepeaterTransformSample {
        self.transform
    }

    /// Returns sampled first-copy opacity.
    #[must_use]
    pub const fn start_opacity(&self) -> f64 {
        self.start_opacity
    }

    /// Returns sampled last-copy opacity.
    #[must_use]
    pub const fn end_opacity(&self) -> f64 {
        self.end_opacity
    }

    /// Returns composite placement relative to the source shape.
    #[must_use]
    pub const fn composite(&self) -> RepeaterComposite {
        self.composite
    }

    /// Returns explicit virtual copies in stable index order.
    #[must_use]
    pub fn copies(&self) -> &[ShapeCopy] {
        &self.copies
    }
}

/// One directly editable vector shape and all of its visual operations.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorShape {
    path: PathAnimation,
    fill: Option<FillStyle>,
    stroke: Option<StrokeStyle>,
    repeater: Option<ShapeRepeater>,
    timebase: Timebase,
}

impl VectorShape {
    /// Creates one vector shape under the path's exact clock.
    pub fn new(
        path: PathAnimation,
        fill: Option<FillStyle>,
        stroke: Option<StrokeStyle>,
        repeater: Option<ShapeRepeater>,
    ) -> Result<Self> {
        let timebase = path.timebase();
        require_common_timebase(
            timebase,
            fill.iter()
                .map(FillStyle::timebase)
                .chain(stroke.iter().map(StrokeStyle::timebase))
                .chain(repeater.iter().map(ShapeRepeater::timebase)),
            "create_vector_shape",
        )?;
        Ok(Self {
            path,
            fill,
            stroke,
            repeater,
            timebase,
        })
    }

    /// Returns the directly editable cubic path.
    #[must_use]
    pub const fn path(&self) -> &PathAnimation {
        &self.path
    }

    /// Returns the editable fill operation when present.
    #[must_use]
    pub const fn fill(&self) -> Option<&FillStyle> {
        self.fill.as_ref()
    }

    /// Returns the editable stroke operation when present.
    #[must_use]
    pub const fn stroke(&self) -> Option<&StrokeStyle> {
        self.stroke.as_ref()
    }

    /// Returns the editable virtual-copy operation when present.
    #[must_use]
    pub const fn repeater(&self) -> Option<&ShapeRepeater> {
        self.repeater.as_ref()
    }

    /// Returns the shared exact clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns an immutable copy with a replacement path.
    pub fn with_path(&self, path: PathAnimation) -> Result<Self> {
        Self::new(
            path,
            self.fill.clone(),
            self.stroke.clone(),
            self.repeater.clone(),
        )
    }

    /// Returns an immutable copy with replacement optional fill state.
    pub fn with_fill(&self, fill: Option<FillStyle>) -> Result<Self> {
        Self::new(
            self.path.clone(),
            fill,
            self.stroke.clone(),
            self.repeater.clone(),
        )
    }

    /// Returns an immutable copy with replacement optional stroke state.
    pub fn with_stroke(&self, stroke: Option<StrokeStyle>) -> Result<Self> {
        Self::new(
            self.path.clone(),
            self.fill.clone(),
            stroke,
            self.repeater.clone(),
        )
    }

    /// Returns an immutable copy with replacement optional repeater state.
    pub fn with_repeater(&self, repeater: Option<ShapeRepeater>) -> Result<Self> {
        Self::new(
            self.path.clone(),
            self.fill.clone(),
            self.stroke.clone(),
            repeater,
        )
    }

    /// Samples every result-affecting property at one exact time.
    pub fn sample(&self, time: RationalTime) -> Result<VectorShapeSample> {
        Ok(VectorShapeSample {
            path: self.path.sample(time)?,
            fill: self
                .fill
                .as_ref()
                .map(|fill| fill.sample(time))
                .transpose()?,
            stroke: self
                .stroke
                .as_ref()
                .map(|stroke| stroke.sample(time))
                .transpose()?,
            repeater: self
                .repeater
                .as_ref()
                .map(|repeater| repeater.sample(time))
                .transpose()?,
        })
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            self.path.retimed(new_start, new_end)?,
            self.fill
                .as_ref()
                .map(|fill| fill.retimed(new_start, new_end))
                .transpose()?,
            self.stroke
                .as_ref()
                .map(|stroke| stroke.retimed(new_start, new_end))
                .transpose()?,
            self.repeater
                .as_ref()
                .map(|repeater| repeater.retimed(new_start, new_end))
                .transpose()?,
        )
    }
}

/// One renderer-ready sampled vector shape.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorShapeSample {
    path: PathSample,
    fill: Option<FillStyleSample>,
    stroke: Option<StrokeStyleSample>,
    repeater: Option<ShapeRepeaterSample>,
}

impl VectorShapeSample {
    /// Returns sampled cubic geometry.
    #[must_use]
    pub const fn path(&self) -> &PathSample {
        &self.path
    }

    /// Returns the sampled fill operation when present.
    #[must_use]
    pub const fn fill(&self) -> Option<&FillStyleSample> {
        self.fill.as_ref()
    }

    /// Returns the sampled stroke operation when present.
    #[must_use]
    pub const fn stroke(&self) -> Option<&StrokeStyleSample> {
        self.stroke.as_ref()
    }

    /// Returns sampled virtual copies when present.
    #[must_use]
    pub const fn repeater(&self) -> Option<&ShapeRepeaterSample> {
        self.repeater.as_ref()
    }
}

/// One reusable strictly persisted vector-shape authoring document.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorShapeDocument {
    timebase: Timebase,
    shapes: Vec<VectorShape>,
}

impl VectorShapeDocument {
    /// Creates one document with a caller-owned exact clock.
    pub fn new(timebase: Timebase, shapes: impl IntoIterator<Item = VectorShape>) -> Result<Self> {
        let shapes = shapes.into_iter().collect::<Vec<_>>();
        if shapes.len() > MAX_VECTOR_SHAPES {
            return Err(shape_error(
                "create_shape_document",
                "vector_shape_limit",
                "vector-shape document exceeds the supported shape count",
            ));
        }
        require_common_timebase(
            timebase,
            shapes.iter().map(VectorShape::timebase),
            "create_shape_document",
        )?;
        Ok(Self { timebase, shapes })
    }

    /// Returns the exact project clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns shapes in direct authoring and composite order.
    #[must_use]
    pub fn shapes(&self) -> &[VectorShape] {
        &self.shapes
    }

    /// Inserts one vector shape without changing this document.
    pub fn with_shape_inserted(&self, index: usize, shape: VectorShape) -> Result<Self> {
        if index > self.shapes.len() {
            return Err(shape_index_error(
                "insert_vector_shape",
                index,
                self.shapes.len(),
            ));
        }
        let mut shapes = self.shapes.clone();
        shapes.insert(index, shape);
        Self::new(self.timebase, shapes)
    }

    /// Replaces one vector shape without changing this document.
    pub fn with_shape_replaced(&self, index: usize, shape: VectorShape) -> Result<Self> {
        if index >= self.shapes.len() {
            return Err(shape_index_error(
                "replace_vector_shape",
                index,
                self.shapes.len(),
            ));
        }
        let mut shapes = self.shapes.clone();
        shapes[index] = shape;
        Self::new(self.timebase, shapes)
    }

    /// Removes one vector shape without changing this document.
    pub fn with_shape_removed(&self, index: usize) -> Result<Self> {
        if index >= self.shapes.len() {
            return Err(shape_index_error(
                "remove_vector_shape",
                index,
                self.shapes.len(),
            ));
        }
        let mut shapes = self.shapes.clone();
        shapes.remove(index);
        Self::new(self.timebase, shapes)
    }

    /// Samples every shape and visual operation at one exact project time.
    pub fn sample(&self, time: RationalTime) -> Result<VectorShapeDocumentSample> {
        Ok(VectorShapeDocumentSample {
            time,
            shapes: self
                .shapes
                .iter()
                .map(|shape| shape.sample(time))
                .collect::<Result<Vec<_>>>()?,
        })
    }

    /// Retimes every nested curve over one exact interval.
    pub fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        if new_start.timebase() != self.timebase || new_end.timebase() != self.timebase {
            return Err(shape_error(
                "retime_shape_document",
                "timebase_mismatch",
                "vector-shape retime bounds must use the document timebase",
            ));
        }
        if new_end <= new_start {
            return Err(shape_error(
                "retime_shape_document",
                "invalid_retime_range",
                "vector-shape retime end must occur after its start",
            ));
        }
        Self::new(
            self.timebase,
            self.shapes
                .iter()
                .map(|shape| shape.retimed(new_start, new_end))
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

/// One sampled vector-shape document at an exact project time.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorShapeDocumentSample {
    time: RationalTime,
    shapes: Vec<VectorShapeSample>,
}

impl VectorShapeDocumentSample {
    /// Returns the exact requested project time.
    #[must_use]
    pub const fn time(&self) -> RationalTime {
        self.time
    }

    /// Returns sampled shapes in stable composite order.
    #[must_use]
    pub fn shapes(&self) -> &[VectorShapeSample] {
        &self.shapes
    }
}

fn shape_index_error(operation: &'static str, index: usize, len: usize) -> Error {
    shape_error(
        operation,
        "vector_shape_index_out_of_bounds",
        "vector-shape edit index is out of bounds",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("index", index.to_string())
            .with_field("shape_count", len.to_string()),
    )
}

fn gradient_stop_index_error(operation: &'static str, index: usize, len: usize) -> Error {
    shape_error(
        operation,
        "gradient_stop_index_out_of_bounds",
        "gradient-stop edit index is out of bounds",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("index", index.to_string())
            .with_field("stop_count", len.to_string()),
    )
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorShapeDocumentWire {
    schema_revision: u32,
    timebase: Timebase,
    shapes: Vec<VectorShapeWire>,
}

impl VectorShapeDocumentWire {
    fn from_document(document: &VectorShapeDocument) -> Self {
        Self {
            schema_revision: VECTOR_SHAPE_DOCUMENT_SCHEMA_REVISION,
            timebase: document.timebase,
            shapes: document
                .shapes
                .iter()
                .map(VectorShapeWire::from_shape)
                .collect(),
        }
    }

    fn into_document(self) -> Result<VectorShapeDocument> {
        if self.schema_revision != VECTOR_SHAPE_DOCUMENT_SCHEMA_REVISION {
            return Err(Error::new(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "vector-shape document schema revision is unsupported",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "deserialize_shape_document")
                    .with_field("document_revision", self.schema_revision.to_string())
                    .with_field(
                        "supported_revision",
                        VECTOR_SHAPE_DOCUMENT_SCHEMA_REVISION.to_string(),
                    ),
            ));
        }
        VectorShapeDocument::new(
            self.timebase,
            self.shapes
                .into_iter()
                .map(VectorShapeWire::into_shape)
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

impl Serialize for VectorShapeDocument {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        VectorShapeDocumentWire::from_document(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VectorShapeDocument {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        VectorShapeDocumentWire::deserialize(deserializer)?
            .into_document()
            .map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorShapeWire {
    path: PathAnimationWire,
    fill: Option<FillStyleWire>,
    stroke: Option<StrokeStyleWire>,
    repeater: Option<ShapeRepeaterWire>,
}

impl VectorShapeWire {
    fn from_shape(shape: &VectorShape) -> Self {
        Self {
            path: PathAnimationWire::from_path(&shape.path),
            fill: shape.fill.as_ref().map(FillStyleWire::from_fill),
            stroke: shape.stroke.as_ref().map(StrokeStyleWire::from_stroke),
            repeater: shape
                .repeater
                .as_ref()
                .map(ShapeRepeaterWire::from_repeater),
        }
    }

    fn into_shape(self) -> Result<VectorShape> {
        VectorShape::new(
            self.path.into_path()?,
            self.fill.map(FillStyleWire::into_fill).transpose()?,
            self.stroke.map(StrokeStyleWire::into_stroke).transpose()?,
            self.repeater
                .map(ShapeRepeaterWire::into_repeater)
                .transpose()?,
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathAnimationWire {
    closed: bool,
    vertices: Vec<AnimationCurve>,
}

impl PathAnimationWire {
    fn from_path(path: &PathAnimation) -> Self {
        Self {
            closed: path.closed,
            vertices: path
                .vertices
                .iter()
                .map(|vertex| vertex.curve.clone())
                .collect(),
        }
    }

    fn into_path(self) -> Result<PathAnimation> {
        PathAnimation::new(
            self.closed,
            self.vertices
                .into_iter()
                .map(PathVertexAnimation::new)
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FillRuleWire {
    NonZero,
    EvenOdd,
}

impl From<FillRule> for FillRuleWire {
    fn from(value: FillRule) -> Self {
        match value {
            FillRule::NonZero => Self::NonZero,
            FillRule::EvenOdd => Self::EvenOdd,
        }
    }
}

impl FillRuleWire {
    const fn into_rule(self) -> FillRule {
        match self {
            Self::NonZero => FillRule::NonZero,
            Self::EvenOdd => FillRule::EvenOdd,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FillStyleWire {
    rule: FillRuleWire,
    paint: PaintWire,
    opacity: AnimationCurve,
}

impl FillStyleWire {
    fn from_fill(fill: &FillStyle) -> Self {
        Self {
            rule: FillRuleWire::from(fill.rule),
            paint: PaintWire::from_paint(&fill.paint),
            opacity: fill.opacity.clone(),
        }
    }

    fn into_fill(self) -> Result<FillStyle> {
        FillStyle::new(
            self.rule.into_rule(),
            self.paint.into_paint()?,
            self.opacity,
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PaintWire {
    Solid { color: AnimationCurve },
    Gradient { gradient: GradientPaintWire },
}

impl PaintWire {
    fn from_paint(paint: &Paint) -> Self {
        match paint {
            Paint::Solid(color) => Self::Solid {
                color: color.curve.clone(),
            },
            Paint::Gradient(gradient) => Self::Gradient {
                gradient: GradientPaintWire::from_gradient(gradient),
            },
        }
    }

    fn into_paint(self) -> Result<Paint> {
        match self {
            Self::Solid { color } => Ok(Paint::Solid(ShapeColorAnimation::new(color)?)),
            Self::Gradient { gradient } => Ok(Paint::Gradient(gradient.into_gradient()?)),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GradientSpreadWire {
    Pad,
    Repeat,
    Reflect,
}

impl From<GradientSpread> for GradientSpreadWire {
    fn from(value: GradientSpread) -> Self {
        match value {
            GradientSpread::Pad => Self::Pad,
            GradientSpread::Repeat => Self::Repeat,
            GradientSpread::Reflect => Self::Reflect,
        }
    }
}

impl GradientSpreadWire {
    const fn into_spread(self) -> GradientSpread {
        match self {
            Self::Pad => GradientSpread::Pad,
            Self::Repeat => GradientSpread::Repeat,
            Self::Reflect => GradientSpread::Reflect,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GradientPaintWire {
    geometry: GradientGeometryWire,
    stops: Vec<GradientStopWire>,
    spread: GradientSpreadWire,
}

impl GradientPaintWire {
    fn from_gradient(gradient: &GradientPaint) -> Self {
        Self {
            geometry: GradientGeometryWire::from_geometry(&gradient.geometry),
            stops: gradient
                .stops
                .iter()
                .map(GradientStopWire::from_stop)
                .collect(),
            spread: GradientSpreadWire::from(gradient.spread),
        }
    }

    fn into_gradient(self) -> Result<GradientPaint> {
        GradientPaint::new(
            self.geometry.into_geometry()?,
            self.stops
                .into_iter()
                .map(GradientStopWire::into_stop)
                .collect::<Result<Vec<_>>>()?,
            self.spread.into_spread(),
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum GradientGeometryWire {
    Linear {
        start: AnimationCurve,
        end: AnimationCurve,
    },
    Radial {
        center: AnimationCurve,
        radius: AnimationCurve,
    },
}

impl GradientGeometryWire {
    fn from_geometry(geometry: &GradientGeometry) -> Self {
        match &geometry.kind {
            GradientGeometryKind::Linear { start, end } => Self::Linear {
                start: start.clone(),
                end: end.clone(),
            },
            GradientGeometryKind::Radial { center, radius } => Self::Radial {
                center: center.clone(),
                radius: radius.clone(),
            },
        }
    }

    fn into_geometry(self) -> Result<GradientGeometry> {
        match self {
            Self::Linear { start, end } => GradientGeometry::linear(start, end),
            Self::Radial { center, radius } => GradientGeometry::radial(center, radius),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GradientStopWire {
    offset: AnimationCurve,
    color: AnimationCurve,
}

impl GradientStopWire {
    fn from_stop(stop: &GradientStopAnimation) -> Self {
        Self {
            offset: stop.offset.clone(),
            color: stop.color.curve.clone(),
        }
    }

    fn into_stop(self) -> Result<GradientStopAnimation> {
        GradientStopAnimation::new(self.offset, ShapeColorAnimation::new(self.color)?)
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LineCapWire {
    Butt,
    Round,
    Square,
}

impl From<LineCap> for LineCapWire {
    fn from(value: LineCap) -> Self {
        match value {
            LineCap::Butt => Self::Butt,
            LineCap::Round => Self::Round,
            LineCap::Square => Self::Square,
        }
    }
}

impl LineCapWire {
    const fn into_cap(self) -> LineCap {
        match self {
            Self::Butt => LineCap::Butt,
            Self::Round => LineCap::Round,
            Self::Square => LineCap::Square,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LineJoinWire {
    Miter,
    Round,
    Bevel,
}

impl From<LineJoin> for LineJoinWire {
    fn from(value: LineJoin) -> Self {
        match value {
            LineJoin::Miter => Self::Miter,
            LineJoin::Round => Self::Round,
            LineJoin::Bevel => Self::Bevel,
        }
    }
}

impl LineJoinWire {
    const fn into_join(self) -> LineJoin {
        match self {
            Self::Miter => LineJoin::Miter,
            Self::Round => LineJoin::Round,
            Self::Bevel => LineJoin::Bevel,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrokeStyleWire {
    paint: PaintWire,
    opacity: AnimationCurve,
    width: AnimationCurve,
    cap: LineCapWire,
    join: LineJoinWire,
    miter_limit: AnimationCurve,
    dash_pattern: Vec<AnimationCurve>,
    dash_offset: AnimationCurve,
}

impl StrokeStyleWire {
    fn from_stroke(stroke: &StrokeStyle) -> Self {
        Self {
            paint: PaintWire::from_paint(&stroke.paint),
            opacity: stroke.opacity.clone(),
            width: stroke.width.clone(),
            cap: LineCapWire::from(stroke.cap),
            join: LineJoinWire::from(stroke.join),
            miter_limit: stroke.miter_limit.clone(),
            dash_pattern: stroke.dash_pattern.clone(),
            dash_offset: stroke.dash_offset.clone(),
        }
    }

    fn into_stroke(self) -> Result<StrokeStyle> {
        StrokeStyle::new(
            self.paint.into_paint()?,
            self.opacity,
            self.width,
            self.cap.into_cap(),
            self.join.into_join(),
            self.miter_limit,
            self.dash_pattern,
            self.dash_offset,
        )
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RepeaterCompositeWire {
    Above,
    Below,
}

impl From<RepeaterComposite> for RepeaterCompositeWire {
    fn from(value: RepeaterComposite) -> Self {
        match value {
            RepeaterComposite::Above => Self::Above,
            RepeaterComposite::Below => Self::Below,
        }
    }
}

impl RepeaterCompositeWire {
    const fn into_composite(self) -> RepeaterComposite {
        match self {
            Self::Above => RepeaterComposite::Above,
            Self::Below => RepeaterComposite::Below,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepeaterTransformWire {
    anchor: AnimationCurve,
    position: AnimationCurve,
    scale: AnimationCurve,
    rotation_degrees: AnimationCurve,
}

impl RepeaterTransformWire {
    fn from_transform(transform: &RepeaterTransformAnimation) -> Self {
        Self {
            anchor: transform.anchor.clone(),
            position: transform.position.clone(),
            scale: transform.scale.clone(),
            rotation_degrees: transform.rotation_degrees.clone(),
        }
    }

    fn into_transform(self) -> Result<RepeaterTransformAnimation> {
        RepeaterTransformAnimation::new(
            self.anchor,
            self.position,
            self.scale,
            self.rotation_degrees,
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShapeRepeaterWire {
    copies: AnimationCurve,
    offset: AnimationCurve,
    transform: RepeaterTransformWire,
    start_opacity: AnimationCurve,
    end_opacity: AnimationCurve,
    composite: RepeaterCompositeWire,
}

impl ShapeRepeaterWire {
    fn from_repeater(repeater: &ShapeRepeater) -> Self {
        Self {
            copies: repeater.copies.clone(),
            offset: repeater.offset.clone(),
            transform: RepeaterTransformWire::from_transform(&repeater.transform),
            start_opacity: repeater.start_opacity.clone(),
            end_opacity: repeater.end_opacity.clone(),
            composite: RepeaterCompositeWire::from(repeater.composite),
        }
    }

    fn into_repeater(self) -> Result<ShapeRepeater> {
        ShapeRepeater::new(
            self.copies,
            self.offset,
            self.transform.into_transform()?,
            self.start_opacity,
            self.end_opacity,
            self.composite.into_composite(),
        )
    }
}

fn validate_copy_count_curve(curve: &AnimationCurve) -> Result<()> {
    require_curve_components(curve, 1, "create_repeater")?;
    for (index, keyframe) in curve.keyframes().iter().enumerate() {
        let value = keyframe.value().components()[0];
        if value.fract() != 0.0 || !(0.0..=f64::from(MAX_REPEATER_COPIES)).contains(&value) {
            return Err(shape_error(
                "create_repeater",
                "invalid_repeater_copy_count",
                "repeater copy count keys must be bounded nonnegative integers",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_repeater")
                    .with_field("keyframe_index", index.to_string()),
            ));
        }
        if index + 1 < curve.keyframes().len()
            && keyframe.outgoing_interpolation() != Interpolation::Hold
        {
            return Err(shape_error(
                "create_repeater",
                "nonheld_repeater_copy_count",
                "repeater copy count animation must use held interpolation",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_repeater")
                    .with_field("keyframe_index", index.to_string()),
            ));
        }
    }
    Ok(())
}

fn sample_gradient_stops(stops: &[GradientStopSample], parameter: f64) -> Result<ShapeColor> {
    let upper = stops.partition_point(|stop| stop.offset <= parameter);
    if upper == 0 {
        return Ok(stops[0].color);
    }
    if upper == stops.len() || stops[upper - 1].offset == parameter {
        return Ok(stops[upper - 1].color);
    }
    let left = stops[upper - 1];
    let right = stops[upper];
    let progress = (parameter - left.offset) / (right.offset - left.offset);
    left.color.interpolated(right.color, progress)
}

fn sample_point(
    curve: &AnimationCurve,
    time: RationalTime,
    operation: &'static str,
) -> Result<Point2> {
    let value = curve.evaluate(time)?;
    let components = value.components();
    if components.len() != 2 {
        return Err(shape_error(
            operation,
            "curve_component_count_mismatch",
            "sampled shape point must contain two components",
        ));
    }
    Point2::new(components[0], components[1])
}

fn sample_vector(
    curve: &AnimationCurve,
    time: RationalTime,
    operation: &'static str,
) -> Result<Vector2> {
    let value = curve.evaluate(time)?;
    let components = value.components();
    if components.len() != 2 {
        return Err(shape_error(
            operation,
            "curve_component_count_mismatch",
            "sampled shape vector must contain two components",
        ));
    }
    Vector2::new(components[0], components[1])
}

fn sample_scalar(
    curve: &AnimationCurve,
    time: RationalTime,
    operation: &'static str,
) -> Result<f64> {
    let value = curve.evaluate(time)?;
    if value.component_count() != 1 {
        return Err(shape_error(
            operation,
            "curve_component_count_mismatch",
            "sampled shape scalar must contain one component",
        ));
    }
    Ok(value.components()[0])
}

fn sample_unit_scalar(
    curve: &AnimationCurve,
    time: RationalTime,
    operation: &'static str,
    reason: &'static str,
) -> Result<f64> {
    let value = sample_scalar(curve, time, operation)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(shape_error(
            operation,
            reason,
            "sampled normalized shape value must be between zero and one",
        ));
    }
    Ok(value)
}

fn validate_unit_scalar_curve(
    curve: &AnimationCurve,
    operation: &'static str,
    reason: &'static str,
) -> Result<()> {
    require_curve_components(curve, 1, operation)?;
    require_all_keyframe_components(
        curve,
        |value| (0.0..=1.0).contains(&value),
        operation,
        reason,
        "normalized shape values must be between zero and one",
    )
}

fn validate_nonnegative_scalar_curve(
    curve: &AnimationCurve,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Result<()> {
    validate_scalar_curve_minimum(curve, 0.0, operation, reason, message)
}

fn validate_scalar_curve_minimum(
    curve: &AnimationCurve,
    minimum: f64,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Result<()> {
    require_curve_components(curve, 1, operation)?;
    require_all_keyframe_components(curve, |value| value >= minimum, operation, reason, message)
}

fn require_keyframe_component_range(
    curve: &AnimationCurve,
    component: usize,
    minimum: f64,
    maximum: f64,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Result<()> {
    for (keyframe_index, keyframe) in curve.keyframes().iter().enumerate() {
        let value = keyframe.value().components()[component];
        if !(minimum..=maximum).contains(&value) {
            return Err(shape_error(operation, reason, message).with_context(
                ErrorContext::new(COMPONENT, operation)
                    .with_field("keyframe_index", keyframe_index.to_string())
                    .with_field("component", component.to_string()),
            ));
        }
    }
    Ok(())
}

fn require_all_keyframe_components(
    curve: &AnimationCurve,
    valid: impl Fn(f64) -> bool,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Result<()> {
    for (keyframe_index, keyframe) in curve.keyframes().iter().enumerate() {
        for (component_index, component) in
            keyframe.value().components().iter().copied().enumerate()
        {
            if !valid(component) {
                return Err(shape_error(operation, reason, message).with_context(
                    ErrorContext::new(COMPONENT, operation)
                        .with_field("keyframe_index", keyframe_index.to_string())
                        .with_field("component", component_index.to_string()),
                ));
            }
        }
    }
    Ok(())
}

fn require_curve_components(
    curve: &AnimationCurve,
    expected: usize,
    operation: &'static str,
) -> Result<()> {
    let actual = curve.keyframes()[0].value().component_count();
    if actual == expected {
        return Ok(());
    }
    Err(shape_error(
        operation,
        "curve_component_count_mismatch",
        "shape animation curve has an unexpected component count",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("expected", expected.to_string())
            .with_field("actual", actual.to_string()),
    ))
}

fn require_common_timebase(
    expected: Timebase,
    timebases: impl IntoIterator<Item = Timebase>,
    operation: &'static str,
) -> Result<()> {
    if timebases.into_iter().all(|timebase| timebase == expected) {
        return Ok(());
    }
    Err(shape_error(
        operation,
        "timebase_mismatch",
        "shape animation curves must use one exact timebase",
    ))
}

fn index_error(operation: &'static str, index: usize, len: usize) -> Error {
    shape_error(
        operation,
        "path_vertex_index_out_of_bounds",
        "path vertex edit index is out of bounds",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("index", index.to_string())
            .with_field("vertex_count", len.to_string()),
    )
}

fn shape_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
