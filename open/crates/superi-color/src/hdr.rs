//! Standards-based transfer functions for SDR and HDR signals.
//!
//! Transfer functions operate on RGB component values after numeric range and
//! YUV matrix normalization, but before a primaries transform. SDR and HLG
//! scene functions use relative linear light. PQ uses absolute display
//! luminance in `cd/m2`, exposed as nits in the API. Distinct validated value
//! types prevent relative light, extended encoded signals, normalized PQ
//! signals, and absolute luminance from being interchanged accidentally.
//!
//! The equations follow IEC 61966-2-1 sRGB, ITU-R BT.709, ITU-R BT.2020, and
//! ITU-R BT.2100-3. SDR curves use a signed extension so finite out-of-gamut
//! working values remain finite. PQ stays in its normative `[0, 1]` signal and
//! `[0, 10000]` nit domains. HLG scene encoding accepts non-negative production
//! headroom above one without clipping.

use superi_core::color_space::TransferFunction;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const PQ_M1: f64 = 2_610.0 / 16_384.0;
const PQ_M2: f64 = (2_523.0 / 4_096.0) * 128.0;
const PQ_C1: f64 = 3_424.0 / 4_096.0;
const PQ_C2: f64 = (2_413.0 / 4_096.0) * 32.0;
const PQ_C3: f64 = (2_392.0 / 4_096.0) * 32.0;
const PQ_PEAK_NITS: f64 = 10_000.0;

const HLG_A: f64 = 0.178_832_77;
const HLG_B: f64 = 1.0 - 4.0 * HLG_A;
const HLG_C: f64 = 0.559_910_729_529_562;
const HLG_LINEAR_LIMIT: f64 = 1.0 / 12.0;
const HLG_SIGNAL_LIMIT: f64 = 0.5;
const HLG_LUMA: [f64; 3] = [0.2627, 0.6780, 0.0593];

/// A finite relative linear-light component.
///
/// Relative light may be signed so SDR out-of-gamut values survive working
/// transforms. Transfer functions with a narrower physical domain validate it
/// when they consume this value.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct RelativeLight(f64);

impl RelativeLight {
    /// Creates a finite relative linear-light value.
    pub fn new(value: f64) -> Result<Self> {
        validate_finite("construct_relative_light", "value", value)?;
        Ok(Self(value))
    }

    /// Returns the relative linear-light scalar.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

/// A finite nonlinear signal component.
///
/// This type intentionally permits signed values and production headroom.
/// Curves such as PQ that require a normalized signal validate `[0, 1]` when
/// they consume it.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct EncodedSignal(f64);

impl EncodedSignal {
    /// Creates a finite nonlinear signal component.
    pub fn new(value: f64) -> Result<Self> {
        validate_finite("construct_encoded_signal", "value", value)?;
        Ok(Self(value))
    }

    /// Returns the nonlinear signal scalar.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

/// A finite nonlinear signal component in the closed interval `[0, 1]`.
///
/// PQ uses this stricter type because the BT.2100 EOTF is defined only over a
/// normalized signal. SDR and HLG production signals use [`EncodedSignal`] so
/// signed values and headroom can remain explicit.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct NormalizedSignal(f64);

impl NormalizedSignal {
    /// Creates a finite nonlinear signal in the closed unit interval.
    pub fn new(value: f64) -> Result<Self> {
        validate_closed_unit_interval("construct_normalized_signal", "value", value)?;
        Ok(Self(value))
    }

    /// Returns the normalized nonlinear signal scalar.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

/// A finite, non-negative absolute luminance in nits (`cd/m2`).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Nits(f64);

impl Nits {
    /// Creates a finite, non-negative absolute luminance.
    pub fn new(value: f64) -> Result<Self> {
        validate_non_negative("construct_nits", "value", value)?;
        Ok(Self(value))
    }

