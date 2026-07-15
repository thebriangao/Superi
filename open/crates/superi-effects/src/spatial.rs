//! Editable spatial layer state and bounded reference sampling.
//!
//! Spatial state composes the existing visual composition artifact instead of duplicating layer,
//! parenting, precomposition, stack-order, or exact-time-remap ownership. Each resolved layer path
//! contributes directly editable transform state. One checked scene record per composition retains
//! cameras, lights, depth ordering, and shutter sampling. The sampled values are renderer-ready
//! binary64 state for headless proof and later GPU parity, not a production playback fallback.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;

use serde::de::{Error as _, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::{Matrix3, PixelBounds};
use superi_core::settings::{CapabilityId, CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, TimeRounding, Timebase};
use superi_graph::mutate::TypedParameterValue;
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, NodeTypeId,
    ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};
use superi_graph::value::GraphValue;
use superi_image::limits::ImageLimits;
use superi_image::value::Image;

use crate::authoring::{
    EffectMetadata, EffectNodeDefinition, EffectParameterDefinition, EffectPortDefinition,
    ParameterControl,
};
use crate::composition::{
    CompositionArtifact, CompositionId, CompositionLayerId, ResolvedLayerKind, MAX_COMPOSITIONS,
};
use crate::keyframe::AnimationCurve;
use crate::reference::{
    evaluate_reference, ReferenceCompositeOperator, ReferenceEffectState, ReferenceSampling,
};

const COMPONENT: &str = "superi-effects.spatial";
const MATRIX_EPSILON: f64 = 1.0e-12;

/// Current standalone spatial-composition wire revision.
pub const SPATIAL_COMPOSITION_SCHEMA_REVISION: u32 = 1;
/// Maximum lights retained by one spatial scene.
pub const MAX_SPATIAL_LIGHTS: usize = 64;
/// Maximum exact shutter samples evaluated by the bounded reference path.
pub const MAX_MOTION_SAMPLES: u16 = 32;
/// Maximum visible layers rendered in one shutter sample by the CPU oracle.
pub const MAX_REFERENCE_SPATIAL_LAYERS: usize = 4_096;
/// Maximum layer evaluations across one complete motion-blurred CPU request.
pub const MAX_REFERENCE_SPATIAL_EVALUATIONS: usize = 65_536;

type Matrix4 = [[f64; 4]; 4];

/// Stable caller-owned identity for one visual source image.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SpatialSourceId(u64);

impl SpatialSourceId {
    /// Creates one persisted source identity.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the persisted integer identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for SpatialSourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Dimensional interpretation of one planar visual layer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerSpace {
    /// X/Y anchor, X/Y scale, Z rotation, and X/Y position with explicit Z depth.
    TwoDimensional,
    /// Complete X/Y/Z anchor, scale, Euler rotation, and position.
    ThreeDimensional,
}

/// Directly editable animated transform state for one planar layer.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LayerTransform {
    anchor: AnimationCurve,
    position: AnimationCurve,
    rotation_degrees: AnimationCurve,
    scale: AnimationCurve,
    opacity: AnimationCurve,
}

impl LayerTransform {
    /// Creates one checked transform from five independently editable curves.
    pub fn new(
        anchor: AnimationCurve,
        position: AnimationCurve,
        rotation_degrees: AnimationCurve,
        scale: AnimationCurve,
        opacity: AnimationCurve,
    ) -> Result<Self> {
        let transform = Self {
            anchor,
            position,
            rotation_degrees,
            scale,
            opacity,
        };
        transform.validate(None)?;
        Ok(transform)
    }

    /// Returns the exact animation clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.anchor.timebase()
    }

    /// Returns the animated local anchor curve.
    #[must_use]
    pub const fn anchor(&self) -> &AnimationCurve {
        &self.anchor
    }

    /// Returns the animated position curve.
    #[must_use]
    pub const fn position(&self) -> &AnimationCurve {
        &self.position
    }

    /// Returns the animated XYZ Euler rotation curve in degrees.
    #[must_use]
    pub const fn rotation_degrees(&self) -> &AnimationCurve {
        &self.rotation_degrees
    }

    /// Returns the animated scale curve where one is identity.
    #[must_use]
    pub const fn scale(&self) -> &AnimationCurve {
        &self.scale
    }

    /// Returns the animated normalized opacity curve.
    #[must_use]
    pub const fn opacity(&self) -> &AnimationCurve {
        &self.opacity
    }

    fn validate(&self, expected_timebase: Option<Timebase>) -> Result<()> {
        let timebase = expected_timebase.unwrap_or(self.anchor.timebase());
        validate_curve(&self.anchor, 3, timebase, "anchor")?;
        validate_curve(&self.position, 3, timebase, "position")?;
        validate_curve(&self.rotation_degrees, 3, timebase, "rotation_degrees")?;
        validate_curve(&self.scale, 3, timebase, "scale")?;
        validate_curve(&self.opacity, 1, timebase, "opacity")?;
        validate_authored_values(&self.scale, "scale", |value| value.abs() > MATRIX_EPSILON)?;
        validate_authored_values(&self.opacity, "opacity", |value| {
            (0.0..=1.0).contains(&value)
        })
    }

    fn sample(&self, time: RationalTime, space: LayerSpace) -> Result<SampledTransform> {
        let mut anchor = sample_curve::<3>(&self.anchor, time, "anchor")?;
        let position = sample_curve::<3>(&self.position, time, "position")?;
        let mut rotation = sample_curve::<3>(&self.rotation_degrees, time, "rotation_degrees")?;
        let mut scale = sample_curve::<3>(&self.scale, time, "scale")?;
        let opacity = sample_curve::<1>(&self.opacity, time, "opacity")?[0];
        if scale.iter().any(|value| value.abs() <= MATRIX_EPSILON) {
            return Err(invalid_error(
                "sample_layer_transform",
                "singular_layer_scale",
                "sampled layer scale must remain nonzero on every active axis",
            ));
        }
        if !(0.0..=1.0).contains(&opacity) {
            return Err(invalid_error(
                "sample_layer_transform",
                "opacity_out_of_range",
                "sampled layer opacity must remain between zero and one",
            ));
        }
        if space == LayerSpace::TwoDimensional {
            anchor[2] = 0.0;
            rotation[0] = 0.0;
            rotation[1] = 0.0;
            scale[2] = 1.0;
        }
        let matrix = compose_transform(anchor, position, rotation, scale);
        Ok(SampledTransform {
            anchor,
            position,
            rotation_degrees: rotation,
            scale,
            opacity,
            matrix,
        })
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            retime_curve(&self.anchor, new_start, new_end)?,
            retime_curve(&self.position, new_start, new_end)?,
            retime_curve(&self.rotation_degrees, new_start, new_end)?,
            retime_curve(&self.scale, new_start, new_end)?,
            retime_curve(&self.opacity, new_start, new_end)?,
        )
    }
}

/// Complete spatial payload retained by one existing composition layer.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialLayer {
    source_id: SpatialSourceId,
    space: LayerSpace,
    transform: LayerTransform,
    sampling: ReferenceSampling,
}

impl SpatialLayer {
    /// Creates one checked layer payload.
    pub fn new(
        source_id: SpatialSourceId,
        space: LayerSpace,
        transform: LayerTransform,
        sampling: ReferenceSampling,
    ) -> Result<Self> {
        transform.validate(None)?;
        Ok(Self {
            source_id,
            space,
            transform,
            sampling,
        })
    }

    /// Returns the caller-owned source identity.
    #[must_use]
    pub const fn source_id(&self) -> SpatialSourceId {
        self.source_id
    }

    /// Returns the dimensional interpretation.
    #[must_use]
    pub const fn space(&self) -> LayerSpace {
        self.space
    }

    /// Returns the complete editable transform.
    #[must_use]
    pub const fn transform(&self) -> &LayerTransform {
        &self.transform
    }

    /// Returns the reconstruction mode.
    #[must_use]
    pub const fn sampling(&self) -> ReferenceSampling {
        self.sampling
    }

