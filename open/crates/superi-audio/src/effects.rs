//! Prepared sample-accurate equalization, dynamics, limiting, delay, and saturation.
//!
//! Constructors validate immutable configuration and allocate all channel state. Processing then
//! preserves the exact interleaved channel order and block timing without allocating or locking.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::ChannelLayout;

use crate::graph::{AudioProcessBlock, AudioProcessor};

const COMPONENT: &str = "superi-audio.effects";
const MIN_DB: f32 = -120.0;
const MAX_DB: f32 = 120.0;

/// A supported equalizer filter shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EqualizerBandKind {
    /// Attenuates frequencies above the cutoff.
    LowPass,
    /// Attenuates frequencies below the cutoff.
    HighPass,
    /// Boosts or cuts a region around the center frequency.
    Peaking,
    /// Boosts or cuts frequencies below the transition.
    LowShelf,
    /// Boosts or cuts frequencies above the transition.
    HighShelf,
}

/// One immutable equalizer band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EqualizerBand {
    kind: EqualizerBandKind,
    frequency_hz: f32,
    shape: f32,
    gain_db: f32,
}

impl EqualizerBand {
    /// Creates a low-pass band with a positive quality factor.
    pub fn low_pass(frequency_hz: f32, quality: f32) -> Result<Self> {
        Self::new(EqualizerBandKind::LowPass, frequency_hz, quality, 0.0)
    }

    /// Creates a high-pass band with a positive quality factor.
    pub fn high_pass(frequency_hz: f32, quality: f32) -> Result<Self> {
        Self::new(EqualizerBandKind::HighPass, frequency_hz, quality, 0.0)
    }

    /// Creates a peaking band with center frequency, quality, and decibel gain.
    pub fn peaking(frequency_hz: f32, quality: f32, gain_db: f32) -> Result<Self> {
        Self::new(EqualizerBandKind::Peaking, frequency_hz, quality, gain_db)
    }

    /// Creates a low shelf whose shape is a finite positive slope no greater than one.
    pub fn low_shelf(frequency_hz: f32, slope: f32, gain_db: f32) -> Result<Self> {
        Self::new(EqualizerBandKind::LowShelf, frequency_hz, slope, gain_db)
    }

    /// Creates a high shelf whose shape is a finite positive slope no greater than one.
    pub fn high_shelf(frequency_hz: f32, slope: f32, gain_db: f32) -> Result<Self> {
        Self::new(EqualizerBandKind::HighShelf, frequency_hz, slope, gain_db)
    }

    fn new(kind: EqualizerBandKind, frequency_hz: f32, shape: f32, gain_db: f32) -> Result<Self> {
        if !frequency_hz.is_finite() || frequency_hz <= 0.0 {
            return Err(invalid(
                "create_equalizer_band",
                "band frequency must be finite and positive",
            ));
        }
        if !shape.is_finite() || shape <= 0.0 {
            return Err(invalid(
                "create_equalizer_band",
                "band shape must be finite and positive",
            ));
        }
        if matches!(
            kind,
            EqualizerBandKind::LowShelf | EqualizerBandKind::HighShelf
        ) && shape > 1.0
        {
            return Err(invalid(
                "create_equalizer_band",
                "shelf slope must not exceed one",
            ));
        }
        if !matches!(
            kind,
            EqualizerBandKind::LowShelf | EqualizerBandKind::HighShelf
        ) && shape > 100.0
        {
            return Err(invalid(
                "create_equalizer_band",
                "filter quality must not exceed one hundred",
            ));
        }
        validate_db(gain_db, "create_equalizer_band", "band gain")?;
        Ok(Self {
            kind,
            frequency_hz,
            shape,
            gain_db,
        })
    }

    /// Returns the filter shape.
    #[must_use]
    pub const fn kind(self) -> EqualizerBandKind {
        self.kind
    }

    /// Returns the cutoff or center frequency in hertz.
    #[must_use]
    pub const fn frequency_hz(self) -> f32 {
        self.frequency_hz
    }

    /// Returns quality for pass and peaking filters or slope for shelves.
    #[must_use]
    pub const fn shape(self) -> f32 {
        self.shape
    }

    /// Returns gain in decibels for peaking and shelf filters.
    #[must_use]
    pub const fn gain_db(self) -> f32 {
        self.gain_db
    }
}

