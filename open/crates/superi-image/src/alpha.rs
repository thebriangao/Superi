//! Explicit alpha interpretation and storage-neutral premultiplication rules.
//!
//! [`AlphaLayout`] binds recognized color channels to alpha without changing
//! their names or order. [`AlphaTransform`] then applies only the numeric alpha
//! association requested by the caller. It never clamps HDR color or alpha
//! values, touches auxiliary channels, changes spatial extent, or performs an
//! implicit composite.

use half::f16;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::AlphaMode;

use crate::channels::{ChannelIndex, ChannelList, ChannelName, StandardChannel};
use crate::value::{Image, ImageDescriptor, ImageSamples};

const COMPONENT: &str = "superi-image.alpha";

/// The intent used when associating straight color with alpha.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum PremultiplicationRule {
    /// A one-time straight-to-premultiplied conversion.
    ///
    /// Color is multiplied by alpha even when alpha is zero. Hidden straight
    /// color is therefore intentionally discarded at fully transparent pixels.
    OneTime,
    /// Re-association after a temporary unpremultiply operation.
    ///
    /// Color is left unchanged where alpha is zero. This preserves premultiplied
    /// emissive or glow values through an unpremultiply, process, re-premultiply
    /// round trip.
    PreserveZeroAlpha,
}

/// Deterministic color-to-alpha bindings for an ordered channel list.
///
/// `R`, `G`, `B`, and `Y` are color channels. `R`, `G`, and `B` first use a
/// same-component alpha (`AR`, `AG`, or `AB`) when present, then `A`. Search
/// starts in the color channel's layer and continues through directly enclosing
/// layers to the base layer. Alpha, depth, identifiers, and arbitrary auxiliary
/// channels are never classified as color.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlphaLayout {
    channel_count: usize,
    alpha_by_channel: Vec<Option<ChannelIndex>>,
    color_channels: Vec<ChannelIndex>,
    alpha_channels: Vec<ChannelIndex>,
}

impl AlphaLayout {
    /// Derives alpha bindings without changing the source channel list.
    #[must_use]
    pub fn from_channels(channels: &ChannelList) -> Self {
        let mut alpha_by_channel = vec![None; channels.len()];
        let mut color_channels = Vec::new();
        let mut alpha_channels = Vec::new();

        for (index, channel) in channels.iter().enumerate() {
            let index = ChannelIndex::new(index);
            match channel.standard() {
                Some(
                    StandardChannel::Alpha
                    | StandardChannel::RedAlpha
                    | StandardChannel::GreenAlpha
                    | StandardChannel::BlueAlpha,
                ) => alpha_channels.push(index),
                Some(
                    standard @ (StandardChannel::Red
                    | StandardChannel::Green
                    | StandardChannel::Blue
                    | StandardChannel::Luminance),
                ) => {
                    color_channels.push(index);
                    alpha_by_channel[index.get()] = resolve_alpha(channels, channel, standard);
                }
                _ => {}
            }
        }

        Self {
            channel_count: channels.len(),
            alpha_by_channel,
            color_channels,
            alpha_channels,
        }
    }

    /// Returns the exact number of channels described by this layout.
    #[must_use]
    pub const fn channel_count(&self) -> usize {
        self.channel_count
    }

    /// Returns the alpha bound to one recognized color channel.
    ///
    /// `None` means the index is not a recognized color channel, is outside the
    /// layout, or has no matching alpha channel.
    #[must_use]
    pub fn alpha_for(&self, channel: ChannelIndex) -> Option<ChannelIndex> {
        self.alpha_by_channel.get(channel.get()).copied().flatten()
    }

    /// Returns recognized color channels in unchanged source order.
    #[must_use]
    pub fn color_channels(&self) -> &[ChannelIndex] {
        &self.color_channels
    }

    /// Returns recognized alpha channels in unchanged source order.
    #[must_use]
    pub fn alpha_channels(&self) -> &[ChannelIndex] {
        &self.alpha_channels
    }
}

/// A validated numeric conversion between two explicit alpha interpretations.
///
/// Each conversion method accepts logical interleaved pixels in the exact order
/// of the [`ChannelList`] used to construct the transform. The transform does
/// not own or reinterpret image storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlphaTransform {
    channels: ChannelList,
    layout: AlphaLayout,
    source_mode: AlphaMode,
    destination_mode: AlphaMode,
    rule: PremultiplicationRule,
}

