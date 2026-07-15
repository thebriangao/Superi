//! Low-latency operating-system playback and output-device discovery.
//!
//! Device enumeration and stream construction run on control threads. The
//! platform callback owns only a preallocated lock-free consumer, an audio
//! master clock, and atomic telemetry.

use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use superi_concurrency::clock::AudioMasterClock;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::time::SampleTime;

/// Maximum preallocated sample capacity for one device-output queue.
pub const MAX_OUTPUT_BUFFER_SAMPLES: usize = 1_048_576;

/// Stable opaque locator reported by the operating-system audio backend.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutputDeviceId(String);

impl OutputDeviceId {
    /// Returns the serialized backend locator.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Failure while parsing a serialized backend device locator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseOutputDeviceIdError;

impl fmt::Display for ParseOutputDeviceIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("output device locator must contain nonempty backend and device parts")
    }
}

impl std::error::Error for ParseOutputDeviceIdError {}

impl FromStr for OutputDeviceId {
    type Err = ParseOutputDeviceIdError;

    fn from_str(serialized: &str) -> Result<Self, Self::Err> {
        let (backend, device) = serialized.split_once(':').ok_or(ParseOutputDeviceIdError)?;
        if backend.is_empty() || device.is_empty() {
            return Err(ParseOutputDeviceIdError);
        }
        Ok(Self(serialized.to_owned()))
    }
}

impl fmt::Display for OutputDeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A sample representation accepted by an output device.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum OutputSampleFormat {
    /// Signed 8-bit integer.
    I8,
    /// Signed 16-bit integer.
    I16,
    /// Signed 24-bit integer stored in 32 bits.
    I24,
    /// Signed 32-bit integer.
    I32,
    /// Signed 64-bit integer.
    I64,
    /// Unsigned 8-bit integer.
    U8,
    /// Unsigned 16-bit integer.
    U16,
    /// Unsigned 24-bit integer stored in 32 bits.
    U24,
    /// Unsigned 32-bit integer.
    U32,
    /// Unsigned 64-bit integer.
    U64,
    /// 32-bit floating point.
    F32,
    /// 64-bit floating point.
    F64,
    /// A format introduced by a newer backend version.
    Other,
}

/// Device callback-buffer constraint, measured in frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OutputBufferSize {
    /// The backend does not publish a useful range.
    Unknown,
    /// Inclusive supported frame range.
    Range {
        /// Smallest supported callback buffer.
        min: u32,
        /// Largest supported callback buffer.
        max: u32,
    },
}

/// One supported output configuration range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputCapability {
    /// Number of interleaved output channels.
    pub channels: u16,
    /// Inclusive minimum sample rate.
    pub min_sample_rate: u32,
    /// Inclusive maximum sample rate.
    pub max_sample_rate: u32,
    /// Device sample representation.
    pub sample_format: OutputSampleFormat,
    /// Supported callback-buffer range.
    pub buffer_size: OutputBufferSize,
}

impl OutputCapability {
    /// Returns whether this range contains an exact stream configuration.
    #[must_use]
    pub fn supports(&self, config: &OutputStreamConfig) -> bool {
        self.channels == config.channels
            && self.sample_format == config.sample_format
            && (self.min_sample_rate..=self.max_sample_rate).contains(&config.sample_rate)
            && match (self.buffer_size, config.buffer_frames) {
                (_, None) | (OutputBufferSize::Unknown, Some(_)) => true,
                (OutputBufferSize::Range { min, max }, Some(frames)) => {
                    (min..=max).contains(&frames)
                }
            }
    }
}

/// An exact output stream configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputStreamConfig {
    /// Number of interleaved output channels.
    pub channels: u16,
    /// Device sample rate in hertz.
    pub sample_rate: u32,
    /// Device sample representation.
    pub sample_format: OutputSampleFormat,
    /// Requested callback-buffer size, or the backend default.
    pub buffer_frames: Option<u32>,
}

/// Discoverable output-device metadata and capabilities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputDevice {
    /// Stable backend locator suitable for a later stream request.
    pub id: OutputDeviceId,
    /// Human-readable platform name.
    pub name: String,
    /// Whether this is the current platform default output.
    pub is_default: bool,
    /// Exact backend default when one is available.
    pub default_config: Option<OutputStreamConfig>,
    /// Supported output configuration ranges.
    pub capabilities: Vec<OutputCapability>,
    /// Semantic speaker positions are unknown when the backend reports only a count.
    pub channel_layout_known: bool,
}