#[derive(Clone, Copy, Debug)]
struct BiquadCoefficients {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct BiquadState {
    z1: f64,
    z2: f64,
}

/// A prepared cascade of channel-identical equalizer bands.
#[derive(Debug)]
pub struct Equalizer {
    layout: ChannelLayout,
    sample_rate: u32,
    coefficients: Vec<BiquadCoefficients>,
    states: Vec<BiquadState>,
}

impl Equalizer {
    /// Prepares all coefficients and per-channel state outside processing.
    pub fn new(layout: ChannelLayout, sample_rate: u32, bands: Vec<EqualizerBand>) -> Result<Self> {
        if sample_rate == 0 {
            return Err(invalid(
                "prepare_equalizer",
                "sample rate must be greater than zero",
            ));
        }
        let nyquist = sample_rate as f32 * 0.5;
        if bands.iter().any(|band| band.frequency_hz >= nyquist) {
            return Err(invalid(
                "prepare_equalizer",
                "every band frequency must be below Nyquist",
            ));
        }
        let coefficients = bands
            .iter()
            .map(|band| coefficients(*band, sample_rate))
            .collect::<Result<Vec<_>>>()?;
        let state_count = coefficients
            .len()
            .checked_mul(layout.len())
            .ok_or_else(|| invalid("prepare_equalizer", "equalizer state length overflowed"))?;
        Ok(Self {
            layout,
            sample_rate,
            coefficients,
            states: vec![BiquadState::default(); state_count],
        })
    }
}

impl AudioProcessor for Equalizer {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let input = validate_block(&block, &self.layout, self.sample_rate)?;
        let channels = self.layout.len();
        for (frame_input, frame_output) in input
            .chunks_exact(channels)
            .zip(block.output.chunks_exact_mut(channels))
        {
            for channel in 0..channels {
                let mut sample = f64::from(frame_input[channel]);
                for (band, coefficients) in self.coefficients.iter().enumerate() {
                    let state = &mut self.states[band * channels + channel];
                    let output = coefficients.b0 * sample + state.z1;
                    state.z1 =
                        scrub(coefficients.b1 * sample - coefficients.a1 * output + state.z2);
                    state.z2 = scrub(coefficients.b2 * sample - coefficients.a2 * output);
                    sample = output;
                }
                let sample = sample as f32;
                if !sample.is_finite() {
                    return Err(internal(
                        "process_equalizer",
                        "equalizer produced a non-finite sample",
                    ));
                }
                frame_output[channel] = sample;
            }
        }
        Ok(())
    }
}

/// Immutable compressor parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressorConfig {
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    attack_seconds: f32,
    release_seconds: f32,
    makeup_db: f32,
}

impl CompressorConfig {
    /// Creates linked-channel compressor parameters.
    pub fn new(
        threshold_db: f32,
        ratio: f32,
        knee_db: f32,
        attack_seconds: f32,
        release_seconds: f32,
        makeup_db: f32,
    ) -> Result<Self> {
        validate_db(threshold_db, "create_compressor_config", "threshold")?;
        validate_db(makeup_db, "create_compressor_config", "makeup gain")?;
        if !ratio.is_finite() || !(1.0..=100.0).contains(&ratio) {
            return Err(invalid(
                "create_compressor_config",
                "ratio must be finite and between one and one hundred",
            ));
        }
        if !knee_db.is_finite() || !(0.0..=60.0).contains(&knee_db) {
            return Err(invalid(
                "create_compressor_config",
                "knee must be finite and between zero and sixty decibels",
            ));
        }
        validate_seconds(attack_seconds, "create_compressor_config", "attack")?;
        validate_seconds(release_seconds, "create_compressor_config", "release")?;
        Ok(Self {
            threshold_db,
            ratio,
            knee_db,
            attack_seconds,
            release_seconds,
            makeup_db,
        })
    }
}

/// A prepared linked-channel feed-forward compressor.
#[derive(Debug)]
pub struct Compressor {
    layout: ChannelLayout,
    sample_rate: u32,
    config: CompressorConfig,
    attack_coefficient: f32,
    release_coefficient: f32,
    envelope: f32,
}

impl Compressor {
    /// Prepares a compressor for one unchanged layout and sample clock.
    pub fn new(layout: ChannelLayout, sample_rate: u32, config: CompressorConfig) -> Result<Self> {
        validate_rate(sample_rate, "prepare_compressor")?;
        Ok(Self {
            layout,
            sample_rate,
            config,
            attack_coefficient: time_coefficient(config.attack_seconds, sample_rate),
            release_coefficient: time_coefficient(config.release_seconds, sample_rate),
            envelope: 0.0,
        })
    }
}