    /// Returns a copy with a replacement transform.
    pub fn with_transform(&self, transform: LayerTransform) -> Result<Self> {
        Self::new(self.source_id, self.space, transform, self.sampling)
    }

    /// Returns a copy with another source identity.
    pub fn with_source(&self, source_id: SpatialSourceId) -> Result<Self> {
        Self::new(source_id, self.space, self.transform.clone(), self.sampling)
    }

    fn validate(&self, timebase: Timebase) -> Result<()> {
        self.transform.validate(Some(timebase))
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            self.source_id,
            self.space,
            self.transform.retimed(new_start, new_end)?,
            self.sampling,
        )
    }
}

/// Editable camera projection.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CameraProjection {
    /// Perspective projection with animated vertical field of view in degrees.
    Perspective {
        /// Animated field of view strictly between zero and 179 degrees.
        vertical_field_of_view_degrees: AnimationCurve,
    },
    /// Orthographic projection with animated vertical world extent.
    Orthographic {
        /// Animated positive vertical world extent.
        vertical_size: AnimationCurve,
    },
}

impl CameraProjection {
    /// Creates a checked orthographic projection.
    pub fn orthographic(vertical_size: AnimationCurve) -> Result<Self> {
        let projection = Self::Orthographic { vertical_size };
        projection.validate(projection.timebase())?;
        Ok(projection)
    }

    /// Creates a checked perspective projection.
    pub fn perspective(vertical_field_of_view_degrees: AnimationCurve) -> Result<Self> {
        let projection = Self::Perspective {
            vertical_field_of_view_degrees,
        };
        projection.validate(projection.timebase())?;
        Ok(projection)
    }

    /// Returns the exact animation clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        match self {
            Self::Perspective {
                vertical_field_of_view_degrees,
            } => vertical_field_of_view_degrees.timebase(),
            Self::Orthographic { vertical_size } => vertical_size.timebase(),
        }
    }

    fn validate(&self, timebase: Timebase) -> Result<()> {
        match self {
            Self::Perspective {
                vertical_field_of_view_degrees,
            } => {
                validate_curve(
                    vertical_field_of_view_degrees,
                    1,
                    timebase,
                    "vertical_field_of_view_degrees",
                )?;
                validate_authored_values(
                    vertical_field_of_view_degrees,
                    "vertical_field_of_view_degrees",
                    |value| value > 0.0 && value < 179.0,
                )
            }
            Self::Orthographic { vertical_size } => {
                validate_curve(vertical_size, 1, timebase, "vertical_size")?;
                validate_authored_values(vertical_size, "vertical_size", |value| value > 0.0)
            }
        }
    }

    fn sample(&self, time: RationalTime) -> Result<SampledProjection> {
        match self {
            Self::Perspective {
                vertical_field_of_view_degrees,
            } => {
                let value = sample_curve::<1>(
                    vertical_field_of_view_degrees,
                    time,
                    "vertical_field_of_view_degrees",
                )?[0];
                if value <= 0.0 || value >= 179.0 {
                    return Err(invalid_error(
                        "sample_camera",
                        "field_of_view_out_of_range",
                        "sampled camera field of view must remain between zero and 179 degrees",
                    ));
                }
                Ok(SampledProjection::Perspective {
                    vertical_field_of_view_degrees: value,
                })
            }
            Self::Orthographic { vertical_size } => {
                let value = sample_curve::<1>(vertical_size, time, "vertical_size")?[0];
                if value <= 0.0 {
                    return Err(invalid_error(
                        "sample_camera",
                        "orthographic_size_out_of_range",
                        "sampled orthographic height must remain positive",
                    ));
                }
                Ok(SampledProjection::Orthographic {
                    vertical_size: value,
                })
            }
        }
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        match self {
            Self::Perspective {
                vertical_field_of_view_degrees,
            } => Self::perspective(retime_curve(
                vertical_field_of_view_degrees,
                new_start,
                new_end,
            )?),
            Self::Orthographic { vertical_size } => {
                Self::orthographic(retime_curve(vertical_size, new_start, new_end)?)
            }
        }
    }
}

/// Directly editable animated camera.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialCamera {
    position: AnimationCurve,
    rotation_degrees: AnimationCurve,
    projection: CameraProjection,
    clipping: AnimationCurve,
}

impl SpatialCamera {
    /// Creates one checked right-handed camera looking down local negative Z.
    pub fn new(
        position: AnimationCurve,
        rotation_degrees: AnimationCurve,
        projection: CameraProjection,
        clipping: AnimationCurve,
    ) -> Result<Self> {
        let camera = Self {
            position,
            rotation_degrees,
            projection,
            clipping,
        };
        camera.validate(camera.position.timebase())?;
        Ok(camera)
    }

    /// Returns the exact animation clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.position.timebase()
    }

    /// Returns the animated position curve.
    #[must_use]
    pub const fn position(&self) -> &AnimationCurve {
        &self.position
    }

    /// Returns the animated Euler rotation curve in degrees.
    #[must_use]
    pub const fn rotation_degrees(&self) -> &AnimationCurve {
        &self.rotation_degrees
    }

    /// Returns the editable projection.
    #[must_use]
    pub const fn projection(&self) -> &CameraProjection {
        &self.projection
    }

    /// Returns the animated near and far clipping curve.
    #[must_use]
    pub const fn clipping(&self) -> &AnimationCurve {
        &self.clipping
    }

    fn validate(&self, timebase: Timebase) -> Result<()> {
        validate_curve(&self.position, 3, timebase, "camera_position")?;
        validate_curve(
            &self.rotation_degrees,
            3,
            timebase,
            "camera_rotation_degrees",
        )?;
        self.projection.validate(timebase)?;
        validate_curve(&self.clipping, 2, timebase, "camera_clipping")?;
        for keyframe in self.clipping.keyframes() {
            let values = keyframe.value().components();
            if values[0] <= 0.0 || values[1] <= values[0] {
                return Err(invalid_error(
                    "create_camera",
                    "invalid_clipping_range",
                    "camera near clipping must be positive and far clipping must be greater",
                ));
            }
        }
        Ok(())
    }

    fn sample(&self, time: RationalTime) -> Result<SampledCamera> {
        let position = sample_curve::<3>(&self.position, time, "camera_position")?;
        let rotation = sample_curve::<3>(&self.rotation_degrees, time, "camera_rotation_degrees")?;
        let clipping = sample_curve::<2>(&self.clipping, time, "camera_clipping")?;
        if clipping[0] <= 0.0 || clipping[1] <= clipping[0] {
            return Err(invalid_error(
                "sample_camera",
                "invalid_clipping_range",
                "sampled camera near clipping must be positive and far clipping must be greater",
            ));
        }
        let world = multiply(translation(position), euler_rotation(rotation));
        Ok(SampledCamera {
            position,
            rotation_degrees: rotation,
            projection: self.projection.sample(time)?,
            near: clipping[0],
            far: clipping[1],
            view_matrix: inverse_rigid(world),
        })
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            retime_curve(&self.position, new_start, new_end)?,
            retime_curve(&self.rotation_degrees, new_start, new_end)?,
            self.projection.retimed(new_start, new_end)?,
            retime_curve(&self.clipping, new_start, new_end)?,
        )
    }
}

/// Directly editable light families supported by the first spatial contract.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpatialLight {
    /// Uniform scene contribution.
    Ambient {
        /// Animated scene-linear RGB color.
        color: AnimationCurve,
        /// Animated nonnegative linear intensity.
        intensity: AnimationCurve,
    },
    /// Infinite directional diffuse light.
    Directional {
        /// Animated XYZ Euler orientation. Local positive Z points toward the light.
        rotation_degrees: AnimationCurve,
        /// Animated scene-linear RGB color.
        color: AnimationCurve,
        /// Animated nonnegative linear intensity.
        intensity: AnimationCurve,
    },
    /// Positional diffuse light with deterministic bounded attenuation.
    Point {
        /// Animated world position.
        position: AnimationCurve,
        /// Animated scene-linear RGB color.
        color: AnimationCurve,
        /// Animated nonnegative linear intensity.
        intensity: AnimationCurve,
        /// Animated positive attenuation distance.
        range: AnimationCurve,
    },
}

