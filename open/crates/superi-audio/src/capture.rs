//! Bounded operating-system audio input, record arming, and input monitoring.
//!
//! Device enumeration and stream construction run on control threads. The platform callback owns
//! only two preallocated lock-free producers, atomic arm and monitoring state, exact sample
//! coordinates, and atomic telemetry. Recording and monitoring apply independent whole-frame
//! backpressure so one saturated destination never blocks or partially writes the other.

use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::time::SampleTime;

use crate::playback::StreamFailureKind;

/// Maximum preallocated samples in each capture destination.
pub const MAX_CAPTURE_BUFFER_SAMPLES: usize = 1_048_576;

/// Stable opaque locator reported by the operating-system input backend.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InputDeviceId(String);

impl InputDeviceId {
    /// Returns the serialized backend locator.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Failure while parsing a serialized input-device locator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseInputDeviceIdError;

impl fmt::Display for ParseInputDeviceIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("input device locator must contain nonempty backend and device parts")
    }
}

impl std::error::Error for ParseInputDeviceIdError {}

impl FromStr for InputDeviceId {
    type Err = ParseInputDeviceIdError;

    fn from_str(serialized: &str) -> Result<Self, Self::Err> {
        let (backend, device) = serialized.split_once(':').ok_or(ParseInputDeviceIdError)?;
        if backend.is_empty() || device.is_empty() {
            return Err(ParseInputDeviceIdError);
        }
        Ok(Self(serialized.to_owned()))
    }
}

impl fmt::Display for InputDeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A sample representation supplied by an input device.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum InputSampleFormat {
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
pub enum InputBufferSize {
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

/// One supported input configuration range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputCapability {
    /// Number of interleaved input channels.
    pub channels: u16,
    /// Inclusive minimum sample rate.
    pub min_sample_rate: u32,
    /// Inclusive maximum sample rate.
    pub max_sample_rate: u32,
    /// Device sample representation.
    pub sample_format: InputSampleFormat,
    /// Supported callback-buffer range.
    pub buffer_size: InputBufferSize,
}

impl InputCapability {
    /// Returns whether this range contains an exact input stream configuration.
    #[must_use]
    pub fn supports(&self, config: &InputStreamConfig) -> bool {
        self.channels == config.channels
            && self.sample_format == config.sample_format
            && (self.min_sample_rate..=self.max_sample_rate).contains(&config.sample_rate)
            && match (self.buffer_size, config.buffer_frames) {
                (_, None) | (InputBufferSize::Unknown, Some(_)) => true,
                (InputBufferSize::Range { min, max }, Some(frames)) => {
                    (min..=max).contains(&frames)
                }
            }
    }
}

/// An exact input stream configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputStreamConfig {
    /// Number of interleaved input channels.
    pub channels: u16,
    /// Device sample rate in hertz.
    pub sample_rate: u32,
    /// Device sample representation.
    pub sample_format: InputSampleFormat,
    /// Requested callback-buffer size, or the backend default.
    pub buffer_frames: Option<u32>,
}

/// Discoverable input-device metadata and capabilities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputDevice {
    /// Stable backend locator suitable for a later stream request.
    pub id: InputDeviceId,
    /// Human-readable platform name.
    pub name: String,
    /// Whether this is the current platform default input.
    pub is_default: bool,
    /// Exact backend default when one is available.
    pub default_config: Option<InputStreamConfig>,
    /// Supported input configuration ranges.
    pub capabilities: Vec<InputCapability>,
    /// Semantic channel positions are unknown when the backend reports only a count.
    pub channel_layout_known: bool,
}

/// A device omitted from discovery because its metadata was incomplete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputDeviceDiscoveryFailure {
    /// Best available device label.
    pub device: String,
    /// Actionable backend failure.
    pub reason: String,
}

