//! Platform-neutral color interpretation tags.
//!
//! These values preserve source metadata and identify working or output color
//! meaning. Transform math and configuration ownership live in `superi-color`.

/// Chromaticity coordinates associated with RGB primary signals.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ColorPrimaries {
    /// Primaries are absent or intentionally unspecified.
    Unspecified,
    /// ITU-R BT.709 primaries, also used by sRGB.
    Bt709,
    /// ITU-R BT.2020 primaries.
    Bt2020,
    /// Display P3 primaries with a D65 white point.
    DisplayP3,
    /// Academy Color Encoding System AP0 primaries.
    AcesAp0,
    /// Academy Color Encoding System AP1 primaries used by ACEScg.
    AcesAp1,
}

impl ColorPrimaries {
    /// Every primary set defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::Unspecified,
        Self::Bt709,
        Self::Bt2020,
        Self::DisplayP3,
        Self::AcesAp0,
        Self::AcesAp1,
    ];

    /// Returns the permanent code for this primary set.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Bt709 => "bt709",
            Self::Bt2020 => "bt2020",
            Self::DisplayP3 => "display_p3",
            Self::AcesAp0 => "aces_ap0",
            Self::AcesAp1 => "aces_ap1",
        }
    }

    /// Looks up a primary set by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "unspecified" => Some(Self::Unspecified),
            "bt709" => Some(Self::Bt709),
            "bt2020" => Some(Self::Bt2020),
            "display_p3" => Some(Self::DisplayP3),
            "aces_ap0" => Some(Self::AcesAp0),
            "aces_ap1" => Some(Self::AcesAp1),
            _ => None,
        }
    }
}

/// The relationship between encoded component values and light.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum TransferFunction {
    /// Transfer behavior is absent or intentionally unspecified.
    Unspecified,
    /// Scene-linear or display-linear component values.
    Linear,
    /// IEC 61966-2-1 sRGB transfer behavior.
    Srgb,
    /// ITU-R BT.709 transfer behavior.
    Bt709,
    /// ITU-R BT.2020 transfer behavior for 10-bit systems.
    Bt2020TenBit,
    /// ITU-R BT.2020 transfer behavior for 12-bit systems.
    Bt2020TwelveBit,
    /// Pure gamma 2.2 transfer behavior.
    Gamma22,
    /// Pure gamma 2.4 transfer behavior.
    Gamma24,
    /// SMPTE ST 2084 perceptual quantizer transfer behavior.
    Pq,
    /// ITU-R BT.2100 hybrid log-gamma transfer behavior.
    Hlg,
}

impl TransferFunction {
    /// Every transfer function defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::Unspecified,
        Self::Linear,
        Self::Srgb,
        Self::Bt709,
        Self::Bt2020TenBit,
        Self::Bt2020TwelveBit,
        Self::Gamma22,
        Self::Gamma24,
        Self::Pq,
        Self::Hlg,
    ];

    /// Returns the permanent code for this transfer function.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Linear => "linear",
            Self::Srgb => "srgb",
            Self::Bt709 => "bt709",
            Self::Bt2020TenBit => "bt2020_10bit",
            Self::Bt2020TwelveBit => "bt2020_12bit",
            Self::Gamma22 => "gamma22",
            Self::Gamma24 => "gamma24",
            Self::Pq => "pq",
            Self::Hlg => "hlg",
        }
    }

    /// Looks up a transfer function by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "unspecified" => Some(Self::Unspecified),
            "linear" => Some(Self::Linear),
            "srgb" => Some(Self::Srgb),
            "bt709" => Some(Self::Bt709),
            "bt2020_10bit" => Some(Self::Bt2020TenBit),
            "bt2020_12bit" => Some(Self::Bt2020TwelveBit),
            "gamma22" => Some(Self::Gamma22),
            "gamma24" => Some(Self::Gamma24),
            "pq" => Some(Self::Pq),
            "hlg" => Some(Self::Hlg),
            _ => None,
        }
    }
}

/// The matrix used to derive stored components from RGB signals.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MatrixCoefficients {
    /// Matrix behavior is absent or intentionally unspecified.
    Unspecified,
    /// Components are RGB and no luma or chroma matrix is applied.
    Rgb,
    /// ITU-R BT.601 luma and color-difference coefficients.
    Bt601,
    /// ITU-R BT.709 luma and color-difference coefficients.
    Bt709,
    /// ITU-R BT.2020 non-constant-luminance coefficients.
    Bt2020NonConstant,
    /// ITU-R BT.2020 constant-luminance coefficients.
    Bt2020Constant,
}

