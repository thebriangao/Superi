//! Prepared sample-rate conversion between independent source and device clocks.
//!
//! Construction allocates the band-limited sinc state and planar scratch buffers.
//! [`PreparedSampleRateConverter::process_interleaved`] then runs on
//! [`ExecutionDomain::Audio`] without allocating or locking. Source and device
//! positions remain exact in their native sample clocks, while a bounded device
//! clock observation smoothly adjusts the conversion ratio.

use rubato::{
    Resampler, SincFixedOut, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::ChannelLayout;
use superi_core::time::SampleTime;

const COMPONENT: &str = "superi-audio.resample";
const SINC_LENGTH: usize = 256;
const SINC_CUTOFF: f32 = 0.95;
const SINC_OVERSAMPLING: usize = 128;
const MAX_SUPPORTED_CLOCK_ERROR_PPM: f64 = 100_000.0;

/// A measured device clock-rate error relative to its nominal sample rate.
///
/// Positive values mean the device consumes output faster than nominal, so the
/// converter consumes more source frames for each fixed device block. Negative
/// values mean the device consumes output more slowly. Updates are ramped across
/// the next block to avoid an audible ratio step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceClockErrorPpm(f64);

impl DeviceClockErrorPpm {
    /// No observed device clock error.
    pub const ZERO: Self = Self(0.0);

    /// Creates a finite signed parts-per-million observation.
    pub fn new(parts_per_million: f64) -> Result<Self> {
        if !parts_per_million.is_finite() {
            return Err(invalid(
                "create_clock_error",
                "device clock error must be finite",
            ));
        }
        Ok(Self(parts_per_million))
    }

    /// Returns signed parts per million.
    #[must_use]
    pub const fn parts_per_million(self) -> f64 {
        self.0
    }

    fn relative_output_ratio(self) -> f64 {
        1.0 / (1.0 + self.0 / 1_000_000.0)
    }
}

/// Immutable preparation parameters for one source-to-device conversion stream.
#[derive(Clone, Debug, PartialEq)]
pub struct SampleRateConverterConfig {
    source_rate: u32,
    device_rate: u32,
    channel_layout: ChannelLayout,
    output_frames: usize,
    source_start: SampleTime,
    device_start: SampleTime,
    max_clock_error_ppm: f64,
}

impl SampleRateConverterConfig {
    /// Creates and validates a fixed-output converter configuration.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_rate: u32,
        device_rate: u32,
        channel_layout: ChannelLayout,
        output_frames: usize,
        source_start: SampleTime,
        device_start: SampleTime,
        max_clock_error_ppm: f64,
    ) -> Result<Self> {
        if source_rate == 0 || device_rate == 0 {
            return Err(invalid(
                "create_config",
                "source and device sample rates must be greater than zero",
            ));
        }
        if output_frames == 0 {
            return Err(invalid(
                "create_config",
                "device output block must contain at least one frame",
            ));
        }
        if source_start.sample_rate() != source_rate {
            return Err(invalid(
                "create_config",
                "source start must use the configured source sample rate",
            ));
        }
        if device_start.sample_rate() != device_rate {
            return Err(invalid(
                "create_config",
                "device start must use the configured device sample rate",
            ));
        }
        if !max_clock_error_ppm.is_finite()
            || !(0.0..=MAX_SUPPORTED_CLOCK_ERROR_PPM).contains(&max_clock_error_ppm)
        {
            return Err(invalid(
                "create_config",
                "maximum device clock error must be finite and within the supported range",
            ));
        }
        Ok(Self {
            source_rate,
            device_rate,
            channel_layout,
            output_frames,
            source_start,
            device_start,
            max_clock_error_ppm,
        })
    }

    /// Returns the source sample rate.
    #[must_use]
    pub const fn source_rate(&self) -> u32 {
        self.source_rate
    }

    /// Returns the device sample rate.
    #[must_use]
    pub const fn device_rate(&self) -> u32 {
        self.device_rate
    }

    /// Returns channels in unchanged routing order.
    #[must_use]
    pub const fn channel_layout(&self) -> &ChannelLayout {
        &self.channel_layout
    }

    /// Returns the fixed number of frames emitted for each device block.
    #[must_use]
    pub const fn output_frames(&self) -> usize {
        self.output_frames
    }

    /// Returns the maximum accepted absolute device clock error.
    #[must_use]
    pub const fn max_clock_error_ppm(&self) -> f64 {
        self.max_clock_error_ppm
    }
}