/// Input-device discovery result, including partial failures.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InputDeviceDiscovery {
    /// Devices with stable identity and complete capability enumeration.
    pub devices: Vec<InputDevice>,
    /// Devices that could not be represented safely.
    pub skipped_devices: Vec<InputDeviceDiscoveryFailure>,
}

/// Control-thread failure while discovering or starting input capture.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum InputDeviceError {
    /// The host could not enumerate input devices.
    EnumerateDevices(String),
    /// The serialized device locator is invalid on this platform.
    InvalidDeviceId(String),
    /// The selected device is no longer available.
    DeviceNotFound(InputDeviceId),
    /// The selected device rejected the exact requested format.
    ConfigurationNotSupported(InputStreamConfig),
    /// The callback and device stream disagree about channel count or rate.
    BufferConfigurationMismatch,
    /// The platform could not build the input stream.
    BuildStream(String),
    /// The platform could not start or resume the input stream.
    PlayStream(String),
    /// The platform could not pause the input stream.
    PauseStream(String),
}

impl InputDeviceError {
    /// Returns whether discovery can fail because a headless or constrained host has no audio service.
    #[must_use]
    pub const fn is_environmental(&self) -> bool {
        matches!(self, Self::EnumerateDevices(_))
    }
}

impl fmt::Display for InputDeviceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnumerateDevices(reason) => {
                write!(formatter, "cannot enumerate input devices: {reason}")
            }
            Self::InvalidDeviceId(id) => write!(formatter, "invalid input device locator: {id}"),
            Self::DeviceNotFound(id) => write!(formatter, "input device is unavailable: {id}"),
            Self::ConfigurationNotSupported(config) => {
                write!(formatter, "input device does not support {config:?}")
            }
            Self::BufferConfigurationMismatch => formatter.write_str(
                "capture buffer channel count or sample rate does not match the input stream",
            ),
            Self::BuildStream(reason) => write!(formatter, "cannot build input stream: {reason}"),
            Self::PlayStream(reason) => write!(formatter, "cannot play input stream: {reason}"),
            Self::PauseStream(reason) => write!(formatter, "cannot pause input stream: {reason}"),
        }
    }
}

impl std::error::Error for InputDeviceError {}

/// Configuration for bounded recording and monitoring buffers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureBufferConfig {
    /// Number of interleaved channels per frame.
    pub channels: u16,
    /// Fixed device sample rate.
    pub sample_rate: u32,
    /// Exact bounded capacity in complete frames for each destination.
    pub capacity_frames: usize,
    /// Physical sample coordinate assigned to the first callback frame.
    pub initial_sample: i64,
}

/// Bounded-buffer validation or real-time callback failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CaptureBufferError {
    /// A stream cannot contain zero channels.
    ZeroChannels,
    /// A stream cannot use a zero sample rate.
    ZeroSampleRate,
    /// A bounded stream cannot have zero frame capacity.
    ZeroCapacity,
    /// Sample capacity overflowed the addressable platform size.
    CapacityOverflow,
    /// A requested destination would exceed the fixed allocation ceiling.
    CapacityTooLarge {
        /// Requested interleaved sample capacity.
        requested_samples: usize,
        /// Maximum permitted interleaved sample capacity.
        max_samples: usize,
    },
    /// The platform supplied a callback containing a partial frame.
    CallbackNotFrameAligned {
        /// Callback sample count.
        samples: usize,
        /// Required samples per frame.
        channels: u16,
    },
    /// A converted input sample was NaN or infinite.
    NonFiniteSample {
        /// Zero-based sample index in the rejected callback.
        index: usize,
    },
    /// A converted input sample was outside the normalized inclusive range.
    SampleOutOfRange {
        /// Zero-based sample index in the rejected callback.
        index: usize,
    },
    /// The physical sample coordinate overflowed.
    SamplePositionOverflow,
    /// A captured sample used a channel index outside the supported integer range.
    ChannelIndexOverflow,
}