/// A device omitted from discovery because its metadata was incomplete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceDiscoveryFailure {
    /// Best available device label.
    pub device: String,
    /// Actionable backend failure.
    pub reason: String,
}

/// Output-device discovery result, including partial failures.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeviceDiscovery {
    /// Devices with stable identity and complete capability enumeration.
    pub devices: Vec<OutputDevice>,
    /// Devices that could not be represented safely.
    pub skipped_devices: Vec<DeviceDiscoveryFailure>,
}

/// Control-thread failure while discovering or starting device output.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AudioDeviceError {
    /// The host could not enumerate output devices.
    EnumerateDevices(String),
    /// The serialized device locator is invalid on this platform.
    InvalidDeviceId(String),
    /// The selected device is no longer available.
    DeviceNotFound(OutputDeviceId),
    /// The selected device rejected the exact requested format.
    ConfigurationNotSupported(OutputStreamConfig),
    /// The producer and device stream disagree about channel count or rate.
    BufferConfigurationMismatch,
    /// The platform could not build the output stream.
    BuildStream(String),
    /// The platform could not start or resume the output stream.
    PlayStream(String),
    /// The platform could not pause the output stream.
    PauseStream(String),
}

impl AudioDeviceError {
    /// Returns whether discovery can fail because a headless or constrained host has no audio service.
    #[must_use]
    pub const fn is_environmental(&self) -> bool {
        matches!(self, Self::EnumerateDevices(_))
    }
}

impl fmt::Display for AudioDeviceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnumerateDevices(reason) => {
                write!(formatter, "cannot enumerate output devices: {reason}")
            }
            Self::InvalidDeviceId(id) => write!(formatter, "invalid output device locator: {id}"),
            Self::DeviceNotFound(id) => write!(formatter, "output device is unavailable: {id}"),
            Self::ConfigurationNotSupported(config) => {
                write!(formatter, "output device does not support {config:?}")
            }
            Self::BufferConfigurationMismatch => formatter.write_str(
                "output buffer channel count or sample rate does not match the device stream",
            ),
            Self::BuildStream(reason) => write!(formatter, "cannot build output stream: {reason}"),
            Self::PlayStream(reason) => write!(formatter, "cannot play output stream: {reason}"),
            Self::PauseStream(reason) => write!(formatter, "cannot pause output stream: {reason}"),
        }
    }
}

impl std::error::Error for AudioDeviceError {}

/// Configuration for the bounded engine-to-device sample buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputBufferConfig {
    /// Number of interleaved channels per frame.
    pub channels: u16,
    /// Fixed device sample rate.
    pub sample_rate: u32,
    /// Exact bounded capacity in complete frames.
    pub capacity_frames: usize,
    /// First sample position published by the device callback.
    pub initial_sample: i64,
}

/// Bounded-buffer validation or realtime callback-shape failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OutputBufferError {
    /// A stream cannot contain zero channels.
    ZeroChannels,
    /// A stream cannot use a zero sample rate.
    ZeroSampleRate,
    /// A bounded stream cannot have zero frame capacity.
    ZeroCapacity,
    /// Sample capacity overflowed the addressable platform size.
    CapacityOverflow,
    /// A requested queue would exceed the fixed allocation ceiling.
    CapacityTooLarge {
        /// Requested interleaved sample capacity.
        requested_samples: usize,
        /// Maximum permitted interleaved sample capacity.
        max_samples: usize,
    },
    /// A producer submission did not contain complete frames.
    SampleCountNotFrameAligned {
        /// Submitted sample count.
        samples: usize,
        /// Required samples per frame.
        channels: u16,
    },
    /// A producer sample was NaN or infinite.
    NonFiniteSample {
        /// Zero-based sample index in the rejected submission.
        index: usize,
    },
    /// A producer sample was outside the normalized inclusive range.
    SampleOutOfRange {
        /// Zero-based sample index in the rejected submission.
        index: usize,
    },
    /// A complete submission did not fit and nothing was accepted.
    InsufficientCapacity {
        /// Samples in the rejected submission.
        requested_samples: usize,
        /// Samples available when admission was checked.
        available_samples: usize,
    },
    /// A discontinuity is waiting for the realtime consumer to discard queued samples.
    DiscardPending {
        /// Latest generation requested by the control-thread producer.
        requested_generation: u64,
        /// Latest generation applied by the realtime consumer.
        applied_generation: u64,
    },
    /// The exact discard-generation counter cannot advance further.
    DiscardGenerationExhausted,
    /// The platform supplied a callback buffer containing a partial frame.
    CallbackNotFrameAligned {
        /// Callback sample count.
        samples: usize,
        /// Required samples per frame.
        channels: u16,
    },
}