/// Exact native-clock accounting for one converted block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResampleBlockReport {
    source_start: SampleTime,
    source_end: SampleTime,
    source_frames: usize,
    device_start: SampleTime,
    device_end: SampleTime,
    device_frames: usize,
}

impl ResampleBlockReport {
    /// Returns the first consumed source sample.
    #[must_use]
    pub const fn source_start(self) -> SampleTime {
        self.source_start
    }

    /// Returns the next unconsumed source sample.
    #[must_use]
    pub const fn source_end(self) -> SampleTime {
        self.source_end
    }

    /// Returns source frames consumed per channel.
    #[must_use]
    pub const fn source_frames(self) -> usize {
        self.source_frames
    }

    /// Returns the first produced device sample.
    #[must_use]
    pub const fn device_start(self) -> SampleTime {
        self.device_start
    }

    /// Returns the next device sample after this block.
    #[must_use]
    pub const fn device_end(self) -> SampleTime {
        self.device_end
    }

    /// Returns device frames produced per channel.
    #[must_use]
    pub const fn device_frames(self) -> usize {
        self.device_frames
    }
}

/// A fully allocated converter ready for the real-time audio domain.
pub struct PreparedSampleRateConverter {
    config: SampleRateConverterConfig,
    resampler: SincFixedOut<f32>,
    input_scratch: Vec<Vec<f32>>,
    output_scratch: Vec<Vec<f32>>,
    source_position: SampleTime,
    device_position: SampleTime,
}

impl PreparedSampleRateConverter {
    /// Allocates and prepares a band-limited converter outside the audio callback.
    pub fn new(config: SampleRateConverterConfig) -> Result<Self> {
        let nominal_ratio = f64::from(config.device_rate) / f64::from(config.source_rate);
        let maximum = config.max_clock_error_ppm / 1_000_000.0;
        let maximum_relative_ratio = if maximum == 0.0 {
            1.0
        } else {
            (1.0 / (1.0 - maximum)).max(1.0 + maximum)
        };
        let parameters = SincInterpolationParameters {
            sinc_len: SINC_LENGTH,
            f_cutoff: SINC_CUTOFF,
            oversampling_factor: SINC_OVERSAMPLING,
            interpolation: SincInterpolationType::Cubic,
            window: WindowFunction::BlackmanHarris2,
        };
        let resampler = SincFixedOut::new(
            nominal_ratio,
            maximum_relative_ratio,
            parameters,
            config.output_frames,
            config.channel_layout.len(),
        )
        .map_err(|error| resampler_error("prepare_converter", error.to_string()))?;
        let input_scratch = resampler.input_buffer_allocate(true);
        let output_scratch = resampler.output_buffer_allocate(true);
        let source_position = config.source_start;
        let device_position = config.device_start;
        Ok(Self {
            config,
            resampler,
            input_scratch,
            output_scratch,
            source_position,
            device_position,
        })
    }

    /// Returns the immutable stream configuration.
    #[must_use]
    pub const fn config(&self) -> &SampleRateConverterConfig {
        &self.config
    }

    /// Returns source frames required per channel for the next call.
    #[must_use]
    pub fn next_input_frames(&self) -> usize {
        self.resampler.input_frames_next()
    }

    /// Returns the prepared source lookahead required for every process call.
    ///
    /// The converter consumes only [`Self::next_input_frames`] from this window.
    /// A fixed maximum lets the callback validate storage before applying a new
    /// clock observation that may change the exact consumption for this block.
    #[must_use]
    pub fn maximum_input_frames(&self) -> usize {
        self.resampler.input_frames_max()
    }

    /// Returns fixed device frames produced per channel for each call.
    #[must_use]
    pub const fn output_frames(&self) -> usize {
        self.config.output_frames
    }

    /// Returns the sinc filter latency in device frames.
    #[must_use]
    pub fn output_delay_frames(&self) -> usize {
        self.resampler.output_delay()
    }

    /// Returns the next exact source position required by this stream.
    #[must_use]
    pub const fn source_position(&self) -> SampleTime {
        self.source_position
    }