impl fmt::Display for CaptureBufferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "capture buffer error: {self:?}")
    }
}

impl std::error::Error for CaptureBufferError {}

/// One normalized captured sample with exact physical frame coordinate and channel index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapturedSample {
    sample_time: SampleTime,
    channel_index: u16,
    value: f32,
}

impl CapturedSample {
    /// Creates one validated captured sample.
    pub fn new(
        sample: i64,
        sample_rate: u32,
        channel_index: u16,
        value: f32,
    ) -> Result<Self, CaptureBufferError> {
        if sample_rate == 0 {
            return Err(CaptureBufferError::ZeroSampleRate);
        }
        if !value.is_finite() {
            return Err(CaptureBufferError::NonFiniteSample { index: 0 });
        }
        if !(-1.0..=1.0).contains(&value) {
            return Err(CaptureBufferError::SampleOutOfRange { index: 0 });
        }
        Ok(Self {
            sample_time: SampleTime::new(sample, sample_rate)
                .expect("nonzero sample rate was validated"),
            channel_index,
            value,
        })
    }

    /// Returns the exact physical input-frame coordinate.
    #[must_use]
    pub const fn sample_time(self) -> SampleTime {
        self.sample_time
    }

    /// Returns the zero-based channel index within the input stream.
    #[must_use]
    pub const fn channel_index(self) -> u16 {
        self.channel_index
    }

    /// Returns the normalized sample value.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.value
    }
}

#[derive(Debug, Default)]
struct CaptureControlInner {
    armed: AtomicBool,
    monitoring: AtomicBool,
}

/// Clonable lock-free control for record arming and input monitoring.
#[derive(Clone, Debug, Default)]
pub struct CaptureControl(Arc<CaptureControlInner>);

impl CaptureControl {
    /// Arms recording. The change becomes effective at the next callback-observed frame.
    pub fn arm(&self) {
        self.0.armed.store(true, Ordering::Release);
    }

    /// Disarms recording without stopping physical sample-clock progress.
    pub fn disarm(&self) {
        self.0.armed.store(false, Ordering::Release);
    }

    /// Returns whether recording is currently armed.
    #[must_use]
    pub fn is_armed(&self) -> bool {
        self.0.armed.load(Ordering::Acquire)
    }

    /// Enables or disables the independent monitoring destination.
    pub fn set_monitoring(&self, enabled: bool) {
        self.0.monitoring.store(enabled, Ordering::Release);
    }

    /// Returns whether input monitoring is enabled.
    #[must_use]
    pub fn is_monitoring(&self) -> bool {
        self.0.monitoring.load(Ordering::Acquire)
    }
}

/// Point-in-time lock-free capture telemetry.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CaptureTelemetrySnapshot {
    /// Complete physical input frames observed.
    pub input_frames: u64,
    /// Complete frames retained by the recording destination.
    pub recorded_frames: u64,
    /// Complete frames retained by the monitoring destination.
    pub monitored_frames: u64,
    /// Armed frames dropped because the recording ring was full.
    pub record_dropped_frames: u64,
    /// Monitored frames dropped because the monitoring ring was full.
    pub monitor_dropped_frames: u64,
    /// Malformed or non-finite callback invocations.
    pub callback_shape_errors: u64,
    /// Callback domain-entry failures.
    pub callback_domain_errors: u64,
    /// Physical sample-coordinate overflow failures.
    pub sample_clock_errors: u64,
    /// Asynchronous stream errors.
    pub stream_errors: u64,
    /// Most recent asynchronous stream-error category.
    pub last_stream_error: Option<StreamFailureKind>,
}

#[derive(Debug, Default)]
struct CaptureTelemetryInner {
    input_frames: AtomicU64,
    recorded_frames: AtomicU64,
    monitored_frames: AtomicU64,
    record_dropped_frames: AtomicU64,
    monitor_dropped_frames: AtomicU64,
    callback_shape_errors: AtomicU64,
    callback_domain_errors: AtomicU64,
    sample_clock_errors: AtomicU64,
    stream_errors: AtomicU64,
    last_stream_error: AtomicU8,
}