    /// Returns the absolute luminance in nits.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

/// Display parameters that determine the reference HLG system rendering.
///
/// The nominal peak and black luminances are absolute display-light values in
/// nits. [`HlgDisplayParameters::new`] derives the BT.2100 system gamma from
/// peak luminance. A view system that has an explicitly chosen rendering
/// intent may use [`HlgDisplayParameters::with_system_gamma`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HlgDisplayParameters {
    nominal_peak_luminance: Nits,
    black_luminance: Nits,
    system_gamma: f64,
    black_level_lift: f64,
}

impl HlgDisplayParameters {
    /// Creates reference HLG display parameters and derives BT.2100 system gamma.
    pub fn new(nominal_peak_luminance: Nits, black_luminance: Nits) -> Result<Self> {
        let nominal_peak_luminance_nits = nominal_peak_luminance.value();
        validate_positive(
            "configure_hlg_display",
            "nominal_peak_luminance_nits",
            nominal_peak_luminance_nits,
        )?;
        let gamma = reference_hlg_system_gamma(nominal_peak_luminance_nits);
        Self::with_system_gamma(nominal_peak_luminance, black_luminance, gamma)
    }

    /// Creates HLG display parameters with an explicit positive system gamma.
    pub fn with_system_gamma(
        nominal_peak_luminance: Nits,
        black_luminance: Nits,
        system_gamma: f64,
    ) -> Result<Self> {
        let nominal_peak_luminance_nits = nominal_peak_luminance.value();
        let black_luminance_nits = black_luminance.value();
        validate_positive(
            "configure_hlg_display",
            "nominal_peak_luminance_nits",
            nominal_peak_luminance_nits,
        )?;
        validate_non_negative(
            "configure_hlg_display",
            "black_luminance_nits",
            black_luminance_nits,
        )?;
        validate_positive("configure_hlg_display", "system_gamma", system_gamma)?;
        if black_luminance_nits >= nominal_peak_luminance_nits {
            return Err(invalid_value(
                "configure_hlg_display",
                "black_luminance_nits",
                black_luminance_nits,
                "HLG display black luminance must be below nominal peak luminance",
            ));
        }

        let black_level_lift = (3.0
            * (black_luminance_nits / nominal_peak_luminance_nits).powf(1.0 / system_gamma))
        .sqrt();
        if black_level_lift >= 1.0 {
            return Err(invalid_value(
                "configure_hlg_display",
                "black_luminance_nits",
                black_luminance_nits,
                "HLG display black luminance leaves no usable signal range",
            ));
        }

        Ok(Self {
            nominal_peak_luminance,
            black_luminance,
            system_gamma,
            black_level_lift,
        })
    }

    /// Returns the nominal achromatic peak luminance in nits.
    #[must_use]
    pub const fn nominal_peak_luminance(self) -> Nits {
        self.nominal_peak_luminance
    }

    /// Returns the display black luminance in nits.
    #[must_use]
    pub const fn black_luminance(self) -> Nits {
        self.black_luminance
    }

