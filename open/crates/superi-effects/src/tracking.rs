//! Editable motion-tracking state and deterministic bounded CPU reference solving.
//!
//! Persisted artifacts retain selections, authored corrections, derived samples, model state, and
//! solver evidence. Source luma frames remain explicit transient inputs. This module does not own
//! decode, color conversion, image storage, GPU resources, timeline attachment, or project files.

use std::cmp::Ordering;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{Error as _, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::{Matrix3, Point2, Rect};
use superi_core::time::Timebase;
use superi_graph::value::FiniteF64;

const COMPONENT: &str = "superi-effects.tracking";

/// Current incompatible revision of the standalone tracking artifact wire.
pub const TRACKING_ARTIFACT_SCHEMA_REVISION: u32 = 1;
/// Maximum tracks retained by one editable artifact.
pub const MAX_TRACKS: usize = 1_024;
/// Maximum feature observations retained by one track sample.
pub const MAX_FEATURES_PER_TRACK: usize = 256;
/// Maximum authored or derived samples retained by one track.
pub const MAX_SAMPLES_PER_TRACK: usize = 100_000;
/// Maximum luma samples accepted by one transient solver frame.
pub const MAX_TRACKING_FRAME_PIXELS: usize = 16_777_216;

const MODEL_RESIDUAL_LIMIT: f64 = 2.0;
const CONSENSUS_RESIDUAL_LIMIT: f64 = 0.75;
const MAX_PLANAR_MODEL_CANDIDATES: usize = 256;

struct BoundedVecVisitor<T, const MAX: usize>(PhantomData<T>);

impl<'de, T, const MAX: usize> Visitor<'de> for BoundedVecVisitor<T, MAX>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "a sequence containing at most {MAX} values")
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        if sequence.size_hint().is_some_and(|size| size > MAX) {
            return Err(A::Error::custom(
                "tracking sequence exceeds its supported bound",
            ));
        }
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX));
        while let Some(value) = sequence.next_element()? {
            if values.len() == MAX {
                return Err(A::Error::custom(
                    "tracking sequence exceeds its supported bound",
                ));
            }
            values.push(value);
        }
        Ok(values)
    }
}

fn deserialize_bounded_vec<'de, D, T, const MAX: usize>(
    deserializer: D,
) -> std::result::Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::<T, MAX>(PhantomData))
}

fn deserialize_features<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<TrackedFeature>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec::<D, TrackedFeature, MAX_FEATURES_PER_TRACK>(deserializer)
}

fn deserialize_landmarks<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<CameraLandmark>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec::<D, CameraLandmark, MAX_FEATURES_PER_TRACK>(deserializer)
}

fn deserialize_observations<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<TrackingObservation>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec::<D, TrackingObservation, MAX_FEATURES_PER_TRACK>(deserializer)
}

fn deserialize_samples<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<TrackingSample>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec::<D, TrackingSample, MAX_SAMPLES_PER_TRACK>(deserializer)
}

fn deserialize_tracks<'de, D>(deserializer: D) -> std::result::Result<Vec<TrackingTrack>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec::<D, TrackingTrack, MAX_TRACKS>(deserializer)
}

/// Stable artifact-local tracking identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrackId(u64);

impl TrackId {
    /// Creates an identity from its canonical integer representation.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the canonical integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stable feature identity inside one track.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeatureId(u64);

impl FeatureId {
    /// Creates an identity from its canonical integer representation.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the canonical integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One exact finite two-dimensional tracking coordinate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingPoint {
    x: FiniteF64,
    y: FiniteF64,
}

impl TrackingPoint {
    /// Creates a point while preserving exact finite binary64 bits.
    pub fn new(x: f64, y: f64) -> Result<Self> {
        Ok(Self {
            x: FiniteF64::new(x)?,
            y: FiniteF64::new(y)?,
        })
    }

    /// Preserves one core-owned finite point for strict artifact persistence.
    #[must_use]
    pub fn from_core(point: Point2) -> Self {
        Self::new(point.x(), point.y()).expect("core point coordinates are finite")
    }

    /// Reconstructs the shared core geometry contract.
    pub fn into_core(self) -> Result<Point2> {
        Point2::new(self.x(), self.y())
    }

    /// Returns the horizontal coordinate.
    #[must_use]
    pub fn x(self) -> f64 {
        self.x.get()
    }

    /// Returns the vertical coordinate.
    #[must_use]
    pub fn y(self) -> f64 {
        self.y.get()
    }
}

/// One exact finite three-dimensional world coordinate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingPoint3 {
    x: FiniteF64,
    y: FiniteF64,
    z: FiniteF64,
}

impl TrackingPoint3 {
    /// Creates a finite world coordinate.
    pub fn new(x: f64, y: f64, z: f64) -> Result<Self> {
        Ok(Self {
            x: FiniteF64::new(x)?,
            y: FiniteF64::new(y)?,
            z: FiniteF64::new(z)?,
        })
    }

    /// Returns the x coordinate.
    #[must_use]
    pub fn x(self) -> f64 {
        self.x.get()
    }

    /// Returns the y coordinate.
    #[must_use]
    pub fn y(self) -> f64 {
        self.y.get()
    }

    /// Returns the z coordinate.
    #[must_use]
    pub fn z(self) -> f64 {
        self.z.get()
    }

    fn values(self) -> [f64; 3] {
        [self.x(), self.y(), self.z()]
    }
}

/// One exact finite row-major projective transform.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingMatrix3 {
    values: [FiniteF64; 9],
}

impl TrackingMatrix3 {
    /// Creates a matrix from finite row-major values.
    pub fn new(values: [f64; 9]) -> Result<Self> {
        let mut output = [FiniteF64::new(0.0)?; 9];
        for (destination, source) in output.iter_mut().zip(values) {
            *destination = FiniteF64::new(source)?;
        }
        Ok(Self { values: output })
    }

    /// Creates the identity transform.
    pub fn identity() -> Self {
        Self::new([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0])
            .expect("identity matrix values are finite")
    }

    /// Preserves one core-owned finite matrix for strict artifact persistence.
    #[must_use]
    pub fn from_core(matrix: Matrix3) -> Self {
        let rows = matrix.rows();
        Self::new([
            rows[0][0], rows[0][1], rows[0][2], rows[1][0], rows[1][1], rows[1][2], rows[2][0],
            rows[2][1], rows[2][2],
        ])
        .expect("core matrix values are finite")
    }

    /// Reconstructs the shared core matrix contract.
    pub fn into_core(self) -> Result<Matrix3> {
        let values = self.values();
        Matrix3::from_rows([
            [values[0], values[1], values[2]],
            [values[3], values[4], values[5]],
            [values[6], values[7], values[8]],
        ])
    }

    /// Returns finite row-major values.
    #[must_use]
    pub fn values(self) -> [f64; 9] {
        self.values.map(FiniteF64::get)
    }

    fn transform_point(self, point: TrackingPoint) -> Result<TrackingPoint> {
        let values = self.values();
        let denominator = values[6] * point.x() + values[7] * point.y() + values[8];
        if !denominator.is_finite() || denominator.abs() <= f64::EPSILON {
            return Err(tracking_error(
                "transform_point",
                "projective_horizon",
                "tracking transform maps a point to the projective horizon",
            ));
        }
        TrackingPoint::new(
            (values[0] * point.x() + values[1] * point.y() + values[2]) / denominator,
            (values[3] * point.x() + values[4] * point.y() + values[5]) / denominator,
        )
    }

    fn checked_mul(self, right: Self) -> Result<Self> {
        let left = self.values();
        let right = right.values();
        let mut output = [0.0; 9];
        for row in 0..3 {
            for column in 0..3 {
                output[row * 3 + column] = (0..3)
                    .map(|inner| left[row * 3 + inner] * right[inner * 3 + column])
                    .sum();
            }
        }
        Self::new(output)
    }
}

/// One exact finite axis-aligned tracked region.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingRect {
    min: TrackingPoint,
    max: TrackingPoint,
}

impl TrackingRect {
    /// Creates a normalized tracked region.
    pub fn new(min: TrackingPoint, max: TrackingPoint) -> Result<Self> {
        let region = Self { min, max };
        region.validate()?;
        Ok(region)
    }

    /// Preserves one core-owned finite region for strict artifact persistence.
    #[must_use]
    pub fn from_core(region: Rect) -> Self {
        Self::new(
            TrackingPoint::from_core(region.min()),
            TrackingPoint::from_core(region.max()),
        )
        .expect("core rectangle bounds are normalized and finite")
    }

    /// Reconstructs the shared core region contract.
    pub fn into_core(self) -> Result<Rect> {
        Rect::new(self.min.into_core()?, self.max.into_core()?)
    }

    fn validate(self) -> Result<()> {
        if self.min.x() > self.max.x() || self.min.y() > self.max.y() {
            return Err(tracking_error(
                "create_region",
                "inverted_bounds",
                "tracking region minimum edges must not exceed maximum edges",
            ));
        }
        Ok(())
    }

    /// Returns the inclusive minimum corner.
    #[must_use]
    pub const fn min(self) -> TrackingPoint {
        self.min
    }

    /// Returns the exclusive maximum corner.
    #[must_use]
    pub const fn max(self) -> TrackingPoint {
        self.max
    }

    /// Returns the nonnegative width.
    #[must_use]
    pub fn width(self) -> f64 {
        self.max.x() - self.min.x()
    }

    /// Returns the nonnegative height.
    #[must_use]
    pub fn height(self) -> f64 {
        self.max.y() - self.min.y()
    }

    /// Returns whether either extent is zero.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.width() == 0.0 || self.height() == 0.0
    }

    fn transformed_bounds(self, transform: TrackingMatrix3) -> Result<Self> {
        let corners = [
            self.min,
            TrackingPoint::new(self.max.x(), self.min.y())?,
            self.max,
            TrackingPoint::new(self.min.x(), self.max.y())?,
        ];
        let mut minimum = [f64::INFINITY; 2];
        let mut maximum = [f64::NEG_INFINITY; 2];
        for corner in corners {
            let transformed = transform.transform_point(corner)?;
            minimum[0] = minimum[0].min(transformed.x());
            minimum[1] = minimum[1].min(transformed.y());
            maximum[0] = maximum[0].max(transformed.x());
            maximum[1] = maximum[1].max(transformed.y());
        }
        Self::new(
            TrackingPoint::new(minimum[0], minimum[1])?,
            TrackingPoint::new(maximum[0], maximum[1])?,
        )
    }
}

/// Checked calibrated pinhole camera intrinsics in pixel coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraIntrinsics {
    focal_x: FiniteF64,
    focal_y: FiniteF64,
    principal_x: FiniteF64,
    principal_y: FiniteF64,
}