/// Clonable lock-free diagnostics handle for one capture stream.
#[derive(Clone, Debug, Default)]
pub struct CaptureTelemetry(Arc<CaptureTelemetryInner>);

impl CaptureTelemetry {
    /// Reads one coherent-enough diagnostic snapshot without blocking the callback.
    #[must_use]
    pub fn snapshot(&self) -> CaptureTelemetrySnapshot {
        CaptureTelemetrySnapshot {
            input_frames: self.0.input_frames.load(Ordering::Relaxed),
            recorded_frames: self.0.recorded_frames.load(Ordering::Relaxed),
            monitored_frames: self.0.monitored_frames.load(Ordering::Relaxed),
            record_dropped_frames: self.0.record_dropped_frames.load(Ordering::Relaxed),
            monitor_dropped_frames: self.0.monitor_dropped_frames.load(Ordering::Relaxed),
            callback_shape_errors: self.0.callback_shape_errors.load(Ordering::Relaxed),
            callback_domain_errors: self.0.callback_domain_errors.load(Ordering::Relaxed),
            sample_clock_errors: self.0.sample_clock_errors.load(Ordering::Relaxed),
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

/// Result of one valid physical input callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureReport {
    /// Complete physical input frames observed.
    pub frames: usize,
    /// Whether recording was armed for this callback.
    pub armed: bool,
    /// Whether monitoring was enabled for this callback.
    pub monitoring: bool,
    /// Whether the complete armed callback was dropped by recording backpressure.
    pub record_dropped: bool,
    /// Whether the complete monitored callback was dropped by monitoring backpressure.
    pub monitor_dropped: bool,
    /// Whether a conflicting execution domain caused both active destinations to drop.
    pub domain_conflict: bool,
}

/// Real-time endpoint owned by exactly one platform input callback.
pub struct CaptureCallback {
    channels: u16,
    sample_rate: u32,
    next_sample: i64,
    control: CaptureControl,
    capture_ring: HeapProd<CapturedSample>,
    monitor_ring: HeapProd<f32>,
    telemetry: CaptureTelemetry,
}

impl CaptureCallback {
    /// Captures one normalized interleaved floating-point callback.
    pub fn capture_f32(&mut self, input: &[f32]) -> Result<CaptureReport, CaptureBufferError> {
        self.capture(input)
    }

    /// Returns the exact physical coordinate assigned to the next valid callback frame.
    #[must_use]
    pub const fn next_sample(&self) -> i64 {
        self.next_sample
    }

    /// Returns the fixed input channel count.
    #[must_use]
    pub const fn channels(&self) -> u16 {
        self.channels
    }

    /// Returns the fixed physical sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn capture<T>(&mut self, input: &[T]) -> Result<CaptureReport, CaptureBufferError>
    where
        T: SizedSample + Copy,
        f32: FromSample<T>,
    {
        let channels = usize::from(self.channels);
        if input.len() % channels != 0 {
            self.telemetry
                .0
                .callback_shape_errors
                .fetch_add(1, Ordering::Relaxed);
            return Err(CaptureBufferError::CallbackNotFrameAligned {
                samples: input.len(),
                channels: self.channels,
            });
        }
        for (index, sample) in input.iter().copied().enumerate() {
            let sample = f32::from_sample(sample);
            if !sample.is_finite() {
                self.telemetry
                    .0
                    .callback_shape_errors
                    .fetch_add(1, Ordering::Relaxed);
                return Err(CaptureBufferError::NonFiniteSample { index });
            }
            if !(-1.0..=1.0).contains(&sample) {
                self.telemetry
                    .0
                    .callback_shape_errors
                    .fetch_add(1, Ordering::Relaxed);
                return Err(CaptureBufferError::SampleOutOfRange { index });
            }
        }

        let frames = input.len() / channels;
        let frame_delta =
            i64::try_from(frames).map_err(|_| CaptureBufferError::SamplePositionOverflow)?;
        let next_sample = self.next_sample.checked_add(frame_delta).ok_or_else(|| {
            self.telemetry
                .0
                .sample_clock_errors
                .fetch_add(1, Ordering::Relaxed);
            CaptureBufferError::SamplePositionOverflow
        })?;
        let armed = self.control.is_armed();
        let monitoring = self.control.is_monitoring();

        let _domain = match ExecutionDomain::Audio.enter_current() {
            Ok(guard) => guard,
            Err(_) => {
                self.telemetry
                    .0
                    .callback_domain_errors
                    .fetch_add(1, Ordering::Relaxed);
                self.telemetry
                    .0
                    .input_frames
                    .fetch_add(frames as u64, Ordering::Relaxed);
                if armed {
                    self.telemetry
                        .0
                        .record_dropped_frames
                        .fetch_add(frames as u64, Ordering::Relaxed);
                }
                if monitoring {
                    self.telemetry
                        .0
                        .monitor_dropped_frames
                        .fetch_add(frames as u64, Ordering::Relaxed);
                }
                self.next_sample = next_sample;
                return Ok(CaptureReport {
                    frames,
                    armed,
                    monitoring,
                    record_dropped: armed,
                    monitor_dropped: monitoring,
                    domain_conflict: true,
                });
            }
        };

        let record_dropped = armed && self.capture_ring.vacant_len() < input.len();
        let monitor_dropped = monitoring && self.monitor_ring.vacant_len() < input.len();

        if armed && !record_dropped {
            for (index, input_sample) in input.iter().copied().enumerate() {
                let frame = index / channels;
                let channel = u16::try_from(index % channels)
                    .map_err(|_| CaptureBufferError::ChannelIndexOverflow)?;
                let sample = CapturedSample {
                    sample_time: SampleTime::new(
                        self.next_sample + i64::try_from(frame).expect("frame fits checked delta"),
                        self.sample_rate,
                    )
                    .expect("nonzero sample rate was validated"),
                    channel_index: channel,
                    value: f32::from_sample(input_sample),
                };
                let rejected = self.capture_ring.try_push(sample);
                debug_assert!(rejected.is_ok());
            }
            self.telemetry
                .0
                .recorded_frames
                .fetch_add(frames as u64, Ordering::Relaxed);
        } else if record_dropped {
            self.telemetry
                .0
                .record_dropped_frames
                .fetch_add(frames as u64, Ordering::Relaxed);
        }

        if monitoring && !monitor_dropped {
            for sample in input.iter().copied() {
                let rejected = self.monitor_ring.try_push(f32::from_sample(sample));
                debug_assert!(rejected.is_ok());
            }
            self.telemetry
                .0
                .monitored_frames
                .fetch_add(frames as u64, Ordering::Relaxed);
        } else if monitor_dropped {
            self.telemetry
                .0
                .monitor_dropped_frames
                .fetch_add(frames as u64, Ordering::Relaxed);
        }

        self.telemetry
            .0
            .input_frames
            .fetch_add(frames as u64, Ordering::Relaxed);
        self.next_sample = next_sample;
        Ok(CaptureReport {
            frames,
            armed,
            monitoring,
            record_dropped,
            monitor_dropped,
            domain_conflict: false,
        })
    }
}

/// Control-thread reader for exact timestamped captured samples.
pub struct CaptureReader {
    channels: u16,
    ring: HeapCons<CapturedSample>,
}

impl CaptureReader {
    /// Drains at most `max_samples`, rounded down to complete interleaved frames.
    #[must_use]
    pub fn drain(&mut self, max_samples: usize) -> Vec<CapturedSample> {
        let channels = usize::from(self.channels);
        let count = self.ring.occupied_len().min(max_samples);
        let count = count - count % channels;
        let mut samples = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(sample) = self.ring.try_pop() {
                samples.push(sample);
            }
        }
        samples
    }
}

/// Control-thread reader for normalized interleaved monitoring samples.
pub struct MonitorReader {
    channels: u16,
    ring: HeapCons<f32>,
}

/// Complete normalized samples transferred from the monitoring ring.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonitorRead {
    /// Complete interleaved frames copied.
    pub frames: usize,
    /// Interleaved samples copied.
    pub samples: usize,
}

