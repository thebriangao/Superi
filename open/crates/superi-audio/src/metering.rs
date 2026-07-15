//! Prepared, graph-native audio metering and analysis.
//!
//! [`PreparedMeter`] is a transparent single-input [`crate::graph::AudioProcessor`] that preserves
//! every interleaved sample while measuring levels and analysis data. All variable storage is
//! allocated by [`PreparedMeter::new`] outside the callback. The callback publishes into bounded
//! atomics; [`MeterReadings::snapshot`] builds an owned control-side view and performs the
//! programme-loudness gate scan without locking the audio thread.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::SampleTime;

use crate::graph::{AudioProcessBlock, AudioProcessor};

const COMPONENT: &str = "superi-audio.metering";
const MAX_CHANNELS: usize = 64;
const MAX_SPECTRUM_FRAMES: usize = 4_096;
const MAX_SPECTRUM_BINS: usize = 256;
const MAX_SPECTRUM_PRODUCTS: usize = 262_144;
const MAX_PROGRAMME_BLOCKS: usize = 1_000_000;
const TRUE_PEAK_TAPS: usize = 12;
const TRUE_PEAK_PHASES: usize = 4;
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
const LOUDNESS_OFFSET: f64 = -0.691;

/// Bounded preparation policy for one exact audio meter placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeterConfig {
    sample_rate: u32,
    channel_layout: ChannelLayout,
    maximum_frames: usize,
    maximum_programme_blocks: usize,
    spectrum_frames: usize,
    spectrum_bins: usize,
}

impl MeterConfig {
    /// Creates a meter with explicit callback and integrated-history bounds.
    pub fn new(
        sample_rate: u32,
        channel_layout: ChannelLayout,
        maximum_frames: usize,
        maximum_programme_blocks: usize,
    ) -> Result<Self> {
        if sample_rate == 0 || sample_rate > 768_000 {
            return Err(invalid(
                "create_meter_config",
                "meter sample rate must be between 1 and 768000 Hz",
            ));
        }
        if channel_layout.len() > MAX_CHANNELS {
            return Err(invalid(
                "create_meter_config",
                "meter channel layout exceeds the supported channel bound",
            ));
        }
        if maximum_frames == 0 {
            return Err(invalid(
                "create_meter_config",
                "meter maximum frame count must be greater than zero",
            ));
        }
        if maximum_programme_blocks == 0 || maximum_programme_blocks > MAX_PROGRAMME_BLOCKS {
            return Err(invalid(
                "create_meter_config",
                "meter programme history must contain between 1 and 1000000 gating blocks",
            ));
        }
        let spectrum_frames = maximum_frames.clamp(16, 256);
        let spectrum_bins = (spectrum_frames / 2 + 1).min(64);
        Ok(Self {
            sample_rate,
            channel_layout,
            maximum_frames,
            maximum_programme_blocks,
            spectrum_frames,
            spectrum_bins,
        })
    }

    /// Replaces the bounded analysis window and number of published bins.
    pub fn with_spectrum(mut self, frames: usize, bins: usize) -> Result<Self> {
        if !(16..=MAX_SPECTRUM_FRAMES).contains(&frames)
            || !(2..=MAX_SPECTRUM_BINS).contains(&bins)
            || bins > frames / 2 + 1
            || frames.saturating_mul(bins) > MAX_SPECTRUM_PRODUCTS
        {
            return Err(invalid(
                "configure_spectrum",
                "spectrum dimensions are outside the prepared analysis bounds",
            ));
        }
        self.spectrum_frames = frames;
        self.spectrum_bins = bins;
        Ok(self)
    }

    /// Returns the exact sample clock.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns channels in exact routing order.
    #[must_use]
    pub const fn channel_layout(&self) -> &ChannelLayout {
        &self.channel_layout
    }

    /// Returns the prepared callback frame bound.
    #[must_use]
    pub const fn maximum_frames(&self) -> usize {
        self.maximum_frames
    }