impl CameraIntrinsics {
    /// Creates calibrated intrinsics with positive focal lengths.
    pub fn new(focal_x: f64, focal_y: f64, principal_x: f64, principal_y: f64) -> Result<Self> {
        if focal_x <= 0.0 || focal_y <= 0.0 {
            return Err(tracking_error(
                "create_intrinsics",
                "focal_length",
                "camera focal lengths must be finite and positive",
            ));
        }
        let intrinsics = Self {
            focal_x: FiniteF64::new(focal_x)?,
            focal_y: FiniteF64::new(focal_y)?,
            principal_x: FiniteF64::new(principal_x)?,
            principal_y: FiniteF64::new(principal_y)?,
        };
        intrinsics.validate()?;
        Ok(intrinsics)
    }

    fn validate(self) -> Result<()> {
        if self.focal_x() <= 0.0 || self.focal_y() <= 0.0 {
            return Err(tracking_error(
                "create_intrinsics",
                "focal_length",
                "camera focal lengths must be finite and positive",
            ));
        }
        Ok(())
    }

    /// Returns horizontal focal length in pixels.
    #[must_use]
    pub fn focal_x(self) -> f64 {
        self.focal_x.get()
    }

    /// Returns vertical focal length in pixels.
    #[must_use]
    pub fn focal_y(self) -> f64 {
        self.focal_y.get()
    }

    /// Returns horizontal principal point in pixels.
    #[must_use]
    pub fn principal_x(self) -> f64 {
        self.principal_x.get()
    }

    /// Returns vertical principal point in pixels.
    #[must_use]
    pub fn principal_y(self) -> f64 {
        self.principal_y.get()
    }
}

/// One world-to-camera pose represented by a Rodrigues rotation vector and translation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraPose {
    rotation: [FiniteF64; 3],
    translation: [FiniteF64; 3],
}

impl CameraPose {
    /// Creates a finite world-to-camera pose.
    pub fn new(rotation: [f64; 3], translation: [f64; 3]) -> Result<Self> {
        let mut checked_rotation = [FiniteF64::new(0.0)?; 3];
        let mut checked_translation = [FiniteF64::new(0.0)?; 3];
        for (destination, source) in checked_rotation.iter_mut().zip(rotation) {
            *destination = FiniteF64::new(source)?;
        }
        for (destination, source) in checked_translation.iter_mut().zip(translation) {
            *destination = FiniteF64::new(source)?;
        }
        Ok(Self {
            rotation: checked_rotation,
            translation: checked_translation,
        })
    }

    /// Creates an identity world-to-camera pose.
    pub fn identity() -> Self {
        Self::new([0.0; 3], [0.0; 3]).expect("zero pose values are finite")
    }

    /// Returns the Rodrigues rotation vector.
    #[must_use]
    pub fn rotation(self) -> [f64; 3] {
        self.rotation.map(FiniteF64::get)
    }

    /// Returns world-to-camera translation.
    #[must_use]
    pub fn translation(self) -> [f64; 3] {
        self.translation.map(FiniteF64::get)
    }

    fn parameters(self) -> [f64; 6] {
        let rotation = self.rotation();
        let translation = self.translation();
        [
            rotation[0],
            rotation[1],
            rotation[2],
            translation[0],
            translation[1],
            translation[2],
        ]
    }

    fn from_parameters(parameters: [f64; 6]) -> Result<Self> {
        Self::new(
            [parameters[0], parameters[1], parameters[2]],
            [parameters[3], parameters[4], parameters[5]],
        )
    }
}

/// One calibrated known 3D landmark and its selected reference image position.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraLandmark {
    id: FeatureId,
    world: TrackingPoint3,
    image_position: TrackingPoint,
}

impl CameraLandmark {
    /// Creates one known landmark correspondence.
    #[must_use]
    pub const fn new(id: u64, world: TrackingPoint3, image_position: TrackingPoint) -> Self {
        Self {
            id: FeatureId::from_raw(id),
            world,
            image_position,
        }
    }

    /// Returns stable feature identity.
    #[must_use]
    pub const fn id(self) -> FeatureId {
        self.id
    }

    /// Returns the known world coordinate.
    #[must_use]
    pub const fn world(self) -> TrackingPoint3 {
        self.world
    }

    /// Returns the selected image position.
    #[must_use]
    pub const fn image_position(self) -> TrackingPoint {
        self.image_position
    }
}

/// One selected image feature with stable identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackedFeature {
    id: FeatureId,
    position: TrackingPoint,
}

impl TrackedFeature {
    /// Creates one selected feature.
    #[must_use]
    pub const fn new(id: u64, position: TrackingPoint) -> Self {
        Self {
            id: FeatureId::from_raw(id),
            position,
        }
    }

    /// Returns the stable feature identity.
    #[must_use]
    pub const fn id(self) -> FeatureId {
        self.id
    }

    /// Returns the selected image position.
    #[must_use]
    pub const fn position(self) -> TrackingPoint {
        self.position
    }
}

/// Complete editable selection used to initialize and solve one track.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrackingSelection {
    /// One locally registered feature.
    Point { feature: TrackedFeature },
    /// A planar region constrained by at least four feature correspondences.
    Planar {
        region: TrackingRect,
        #[serde(deserialize_with = "deserialize_features")]
        features: Vec<TrackedFeature>,
    },
    /// An object region constrained to 2D rotation, translation, and uniform scale.
    Object {
        region: TrackingRect,
        #[serde(deserialize_with = "deserialize_features")]
        features: Vec<TrackedFeature>,
    },
    /// A calibrated camera pose constrained by known noncoplanar 3D landmarks.
    Camera {
        intrinsics: CameraIntrinsics,
        initial_pose: CameraPose,
        #[serde(deserialize_with = "deserialize_landmarks")]
        landmarks: Vec<CameraLandmark>,
    },
}

impl TrackingSelection {
    /// Creates a point selection.
    #[must_use]
    pub const fn point(feature: TrackedFeature) -> Self {
        Self::Point { feature }
    }

    /// Creates a canonical planar selection.
    pub fn planar(
        region: TrackingRect,
        features: impl IntoIterator<Item = TrackedFeature>,
    ) -> Result<Self> {
        if region.is_empty() {
            return Err(tracking_error(
                "create_planar_selection",
                "empty_region",
                "planar tracking requires a nonempty selected region",
            ));
        }
        Ok(Self::Planar {
            region,
            features: canonical_features(features, 4, "create_planar_selection")?,
        })
    }

    /// Creates a canonical object selection.
    pub fn object(
        region: TrackingRect,
        features: impl IntoIterator<Item = TrackedFeature>,
    ) -> Result<Self> {
        if region.is_empty() {
            return Err(tracking_error(
                "create_object_selection",
                "empty_region",
                "object tracking requires a nonempty selected region",
            ));
        }
        Ok(Self::Object {
            region,
            features: canonical_features(features, 2, "create_object_selection")?,
        })
    }

    /// Creates a calibrated known-landmark camera selection.
    pub fn camera(
        intrinsics: CameraIntrinsics,
        initial_pose: CameraPose,
        landmarks: impl IntoIterator<Item = CameraLandmark>,
    ) -> Result<Self> {
        let mut landmarks = landmarks.into_iter().collect::<Vec<_>>();
        if !(6..=MAX_FEATURES_PER_TRACK).contains(&landmarks.len()) {
            return Err(tracking_error(
                "create_camera_selection",
                "landmark_count",
                "camera tracking requires from six through 256 known landmarks",
            ));
        }
        landmarks.sort_by_key(|landmark| landmark.id());
        if landmarks
            .windows(2)
            .any(|window| window[0].id() == window[1].id())
        {
            return Err(tracking_error(
                "create_camera_selection",
                "duplicate_landmark",
                "camera tracking cannot repeat a landmark identity",
            ));
        }
        validate_non_coplanar(&landmarks)?;
        Ok(Self::Camera {
            intrinsics,
            initial_pose,
            landmarks,
        })
    }

    /// Returns the stable selection kind code.
    #[must_use]
    pub const fn kind_code(&self) -> &'static str {
        match self {
            Self::Point { .. } => "point",
            Self::Planar { .. } => "planar",
            Self::Object { .. } => "object",
            Self::Camera { .. } => "camera",
        }
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::Point { .. } => Ok(()),
            Self::Planar { region, features } => {
                region.validate()?;
                if region.is_empty() {
                    return Err(tracking_error(
                        "validate_selection",
                        "empty_region",
                        "planar tracking requires a nonempty selected region",
                    ));
                }
                validate_feature_slice(features, 4)
            }
            Self::Object { region, features } => {
                region.validate()?;
                if region.is_empty() {
                    return Err(tracking_error(
                        "validate_selection",
                        "empty_region",
                        "object tracking requires a nonempty selected region",
                    ));
                }
                validate_feature_slice(features, 2)
            }
            Self::Camera {
                intrinsics,
                landmarks,
                ..
            } => {
                intrinsics.validate()?;
                if !(6..=MAX_FEATURES_PER_TRACK).contains(&landmarks.len())
                    || landmarks
                        .windows(2)
                        .any(|window| window[0].id() >= window[1].id())
                {
                    return Err(tracking_error(
                        "validate_selection",
                        "landmark_order",
                        "camera landmarks must be bounded, unique, and ordered by identity",
                    ));
                }
                validate_non_coplanar(landmarks)
            }
        }
    }

    fn reference_sample(&self, frame: i64) -> Result<TrackingSample> {
        match self {
            Self::Point { feature } => TrackingSample::new(
                frame,
                TrackingModel::Point {
                    position: feature.position(),
                },
                [TrackingObservation::new(
                    feature.id(),
                    feature.position(),
                    1.0,
                )?],
            ),
            Self::Planar { region, features } => TrackingSample::new(
                frame,
                TrackingModel::Planar {
                    homography: TrackingMatrix3::identity(),
                    region: *region,
                },
                features
                    .iter()
                    .map(|feature| TrackingObservation::new(feature.id(), feature.position(), 1.0))
                    .collect::<Result<Vec<_>>>()?,
            ),
            Self::Object { region, features } => TrackingSample::new(
                frame,
                TrackingModel::Object {
                    transform: TrackingMatrix3::identity(),
                    region: *region,
                },
                features
                    .iter()
                    .map(|feature| TrackingObservation::new(feature.id(), feature.position(), 1.0))
                    .collect::<Result<Vec<_>>>()?,
            ),
            Self::Camera {
                initial_pose,
                landmarks,
                ..
            } => TrackingSample::new(
                frame,
                TrackingModel::Camera {
                    pose: *initial_pose,
                },
                landmarks
                    .iter()
                    .map(|landmark| {
                        TrackingObservation::new(landmark.id(), landmark.image_position(), 1.0)
                    })
                    .collect::<Result<Vec<_>>>()?,
            ),
        }
    }

    fn validate_sample(&self, sample: &TrackingSample) -> Result<()> {
        let expected_ids = match (self, sample.model()) {
            (Self::Point { feature }, TrackingModel::Point { .. }) => vec![feature.id()],
            (Self::Planar { features, .. }, TrackingModel::Planar { .. })
            | (Self::Object { features, .. }, TrackingModel::Object { .. }) => {
                features.iter().map(|feature| feature.id()).collect()
            }
            (Self::Camera { landmarks, .. }, TrackingModel::Camera { .. }) => {
                landmarks.iter().map(|landmark| landmark.id()).collect()
            }
            _ => {
                return Err(tracking_error(
                    "validate_sample",
                    "model_kind",
                    "tracking sample model kind must match its selection",
                ));
            }
        };
        if sample.observations().len() != expected_ids.len()
            || sample
                .observations()
                .iter()
                .map(|observation| observation.feature_id())
                .ne(expected_ids)
        {
            return Err(tracking_error(
                "validate_sample",
                "observation_identity",
                "tracking sample observations must match every selected feature identity",
            ));
        }
        match (self, sample.model()) {
            (Self::Point { .. }, TrackingModel::Point { position }) => {
                if sample.observations()[0].position() != *position {
                    return Err(tracking_error(
                        "validate_sample",
                        "point_model_observation",
                        "point model position must equal its feature observation",
                    ));
                }
            }
            (
                Self::Planar {
                    region: reference_region,
                    features,
                },
                TrackingModel::Planar { homography, region },
            ) => {
                if reference_region.transformed_bounds(*homography)? != *region {
                    return Err(tracking_error(
                        "validate_sample",
                        "planar_region",
                        "planar model region must match its reference homography",
                    ));
                }
                validate_selection_residual(features, *homography, sample.observations())?;
            }
            (
                Self::Object {
                    region: reference_region,
                    features,
                },
                TrackingModel::Object { transform, region },
            ) => {
                if reference_region.transformed_bounds(*transform)? != *region {
                    return Err(tracking_error(
                        "validate_sample",
                        "object_region",
                        "object model region must match its reference similarity transform",
                    ));
                }
                validate_selection_residual(features, *transform, sample.observations())?;
            }
            (
                Self::Camera {
                    intrinsics,
                    landmarks,
                    ..
                },
                TrackingModel::Camera { pose },
            ) => validate_camera_residual(*intrinsics, landmarks, *pose, sample.observations())?,
            _ => unreachable!("model kind was validated above"),
        }
        Ok(())
    }
}

