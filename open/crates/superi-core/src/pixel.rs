//! Shared image and audio representation tags.
//!
//! These values describe storage and semantic identity only. Conversion,
//! mixing, resampling, color transforms, and buffer ownership belong to their
//! respective subsystem crates.

use crate::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

/// The component model stored by a pixel format.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum PixelModel {
    /// One luminance or generic scalar component.
    Gray,
    /// Red and green components.
    Rg,
    /// Red, green, and blue components in that order.
    Rgb,
    /// Blue, green, and red components in that order.
    Bgr,
    /// Red, green, blue, and alpha components in that order.
    Rgba,
    /// Blue, green, red, and alpha components in that order.
    Bgra,
    /// Luma and two color-difference components.
    Yuv,
}

/// The numeric interpretation of each pixel component.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum PixelNumeric {
    /// Unsigned normalized integer data.
    Unorm,
    /// IEEE binary floating point data.
    Float,
}

/// How pixel components are divided between memory planes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum PixelPacking {
    /// Every component for a pixel is stored together in one plane.
    Packed,
    /// Each component is stored in a separate plane.
    Planar,
    /// Luma is stored separately from an interleaved chroma plane.
    Semiplanar,
}

/// Horizontal and vertical chroma sampling density relative to luma.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ChromaSubsampling {
    /// Full chroma resolution in both dimensions.
    Cs444,
    /// Half horizontal chroma resolution and full vertical resolution.
    Cs422,
    /// Half chroma resolution in both dimensions.
    Cs420,
}

/// A platform-neutral pixel storage format.
///
/// Color interpretation and alpha association are deliberately carried by
/// [`crate::color_space::ColorSpace`] and [`AlphaMode`] instead of being
/// inferred from this value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum PixelFormat {
    /// One 8-bit unsigned normalized component.
    R8Unorm,
    /// One 16-bit unsigned normalized component.
    R16Unorm,
    /// One 16-bit floating point component.
    R16Float,
    /// One 32-bit floating point component.
    R32Float,
    /// Two 8-bit unsigned normalized components.
    Rg8Unorm,
    /// Two 16-bit unsigned normalized components.
    Rg16Unorm,
    /// Two 16-bit floating point components.
    Rg16Float,
    /// Two 32-bit floating point components.
    Rg32Float,
    /// Packed RGB with 8-bit unsigned normalized components.
    Rgb8Unorm,
    /// Packed BGR with 8-bit unsigned normalized components.
    Bgr8Unorm,
    /// Packed RGBA with 8-bit unsigned normalized components.
    Rgba8Unorm,
    /// Packed BGRA with 8-bit unsigned normalized components.
    Bgra8Unorm,
    /// Packed RGBA with 16-bit unsigned normalized components.
    Rgba16Unorm,
    /// Packed RGBA with 16-bit floating point components.
    Rgba16Float,
    /// Packed RGBA with 32-bit floating point components.
    Rgba32Float,
    /// Three-plane 8-bit YUV with 4:2:0 chroma sampling.
    Yuv420p8,
    /// Three-plane 10-bit YUV with 4:2:0 chroma sampling.
    Yuv420p10,
    /// Three-plane 8-bit YUV with 4:2:2 chroma sampling.
    Yuv422p8,
    /// Three-plane 10-bit YUV with 4:2:2 chroma sampling.
    Yuv422p10,
    /// Three-plane 8-bit YUV with 4:4:4 chroma sampling.
    Yuv444p8,
    /// Three-plane 10-bit YUV with 4:4:4 chroma sampling.
    Yuv444p10,
    /// Two-plane 8-bit YUV with 4:2:0 chroma sampling.
    Nv12,
    /// Two-plane 10-bit YUV in 16-bit containers with 4:2:0 chroma sampling.
    P010,
}