impl MonitorReader {
    /// Copies available complete frames into caller-owned storage without allocating.
    pub fn read_interleaved(&mut self, output: &mut [f32]) -> MonitorRead {
        let channels = usize::from(self.channels);
        let count = self.ring.occupied_len().min(output.len());
        let count = count - count % channels;
        for destination in &mut output[..count] {
            *destination = self
                .ring
                .try_pop()
                .expect("occupied sample count was observed before the single-consumer drain");
        }
        MonitorRead {
            frames: count / channels,
            samples: count,
        }
    }

    /// Drains at most `max_samples`, rounded down to complete interleaved frames.
    #[must_use]
    pub fn drain(&mut self, max_samples: usize) -> Vec<f32> {
        let channels = usize::from(self.channels);
        let count = self.ring.occupied_len().min(max_samples);
        let count = count - count % channels;
        let mut samples = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(sample) = self.ring.try_pop() {
                samples.push(sample);
            }
        }
        samples
    }
}

/// Creates one preallocated lock-free device-to-recording and monitoring path.
pub fn create_capture_buffer(
    config: CaptureBufferConfig,
) -> Result<
    (
        CaptureControl,
        CaptureCallback,
        CaptureReader,
        MonitorReader,
        CaptureTelemetry,
    ),
    CaptureBufferError,