fn validate_selection_residual(
    features: &[TrackedFeature],
    transform: TrackingMatrix3,
    observations: &[TrackingObservation],
) -> Result<()> {
    let mut squared_residual = 0.0;
    for (feature, observation) in features.iter().zip(observations) {
        let projected = transform.transform_point(feature.position())?;
        squared_residual += (projected.x() - observation.position().x()).powi(2)
            + (projected.y() - observation.position().y()).powi(2);
    }
    let residual = (squared_residual / features.len() as f64).sqrt();
    if !residual.is_finite() || residual > MODEL_RESIDUAL_LIMIT {
        return Err(tracking_error(
            "validate_sample",
            "model_observation_residual",
            "tracking model does not explain its feature observations",
        ));
    }
    Ok(())
}

fn validate_camera_residual(
    intrinsics: CameraIntrinsics,
    landmarks: &[CameraLandmark],
    pose: CameraPose,
    observations: &[TrackingObservation],
) -> Result<()> {
    for (landmark, observation) in landmarks.iter().zip(observations) {
        let projected = project_camera(intrinsics, pose.parameters(), landmark.world())?;
        if (projected.x() - observation.position().x())
            .hypot(projected.y() - observation.position().y())
            > MODEL_RESIDUAL_LIMIT
        {
            return Err(tracking_error(
                "validate_sample",
                "camera_observation_residual",
                "camera pose does not explain its landmark observations",
            ));
        }
    }
    Ok(())
}

fn canonical_features(
    features: impl IntoIterator<Item = TrackedFeature>,
    minimum: usize,
    operation: &'static str,
) -> Result<Vec<TrackedFeature>> {
    let mut features = features.into_iter().collect::<Vec<_>>();
    if features.len() < minimum || features.len() > MAX_FEATURES_PER_TRACK {
        return Err(tracking_error(
            operation,
            "feature_count",
            "tracking selection feature count is outside its supported bounds",
        ));
    }
    features.sort_by_key(|feature| feature.id());
    if features
        .windows(2)
        .any(|window| window[0].id() == window[1].id())
    {
        return Err(tracking_error(
            operation,
            "duplicate_feature",
            "tracking selection cannot repeat a feature identity",
        ));
    }
    Ok(features)
}

fn validate_feature_slice(features: &[TrackedFeature], minimum: usize) -> Result<()> {
    if features.len() < minimum
        || features.len() > MAX_FEATURES_PER_TRACK
        || features
            .windows(2)
            .any(|window| window[0].id() >= window[1].id())
    {
        return Err(tracking_error(
            "validate_selection",
            "feature_order",
            "tracking features must be bounded, unique, and ordered by identity",
        ));
    }
    Ok(())
}

fn validate_non_coplanar(landmarks: &[CameraLandmark]) -> Result<()> {
    let origin = landmarks[0].world().values();
    let farthest = landmarks
        .iter()
        .skip(1)
        .map(|landmark| subtract3(landmark.world().values(), origin))
        .max_by(|left, right| {
            dot3(*left, *left)
                .partial_cmp(&dot3(*right, *right))
                .unwrap_or(Ordering::Equal)
        })
        .expect("camera selection has at least six landmarks");
    let normal = landmarks
        .iter()
        .skip(1)
        .map(|landmark| cross3(farthest, subtract3(landmark.world().values(), origin)))
        .max_by(|left, right| {
            dot3(*left, *left)
                .partial_cmp(&dot3(*right, *right))
                .unwrap_or(Ordering::Equal)
        })
        .expect("camera selection has at least six landmarks");
    let volume = landmarks
        .iter()
        .skip(1)
        .map(|landmark| dot3(normal, subtract3(landmark.world().values(), origin)).abs())
        .fold(0.0, f64::max);
    if dot3(farthest, farthest) <= 1.0e-12 || dot3(normal, normal) <= 1.0e-12 || volume <= 1.0e-9 {
        return Err(tracking_error(
            "create_camera_selection",
            "landmark_degeneracy",
            "camera tracking landmarks must span a noncoplanar 3D configuration",
        ));
    }
    Ok(())
}

/// Inspectable model state at one exact frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrackingModel {
    /// Solved point position.
    Point { position: TrackingPoint },
    /// Reference-to-frame planar homography and transformed selected region.
    Planar {
        homography: TrackingMatrix3,
        region: TrackingRect,
    },
    /// Reference-to-frame 2D similarity transform and transformed object bounds.
    Object {
        transform: TrackingMatrix3,
        region: TrackingRect,
    },
    /// Calibrated world-to-camera pose.
    Camera { pose: CameraPose },
}

impl TrackingModel {
    /// Returns the stable model kind code.
    #[must_use]
    pub const fn kind_code(&self) -> &'static str {
        match self {
            Self::Point { .. } => "point",
            Self::Planar { .. } => "planar",
            Self::Object { .. } => "object",
            Self::Camera { .. } => "camera",
        }
    }
}

/// One solved feature position and its normalized confidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingObservation {
    feature_id: FeatureId,
    position: TrackingPoint,
    confidence: FiniteF64,
}

impl TrackingObservation {
    /// Creates a checked observation.
    pub fn new(feature_id: FeatureId, position: TrackingPoint, confidence: f64) -> Result<Self> {
        if !(0.0..=1.0).contains(&confidence) {
            return Err(tracking_error(
                "create_observation",
                "confidence_domain",
                "tracking observation confidence must be from zero through one",
            ));
        }
        Ok(Self {
            feature_id,
            position,
            confidence: FiniteF64::new(confidence)?,
        })
    }

    /// Returns the stable feature identity.
    #[must_use]
    pub const fn feature_id(self) -> FeatureId {
        self.feature_id
    }

    /// Returns the solved image position.
    #[must_use]
    pub const fn position(self) -> TrackingPoint {
        self.position
    }

    /// Returns normalized confidence.
    #[must_use]
    pub fn confidence(self) -> f64 {
        self.confidence.get()
    }
}

/// Complete inspectable state for one exact integer frame coordinate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingSample {
    frame: i64,
    model: TrackingModel,
    #[serde(deserialize_with = "deserialize_observations")]
    observations: Vec<TrackingObservation>,
}

impl TrackingSample {
    /// Creates a checked sample and canonicalizes observation identity order.
    pub fn new(
        frame: i64,
        model: TrackingModel,
        observations: impl IntoIterator<Item = TrackingObservation>,
    ) -> Result<Self> {
        let mut observations = observations.into_iter().collect::<Vec<_>>();
        if observations.is_empty() || observations.len() > MAX_FEATURES_PER_TRACK {
            return Err(tracking_error(
                "create_sample",
                "observation_count",
                "tracking samples require a bounded nonempty observation set",
            ));
        }
        observations.sort_by_key(|observation| observation.feature_id());
        if observations
            .windows(2)
            .any(|window| window[0].feature_id() == window[1].feature_id())
        {
            return Err(tracking_error(
                "create_sample",
                "duplicate_observation",
                "tracking samples cannot repeat a feature identity",
            ));
        }
        let sample = Self {
            frame,
            model,
            observations,
        };
        sample.validate()?;
        Ok(sample)
    }

    /// Returns the exact artifact-local frame coordinate.
    #[must_use]
    pub const fn frame(&self) -> i64 {
        self.frame
    }

    /// Returns the inspectable solved model.
    #[must_use]
    pub const fn model(&self) -> &TrackingModel {
        &self.model
    }

    /// Returns observations in canonical feature identity order.
    #[must_use]
    pub fn observations(&self) -> &[TrackingObservation] {
        &self.observations
    }

    fn validate(&self) -> Result<()> {
        if self.observations.is_empty() || self.observations.len() > MAX_FEATURES_PER_TRACK {
            return Err(tracking_error(
                "validate_sample",
                "observation_count",
                "tracking samples require a bounded nonempty observation set",
            ));
        }
        if self
            .observations
            .windows(2)
            .any(|window| window[0].feature_id() >= window[1].feature_id())
            || self
                .observations
                .iter()
                .any(|observation| !(0.0..=1.0).contains(&observation.confidence()))
        {
            return Err(tracking_error(
                "validate_sample",
                "observation_order",
                "tracking observations must be finite, bounded, unique, and identity ordered",
            ));
        }
        match &self.model {
            TrackingModel::Planar { region, .. } | TrackingModel::Object { region, .. } => {
                region.validate()?;
            }
            TrackingModel::Point { .. } | TrackingModel::Camera { .. } => {}
        }
        Ok(())
    }
}

/// One stable selection with canonical authored and replaceable derived samples.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingTrack {
    id: TrackId,
    selection: TrackingSelection,
    reference: TrackingSample,
    #[serde(deserialize_with = "deserialize_samples")]
    corrections: Vec<TrackingSample>,
    #[serde(deserialize_with = "deserialize_samples")]
    derived: Vec<TrackingSample>,
}