impl PixelFormat {
    /// Every pixel format defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::R8Unorm,
        Self::R16Unorm,
        Self::R16Float,
        Self::R32Float,
        Self::Rg8Unorm,
        Self::Rg16Unorm,
        Self::Rg16Float,
        Self::Rg32Float,
        Self::Rgb8Unorm,
        Self::Bgr8Unorm,
        Self::Rgba8Unorm,
        Self::Bgra8Unorm,
        Self::Rgba16Unorm,
        Self::Rgba16Float,
        Self::Rgba32Float,
        Self::Yuv420p8,
        Self::Yuv420p10,
        Self::Yuv422p8,
        Self::Yuv422p10,
        Self::Yuv444p8,
        Self::Yuv444p10,
        Self::Nv12,
        Self::P010,
    ];

    /// Returns the permanent platform-neutral code for this format.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::R8Unorm => "r8_unorm",
            Self::R16Unorm => "r16_unorm",
            Self::R16Float => "r16_float",
            Self::R32Float => "r32_float",
            Self::Rg8Unorm => "rg8_unorm",
            Self::Rg16Unorm => "rg16_unorm",
            Self::Rg16Float => "rg16_float",
            Self::Rg32Float => "rg32_float",
            Self::Rgb8Unorm => "rgb8_unorm",
            Self::Bgr8Unorm => "bgr8_unorm",
            Self::Rgba8Unorm => "rgba8_unorm",
            Self::Bgra8Unorm => "bgra8_unorm",
            Self::Rgba16Unorm => "rgba16_unorm",
            Self::Rgba16Float => "rgba16_float",
            Self::Rgba32Float => "rgba32_float",
            Self::Yuv420p8 => "yuv420p8",
            Self::Yuv420p10 => "yuv420p10",
            Self::Yuv422p8 => "yuv422p8",
            Self::Yuv422p10 => "yuv422p10",
            Self::Yuv444p8 => "yuv444p8",
            Self::Yuv444p10 => "yuv444p10",
            Self::Nv12 => "nv12",
            Self::P010 => "p010",
        }
    }

    /// Looks up a pixel format by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "r8_unorm" => Some(Self::R8Unorm),
            "r16_unorm" => Some(Self::R16Unorm),
            "r16_float" => Some(Self::R16Float),
            "r32_float" => Some(Self::R32Float),
            "rg8_unorm" => Some(Self::Rg8Unorm),
            "rg16_unorm" => Some(Self::Rg16Unorm),
            "rg16_float" => Some(Self::Rg16Float),
            "rg32_float" => Some(Self::Rg32Float),
            "rgb8_unorm" => Some(Self::Rgb8Unorm),
            "bgr8_unorm" => Some(Self::Bgr8Unorm),
            "rgba8_unorm" => Some(Self::Rgba8Unorm),
            "bgra8_unorm" => Some(Self::Bgra8Unorm),
            "rgba16_unorm" => Some(Self::Rgba16Unorm),
            "rgba16_float" => Some(Self::Rgba16Float),
            "rgba32_float" => Some(Self::Rgba32Float),
            "yuv420p8" => Some(Self::Yuv420p8),
            "yuv420p10" => Some(Self::Yuv420p10),
            "yuv422p8" => Some(Self::Yuv422p8),
            "yuv422p10" => Some(Self::Yuv422p10),
            "yuv444p8" => Some(Self::Yuv444p8),
            "yuv444p10" => Some(Self::Yuv444p10),
            "nv12" => Some(Self::Nv12),
            "p010" => Some(Self::P010),
            _ => None,
        }
    }

    /// Returns the ordered component model.
    #[must_use]
    pub const fn model(self) -> PixelModel {
        match self {
            Self::R8Unorm | Self::R16Unorm | Self::R16Float | Self::R32Float => PixelModel::Gray,
            Self::Rg8Unorm | Self::Rg16Unorm | Self::Rg16Float | Self::Rg32Float => PixelModel::Rg,
            Self::Rgb8Unorm => PixelModel::Rgb,
            Self::Bgr8Unorm => PixelModel::Bgr,
            Self::Rgba8Unorm | Self::Rgba16Unorm | Self::Rgba16Float | Self::Rgba32Float => {
                PixelModel::Rgba
            }
            Self::Bgra8Unorm => PixelModel::Bgra,
            Self::Yuv420p8
            | Self::Yuv420p10
            | Self::Yuv422p8
            | Self::Yuv422p10
            | Self::Yuv444p8
            | Self::Yuv444p10
            | Self::Nv12
            | Self::P010 => PixelModel::Yuv,
        }
    }

    /// Returns the numeric interpretation of every stored component.
    #[must_use]
    pub const fn numeric(self) -> PixelNumeric {
        match self {
            Self::R16Float
            | Self::R32Float
            | Self::Rg16Float
            | Self::Rg32Float
            | Self::Rgba16Float
            | Self::Rgba32Float => PixelNumeric::Float,
            _ => PixelNumeric::Unorm,
        }
    }

    /// Returns how components are divided between planes.
    #[must_use]
    pub const fn packing(self) -> PixelPacking {
        match self {
            Self::Yuv420p8
            | Self::Yuv420p10
            | Self::Yuv422p8
            | Self::Yuv422p10
            | Self::Yuv444p8
            | Self::Yuv444p10 => PixelPacking::Planar,
            Self::Nv12 | Self::P010 => PixelPacking::Semiplanar,
            _ => PixelPacking::Packed,
        }
    }

    /// Returns meaningful bits per component.
    #[must_use]
    pub const fn bits_per_component(self) -> u8 {
        match self {
            Self::R8Unorm
            | Self::Rg8Unorm
            | Self::Rgb8Unorm
            | Self::Bgr8Unorm
            | Self::Rgba8Unorm
            | Self::Bgra8Unorm
            | Self::Yuv420p8
            | Self::Yuv422p8
            | Self::Yuv444p8
            | Self::Nv12 => 8,
            Self::Yuv420p10 | Self::Yuv422p10 | Self::Yuv444p10 | Self::P010 => 10,
            Self::R16Unorm
            | Self::R16Float
            | Self::Rg16Unorm
            | Self::Rg16Float
            | Self::Rgba16Unorm
            | Self::Rgba16Float => 16,
            Self::R32Float | Self::Rg32Float | Self::Rgba32Float => 32,
        }
    }

    /// Returns the number of independently addressable memory planes.
    #[must_use]
    pub const fn plane_count(self) -> u8 {
        match self.packing() {
            PixelPacking::Packed => 1,
            PixelPacking::Planar => 3,
            PixelPacking::Semiplanar => 2,
        }
    }

    /// Returns bytes per pixel for packed formats.
    ///
    /// Planar and semiplanar formats return `None` because their storage depends
    /// on plane dimensions and row stride.
    #[must_use]
    pub const fn packed_bytes_per_pixel(self) -> Option<u8> {
        match self {
            Self::R8Unorm => Some(1),
            Self::R16Unorm | Self::R16Float | Self::Rg8Unorm => Some(2),
            Self::R32Float
            | Self::Rg16Unorm
            | Self::Rg16Float
            | Self::Rgba8Unorm
            | Self::Bgra8Unorm => Some(4),
            Self::Rgb8Unorm | Self::Bgr8Unorm => Some(3),
            Self::Rg32Float | Self::Rgba16Unorm | Self::Rgba16Float => Some(8),
            Self::Rgba32Float => Some(16),
            Self::Yuv420p8
            | Self::Yuv420p10
            | Self::Yuv422p8
            | Self::Yuv422p10
            | Self::Yuv444p8
            | Self::Yuv444p10
            | Self::Nv12
            | Self::P010 => None,
        }
    }

    /// Returns chroma subsampling when this is a YUV format.
    #[must_use]
    pub const fn chroma_subsampling(self) -> Option<ChromaSubsampling> {
        match self {
            Self::Yuv420p8 | Self::Yuv420p10 | Self::Nv12 | Self::P010 => {
                Some(ChromaSubsampling::Cs420)
            }
            Self::Yuv422p8 | Self::Yuv422p10 => Some(ChromaSubsampling::Cs422),
            Self::Yuv444p8 | Self::Yuv444p10 => Some(ChromaSubsampling::Cs444),
            _ => None,
        }
    }

    /// Returns true when the stored component model contains alpha.
    #[must_use]
    pub const fn has_alpha(self) -> bool {
        matches!(self.model(), PixelModel::Rgba | PixelModel::Bgra)
    }
}