impl AlphaTransform {
    /// Creates an ordinary alpha conversion.
    ///
    /// Straight-to-premultiplied conversion uses
    /// [`PremultiplicationRule::OneTime`].
    pub fn new(
        channels: &ChannelList,
        source_mode: AlphaMode,
        destination_mode: AlphaMode,
    ) -> Result<Self> {
        Self::with_rule(
            channels,
            source_mode,
            destination_mode,
            PremultiplicationRule::OneTime,
        )
    }

    /// Creates an alpha conversion with an explicit association rule.
    pub fn with_rule(
        channels: &ChannelList,
        source_mode: AlphaMode,
        destination_mode: AlphaMode,
        rule: PremultiplicationRule,
    ) -> Result<Self> {
        if rule == PremultiplicationRule::PreserveZeroAlpha
            && (source_mode != AlphaMode::Straight || destination_mode != AlphaMode::Premultiplied)
        {
            return Err(invalid(
                "create_alpha_transform",
                "zero-alpha preservation is valid only when re-premultiplying straight color",
            ));
        }

        let layout = AlphaLayout::from_channels(channels);
        if source_mode != AlphaMode::Opaque || destination_mode != AlphaMode::Opaque {
            for &color in layout.color_channels() {
                if layout.alpha_for(color).is_none() {
                    let name = channels
                        .get(color)
                        .expect("alpha layout color indices come from the channel list");
                    return Err(invalid(
                        "create_alpha_transform",
                        "a non-opaque color channel has no matching alpha channel",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "bind_color_alpha")
                            .with_field("channel", name.as_str()),
                    ));
                }
            }
        }