    /// Returns the retained 400 ms programme block bound.
    #[must_use]
    pub const fn maximum_programme_blocks(&self) -> usize {
        self.maximum_programme_blocks
    }
}

/// One channel's instantaneous and held level measurements.
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelMeter {
    position: ChannelPosition,
    sample_peak: f64,
    rms: f64,
    true_peak: f64,
    maximum_true_peak: f64,
}

impl ChannelMeter {
    /// Returns the semantic channel position.
    #[must_use]
    pub const fn position(&self) -> ChannelPosition {
        self.position
    }

    /// Returns the largest absolute PCM sample in the latest block.
    #[must_use]
    pub const fn sample_peak(&self) -> f64 {
        self.sample_peak
    }

    /// Returns the root mean square level of the latest block.
    #[must_use]
    pub const fn rms(&self) -> f64 {
        self.rms
    }

    /// Returns the four-times oversampled true-peak estimate for the latest block.
    #[must_use]
    pub const fn true_peak(&self) -> f64 {
        self.true_peak
    }

    /// Returns the maximum true-peak estimate since preparation.
    #[must_use]
    pub const fn maximum_true_peak(&self) -> f64 {
        self.maximum_true_peak
    }
}

/// One deterministic magnitude-spectrum result.
#[derive(Clone, Debug, PartialEq)]
pub struct SpectrumBin {
    center_hz: f64,
    magnitude: f64,
}

impl SpectrumBin {
    /// Returns the represented frequency.
    #[must_use]
    pub const fn center_hz(&self) -> f64 {
        self.center_hz
    }

    /// Returns the linear Hann-window-corrected magnitude.
    #[must_use]
    pub const fn magnitude(&self) -> f64 {
        self.magnitude
    }
}

/// One coherent control-side view of the latest completed meter block.
#[derive(Clone, Debug, PartialEq)]
pub struct MeterSnapshot {
    start_time: SampleTime,
    frame_count: usize,
    channels: Vec<ChannelMeter>,
    phase_correlation: Option<f64>,
    spectrum: Vec<SpectrumBin>,
    momentary_lufs: Option<f64>,
    short_term_lufs: Option<f64>,
    integrated_lufs: Option<f64>,
    programme_blocks: usize,
    programme_history_saturated: bool,
}

impl MeterSnapshot {
    /// Returns the exact first sample represented by instantaneous values.
    #[must_use]
    pub const fn start_time(&self) -> SampleTime {
        self.start_time
    }

    /// Returns the latest complete frame count.
    #[must_use]
    pub const fn frame_count(&self) -> usize {
        self.frame_count
    }

    /// Returns channel meters in exact layout order.
    #[must_use]
    pub fn channels(&self) -> &[ChannelMeter] {
        &self.channels
    }

    /// Returns stereo correlation from -1 through 1, or `None` without two energized channels.
    #[must_use]
    pub const fn phase_correlation(&self) -> Option<f64> {
        self.phase_correlation
    }

    /// Returns deterministic spectrum bins in ascending frequency order.
    #[must_use]
    pub fn spectrum(&self) -> &[SpectrumBin] {
        &self.spectrum
    }

    /// Returns EBU 400 ms ungated momentary loudness when enough samples exist.
    #[must_use]
    pub const fn momentary_lufs(&self) -> Option<f64> {
        self.momentary_lufs
    }

    /// Returns EBU 3 s ungated short-term loudness when enough samples exist.
    #[must_use]
    pub const fn short_term_lufs(&self) -> Option<f64> {
        self.short_term_lufs
    }

    /// Returns ITU-R BS.1770 integrated loudness over retained complete gating blocks.
    #[must_use]
    pub const fn integrated_lufs(&self) -> Option<f64> {
        self.integrated_lufs
    }

    /// Returns the number of complete retained 400 ms gating blocks.
    #[must_use]
    pub const fn programme_blocks(&self) -> usize {
        self.programme_blocks
    }