impl TrackingTrack {
    /// Creates one track and derives its reference model from the selection.
    pub fn new(id: TrackId, reference_frame: i64, selection: TrackingSelection) -> Result<Self> {
        let reference = selection.reference_sample(reference_frame)?;
        let track = Self {
            id,
            selection,
            reference,
            corrections: Vec::new(),
            derived: Vec::new(),
        };
        track.validate()?;
        Ok(track)
    }

    /// Returns the stable track identity.
    #[must_use]
    pub const fn id(&self) -> TrackId {
        self.id
    }

    /// Returns the complete editable selection.
    #[must_use]
    pub const fn selection(&self) -> &TrackingSelection {
        &self.selection
    }

    /// Returns the canonical authored reference sample.
    #[must_use]
    pub const fn reference(&self) -> &TrackingSample {
        &self.reference
    }

    /// Returns manual corrections in increasing frame order.
    #[must_use]
    pub fn corrections(&self) -> &[TrackingSample] {
        &self.corrections
    }

    /// Returns replaceable solver samples in increasing frame order.
    #[must_use]
    pub fn derived_samples(&self) -> &[TrackingSample] {
        &self.derived
    }

    fn validate(&self) -> Result<()> {
        self.selection.validate()?;
        self.reference.validate()?;
        self.selection.validate_sample(&self.reference)?;
        validate_sample_sequence(
            &self.selection,
            self.reference.frame(),
            &self.corrections,
            "correction",
        )?;
        validate_sample_sequence(
            &self.selection,
            self.reference.frame(),
            &self.derived,
            "derived",
        )?;
        if self.corrections.iter().any(|correction| {
            self.derived
                .binary_search_by_key(&correction.frame(), TrackingSample::frame)
                .is_ok()
        }) {
            return Err(tracking_error(
                "validate_track",
                "authored_derived_overlap",
                "manual corrections cannot overlap derived tracking samples",
            ));
        }
        Ok(())
    }
}

fn validate_sample_sequence(
    selection: &TrackingSelection,
    reference_frame: i64,
    samples: &[TrackingSample],
    collection: &'static str,
) -> Result<()> {
    if samples.len() > MAX_SAMPLES_PER_TRACK {
        return Err(tracking_error(
            "validate_track",
            "sample_limit",
            "tracking sample collection exceeds its supported bound",
        ));
    }
    let mut prior = None;
    for sample in samples {
        sample.validate()?;
        selection.validate_sample(sample)?;
        if sample.frame() == reference_frame {
            return Err(tracking_error(
                "validate_track",
                "reference_overlap",
                "tracking samples cannot overlap the authored reference frame",
            ));
        }
        if prior.is_some_and(|frame| frame >= sample.frame()) {
            return Err(tracking_error(
                "validate_track",
                "sample_order",
                "tracking samples must use unique increasing frame coordinates",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "validate_track").with_field("collection", collection),
            ));
        }
        prior = Some(sample.frame());
    }
    Ok(())
}

/// Provenance for one resolved tracking sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackingSampleSource {
    /// Canonical selection reference.
    Reference,
    /// User-authored correction.
    ManualCorrection,
    /// Replaceable solver output.
    Solver,
}

impl TrackingSampleSource {
    /// Returns the stable provenance code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::ManualCorrection => "manual_correction",
            Self::Solver => "solver",
        }
    }
}

/// Borrowed resolved sample with visible provenance.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedTrackingSample<'a> {
    source: TrackingSampleSource,
    sample: &'a TrackingSample,
}

impl<'a> ResolvedTrackingSample<'a> {
    /// Returns the visible sample provenance.
    #[must_use]
    pub const fn source(self) -> TrackingSampleSource {
        self.source
    }

    /// Returns the resolved sample.
    #[must_use]
    pub const fn sample(self) -> &'a TrackingSample {
        self.sample
    }
}

/// Immutable revisioned tracking data shared by workflow roles.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackingArtifact {
    timebase: Timebase,
    revision: u64,
    tracks: Vec<TrackingTrack>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct TrackingArtifactWireRef<'a> {
    schema_revision: u32,
    timebase_numerator: u32,
    timebase_denominator: u32,
    revision: String,
    tracks: &'a [TrackingTrack],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrackingArtifactWire {
    schema_revision: u32,
    timebase_numerator: u32,
    timebase_denominator: u32,
    revision: String,
    #[serde(deserialize_with = "deserialize_tracks")]
    tracks: Vec<TrackingTrack>,
}

impl Serialize for TrackingArtifact {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TrackingArtifactWireRef {
            schema_revision: TRACKING_ARTIFACT_SCHEMA_REVISION,
            timebase_numerator: self.timebase.numerator(),
            timebase_denominator: self.timebase.denominator(),
            revision: self.revision.to_string(),
            tracks: &self.tracks,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TrackingArtifact {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TrackingArtifactWire::deserialize(deserializer)?;
        if wire.schema_revision != TRACKING_ARTIFACT_SCHEMA_REVISION {
            return Err(D::Error::custom(
                "unsupported tracking artifact schema revision",
            ));
        }
        let revision = parse_canonical_revision(&wire.revision).map_err(D::Error::custom)?;
        let timebase = Timebase::new(wire.timebase_numerator, wire.timebase_denominator)
            .map_err(D::Error::custom)?;
        let mut artifact = Self::new(timebase, wire.tracks).map_err(D::Error::custom)?;
        if revision == 0
            && artifact
                .tracks
                .iter()
                .any(|track| !track.corrections.is_empty() || !track.derived.is_empty())
        {
            return Err(D::Error::custom(
                "tracking revision zero cannot contain edited samples",
            ));
        }
        artifact.revision = revision;
        Ok(artifact)
    }
}

impl TrackingArtifact {
    /// Creates a canonical artifact at content revision zero.
    pub fn new(
        timebase: Timebase,
        tracks: impl IntoIterator<Item = TrackingTrack>,
    ) -> Result<Self> {
        let mut tracks = tracks.into_iter().collect::<Vec<_>>();
        if tracks.len() > MAX_TRACKS {
            return Err(tracking_error(
                "create_artifact",
                "track_limit",
                "tracking artifact exceeds its supported track count",
            ));
        }
        tracks.sort_by_key(TrackingTrack::id);
        if tracks
            .windows(2)
            .any(|window| window[0].id() == window[1].id())
        {
            return Err(tracking_error(
                "create_artifact",
                "duplicate_track",
                "tracking artifact cannot repeat a track identity",
            ));
        }
        for track in &tracks {
            track.validate()?;
        }
        Ok(Self {
            timebase,
            revision: 0,
            tracks,
        })
    }

    /// Returns the exact frame clock shared by every track.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns the last successful immutable content edit revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns tracks in canonical identity order.
    #[must_use]
    pub fn tracks(&self) -> &[TrackingTrack] {
        &self.tracks
    }

    /// Returns one track by stable identity.
    #[must_use]
    pub fn track(&self, id: TrackId) -> Option<&TrackingTrack> {
        self.tracks
            .binary_search_by_key(&id, TrackingTrack::id)
            .ok()
            .map(|index| &self.tracks[index])
    }

    /// Inserts one new canonical track.
    pub fn with_track(&self, track: TrackingTrack) -> Result<Self> {
        track.validate()?;
        if self.tracks.len() >= MAX_TRACKS {
            return Err(tracking_category_error(
                ErrorCategory::ResourceExhausted,
                "add_track",
                "track_limit",
                "tracking artifact track limit is exhausted",
            ));
        }
        let index = self
            .tracks
            .binary_search_by_key(&track.id(), TrackingTrack::id)
            .map_or_else(Ok, |_| {
                Err(tracking_category_error(
                    ErrorCategory::Conflict,
                    "add_track",
                    "duplicate_track",
                    "tracking artifact already contains this track identity",
                ))
            })?;
        let mut next = self.clone();
        next.tracks.insert(index, track);
        next.revision = next_revision(self.revision)?;
        Ok(next)
    }

    /// Replaces one complete selection and resets its authored and derived samples explicitly.
    pub fn with_replaced_track(&self, replacement: TrackingTrack) -> Result<Self> {
        replacement.validate()?;
        let index = self
            .tracks
            .binary_search_by_key(&replacement.id(), TrackingTrack::id)
            .map_err(|_| {
                tracking_error(
                    "replace_track",
                    "missing_track",
                    "tracking replacement references a missing track",
                )
            })?;
        let mut next = self.clone();
        next.tracks[index] = replacement;
        next.revision = next_revision(self.revision)?;
        Ok(next)
    }

    /// Removes one complete track.
    pub fn without_track(&self, id: TrackId) -> Result<Self> {
        let index = self
            .tracks
            .binary_search_by_key(&id, TrackingTrack::id)
            .map_err(|_| {
                tracking_error(
                    "remove_track",
                    "missing_track",
                    "tracking removal references a missing track",
                )
            })?;
        let mut next = self.clone();
        next.tracks.remove(index);
        next.revision = next_revision(self.revision)?;
        Ok(next)
    }

    /// Clears replaceable solver output while retaining selection and authored corrections.
    pub fn without_derived_samples(&self, id: TrackId) -> Result<Self> {
        let index = self
            .tracks
            .binary_search_by_key(&id, TrackingTrack::id)
            .map_err(|_| {
                tracking_error(
                    "clear_derived",
                    "missing_track",
                    "tracking derived-state clearing references a missing track",
                )
            })?;
        let mut next = self.clone();
        next.tracks[index].derived.clear();
        next.revision = next_revision(self.revision)?;
        Ok(next)
    }

    /// Resolves authored state above replaceable solver output.
    #[must_use]
    pub fn resolved_sample(&self, id: TrackId, frame: i64) -> Option<ResolvedTrackingSample<'_>> {
        let track = self.track(id)?;
        if track.reference.frame() == frame {
            return Some(ResolvedTrackingSample {
                source: TrackingSampleSource::Reference,
                sample: &track.reference,
            });
        }
        if let Ok(index) = track
            .corrections
            .binary_search_by_key(&frame, TrackingSample::frame)
        {
            return Some(ResolvedTrackingSample {
                source: TrackingSampleSource::ManualCorrection,
                sample: &track.corrections[index],
            });
        }
        track
            .derived
            .binary_search_by_key(&frame, TrackingSample::frame)
            .ok()
            .map(|index| ResolvedTrackingSample {
                source: TrackingSampleSource::Solver,
                sample: &track.derived[index],
            })
    }