        Ok(Self {
            channels: channels.clone(),
            layout,
            source_mode,
            destination_mode,
            rule,
        })
    }

    /// Returns the interpretation expected for source samples.
    #[must_use]
    pub const fn source_mode(&self) -> AlphaMode {
        self.source_mode
    }

    /// Returns the interpretation produced for destination samples.
    #[must_use]
    pub const fn destination_mode(&self) -> AlphaMode {
        self.destination_mode
    }

    /// Returns the requested straight-to-premultiplied association rule.
    #[must_use]
    pub const fn rule(&self) -> PremultiplicationRule {
        self.rule
    }

    /// Returns the immutable channel binding used by this transform.
    #[must_use]
    pub const fn layout(&self) -> &AlphaLayout {
        &self.layout
    }

    /// Returns the exact ordered channel identity expected by this transform.
    #[must_use]
    pub const fn channels(&self) -> &ChannelList {
        &self.channels
    }

    /// Transforms one logical pixel in place.
    pub fn transform_pixel(&self, samples: &mut [f32]) -> Result<()> {
        if samples.len() != self.layout.channel_count() {
            return Err(sample_count_error(
                "transform_alpha_pixel",
                samples.len(),
                self.layout.channel_count(),
            ));
        }
        if self.source_mode == self.destination_mode {
            return Ok(());
        }

        match (self.source_mode, self.destination_mode) {
            (AlphaMode::Opaque, AlphaMode::Straight | AlphaMode::Premultiplied) => {
                self.make_alpha_opaque(samples);
            }
            (AlphaMode::Straight | AlphaMode::Premultiplied, AlphaMode::Opaque) => {
                self.make_alpha_opaque(samples);
            }
            (AlphaMode::Straight, AlphaMode::Premultiplied) => {
                for &color in self.layout.color_channels() {
                    let alpha = self.bound_alpha(samples, color);
                    let value = samples[color.get()];
                    samples[color.get()] =
                        if self.rule == PremultiplicationRule::PreserveZeroAlpha && alpha == 0.0 {
                            value
                        } else {
                            value * alpha
                        };
                }
            }
            (AlphaMode::Premultiplied, AlphaMode::Straight) => {
                for &color in self.layout.color_channels() {
                    let alpha = self.bound_alpha(samples, color);
                    if alpha != 0.0 {
                        samples[color.get()] /= alpha;
                    }
                }
            }
            _ => {
                return Err(self.unsupported_conversion("transform_alpha_pixel"));
            }
        }
        Ok(())
    }

    /// Transforms normalized unsigned 8-bit logical pixels in place.
    ///
    /// Results use deterministic nearest-integer rounding. Unpremultiplication
    /// saturates only when invalid associated input cannot fit in the bounded
    /// destination representation.
    pub fn transform_u8_pixels(&self, samples: &mut [u8]) -> Result<()> {
        self.transform_unsigned_pixels(samples, "transform_alpha_u8_pixels")
    }

    /// Transforms normalized unsigned 16-bit logical pixels in place.
    ///
    /// Results use deterministic nearest-integer rounding. Unpremultiplication
    /// saturates only when invalid associated input cannot fit in the bounded
    /// destination representation.
    pub fn transform_u16_pixels(&self, samples: &mut [u16]) -> Result<()> {
        self.transform_unsigned_pixels(samples, "transform_alpha_u16_pixels")
    }

    /// Converts a dense image while retaining its complete non-alpha identity.
    ///
    /// Data and display windows, pixel format, color space, channel names and
    /// order, metadata, sample representation, and source image remain
    /// unchanged. Only color samples and the explicit alpha interpretation of
    /// the returned image change.
    pub fn transform_image(&self, image: &Image) -> Result<Image> {
        self.validate_image(image)?;
        if self.source_mode == self.destination_mode {
            return Ok(image.clone());
        }

        let samples = match image.samples() {
            ImageSamples::U8(values) => {
                let mut values = values.to_vec();
                self.transform_u8_pixels(&mut values)?;
                ImageSamples::from_u8(values)
            }
            ImageSamples::U16(values) => {
                let mut values = values.to_vec();
                self.transform_u16_pixels(&mut values)?;
                ImageSamples::from_u16(values)
            }
            ImageSamples::F16(bits) => {
                let mut bits = bits.to_vec();
                self.transform_f16_bits(&mut bits)?;
                ImageSamples::from_f16_bits(bits)
            }
            ImageSamples::F32(bits) => {
                let mut bits = bits.to_vec();
                self.transform_f32_bits(&mut bits)?;
                ImageSamples::from_f32_bits(bits)
            }
        };

        let source = image.descriptor();
        let descriptor = ImageDescriptor::new_with_color_tags(
            source.data_window(),
            source.display_window(),
            source.pixel_format(),
            source.color_tags().clone(),
            self.destination_mode,
        )?
        .with_channels(self.channels.clone())?;
        Image::new_with_metadata(descriptor, samples, image.metadata().clone())
    }

    /// Transforms zero or more complete interleaved logical pixels in place.
    pub fn transform_pixels(&self, samples: &mut [f32]) -> Result<()> {
        if samples.len() % self.layout.channel_count() != 0 {
            return Err(sample_count_error(
                "transform_alpha_pixels",
                samples.len(),
                self.layout.channel_count(),
            ));
        }
        for pixel in samples.chunks_exact_mut(self.layout.channel_count()) {
            self.transform_pixel(pixel)?;
        }
        Ok(())
    }

    /// Transforms raw IEEE binary16 payloads in place.
    ///
    /// Only recognized color channels and explicitly opaque alpha outputs are
    /// rewritten. Untouched alpha and auxiliary payload bits remain exact,
    /// including signed zero and NaN payloads.
    pub fn transform_f16_bits(&self, bits: &mut [u16]) -> Result<()> {
        self.transform_float_bits(bits, "transform_alpha_f16_bits")
    }

    /// Transforms raw IEEE binary32 payloads in place.
    ///
    /// Only recognized color channels and explicitly opaque alpha outputs are
    /// rewritten. Untouched alpha and auxiliary payload bits remain exact,
    /// including signed zero and NaN payloads.
    pub fn transform_f32_bits(&self, bits: &mut [u32]) -> Result<()> {
        self.transform_float_bits(bits, "transform_alpha_f32_bits")
    }

    fn bound_alpha(&self, samples: &[f32], color: ChannelIndex) -> f32 {
        let alpha = self
            .layout
            .alpha_for(color)
            .expect("alpha transform construction validates every color binding");
        samples[alpha.get()]
    }

    fn make_alpha_opaque(&self, samples: &mut [f32]) {
        for &alpha in self.layout.alpha_channels() {
            samples[alpha.get()] = 1.0;
        }
    }

    fn transform_float_bits<T: FloatAlphaBits>(
        &self,
        bits: &mut [T],
        operation: &'static str,
    ) -> Result<()> {
        if bits.len() % self.layout.channel_count() != 0 {
            return Err(sample_count_error(
                operation,
                bits.len(),
                self.layout.channel_count(),
            ));
        }
        if self.source_mode == self.destination_mode {
            return Ok(());
        }
        for pixel in bits.chunks_exact_mut(self.layout.channel_count()) {
            match (self.source_mode, self.destination_mode) {
                (AlphaMode::Opaque, AlphaMode::Straight | AlphaMode::Premultiplied)
                | (AlphaMode::Straight | AlphaMode::Premultiplied, AlphaMode::Opaque) => {
                    for &alpha in self.layout.alpha_channels() {
                        pixel[alpha.get()] = T::one();
                    }
                }
                (AlphaMode::Straight, AlphaMode::Premultiplied) => {
                    for &color in self.layout.color_channels() {
                        let alpha = self
                            .layout
                            .alpha_for(color)
                            .expect("alpha transform construction validates every color binding");
                        let alpha = pixel[alpha.get()].into_f32();
                        let value = pixel[color.get()].into_f32();
                        let converted = if self.rule == PremultiplicationRule::PreserveZeroAlpha
                            && alpha == 0.0
                        {
                            value
                        } else {
                            value * alpha
                        };
                        pixel[color.get()] = T::from_f32(converted);
                    }
                }
                (AlphaMode::Premultiplied, AlphaMode::Straight) => {
                    for &color in self.layout.color_channels() {
                        let alpha = self
                            .layout
                            .alpha_for(color)
                            .expect("alpha transform construction validates every color binding");
                        let alpha = pixel[alpha.get()].into_f32();
                        if alpha != 0.0 {
                            let value = pixel[color.get()].into_f32();
                            pixel[color.get()] = T::from_f32(value / alpha);
                        }
                    }
                }
                _ => return Err(self.unsupported_conversion(operation)),
            }
        }
        Ok(())
    }

    fn transform_unsigned_pixels<T: UnsignedAlphaSample>(
        &self,
        samples: &mut [T],
        operation: &'static str,
    ) -> Result<()> {
        if samples.len() % self.layout.channel_count() != 0 {
            return Err(sample_count_error(
                operation,
                samples.len(),
                self.layout.channel_count(),
            ));
        }
        if self.source_mode == self.destination_mode {
            return Ok(());
        }
        for pixel in samples.chunks_exact_mut(self.layout.channel_count()) {
            match (self.source_mode, self.destination_mode) {
                (AlphaMode::Opaque, AlphaMode::Straight | AlphaMode::Premultiplied)
                | (AlphaMode::Straight | AlphaMode::Premultiplied, AlphaMode::Opaque) => {
                    self.make_unsigned_alpha_opaque(pixel)
                }
                (AlphaMode::Straight, AlphaMode::Premultiplied) => {
                    for &color in self.layout.color_channels() {
                        let alpha = self.bound_unsigned_alpha(pixel, color);
                        let value = pixel[color.get()].into_u64();
                        let converted = if self.rule == PremultiplicationRule::PreserveZeroAlpha
                            && alpha == 0
                        {
                            value
                        } else {
                            value
                                .checked_mul(alpha)
                                .and_then(|product| product.checked_add(T::MAX / 2))
                                .expect("u16 normalized alpha products fit in u64")
                                / T::MAX
                        };
                        pixel[color.get()] = T::from_u64(converted);
                    }
                }
                (AlphaMode::Premultiplied, AlphaMode::Straight) => {
                    for &color in self.layout.color_channels() {
                        let alpha = self.bound_unsigned_alpha(pixel, color);
                        let value = pixel[color.get()].into_u64();
                        let converted = value
                            .checked_mul(T::MAX)
                            .and_then(|product| product.checked_add(alpha / 2))
                            .expect("u16 normalized alpha products fit in u64")
                            .checked_div(alpha);
                        if let Some(converted) = converted {
                            pixel[color.get()] = T::from_u64(converted.min(T::MAX));
                        }
                    }
                }
                _ => return Err(self.unsupported_conversion(operation)),
            }
        }
        Ok(())
    }

    fn bound_unsigned_alpha<T: UnsignedAlphaSample>(
        &self,
        samples: &[T],
        color: ChannelIndex,
    ) -> u64 {
        let alpha = self
            .layout
            .alpha_for(color)
            .expect("alpha transform construction validates every color binding");
        samples[alpha.get()].into_u64()
    }

    fn make_unsigned_alpha_opaque<T: UnsignedAlphaSample>(&self, samples: &mut [T]) {
        for &alpha in self.layout.alpha_channels() {
            samples[alpha.get()] = T::from_u64(T::MAX);
        }
    }

    fn validate_image(&self, image: &Image) -> Result<()> {
        if image.descriptor().channels() != &self.channels {
            return Err(invalid(
                "transform_alpha_image",
                "image channels do not match the alpha transform channel identity",
            ));
        }
        if image.descriptor().alpha_mode() != self.source_mode {
            return Err(invalid(
                "transform_alpha_image",
                "image alpha interpretation does not match the transform source mode",
            )
            .with_context(alpha_mode_context(
                "inspect_image_alpha_mode",
                image.descriptor().alpha_mode(),
                self.source_mode,
            )));
        }
        Ok(())
    }

    fn unsupported_conversion(&self, operation: &'static str) -> Error {
        Error::new(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "this build does not support the requested alpha interpretation conversion",
        )
        .with_context(alpha_mode_context(
            operation,
            self.source_mode,
            self.destination_mode,
        ))
    }
}