> {
    if config.channels == 0 {
        return Err(CaptureBufferError::ZeroChannels);
    }
    if config.sample_rate == 0 {
        return Err(CaptureBufferError::ZeroSampleRate);
    }
    if config.capacity_frames == 0 {
        return Err(CaptureBufferError::ZeroCapacity);
    }
    let capacity_samples = config
        .capacity_frames
        .checked_mul(usize::from(config.channels))
        .ok_or(CaptureBufferError::CapacityOverflow)?;
    if capacity_samples > MAX_CAPTURE_BUFFER_SAMPLES {
        return Err(CaptureBufferError::CapacityTooLarge {
            requested_samples: capacity_samples,
            max_samples: MAX_CAPTURE_BUFFER_SAMPLES,
        });
    }

    let capture_ring = HeapRb::<CapturedSample>::new(capacity_samples);
    let monitor_ring = HeapRb::<f32>::new(capacity_samples);
    let (capture_producer, capture_consumer) = capture_ring.split();
    let (monitor_producer, monitor_consumer) = monitor_ring.split();
    let control = CaptureControl::default();
    let telemetry = CaptureTelemetry::default();

    Ok((
        control.clone(),
        CaptureCallback {
            channels: config.channels,
            sample_rate: config.sample_rate,
            next_sample: config.initial_sample,
            control,
            capture_ring: capture_producer,
            monitor_ring: monitor_producer,
            telemetry: telemetry.clone(),
        },
        CaptureReader {
            channels: config.channels,
            ring: capture_consumer,
        },
        MonitorReader {
            channels: config.channels,
            ring: monitor_consumer,
        },
        telemetry,
    ))
}