impl SpatialLight {
    /// Creates one checked ambient light.
    pub fn ambient(color: AnimationCurve, intensity: AnimationCurve) -> Result<Self> {
        let light = Self::Ambient { color, intensity };
        light.validate(light.timebase())?;
        Ok(light)
    }

    /// Creates one checked directional light.
    pub fn directional(
        rotation_degrees: AnimationCurve,
        color: AnimationCurve,
        intensity: AnimationCurve,
    ) -> Result<Self> {
        let light = Self::Directional {
            rotation_degrees,
            color,
            intensity,
        };
        light.validate(light.timebase())?;
        Ok(light)
    }

    /// Creates one checked point light.
    pub fn point(
        position: AnimationCurve,
        color: AnimationCurve,
        intensity: AnimationCurve,
        range: AnimationCurve,
    ) -> Result<Self> {
        let light = Self::Point {
            position,
            color,
            intensity,
            range,
        };
        light.validate(light.timebase())?;
        Ok(light)
    }

    /// Returns the exact animation clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        match self {
            Self::Ambient { color, .. }
            | Self::Directional { color, .. }
            | Self::Point { color, .. } => color.timebase(),
        }
    }

    fn validate(&self, timebase: Timebase) -> Result<()> {
        let (color, intensity) = match self {
            Self::Ambient { color, intensity }
            | Self::Directional {
                color, intensity, ..
            }
            | Self::Point {
                color, intensity, ..
            } => (color, intensity),
        };
        validate_curve(color, 3, timebase, "light_color")?;
        validate_curve(intensity, 1, timebase, "light_intensity")?;
        validate_authored_values(color, "light_color", |value| value >= 0.0)?;
        validate_authored_values(intensity, "light_intensity", |value| value >= 0.0)?;
        match self {
            Self::Ambient { .. } => Ok(()),
            Self::Directional {
                rotation_degrees, ..
            } => validate_curve(rotation_degrees, 3, timebase, "light_rotation_degrees"),
            Self::Point {
                position, range, ..
            } => {
                validate_curve(position, 3, timebase, "light_position")?;
                validate_curve(range, 1, timebase, "light_range")?;
                validate_authored_values(range, "light_range", |value| value > 0.0)
            }
        }
    }

    fn sample(&self, time: RationalTime) -> Result<SampledLight> {
        let (color_curve, intensity_curve) = match self {
            Self::Ambient { color, intensity }
            | Self::Directional {
                color, intensity, ..
            }
            | Self::Point {
                color, intensity, ..
            } => (color, intensity),
        };
        let color = sample_curve::<3>(color_curve, time, "light_color")?;
        let intensity = sample_curve::<1>(intensity_curve, time, "light_intensity")?[0];
        if color.iter().any(|value| *value < 0.0) || intensity < 0.0 {
            return Err(invalid_error(
                "sample_light",
                "light_value_out_of_range",
                "sampled light color and intensity must remain nonnegative",
            ));
        }
        let kind = match self {
            Self::Ambient { .. } => SampledLightKind::Ambient,
            Self::Directional {
                rotation_degrees, ..
            } => {
                let rotation = sample_curve::<3>(rotation_degrees, time, "light_rotation_degrees")?;
                let direction_to_light = normalize(transform_direction(
                    euler_rotation(rotation),
                    [0.0, 0.0, 1.0],
                ))?;
                SampledLightKind::Directional { direction_to_light }
            }
            Self::Point {
                position, range, ..
            } => {
                let position = sample_curve::<3>(position, time, "light_position")?;
                let range = sample_curve::<1>(range, time, "light_range")?[0];
                if range <= 0.0 {
                    return Err(invalid_error(
                        "sample_light",
                        "light_range_out_of_range",
                        "sampled point-light range must remain positive",
                    ));
                }
                SampledLightKind::Point { position, range }
            }
        };
        Ok(SampledLight {
            kind,
            color,
            intensity,
        })
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        match self {
            Self::Ambient { color, intensity } => Self::ambient(
                retime_curve(color, new_start, new_end)?,
                retime_curve(intensity, new_start, new_end)?,
            ),
            Self::Directional {
                rotation_degrees,
                color,
                intensity,
            } => Self::directional(
                retime_curve(rotation_degrees, new_start, new_end)?,
                retime_curve(color, new_start, new_end)?,
                retime_curve(intensity, new_start, new_end)?,
            ),
            Self::Point {
                position,
                color,
                intensity,
                range,
            } => Self::point(
                retime_curve(position, new_start, new_end)?,
                retime_curve(color, new_start, new_end)?,
                retime_curve(intensity, new_start, new_end)?,
                retime_curve(range, new_start, new_end)?,
            ),
        }
    }
}

/// Ordering policy for sampled planar layers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DepthOrdering {
    /// Preserve authored bottom-to-top order exactly.
    LayerStack,
    /// Sort far to near by camera-space anchor depth, retaining stack order for ties.
    CameraDepth,
}

/// Exact shutter sampling settings on the owning composition clock.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MotionBlur {
    shutter_open_ticks: i64,
    shutter_close_ticks: i64,
    sample_count: u16,
}

impl MotionBlur {
    /// Creates one checked exact-tick shutter sample sequence.
    pub fn new(
        shutter_open_ticks: i64,
        shutter_close_ticks: i64,
        sample_count: u16,
    ) -> Result<Self> {
        if sample_count == 0 || sample_count > MAX_MOTION_SAMPLES {
            return Err(resource_error(
                "create_motion_blur",
                "motion_sample_limit",
                "motion blur sample count must be within the supported nonzero bound",
            ));
        }
        let span = i128::from(shutter_close_ticks) - i128::from(shutter_open_ticks);
        if span < 0 {
            return Err(invalid_error(
                "create_motion_blur",
                "shutter_order",
                "motion blur shutter close must not precede shutter open",
            ));
        }
        if sample_count == 1 {
            if span != 0 {
                return Err(invalid_error(
                    "create_motion_blur",
                    "single_sample_shutter",
                    "one motion sample requires equal shutter open and close ticks",
                ));
            }
        } else {
            let intervals = i128::from(sample_count - 1);
            if span == 0 || span % intervals != 0 {
                return Err(invalid_error(
                    "create_motion_blur",
                    "inexact_shutter_samples",
                    "shutter endpoints must divide into exact distinct project-clock samples",
                ));
            }
        }
        Ok(Self {
            shutter_open_ticks,
            shutter_close_ticks,
            sample_count,
        })
    }

    /// Returns the frame-relative shutter open offset in exact project-clock ticks.
    #[must_use]
    pub const fn shutter_open_ticks(self) -> i64 {
        self.shutter_open_ticks
    }

    /// Returns the frame-relative shutter close offset in exact project-clock ticks.
    #[must_use]
    pub const fn shutter_close_ticks(self) -> i64 {
        self.shutter_close_ticks
    }

    /// Returns the bounded exact sample count.
    #[must_use]
    pub const fn sample_count(self) -> u16 {
        self.sample_count
    }

    /// Returns the exact deterministic shutter offsets in temporal order.
    pub fn sample_offsets(self) -> Result<Vec<i64>> {
        Self::new(
            self.shutter_open_ticks,
            self.shutter_close_ticks,
            self.sample_count,
        )?;
        if self.sample_count == 1 {
            return Ok(vec![self.shutter_open_ticks]);
        }
        let step = (i128::from(self.shutter_close_ticks) - i128::from(self.shutter_open_ticks))
            / i128::from(self.sample_count - 1);
        (0..self.sample_count)
            .map(|index| {
                i128::from(self.shutter_open_ticks)
                    .checked_add(i128::from(index) * step)
                    .and_then(|value| i64::try_from(value).ok())
                    .ok_or_else(|| {
                        resource_error(
                            "sample_motion_blur",
                            "shutter_sample_overflow",
                            "motion blur shutter sample exceeds the supported time coordinate",
                        )
                    })
            })
            .collect()
    }
}

