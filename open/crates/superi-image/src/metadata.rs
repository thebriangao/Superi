//! Typed image interpretation metadata and lossless source attributes.
//!
//! Orientation uses the TIFF/Exif values without rewriting pixels. Missing
//! orientation and pixel-aspect tags remain distinguishable from their standard
//! display defaults. Timecode retains its exact frame rate and counting mode
//! through [`superi_core::timecode::Timecode`]. Color tags keep one authoritative
//! platform-neutral interpretation while preserving optional source identifiers
//! and embedded ICC bytes for later color-management policy.
//!
//! Arbitrary attributes are kept separately in deterministic key order. Format
//! readers must populate typed fields explicitly; this module never guesses
//! semantic values from source-specific attribute names.

use std::collections::BTreeMap;
use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::AspectRatio;
use superi_core::timecode::Timecode;

const COMPONENT: &str = "superi-image.metadata";

/// The relationship of stored rows and columns to intended display orientation.
///
/// Discriminants are the permanent TIFF/Exif values. The corner names describe
/// where the stored `(0, 0)` sample lies in the intended display. No variant
/// claims that the underlying samples have already been transformed.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
#[repr(u8)]
pub enum ImageOrientation {
    /// Stored rows run top-to-bottom and columns left-to-right.
    #[default]
    TopLeft = 1,
    /// Stored rows run top-to-bottom and columns right-to-left.
    TopRight = 2,
    /// Stored rows run bottom-to-top and columns right-to-left.
    BottomRight = 3,
    /// Stored rows run bottom-to-top and columns left-to-right.
    BottomLeft = 4,
    /// Stored rows run left-to-right and columns top-to-bottom (transpose).
    LeftTop = 5,
    /// Stored rows run right-to-left and columns top-to-bottom (90° clockwise display correction).
    RightTop = 6,
    /// Stored rows run right-to-left and columns bottom-to-top (transverse).
    RightBottom = 7,
    /// Stored rows run left-to-right and columns bottom-to-top (90° counter-clockwise display correction).
    LeftBottom = 8,
}

impl ImageOrientation {
    /// Every orientation in TIFF/Exif numeric order.
    pub const ALL: &'static [Self] = &[
        Self::TopLeft,
        Self::TopRight,
        Self::BottomRight,
        Self::BottomLeft,
        Self::LeftTop,
        Self::RightTop,
        Self::RightBottom,
        Self::LeftBottom,
    ];

    /// Returns the exact TIFF/Exif integer value in the range 1 through 8.
    #[must_use]
    pub const fn exif_value(self) -> u8 {
        self as u8
    }

    /// Returns true when intended display width and height exchange stored axes.
    #[must_use]
    pub const fn swaps_axes(self) -> bool {
        matches!(
            self,
            Self::LeftTop | Self::RightTop | Self::RightBottom | Self::LeftBottom
        )
    }
}

impl TryFrom<u8> for ImageOrientation {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::TopLeft),
            2 => Ok(Self::TopRight),
            3 => Ok(Self::BottomRight),
            4 => Ok(Self::BottomLeft),
            5 => Ok(Self::LeftTop),
            6 => Ok(Self::RightTop),
            7 => Ok(Self::RightBottom),
            8 => Ok(Self::LeftBottom),
            _ => Err(invalid(
                "parse_orientation",
                "TIFF/Exif orientation must be an integer from 1 through 8",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "orientation_value")
                    .with_field("value", value.to_string()),
            )),
        }
    }
}

/// An exactly preserved floating-point metadata value.
///
/// The IEEE binary64 payload is stored verbatim so signed zero, infinities, and
/// distinct NaN payloads survive inspection and round trips.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImageMetadataFloat(u64);

impl ImageMetadataFloat {
    /// Stores the exact IEEE bits of `value`.
    #[must_use]
    pub fn new(value: f64) -> Self {
        Self(value.to_bits())
    }