/// How color components relate to an alpha component.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AlphaMode {
    /// The image is fully opaque and any stored alpha component is ignored.
    Opaque,
    /// Color components are independent of alpha, also called unassociated alpha.
    Straight,
    /// Color components have already been multiplied by alpha.
    Premultiplied,
}

impl AlphaMode {
    /// Every alpha interpretation defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[Self::Opaque, Self::Straight, Self::Premultiplied];

    /// Returns the permanent code for this alpha interpretation.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::Straight => "straight",
            Self::Premultiplied => "premultiplied",
        }
    }

    /// Looks up an alpha interpretation by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "opaque" => Some(Self::Opaque),
            "straight" => Some(Self::Straight),
            "premultiplied" => Some(Self::Premultiplied),
            _ => None,
        }
    }
}

/// The numeric interpretation of one audio sample.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum SampleNumeric {
    /// Unsigned integer PCM.
    UnsignedInteger,
    /// Signed integer PCM.
    SignedInteger,
    /// IEEE binary floating point PCM.
    Float,
}

/// A platform-neutral uncompressed audio sample format.
///
/// Planar variants store each channel in a separate plane. Packed variants
/// interleave channels frame by frame. Neither form changes sample timing or
/// channel identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum SampleFormat {
    /// Packed unsigned 8-bit PCM.
    U8,
    /// Planar unsigned 8-bit PCM.
    U8Planar,
    /// Packed signed 16-bit PCM.
    I16,
    /// Planar signed 16-bit PCM.
    I16Planar,
    /// Packed signed 24-bit PCM in three-byte containers.
    I24,
    /// Planar signed 24-bit PCM in three-byte containers.
    I24Planar,
    /// Packed signed 32-bit PCM.
    I32,
    /// Planar signed 32-bit PCM.
    I32Planar,
    /// Packed 32-bit floating point PCM.
    F32,
    /// Planar 32-bit floating point PCM.
    F32Planar,
    /// Packed 64-bit floating point PCM.
    F64,
    /// Planar 64-bit floating point PCM.
    F64Planar,
}