/// Complete editable spatial settings for one existing composition.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialScene {
    composition_id: CompositionId,
    camera: SpatialCamera,
    #[serde(deserialize_with = "deserialize_bounded_spatial_lights")]
    lights: Vec<SpatialLight>,
    depth_ordering: DepthOrdering,
    motion_blur: MotionBlur,
}

impl SpatialScene {
    /// Creates one checked scene.
    pub fn new(
        composition_id: CompositionId,
        camera: SpatialCamera,
        lights: impl IntoIterator<Item = SpatialLight>,
        depth_ordering: DepthOrdering,
        motion_blur: MotionBlur,
    ) -> Result<Self> {
        let lights = lights.into_iter().collect::<Vec<_>>();
        if lights.len() > MAX_SPATIAL_LIGHTS {
            return Err(resource_error(
                "create_spatial_scene",
                "light_limit",
                "spatial scene exceeds the supported light bound",
            ));
        }
        let scene = Self {
            composition_id,
            camera,
            lights,
            depth_ordering,
            motion_blur,
        };
        scene.validate(scene.camera.timebase())?;
        Ok(scene)
    }

    /// Returns the owning composition identity.
    #[must_use]
    pub const fn composition_id(&self) -> CompositionId {
        self.composition_id
    }

    /// Returns the editable camera.
    #[must_use]
    pub const fn camera(&self) -> &SpatialCamera {
        &self.camera
    }

    /// Returns lights in stable authored order.
    #[must_use]
    pub fn lights(&self) -> &[SpatialLight] {
        &self.lights
    }

    /// Returns the layer ordering policy.
    #[must_use]
    pub const fn depth_ordering(&self) -> DepthOrdering {
        self.depth_ordering
    }

    /// Returns exact shutter settings.
    #[must_use]
    pub const fn motion_blur(&self) -> MotionBlur {
        self.motion_blur
    }

    /// Returns the exact scene animation clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.camera.timebase()
    }

    fn validate(&self, timebase: Timebase) -> Result<()> {
        if self.lights.len() > MAX_SPATIAL_LIGHTS {
            return Err(resource_error(
                "validate_spatial_scene",
                "light_limit",
                "spatial scene exceeds the supported light bound",
            ));
        }
        self.camera.validate(timebase)?;
        for light in &self.lights {
            light.validate(timebase)?;
        }
        self.motion_blur.sample_offsets().map(|_| ())
    }

    fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        Self::new(
            self.composition_id,
            self.camera.retimed(new_start, new_end)?,
            self.lights
                .iter()
                .map(|light| light.retimed(new_start, new_end))
                .collect::<Result<Vec<_>>>()?,
            self.depth_ordering,
            self.motion_blur,
        )
    }
}

/// Strict reusable spatial state over the existing visual composition owner.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialCompositionArtifact {
    schema_revision: u32,
    compositions: CompositionArtifact<SpatialLayer>,
    scenes: Vec<SpatialScene>,
}

impl SpatialCompositionArtifact {
    /// Creates a checked artifact with exactly one scene per reusable composition.
    pub fn new(
        compositions: CompositionArtifact<SpatialLayer>,
        scenes: impl IntoIterator<Item = SpatialScene>,
    ) -> Result<Self> {
        let mut scenes = scenes.into_iter().collect::<Vec<_>>();
        scenes.sort_by_key(SpatialScene::composition_id);
        let artifact = Self {
            schema_revision: SPATIAL_COMPOSITION_SCHEMA_REVISION,
            compositions,
            scenes,
        };
        artifact.validate()?;
        Ok(artifact)
    }

    /// Returns the authoritative structural composition artifact.
    #[must_use]
    pub const fn compositions(&self) -> &CompositionArtifact<SpatialLayer> {
        &self.compositions
    }

    /// Returns per-composition settings in canonical composition-ID order.
    #[must_use]
    pub fn scenes(&self) -> &[SpatialScene] {
        &self.scenes
    }

    /// Looks up one composition's scene settings.
    #[must_use]
    pub fn scene(&self, composition_id: CompositionId) -> Option<&SpatialScene> {
        self.scenes
            .binary_search_by_key(&composition_id, SpatialScene::composition_id)
            .ok()
            .map(|index| &self.scenes[index])
    }

    /// Returns a copy with replacement scene settings.
    pub fn with_scene(&self, replacement: SpatialScene) -> Result<Self> {
        let index = self
            .scenes
            .binary_search_by_key(&replacement.composition_id, SpatialScene::composition_id)
            .map_err(|_| {
                invalid_error(
                    "replace_spatial_scene",
                    "scene_not_found",
                    "replacement spatial scene has no matching composition",
                )
            })?;
        let mut scenes = self.scenes.clone();
        scenes[index] = replacement;
        Self::new(self.compositions.clone(), scenes)
    }

    /// Returns a copy over a replacement structural artifact.
    pub fn with_compositions(
        &self,
        compositions: CompositionArtifact<SpatialLayer>,
    ) -> Result<Self> {
        Self::new(compositions, self.scenes.clone())
    }

    /// Samples one exact root frame into inspectable world-space state.
    pub fn sample(&self, time: RationalTime, rounding: TimeRounding) -> Result<SpatialFrame> {
        self.validate()?;
        let resolved = self.compositions.resolve_frame(time, rounding)?;
        let scene = self.scene(resolved.composition_id()).ok_or_else(|| {
            invalid_error(
                "sample_spatial_artifact",
                "missing_root_scene",
                "root composition has no spatial scene settings",
            )
        })?;
        let camera = scene.camera.sample(resolved.time())?;
        let lights = scene
            .lights
            .iter()
            .map(|light| light.sample(resolved.time()))
            .collect::<Result<Vec<_>>>()?;
        let mut layers = Vec::with_capacity(resolved.layers().len());
        for (stack_index, resolved_layer) in resolved.layers().iter().enumerate() {
            let mut world = identity();
            let mut opacity = 1.0;
            let mut path = Vec::new();
            let mut final_transform = None;
            for step in resolved_layer.path() {
                let composition = self
                    .compositions
                    .composition(step.composition_id())
                    .expect("resolved layer step retains its owning composition");
                for parent_id in step.parent_chain() {
                    let parent = composition
                        .layer(*parent_id)
                        .expect("resolved parent chain retains every parent layer");
                    let source_time = parent
                        .time_remap()
                        .sample(step.composition_time(), rounding)?
                        .source_time();
                    let payload = parent.payload();
                    let sampled = payload
                        .transform
                        .sample(step.composition_time(), payload.space)?;
                    world = multiply(world, sampled.matrix);
                    opacity *= sampled.opacity;
                    path.push(SampledLayerPathStep {
                        composition_id: step.composition_id(),
                        layer_id: parent.id(),
                        composition_time: step.composition_time(),
                        source_time,
                        anchor: sampled.anchor,
                        position: sampled.position,
                        rotation_degrees: sampled.rotation_degrees,
                        scale: sampled.scale,
                        opacity: sampled.opacity,
                        local_matrix: sampled.matrix,
                    });
                }
                let payload = step.layer().payload();
                let sampled = payload
                    .transform
                    .sample(step.composition_time(), payload.space)?;
                world = multiply(world, sampled.matrix);
                opacity *= sampled.opacity;
                final_transform = Some(sampled);
                path.push(SampledLayerPathStep {
                    composition_id: step.composition_id(),
                    layer_id: step.layer().id(),
                    composition_time: step.composition_time(),
                    source_time: step.source_time(),
                    anchor: sampled.anchor,
                    position: sampled.position,
                    rotation_degrees: sampled.rotation_degrees,
                    scale: sampled.scale,
                    opacity: sampled.opacity,
                    local_matrix: sampled.matrix,
                });
            }
            let sampled = final_transform.expect("resolved spatial layer paths are nonempty");
            if !(0.0..=1.0).contains(&opacity) {
                return Err(invalid_error(
                    "sample_spatial_artifact",
                    "composed_opacity_out_of_range",
                    "composed layer opacity must remain between zero and one",
                ));
            }
            let final_layer = resolved_layer
                .path()
                .last()
                .expect("resolved spatial layer paths are nonempty")
                .layer()
                .payload();
            let world_anchor = transform_point(world, sampled.anchor)?;
            let world_normal = normalize(cross(
                transform_direction(world, [1.0, 0.0, 0.0]),
                transform_direction(world, [0.0, 1.0, 0.0]),
            ))?;
            let camera_point = transform_point(camera.view_matrix, world_anchor)?;
            let depth = -camera_point[2];
            if !depth.is_finite() {
                return Err(invalid_error(
                    "sample_spatial_artifact",
                    "nonfinite_camera_depth",
                    "sampled layer camera depth must remain finite",
                ));
            }
            layers.push(SampledSpatialLayer {
                source_id: final_layer.source_id,
                space: final_layer.space,
                sampling: final_layer.sampling,
                resolved_kind: resolved_layer.kind(),
                path,
                world_matrix: world,
                world_anchor,
                world_normal,
                opacity,
                camera_depth: depth,
                stack_index,
            });
        }
        if scene.depth_ordering == DepthOrdering::CameraDepth {
            layers.sort_by(|left, right| {
                right
                    .camera_depth
                    .total_cmp(&left.camera_depth)
                    .then_with(|| left.stack_index.cmp(&right.stack_index))
                    .then_with(|| left.source_id.cmp(&right.source_id))
            });
        }
        Ok(SpatialFrame {
            composition_id: resolved.composition_id(),
            time: resolved.time(),
            layers,
            camera,
            lights,
            depth_ordering: scene.depth_ordering,
            motion_sample_offsets: scene.motion_blur.sample_offsets()?,
        })
    }