    /// Returns whether later programme blocks exceeded the explicit history bound.
    #[must_use]
    pub const fn programme_history_saturated(&self) -> bool {
        self.programme_history_saturated
    }
}

/// Lock-free reader for one prepared meter's published analysis.
#[derive(Clone)]
pub struct MeterReadings {
    shared: Arc<SharedReadings>,
    sample_rate: u32,
    positions: Box<[ChannelPosition]>,
    spectrum_hz: Box<[f64]>,
}

impl MeterReadings {
    /// Builds one owned coherent snapshot outside the audio callback.
    #[must_use]
    pub fn snapshot(&self) -> MeterSnapshot {
        loop {
            let before = self.shared.generation.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let start_sample = self.shared.start_sample.load(Ordering::Relaxed);
            let frame_count = self.shared.frame_count.load(Ordering::Relaxed);
            let channels = self
                .positions
                .iter()
                .enumerate()
                .map(|(index, position)| ChannelMeter {
                    position: *position,
                    sample_peak: self.shared.channel_sample_peak[index].load(),
                    rms: self.shared.channel_rms[index].load(),
                    true_peak: self.shared.channel_true_peak[index].load(),
                    maximum_true_peak: self.shared.channel_maximum_true_peak[index].load(),
                })
                .collect();
            let phase = option_from_atomic(&self.shared.phase_correlation);
            let spectrum = self
                .spectrum_hz
                .iter()
                .enumerate()
                .map(|(index, center_hz)| SpectrumBin {
                    center_hz: *center_hz,
                    magnitude: self.shared.spectrum[index].load(),
                })
                .collect();
            let momentary = option_from_atomic(&self.shared.momentary_lufs);
            let short_term = option_from_atomic(&self.shared.short_term_lufs);
            let after = self.shared.generation.load(Ordering::Acquire);
            if before != after {
                continue;
            }

            let programme_blocks = self.shared.programme_count.load(Ordering::Acquire);
            let integrated_lufs =
                integrated_loudness(&self.shared.programme_energy[..programme_blocks]);
            return MeterSnapshot {
                start_time: SampleTime::new(start_sample, self.sample_rate)
                    .expect("prepared meter sample rate remains valid"),
                frame_count,
                channels,
                phase_correlation: phase,
                spectrum,
                momentary_lufs: momentary,
                short_term_lufs: short_term,
                integrated_lufs,
                programme_blocks,
                programme_history_saturated: self
                    .shared
                    .programme_saturated
                    .load(Ordering::Acquire),
            };
        }
    }
}

/// Transparent, preallocated audio-graph meter.
pub struct PreparedMeter {
    config: MeterConfig,
    shared: Arc<SharedReadings>,
    k_filters: Vec<KWeightFilter>,
    loudness_weights: Vec<f64>,
    loudness_ring: Vec<f64>,
    loudness_next: usize,
    loudness_filled: usize,
    loudness_total: u64,
    momentary_frames: usize,
    short_term_frames: usize,
    gate_step_frames: usize,
    momentary_sum: f64,
    short_term_sum: f64,
    true_peak_history: Vec<f64>,
    true_peak_next: usize,
    maximum_true_peak: Vec<f64>,
    spectrum_ring: Vec<f64>,
    spectrum_next: usize,
    spectrum_filled: usize,
    spectrum_window: Vec<f64>,
    spectrum_twiddles: Vec<(f64, f64)>,
}