impl AudioProcessor for Compressor {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let input = validate_block(&block, &self.layout, self.sample_rate)?;
        let channels = self.layout.len();
        for (frame_input, frame_output) in input
            .chunks_exact(channels)
            .zip(block.output.chunks_exact_mut(channels))
        {
            let peak = frame_input
                .iter()
                .fold(0.0_f32, |value, sample| value.max(sample.abs()));
            let coefficient = if peak > self.envelope {
                self.attack_coefficient
            } else {
                self.release_coefficient
            };
            self.envelope = coefficient * self.envelope + (1.0 - coefficient) * peak;
            let gain = compressor_gain(self.envelope, self.config);
            for (output, input) in frame_output.iter_mut().zip(frame_input) {
                let compressed = *input * gain;
                if !compressed.is_finite() {
                    return Err(internal(
                        "process_compressor",
                        "compressor produced a non-finite sample",
                    ));
                }
                *output = compressed;
            }
        }
        Ok(())
    }
}

/// Immutable peak-limiter parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimiterConfig {
    ceiling_db: f32,
    release_seconds: f32,
}

impl LimiterConfig {
    /// Creates a ceiling and release pair for a zero-lookahead peak limiter.
    pub fn new(ceiling_db: f32, release_seconds: f32) -> Result<Self> {
        if !ceiling_db.is_finite() || !(MIN_DB..=0.0).contains(&ceiling_db) {
            return Err(invalid(
                "create_limiter_config",
                "ceiling must be finite and no greater than zero decibels",
            ));
        }
        validate_seconds(release_seconds, "create_limiter_config", "release")?;
        Ok(Self {
            ceiling_db,
            release_seconds,
        })
    }
}

/// A prepared linked-channel zero-lookahead peak limiter.
#[derive(Debug)]
pub struct PeakLimiter {
    layout: ChannelLayout,
    sample_rate: u32,
    ceiling: f32,
    release_coefficient: f32,
    gain: f32,
}

impl PeakLimiter {
    /// Prepares a limiter for one unchanged layout and sample clock.
    pub fn new(layout: ChannelLayout, sample_rate: u32, config: LimiterConfig) -> Result<Self> {
        validate_rate(sample_rate, "prepare_limiter")?;
        Ok(Self {
            layout,
            sample_rate,
            ceiling: db_to_linear(config.ceiling_db),
            release_coefficient: time_coefficient(config.release_seconds, sample_rate),
            gain: 1.0,
        })
    }
}

impl AudioProcessor for PeakLimiter {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let input = validate_block(&block, &self.layout, self.sample_rate)?;
        let channels = self.layout.len();
        for (frame_input, frame_output) in input
            .chunks_exact(channels)
            .zip(block.output.chunks_exact_mut(channels))
        {
            let peak = frame_input
                .iter()
                .fold(0.0_f32, |value, sample| value.max(sample.abs()));
            let required = if peak > self.ceiling {
                self.ceiling / peak
            } else {
                1.0
            };
            self.gain = if required < self.gain {
                required
            } else {
                self.release_coefficient * self.gain + (1.0 - self.release_coefficient) * required
            };
            for (output, input) in frame_output.iter_mut().zip(frame_input) {
                let limited = *input * self.gain;
                if !limited.is_finite() {
                    return Err(internal(
                        "process_limiter",
                        "limiter produced a non-finite sample",
                    ));
                }
                *output = limited;
            }
        }
        Ok(())
    }
}

/// Immutable fixed-delay parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DelayConfig {
    delay_frames: usize,
    feedback: f32,
    wet_mix: f32,
}

impl DelayConfig {
    /// Creates a positive fixed delay with bounded feedback and wet mix.
    pub fn new(delay_frames: usize, feedback: f32, wet_mix: f32) -> Result<Self> {
        if delay_frames == 0 {
            return Err(invalid(
                "create_delay_config",
                "delay must contain at least one frame",
            ));
        }
        if !feedback.is_finite() || !(-0.999..=0.999).contains(&feedback) {
            return Err(invalid(
                "create_delay_config",
                "feedback must be finite with magnitude below one",
            ));
        }
        validate_mix(wet_mix, "create_delay_config")?;
        Ok(Self {
            delay_frames,
            feedback,
            wet_mix,
        })
    }
}

/// A prepared channel-preserving fixed delay.
#[derive(Debug)]
pub struct DelayEffect {
    layout: ChannelLayout,
    config: DelayConfig,
    ring: Vec<f32>,
    cursor: usize,
}

impl DelayEffect {
    /// Allocates the complete interleaved delay line outside processing.
    pub fn new(layout: ChannelLayout, config: DelayConfig) -> Result<Self> {
        let samples = config
            .delay_frames
            .checked_mul(layout.len())
            .ok_or_else(|| invalid("prepare_delay", "delay storage length overflowed"))?;
        Ok(Self {
            layout,
            config,
            ring: vec![0.0; samples],
            cursor: 0,
        })
    }
}