    /// Retimes every ranged spatial animation curve to one new exact interval.
    ///
    /// Static one-key curves remain static. Structural layer-to-source time maps remain owned and
    /// unchanged by `CompositionArtifact`.
    pub fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        if new_start.timebase() != new_end.timebase() || new_end.value() <= new_start.value() {
            return Err(invalid_error(
                "retime_spatial_artifact",
                "invalid_retime_range",
                "spatial retime endpoints must use one clock and increase strictly",
            ));
        }
        let mut compositions = self.compositions.clone();
        let originals = self.compositions.compositions().to_vec();
        for composition in &originals {
            let local_start =
                new_start.checked_rescale(composition.timebase(), TimeRounding::Exact)?;
            let local_end = new_end.checked_rescale(composition.timebase(), TimeRounding::Exact)?;
            let mut updated = composition.clone();
            for layer in composition.layers() {
                let replacement = layer
                    .clone()
                    .with_payload(layer.payload().retimed(local_start, local_end)?);
                updated = updated.with_replaced_layer(replacement)?;
            }
            compositions = compositions.with_replaced_composition(updated)?;
        }
        let scenes = self
            .scenes
            .iter()
            .map(|scene| {
                let composition = self
                    .compositions
                    .composition(scene.composition_id())
                    .expect("validated spatial scene retains its composition");
                let local_start =
                    new_start.checked_rescale(composition.timebase(), TimeRounding::Exact)?;
                let local_end =
                    new_end.checked_rescale(composition.timebase(), TimeRounding::Exact)?;
                scene.retimed(local_start, local_end)
            })
            .collect::<Result<Vec<_>>>()?;
        Self::new(compositions, scenes)
    }

    fn validate(&self) -> Result<()> {
        if self.schema_revision != SPATIAL_COMPOSITION_SCHEMA_REVISION {
            return Err(unsupported_error(
                "validate_spatial_artifact",
                "unsupported_schema_revision",
                "spatial composition schema revision is not supported",
            ));
        }
        if self.scenes.len() != self.compositions.compositions().len() {
            return Err(invalid_error(
                "validate_spatial_artifact",
                "scene_coverage_mismatch",
                "spatial artifact requires exactly one scene for every composition",
            ));
        }
        let mut scene_ids = BTreeSet::new();
        for scene in &self.scenes {
            if !scene_ids.insert(scene.composition_id) {
                return Err(invalid_error(
                    "validate_spatial_artifact",
                    "duplicate_scene",
                    "spatial scene composition identity appears more than once",
                ));
            }
            let composition = self
                .compositions
                .composition(scene.composition_id)
                .ok_or_else(|| {
                    invalid_error(
                        "validate_spatial_artifact",
                        "scene_without_composition",
                        "spatial scene references a missing composition",
                    )
                })?;
            scene.validate(composition.timebase())?;
            for layer in composition.layers() {
                layer.payload().validate(composition.timebase())?;
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for SpatialCompositionArtifact {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SpatialCompositionWire::deserialize(deserializer)?;
        if wire.schema_revision != SPATIAL_COMPOSITION_SCHEMA_REVISION {
            return Err(D::Error::custom(format_args!(
                "unsupported spatial composition schema revision {}",
                wire.schema_revision
            )));
        }
        Self::new(wire.compositions, wire.scenes).map_err(D::Error::custom)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpatialCompositionWire {
    schema_revision: u32,
    compositions: CompositionArtifact<SpatialLayer>,
    #[serde(deserialize_with = "deserialize_bounded_spatial_scenes")]
    scenes: Vec<SpatialScene>,
}

fn deserialize_bounded_spatial_lights<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<SpatialLight>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::new(MAX_SPATIAL_LIGHTS, "spatial lights"))
}

fn deserialize_bounded_spatial_scenes<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<SpatialScene>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::new(MAX_COMPOSITIONS, "spatial scenes"))
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
                "{} exceed the supported bound of {}",
                self.description, self.limit
            )));
        }
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(self.limit));
        loop {
            if values.len() == self.limit {
                if sequence.next_element::<IgnoredAny>()?.is_some() {
                    return Err(A::Error::custom(format_args!(
                        "{} exceed the supported bound of {}",
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

/// Sampled camera projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SampledProjection {
    /// Perspective projection.
    Perspective {
        /// Vertical field of view in degrees.
        vertical_field_of_view_degrees: f64,
    },
    /// Orthographic projection.
    Orthographic {
        /// Vertical world extent.
        vertical_size: f64,
    },
}

/// One sampled camera with an inspectable world-to-camera matrix.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampledCamera {
    position: [f64; 3],
    rotation_degrees: [f64; 3],
    projection: SampledProjection,
    near: f64,
    far: f64,
    view_matrix: Matrix4,
}

impl SampledCamera {
    /// Returns the world position.
    #[must_use]
    pub const fn position(self) -> [f64; 3] {
        self.position
    }

    /// Returns XYZ Euler orientation in degrees.
    #[must_use]
    pub const fn rotation_degrees(self) -> [f64; 3] {
        self.rotation_degrees
    }

    /// Returns the sampled projection.
    #[must_use]
    pub const fn projection(self) -> SampledProjection {
        self.projection
    }

    /// Returns near clipping distance.
    #[must_use]
    pub const fn near(self) -> f64 {
        self.near
    }

    /// Returns far clipping distance.
    #[must_use]
    pub const fn far(self) -> f64 {
        self.far
    }

    /// Returns the row-major world-to-camera matrix.
    #[must_use]
    pub const fn view_matrix(self) -> Matrix4 {
        self.view_matrix
    }
}

/// Sampled executable light kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SampledLightKind {
    /// Uniform contribution.
    Ambient,
    /// Infinite direction toward the light.
    Directional {
        /// Normalized world direction from a surface toward the light.
        direction_to_light: [f64; 3],
    },
    /// Positional light.
    Point {
        /// World position.
        position: [f64; 3],
        /// Positive attenuation distance.
        range: f64,
    },
}

/// One sampled light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampledLight {
    kind: SampledLightKind,
    color: [f64; 3],
    intensity: f64,
}

impl SampledLight {
    /// Returns the executable light family and geometry.
    #[must_use]
    pub const fn kind(self) -> SampledLightKind {
        self.kind
    }

    /// Returns scene-linear RGB color.
    #[must_use]
    pub const fn color(self) -> [f64; 3] {
        self.color
    }

    /// Returns nonnegative linear intensity.
    #[must_use]
    pub const fn intensity(self) -> f64 {
        self.intensity
    }
}

/// One retained resolved layer-path step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampledLayerPathStep {
    composition_id: CompositionId,
    layer_id: CompositionLayerId,
    composition_time: RationalTime,
    source_time: RationalTime,
    anchor: [f64; 3],
    position: [f64; 3],
    rotation_degrees: [f64; 3],
    scale: [f64; 3],
    opacity: f64,
    local_matrix: Matrix4,
}

impl SampledLayerPathStep {
    /// Returns the owning composition.
    #[must_use]
    pub const fn composition_id(self) -> CompositionId {
        self.composition_id
    }

    /// Returns the stable layer identity.
    #[must_use]
    pub const fn layer_id(self) -> CompositionLayerId {
        self.layer_id
    }

    /// Returns the exact coordinate used to sample this step's spatial properties.
    #[must_use]
    pub const fn composition_time(self) -> RationalTime {
        self.composition_time
    }

    /// Returns the exact mapped source coordinate.
    #[must_use]
    pub const fn source_time(self) -> RationalTime {
        self.source_time
    }

    /// Returns the sampled local anchor.
    #[must_use]
    pub const fn anchor(self) -> [f64; 3] {
        self.anchor
    }

    /// Returns the sampled local position.
    #[must_use]
    pub const fn position(self) -> [f64; 3] {
        self.position
    }

    /// Returns the sampled local XYZ Euler rotation in degrees.
    #[must_use]
    pub const fn rotation_degrees(self) -> [f64; 3] {
        self.rotation_degrees
    }

    /// Returns the sampled local scale.
    #[must_use]
    pub const fn scale(self) -> [f64; 3] {
        self.scale
    }

    /// Returns the sampled local normalized opacity.
    #[must_use]
    pub const fn opacity(self) -> f64 {
        self.opacity
    }

    /// Returns the sampled row-major local transform matrix.
    #[must_use]
    pub const fn local_matrix(self) -> Matrix4 {
        self.local_matrix
    }
}

/// One sampled layer in final bottom-to-top or camera-depth order.
#[derive(Clone, Debug, PartialEq)]
pub struct SampledSpatialLayer {
    source_id: SpatialSourceId,
    space: LayerSpace,
    sampling: ReferenceSampling,
    resolved_kind: ResolvedLayerKind,
    path: Vec<SampledLayerPathStep>,
    world_matrix: Matrix4,
    world_anchor: [f64; 3],
    world_normal: [f64; 3],
    opacity: f64,
    camera_depth: f64,
    stack_index: usize,
}

impl SampledSpatialLayer {
    /// Returns the final source identity.
    #[must_use]
    pub const fn source_id(&self) -> SpatialSourceId {
        self.source_id
    }

    /// Returns the final layer's dimensional interpretation.
    #[must_use]
    pub const fn space(&self) -> LayerSpace {
        self.space
    }

    /// Returns reconstruction mode.
    #[must_use]
    pub const fn sampling(&self) -> ReferenceSampling {
        self.sampling
    }

    /// Returns whether this is a visual leaf or retained precomposition boundary.
    #[must_use]
    pub const fn resolved_kind(&self) -> ResolvedLayerKind {
        self.resolved_kind
    }

    /// Returns every retained transform step in root-to-leaf order.
    #[must_use]
    pub fn path(&self) -> &[SampledLayerPathStep] {
        &self.path
    }

    /// Returns the row-major local-to-world matrix.
    #[must_use]
    pub const fn world_matrix(&self) -> Matrix4 {
        self.world_matrix
    }

    /// Returns the transformed final anchor.
    #[must_use]
    pub const fn world_anchor(&self) -> [f64; 3] {
        self.world_anchor
    }

    /// Returns the normalized transformed planar normal.
    #[must_use]
    pub const fn world_normal(&self) -> [f64; 3] {
        self.world_normal
    }

    /// Returns composed normalized opacity.
    #[must_use]
    pub const fn opacity(&self) -> f64 {
        self.opacity
    }

    /// Returns positive distance along the camera's negative-Z viewing direction.
    #[must_use]
    pub const fn camera_depth(&self) -> f64 {
        self.camera_depth
    }

    /// Returns authored bottom-to-top stack position before optional depth sorting.
    #[must_use]
    pub const fn stack_index(&self) -> usize {
        self.stack_index
    }
}

/// Complete inspectable sample of one root composition frame.
#[derive(Clone, Debug, PartialEq)]
pub struct SpatialFrame {
    composition_id: CompositionId,
    time: RationalTime,
    layers: Vec<SampledSpatialLayer>,
    camera: SampledCamera,
    lights: Vec<SampledLight>,
    depth_ordering: DepthOrdering,
    motion_sample_offsets: Vec<i64>,
}

impl SpatialFrame {
    /// Returns the root composition identity.
    #[must_use]
    pub const fn composition_id(&self) -> CompositionId {
        self.composition_id
    }

    /// Returns the exact sampled coordinate.
    #[must_use]
    pub const fn time(&self) -> RationalTime {
        self.time
    }

    /// Returns layers in effective back-to-front order.
    #[must_use]
    pub fn layers(&self) -> &[SampledSpatialLayer] {
        &self.layers
    }

    /// Returns the sampled camera.
    #[must_use]
    pub const fn camera(&self) -> SampledCamera {
        self.camera
    }

    /// Returns lights in authored order.
    #[must_use]
    pub fn lights(&self) -> &[SampledLight] {
        &self.lights
    }

    /// Returns the effective depth-ordering policy.
    #[must_use]
    pub const fn depth_ordering(&self) -> DepthOrdering {
        self.depth_ordering
    }

    /// Returns exact shutter offsets in temporal order.
    #[must_use]
    pub fn motion_sample_offsets(&self) -> &[i64] {
        &self.motion_sample_offsets
    }
}

/// Creates the graph-native spatial definition shared by timeline and graph hosts.
pub fn spatial_node_definition(
    default: SpatialCompositionArtifact,
) -> Result<EffectNodeDefinition<GraphValue<SpatialCompositionArtifact>>> {
    let image_type = value_type("superi.value.image")?;
    let composition_type = value_type("superi.value.spatial-composition")?;
    let capability = CapabilityId::new("superi.render.gpu")
        .map_err(|error| literal_error("capability", error.to_string()))?;
    EffectNodeDefinition::new(
        NodeSchemaId::new(
            NodeTypeId::new("superi.effect.spatial-composition")
                .map_err(|error| literal_error("node_type", error.to_string()))?,
            SemanticVersion::new(1, 0, 0),
        ),
        EffectMetadata::new(
            "Spatial Composition",
            "Editable 2D and 3D layers with cameras, lights, depth, and motion blur.",
            "Compositing",
        )?,
        [EffectPortDefinition::new(
            PortSchema::new(
                port_name("layers")?,
                image_type.clone(),
                PortCardinality::Variadic,
            ),
            "Layers",
            "Stable source images addressed by the editable composition state.",
        )?],
        [EffectPortDefinition::new(
            PortSchema::new(port_name("result")?, image_type, PortCardinality::Single),
            "Result",
            "Rendered canonical scene-linear composition image.",
        )?],
        [EffectParameterDefinition::new(
            ParameterSchema::new(
                parameter_name("composition")?,
                composition_type.clone(),
                true,
            ),
            "Composition",
            "Complete editable and reusable spatial composition state.",
            ParameterControl::Automatic,
            TypedParameterValue::new(composition_type, GraphValue::domain(default)),
        )?],
        NodeBehavior::new(
            TimeBehavior::Unbounded,
            RoiBehavior::Custom,
            ColorRequirements::Exact(ColorSpace::ACESCG),
            Determinism::Deterministic,
            CachePolicy::PerRegion,
        ),
        CapabilitySet::new([capability]),
    )
}

/// Evaluates a bounded CPU oracle for spatial state into real canonical pixels.
///
/// This path is a deterministic validation oracle. Production playback remains GPU-owned.
pub fn render_spatial_reference(
    artifact: &SpatialCompositionArtifact,
    time: RationalTime,
    rounding: TimeRounding,
    sources: &BTreeMap<SpatialSourceId, Image>,
    output: PixelBounds,
    limits: &ImageLimits,
) -> Result<Image> {
    let base = artifact.sample(time, rounding)?;
    let mut frames = Vec::with_capacity(base.motion_sample_offsets().len());
    let mut evaluations = 0_usize;
    let mut expected_sources = BTreeSet::new();
    for offset in base.motion_sample_offsets() {
        let value = base.time().value().checked_add(*offset).ok_or_else(|| {
            resource_error(
                "render_spatial_reference",
                "motion_time_overflow",
                "motion sample exceeds the supported project-time coordinate",
            )
        })?;
        let frame = artifact.sample(
            RationalTime::new(value, base.time().timebase()),
            TimeRounding::Exact,
        )?;
        if frame.layers().len() > MAX_REFERENCE_SPATIAL_LAYERS {
            return Err(resource_error(
                "render_spatial_reference",
                "reference_layer_limit",
                "spatial frame exceeds the bounded CPU layer limit",
            ));
        }
        evaluations = evaluations
            .checked_add(frame.layers().len())
            .ok_or_else(|| {
                resource_error(
                    "render_spatial_reference",
                    "reference_evaluation_overflow",
                    "spatial reference evaluation count overflowed",
                )
            })?;
        if evaluations > MAX_REFERENCE_SPATIAL_EVALUATIONS {
            return Err(resource_error(
                "render_spatial_reference",
                "reference_evaluation_limit",
                "motion-blurred spatial request exceeds the bounded CPU evaluation limit",
            ));
        }
        expected_sources.extend(frame.layers().iter().map(SampledSpatialLayer::source_id));
        frames.push(frame);
    }
    let template_id = expected_sources.first().copied().ok_or_else(|| {
        invalid_error(
            "render_spatial_reference",
            "empty_spatial_frame",
            "spatial reference rendering requires at least one resolved visual layer",
        )
    })?;
    for source_id in &expected_sources {
        if !sources.contains_key(source_id) {
            return Err(invalid_error(
                "render_spatial_reference",
                "missing_spatial_source",
                "spatial reference rendering is missing a resolved source image",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "render_spatial_reference")
                    .with_field("source_id", source_id.to_string()),
            ));
        }
    }
    let template = sources
        .get(&template_id)
        .expect("validated source identity must exist");
    let mut accumulated = None;
    for (sample_index, frame) in frames.iter().enumerate() {
        let mut canvas = evaluate_reference(
            &ReferenceEffectState::Opacity { opacity: 0.0 },
            std::slice::from_ref(template),
            output,
            limits,
        )?;
        for layer in frame.layers() {
            if layer.camera_depth() < frame.camera().near()
                || layer.camera_depth() > frame.camera().far()
            {
                continue;
            }
            let source = sources
                .get(&layer.source_id())
                .expect("validated source identity must exist");
            let matrix = layer_projection_matrix(layer, frame.camera(), output)?;
            let transformed = evaluate_reference(
                &ReferenceEffectState::Transform {
                    matrix,
                    sampling: layer.sampling(),
                },
                std::slice::from_ref(source),
                output,
                limits,
            )?;
            let lit = evaluate_reference(
                &ReferenceEffectState::Grade {
                    gain: lighting_gain(layer, frame.lights())?,
                    offset: [0.0; 3],
                },
                &[transformed],
                output,
                limits,
            )?;
            let prepared = evaluate_reference(
                &ReferenceEffectState::Opacity {
                    opacity: layer.opacity(),
                },
                &[lit],
                output,
                limits,
            )?;
            canvas = evaluate_reference(
                &ReferenceEffectState::Composite {
                    operator: ReferenceCompositeOperator::DestinationOver,
                    opacity: 1.0,
                },
                &[canvas, prepared],
                output,
                limits,
            )?;
        }
        accumulated = Some(match accumulated {
            None => canvas,
            Some(previous) => evaluate_reference(
                &ReferenceEffectState::CrossDissolve {
                    progress: 1.0 / (sample_index as f64 + 1.0),
                },
                &[previous, canvas],
                output,
                limits,
            )?,
        });
    }
    accumulated.ok_or_else(|| {
        invalid_error(
            "render_spatial_reference",
            "empty_motion_samples",
            "spatial reference rendering requires at least one motion sample",
        )
    })
}