impl PreparedMeter {
    /// Preallocates DSP and publication storage and returns its independent reader.
    pub fn new(config: MeterConfig) -> Result<(Self, MeterReadings)> {
        let channels = config.channel_layout.len();
        let momentary_frames = window_frames(config.sample_rate, 2, 5)?;
        let short_term_frames = usize::try_from(config.sample_rate)
            .ok()
            .and_then(|rate| rate.checked_mul(3))
            .ok_or_else(|| resource("prepare_meter", "short-term meter storage overflowed"))?;
        let gate_step_frames = window_frames(config.sample_rate, 1, 10)?;
        let shared = Arc::new(SharedReadings::new(
            channels,
            config.spectrum_bins,
            config.maximum_programme_blocks,
        )?);
        let positions = config
            .channel_layout
            .positions()
            .to_vec()
            .into_boxed_slice();
        let spectrum_indices = spectrum_indices(config.spectrum_frames, config.spectrum_bins);
        let spectrum_hz = spectrum_indices
            .iter()
            .map(|index| {
                *index as f64 * f64::from(config.sample_rate) / config.spectrum_frames as f64
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let readings = MeterReadings {
            shared: Arc::clone(&shared),
            sample_rate: config.sample_rate,
            positions,
            spectrum_hz,
        };
        let spectrum_window = hann_window(config.spectrum_frames);
        let mut spectrum_twiddles = Vec::new();
        spectrum_twiddles
            .try_reserve_exact(config.spectrum_frames * config.spectrum_bins)
            .map_err(|_| resource("prepare_meter", "spectrum coefficient allocation failed"))?;
        for index in &spectrum_indices {
            for sample in 0..config.spectrum_frames {
                let phase = -std::f64::consts::TAU * *index as f64 * sample as f64
                    / config.spectrum_frames as f64;
                spectrum_twiddles.push(phase.sin_cos());
            }
        }
        let k_filters = (0..channels)
            .map(|_| KWeightFilter::new(config.sample_rate))
            .collect();
        let loudness_weights = config
            .channel_layout
            .positions()
            .iter()
            .copied()
            .map(loudness_weight)
            .collect();
        let meter = Self {
            loudness_ring: fallible_zeroed(short_term_frames, "loudness window")?,
            true_peak_history: fallible_zeroed(channels * TRUE_PEAK_TAPS, "true-peak history")?,
            maximum_true_peak: fallible_zeroed(channels, "true-peak hold")?,
            spectrum_ring: fallible_zeroed(config.spectrum_frames, "spectrum window")?,
            config,
            shared,
            k_filters,
            loudness_weights,
            loudness_next: 0,
            loudness_filled: 0,
            loudness_total: 0,
            momentary_frames,
            short_term_frames,
            gate_step_frames,
            momentary_sum: 0.0,
            short_term_sum: 0.0,
            true_peak_next: 0,
            spectrum_next: 0,
            spectrum_filled: 0,
            spectrum_window,
            spectrum_twiddles,
        };
        Ok((meter, readings))
    }

    fn validate_block(&self, block: &AudioProcessBlock<'_>, input: &[f32]) -> Result<()> {
        if block.start_time.sample_rate() != self.config.sample_rate
            || block.input_layout != Some(&self.config.channel_layout)
            || block.output_layout != &self.config.channel_layout
        {
            return Err(invalid(
                "process_meter",
                "meter block does not match its prepared clock and channel layout",
            ));
        }
        if block.frame_count == 0 || block.frame_count > self.config.maximum_frames {
            return Err(invalid(
                "process_meter",
                "meter block frame count is outside the prepared bound",
            ));
        }
        let samples = block
            .frame_count
            .checked_mul(self.config.channel_layout.len())
            .ok_or_else(|| invalid("process_meter", "meter sample count overflowed"))?;
        if input.len() != samples || block.output.len() != samples {
            return Err(invalid(
                "process_meter",
                "meter buffers do not match frame count and channel layout",
            ));
        }
        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(invalid(
                "process_meter",
                "meter input contains a non-finite sample",
            ));
        }
        Ok(())
    }

    fn measure(&mut self, block: &AudioProcessBlock<'_>, input: &[f32]) {
        let channels = self.config.channel_layout.len();
        let mut peak = [0.0_f64; MAX_CHANNELS];
        let mut squares = [0.0_f64; MAX_CHANNELS];
        let mut true_peak = [0.0_f64; MAX_CHANNELS];
        let mut phase_cross = 0.0;
        let mut phase_left = 0.0;
        let mut phase_right = 0.0;

        for frame in input.chunks_exact(channels) {
            let mut loudness_energy = 0.0;
            let mono = frame.iter().map(|sample| f64::from(*sample)).sum::<f64>() / channels as f64;
            self.spectrum_ring[self.spectrum_next] = mono;
            self.spectrum_next = (self.spectrum_next + 1) % self.spectrum_ring.len();
            self.spectrum_filled = (self.spectrum_filled + 1).min(self.spectrum_ring.len());

            for (channel, sample) in frame.iter().copied().enumerate() {
                let sample = f64::from(sample);
                peak[channel] = peak[channel].max(sample.abs());
                squares[channel] += sample * sample;
                self.true_peak_history[channel * TRUE_PEAK_TAPS + self.true_peak_next] = sample;
                let weighted = self.k_filters[channel].process(sample);
                loudness_energy += self.loudness_weights[channel] * weighted * weighted;
            }
            if channels >= 2 {
                let left = f64::from(frame[0]);
                let right = f64::from(frame[1]);
                phase_cross += left * right;
                phase_left += left * left;
                phase_right += right * right;
            }
            for channel in 0..channels {
                for phase in 0..TRUE_PEAK_PHASES {
                    let mut interpolated = 0.0;
                    for (tap, coefficients) in TRUE_PEAK_COEFFICIENTS.iter().enumerate() {
                        let history = (self.true_peak_next + TRUE_PEAK_TAPS - tap) % TRUE_PEAK_TAPS;
                        interpolated += coefficients[phase]
                            * self.true_peak_history[channel * TRUE_PEAK_TAPS + history];
                    }
                    true_peak[channel] = true_peak[channel].max(interpolated.abs());
                }
                true_peak[channel] = true_peak[channel].max(peak[channel]);
            }
            self.true_peak_next = (self.true_peak_next + 1) % TRUE_PEAK_TAPS;
            self.push_loudness_energy(loudness_energy);
        }

        let phase = if channels >= 2 && phase_left > 0.0 && phase_right > 0.0 {
            Some((phase_cross / (phase_left * phase_right).sqrt()).clamp(-1.0, 1.0))
        } else {
            None
        };
        let momentary = (self.loudness_filled >= self.momentary_frames)
            .then(|| loudness(self.momentary_sum / self.momentary_frames as f64));
        let short_term = (self.loudness_filled >= self.short_term_frames)
            .then(|| loudness(self.short_term_sum / self.short_term_frames as f64));

        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared
            .start_sample
            .store(block.start_time.sample(), Ordering::Relaxed);
        self.shared
            .frame_count
            .store(block.frame_count, Ordering::Relaxed);
        for channel in 0..channels {
            self.maximum_true_peak[channel] =
                self.maximum_true_peak[channel].max(true_peak[channel]);
            self.shared.channel_sample_peak[channel].store(peak[channel]);
            self.shared.channel_rms[channel]
                .store((squares[channel] / block.frame_count as f64).sqrt());
            self.shared.channel_true_peak[channel].store(true_peak[channel]);
            self.shared.channel_maximum_true_peak[channel].store(self.maximum_true_peak[channel]);
        }
        store_option(&self.shared.phase_correlation, phase);
        let spectrum = self.calculate_spectrum();
        for (cell, value) in self.shared.spectrum.iter().zip(spectrum) {
            cell.store(value);
        }
        store_option(
            &self.shared.momentary_lufs,
            momentary.filter(|value| value.is_finite()),
        );
        store_option(
            &self.shared.short_term_lufs,
            short_term.filter(|value| value.is_finite()),
        );
        self.shared.generation.fetch_add(1, Ordering::Release);
    }

    fn push_loudness_energy(&mut self, energy: f64) {
        let old_short = if self.loudness_filled == self.short_term_frames {
            self.loudness_ring[self.loudness_next]
        } else {
            0.0
        };
        let old_momentary = if self.loudness_filled >= self.momentary_frames {
            let index = (self.loudness_next + self.short_term_frames - self.momentary_frames)
                % self.short_term_frames;
            self.loudness_ring[index]
        } else {
            0.0
        };
        self.short_term_sum += energy - old_short;
        self.momentary_sum += energy - old_momentary;
        self.loudness_ring[self.loudness_next] = energy;
        self.loudness_next = (self.loudness_next + 1) % self.short_term_frames;
        self.loudness_filled = (self.loudness_filled + 1).min(self.short_term_frames);
        self.loudness_total = self.loudness_total.saturating_add(1);

        if self.loudness_total >= self.momentary_frames as u64
            && (self.loudness_total - self.momentary_frames as u64) % self.gate_step_frames as u64
                == 0
        {
            let index = self.shared.programme_count.load(Ordering::Relaxed);
            if let Some(cell) = self.shared.programme_energy.get(index) {
                cell.store(self.momentary_sum / self.momentary_frames as f64);
                self.shared
                    .programme_count
                    .store(index + 1, Ordering::Release);
            } else {
                self.shared
                    .programme_saturated
                    .store(true, Ordering::Release);
            }
        }
    }

    fn calculate_spectrum(&self) -> impl Iterator<Item = f64> + '_ {
        let window_sum = self.spectrum_window.iter().sum::<f64>();
        (0..self.config.spectrum_bins).map(move |bin| {
            if self.spectrum_filled < self.spectrum_ring.len() {
                return 0.0;
            }
            let mut real = 0.0;
            let mut imaginary = 0.0;
            let twiddles = &self.spectrum_twiddles
                [bin * self.spectrum_ring.len()..(bin + 1) * self.spectrum_ring.len()];
            for (sample, &(sine, cosine)) in twiddles.iter().enumerate() {
                let index = (self.spectrum_next + sample) % self.spectrum_ring.len();
                let value = self.spectrum_ring[index] * self.spectrum_window[sample];
                real += value * cosine;
                imaginary += value * sine;
            }
            let scale = if bin == 0 { 1.0 } else { 2.0 } / window_sum;
            real.hypot(imaginary) * scale
        })
    }
}