impl fmt::Display for OutputBufferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "output buffer error: {self:?}")
    }
}

impl std::error::Error for OutputBufferError {}

/// Successful producer admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputWrite {
    /// Complete frames accepted.
    pub frames: usize,
    /// Interleaved samples accepted.
    pub samples: usize,
}

/// Realtime output callback result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputRenderReport {
    /// Complete frames presented to the device.
    pub frames: usize,
    /// Samples read from the producer.
    pub consumed_samples: usize,
    /// Samples replaced with digital silence.
    pub silence_samples: usize,
    /// Whether any sample in this callback starved.
    pub underrun: bool,
}

/// Producer-visible state for the asynchronous output discontinuity boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutputDiscardStatus {
    /// Latest generation requested by the control-thread producer.
    pub requested_generation: u64,
    /// Latest generation applied by the realtime consumer.
    pub applied_generation: u64,
}

impl OutputDiscardStatus {
    /// Reports whether sample admission must remain blocked for the pending discontinuity.
    #[must_use]
    pub const fn is_pending(self) -> bool {
        self.requested_generation != self.applied_generation
    }
}

/// Stable category for an asynchronous platform stream error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StreamFailureKind {
    /// Device was removed or became unavailable.
    DeviceNotAvailable,
    /// The active format became invalid and the stream must be rebuilt.
    StreamInvalidated,
    /// The platform reported a hardware buffer underrun or overrun.
    BufferUnderrun,
    /// Backend-specific failure without a portable category.
    BackendSpecific,
}

/// Point-in-time lock-free output telemetry.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutputTelemetrySnapshot {
    /// Complete frames sent to the device.
    pub rendered_frames: u64,
    /// Samples replaced by silence because the producer starved.
    pub silence_samples: u64,
    /// Callback invocations affected by producer starvation or platform underrun.
    pub underruns: u64,
    /// Samples rejected by bounded producer backpressure.
    pub dropped_samples: u64,
    /// Producer discontinuity requests accepted by the output path.
    pub discard_requests: u64,
    /// Queued samples discarded by the realtime consumer.
    pub discarded_samples: u64,
    /// Malformed platform callback buffers.
    pub callback_shape_errors: u64,
    /// Callback domain-entry failures.
    pub callback_domain_errors: u64,
    /// Sample-clock publication failures.
    pub clock_errors: u64,
    /// Asynchronous stream errors.
    pub stream_errors: u64,
    /// Most recent asynchronous stream-error category.
    pub last_stream_error: Option<StreamFailureKind>,
}

#[derive(Debug, Default)]
struct OutputTelemetryInner {
    rendered_frames: AtomicU64,
    silence_samples: AtomicU64,
    underruns: AtomicU64,
    dropped_samples: AtomicU64,
    discard_requests: AtomicU64,
    discarded_samples: AtomicU64,
    callback_shape_errors: AtomicU64,
    callback_domain_errors: AtomicU64,
    clock_errors: AtomicU64,
    stream_errors: AtomicU64,
    last_stream_error: AtomicU8,
}

/// Clonable lock-free diagnostics handle for one output stream.
#[derive(Clone, Debug, Default)]
pub struct OutputTelemetry(Arc<OutputTelemetryInner>);