fn layer_projection_matrix(
    layer: &SampledSpatialLayer,
    camera: SampledCamera,
    output: PixelBounds,
) -> Result<Matrix3> {
    let view_model = multiply(camera.view_matrix(), layer.world_matrix());
    let width = f64::from(output.width());
    let height = f64::from(output.height());
    if width <= 0.0 || height <= 0.0 {
        return Err(invalid_error(
            "project_spatial_layer",
            "empty_output_region",
            "spatial projection requires a nonempty output region",
        ));
    }
    let aspect = width / height;
    let (clip_x, clip_y, clip_w) = match camera.projection() {
        SampledProjection::Perspective {
            vertical_field_of_view_degrees,
        } => {
            let focal = 1.0 / (vertical_field_of_view_degrees.to_radians() * 0.5).tan();
            (
                scale_row(view_model[0], focal / aspect),
                scale_row(view_model[1], focal),
                scale_row(view_model[2], -1.0),
            )
        }
        SampledProjection::Orthographic { vertical_size } => {
            let half_y = vertical_size * 0.5;
            let half_x = half_y * aspect;
            (
                scale_row(view_model[0], 1.0 / half_x),
                scale_row(view_model[1], 1.0 / half_y),
                [0.0, 0.0, 0.0, 1.0],
            )
        }
    };
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let center_x = f64::from(output.min_x()) + half_width;
    let center_y = f64::from(output.min_y()) + half_height;
    let screen_x = add_rows(scale_row(clip_x, half_width), scale_row(clip_w, center_x));
    let screen_y = add_rows(scale_row(clip_y, -half_height), scale_row(clip_w, center_y));
    Matrix3::from_rows([plane_row(screen_x), plane_row(screen_y), plane_row(clip_w)])
        .map_err(|error| error.with_context(ErrorContext::new(COMPONENT, "project_spatial_layer")))
}