impl AudioProcessor for PreparedMeter {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let input = block.input.ok_or_else(|| {
            invalid(
                "process_meter",
                "meter processor requires one connected input",
            )
        })?;
        self.validate_block(&block, input)?;
        block.output.copy_from_slice(input);
        self.measure(&block, input);
        Ok(())
    }
}

struct SharedReadings {
    generation: AtomicU64,
    start_sample: AtomicI64,
    frame_count: AtomicUsize,
    channel_sample_peak: Vec<AtomicFloat>,
    channel_rms: Vec<AtomicFloat>,
    channel_true_peak: Vec<AtomicFloat>,
    channel_maximum_true_peak: Vec<AtomicFloat>,
    phase_correlation: AtomicFloat,
    spectrum: Vec<AtomicFloat>,
    momentary_lufs: AtomicFloat,
    short_term_lufs: AtomicFloat,
    programme_energy: Vec<AtomicFloat>,
    programme_count: AtomicUsize,
    programme_saturated: AtomicBool,
}

impl SharedReadings {
    fn new(channels: usize, spectrum_bins: usize, programme_blocks: usize) -> Result<Self> {
        Ok(Self {
            generation: AtomicU64::new(0),
            start_sample: AtomicI64::new(0),
            frame_count: AtomicUsize::new(0),
            channel_sample_peak: atomic_floats(channels, 0.0)?,
            channel_rms: atomic_floats(channels, 0.0)?,
            channel_true_peak: atomic_floats(channels, 0.0)?,
            channel_maximum_true_peak: atomic_floats(channels, 0.0)?,
            phase_correlation: AtomicFloat::new(f64::NAN),
            spectrum: atomic_floats(spectrum_bins, 0.0)?,
            momentary_lufs: AtomicFloat::new(f64::NAN),
            short_term_lufs: AtomicFloat::new(f64::NAN),
            programme_energy: atomic_floats(programme_blocks, 0.0)?,
            programme_count: AtomicUsize::new(0),
            programme_saturated: AtomicBool::new(false),
        })
    }
}