    /// Constructs a value from its exact IEEE binary64 payload.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Returns the represented floating-point value.
    #[must_use]
    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Returns the exact stored IEEE binary64 payload.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

/// One losslessly retained, source-specific image attribute value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ImageMetadataValue {
    /// Boolean data.
    Boolean(bool),
    /// UTF-8 text.
    Text(String),
    /// Signed integer data.
    Signed(i64),
    /// Unsigned integer data.
    Unsigned(u64),
    /// An exact IEEE binary64 value.
    Float(ImageMetadataFloat),
    /// Uninterpreted bytes for source attributes not understood by this build.
    Bytes(Arc<[u8]>),
}

impl ImageMetadataValue {
    /// Returns a floating-point value when this attribute stores one.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(value.value()),
            _ => None,
        }
    }
}

/// Authoritative color interpretation plus optional source color payloads.
///
/// `interpretation` is the stable value consumed by Superi processing. A named
/// source space and ICC profile are retained exactly for interchange and later
/// color-management decisions; their presence never silently overrides the
/// authoritative interpretation or chooses a transform.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageColorTags {
    interpretation: ColorSpace,
    named_space: Option<String>,
    icc_profile: Option<Arc<[u8]>>,
}

impl ImageColorTags {
    /// Creates color tags with an explicit processing interpretation.
    #[must_use]
    pub const fn new(interpretation: ColorSpace) -> Self {
        Self {
            interpretation,
            named_space: None,
            icc_profile: None,
        }
    }

    /// Returns the authoritative platform-neutral color interpretation.
    #[must_use]
    pub const fn interpretation(&self) -> ColorSpace {
        self.interpretation
    }

    /// Replaces the authoritative processing interpretation.
    #[must_use]
    pub fn with_interpretation(mut self, interpretation: ColorSpace) -> Self {
        self.interpretation = interpretation;
        self
    }

    /// Preserves a nonempty source color-space identifier such as an OCIO name.
    pub fn with_named_space(mut self, name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        validate_text(&name, "set_named_color_space", "named color space")?;
        self.named_space = Some(name);
        Ok(self)
    }

    /// Returns the exact source color-space identifier, when present.
    #[must_use]
    pub fn named_space(&self) -> Option<&str> {
        self.named_space.as_deref()
    }

    /// Removes and returns the source color-space identifier.
    pub fn clear_named_space(&mut self) -> Option<String> {
        self.named_space.take()
    }

    /// Preserves a nonempty embedded ICC profile without parsing or rewriting it.
    pub fn with_icc_profile(mut self, profile: Arc<[u8]>) -> Result<Self> {
        if profile.is_empty() {
            return Err(invalid(
                "set_icc_profile",
                "embedded ICC profile bytes must not be empty",
            ));
        }
        self.icc_profile = Some(profile);
        Ok(self)
    }

    /// Returns the exact shared embedded ICC payload, when present.
    #[must_use]
    pub fn icc_profile(&self) -> Option<&[u8]> {
        self.icc_profile.as_deref()
    }

    /// Removes and returns the embedded ICC payload.
    pub fn clear_icc_profile(&mut self) -> Option<Arc<[u8]>> {
        self.icc_profile.take()
    }
}

impl Default for ImageColorTags {
    fn default() -> Self {
        Self::new(ColorSpace::UNSPECIFIED)
    }
}

/// Typed standard metadata and deterministically ordered source attributes.
///
/// Optional standard fields preserve whether a source actually declared a
/// value. [`ImageMetadata::effective_orientation`] and
/// [`ImageMetadata::effective_pixel_aspect_ratio`] expose industry defaults for
/// display without destroying that distinction.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImageMetadata {
    orientation: Option<ImageOrientation>,
    pixel_aspect_ratio: Option<AspectRatio>,
    timecode: Option<Timecode>,
    attributes: BTreeMap<String, ImageMetadataValue>,
}

