//! Shared two-dimensional geometry and image-space bounds.
//!
//! Continuous coordinates use finite `f64` values. Pixel bounds use signed,
//! half-open integer edges so regions of interest may extend outside a source
//! image without changing their meaning. Geometry carries only spatial extent
//! and transform intent. Channel order, alpha association, pixel format, and
//! color space remain separate explicit image contracts.
//!
//! Points and vectors are intentionally distinct:
//!
//! ```compile_fail
//! use superi_core::geometry::{Point2, Vector2};
//!
//! fn place_at(_point: Point2) {}
//!
//! place_at(Vector2::new(1.0, 2.0).unwrap());
//! ```

use crate::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

/// A finite location in a two-dimensional continuous coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point2 {
    x: f64,
    y: f64,
}

impl Point2 {
    /// The coordinate-space origin.
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0 };

    /// Creates a point from finite coordinates.
    pub fn new(x: f64, y: f64) -> Result<Self> {
        ensure_finite_pair(x, y, "create_point")?;
        Ok(Self { x, y })
    }

    /// Returns the horizontal coordinate.
    #[must_use]
    pub const fn x(self) -> f64 {
        self.x
    }

    /// Returns the vertical coordinate.
    #[must_use]
    pub const fn y(self) -> f64 {
        self.y
    }

    /// Returns this point displaced by `vector`.
    pub fn checked_offset(self, vector: Vector2) -> Result<Self> {
        let x = self.x + vector.x;
        let y = self.y + vector.y;
        ensure_finite_pair(x, y, "offset_point")?;
        Ok(Self { x, y })
    }

    /// Returns the displacement from this point to `other`.
    pub fn checked_vector_to(self, other: Self) -> Result<Vector2> {
        let x = other.x - self.x;
        let y = other.y - self.y;
        ensure_finite_pair(x, y, "subtract_points")?;
        Ok(Vector2 { x, y })
    }
}

/// A finite displacement in a two-dimensional continuous coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vector2 {
    x: f64,
    y: f64,
}

impl Vector2 {
    /// A displacement with no magnitude.
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    /// Creates a vector from finite components.
    pub fn new(x: f64, y: f64) -> Result<Self> {
        ensure_finite_pair(x, y, "create_vector")?;
        Ok(Self { x, y })
    }

    /// Returns the horizontal component.
    #[must_use]
    pub const fn x(self) -> f64 {
        self.x
    }

    /// Returns the vertical component.
    #[must_use]
    pub const fn y(self) -> f64 {
        self.y
    }

    /// Adds two displacements.
    pub fn checked_add(self, other: Self) -> Result<Self> {
        let x = self.x + other.x;
        let y = self.y + other.y;
        ensure_finite_pair(x, y, "add_vectors")?;
        Ok(Self { x, y })
    }

    /// Subtracts `other` from this displacement.
    pub fn checked_sub(self, other: Self) -> Result<Self> {
        let x = self.x - other.x;
        let y = self.y - other.y;
        ensure_finite_pair(x, y, "subtract_vectors")?;
        Ok(Self { x, y })
    }

    /// Multiplies both components by a finite scalar.
    pub fn checked_scale(self, scalar: f64) -> Result<Self> {
        ensure_finite(scalar, "scale_vector")?;
        let x = self.x * scalar;
        let y = self.y * scalar;
        ensure_finite_pair(x, y, "scale_vector")?;
        Ok(Self { x, y })
    }
}

/// A finite 3 by 3 homogeneous transform for two-dimensional points.
///
/// Values are stored in row-major order and multiply column vectors. Translation
/// therefore occupies the final column. [`Matrix3::checked_mul`] is mathematical
/// matrix multiplication. [`Matrix3::checked_then`] names application order and
/// returns a transform that applies `self` first and its argument second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix3 {
    rows: [[f64; 3]; 3],
}