struct AtomicFloat(AtomicU64);

impl AtomicFloat {
    fn new(value: f64) -> Self {
        Self(AtomicU64::new(value.to_bits()))
    }

    fn load(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn store(&self, value: f64) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn process(&mut self, input: f64) -> f64 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        output
    }
}

struct KWeightFilter {
    shelf: Biquad,
    high_pass: Biquad,
}

impl KWeightFilter {
    fn new(sample_rate: u32) -> Self {
        if sample_rate == 48_000 {
            return Self {
                shelf: Biquad {
                    b0: 1.535_124_859_586_97,
                    b1: -2.691_696_189_406_38,
                    b2: 1.198_392_810_852_85,
                    a1: -1.690_659_293_182_41,
                    a2: 0.732_480_774_215_85,
                    z1: 0.0,
                    z2: 0.0,
                },
                high_pass: Biquad {
                    b0: 1.0,
                    b1: -2.0,
                    b2: 1.0,
                    a1: -1.990_047_454_833_98,
                    a2: 0.990_072_250_366_21,
                    z1: 0.0,
                    z2: 0.0,
                },
            };
        }
        Self {
            shelf: high_shelf(sample_rate),
            high_pass: high_pass(sample_rate),
        }
    }

    fn process(&mut self, input: f64) -> f64 {
        self.high_pass.process(self.shelf.process(input))
    }
}