trait UnsignedAlphaSample: Copy {
    const MAX: u64;

    fn into_u64(self) -> u64;
    fn from_u64(value: u64) -> Self;
}

trait FloatAlphaBits: Copy {
    fn one() -> Self;
    fn into_f32(self) -> f32;
    fn from_f32(value: f32) -> Self;
}

impl FloatAlphaBits for u16 {
    fn one() -> Self {
        f16::from_f32(1.0).to_bits()
    }

    fn into_f32(self) -> f32 {
        f16::from_bits(self).to_f32()
    }

    fn from_f32(value: f32) -> Self {
        f16::from_f32(value).to_bits()
    }
}

impl FloatAlphaBits for u32 {
    fn one() -> Self {
        1.0_f32.to_bits()
    }

    fn into_f32(self) -> f32 {
        f32::from_bits(self)
    }

    fn from_f32(value: f32) -> Self {
        value.to_bits()
    }
}

impl UnsignedAlphaSample for u8 {
    const MAX: u64 = u8::MAX as u64;

    fn into_u64(self) -> u64 {
        u64::from(self)
    }

    fn from_u64(value: u64) -> Self {
        u8::try_from(value).expect("normalized u8 alpha conversion stays in range")
    }
}

impl UnsignedAlphaSample for u16 {
    const MAX: u64 = u16::MAX as u64;