/// Enumerates input devices through the current operating-system backend.
pub fn discover_input_devices() -> Result<InputDeviceDiscovery, InputDeviceError> {
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.to_string());
    let devices = host
        .input_devices()
        .map_err(|error| InputDeviceError::EnumerateDevices(error.to_string()))?;
    let mut discovery = InputDeviceDiscovery::default();

    for device in devices {
        match inspect_device(&device, default_id.as_deref()) {
            Ok(input) => discovery.devices.push(input),
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
) -> Result<InputDevice, InputDeviceDiscoveryFailure> {
    let description = device
        .description()
        .map_err(|error| InputDeviceDiscoveryFailure {
            device: "unknown input device".to_owned(),
            reason: error.to_string(),
        })?;
    let id = device.id().map_err(|error| InputDeviceDiscoveryFailure {
        device: description.name().to_owned(),
        reason: error.to_string(),
    })?;
    let serialized_id = id.to_string();
    let configs =
        device
            .supported_input_configs()
            .map_err(|error| InputDeviceDiscoveryFailure {
                device: description.name().to_owned(),
                reason: error.to_string(),
            })?;
    let capabilities = configs.map(map_capability).collect();
    let default_config = device.default_input_config().ok().map(map_supported_config);

    Ok(InputDevice {
        id: InputDeviceId(serialized_id.clone()),
        name: description.name().to_owned(),
        is_default: default_id == Some(serialized_id.as_str()),
        default_config,
        capabilities,
        channel_layout_known: false,
    })
}

fn map_capability(config: cpal::SupportedStreamConfigRange) -> InputCapability {
    InputCapability {
        channels: config.channels(),
        min_sample_rate: config.min_sample_rate(),
        max_sample_rate: config.max_sample_rate(),
        sample_format: map_sample_format(config.sample_format()),
        buffer_size: match config.buffer_size() {
            cpal::SupportedBufferSize::Unknown => InputBufferSize::Unknown,
            cpal::SupportedBufferSize::Range { min, max } => InputBufferSize::Range {
                min: *min,
                max: *max,
            },
        },
    }
}

fn map_supported_config(config: cpal::SupportedStreamConfig) -> InputStreamConfig {
    InputStreamConfig {
        channels: config.channels(),
        sample_rate: config.sample_rate(),
        sample_format: map_sample_format(config.sample_format()),
        buffer_frames: None,
    }
}

const fn map_sample_format(format: cpal::SampleFormat) -> InputSampleFormat {
    match format {
        cpal::SampleFormat::I8 => InputSampleFormat::I8,
        cpal::SampleFormat::I16 => InputSampleFormat::I16,
        cpal::SampleFormat::I24 => InputSampleFormat::I24,
        cpal::SampleFormat::I32 => InputSampleFormat::I32,
        cpal::SampleFormat::I64 => InputSampleFormat::I64,
        cpal::SampleFormat::U8 => InputSampleFormat::U8,
        cpal::SampleFormat::U16 => InputSampleFormat::U16,
        cpal::SampleFormat::U24 => InputSampleFormat::U24,
        cpal::SampleFormat::U32 => InputSampleFormat::U32,
        cpal::SampleFormat::U64 => InputSampleFormat::U64,
        cpal::SampleFormat::F32 => InputSampleFormat::F32,
        cpal::SampleFormat::F64 => InputSampleFormat::F64,
        _ => InputSampleFormat::Other,
    }
}

/// Starts a production input stream for an exact selected configuration.
pub fn start_device_capture(
    device_id: &InputDeviceId,
    config: InputStreamConfig,
    callback: CaptureCallback,
) -> Result<DeviceCapture, InputDeviceError> {
    if callback.channels() != config.channels || callback.sample_rate() != config.sample_rate {
        return Err(InputDeviceError::BufferConfigurationMismatch);
    }
    let parsed_id = cpal::DeviceId::from_str(device_id.as_str())
        .map_err(|_| InputDeviceError::InvalidDeviceId(device_id.to_string()))?;
    let host = cpal::default_host();
    let device = host
        .device_by_id(&parsed_id)
        .ok_or_else(|| InputDeviceError::DeviceNotFound(device_id.clone()))?;
    let supported = device
        .supported_input_configs()
        .map_err(|error| InputDeviceError::BuildStream(error.to_string()))?
        .map(map_capability)
        .any(|capability| capability.supports(&config));
    if !supported {
        return Err(InputDeviceError::ConfigurationNotSupported(config));
    }

    let stream_config = cpal::StreamConfig {
        channels: config.channels,
        sample_rate: config.sample_rate,
        buffer_size: config
            .buffer_frames
            .map_or(cpal::BufferSize::Default, cpal::BufferSize::Fixed),
    };
    let telemetry = callback.telemetry.clone();
    let error_telemetry = telemetry.clone();
    let stream = build_typed_stream(
        &device,
        &stream_config,
        config.sample_format,
        callback,
        move |error| error_telemetry.record_stream_error(error),
    )?;
    stream
        .play()
        .map_err(|error| InputDeviceError::PlayStream(error.to_string()))?;
    Ok(DeviceCapture {
        stream,
        device_id: device_id.clone(),
        config,
        telemetry,
    })
}

fn build_typed_stream<E>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: InputSampleFormat,
    mut callback: CaptureCallback,
    error_callback: E,
) -> Result<cpal::Stream, InputDeviceError>
where
    E: FnMut(cpal::StreamError) + Send + 'static,
{
    macro_rules! build {
        ($sample:ty, $callback:expr, $errors:expr) => {
            device.build_input_stream(
                config,
                move |input: &[$sample], _| {
                    let _ = $callback.capture(input);
                },
                $errors,
                None,
            )
        };
    }

    let result = match format {
        InputSampleFormat::I8 => build!(i8, callback, error_callback),
        InputSampleFormat::I16 => build!(i16, callback, error_callback),
        InputSampleFormat::I24 => build!(cpal::I24, callback, error_callback),
        InputSampleFormat::I32 => build!(i32, callback, error_callback),
        InputSampleFormat::I64 => build!(i64, callback, error_callback),
        InputSampleFormat::U8 => build!(u8, callback, error_callback),
        InputSampleFormat::U16 => build!(u16, callback, error_callback),
        InputSampleFormat::U24 => build!(cpal::U24, callback, error_callback),
        InputSampleFormat::U32 => build!(u32, callback, error_callback),
        InputSampleFormat::U64 => build!(u64, callback, error_callback),
        InputSampleFormat::F32 => build!(f32, callback, error_callback),
        InputSampleFormat::F64 => build!(f64, callback, error_callback),
        InputSampleFormat::Other => {
            return Err(InputDeviceError::ConfigurationNotSupported(
                InputStreamConfig {
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
    result.map_err(|error| InputDeviceError::BuildStream(error.to_string()))
}

/// Owning handle for one active production input stream.
pub struct DeviceCapture {
    stream: cpal::Stream,
    device_id: InputDeviceId,
    config: InputStreamConfig,
    telemetry: CaptureTelemetry,
}

impl DeviceCapture {
    /// Returns the selected stable input-device locator.
    #[must_use]
    pub fn device_id(&self) -> &InputDeviceId {
        &self.device_id
    }

    /// Returns the exact active configuration.
    #[must_use]
    pub const fn config(&self) -> InputStreamConfig {
        self.config
    }

    /// Returns a clonable diagnostics handle.
    #[must_use]
    pub fn telemetry(&self) -> CaptureTelemetry {
        self.telemetry.clone()
    }

    /// Resumes capture after a pause.
    pub fn play(&self) -> Result<(), InputDeviceError> {
        self.stream
            .play()
            .map_err(|error| InputDeviceError::PlayStream(error.to_string()))
    }

    /// Pauses capture without destroying the configured stream.
    pub fn pause(&self) -> Result<(), InputDeviceError> {
        self.stream
            .pause()
            .map_err(|error| InputDeviceError::PauseStream(error.to_string()))
    }
}