fn lighting_gain(layer: &SampledSpatialLayer, lights: &[SampledLight]) -> Result<[f64; 3]> {
    let mut gain = [0.0; 3];
    for light in lights {
        let factor = match light.kind() {
            SampledLightKind::Ambient => light.intensity(),
            SampledLightKind::Directional { direction_to_light } => {
                dot(layer.world_normal(), direction_to_light).max(0.0) * light.intensity()
            }
            SampledLightKind::Point { position, range } => {
                let toward = subtract(position, layer.world_anchor());
                let distance = dot(toward, toward).sqrt();
                let diffuse = if distance <= MATRIX_EPSILON {
                    1.0
                } else {
                    dot(layer.world_normal(), toward.map(|value| value / distance)).max(0.0)
                };
                let attenuation = 1.0 / (1.0 + (distance / range).powi(2));
                diffuse * attenuation * light.intensity()
            }
        };
        for (gain_channel, light_channel) in gain.iter_mut().zip(light.color()) {
            *gain_channel += light_channel * factor;
        }
    }
    if gain.iter().any(|value| !value.is_finite()) {
        return Err(invalid_error(
            "evaluate_spatial_lighting",
            "nonfinite_lighting_result",
            "spatial lighting produced a nonfinite channel gain",
        ));
    }
    Ok(gain)
}