    /// Returns the HLG system gamma used by the OOTF.
    #[must_use]
    pub const fn system_gamma(self) -> f64 {
        self.system_gamma
    }
}

/// Decodes an SDR or HLG signal component to relative linear light.
///
/// PQ is rejected because its EOTF produces absolute display luminance. Use
/// [`pq_eotf`] for PQ. `Unspecified` is rejected because guessing a curve would
/// change source meaning.
pub fn decode_relative_transfer(
    transfer: TransferFunction,
    encoded: EncodedSignal,
) -> Result<RelativeLight> {
    let encoded = encoded.value();
    let decoded = match transfer {
        TransferFunction::Unspecified => Err(invalid_transfer(
            "decode_relative_transfer",
            transfer,
            "cannot decode an unspecified transfer function",
        )),
        TransferFunction::Linear => Ok(encoded),
        TransferFunction::Srgb => Ok(decode_srgb(encoded)),
        TransferFunction::Bt709 => Ok(decode_bt(encoded, 1.099, 0.018)),
        TransferFunction::Bt2020TenBit => Ok(decode_bt(
            encoded,
            1.099_296_826_809_44,
            0.018_053_968_510_807,
        )),
        TransferFunction::Bt2020TwelveBit => Ok(decode_bt(encoded, 1.0993, 0.0181)),
        TransferFunction::Gamma22 => Ok(signed_power(encoded, 2.2)),
        TransferFunction::Gamma24 => Ok(signed_power(encoded, 2.4)),
        TransferFunction::Pq => Err(invalid_transfer(
            "decode_relative_transfer",
            transfer,
            "PQ decodes to absolute luminance; use pq_eotf",
        )),
        TransferFunction::Hlg => return hlg_inverse_oetf(EncodedSignal(encoded)),
        _ => Err(invalid_transfer(
            "decode_relative_transfer",
            transfer,
            "transfer function is not supported by this version",
        )),
    }?;
    computed_relative_light("decode_relative_transfer", decoded)
}

/// Encodes a relative linear-light component with an SDR or HLG curve.
///
/// PQ is rejected because its inverse EOTF accepts absolute display luminance
/// in nits. Use [`pq_inverse_eotf`] for PQ.
pub fn encode_relative_transfer(
    transfer: TransferFunction,
    linear: RelativeLight,
) -> Result<EncodedSignal> {
    let linear = linear.value();
    let encoded = match transfer {
        TransferFunction::Unspecified => Err(invalid_transfer(
            "encode_relative_transfer",
            transfer,
            "cannot encode an unspecified transfer function",
        )),
        TransferFunction::Linear => Ok(linear),
        TransferFunction::Srgb => Ok(encode_srgb(linear)),
        TransferFunction::Bt709 => Ok(encode_bt(linear, 1.099, 0.018)),
        TransferFunction::Bt2020TenBit => Ok(encode_bt(
            linear,
            1.099_296_826_809_44,
            0.018_053_968_510_807,
        )),
        TransferFunction::Bt2020TwelveBit => Ok(encode_bt(linear, 1.0993, 0.0181)),
        TransferFunction::Gamma22 => Ok(signed_power(linear, 1.0 / 2.2)),
        TransferFunction::Gamma24 => Ok(signed_power(linear, 1.0 / 2.4)),
        TransferFunction::Pq => Err(invalid_transfer(
            "encode_relative_transfer",
            transfer,
            "PQ encodes absolute luminance; use pq_inverse_eotf",
        )),
        TransferFunction::Hlg => return hlg_oetf(RelativeLight(linear)),
        _ => Err(invalid_transfer(
            "encode_relative_transfer",
            transfer,
            "transfer function is not supported by this version",
        )),
    }?;
    computed_encoded_signal("encode_relative_transfer", encoded)
}

/// Converts one relative transfer encoding to another in decode-then-encode order.
///
/// This function changes only the transfer encoding. Numeric range, component
/// matrices, primaries, gamut mapping, and display view rendering are separate
/// ordered stages owned by their respective color operations.
pub fn convert_relative_transfer(
    source: TransferFunction,
    destination: TransferFunction,
    encoded: EncodedSignal,
) -> Result<EncodedSignal> {
    let linear = decode_relative_transfer(source, encoded)?;
    encode_relative_transfer(destination, linear)
}

/// Applies the BT.2100 PQ EOTF, returning absolute display luminance in nits.
pub fn pq_eotf(encoded: NormalizedSignal) -> Result<Nits> {
    let encoded = encoded.value();
    let encoded_power = encoded.powf(1.0 / PQ_M2);
    let numerator = (encoded_power - PQ_C1).max(0.0);
    let denominator = PQ_C2 - PQ_C3 * encoded_power;
    computed_nits(
        "pq_eotf",
        PQ_PEAK_NITS * (numerator / denominator).powf(1.0 / PQ_M1),
    )
}

/// Applies the inverse BT.2100 PQ EOTF to absolute display luminance in nits.
pub fn pq_inverse_eotf(luminance: Nits) -> Result<NormalizedSignal> {
    let luminance_nits = luminance.value();
    if !(0.0..=PQ_PEAK_NITS).contains(&luminance_nits) {
        return Err(invalid_value(
            "pq_inverse_eotf",
            "luminance_nits",
            luminance_nits,
            "PQ luminance must be within 0 to 10000 nits",
        ));
    }
    let normalized_power = (luminance_nits / PQ_PEAK_NITS).powf(PQ_M1);
    computed_normalized_signal(
        "pq_inverse_eotf",
        ((PQ_C1 + PQ_C2 * normalized_power) / (1.0 + PQ_C3 * normalized_power)).powf(PQ_M2),
    )
}

/// Applies the BT.2100 HLG OETF to non-negative relative scene light.
///
/// Values above one are retained as production headroom and encode above one.
/// Negative scene light is outside the OETF domain and is rejected rather than
/// silently clipped.
pub fn hlg_oetf(scene_linear: RelativeLight) -> Result<EncodedSignal> {
    let scene_linear = scene_linear.value();
    validate_non_negative("hlg_oetf", "scene_linear", scene_linear)?;
    let encoded = if scene_linear <= HLG_LINEAR_LIMIT {
        (3.0 * scene_linear).sqrt()
    } else {
        HLG_A * (12.0 * scene_linear - HLG_B).ln() + HLG_C
    };
    computed_encoded_signal("hlg_oetf", encoded)
}

/// Applies the inverse BT.2100 HLG OETF to a non-negative signal component.
///
/// Signal values above one decode without clipping so production headroom
/// survives conversion to relative scene light.
pub fn hlg_inverse_oetf(encoded: EncodedSignal) -> Result<RelativeLight> {
    let encoded = encoded.value();
    validate_non_negative("hlg_inverse_oetf", "encoded", encoded)?;
    let scene_linear = if encoded <= HLG_SIGNAL_LIMIT {
        encoded * encoded / 3.0
    } else {
        ((encoded - HLG_C) / HLG_A).exp().mul_add(1.0, HLG_B) / 12.0
    };
    computed_relative_light("hlg_inverse_oetf", scene_linear)
}

/// Applies the complete BT.2100 reference HLG EOTF to nonlinear RGB.
///
/// Input is nonlinear HLG RGB and output is display-linear RGB in nits. The
/// OOTF is luminance-dependent, so this operation intentionally accepts the
/// complete RGB triplet rather than transforming components independently.
pub fn hlg_eotf(
    encoded_rgb: [EncodedSignal; 3],
    display: HlgDisplayParameters,
) -> Result<[Nits; 3]> {
    let encoded_rgb = encoded_rgb.map(EncodedSignal::value);
    let beta = display.black_level_lift;
    let adjusted = encoded_rgb.map(|component| ((1.0 - beta) * component + beta).max(0.0));
    let scene_rgb = [
        hlg_inverse_oetf(EncodedSignal(adjusted[0]))?.value(),
        hlg_inverse_oetf(EncodedSignal(adjusted[1]))?.value(),
        hlg_inverse_oetf(EncodedSignal(adjusted[2]))?.value(),
    ];
    let scene_luminance = dot(scene_rgb, HLG_LUMA);
    let scale = if scene_luminance == 0.0 {
        0.0
    } else {
        display.nominal_peak_luminance.value() * scene_luminance.powf(display.system_gamma - 1.0)
    };
    let display_rgb = scene_rgb.map(|component| component * scale);
    Ok([
        computed_nits("hlg_eotf", display_rgb[0])?,
        computed_nits("hlg_eotf", display_rgb[1])?,
        computed_nits("hlg_eotf", display_rgb[2])?,
    ])
}

/// Applies the inverse BT.2100 reference HLG EOTF to display-linear RGB.
///
/// Input and luminance output units are nits. Non-negative values above the
/// nominal display peak remain headroom and are not clipped.
pub fn hlg_inverse_eotf(
    display_rgb_nits: [Nits; 3],
    display: HlgDisplayParameters,
) -> Result<[EncodedSignal; 3]> {
    let display_rgb_nits = display_rgb_nits.map(Nits::value);
    let display_luminance = dot(display_rgb_nits, HLG_LUMA);
    let scene_rgb = if display_luminance == 0.0 {
        [0.0; 3]
    } else {
        let normalized_luminance = display_luminance / display.nominal_peak_luminance.value();
        let scale = normalized_luminance.powf((1.0 - display.system_gamma) / display.system_gamma)
            / display.nominal_peak_luminance.value();
        display_rgb_nits.map(|component| component * scale)
    };
    let beta = display.black_level_lift;
    let denominator = 1.0 - beta;
    Ok([
        computed_encoded_signal(
            "hlg_inverse_eotf",
            (hlg_oetf(RelativeLight(scene_rgb[0]))?.value() - beta) / denominator,
        )?,
        computed_encoded_signal(
            "hlg_inverse_eotf",
            (hlg_oetf(RelativeLight(scene_rgb[1]))?.value() - beta) / denominator,
        )?,
        computed_encoded_signal(
            "hlg_inverse_eotf",
            (hlg_oetf(RelativeLight(scene_rgb[2]))?.value() - beta) / denominator,
        )?,
    ])
}

fn reference_hlg_system_gamma(nominal_peak_luminance_nits: f64) -> f64 {
    if (400.0..=2_000.0).contains(&nominal_peak_luminance_nits) {
        1.2 + 0.42 * (nominal_peak_luminance_nits / 1_000.0).log10()
    } else {
        1.2 * 1.111_f64.powf((nominal_peak_luminance_nits / 1_000.0).log2())
    }
}

fn encode_srgb(linear: f64) -> f64 {
    let absolute = linear.abs();
    if absolute <= 0.003_130_8 {
        12.92 * linear
    } else {
        linear.signum() * 1.055f64.mul_add(absolute.powf(1.0 / 2.4), -0.055)
    }
}

fn decode_srgb(encoded: f64) -> f64 {
    let absolute = encoded.abs();
    if absolute <= 0.04045 {
        encoded / 12.92
    } else {
        encoded.signum() * ((absolute + 0.055) / 1.055).powf(2.4)
    }
}

fn encode_bt(linear: f64, alpha: f64, beta: f64) -> f64 {
    let absolute = linear.abs();
    if absolute < beta {
        4.5 * linear
    } else {
        linear.signum() * alpha.mul_add(absolute.powf(0.45), 1.0 - alpha)
    }
}

fn decode_bt(encoded: f64, alpha: f64, beta: f64) -> f64 {
    let absolute = encoded.abs();
    if absolute < 4.5 * beta {
        encoded / 4.5
    } else {
        encoded.signum() * ((absolute + alpha - 1.0) / alpha).powf(1.0 / 0.45)
    }
}

fn signed_power(value: f64, exponent: f64) -> f64 {
    value.signum() * value.abs().powf(exponent)
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0].mul_add(right[0], left[1].mul_add(right[1], left[2] * right[2]))
}

fn computed_relative_light(operation: &'static str, value: f64) -> Result<RelativeLight> {
    validate_finite(operation, "computed_relative_light", value)?;
    Ok(RelativeLight(value))
}

fn computed_encoded_signal(operation: &'static str, value: f64) -> Result<EncodedSignal> {
    validate_finite(operation, "computed_encoded_signal", value)?;
    Ok(EncodedSignal(value))
}

fn computed_normalized_signal(operation: &'static str, value: f64) -> Result<NormalizedSignal> {
    validate_closed_unit_interval(operation, "computed_normalized_signal", value)?;
    Ok(NormalizedSignal(value))
}

fn computed_nits(operation: &'static str, value: f64) -> Result<Nits> {
    validate_non_negative(operation, "computed_nits", value)?;
    Ok(Nits(value))
}

fn validate_closed_unit_interval(
    operation: &'static str,
    field: &'static str,
    value: f64,
) -> Result<()> {
    validate_finite(operation, field, value)?;
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(invalid_value(
            operation,
            field,
            value,
            "transfer value must be within the closed unit interval",
        ))
    }
}

fn validate_positive(operation: &'static str, field: &'static str, value: f64) -> Result<()> {
    validate_finite(operation, field, value)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(invalid_value(
            operation,
            field,
            value,
            "transfer value must be positive",
        ))
    }
}

fn validate_non_negative(operation: &'static str, field: &'static str, value: f64) -> Result<()> {
    validate_finite(operation, field, value)?;
    if value >= 0.0 {
        Ok(())
    } else {
        Err(invalid_value(
            operation,
            field,
            value,
            "transfer value must be non-negative",
        ))
    }
}

fn validate_finite(operation: &'static str, field: &'static str, value: f64) -> Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(invalid_value(
            operation,
            field,
            value,
            "transfer value must be finite",
        ))
    }
}

fn invalid_transfer(
    operation: &'static str,
    transfer: TransferFunction,
    message: &'static str,
) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-color", operation).with_field("transfer", transfer.code()),
    )
}

fn invalid_value(
    operation: &'static str,
    field: &'static str,
    value: f64,
    message: &'static str,
) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-color", operation)
            .with_field("field", field)
            .with_field("value", value.to_string()),
    )
}