    fn into_u64(self) -> u64 {
        u64::from(self)
    }

    fn from_u64(value: u64) -> Self {
        u16::try_from(value).expect("normalized u16 alpha conversion stays in range")
    }
}

fn resolve_alpha(
    channels: &ChannelList,
    color: &ChannelName,
    standard: StandardChannel,
) -> Option<ChannelIndex> {
    let candidates: &[&str] = match standard {
        StandardChannel::Red => &["AR", "A"],
        StandardChannel::Green => &["AG", "A"],
        StandardChannel::Blue => &["AB", "A"],
        StandardChannel::Luminance => &["A"],
        _ => return None,
    };
    let mut layer = Some(color.layer().clone());
    while let Some(current) = layer {
        for candidate in candidates {
            let name = ChannelName::in_layer(current.clone(), candidate)
                .expect("existing layer paths and standard alpha names are valid");
            if let Some(index) = channels.index_of(name.as_str()) {
                return Some(index);
            }
        }
        layer = current.parent();
    }
    None
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn sample_count_error(operation: &'static str, actual: usize, channels: usize) -> Error {
    invalid(
        operation,
        "alpha sample storage must contain complete logical pixels",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "sample_count")
            .with_field("actual_samples", actual.to_string())
            .with_field("channels_per_pixel", channels.to_string()),
    )
}

fn alpha_mode_context(
    operation: &'static str,
    source: AlphaMode,
    destination: AlphaMode,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
        .with_field("source_alpha_mode", source.code())
        .with_field("destination_alpha_mode", destination.code())
}