impl MatrixCoefficients {
    /// Every matrix tag defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::Unspecified,
        Self::Rgb,
        Self::Bt601,
        Self::Bt709,
        Self::Bt2020NonConstant,
        Self::Bt2020Constant,
    ];

    /// Returns the permanent code for these matrix coefficients.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Rgb => "rgb",
            Self::Bt601 => "bt601",
            Self::Bt709 => "bt709",
            Self::Bt2020NonConstant => "bt2020_non_constant",
            Self::Bt2020Constant => "bt2020_constant",
        }
    }

    /// Looks up matrix coefficients by their permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "unspecified" => Some(Self::Unspecified),
            "rgb" => Some(Self::Rgb),
            "bt601" => Some(Self::Bt601),
            "bt709" => Some(Self::Bt709),
            "bt2020_non_constant" => Some(Self::Bt2020NonConstant),
            "bt2020_constant" => Some(Self::Bt2020Constant),
            _ => None,
        }
    }
}

/// The code-value range used by stored components.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ColorRange {
    /// Range signaling is absent or intentionally unspecified.
    Unspecified,
    /// The entire numeric code range is meaningful image data.
    Full,
    /// Image data occupies the video legal range with headroom and footroom.
    Limited,
}

impl ColorRange {
    /// Every color range defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[Self::Unspecified, Self::Full, Self::Limited];

    /// Returns the permanent code for this range.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Full => "full",
            Self::Limited => "limited",
        }
    }

    /// Looks up a color range by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "unspecified" => Some(Self::Unspecified),
            "full" => Some(Self::Full),
            "limited" => Some(Self::Limited),
            _ => None,
        }
    }
}

/// A complete color interpretation assembled from independent metadata axes.
///
/// Construction does not normalize or reject unusual combinations. Ingest
/// must retain declared source metadata exactly, while media and color
/// subsystems decide whether a particular operation supports it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ColorSpace {
    primaries: ColorPrimaries,
    transfer: TransferFunction,
    matrix: MatrixCoefficients,
    range: ColorRange,
}

impl ColorSpace {
    /// Color interpretation is absent or intentionally unspecified.
    pub const UNSPECIFIED: Self = Self::new(
        ColorPrimaries::Unspecified,
        TransferFunction::Unspecified,
        MatrixCoefficients::Unspecified,
        ColorRange::Unspecified,
    );

    /// Standard sRGB interpretation.
    pub const SRGB: Self = Self::new(
        ColorPrimaries::Bt709,
        TransferFunction::Srgb,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    );

    /// Standard limited-range BT.709 YUV interpretation.
    pub const BT709: Self = Self::new(
        ColorPrimaries::Bt709,
        TransferFunction::Bt709,
        MatrixCoefficients::Bt709,
        ColorRange::Limited,
    );

    /// Standard limited-range BT.2020 SDR YUV interpretation.
    pub const BT2020: Self = Self::new(
        ColorPrimaries::Bt2020,
        TransferFunction::Bt2020TenBit,
        MatrixCoefficients::Bt2020NonConstant,
        ColorRange::Limited,
    );

    /// Standard limited-range BT.2100 perceptual-quantizer YUV interpretation.
    pub const BT2100_PQ: Self = Self::new(
        ColorPrimaries::Bt2020,
        TransferFunction::Pq,
        MatrixCoefficients::Bt2020NonConstant,
        ColorRange::Limited,
    );

    /// Standard limited-range BT.2100 hybrid log-gamma YUV interpretation.
    pub const BT2100_HLG: Self = Self::new(
        ColorPrimaries::Bt2020,
        TransferFunction::Hlg,
        MatrixCoefficients::Bt2020NonConstant,
        ColorRange::Limited,
    );

    /// Display P3 primaries with sRGB transfer behavior.
    pub const DISPLAY_P3: Self = Self::new(
        ColorPrimaries::DisplayP3,
        TransferFunction::Srgb,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    );

    /// ACES2065-1 interchange space using AP0 primaries.
    pub const ACES2065_1: Self = Self::new(
        ColorPrimaries::AcesAp0,
        TransferFunction::Linear,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    );

    /// Scene-linear ACEScg working space using AP1 primaries.
    pub const ACESCG: Self = Self::new(
        ColorPrimaries::AcesAp1,
        TransferFunction::Linear,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    );

    /// Creates a color interpretation without changing any declared axis.
    #[must_use]
    pub const fn new(
        primaries: ColorPrimaries,
        transfer: TransferFunction,
        matrix: MatrixCoefficients,
        range: ColorRange,
    ) -> Self {
        Self {
            primaries,
            transfer,
            matrix,
            range,
        }
    }

    /// Returns the declared primary set.
    #[must_use]
    pub const fn primaries(self) -> ColorPrimaries {
        self.primaries
    }

    /// Returns the declared transfer function.
    #[must_use]
    pub const fn transfer(self) -> TransferFunction {
        self.transfer
    }

    /// Returns the declared matrix coefficients.
    #[must_use]
    pub const fn matrix(self) -> MatrixCoefficients {
        self.matrix
    }

    /// Returns the declared code-value range.
    #[must_use]
    pub const fn range(self) -> ColorRange {
        self.range
    }
}