fn high_shelf(sample_rate: u32) -> Biquad {
    let frequency = 1_681.974_450_955_533;
    let gain = 3.999_843_853_973_347;
    let q = 0.707_175_236_955_419_6;
    let a = 10.0_f64.powf(gain / 40.0);
    let omega = std::f64::consts::TAU * frequency / f64::from(sample_rate);
    let alpha = omega.sin() / (2.0 * q);
    let cosine = omega.cos();
    let root_a = a.sqrt();
    normalize_biquad(
        a * ((a + 1.0) + (a - 1.0) * cosine + 2.0 * root_a * alpha),
        -2.0 * a * ((a - 1.0) + (a + 1.0) * cosine),
        a * ((a + 1.0) + (a - 1.0) * cosine - 2.0 * root_a * alpha),
        (a + 1.0) - (a - 1.0) * cosine + 2.0 * root_a * alpha,
        2.0 * ((a - 1.0) - (a + 1.0) * cosine),
        (a + 1.0) - (a - 1.0) * cosine - 2.0 * root_a * alpha,
    )
}

fn high_pass(sample_rate: u32) -> Biquad {
    let frequency = 38.135_470_876_024_44;
    let q = 0.500_327_037_323_877_3;
    let omega = std::f64::consts::TAU * frequency / f64::from(sample_rate);
    let alpha = omega.sin() / (2.0 * q);
    let cosine = omega.cos();
    normalize_biquad(
        (1.0 + cosine) / 2.0,
        -(1.0 + cosine),
        (1.0 + cosine) / 2.0,
        1.0 + alpha,
        -2.0 * cosine,
        1.0 - alpha,
    )
}