    /// Creates a revision-fenced request from the nearest available coherent sample.
    pub fn solve_request(&self, id: TrackId, target_frame: i64) -> Result<TrackingRequest> {
        let track = self.track(id).ok_or_else(|| {
            tracking_error(
                "create_request",
                "missing_track",
                "tracking request references a missing track",
            )
        })?;
        if target_frame == track.reference.frame()
            || track
                .corrections
                .binary_search_by_key(&target_frame, TrackingSample::frame)
                .is_ok()
        {
            return Err(tracking_error(
                "create_request",
                "authored_target",
                "solver output cannot replace authored tracking state",
            ));
        }

        let source = std::iter::once(&track.reference)
            .chain(track.corrections.iter())
            .chain(track.derived.iter())
            .filter(|sample| sample.frame() != target_frame)
            .min_by(|left, right| compare_source_distance(left, right, target_frame))
            .ok_or_else(|| {
                tracking_error(
                    "create_request",
                    "missing_source",
                    "tracking request has no coherent source sample",
                )
            })?;
        Ok(TrackingRequest {
            source_revision: self.revision,
            track_id: id,
            selection: track.selection.clone(),
            source: source.clone(),
            target_frame,
        })
    }

    /// Inserts or replaces one manual correction and invalidates only its authored segment.
    pub fn with_correction(&self, id: TrackId, correction: TrackingSample) -> Result<Self> {
        let track_index = self
            .tracks
            .binary_search_by_key(&id, TrackingTrack::id)
            .map_err(|_| {
                tracking_error(
                    "set_correction",
                    "missing_track",
                    "tracking correction references a missing track",
                )
            })?;
        let track = &self.tracks[track_index];
        if correction.frame() == track.reference.frame() {
            return Err(tracking_error(
                "set_correction",
                "reference_overlap",
                "tracking correction cannot replace the reference frame",
            ));
        }
        track.selection.validate_sample(&correction)?;

        let correction_frame = correction.frame();
        let mut next = self.clone();
        let next_track = &mut next.tracks[track_index];
        match next_track
            .corrections
            .binary_search_by_key(&correction_frame, TrackingSample::frame)
        {
            Ok(index) => next_track.corrections[index] = correction,
            Err(index) => {
                if next_track.corrections.len() >= MAX_SAMPLES_PER_TRACK {
                    return Err(tracking_category_error(
                        ErrorCategory::ResourceExhausted,
                        "set_correction",
                        "sample_limit",
                        "tracking correction limit is exhausted",
                    ));
                }
                next_track.corrections.insert(index, correction);
            }
        }
        invalidate_authored_segment(next_track, correction_frame);
        next_track.validate()?;
        next.revision = next_revision(self.revision)?;
        Ok(next)
    }

    /// Removes one manual correction and invalidates the newly joined authored segment.
    pub fn without_correction(&self, id: TrackId, frame: i64) -> Result<Self> {
        let track_index = self
            .tracks
            .binary_search_by_key(&id, TrackingTrack::id)
            .map_err(|_| {
                tracking_error(
                    "remove_correction",
                    "missing_track",
                    "tracking correction removal references a missing track",
                )
            })?;
        let correction_index = self.tracks[track_index]
            .corrections
            .binary_search_by_key(&frame, TrackingSample::frame)
            .map_err(|_| {
                tracking_error(
                    "remove_correction",
                    "missing_correction",
                    "tracking correction does not exist at the requested frame",
                )
            })?;

        let mut next = self.clone();
        let next_track = &mut next.tracks[track_index];
        next_track.corrections.remove(correction_index);
        invalidate_authored_segment(next_track, frame);
        next_track.validate()?;
        next.revision = next_revision(self.revision)?;
        Ok(next)
    }

    /// Applies one complete solver result if its source state is still current.
    pub fn apply_solver_result(&self, result: TrackingResult) -> Result<Self> {
        if result.source_revision != self.revision {
            return Err(tracking_category_error(
                ErrorCategory::Conflict,
                "apply_result",
                "stale_revision",
                "tracking solver result targets a stale artifact revision",
            ));
        }
        let track_index = self
            .tracks
            .binary_search_by_key(&result.track_id, TrackingTrack::id)
            .map_err(|_| {
                tracking_error(
                    "apply_result",
                    "missing_track",
                    "tracking solver result references a missing track",
                )
            })?;
        let track = &self.tracks[track_index];
        let resolved = self
            .resolved_sample(result.track_id, result.source.frame())
            .ok_or_else(|| {
                tracking_category_error(
                    ErrorCategory::Conflict,
                    "apply_result",
                    "changed_source",
                    "tracking solver source sample is no longer available",
                )
            })?;
        if resolved.sample() != &result.source {
            return Err(tracking_category_error(
                ErrorCategory::Conflict,
                "apply_result",
                "changed_source",
                "tracking solver source sample changed after request creation",
            ));
        }
        if result.sample.frame() == track.reference.frame()
            || track
                .corrections
                .binary_search_by_key(&result.sample.frame(), TrackingSample::frame)
                .is_ok()
        {
            return Err(tracking_category_error(
                ErrorCategory::Conflict,
                "apply_result",
                "authored_target",
                "tracking solver result cannot overwrite authored state",
            ));
        }
        track.selection.validate_sample(&result.sample)?;

        let mut next = self.clone();
        let next_track = &mut next.tracks[track_index];
        match next_track
            .derived
            .binary_search_by_key(&result.sample.frame(), TrackingSample::frame)
        {
            Ok(index) => next_track.derived[index] = result.sample,
            Err(index) => {
                if next_track.derived.len() >= MAX_SAMPLES_PER_TRACK {
                    return Err(tracking_category_error(
                        ErrorCategory::ResourceExhausted,
                        "apply_result",
                        "sample_limit",
                        "tracking derived sample limit is exhausted",
                    ));
                }
                next_track.derived.insert(index, result.sample);
            }
        }
        next.revision = next_revision(self.revision)?;
        next_track.validate()?;
        Ok(next)
    }
}

fn invalidate_authored_segment(track: &mut TrackingTrack, edited_frame: i64) {
    let anchors = std::iter::once(track.reference.frame())
        .chain(track.corrections.iter().map(TrackingSample::frame))
        .filter(|frame| *frame != edited_frame);
    let mut lower = None;
    let mut upper = None;
    for frame in anchors {
        if frame < edited_frame && lower.map_or(true, |candidate| frame > candidate) {
            lower = Some(frame);
        } else if frame > edited_frame && upper.map_or(true, |candidate| frame < candidate) {
            upper = Some(frame);
        }
    }
    track.derived.retain(|sample| {
        lower.is_some_and(|frame| sample.frame() <= frame)
            || upper.is_some_and(|frame| sample.frame() >= frame)
    });
}

fn compare_source_distance(left: &TrackingSample, right: &TrackingSample, target: i64) -> Ordering {
    let left_distance = (i128::from(left.frame()) - i128::from(target)).abs();
    let right_distance = (i128::from(right.frame()) - i128::from(target)).abs();
    left_distance
        .cmp(&right_distance)
        .then_with(|| left.frame().cmp(&right.frame()))
}

/// Immutable solver request containing the exact selection and coherent source sample.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackingRequest {
    source_revision: u64,
    track_id: TrackId,
    selection: TrackingSelection,
    source: TrackingSample,
    target_frame: i64,
}

impl TrackingRequest {
    /// Returns the artifact revision captured by this request.
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }

    /// Returns the stable track identity.
    #[must_use]
    pub const fn track_id(&self) -> TrackId {
        self.track_id
    }

    /// Returns the complete editable selection.
    #[must_use]
    pub const fn selection(&self) -> &TrackingSelection {
        &self.selection
    }

    /// Returns the coherent source sample.
    #[must_use]
    pub const fn source(&self) -> &TrackingSample {
        &self.source
    }

    /// Returns the requested target frame coordinate.
    #[must_use]
    pub const fn target_frame(&self) -> i64 {
        self.target_frame
    }
}

/// Complete solver output fenced to one artifact revision and source sample.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackingResult {
    source_revision: u64,
    track_id: TrackId,
    source: TrackingSample,
    sample: TrackingSample,
}

impl TrackingResult {
    /// Creates a complete external solver result bound to one exact request.
    pub fn new(request: &TrackingRequest, sample: TrackingSample) -> Result<Self> {
        if sample.frame() != request.target_frame() {
            return Err(tracking_error(
                "create_result",
                "target_frame",
                "tracking solver result frame must match its request target",
            ));
        }
        sample.validate()?;
        request.selection().validate_sample(&sample)?;
        Ok(Self {
            source_revision: request.source_revision(),
            track_id: request.track_id(),
            source: request.source().clone(),
            sample,
        })
    }

    /// Returns the artifact revision captured by the request.
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }

    /// Returns the stable track identity.
    #[must_use]
    pub const fn track_id(&self) -> TrackId {
        self.track_id
    }

    /// Returns the exact coherent source sample.
    #[must_use]
    pub const fn source(&self) -> &TrackingSample {
        &self.source
    }

    /// Returns the complete target sample.
    #[must_use]
    pub const fn sample(&self) -> &TrackingSample {
        &self.sample
    }
}

/// Checked transient grayscale solver input at one exact artifact frame coordinate.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackingFrame {
    frame: i64,
    width: u32,
    height: u32,
    luma: Vec<f32>,
}

impl TrackingFrame {
    /// Creates a bounded dense row-major luma frame.
    pub fn new(frame: i64, width: u32, height: u32, luma: Vec<f32>) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(tracking_error(
                "create_frame",
                "empty_dimensions",
                "tracking frame dimensions must be nonzero",
            ));
        }
        let pixels = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or_else(|| {
                tracking_category_error(
                    ErrorCategory::ResourceExhausted,
                    "create_frame",
                    "dimension_overflow",
                    "tracking frame dimensions overflow the host address space",
                )
            })?;
        if pixels > MAX_TRACKING_FRAME_PIXELS {
            return Err(tracking_category_error(
                ErrorCategory::ResourceExhausted,
                "create_frame",
                "pixel_limit",
                "tracking frame exceeds its supported pixel count",
            ));
        }
        if luma.len() != pixels {
            return Err(tracking_error(
                "create_frame",
                "sample_count",
                "tracking frame luma count must match its dimensions",
            ));
        }
        if luma.iter().any(|sample| !sample.is_finite()) {
            return Err(tracking_error(
                "create_frame",
                "nonfinite_luma",
                "tracking frame luma samples must be finite",
            ));
        }
        Ok(Self {
            frame,
            width,
            height,
            luma,
        })
    }

    /// Returns the artifact-local frame coordinate.
    #[must_use]
    pub const fn frame(&self) -> i64 {
        self.frame
    }

    /// Returns the frame width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the frame height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns dense row-major luma samples.
    #[must_use]
    pub fn luma(&self) -> &[f32] {
        &self.luma
    }

    fn sample(&self, x: f64, y: f64) -> Option<f64> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x > f64::from(self.width - 1)
            || y > f64::from(self.height - 1)
        {
            return None;
        }
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.width as usize - 1);
        let y1 = (y0 + 1).min(self.height as usize - 1);
        let fx = x - x0 as f64;
        let fy = y - y0 as f64;
        let width = self.width as usize;
        let top = f64::from(self.luma[y0 * width + x0]) * (1.0 - fx)
            + f64::from(self.luma[y0 * width + x1]) * fx;
        let bottom = f64::from(self.luma[y1 * width + x0]) * (1.0 - fx)
            + f64::from(self.luma[y1 * width + x1]) * fx;
        Some(top * (1.0 - fy) + bottom * fy)
    }
}