impl SampleFormat {
    /// Every sample format defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::U8,
        Self::U8Planar,
        Self::I16,
        Self::I16Planar,
        Self::I24,
        Self::I24Planar,
        Self::I32,
        Self::I32Planar,
        Self::F32,
        Self::F32Planar,
        Self::F64,
        Self::F64Planar,
    ];

    /// Returns the permanent code for this sample representation.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::U8Planar => "u8_planar",
            Self::I16 => "i16",
            Self::I16Planar => "i16_planar",
            Self::I24 => "i24",
            Self::I24Planar => "i24_planar",
            Self::I32 => "i32",
            Self::I32Planar => "i32_planar",
            Self::F32 => "f32",
            Self::F32Planar => "f32_planar",
            Self::F64 => "f64",
            Self::F64Planar => "f64_planar",
        }
    }

    /// Looks up a sample representation by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "u8" => Some(Self::U8),
            "u8_planar" => Some(Self::U8Planar),
            "i16" => Some(Self::I16),
            "i16_planar" => Some(Self::I16Planar),
            "i24" => Some(Self::I24),
            "i24_planar" => Some(Self::I24Planar),
            "i32" => Some(Self::I32),
            "i32_planar" => Some(Self::I32Planar),
            "f32" => Some(Self::F32),
            "f32_planar" => Some(Self::F32Planar),
            "f64" => Some(Self::F64),
            "f64_planar" => Some(Self::F64Planar),
            _ => None,
        }
    }

    /// Returns the numeric interpretation of each sample.
    #[must_use]
    pub const fn numeric(self) -> SampleNumeric {
        match self {
            Self::U8 | Self::U8Planar => SampleNumeric::UnsignedInteger,
            Self::I16
            | Self::I16Planar
            | Self::I24
            | Self::I24Planar
            | Self::I32
            | Self::I32Planar => SampleNumeric::SignedInteger,
            Self::F32 | Self::F32Planar | Self::F64 | Self::F64Planar => SampleNumeric::Float,
        }
    }

    /// Returns meaningful bits in each sample container.
    #[must_use]
    pub const fn bits_per_sample(self) -> u8 {
        match self {
            Self::U8 | Self::U8Planar => 8,
            Self::I16 | Self::I16Planar => 16,
            Self::I24 | Self::I24Planar => 24,
            Self::I32 | Self::I32Planar | Self::F32 | Self::F32Planar => 32,
            Self::F64 | Self::F64Planar => 64,
        }
    }

    /// Returns bytes occupied by one stored sample.
    #[must_use]
    pub const fn bytes_per_sample(self) -> u8 {
        self.bits_per_sample() / 8
    }

    /// Returns true when each channel occupies a separate memory plane.
    #[must_use]
    pub const fn is_planar(self) -> bool {
        matches!(
            self,
            Self::U8Planar
                | Self::I16Planar
                | Self::I24Planar
                | Self::I32Planar
                | Self::F32Planar
                | Self::F64Planar
        )
    }
}