impl OutputTelemetry {
    /// Reads one coherent-enough diagnostic snapshot without blocking the callback.
    #[must_use]
    pub fn snapshot(&self) -> OutputTelemetrySnapshot {
        OutputTelemetrySnapshot {
            rendered_frames: self.0.rendered_frames.load(Ordering::Relaxed),
            silence_samples: self.0.silence_samples.load(Ordering::Relaxed),
            underruns: self.0.underruns.load(Ordering::Relaxed),
            dropped_samples: self.0.dropped_samples.load(Ordering::Relaxed),
            discard_requests: self.0.discard_requests.load(Ordering::Relaxed),
            discarded_samples: self.0.discarded_samples.load(Ordering::Relaxed),
            callback_shape_errors: self.0.callback_shape_errors.load(Ordering::Relaxed),
            callback_domain_errors: self.0.callback_domain_errors.load(Ordering::Relaxed),
            clock_errors: self.0.clock_errors.load(Ordering::Relaxed),
            stream_errors: self.0.stream_errors.load(Ordering::Relaxed),
            last_stream_error: decode_stream_failure(
                self.0.last_stream_error.load(Ordering::Relaxed),
            ),
        }
    }

    fn record_stream_error(&self, error: cpal::StreamError) {
        let kind = match error {
            cpal::StreamError::DeviceNotAvailable => StreamFailureKind::DeviceNotAvailable,
            cpal::StreamError::StreamInvalidated => StreamFailureKind::StreamInvalidated,
            cpal::StreamError::BufferUnderrun => StreamFailureKind::BufferUnderrun,
            cpal::StreamError::BackendSpecific { .. } => StreamFailureKind::BackendSpecific,
        };
        self.0
            .last_stream_error
            .store(encode_stream_failure(kind), Ordering::Relaxed);
        self.0.stream_errors.fetch_add(1, Ordering::Relaxed);
        if kind == StreamFailureKind::BufferUnderrun {
            self.0.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, Default)]
struct OutputDiscardState {
    requested_generation: AtomicU64,
    applied_generation: AtomicU64,
}

impl OutputDiscardState {
    fn status(&self) -> OutputDiscardStatus {
        let applied_generation = self.applied_generation.load(Ordering::Acquire);
        OutputDiscardStatus {
            requested_generation: self.requested_generation.load(Ordering::Acquire),
            applied_generation,
        }
    }
}

const fn encode_stream_failure(kind: StreamFailureKind) -> u8 {
    match kind {
        StreamFailureKind::DeviceNotAvailable => 1,
        StreamFailureKind::StreamInvalidated => 2,
        StreamFailureKind::BufferUnderrun => 3,
        StreamFailureKind::BackendSpecific => 4,
    }
}

const fn decode_stream_failure(code: u8) -> Option<StreamFailureKind> {
    match code {
        1 => Some(StreamFailureKind::DeviceNotAvailable),
        2 => Some(StreamFailureKind::StreamInvalidated),
        3 => Some(StreamFailureKind::BufferUnderrun),
        4 => Some(StreamFailureKind::BackendSpecific),
        _ => None,
    }
}

/// Control-thread producer for normalized interleaved `f32` samples.
pub struct OutputProducer {
    channels: u16,
    ring: HeapProd<f32>,
    telemetry: OutputTelemetry,
    discard: Arc<OutputDiscardState>,
}

impl OutputProducer {
    /// Requests that the realtime callback discard every queued pre-discontinuity sample.
    pub fn request_discard(&self) -> Result<OutputDiscardStatus, OutputBufferError> {
        let requested_generation = self
            .discard
            .requested_generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |generation| {
                generation.checked_add(1)
            })
            .map_err(|_| OutputBufferError::DiscardGenerationExhausted)?
            .checked_add(1)
            .expect("successful discard generation advance cannot overflow");
        self.telemetry
            .0
            .discard_requests
            .fetch_add(1, Ordering::Relaxed);
        Ok(OutputDiscardStatus {
            requested_generation,
            applied_generation: self.discard.applied_generation.load(Ordering::Acquire),
        })
    }

    /// Returns the current producer-requested and consumer-applied discard generations.
    #[must_use]
    pub fn discard_status(&self) -> OutputDiscardStatus {
        self.discard.status()
    }