/// Checked bounds and convergence policy for the CPU reference solver.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackingSolverOptions {
    patch_radius: u32,
    max_iterations: u32,
    minimum_eigenvalue: f64,
    convergence_epsilon: f64,
    maximum_displacement: f64,
    maximum_residual: f64,
}

impl TrackingSolverOptions {
    /// Creates a bounded local registration policy.
    pub fn new(
        patch_radius: u32,
        max_iterations: u32,
        minimum_eigenvalue: f64,
        convergence_epsilon: f64,
        maximum_displacement: f64,
        maximum_residual: f64,
    ) -> Result<Self> {
        if !(1..=16).contains(&patch_radius) || !(1..=100).contains(&max_iterations) {
            return Err(tracking_error(
                "create_solver_options",
                "work_bound",
                "tracking patch radius or iteration count exceeds its supported bound",
            ));
        }
        if !minimum_eigenvalue.is_finite()
            || minimum_eigenvalue <= 0.0
            || !convergence_epsilon.is_finite()
            || convergence_epsilon <= 0.0
            || !maximum_displacement.is_finite()
            || maximum_displacement <= 0.0
            || !maximum_residual.is_finite()
            || maximum_residual <= 0.0
        {
            return Err(tracking_error(
                "create_solver_options",
                "numeric_domain",
                "tracking solver numeric limits must be finite and positive",
            ));
        }
        Ok(Self {
            patch_radius,
            max_iterations,
            minimum_eigenvalue,
            convergence_epsilon,
            maximum_displacement,
            maximum_residual,
        })
    }

    /// Returns the local patch radius in pixels.
    #[must_use]
    pub const fn patch_radius(self) -> u32 {
        self.patch_radius
    }

    /// Returns the bounded iteration limit.
    #[must_use]
    pub const fn max_iterations(self) -> u32 {
        self.max_iterations
    }

    /// Returns the Tomasi-Kanade minimum texture eigenvalue.
    #[must_use]
    pub const fn minimum_eigenvalue(self) -> f64 {
        self.minimum_eigenvalue
    }

    /// Returns the local update convergence threshold in pixels.
    #[must_use]
    pub const fn convergence_epsilon(self) -> f64 {
        self.convergence_epsilon
    }

    /// Returns the maximum local displacement in pixels.
    #[must_use]
    pub const fn maximum_displacement(self) -> f64 {
        self.maximum_displacement
    }

    /// Returns the maximum accepted root-mean-square luma residual.
    #[must_use]
    pub const fn maximum_residual(self) -> f64 {
        self.maximum_residual
    }
}

impl Default for TrackingSolverOptions {
    fn default() -> Self {
        Self {
            patch_radius: 4,
            max_iterations: 30,
            minimum_eigenvalue: 1.0e-4,
            convergence_epsilon: 1.0e-4,
            maximum_displacement: 16.0,
            maximum_residual: 0.1,
        }
    }
}

/// Engine-neutral tracking solver seam.
pub trait TrackingSolver: Send + Sync {
    /// Solves one exact request from explicit source and target luma frames.
    fn solve(
        &self,
        request: &TrackingRequest,
        source: &TrackingFrame,
        target: &TrackingFrame,
    ) -> Result<TrackingResult>;
}

/// Deterministic bounded CPU reference implementation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpuTrackingSolver {
    options: TrackingSolverOptions,
}

impl CpuTrackingSolver {
    /// Creates a solver with an explicit checked policy.
    #[must_use]
    pub const fn new(options: TrackingSolverOptions) -> Self {
        Self { options }
    }

    /// Returns the active solver policy.
    #[must_use]
    pub const fn options(&self) -> TrackingSolverOptions {
        self.options
    }
}

impl TrackingSolver for CpuTrackingSolver {
    fn solve(
        &self,
        request: &TrackingRequest,
        source: &TrackingFrame,
        target: &TrackingFrame,
    ) -> Result<TrackingResult> {
        validate_frame_pair(request, source, target)?;
        let sample = match request.selection() {
            TrackingSelection::Point { feature } => {
                solve_point_request(request, source, target, *feature, self.options)?
            }
            TrackingSelection::Planar {
                region, features, ..
            } => solve_planar_request(request, source, target, *region, features, self.options)?,
            TrackingSelection::Object {
                region, features, ..
            } => solve_object_request(request, source, target, *region, features, self.options)?,
            TrackingSelection::Camera {
                intrinsics,
                landmarks,
                ..
            } => solve_camera_request(
                request,
                source,
                target,
                *intrinsics,
                landmarks,
                self.options,
            )?,
        };
        TrackingResult::new(request, sample)
    }
}

fn solve_point_request(
    request: &TrackingRequest,
    source_frame: &TrackingFrame,
    target_frame: &TrackingFrame,
    feature: TrackedFeature,
    options: TrackingSolverOptions,
) -> Result<TrackingSample> {
    let TrackingModel::Point { position } = request.source().model() else {
        return Err(model_mismatch_error("solve_point"));
    };
    if request.source().observations()[0].feature_id() != feature.id() {
        return Err(tracking_error(
            "solve_point",
            "source_observation_identity",
            "point solver source observation does not match its selection",
        ));
    }
    let solved = track_point(source_frame, target_frame, *position, options)?;
    TrackingSample::new(
        request.target_frame(),
        TrackingModel::Point {
            position: solved.position,
        },
        [TrackingObservation::new(
            feature.id(),
            solved.position,
            solved.confidence,
        )?],
    )
}

fn solve_planar_request(
    request: &TrackingRequest,
    source_frame: &TrackingFrame,
    target_frame: &TrackingFrame,
    reference_region: TrackingRect,
    features: &[TrackedFeature],
    options: TrackingSolverOptions,
) -> Result<TrackingSample> {
    let TrackingModel::Planar {
        homography: source_homography,
        ..
    } = request.source().model()
    else {
        return Err(model_mismatch_error("solve_planar"));
    };
    if features.len() < 4 {
        return Err(tracking_error(
            "solve_planar",
            "feature_count",
            "planar solver requires at least four feature observations",
        ));
    }
    let observations = track_observations(
        request.source().observations(),
        source_frame,
        target_frame,
        options,
    )?;
    let incremental = fit_homography(request.source().observations(), &observations)?;
    let homography = incremental.checked_mul(*source_homography)?;
    let region = reference_region.transformed_bounds(homography)?;
    TrackingSample::new(
        request.target_frame(),
        TrackingModel::Planar { homography, region },
        observations,
    )
}

fn solve_object_request(
    request: &TrackingRequest,
    source_frame: &TrackingFrame,
    target_frame: &TrackingFrame,
    reference_region: TrackingRect,
    features: &[TrackedFeature],
    options: TrackingSolverOptions,
) -> Result<TrackingSample> {
    let TrackingModel::Object {
        transform: source_transform,
        ..
    } = request.source().model()
    else {
        return Err(model_mismatch_error("solve_object"));
    };
    if features.len() < 2 {
        return Err(tracking_error(
            "solve_object",
            "feature_count",
            "object solver requires at least two feature observations",
        ));
    }
    let observations = track_observations(
        request.source().observations(),
        source_frame,
        target_frame,
        options,
    )?;
    let incremental = fit_similarity(request.source().observations(), &observations)?;
    let transform = incremental.checked_mul(*source_transform)?;
    let region = reference_region.transformed_bounds(transform)?;
    TrackingSample::new(
        request.target_frame(),
        TrackingModel::Object { transform, region },
        observations,
    )
}

fn solve_camera_request(
    request: &TrackingRequest,
    source_frame: &TrackingFrame,
    target_frame: &TrackingFrame,
    intrinsics: CameraIntrinsics,
    landmarks: &[CameraLandmark],
    options: TrackingSolverOptions,
) -> Result<TrackingSample> {
    let TrackingModel::Camera { pose: source_pose } = request.source().model() else {
        return Err(model_mismatch_error("solve_camera"));
    };
    let observations = track_observations(
        request.source().observations(),
        source_frame,
        target_frame,
        options,
    )?;
    let pose = refine_camera_pose(
        intrinsics,
        landmarks,
        &observations,
        *source_pose,
        options.max_iterations,
    )?;
    TrackingSample::new(
        request.target_frame(),
        TrackingModel::Camera { pose },
        observations,
    )
}

fn model_mismatch_error(operation: &'static str) -> Error {
    tracking_error(
        operation,
        "source_model_kind",
        "tracking solver source model does not match its selection",
    )
}

fn track_observations(
    source_observations: &[TrackingObservation],
    source_frame: &TrackingFrame,
    target_frame: &TrackingFrame,
    options: TrackingSolverOptions,
) -> Result<Vec<TrackingObservation>> {
    source_observations
        .iter()
        .map(|observation| {
            let solved = track_point(source_frame, target_frame, observation.position(), options)?;
            TrackingObservation::new(observation.feature_id(), solved.position, solved.confidence)
        })
        .collect()
}

fn fit_homography(
    source: &[TrackingObservation],
    target: &[TrackingObservation],
) -> Result<TrackingMatrix3> {
    validate_correspondences(source, target, 4, "solve_planar")?;
    let all = (0..source.len()).collect::<Vec<_>>();
    let mut best = None::<(Vec<usize>, f64, TrackingMatrix3)>;
    if let Ok(model) = fit_homography_indices(source, target, &all) {
        consider_planar_candidate(source, target, model, &mut best)?;
    }
    let mut candidate_count = 0;
    'candidates: for first in 0..source.len().saturating_sub(3) {
        for second in first + 1..source.len().saturating_sub(2) {
            for third in second + 1..source.len().saturating_sub(1) {
                for fourth in third + 1..source.len() {
                    if candidate_count == MAX_PLANAR_MODEL_CANDIDATES {
                        break 'candidates;
                    }
                    candidate_count += 1;
                    let indices = [first, second, third, fourth];
                    if let Ok(model) = fit_homography_indices(source, target, &indices) {
                        consider_planar_candidate(source, target, model, &mut best)?;
                    }
                }
            }
        }
    }
    let Some((inliers, _, candidate)) = best else {
        return Err(tracking_error(
            "solve_planar",
            "residual_consensus",
            "planar observations do not support one coherent homography",
        ));
    };
    let fitted = if inliers.len() == source.len() {
        candidate
    } else {
        fit_homography_indices(source, target, &inliers)?
    };
    validate_model_residual(fitted, source, target, &inliers, "solve_planar")?;
    Ok(fitted)
}