impl Matrix3 {
    /// The identity transform.
    pub const IDENTITY: Self = Self {
        rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    /// Creates a matrix from finite row-major values.
    pub fn from_rows(rows: [[f64; 3]; 3]) -> Result<Self> {
        for row in rows {
            for value in row {
                ensure_finite(value, "create_matrix")?;
            }
        }
        Ok(Self { rows })
    }

    /// Returns all values in row-major order.
    #[must_use]
    pub const fn rows(self) -> [[f64; 3]; 3] {
        self.rows
    }

    /// Creates an affine translation.
    #[must_use]
    pub const fn translation(displacement: Vector2) -> Self {
        Self {
            rows: [
                [1.0, 0.0, displacement.x],
                [0.0, 1.0, displacement.y],
                [0.0, 0.0, 1.0],
            ],
        }
    }

    /// Creates an affine scale about the coordinate-space origin.
    #[must_use]
    pub const fn scale(factors: Vector2) -> Self {
        Self {
            rows: [
                [factors.x, 0.0, 0.0],
                [0.0, factors.y, 0.0],
                [0.0, 0.0, 1.0],
            ],
        }
    }

    /// Returns the checked mathematical product `self * right`.
    pub fn checked_mul(self, right: Self) -> Result<Self> {
        let left = self.rows;
        let right = right.rows;
        let rows = [
            [
                dot3(left[0], [right[0][0], right[1][0], right[2][0]]),
                dot3(left[0], [right[0][1], right[1][1], right[2][1]]),
                dot3(left[0], [right[0][2], right[1][2], right[2][2]]),
            ],
            [
                dot3(left[1], [right[0][0], right[1][0], right[2][0]]),
                dot3(left[1], [right[0][1], right[1][1], right[2][1]]),
                dot3(left[1], [right[0][2], right[1][2], right[2][2]]),
            ],
            [
                dot3(left[2], [right[0][0], right[1][0], right[2][0]]),
                dot3(left[2], [right[0][1], right[1][1], right[2][1]]),
                dot3(left[2], [right[0][2], right[1][2], right[2][2]]),
            ],
        ];
        Self::from_rows_with_operation(rows, "multiply_matrices")
    }

    /// Composes two transforms in evaluation order.
    ///
    /// The returned matrix applies `self` first and `next` second.
    pub fn checked_then(self, next: Self) -> Result<Self> {
        next.checked_mul(self)
    }

    /// Maps a point through this homogeneous transform.
    ///
    /// A point on the projective horizon, or any non-finite result, is rejected.
    pub fn checked_transform_point(self, point: Point2) -> Result<Point2> {
        let x = dot3(self.rows[0], [point.x, point.y, 1.0]);
        let y = dot3(self.rows[1], [point.x, point.y, 1.0]);
        let w = self.homogeneous_w(point);
        ensure_finite_pair(x, y, "transform_point")?;
        ensure_finite(w, "transform_point")?;
        if w == 0.0 {
            return Err(invalid_geometry(
                "transform_point",
                "point lies on the transform's projective horizon",
            ));
        }
        let x = x / w;
        let y = y / w;
        ensure_finite_pair(x, y, "transform_point")?;
        Ok(Point2 { x, y })
    }

    /// Returns the inverse of an invertible matrix.
    pub fn checked_inverse(self) -> Result<Self> {
        let m = self.rows;
        let c00 = m[1][1] * m[2][2] - m[1][2] * m[2][1];
        let c01 = m[1][2] * m[2][0] - m[1][0] * m[2][2];
        let c02 = m[1][0] * m[2][1] - m[1][1] * m[2][0];
        let determinant = m[0][0] * c00 + m[0][1] * c01 + m[0][2] * c02;
        ensure_finite(determinant, "invert_matrix")?;
        if determinant == 0.0 {
            return Err(invalid_geometry(
                "invert_matrix",
                "matrix is not invertible",
            ));
        }

        let reciprocal = 1.0 / determinant;
        ensure_finite(reciprocal, "invert_matrix")?;
        let rows = [
            [
                c00 * reciprocal,
                (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * reciprocal,
                (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * reciprocal,
            ],
            [
                c01 * reciprocal,
                (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * reciprocal,
                (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * reciprocal,
            ],
            [
                c02 * reciprocal,
                (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * reciprocal,
                (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * reciprocal,
            ],
        ];
        Self::from_rows_with_operation(rows, "invert_matrix")
    }

    fn from_rows_with_operation(rows: [[f64; 3]; 3], operation: &'static str) -> Result<Self> {
        for row in rows {
            for value in row {
                ensure_finite(value, operation)?;
            }
        }
        Ok(Self { rows })
    }

    fn homogeneous_w(self, point: Point2) -> f64 {
        dot3(self.rows[2], [point.x, point.y, 1.0])
    }
}

/// A finite, axis-aligned, half-open continuous rectangle.
///
/// The minimum edges are included and the maximum edges are excluded. Empty
/// rectangles are valid when either dimension is zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    min: Point2,
    max: Point2,
}

impl Rect {
    /// Creates a normalized rectangle from minimum and maximum edges.
    pub fn new(min: Point2, max: Point2) -> Result<Self> {
        if min.x > max.x || min.y > max.y {
            return Err(invalid_geometry(
                "create_rectangle",
                "rectangle minimum edges must not exceed maximum edges",
            ));
        }
        ensure_finite_pair(max.x - min.x, max.y - min.y, "create_rectangle")?;
        Ok(Self { min, max })
    }

    /// Creates a rectangle from an origin and nonnegative size.
    pub fn from_origin_size(origin: Point2, size: Vector2) -> Result<Self> {
        if size.x < 0.0 || size.y < 0.0 {
            return Err(invalid_geometry(
                "create_rectangle",
                "rectangle size must be nonnegative",
            ));
        }
        Self::new(origin, origin.checked_offset(size)?)
    }

    /// Returns the inclusive minimum corner.
    #[must_use]
    pub const fn min(self) -> Point2 {
        self.min
    }

    /// Returns the exclusive maximum corner.
    #[must_use]
    pub const fn max(self) -> Point2 {
        self.max
    }

    /// Returns the nonnegative horizontal extent.
    #[must_use]
    pub fn width(self) -> f64 {
        self.max.x - self.min.x
    }

    /// Returns the nonnegative vertical extent.
    #[must_use]
    pub fn height(self) -> f64 {
        self.max.y - self.min.y
    }

    /// Returns true when either dimension is zero.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.min.x == self.max.x || self.min.y == self.max.y
    }

    /// Returns true when `point` lies inside the half-open rectangle.
    #[must_use]
    pub fn contains(self, point: Point2) -> bool {
        point.x >= self.min.x
            && point.x < self.max.x
            && point.y >= self.min.y
            && point.y < self.max.y
    }

    /// Returns the nonempty overlap of two rectangles.
    #[must_use]
    pub fn intersection(self, other: Self) -> Option<Self> {
        let min = Point2 {
            x: self.min.x.max(other.min.x),
            y: self.min.y.max(other.min.y),
        };
        let max = Point2 {
            x: self.max.x.min(other.max.x),
            y: self.max.y.min(other.max.y),
        };
        if min.x < max.x && min.y < max.y {
            Some(Self { min, max })
        } else {
            None
        }
    }

    /// Returns the smallest rectangle containing both inputs.
    pub fn checked_union(self, other: Self) -> Result<Self> {
        if self.is_empty() {
            return Ok(other);
        }
        if other.is_empty() {
            return Ok(self);
        }
        Self::new(
            Point2 {
                x: self.min.x.min(other.min.x),
                y: self.min.y.min(other.min.y),
            },
            Point2 {
                x: self.max.x.max(other.max.x),
                y: self.max.y.max(other.max.y),
            },
        )
    }

    /// Maps all corners and returns their axis-aligned bounding rectangle.
    ///
    /// A projective horizon crossing the rectangle is rejected because the
    /// transformed extent would be unbounded.
    pub fn checked_transform_bounds(self, transform: Matrix3) -> Result<Self> {
        let corners = [
            self.min,
            Point2 {
                x: self.max.x,
                y: self.min.y,
            },
            Point2 {
                x: self.min.x,
                y: self.max.y,
            },
            self.max,
        ];
        let mut minimum_w = f64::INFINITY;
        let mut maximum_w = f64::NEG_INFINITY;
        for corner in corners {
            let w = transform.homogeneous_w(corner);
            ensure_finite(w, "transform_rectangle")?;
            minimum_w = minimum_w.min(w);
            maximum_w = maximum_w.max(w);
        }
        if minimum_w <= 0.0 && maximum_w >= 0.0 {
            return Err(invalid_geometry(
                "transform_rectangle",
                "transform's projective horizon crosses the rectangle",
            ));
        }

        let first = transform.checked_transform_point(corners[0])?;
        let mut min = first;
        let mut max = first;
        for corner in &corners[1..] {
            let mapped = transform.checked_transform_point(*corner)?;
            min.x = min.x.min(mapped.x);
            min.y = min.y.min(mapped.y);
            max.x = max.x.max(mapped.x);
            max.y = max.y.max(mapped.y);
        }
        Self::new(min, max)
    }
}

/// An exact, reduced, positive display aspect ratio.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AspectRatio {
    numerator: u32,
    denominator: u32,
}

impl AspectRatio {
    /// A square-pixel or otherwise one-to-one aspect ratio.
    pub const SQUARE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    /// Creates and reduces a positive ratio.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self> {
        if numerator == 0 || denominator == 0 {
            return Err(invalid_geometry(
                "create_aspect_ratio",
                "aspect ratio terms must be greater than zero",
            ));
        }
        let divisor = greatest_common_divisor(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    /// Returns the reduced horizontal term.
    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    /// Returns the reduced vertical term.
    #[must_use]
    pub const fn denominator(self) -> u32 {
        self.denominator
    }
}

impl std::fmt::Display for AspectRatio {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:{}", self.numerator, self.denominator)
    }
}

/// Signed, axis-aligned, half-open pixel edges.
///
/// Integer coordinates identify pixel edges. Pixel `(x, y)` has its center at
/// `(x + 0.5, y + 0.5)`. The type does not impose an image origin or vertical
/// orientation, so graph regions may extend outside a source image.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PixelBounds {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl PixelBounds {
    /// Creates pixel bounds from inclusive minimum and exclusive maximum edges.
    pub fn new(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Result<Self> {
        if min_x > max_x || min_y > max_y {
            return Err(invalid_geometry(
                "create_pixel_bounds",
                "pixel-bound minimum edges must not exceed maximum edges",
            ));
        }
        Ok(Self {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    /// Creates pixel bounds from a signed origin and unsigned size.
    pub fn from_origin_size(min_x: i32, min_y: i32, width: u32, height: u32) -> Result<Self> {
        let max_x = i64::from(min_x) + i64::from(width);
        let max_y = i64::from(min_y) + i64::from(height);
        let max_x = i32::try_from(max_x).map_err(|_| {
            invalid_geometry(
                "create_pixel_bounds",
                "pixel bounds exceed the supported coordinate range",
            )
        })?;
        let max_y = i32::try_from(max_y).map_err(|_| {
            invalid_geometry(
                "create_pixel_bounds",
                "pixel bounds exceed the supported coordinate range",
            )
        })?;
        Self::new(min_x, min_y, max_x, max_y)
    }

    /// Returns the inclusive minimum horizontal edge.
    #[must_use]
    pub const fn min_x(self) -> i32 {
        self.min_x
    }

    /// Returns the inclusive minimum vertical edge.
    #[must_use]
    pub const fn min_y(self) -> i32 {
        self.min_y
    }

    /// Returns the exclusive maximum horizontal edge.
    #[must_use]
    pub const fn max_x(self) -> i32 {
        self.max_x
    }

    /// Returns the exclusive maximum vertical edge.
    #[must_use]
    pub const fn max_y(self) -> i32 {
        self.max_y
    }

    /// Returns the exact horizontal size.
    #[must_use]
    pub fn width(self) -> u32 {
        (i64::from(self.max_x) - i64::from(self.min_x)) as u32
    }

    /// Returns the exact vertical size.
    #[must_use]
    pub fn height(self) -> u32 {
        (i64::from(self.max_y) - i64::from(self.min_y)) as u32
    }

    /// Returns true when either dimension is zero.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.min_x == self.max_x || self.min_y == self.max_y
    }

    /// Returns true when the pixel index lies inside these half-open edges.
    #[must_use]
    pub const fn contains(self, x: i32, y: i32) -> bool {
        x >= self.min_x && x < self.max_x && y >= self.min_y && y < self.max_y
    }

    /// Returns the nonempty overlap of two pixel bounds.
    #[must_use]
    pub fn intersection(self, other: Self) -> Option<Self> {
        let min_x = self.min_x.max(other.min_x);
        let min_y = self.min_y.max(other.min_y);
        let max_x = self.max_x.min(other.max_x);
        let max_y = self.max_y.min(other.max_y);
        if min_x < max_x && min_y < max_y {
            Some(Self {
                min_x,
                min_y,
                max_x,
                max_y,
            })
        } else {
            None
        }
    }

    /// Returns the smallest pixel bounds containing both inputs.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    /// Translates every edge with checked integer arithmetic.
    pub fn checked_translate(self, x: i32, y: i32) -> Result<Self> {
        let min_x = self.min_x.checked_add(x);
        let min_y = self.min_y.checked_add(y);
        let max_x = self.max_x.checked_add(x);
        let max_y = self.max_y.checked_add(y);
        match (min_x, min_y, max_x, max_y) {
            (Some(min_x), Some(min_y), Some(max_x), Some(max_y)) => Ok(Self {
                min_x,
                min_y,
                max_x,
                max_y,
            }),
            _ => Err(invalid_geometry(
                "translate_pixel_bounds",
                "pixel bounds exceed the supported coordinate range",
            )),
        }
    }

    /// Converts integer edges to their exact continuous rectangle.
    #[must_use]
    pub fn to_rect(self) -> Rect {
        Rect {
            min: Point2 {
                x: f64::from(self.min_x),
                y: f64::from(self.min_y),
            },
            max: Point2 {
                x: f64::from(self.max_x),
                y: f64::from(self.max_y),
            },
        }
    }

    /// Returns the exact ratio of a nonempty pixel extent.
    pub fn aspect_ratio(self) -> Result<AspectRatio> {
        AspectRatio::new(self.width(), self.height())
    }
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn ensure_finite_pair(x: f64, y: f64, operation: &'static str) -> Result<()> {
    ensure_finite(x, operation)?;
    ensure_finite(y, operation)
}

fn ensure_finite(value: f64, operation: &'static str) -> Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(invalid_geometry(
            operation,
            "geometry values must be finite",
        ))
    }
}

fn invalid_geometry(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-core.geometry", operation))
}