    /// Atomically admits all complete frames or rejects the entire submission.
    pub fn push_interleaved(&mut self, samples: &[f32]) -> Result<OutputWrite, OutputBufferError> {
        let channels = usize::from(self.channels);
        if samples.len() % channels != 0 {
            return Err(OutputBufferError::SampleCountNotFrameAligned {
                samples: samples.len(),
                channels: self.channels,
            });
        }
        for (index, sample) in samples.iter().copied().enumerate() {
            if !sample.is_finite() {
                return Err(OutputBufferError::NonFiniteSample { index });
            }
            if !(-1.0..=1.0).contains(&sample) {
                return Err(OutputBufferError::SampleOutOfRange { index });
            }
        }
        let discard = self.discard.status();
        if discard.is_pending() {
            return Err(OutputBufferError::DiscardPending {
                requested_generation: discard.requested_generation,
                applied_generation: discard.applied_generation,
            });
        }
        let available_samples = self.ring.vacant_len();
        if samples.len() > available_samples {
            self.telemetry
                .0
                .dropped_samples
                .fetch_add(samples.len() as u64, Ordering::Relaxed);
            return Err(OutputBufferError::InsufficientCapacity {
                requested_samples: samples.len(),
                available_samples,
            });
        }
        let written = self.ring.push_slice(samples);
        debug_assert_eq!(written, samples.len());
        Ok(OutputWrite {
            frames: written / channels,
            samples: written,
        })
    }
}

/// Realtime consumer owned by exactly one platform output callback.
pub struct OutputConsumer {
    channels: u16,
    sample_rate: u32,
    next_sample: i64,
    ring: HeapCons<f32>,
    clock: Arc<AudioMasterClock>,
    telemetry: OutputTelemetry,
    discard: Arc<OutputDiscardState>,
}

impl OutputConsumer {
    /// Renders normalized floating-point device samples.
    pub fn render_f32(
        &mut self,
        device_buffer: &mut [f32],
    ) -> Result<OutputRenderReport, OutputBufferError> {
        self.render(device_buffer)
    }

    /// Returns the shared device sample clock.
    #[must_use]
    pub fn clock(&self) -> &Arc<AudioMasterClock> {
        &self.clock
    }

    /// Returns the fixed channel count.
    #[must_use]
    pub const fn channels(&self) -> u16 {
        self.channels
    }