fn consider_planar_candidate(
    source: &[TrackingObservation],
    target: &[TrackingObservation],
    model: TrackingMatrix3,
    best: &mut Option<(Vec<usize>, f64, TrackingMatrix3)>,
) -> Result<()> {
    let mut inliers = Vec::with_capacity(source.len());
    let mut total_error = 0.0;
    for index in 0..source.len() {
        let error = model_error(model, source[index].position(), target[index].position())?;
        if error <= CONSENSUS_RESIDUAL_LIMIT {
            inliers.push(index);
            total_error += error;
        }
    }
    if inliers.len() < 4 {
        return Ok(());
    }
    let replace = best.as_ref().map_or(true, |(best_inliers, best_error, _)| {
        inliers.len() > best_inliers.len()
            || (inliers.len() == best_inliers.len() && total_error < *best_error)
    });
    if replace {
        *best = Some((inliers, total_error, model));
    }
    Ok(())
}

fn fit_homography_indices(
    source: &[TrackingObservation],
    target: &[TrackingObservation],
    indices: &[usize],
) -> Result<TrackingMatrix3> {
    let source_points = indices
        .iter()
        .map(|index| source[*index].position())
        .collect::<Vec<_>>();
    let target_points = indices
        .iter()
        .map(|index| target[*index].position())
        .collect::<Vec<_>>();
    let source_normalization = point_normalization(&source_points, "solve_planar")?;
    let target_normalization = point_normalization(&target_points, "solve_planar")?;
    let mut normal = [[0.0; 8]; 8];
    let mut right = [0.0; 8];
    for (source_point, target_point) in source_points.iter().zip(&target_points) {
        let [x, y] = source_normalization.apply(*source_point);
        let [u, v] = target_normalization.apply(*target_point);
        accumulate_least_squares(
            &mut normal,
            &mut right,
            [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y],
            u,
        );
        accumulate_least_squares(
            &mut normal,
            &mut right,
            [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y],
            v,
        );
    }
    let solved = solve_linear(normal, right).ok_or_else(|| {
        tracking_error(
            "solve_planar",
            "singular_homography",
            "planar feature geometry produces a singular homography system",
        )
    })?;
    let normalized = [
        solved[0], solved[1], solved[2], solved[3], solved[4], solved[5], solved[6], solved[7], 1.0,
    ];
    let denormalized = mat3_mul(
        target_normalization.inverse_matrix(),
        mat3_mul(normalized, source_normalization.matrix()),
    );
    if denormalized[8].abs() <= 1.0e-12 {
        return Err(tracking_error(
            "solve_planar",
            "homography_scale",
            "planar homography has an invalid projective scale",
        ));
    }
    let scale = denormalized[8];
    TrackingMatrix3::new(denormalized.map(|value| value / scale))
}

#[derive(Clone, Copy)]
struct PointNormalization {
    scale: f64,
    center: [f64; 2],
}

impl PointNormalization {
    fn apply(self, point: TrackingPoint) -> [f64; 2] {
        [
            self.scale * (point.x() - self.center[0]),
            self.scale * (point.y() - self.center[1]),
        ]
    }

    fn matrix(self) -> [f64; 9] {
        [
            self.scale,
            0.0,
            -self.scale * self.center[0],
            0.0,
            self.scale,
            -self.scale * self.center[1],
            0.0,
            0.0,
            1.0,
        ]
    }

    fn inverse_matrix(self) -> [f64; 9] {
        [
            1.0 / self.scale,
            0.0,
            self.center[0],
            0.0,
            1.0 / self.scale,
            self.center[1],
            0.0,
            0.0,
            1.0,
        ]
    }
}

fn point_normalization(
    points: &[TrackingPoint],
    operation: &'static str,
) -> Result<PointNormalization> {
    let count = points.len() as f64;
    let center = [
        points.iter().map(|point| point.x()).sum::<f64>() / count,
        points.iter().map(|point| point.y()).sum::<f64>() / count,
    ];
    let mean_distance = points
        .iter()
        .map(|point| (point.x() - center[0]).hypot(point.y() - center[1]))
        .sum::<f64>()
        / count;
    if !mean_distance.is_finite() || mean_distance <= 1.0e-9 {
        return Err(tracking_error(
            operation,
            "feature_spread",
            "tracking model features do not span a usable image region",
        ));
    }
    Ok(PointNormalization {
        scale: 2.0_f64.sqrt() / mean_distance,
        center,
    })
}

fn fit_similarity(
    source: &[TrackingObservation],
    target: &[TrackingObservation],
) -> Result<TrackingMatrix3> {
    validate_correspondences(source, target, 2, "solve_object")?;
    let mut inliers = (0..source.len()).collect::<Vec<_>>();
    let first = fit_similarity_indices(source, target, &inliers)?;
    inliers.retain(|index| {
        model_error(first, source[*index].position(), target[*index].position())
            .is_ok_and(|error| error <= MODEL_RESIDUAL_LIMIT)
    });
    if inliers.len() < 2 {
        return Err(tracking_error(
            "solve_object",
            "residual_consensus",
            "object observations do not support one coherent similarity transform",
        ));
    }
    let fitted = if inliers.len() == source.len() {
        first
    } else {
        fit_similarity_indices(source, target, &inliers)?
    };
    validate_model_residual(fitted, source, target, &inliers, "solve_object")?;
    Ok(fitted)
}

fn fit_similarity_indices(
    source: &[TrackingObservation],
    target: &[TrackingObservation],
    indices: &[usize],
) -> Result<TrackingMatrix3> {
    let count = indices.len() as f64;
    let source_center = [
        indices
            .iter()
            .map(|index| source[*index].position().x())
            .sum::<f64>()
            / count,
        indices
            .iter()
            .map(|index| source[*index].position().y())
            .sum::<f64>()
            / count,
    ];
    let target_center = [
        indices
            .iter()
            .map(|index| target[*index].position().x())
            .sum::<f64>()
            / count,
        indices
            .iter()
            .map(|index| target[*index].position().y())
            .sum::<f64>()
            / count,
    ];
    let mut dot = 0.0;
    let mut cross = 0.0;
    let mut spread = 0.0;
    for index in indices {
        let source_x = source[*index].position().x() - source_center[0];
        let source_y = source[*index].position().y() - source_center[1];
        let target_x = target[*index].position().x() - target_center[0];
        let target_y = target[*index].position().y() - target_center[1];
        dot += source_x * target_x + source_y * target_y;
        cross += source_x * target_y - source_y * target_x;
        spread += source_x * source_x + source_y * source_y;
    }
    if !spread.is_finite() || spread <= 1.0e-9 {
        return Err(tracking_error(
            "solve_object",
            "feature_spread",
            "object features do not span a usable image region",
        ));
    }
    let a = dot / spread;
    let b = cross / spread;
    let translation_x = target_center[0] - a * source_center[0] + b * source_center[1];
    let translation_y = target_center[1] - b * source_center[0] - a * source_center[1];
    TrackingMatrix3::new([a, -b, translation_x, b, a, translation_y, 0.0, 0.0, 1.0])
}

fn validate_correspondences(
    source: &[TrackingObservation],
    target: &[TrackingObservation],
    minimum: usize,
    operation: &'static str,
) -> Result<()> {
    if source.len() < minimum || source.len() != target.len() {
        return Err(tracking_error(
            operation,
            "correspondence_count",
            "tracking model requires a complete bounded correspondence set",
        ));
    }
    if source
        .iter()
        .zip(target)
        .any(|(left, right)| left.feature_id() != right.feature_id())
    {
        return Err(tracking_error(
            operation,
            "correspondence_identity",
            "tracking model correspondence identities must match",
        ));
    }
    Ok(())
}

fn model_error(
    model: TrackingMatrix3,
    source: TrackingPoint,
    target: TrackingPoint,
) -> Result<f64> {
    let projected = model.transform_point(source)?;
    Ok((projected.x() - target.x()).hypot(projected.y() - target.y()))
}

fn validate_model_residual(
    model: TrackingMatrix3,
    source: &[TrackingObservation],
    target: &[TrackingObservation],
    indices: &[usize],
    operation: &'static str,
) -> Result<()> {
    let squared = indices.iter().try_fold(0.0, |sum, index| {
        model_error(model, source[*index].position(), target[*index].position())
            .map(|error| sum + error * error)
    })?;
    let residual = (squared / indices.len() as f64).sqrt();
    if !residual.is_finite() || residual > MODEL_RESIDUAL_LIMIT {
        return Err(tracking_error(
            operation,
            "model_residual",
            "tracking model residual exceeds its deterministic acceptance bound",
        ));
    }
    Ok(())
}

fn refine_camera_pose(
    intrinsics: CameraIntrinsics,
    landmarks: &[CameraLandmark],
    observations: &[TrackingObservation],
    initial_pose: CameraPose,
    max_iterations: u32,
) -> Result<CameraPose> {
    if landmarks.len() != observations.len()
        || landmarks
            .iter()
            .zip(observations)
            .any(|(landmark, observation)| landmark.id() != observation.feature_id())
    {
        return Err(tracking_error(
            "solve_camera",
            "correspondence_identity",
            "camera landmarks must match every tracked observation",
        ));
    }
    let mut parameters = initial_pose.parameters();
    for _ in 0..max_iterations {
        let mut normal = [[0.0; 6]; 6];
        let mut right = [0.0; 6];
        for (landmark, observation) in landmarks.iter().zip(observations) {
            let projected = project_camera(intrinsics, parameters, landmark.world())?;
            let residual = [
                projected.x() - observation.position().x(),
                projected.y() - observation.position().y(),
            ];
            let mut jacobian = [[0.0; 6]; 2];
            for parameter in 0..6 {
                let mut perturbed = parameters;
                let epsilon = if parameter < 3 { 1.0e-6 } else { 1.0e-5 };
                perturbed[parameter] += epsilon;
                let shifted = project_camera(intrinsics, perturbed, landmark.world())?;
                jacobian[0][parameter] = (shifted.x() - projected.x()) / epsilon;
                jacobian[1][parameter] = (shifted.y() - projected.y()) / epsilon;
            }
            for row in 0..6 {
                right[row] -= jacobian[0][row] * residual[0] + jacobian[1][row] * residual[1];
                for column in 0..6 {
                    normal[row][column] += jacobian[0][row] * jacobian[0][column]
                        + jacobian[1][row] * jacobian[1][column];
                }
            }
        }
        for (index, row) in normal.iter_mut().enumerate() {
            row[index] += 1.0e-8;
        }
        let update = solve_linear(normal, right).ok_or_else(|| {
            tracking_error(
                "solve_camera",
                "singular_pose",
                "camera landmark geometry produces a singular pose system",
            )
        })?;
        for (parameter, delta) in parameters.iter_mut().zip(update) {
            *parameter += delta;
        }
        if update.iter().map(|value| value * value).sum::<f64>().sqrt() <= 1.0e-8 {
            break;
        }
    }
    let squared_residual =
        landmarks
            .iter()
            .zip(observations)
            .try_fold(0.0, |sum, (landmark, observation)| {
                let projected = project_camera(intrinsics, parameters, landmark.world())?;
                Ok::<_, Error>(
                    sum + (projected.x() - observation.position().x()).powi(2)
                        + (projected.y() - observation.position().y()).powi(2),
                )
            })?;
    let residual = (squared_residual / landmarks.len() as f64).sqrt();
    if !residual.is_finite() || residual > MODEL_RESIDUAL_LIMIT {
        return Err(tracking_error(
            "solve_camera",
            "reprojection_residual",
            "camera pose reprojection residual exceeds its acceptance bound",
        ));
    }
    CameraPose::from_parameters(parameters)
}