impl AudioProcessor for DelayEffect {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let input = validate_layout_block(&block, &self.layout)?;
        let dry_mix = 1.0 - self.config.wet_mix;
        for (input, output) in input.iter().zip(block.output.iter_mut()) {
            let delayed = self.ring[self.cursor];
            let next = *input + delayed * self.config.feedback;
            let mixed = *input * dry_mix + delayed * self.config.wet_mix;
            if !next.is_finite() || !mixed.is_finite() {
                return Err(internal(
                    "process_delay",
                    "delay produced a non-finite sample",
                ));
            }
            self.ring[self.cursor] = next;
            *output = mixed;
            self.cursor += 1;
            if self.cursor == self.ring.len() {
                self.cursor = 0;
            }
        }
        Ok(())
    }
}

/// Immutable normalized soft-saturation parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SaturatorConfig {
    drive: f32,
    wet_mix: f32,
}

impl SaturatorConfig {
    /// Creates finite positive drive and a bounded wet mix.
    pub fn new(drive: f32, wet_mix: f32) -> Result<Self> {
        if !drive.is_finite() || !(0.001..=100.0).contains(&drive) {
            return Err(invalid(
                "create_saturator_config",
                "drive must be finite and within the supported range",
            ));
        }
        validate_mix(wet_mix, "create_saturator_config")?;
        Ok(Self { drive, wet_mix })
    }
}

/// A prepared memoryless normalized soft saturator.
#[derive(Debug)]
pub struct Saturator {
    layout: ChannelLayout,
    config: SaturatorConfig,
    normalization: f32,
}

impl Saturator {
    /// Prepares the normalization factor outside processing.
    pub fn new(layout: ChannelLayout, config: SaturatorConfig) -> Result<Self> {
        Ok(Self {
            layout,
            config,
            normalization: config.drive.tanh().recip(),
        })
    }
}

impl AudioProcessor for Saturator {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let input = validate_layout_block(&block, &self.layout)?;
        let dry_mix = 1.0 - self.config.wet_mix;
        for (input, output) in input.iter().zip(block.output.iter_mut()) {
            let saturated =
                ((self.config.drive * *input).tanh() * self.normalization).clamp(-1.0, 1.0);
            let mixed = *input * dry_mix + saturated * self.config.wet_mix;
            if !mixed.is_finite() {
                return Err(internal(
                    "process_saturator",
                    "saturator produced a non-finite sample",
                ));
            }
            *output = mixed;
        }
        Ok(())
    }
}

fn validate_block<'a>(
    block: &AudioProcessBlock<'a>,
    layout: &ChannelLayout,
    sample_rate: u32,
) -> Result<&'a [f32]> {
    if block.start_time.sample_rate() != sample_rate {
        return Err(invalid(
            "process_effect",
            "block sample rate does not match the prepared effect",
        ));
    }
    validate_layout_block(block, layout)
}

fn validate_layout_block<'a>(
    block: &AudioProcessBlock<'a>,
    layout: &ChannelLayout,
) -> Result<&'a [f32]> {
    let input = block.input.ok_or_else(|| {
        invalid(
            "process_effect",
            "audio effect requires one connected input",
        )
    })?;
    if block.input_layout != Some(layout) || block.output_layout != layout {
        return Err(invalid(
            "process_effect",
            "block layout does not match the prepared effect",
        ));
    }
    let samples = block
        .frame_count
        .checked_mul(layout.len())
        .ok_or_else(|| invalid("process_effect", "effect block length overflowed"))?;
    if input.len() != samples || block.output.len() != samples {
        return Err(invalid(
            "process_effect",
            "effect buffers do not match the frame count",
        ));
    }
    if input.iter().any(|sample| !sample.is_finite()) {
        return Err(invalid(
            "process_effect",
            "effect input samples must be finite",
        ));
    }
    Ok(input)
}