    /// Returns the fixed device sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn render<T>(
        &mut self,
        device_buffer: &mut [T],
    ) -> Result<OutputRenderReport, OutputBufferError>
    where
        T: SizedSample + FromSample<f32>,
    {
        let channels = usize::from(self.channels);
        if device_buffer.len() % channels != 0 {
            let silence = T::from_sample(0.0);
            device_buffer.fill(silence);
            self.telemetry
                .0
                .callback_shape_errors
                .fetch_add(1, Ordering::Relaxed);
            return Err(OutputBufferError::CallbackNotFrameAligned {
                samples: device_buffer.len(),
                channels: self.channels,
            });
        }

        self.apply_pending_discard();

        let _domain = match ExecutionDomain::Audio.enter_current() {
            Ok(guard) => guard,
            Err(_) => {
                let silence = T::from_sample(0.0);
                device_buffer.fill(silence);
                self.telemetry
                    .0
                    .callback_domain_errors
                    .fetch_add(1, Ordering::Relaxed);
                let frames = device_buffer.len() / channels;
                self.finish_callback(frames, device_buffer.len(), true);
                return Ok(OutputRenderReport {
                    frames,
                    consumed_samples: 0,
                    silence_samples: device_buffer.len(),
                    underrun: true,
                });
            }
        };

        let mut consumed_samples = 0;
        for destination in &mut *device_buffer {
            if let Some(sample) = self.ring.try_pop() {
                *destination = T::from_sample(sample);
                consumed_samples += 1;
            } else {
                *destination = T::from_sample(0.0);
            }
        }
        let silence_samples = device_buffer.len() - consumed_samples;
        let underrun = silence_samples != 0;
        let frames = device_buffer.len() / channels;
        self.finish_callback(frames, silence_samples, underrun);

        Ok(OutputRenderReport {
            frames,
            consumed_samples,
            silence_samples,
            underrun,
        })
    }

    fn apply_pending_discard(&mut self) {
        let requested_generation = self.discard.requested_generation.load(Ordering::Acquire);
        let applied_generation = self.discard.applied_generation.load(Ordering::Relaxed);
        if requested_generation == applied_generation {
            return;
        }
        let discarded_samples = self.ring.clear();
        self.telemetry.0.discarded_samples.fetch_add(
            u64::try_from(discarded_samples).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        self.discard
            .applied_generation
            .store(requested_generation, Ordering::Release);
    }

    fn finish_callback(&mut self, frames: usize, silence_samples: usize, underrun: bool) {
        self.telemetry
            .0
            .rendered_frames
            .fetch_add(u64::try_from(frames).unwrap_or(u64::MAX), Ordering::Relaxed);
        self.telemetry.0.silence_samples.fetch_add(
            u64::try_from(silence_samples).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        if underrun {
            self.telemetry.0.underruns.fetch_add(1, Ordering::Relaxed);
        }

        let frame_delta = i64::try_from(frames).ok();
        if let Some(next_sample) = frame_delta.and_then(|delta| self.next_sample.checked_add(delta))
        {
            if self.clock.publish_sample(next_sample).is_ok() {
                self.next_sample = next_sample;
            } else {
                self.telemetry
                    .0
                    .clock_errors
                    .fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.telemetry
                .0
                .clock_errors
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Creates one preallocated lock-free engine-to-device sample path.
pub fn create_output_buffer(
    config: OutputBufferConfig,
) -> Result<(OutputProducer, OutputConsumer, OutputTelemetry), OutputBufferError> {
    if config.channels == 0 {
        return Err(OutputBufferError::ZeroChannels);
    }
    if config.sample_rate == 0 {
        return Err(OutputBufferError::ZeroSampleRate);
    }
    if config.capacity_frames == 0 {
        return Err(OutputBufferError::ZeroCapacity);
    }
    let capacity_samples = config
        .capacity_frames
        .checked_mul(usize::from(config.channels))
        .ok_or(OutputBufferError::CapacityOverflow)?;
    if capacity_samples > MAX_OUTPUT_BUFFER_SAMPLES {
        return Err(OutputBufferError::CapacityTooLarge {
            requested_samples: capacity_samples,
            max_samples: MAX_OUTPUT_BUFFER_SAMPLES,
        });
    }
    let ring = HeapRb::<f32>::new(capacity_samples);
    let (producer, consumer) = ring.split();
    let telemetry = OutputTelemetry::default();
    let discard = Arc::new(OutputDiscardState::default());
    let clock = Arc::new(AudioMasterClock::new(
        SampleTime::new(config.initial_sample, config.sample_rate)
            .expect("nonzero sample rate was validated"),
    ));
    Ok((
        OutputProducer {
            channels: config.channels,
            ring: producer,
            telemetry: telemetry.clone(),
            discard: Arc::clone(&discard),
        },
        OutputConsumer {
            channels: config.channels,
            sample_rate: config.sample_rate,
            next_sample: config.initial_sample,
            ring: consumer,
            clock,
            telemetry: telemetry.clone(),
            discard,
        },
        telemetry,
    ))
}

/// Enumerates output devices through the current operating-system backend.
pub fn discover_output_devices() -> Result<DeviceDiscovery, AudioDeviceError> {
    let host = cpal::default_host();
    let default_id = host
        .default_output_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.to_string());
    let devices = host
        .output_devices()
        .map_err(|error| AudioDeviceError::EnumerateDevices(error.to_string()))?;
    let mut discovery = DeviceDiscovery::default();

    for device in devices {
        match inspect_device(&device, default_id.as_deref()) {
            Ok(output) => discovery.devices.push(output),
            Err(failure) => discovery.skipped_devices.push(failure),
        }
    }
    discovery
        .devices
        .sort_by(|left, right| left.id.cmp(&right.id));
    Ok(discovery)
}

fn inspect_device(
    device: &cpal::Device,
    default_id: Option<&str>,
) -> Result<OutputDevice, DeviceDiscoveryFailure> {
    let description = device
        .description()
        .map_err(|error| DeviceDiscoveryFailure {
            device: "unknown output device".to_owned(),
            reason: error.to_string(),
        })?;
    let id = device.id().map_err(|error| DeviceDiscoveryFailure {
        device: description.name().to_owned(),
        reason: error.to_string(),
    })?;
    let serialized_id = id.to_string();
    let configs = device
        .supported_output_configs()
        .map_err(|error| DeviceDiscoveryFailure {
            device: description.name().to_owned(),
            reason: error.to_string(),
        })?;
    let capabilities = configs.map(map_capability).collect();
    let default_config = device
        .default_output_config()
        .ok()
        .map(map_supported_config);

    Ok(OutputDevice {
        id: OutputDeviceId(serialized_id.clone()),
        name: description.name().to_owned(),
        is_default: default_id == Some(serialized_id.as_str()),
        default_config,
        capabilities,
        channel_layout_known: false,
    })
}

fn map_capability(config: cpal::SupportedStreamConfigRange) -> OutputCapability {
    OutputCapability {
        channels: config.channels(),
        min_sample_rate: config.min_sample_rate(),
        max_sample_rate: config.max_sample_rate(),
        sample_format: map_sample_format(config.sample_format()),
        buffer_size: match config.buffer_size() {
            cpal::SupportedBufferSize::Unknown => OutputBufferSize::Unknown,
            cpal::SupportedBufferSize::Range { min, max } => OutputBufferSize::Range {
                min: *min,
                max: *max,
            },
        },
    }
}

fn map_supported_config(config: cpal::SupportedStreamConfig) -> OutputStreamConfig {
    OutputStreamConfig {
        channels: config.channels(),
        sample_rate: config.sample_rate(),
        sample_format: map_sample_format(config.sample_format()),
        buffer_frames: None,
    }
}

const fn map_sample_format(format: cpal::SampleFormat) -> OutputSampleFormat {
    match format {
        cpal::SampleFormat::I8 => OutputSampleFormat::I8,
        cpal::SampleFormat::I16 => OutputSampleFormat::I16,
        cpal::SampleFormat::I24 => OutputSampleFormat::I24,
        cpal::SampleFormat::I32 => OutputSampleFormat::I32,
        cpal::SampleFormat::I64 => OutputSampleFormat::I64,
        cpal::SampleFormat::U8 => OutputSampleFormat::U8,
        cpal::SampleFormat::U16 => OutputSampleFormat::U16,
        cpal::SampleFormat::U24 => OutputSampleFormat::U24,
        cpal::SampleFormat::U32 => OutputSampleFormat::U32,
        cpal::SampleFormat::U64 => OutputSampleFormat::U64,
        cpal::SampleFormat::F32 => OutputSampleFormat::F32,
        cpal::SampleFormat::F64 => OutputSampleFormat::F64,
        _ => OutputSampleFormat::Other,
    }
}

/// Starts a production output stream for an exact selected configuration.
pub fn start_device_output(
    device_id: &OutputDeviceId,
    config: OutputStreamConfig,
    consumer: OutputConsumer,
) -> Result<DeviceOutput, AudioDeviceError> {
    if consumer.channels() != config.channels || consumer.sample_rate() != config.sample_rate {
        return Err(AudioDeviceError::BufferConfigurationMismatch);
    }
    let parsed_id = cpal::DeviceId::from_str(device_id.as_str())
        .map_err(|_| AudioDeviceError::InvalidDeviceId(device_id.to_string()))?;
    let host = cpal::default_host();
    let device = host
        .device_by_id(&parsed_id)
        .ok_or_else(|| AudioDeviceError::DeviceNotFound(device_id.clone()))?;
    let supported = device
        .supported_output_configs()
        .map_err(|error| AudioDeviceError::BuildStream(error.to_string()))?
        .map(map_capability)
        .any(|capability| capability.supports(&config));
    if !supported {
        return Err(AudioDeviceError::ConfigurationNotSupported(config));
    }

    let stream_config = cpal::StreamConfig {
        channels: config.channels,
        sample_rate: config.sample_rate,
        buffer_size: config
            .buffer_frames
            .map_or(cpal::BufferSize::Default, cpal::BufferSize::Fixed),
    };
    let telemetry = consumer.telemetry.clone();
    let error_telemetry = telemetry.clone();
    let stream = build_typed_stream(
        &device,
        &stream_config,
        config.sample_format,
        consumer,
        move |error| error_telemetry.record_stream_error(error),
    )?;
    stream
        .play()
        .map_err(|error| AudioDeviceError::PlayStream(error.to_string()))?;
    Ok(DeviceOutput {
        stream,
        device_id: device_id.clone(),
        config,
        telemetry,
    })
}

fn build_typed_stream<E>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: OutputSampleFormat,
    mut consumer: OutputConsumer,
    error_callback: E,
) -> Result<cpal::Stream, AudioDeviceError>
where
    E: FnMut(cpal::StreamError) + Send + 'static,
{
    macro_rules! build {
        ($sample:ty, $consumer:expr, $errors:expr) => {
            device.build_output_stream(
                config,
                move |buffer: &mut [$sample], _| {
                    let _ = $consumer.render(buffer);
                },
                $errors,
                None,
            )
        };
    }

    let result = match format {
        OutputSampleFormat::I8 => build!(i8, consumer, error_callback),
        OutputSampleFormat::I16 => build!(i16, consumer, error_callback),
        OutputSampleFormat::I24 => build!(cpal::I24, consumer, error_callback),
        OutputSampleFormat::I32 => build!(i32, consumer, error_callback),
        OutputSampleFormat::I64 => build!(i64, consumer, error_callback),
        OutputSampleFormat::U8 => build!(u8, consumer, error_callback),
        OutputSampleFormat::U16 => build!(u16, consumer, error_callback),
        OutputSampleFormat::U24 => build!(cpal::U24, consumer, error_callback),
        OutputSampleFormat::U32 => build!(u32, consumer, error_callback),
        OutputSampleFormat::U64 => build!(u64, consumer, error_callback),
        OutputSampleFormat::F32 => build!(f32, consumer, error_callback),
        OutputSampleFormat::F64 => build!(f64, consumer, error_callback),
        OutputSampleFormat::Other => {
            return Err(AudioDeviceError::ConfigurationNotSupported(
                OutputStreamConfig {
                    channels: config.channels,
                    sample_rate: config.sample_rate,
                    sample_format: format,
                    buffer_frames: match config.buffer_size {
                        cpal::BufferSize::Default => None,
                        cpal::BufferSize::Fixed(frames) => Some(frames),
                    },
                },
            ));
        }
    };
    result.map_err(|error| AudioDeviceError::BuildStream(error.to_string()))
}

/// Owning handle for one active production device stream.
pub struct DeviceOutput {
    stream: cpal::Stream,
    device_id: OutputDeviceId,
    config: OutputStreamConfig,
    telemetry: OutputTelemetry,
}

impl DeviceOutput {
    /// Returns the selected stable device locator.
    #[must_use]
    pub fn device_id(&self) -> &OutputDeviceId {
        &self.device_id
    }

    /// Returns the exact active configuration.
    #[must_use]
    pub const fn config(&self) -> OutputStreamConfig {
        self.config
    }

    /// Returns a clonable diagnostics handle.
    #[must_use]
    pub fn telemetry(&self) -> OutputTelemetry {
        self.telemetry.clone()
    }

    /// Resumes playback after a pause.
    pub fn play(&self) -> Result<(), AudioDeviceError> {
        self.stream
            .play()
            .map_err(|error| AudioDeviceError::PlayStream(error.to_string()))
    }

    /// Pauses playback without destroying the configured stream.
    pub fn pause(&self) -> Result<(), AudioDeviceError> {
        self.stream
            .pause()
            .map_err(|error| AudioDeviceError::PauseStream(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_callback_converts_normalized_samples_without_changing_frame_time() {
        let (mut producer, mut consumer, telemetry) = create_output_buffer(OutputBufferConfig {
            channels: 1,
            sample_rate: 48_000,
            capacity_frames: 3,
            initial_sample: 7,
        })
        .expect("valid output buffer");
        producer
            .push_interleaved(&[-1.0, 0.0, 1.0])
            .expect("three samples fit");
        let mut integer_output = [0_i16; 3];

        let report = consumer
            .render(&mut integer_output)
            .expect("mono callback is aligned");

        assert_eq!(integer_output, [i16::MIN, 0, i16::MAX]);
        assert_eq!(report.frames, 3);
        assert_eq!(consumer.clock().position().sample(), 10);
        assert_eq!(telemetry.snapshot().rendered_frames, 3);
    }

    #[test]
    fn backend_default_config_does_not_invent_a_callback_buffer_size() {
        let backend = cpal::SupportedStreamConfig::new(
            2,
            48_000,
            cpal::SupportedBufferSize::Range { min: 64, max: 512 },
            cpal::SampleFormat::F32,
        );

        assert_eq!(map_supported_config(backend).buffer_frames, None);
    }
}