fn project_camera(
    intrinsics: CameraIntrinsics,
    parameters: [f64; 6],
    world: TrackingPoint3,
) -> Result<TrackingPoint> {
    let rotation = rodrigues([parameters[0], parameters[1], parameters[2]]);
    let world = world.values();
    let camera = [
        dot3(rotation[0], world) + parameters[3],
        dot3(rotation[1], world) + parameters[4],
        dot3(rotation[2], world) + parameters[5],
    ];
    if !camera.iter().all(|value| value.is_finite()) || camera[2] <= 1.0e-6 {
        return Err(tracking_error(
            "solve_camera",
            "landmark_depth",
            "camera pose places a known landmark behind the camera",
        ));
    }
    TrackingPoint::new(
        intrinsics.focal_x() * camera[0] / camera[2] + intrinsics.principal_x(),
        intrinsics.focal_y() * camera[1] / camera[2] + intrinsics.principal_y(),
    )
}

fn rodrigues(vector: [f64; 3]) -> [[f64; 3]; 3] {
    let angle = dot3(vector, vector).sqrt();
    if angle <= 1.0e-12 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let axis = [vector[0] / angle, vector[1] / angle, vector[2] / angle];
    let cosine = angle.cos();
    let sine = angle.sin();
    let complement = 1.0 - cosine;
    [
        [
            cosine + axis[0] * axis[0] * complement,
            axis[0] * axis[1] * complement - axis[2] * sine,
            axis[0] * axis[2] * complement + axis[1] * sine,
        ],
        [
            axis[1] * axis[0] * complement + axis[2] * sine,
            cosine + axis[1] * axis[1] * complement,
            axis[1] * axis[2] * complement - axis[0] * sine,
        ],
        [
            axis[2] * axis[0] * complement - axis[1] * sine,
            axis[2] * axis[1] * complement + axis[0] * sine,
            cosine + axis[2] * axis[2] * complement,
        ],
    ]
}

fn accumulate_least_squares<const N: usize>(
    normal: &mut [[f64; N]; N],
    right: &mut [f64; N],
    row: [f64; N],
    value: f64,
) {
    for outer in 0..N {
        right[outer] += row[outer] * value;
        for inner in 0..N {
            normal[outer][inner] += row[outer] * row[inner];
        }
    }
}

fn solve_linear<const N: usize>(
    mut matrix: [[f64; N]; N],
    mut right: [f64; N],
) -> Option<[f64; N]> {
    for column in 0..N {
        let pivot = (column..N).max_by(|left, right_index| {
            matrix[*left][column]
                .abs()
                .partial_cmp(&matrix[*right_index][column].abs())
                .unwrap_or(Ordering::Equal)
        })?;
        if !matrix[pivot][column].is_finite() || matrix[pivot][column].abs() <= 1.0e-12 {
            return None;
        }
        if pivot != column {
            matrix.swap(pivot, column);
            right.swap(pivot, column);
        }
        let divisor = matrix[column][column];
        for value in &mut matrix[column][column..] {
            *value /= divisor;
        }
        right[column] /= divisor;
        let pivot_row = matrix[column];
        for row in 0..N {
            if row == column {
                continue;
            }
            let factor = matrix[row][column];
            for (value, pivot_value) in matrix[row][column..].iter_mut().zip(&pivot_row[column..]) {
                *value -= factor * pivot_value;
            }
            right[row] -= factor * right[column];
        }
    }
    right.iter().all(|value| value.is_finite()).then_some(right)
}

fn mat3_mul(left: [f64; 9], right: [f64; 9]) -> [f64; 9] {
    let mut output = [0.0; 9];
    for row in 0..3 {
        for column in 0..3 {
            output[row * 3 + column] = (0..3)
                .map(|inner| left[row * 3 + inner] * right[inner * 3 + column])
                .sum();
        }
    }
    output
}

fn subtract3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn validate_frame_pair(
    request: &TrackingRequest,
    source: &TrackingFrame,
    target: &TrackingFrame,
) -> Result<()> {
    if source.frame() != request.source().frame() || target.frame() != request.target_frame() {
        return Err(tracking_error(
            "solve",
            "frame_identity",
            "tracking frame coordinates must match the solver request",
        ));
    }
    if source.width() != target.width() || source.height() != target.height() {
        return Err(tracking_error(
            "solve",
            "frame_dimensions",
            "tracking source and target dimensions must match",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct PointSolve {
    position: TrackingPoint,
    confidence: f64,
}

fn track_point(
    source: &TrackingFrame,
    target: &TrackingFrame,
    position: TrackingPoint,
    options: TrackingSolverOptions,
) -> Result<PointSolve> {
    let radius = i32::try_from(options.patch_radius).expect("patch radius is bounded to 16");
    let mut hessian = [[0.0; 2]; 2];
    let mut samples = Vec::with_capacity(((radius * 2 + 1).pow(2)) as usize);
    for offset_y in -radius..=radius {
        for offset_x in -radius..=radius {
            let x = position.x() + f64::from(offset_x);
            let y = position.y() + f64::from(offset_y);
            let value = source.sample(x, y).ok_or_else(|| {
                tracking_error(
                    "solve_point",
                    "source_border",
                    "point tracking patch extends beyond the source frame",
                )
            })?;
            let gradient_x = (source.sample(x + 1.0, y).ok_or_else(|| {
                tracking_error(
                    "solve_point",
                    "source_border",
                    "point tracking gradient extends beyond the source frame",
                )
            })? - source.sample(x - 1.0, y).ok_or_else(|| {
                tracking_error(
                    "solve_point",
                    "source_border",
                    "point tracking gradient extends beyond the source frame",
                )
            })?) * 0.5;
            let gradient_y = (source.sample(x, y + 1.0).ok_or_else(|| {
                tracking_error(
                    "solve_point",
                    "source_border",
                    "point tracking gradient extends beyond the source frame",
                )
            })? - source.sample(x, y - 1.0).ok_or_else(|| {
                tracking_error(
                    "solve_point",
                    "source_border",
                    "point tracking gradient extends beyond the source frame",
                )
            })?) * 0.5;
            hessian[0][0] += gradient_x * gradient_x;
            hessian[0][1] += gradient_x * gradient_y;
            hessian[1][1] += gradient_y * gradient_y;
            samples.push((offset_x, offset_y, value, gradient_x, gradient_y));
        }
    }
    hessian[1][0] = hessian[0][1];
    let trace = hessian[0][0] + hessian[1][1];
    let discriminant =
        ((hessian[0][0] - hessian[1][1]).powi(2) + 4.0 * hessian[0][1].powi(2)).sqrt();
    let minimum_eigenvalue = 0.5 * (trace - discriminant);
    if !minimum_eigenvalue.is_finite() || minimum_eigenvalue < options.minimum_eigenvalue {
        return Err(tracking_error(
            "solve_point",
            "texture_degeneracy",
            "point tracking patch does not contain two-dimensional texture",
        ));
    }
    let determinant = hessian[0][0] * hessian[1][1] - hessian[0][1] * hessian[1][0];
    if determinant.abs() <= f64::EPSILON {
        return Err(tracking_error(
            "solve_point",
            "singular_hessian",
            "point tracking gradient system is singular",
        ));
    }

    let mut solved_x = position.x();
    let mut solved_y = position.y();
    for _ in 0..options.max_iterations {
        let mut right = [0.0; 2];
        for (offset_x, offset_y, source_value, gradient_x, gradient_y) in &samples {
            let target_value = target
                .sample(
                    solved_x + f64::from(*offset_x),
                    solved_y + f64::from(*offset_y),
                )
                .ok_or_else(|| {
                    tracking_error(
                        "solve_point",
                        "target_border",
                        "point tracking patch extends beyond the target frame",
                    )
                })?;
            let residual = target_value - source_value;
            right[0] -= gradient_x * residual;
            right[1] -= gradient_y * residual;
        }
        let delta_x = (right[0] * hessian[1][1] - hessian[0][1] * right[1]) / determinant;
        let delta_y = (hessian[0][0] * right[1] - right[0] * hessian[1][0]) / determinant;
        if !delta_x.is_finite() || !delta_y.is_finite() {
            return Err(tracking_error(
                "solve_point",
                "nonfinite_update",
                "point tracking produced a nonfinite registration update",
            ));
        }
        solved_x += delta_x;
        solved_y += delta_y;
        let displacement = (solved_x - position.x()).hypot(solved_y - position.y());
        if displacement > options.maximum_displacement {
            return Err(tracking_error(
                "solve_point",
                "displacement_limit",
                "point tracking exceeded its configured displacement bound",
            ));
        }
        if delta_x.hypot(delta_y) <= options.convergence_epsilon {
            break;
        }
    }

    let mut squared_residual = 0.0;
    for (offset_x, offset_y, source_value, _, _) in &samples {
        let target_value = target
            .sample(
                solved_x + f64::from(*offset_x),
                solved_y + f64::from(*offset_y),
            )
            .ok_or_else(|| {
                tracking_error(
                    "solve_point",
                    "target_border",
                    "point tracking result extends beyond the target frame",
                )
            })?;
        squared_residual += (target_value - source_value).powi(2);
    }
    let residual = (squared_residual / samples.len() as f64).sqrt();
    if !residual.is_finite() || residual > options.maximum_residual {
        return Err(tracking_error(
            "solve_point",
            "residual_limit",
            "point tracking residual exceeds its configured acceptance bound",
        ));
    }
    let confidence =
        (minimum_eigenvalue / (minimum_eigenvalue + residual * residual)).clamp(0.0, 1.0);
    Ok(PointSolve {
        position: TrackingPoint::new(solved_x, solved_y)?,
        confidence,
    })
}

fn next_revision(revision: u64) -> Result<u64> {
    revision.checked_add(1).ok_or_else(|| {
        tracking_category_error(
            ErrorCategory::ResourceExhausted,
            "advance_revision",
            "revision_exhausted",
            "tracking artifact revision space is exhausted",
        )
    })
}

fn parse_canonical_revision(value: &str) -> std::result::Result<u64, &'static str> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("tracking revision must be a canonical unsigned decimal integer");
    }
    value
        .parse()
        .map_err(|_| "tracking revision exceeds the supported integer range")
}

fn tracking_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    tracking_category_error(ErrorCategory::InvalidInput, operation, reason, message)
}

fn tracking_category_error(
    category: ErrorCategory,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