impl ImageMetadata {
    /// Creates metadata with no source-declared fields or attributes.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            orientation: None,
            pixel_aspect_ratio: None,
            timecode: None,
            attributes: BTreeMap::new(),
        }
    }

    /// Returns the source-declared TIFF/Exif orientation, when present.
    #[must_use]
    pub const fn orientation(&self) -> Option<ImageOrientation> {
        self.orientation
    }

    /// Returns the declared orientation or the standard top-left default.
    #[must_use]
    pub fn effective_orientation(&self) -> ImageOrientation {
        self.orientation.unwrap_or_default()
    }

    /// Sets the source-declared orientation and returns the prior declaration.
    pub fn set_orientation(&mut self, orientation: ImageOrientation) -> Option<ImageOrientation> {
        self.orientation.replace(orientation)
    }

    /// Returns a copy with an explicit source orientation.
    #[must_use]
    pub fn with_orientation(mut self, orientation: ImageOrientation) -> Self {
        self.orientation = Some(orientation);
        self
    }

    /// Removes and returns the source-declared orientation.
    pub fn clear_orientation(&mut self) -> Option<ImageOrientation> {
        self.orientation.take()
    }

    /// Returns the source-declared exact horizontal-to-vertical pixel ratio.
    #[must_use]
    pub const fn pixel_aspect_ratio(&self) -> Option<AspectRatio> {
        self.pixel_aspect_ratio
    }

    /// Returns the declared pixel ratio or the standard square-pixel default.
    #[must_use]
    pub fn effective_pixel_aspect_ratio(&self) -> AspectRatio {
        self.pixel_aspect_ratio.unwrap_or(AspectRatio::SQUARE)
    }

    /// Sets the source-declared pixel aspect ratio and returns the prior value.
    pub fn set_pixel_aspect_ratio(&mut self, ratio: AspectRatio) -> Option<AspectRatio> {
        self.pixel_aspect_ratio.replace(ratio)
    }

    /// Returns a copy with an explicit source pixel aspect ratio.
    #[must_use]
    pub fn with_pixel_aspect_ratio(mut self, ratio: AspectRatio) -> Self {
        self.pixel_aspect_ratio = Some(ratio);
        self
    }

    /// Removes and returns the source-declared pixel aspect ratio.
    pub fn clear_pixel_aspect_ratio(&mut self) -> Option<AspectRatio> {
        self.pixel_aspect_ratio.take()
    }

    /// Returns editorial timecode with its exact rate and counting mode.
    #[must_use]
    pub const fn timecode(&self) -> Option<Timecode> {
        self.timecode
    }

    /// Sets timecode and returns the prior value.
    pub fn set_timecode(&mut self, timecode: Timecode) -> Option<Timecode> {
        self.timecode.replace(timecode)
    }

    /// Returns a copy with exact editorial timecode.
    #[must_use]
    pub fn with_timecode(mut self, timecode: Timecode) -> Self {
        self.timecode = Some(timecode);
        self
    }

    /// Removes and returns editorial timecode.
    pub fn clear_timecode(&mut self) -> Option<Timecode> {
        self.timecode.take()
    }

    /// Inserts a source-specific attribute and returns the prior value for its key.
    ///
    /// Keys retain exact spelling and namespaces. Empty keys and embedded NUL
    /// bytes are rejected because common image APIs cannot transport them safely.
    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: ImageMetadataValue,
    ) -> Result<Option<ImageMetadataValue>> {
        let key = key.into();
        validate_text(&key, "insert_metadata", "image metadata key")?;
        Ok(self.attributes.insert(key, value))
    }

    /// Returns a source-specific attribute by exact key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&ImageMetadataValue> {
        self.attributes.get(key)
    }

    /// Removes and returns a source-specific attribute by exact key.
    pub fn remove(&mut self, key: &str) -> Option<ImageMetadataValue> {
        self.attributes.remove(key)
    }

    /// Iterates source-specific attributes in deterministic key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ImageMetadataValue)> {
        self.attributes
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    /// Returns the number of source-specific attributes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.attributes.len()
    }

    /// Returns true when no standard field or source-specific attribute is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.orientation.is_none()
            && self.pixel_aspect_ratio.is_none()
            && self.timecode.is_none()
            && self.attributes.is_empty()
    }
}

fn validate_text(value: &str, operation: &'static str, field: &'static str) -> Result<()> {
    if value.is_empty() || value.contains('\0') {
        return Err(invalid(
            operation,
            format!("{field} must be nonempty and contain no NUL bytes"),
        )
        .with_context(
            ErrorContext::new(COMPONENT, "text_value")
                .with_field("field", field)
                .with_field("length", value.len().to_string()),
        ));
    }
    Ok(())
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