    /// Returns the next exact device position emitted by this stream.
    #[must_use]
    pub const fn device_position(&self) -> SampleTime {
        self.device_position
    }

    /// Converts one exact consecutive interleaved block on the audio domain.
    ///
    /// The input must contain exactly [`Self::maximum_input_frames`] frames of
    /// prepared lookahead and the output exactly [`Self::output_frames`] frames
    /// in the configured channel order. The report states how many leading
    /// source frames were consumed. All storage validation completes before DSP
    /// state changes. The successful path reuses preparation-time storage and
    /// does not allocate or lock.
    pub fn process_interleaved(
        &mut self,
        source_start: SampleTime,
        device_start: SampleTime,
        input: &[f32],
        output: &mut [f32],
        device_clock_error: DeviceClockErrorPpm,
    ) -> Result<ResampleBlockReport> {
        ExecutionDomain::Audio.require_current()?;
        if source_start != self.source_position {
            return Err(invalid(
                "process_block",
                "source sample position must continue exactly from the prior block",
            ));
        }
        if device_start != self.device_position {
            return Err(invalid(
                "process_block",
                "device sample position must continue exactly from the prior block",
            ));
        }
        if device_clock_error.0.abs() > self.config.max_clock_error_ppm {
            return Err(invalid(
                "process_block",
                "device clock error exceeds the configured maximum clock error",
            ));
        }

        let channels = self.config.channel_layout.len();
        let expected_input = self
            .resampler
            .input_frames_max()
            .checked_mul(channels)
            .ok_or_else(|| invalid("process_block", "source buffer length overflowed"))?;
        let expected_output = self
            .config
            .output_frames
            .checked_mul(channels)
            .ok_or_else(|| invalid("process_block", "device buffer length overflowed"))?;
        if input.len() != expected_input {
            return Err(invalid(
                "process_block",
                "source buffer length does not match the prepared lookahead frame count",
            ));
        }
        if output.len() != expected_output {
            return Err(invalid(
                "process_block",
                "device buffer length does not match the fixed output frame count",
            ));
        }
        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(invalid(
                "process_block",
                "source audio samples must be finite",
            ));
        }

        advanced(
            source_start,
            self.resampler.input_frames_max(),
            "validate_source_clock_capacity",
        )?;
        let device_end = advanced(
            device_start,
            self.config.output_frames,
            "advance_device_clock",
        )?;

        self.resampler
            .set_resample_ratio_relative(device_clock_error.relative_output_ratio(), true)
            .map_err(|error| resampler_error("adjust_clock_ratio", error.to_string()))?;
        let source_frames = self.resampler.input_frames_next();
        let source_end = advanced(source_start, source_frames, "advance_source_clock")?;
        for (frame, interleaved) in input.chunks_exact(channels).take(source_frames).enumerate() {
            for (channel, sample) in interleaved.iter().enumerate() {
                self.input_scratch[channel][frame] = *sample;
            }
        }
        let (consumed, produced) = self
            .resampler
            .process_into_buffer(&self.input_scratch, &mut self.output_scratch, None)
            .map_err(|error| resampler_error("convert_block", error.to_string()))?;
        if consumed != source_frames || produced != self.config.output_frames {
            return Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "sample-rate converter violated its fixed block contract",
            )
            .with_context(ErrorContext::new(COMPONENT, "convert_block")));
        }

        for frame in 0..produced {
            for channel in 0..channels {
                output[frame * channels + channel] = self.output_scratch[channel][frame];
            }
        }
        self.source_position = source_end;
        self.device_position = device_end;
        Ok(ResampleBlockReport {
            source_start,
            source_end,
            source_frames,
            device_start,
            device_end,
            device_frames: produced,
        })
    }
}

fn advanced(start: SampleTime, frames: usize, operation: &'static str) -> Result<SampleTime> {
    let frames = i64::try_from(frames)
        .map_err(|_| invalid(operation, "sample frame count exceeds the supported range"))?;
    let sample = start
        .sample()
        .checked_add(frames)
        .ok_or_else(|| invalid(operation, "sample position overflowed"))?;
    SampleTime::new(sample, start.sample_rate())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn resampler_error(operation: &'static str, detail: String) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "sample-rate converter rejected validated prepared state",
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("detail", detail))
}