fn value_type(value: &str) -> Result<ValueTypeId> {
    ValueTypeId::new(value).map_err(|error| literal_error("value_type", error.to_string()))
}

fn port_name(value: &str) -> Result<PortName> {
    PortName::new(value).map_err(|error| literal_error("port_name", error.to_string()))
}

fn parameter_name(value: &str) -> Result<ParameterName> {
    ParameterName::new(value).map_err(|error| literal_error("parameter_name", error.to_string()))
}

fn literal_error(field: &'static str, detail: String) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "repository-owned spatial graph literal is invalid",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "build_spatial_definition")
            .with_field("field", field)
            .with_field("detail", detail),
    )
}

fn scale_row(row: [f64; 4], scale: f64) -> [f64; 4] {
    row.map(|value| value * scale)
}

fn add_rows(left: [f64; 4], right: [f64; 4]) -> [f64; 4] {
    std::array::from_fn(|index| left[index] + right[index])
}

const fn plane_row(row: [f64; 4]) -> [f64; 3] {
    [row[0], row[1], row[3]]
}

fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|index| left[index] - right[index])
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    (0..3).map(|index| left[index] * right[index]).sum()
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

#[derive(Clone, Copy)]
struct SampledTransform {
    anchor: [f64; 3],
    position: [f64; 3],
    rotation_degrees: [f64; 3],
    scale: [f64; 3],
    opacity: f64,
    matrix: Matrix4,
}

fn validate_curve(
    curve: &AnimationCurve,
    components: usize,
    timebase: Timebase,
    field: &'static str,
) -> Result<()> {
    if curve.timebase() != timebase {
        return Err(invalid_error(
            "validate_spatial_curve",
            "curve_timebase_mismatch",
            "every spatial animation curve must use its owning composition clock",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_spatial_curve").with_field("field", field),
        ));
    }
    if curve
        .keyframes()
        .iter()
        .any(|keyframe| keyframe.value().component_count() != components)
    {
        return Err(invalid_error(
            "validate_spatial_curve",
            "curve_component_count",
            "spatial animation curve has the wrong component count",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_spatial_curve")
                .with_field("field", field)
                .with_field("expected_components", components.to_string()),
        ));
    }
    Ok(())
}

fn validate_authored_values(
    curve: &AnimationCurve,
    field: &'static str,
    predicate: impl Fn(f64) -> bool,
) -> Result<()> {
    for (keyframe_index, keyframe) in curve.keyframes().iter().enumerate() {
        if keyframe
            .value()
            .components()
            .iter()
            .copied()
            .any(|value| !predicate(value))
        {
            return Err(invalid_error(
                "validate_spatial_curve",
                "authored_value_out_of_range",
                "spatial animation keyframe is outside its supported domain",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "validate_spatial_curve")
                    .with_field("field", field)
                    .with_field("keyframe_index", keyframe_index.to_string()),
            ));
        }
    }
    Ok(())
}

fn sample_curve<const N: usize>(
    curve: &AnimationCurve,
    time: RationalTime,
    field: &'static str,
) -> Result<[f64; N]> {
    let sampled = curve.evaluate(time)?;
    if sampled.component_count() != N {
        return Err(invalid_error(
            "sample_spatial_curve",
            "curve_component_count",
            "sampled spatial curve has the wrong component count",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "sample_spatial_curve").with_field("field", field),
        ));
    }
    let mut values = [0.0; N];
    values.copy_from_slice(sampled.components());
    Ok(values)
}

fn retime_curve(
    curve: &AnimationCurve,
    new_start: RationalTime,
    new_end: RationalTime,
) -> Result<AnimationCurve> {
    if curve.keyframes().len() == 1 {
        Ok(curve.clone())
    } else {
        curve.retimed(new_start, new_end)
    }
}

const fn identity() -> Matrix4 {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn multiply(left: Matrix4, right: Matrix4) -> Matrix4 {
    let mut result = [[0.0; 4]; 4];
    for row in 0..4 {
        for column in 0..4 {
            result[row][column] = (0..4)
                .map(|index| left[row][index] * right[index][column])
                .sum();
        }
    }
    result
}

fn translation(value: [f64; 3]) -> Matrix4 {
    let mut matrix = identity();
    matrix[0][3] = value[0];
    matrix[1][3] = value[1];
    matrix[2][3] = value[2];
    matrix
}

fn scaling(value: [f64; 3]) -> Matrix4 {
    [
        [value[0], 0.0, 0.0, 0.0],
        [0.0, value[1], 0.0, 0.0],
        [0.0, 0.0, value[2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn euler_rotation(degrees: [f64; 3]) -> Matrix4 {
    let [x, y, z] = degrees.map(f64::to_radians);
    let (sin_x, cos_x) = x.sin_cos();
    let (sin_y, cos_y) = y.sin_cos();
    let (sin_z, cos_z) = z.sin_cos();
    let rotation_x = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, cos_x, -sin_x, 0.0],
        [0.0, sin_x, cos_x, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let rotation_y = [
        [cos_y, 0.0, sin_y, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [-sin_y, 0.0, cos_y, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let rotation_z = [
        [cos_z, -sin_z, 0.0, 0.0],
        [sin_z, cos_z, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    multiply(rotation_z, multiply(rotation_y, rotation_x))
}

fn compose_transform(
    anchor: [f64; 3],
    position: [f64; 3],
    rotation: [f64; 3],
    scale: [f64; 3],
) -> Matrix4 {
    multiply(
        translation(position),
        multiply(
            euler_rotation(rotation),
            multiply(
                scaling(scale),
                translation([-anchor[0], -anchor[1], -anchor[2]]),
            ),
        ),
    )
}

fn inverse_rigid(matrix: Matrix4) -> Matrix4 {
    let mut result = identity();
    for row in 0..3 {
        for column in 0..3 {
            result[row][column] = matrix[column][row];
        }
    }
    let translation = [matrix[0][3], matrix[1][3], matrix[2][3]];
    for row in result.iter_mut().take(3) {
        row[3] = -(row[0] * translation[0] + row[1] * translation[1] + row[2] * translation[2]);
    }
    result
}

fn transform_point(matrix: Matrix4, point: [f64; 3]) -> Result<[f64; 3]> {
    let values = [point[0], point[1], point[2], 1.0];
    let transformed = std::array::from_fn::<_, 4, _>(|row| {
        (0..4)
            .map(|column| matrix[row][column] * values[column])
            .sum::<f64>()
    });
    if transformed.iter().any(|value| !value.is_finite()) || transformed[3].abs() <= MATRIX_EPSILON
    {
        return Err(invalid_error(
            "transform_spatial_point",
            "invalid_homogeneous_point",
            "spatial transform produced a nonfinite point or zero homogeneous coordinate",
        ));
    }
    Ok([
        transformed[0] / transformed[3],
        transformed[1] / transformed[3],
        transformed[2] / transformed[3],
    ])
}

fn transform_direction(matrix: Matrix4, direction: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|row| {
        matrix[row][0] * direction[0]
            + matrix[row][1] * direction[1]
            + matrix[row][2] * direction[2]
    })
}

fn normalize(value: [f64; 3]) -> Result<[f64; 3]> {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f64>()
        .sqrt();
    if !length.is_finite() || length <= MATRIX_EPSILON {
        return Err(invalid_error(
            "normalize_spatial_vector",
            "degenerate_spatial_vector",
            "spatial direction must retain a finite nonzero length",
        ));
    }
    Ok(value.map(|component| component / length))
}

fn invalid_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    spatial_error(ErrorCategory::InvalidInput, operation, reason, message)
}

fn resource_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    spatial_error(ErrorCategory::ResourceExhausted, operation, reason, message)
}

fn unsupported_error(
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Error {
    spatial_error(ErrorCategory::Unsupported, operation, reason, message)
}

fn spatial_error(
    category: ErrorCategory,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