/// The semantic position of one audio channel.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ChannelPosition {
    /// Front left speaker.
    FrontLeft,
    /// Front right speaker.
    FrontRight,
    /// Front center speaker.
    FrontCenter,
    /// Low-frequency effects channel.
    LowFrequency,
    /// Back left speaker.
    BackLeft,
    /// Back right speaker.
    BackRight,
    /// Front speaker between left and center.
    FrontLeftOfCenter,
    /// Front speaker between right and center.
    FrontRightOfCenter,
    /// Back center speaker.
    BackCenter,
    /// Side left speaker.
    SideLeft,
    /// Side right speaker.
    SideRight,
    /// Top center speaker.
    TopCenter,
    /// Top front left speaker.
    TopFrontLeft,
    /// Top front center speaker.
    TopFrontCenter,
    /// Top front right speaker.
    TopFrontRight,
    /// Top back left speaker.
    TopBackLeft,
    /// Top back center speaker.
    TopBackCenter,
    /// Top back right speaker.
    TopBackRight,
    /// An ordered channel with no implied speaker interpretation.
    Discrete(u16),
}

/// An immutable ordered audio channel layout.
///
/// Equality and ordering include the complete position sequence. This preserves
/// routing intent instead of reducing the layout to an unordered speaker mask.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChannelLayout {
    positions: Box<[ChannelPosition]>,
}

impl ChannelLayout {
    /// Creates a validated ordered layout.
    ///
    /// Empty layouts and repeated semantic positions are rejected because they
    /// cannot identify an unambiguous channel stream.
    pub fn new<I>(positions: I) -> Result<Self>
    where
        I: IntoIterator<Item = ChannelPosition>,
    {
        let positions: Vec<_> = positions.into_iter().collect();
        if positions.is_empty() {
            return Err(invalid_layout(
                "channel layout must contain at least one position",
                0,
            ));
        }
        for (index, position) in positions.iter().enumerate() {
            if positions[..index].contains(position) {
                return Err(invalid_layout(
                    "channel layout contains a duplicate position",
                    positions.len(),
                ));
            }
        }
        Ok(Self {
            positions: positions.into_boxed_slice(),
        })
    }

    /// Canonical mono speaker order.
    #[must_use]
    pub fn mono() -> Self {
        Self::standard([ChannelPosition::FrontCenter])
    }

    /// Canonical stereo speaker order.
    #[must_use]
    pub fn stereo() -> Self {
        Self::standard([ChannelPosition::FrontLeft, ChannelPosition::FrontRight])
    }

    /// Canonical quad speaker order.
    #[must_use]
    pub fn quad() -> Self {
        Self::standard([
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::BackLeft,
            ChannelPosition::BackRight,
        ])
    }

    /// Canonical 5.1 speaker order.
    #[must_use]
    pub fn surround_5_1() -> Self {
        Self::standard([
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::FrontCenter,
            ChannelPosition::LowFrequency,
            ChannelPosition::BackLeft,
            ChannelPosition::BackRight,
        ])
    }

    /// Canonical 7.1 speaker order using back and side pairs.
    #[must_use]
    pub fn surround_7_1() -> Self {
        Self::standard([
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::FrontCenter,
            ChannelPosition::LowFrequency,
            ChannelPosition::BackLeft,
            ChannelPosition::BackRight,
            ChannelPosition::SideLeft,
            ChannelPosition::SideRight,
        ])
    }

    fn standard<const N: usize>(positions: [ChannelPosition; N]) -> Self {
        Self {
            positions: Vec::from(positions).into_boxed_slice(),
        }
    }

    /// Returns positions in stream order without allocating.
    #[must_use]
    pub fn positions(&self) -> &[ChannelPosition] {
        &self.positions
    }

    /// Returns one position by stream index.
    #[must_use]
    pub fn position(&self, index: usize) -> Option<ChannelPosition> {
        self.positions.get(index).copied()
    }

    /// Returns the number of channels in the layout.
    #[must_use]
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Returns true when the layout contains no channels.
    ///
    /// Valid layouts are never empty. This method supports generic collection
    /// inspection without weakening the constructor invariant.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

fn invalid_layout(message: &'static str, channel_count: usize) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-core.pixel", "create_channel_layout")
            .with_field("channel_count", channel_count.to_string()),
    )
}