fn coefficients(band: EqualizerBand, sample_rate: u32) -> Result<BiquadCoefficients> {
    let omega = std::f64::consts::TAU * f64::from(band.frequency_hz) / f64::from(sample_rate);
    let cosine = omega.cos();
    let sine = omega.sin();
    let shape = f64::from(band.shape);
    let gain = 10.0_f64.powf(f64::from(band.gain_db) / 40.0);
    let alpha = match band.kind {
        EqualizerBandKind::LowShelf | EqualizerBandKind::HighShelf => {
            sine * 0.5 * ((gain + gain.recip()) * (shape.recip() - 1.0) + 2.0).sqrt()
        }
        _ => sine / (2.0 * shape),
    };
    let two_sqrt_gain_alpha = 2.0 * gain.sqrt() * alpha;
    let (b0, b1, b2, a0, a1, a2) = match band.kind {
        EqualizerBandKind::LowPass => (
            (1.0 - cosine) * 0.5,
            1.0 - cosine,
            (1.0 - cosine) * 0.5,
            1.0 + alpha,
            -2.0 * cosine,
            1.0 - alpha,
        ),
        EqualizerBandKind::HighPass => (
            (1.0 + cosine) * 0.5,
            -(1.0 + cosine),
            (1.0 + cosine) * 0.5,
            1.0 + alpha,
            -2.0 * cosine,
            1.0 - alpha,
        ),
        EqualizerBandKind::Peaking => (
            1.0 + alpha * gain,
            -2.0 * cosine,
            1.0 - alpha * gain,
            1.0 + alpha / gain,
            -2.0 * cosine,
            1.0 - alpha / gain,
        ),
        EqualizerBandKind::LowShelf => (
            gain * ((gain + 1.0) - (gain - 1.0) * cosine + two_sqrt_gain_alpha),
            2.0 * gain * ((gain - 1.0) - (gain + 1.0) * cosine),
            gain * ((gain + 1.0) - (gain - 1.0) * cosine - two_sqrt_gain_alpha),
            (gain + 1.0) + (gain - 1.0) * cosine + two_sqrt_gain_alpha,
            -2.0 * ((gain - 1.0) + (gain + 1.0) * cosine),
            (gain + 1.0) + (gain - 1.0) * cosine - two_sqrt_gain_alpha,
        ),
        EqualizerBandKind::HighShelf => (
            gain * ((gain + 1.0) + (gain - 1.0) * cosine + two_sqrt_gain_alpha),
            -2.0 * gain * ((gain - 1.0) + (gain + 1.0) * cosine),
            gain * ((gain + 1.0) + (gain - 1.0) * cosine - two_sqrt_gain_alpha),
            (gain + 1.0) - (gain - 1.0) * cosine + two_sqrt_gain_alpha,
            2.0 * ((gain - 1.0) - (gain + 1.0) * cosine),
            (gain + 1.0) - (gain - 1.0) * cosine - two_sqrt_gain_alpha,
        ),
    };
    let values = [b0, b1, b2, a0, a1, a2];
    if values.iter().any(|value| !value.is_finite()) || a0.abs() < f64::EPSILON {
        return Err(invalid(
            "prepare_equalizer",
            "equalizer coefficients are not stable finite values",
        ));
    }
    Ok(BiquadCoefficients {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    })
}

fn compressor_gain(envelope: f32, config: CompressorConfig) -> f32 {
    if envelope <= f32::EPSILON {
        return db_to_linear(config.makeup_db);
    }
    let input_db = 20.0 * envelope.log10();
    let delta = input_db - config.threshold_db;
    let over_db = if config.knee_db == 0.0 {
        delta.max(0.0)
    } else if delta <= -config.knee_db * 0.5 {
        0.0
    } else if delta >= config.knee_db * 0.5 {
        delta
    } else {
        let knee_position = delta + config.knee_db * 0.5;
        knee_position * knee_position / (2.0 * config.knee_db)
    };
    db_to_linear(config.makeup_db - over_db * (1.0 - config.ratio.recip()))
}

fn validate_rate(sample_rate: u32, operation: &'static str) -> Result<()> {
    if sample_rate == 0 {
        return Err(invalid(operation, "sample rate must be greater than zero"));
    }
    Ok(())
}

fn validate_db(value: f32, operation: &'static str, name: &'static str) -> Result<()> {
    if !value.is_finite() || !(MIN_DB..=MAX_DB).contains(&value) {
        return Err(invalid(operation, name));
    }
    Ok(())
}

fn validate_seconds(value: f32, operation: &'static str, name: &'static str) -> Result<()> {
    if !value.is_finite() || !(0.0..=60.0).contains(&value) {
        return Err(invalid(operation, name));
    }
    Ok(())
}

fn validate_mix(value: f32, operation: &'static str) -> Result<()> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(invalid(
            operation,
            "wet mix must be finite and between zero and one",
        ));
    }
    Ok(())
}

fn time_coefficient(seconds: f32, sample_rate: u32) -> f32 {
    if seconds == 0.0 {
        0.0
    } else {
        (-1.0 / (seconds * sample_rate as f32)).exp()
    }
}

fn db_to_linear(decibels: f32) -> f32 {
    10.0_f32.powf(decibels / 20.0)
}

fn scrub(value: f64) -> f64 {
    if value.abs() < 1.0e-30 {
        0.0
    } else {
        value
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