fn normalize_biquad(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Biquad {
    Biquad {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
        z1: 0.0,
        z2: 0.0,
    }
}

fn loudness_weight(position: ChannelPosition) -> f64 {
    match position {
        ChannelPosition::LowFrequency => 0.0,
        ChannelPosition::BackLeft
        | ChannelPosition::BackRight
        | ChannelPosition::SideLeft
        | ChannelPosition::SideRight => 1.41,
        _ => 1.0,
    }
}

fn loudness(energy: f64) -> f64 {
    LOUDNESS_OFFSET + 10.0 * energy.log10()
}

fn integrated_loudness(blocks: &[AtomicFloat]) -> Option<f64> {
    let mut absolute_sum = 0.0;
    let mut absolute_count = 0_u64;
    for block in blocks {
        let energy = block.load();
        if loudness(energy) > ABSOLUTE_GATE_LUFS {
            absolute_sum += energy;
            absolute_count += 1;
        }
    }
    if absolute_count == 0 {
        return None;
    }
    let relative_gate = loudness(absolute_sum / absolute_count as f64) - 10.0;
    let gate = relative_gate.max(ABSOLUTE_GATE_LUFS);
    let mut gated_sum = 0.0;
    let mut gated_count = 0_u64;
    for block in blocks {
        let energy = block.load();
        if loudness(energy) > gate {
            gated_sum += energy;
            gated_count += 1;
        }
    }
    (gated_count > 0).then(|| loudness(gated_sum / gated_count as f64))
}

fn window_frames(sample_rate: u32, numerator: usize, denominator: usize) -> Result<usize> {
    usize::try_from(sample_rate)
        .ok()
        .and_then(|rate| rate.checked_mul(numerator))
        .map(|frames| (frames + denominator / 2) / denominator)
        .filter(|frames| *frames > 0)
        .ok_or_else(|| resource("prepare_meter", "loudness window frame count overflowed"))
}

fn spectrum_indices(frames: usize, bins: usize) -> Vec<usize> {
    let maximum = frames / 2;
    (0..bins)
        .map(|bin| (bin * maximum + (bins - 1) / 2) / (bins - 1))
        .collect()
}

fn hann_window(frames: usize) -> Vec<f64> {
    if frames == 1 {
        return vec![1.0];
    }
    (0..frames)
        .map(|index| 0.5 - 0.5 * (std::f64::consts::TAU * index as f64 / (frames - 1) as f64).cos())
        .collect()
}

fn fallible_zeroed(length: usize, purpose: &'static str) -> Result<Vec<f64>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| resource("prepare_meter", purpose))?;
    values.resize(length, 0.0);
    Ok(values)
}

fn atomic_floats(length: usize, value: f64) -> Result<Vec<AtomicFloat>> {
    let mut values = Vec::new();
    values.try_reserve_exact(length).map_err(|_| {
        resource(
            "prepare_meter",
            "meter publication storage allocation failed",
        )
    })?;
    values.resize_with(length, || AtomicFloat::new(value));
    Ok(values)
}

fn option_from_atomic(value: &AtomicFloat) -> Option<f64> {
    let value = value.load();
    value.is_finite().then_some(value)
}

fn store_option(destination: &AtomicFloat, value: Option<f64>) {
    destination.store(value.unwrap_or(f64::NAN));
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn resource(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

// ITU-R BS.1770-5 Annex 2, 48th-order four-phase FIR interpolation coefficients.
const TRUE_PEAK_COEFFICIENTS: [[f64; TRUE_PEAK_PHASES]; TRUE_PEAK_TAPS] = [
    [
        0.001708984375,
        -0.0291748046875,
        -0.0189208984375,
        -0.00830078125,
    ],
    [0.010986328125, 0.029296875, 0.0330810546875, 0.014892578125],
    [
        -0.0196533203125,
        -0.0517578125,
        -0.0582275390625,
        -0.026611328125,
    ],
    [0.033203125, 0.089111328125, 0.1015625, 0.047607421875],
    [
        -0.0594482421875,
        -0.16650390625,
        -0.2003173828125,
        -0.102294921875,
    ],
    [
        0.1373291015625,
        0.465087890625,
        0.77978515625,
        0.97216796875,
    ],
    [
        0.97216796875,
        0.77978515625,
        0.465087890625,
        0.1373291015625,
    ],
    [
        -0.102294921875,
        -0.2003173828125,
        -0.16650390625,
        -0.0594482421875,
    ],
    [0.047607421875, 0.1015625, 0.089111328125, 0.033203125],
    [
        -0.026611328125,
        -0.0582275390625,
        -0.0517578125,
        -0.0196533203125,
    ],
    [0.014892578125, 0.0330810546875, 0.029296875, 0.010986328125],
    [
        -0.00830078125,
        -0.0189208984375,
        -0.0291748046875,
        0.001708984375,
    ],
];
