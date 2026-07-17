//! Format-neutral audio plugin state and a fixed-latency isolated process bridge.
//!
//! Native format hosts retain their own ABI details. This module supplies the common durable state
//! envelope and the real-time contract used when native processing lives in a supervised worker.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::ChannelLayout;
use superi_core::time::SampleTime;

use crate::graph::{AudioProcessBlock, AudioProcessor};

const COMPONENT: &str = "superi-audio.plugins";
const STATE_MAGIC: &[u8; 8] = b"SUPAUST\0";
const STATE_SCHEMA: u16 = 1;
const STATE_HEADER_BYTES: usize = 48;
const STATE_DIGEST_BYTES: usize = 32;
const MAXIMUM_FRAMES: usize = 1_048_576;
/// Maximum bytes accepted for either native component or controller state.
pub const MAX_AUDIO_PLUGIN_STATE_BYTES: usize = 32 * 1024 * 1024;
/// Maximum combined native bytes while reserving room for the bounded project envelope.
pub const MAX_AUDIO_PLUGIN_STATE_TOTAL_BYTES: usize = 64 * 1024 * 1024 - 4 * 1024;
/// Maximum UTF-8 byte length of one retained audio plugin identity field.
pub const MAX_AUDIO_PLUGIN_IDENTITY_FIELD_BYTES: usize = 1_024;

/// Native audio plugin format retained in durable state and scan results.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AudioPluginFormat {
    /// Apple Audio Unit component.
    AudioUnit,
    /// Steinberg VST3 component.
    Vst3,
}

impl AudioPluginFormat {
    /// Returns the stable public and persistence code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::AudioUnit => "audio_unit",
            Self::Vst3 => "vst3",
        }
    }

    const fn binary_code(self) -> u8 {
        match self {
            Self::AudioUnit => 1,
            Self::Vst3 => 2,
        }
    }

    fn from_binary_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::AudioUnit),
            2 => Some(Self::Vst3),
            _ => None,
        }
    }
}

/// Exact native identity used to reconnect durable state after discovery or an upgrade.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AudioPluginIdentity {
    format: AudioPluginFormat,
    vendor: String,
    identifier: String,
    version: String,
}

impl AudioPluginIdentity {
    /// Validates one complete format-specific identity without normalizing its bytes.
    pub fn new(
        format: AudioPluginFormat,
        vendor: impl Into<String>,
        identifier: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self> {
        let identity = Self {
            format,
            vendor: vendor.into(),
            identifier: identifier.into(),
            version: version.into(),
        };
        identity.validate(ErrorCategory::InvalidInput)?;
        Ok(identity)
    }

    /// Returns the native plugin format.
    #[must_use]
    pub const fn format(&self) -> AudioPluginFormat {
        self.format
    }

    /// Returns the retained vendor identity.
    #[must_use]
    pub fn vendor(&self) -> &str {
        &self.vendor
    }

    /// Returns the exact format-specific component identity.
    #[must_use]
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Returns the native component version observed with this state.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns whether two saved identities name the same format-specific component.
    ///
    /// Vendor and version remain retained evidence, but the native component identifier is the
    /// stable compatibility key across an installed upgrade.
    #[must_use]
    pub fn is_same_component(&self, other: &Self) -> bool {
        self.format == other.format && self.identifier == other.identifier
    }

    fn validate(&self, category: ErrorCategory) -> Result<()> {
        for (field, value) in [
            ("vendor", self.vendor.as_str()),
            ("identifier", self.identifier.as_str()),
            ("version", self.version.as_str()),
        ] {
            if value.is_empty()
                || value.len() > MAX_AUDIO_PLUGIN_IDENTITY_FIELD_BYTES
                || value.chars().any(char::is_control)
            {
                return Err(plugin_error(
                    category,
                    "validate_identity",
                    "audio plugin identity field is empty, invalid, or exceeds its bound",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "validate_identity").with_field("field", field),
                ));
            }
        }
        Ok(())
    }
}

/// Versioned format-neutral envelope around exact native component and controller bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPluginState {
    identity: AudioPluginIdentity,
    sample_rate: u32,
    native_latency_samples: usize,
    transport_latency_samples: usize,
    component_state: Vec<u8>,
    controller_state: Vec<u8>,
}

impl AudioPluginState {
    /// Creates a bounded state checkpoint for one exact native component identity.
    pub fn new(
        identity: AudioPluginIdentity,
        sample_rate: u32,
        native_latency_samples: usize,
        transport_latency_samples: usize,
        component_state: Vec<u8>,
        controller_state: Vec<u8>,
    ) -> Result<Self> {
        identity.validate(ErrorCategory::InvalidInput)?;
        if sample_rate == 0 {
            return Err(plugin_error(
                ErrorCategory::InvalidInput,
                "create_state",
                "audio plugin state sample rate must be positive",
            ));
        }
        validate_state_lengths(
            component_state.len(),
            controller_state.len(),
            ErrorCategory::ResourceExhausted,
        )?;
        let _ = u64::try_from(native_latency_samples).map_err(|_| {
            plugin_error(
                ErrorCategory::ResourceExhausted,
                "create_state",
                "audio plugin native latency exceeds the durable sample domain",
            )
        })?;
        let _ = u64::try_from(transport_latency_samples).map_err(|_| {
            plugin_error(
                ErrorCategory::ResourceExhausted,
                "create_state",
                "audio plugin transport latency exceeds the durable sample domain",
            )
        })?;
        let _ = native_latency_samples
            .checked_add(transport_latency_samples)
            .ok_or_else(|| {
                plugin_error(
                    ErrorCategory::ResourceExhausted,
                    "create_state",
                    "audio plugin total latency exceeds the local sample domain",
                )
            })?;
        Ok(Self {
            identity,
            sample_rate,
            native_latency_samples,
            transport_latency_samples,
            component_state,
            controller_state,
        })
    }

    /// Returns the exact native identity associated with the state.
    #[must_use]
    pub const fn identity(&self) -> &AudioPluginIdentity {
        &self.identity
    }

    /// Returns the sample rate used to interpret retained latency samples.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the native algorithmic latency observed at capture.
    #[must_use]
    pub const fn native_latency_samples(&self) -> usize {
        self.native_latency_samples
    }

    /// Returns the fixed isolated transport latency observed at capture.
    #[must_use]
    pub const fn transport_latency_samples(&self) -> usize {
        self.transport_latency_samples
    }

    /// Returns the complete delayed-dry and graph-compensation latency.
    #[must_use]
    pub const fn total_latency_samples(&self) -> usize {
        self.native_latency_samples + self.transport_latency_samples
    }

    /// Returns exact native component-state bytes.
    #[must_use]
    pub fn component_state(&self) -> &[u8] {
        &self.component_state
    }

    /// Returns exact native controller-state bytes.
    #[must_use]
    pub fn controller_state(&self) -> &[u8] {
        &self.controller_state
    }

    /// Encodes the checkpoint into the stable bounded project payload schema.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.identity.validate(ErrorCategory::InvalidInput)?;
        validate_state_lengths(
            self.component_state.len(),
            self.controller_state.len(),
            ErrorCategory::ResourceExhausted,
        )?;
        let total = STATE_HEADER_BYTES
            .checked_add(self.identity.vendor.len())
            .and_then(|value| value.checked_add(self.identity.identifier.len()))
            .and_then(|value| value.checked_add(self.identity.version.len()))
            .and_then(|value| value.checked_add(self.component_state.len()))
            .and_then(|value| value.checked_add(self.controller_state.len()))
            .and_then(|value| value.checked_add(STATE_DIGEST_BYTES))
            .ok_or_else(|| {
                plugin_error(
                    ErrorCategory::ResourceExhausted,
                    "encode_state",
                    "audio plugin state envelope size overflowed",
                )
            })?;
        let mut encoded = Vec::new();
        encoded.try_reserve_exact(total).map_err(|_| {
            plugin_error(
                ErrorCategory::ResourceExhausted,
                "encode_state",
                "audio plugin state envelope allocation failed",
            )
        })?;
        encoded.extend_from_slice(STATE_MAGIC);
        encoded.extend_from_slice(&STATE_SCHEMA.to_le_bytes());
        encoded.push(self.identity.format.binary_code());
        encoded.push(0);
        encoded.extend_from_slice(&self.sample_rate.to_le_bytes());
        encoded.extend_from_slice(
            &u64::try_from(self.native_latency_samples)
                .expect("state construction validated native latency")
                .to_le_bytes(),
        );
        encoded.extend_from_slice(
            &u64::try_from(self.transport_latency_samples)
                .expect("state construction validated transport latency")
                .to_le_bytes(),
        );
        push_u16_length(&mut encoded, self.identity.vendor.len());
        push_u16_length(&mut encoded, self.identity.identifier.len());
        push_u16_length(&mut encoded, self.identity.version.len());
        encoded.extend_from_slice(&0_u16.to_le_bytes());
        push_u32_length(&mut encoded, self.component_state.len());
        push_u32_length(&mut encoded, self.controller_state.len());
        encoded.extend_from_slice(self.identity.vendor.as_bytes());
        encoded.extend_from_slice(self.identity.identifier.as_bytes());
        encoded.extend_from_slice(self.identity.version.as_bytes());
        encoded.extend_from_slice(&self.component_state);
        encoded.extend_from_slice(&self.controller_state);
        let digest = Sha256::digest(&encoded);
        encoded.extend_from_slice(&digest);
        debug_assert_eq!(encoded.len(), total);
        Ok(encoded)
    }

    /// Decodes one exact state payload without interpreting native bytes.
    pub fn decode(encoded: &[u8]) -> Result<Self> {
        if encoded.len() < STATE_HEADER_BYTES + STATE_DIGEST_BYTES
            || &encoded[..STATE_MAGIC.len()] != STATE_MAGIC
        {
            return Err(corrupt_state(
                "audio plugin state header is missing or invalid",
            ));
        }
        let mut cursor = STATE_MAGIC.len();
        let schema = read_u16(encoded, &mut cursor)?;
        if schema != STATE_SCHEMA {
            return Err(plugin_error(
                ErrorCategory::Unsupported,
                "decode_state",
                "audio plugin state schema is not supported by this build",
            ));
        }
        let format = AudioPluginFormat::from_binary_code(read_u8(encoded, &mut cursor)?)
            .ok_or_else(|| corrupt_state("audio plugin state format code is invalid"))?;
        if read_u8(encoded, &mut cursor)? != 0 {
            return Err(corrupt_state("audio plugin state reserved byte is nonzero"));
        }
        let sample_rate = read_u32(encoded, &mut cursor)?;
        if sample_rate == 0 {
            return Err(corrupt_state("audio plugin state sample rate is zero"));
        }
        let native_latency_samples =
            usize::try_from(read_u64(encoded, &mut cursor)?).map_err(|_| {
                corrupt_state("audio plugin native latency exceeds the local sample domain")
            })?;
        let transport_latency_samples =
            usize::try_from(read_u64(encoded, &mut cursor)?).map_err(|_| {
                corrupt_state("audio plugin transport latency exceeds the local sample domain")
            })?;
        let _ = native_latency_samples
            .checked_add(transport_latency_samples)
            .ok_or_else(|| corrupt_state("audio plugin total latency overflowed"))?;
        let vendor_len = usize::from(read_u16(encoded, &mut cursor)?);
        let identifier_len = usize::from(read_u16(encoded, &mut cursor)?);
        let version_len = usize::from(read_u16(encoded, &mut cursor)?);
        if read_u16(encoded, &mut cursor)? != 0 {
            return Err(corrupt_state(
                "audio plugin state reserved field is nonzero",
            ));
        }
        let component_len = usize::try_from(read_u32(encoded, &mut cursor)?).map_err(|_| {
            corrupt_state("audio plugin component-state length exceeds the local domain")
        })?;
        let controller_len = usize::try_from(read_u32(encoded, &mut cursor)?).map_err(|_| {
            corrupt_state("audio plugin controller-state length exceeds the local domain")
        })?;
        validate_state_lengths(component_len, controller_len, ErrorCategory::CorruptData)?;
        if [vendor_len, identifier_len, version_len]
            .into_iter()
            .any(|length| length == 0 || length > MAX_AUDIO_PLUGIN_IDENTITY_FIELD_BYTES)
        {
            return Err(corrupt_state(
                "audio plugin state identity length is invalid or exceeds its bound",
            ));
        }
        let payload_end = cursor
            .checked_add(vendor_len)
            .and_then(|value| value.checked_add(identifier_len))
            .and_then(|value| value.checked_add(version_len))
            .and_then(|value| value.checked_add(component_len))
            .and_then(|value| value.checked_add(controller_len))
            .ok_or_else(|| corrupt_state("audio plugin state payload length overflowed"))?;
        let expected = payload_end
            .checked_add(STATE_DIGEST_BYTES)
            .ok_or_else(|| corrupt_state("audio plugin state digest length overflowed"))?;
        if expected != encoded.len() {
            return Err(corrupt_state(
                "audio plugin state payload length does not match its header",
            ));
        }
        let digest = Sha256::digest(&encoded[..payload_end]);
        if digest[..] != encoded[payload_end..] {
            return Err(corrupt_state("audio plugin state digest does not match"));
        }
        let vendor = read_utf8(encoded, &mut cursor, vendor_len, "vendor")?;
        let identifier = read_utf8(encoded, &mut cursor, identifier_len, "identifier")?;
        let version = read_utf8(encoded, &mut cursor, version_len, "version")?;
        let component_state = take(encoded, &mut cursor, component_len)?.to_vec();
        let controller_state = take(encoded, &mut cursor, controller_len)?.to_vec();
        let identity = AudioPluginIdentity {
            format,
            vendor,
            identifier,
            version,
        };
        identity.validate(ErrorCategory::CorruptData)?;
        Ok(Self {
            identity,
            sample_rate,
            native_latency_samples,
            transport_latency_samples,
            component_state,
            controller_state,
        })
    }
}

/// Nonblocking result of one isolated worker process attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum AudioPluginBridgeStatus {
    /// The bridge wrote the exact latency-aligned wet output.
    Produced,
    /// No matching worker output was ready, so the host must use delayed dry audio.
    Unavailable,
    /// The worker generation faulted and must be recovered off the audio thread.
    Faulted,
}

/// Real-time side of one bounded out-of-process native audio plugin bridge.
///
/// Implementations must use preallocated bounded transport, return immediately, never load native
/// plugin code in the editor process, and write output aligned to the fixed transport plus native
/// algorithmic latency declared at preparation.
pub trait IsolatedAudioPluginProcessBridge: Send {
    /// Returns the fixed transport delay added by the isolated bridge in sample frames.
    fn fixed_transport_latency_samples(&self) -> usize;

    /// Attempts one complete nonblocking process exchange.
    fn try_process(
        &mut self,
        start_time: SampleTime,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<AudioPluginBridgeStatus>;
}

#[derive(Debug)]
struct RuntimeTelemetry {
    faulted: AtomicBool,
    processed_blocks: AtomicU64,
    produced_blocks: AtomicU64,
    delayed_dry_blocks: AtomicU64,
    worker_faults: AtomicU64,
    last_start_sample: AtomicI64,
}

impl RuntimeTelemetry {
    fn new() -> Self {
        Self {
            faulted: AtomicBool::new(false),
            processed_blocks: AtomicU64::new(0),
            produced_blocks: AtomicU64::new(0),
            delayed_dry_blocks: AtomicU64::new(0),
            worker_faults: AtomicU64::new(0),
            last_start_sample: AtomicI64::new(0),
        }
    }
}

/// Cloneable control-side view of isolated plugin real-time health.
#[derive(Clone, Debug)]
pub struct AudioPluginRuntimeReadings {
    telemetry: Arc<RuntimeTelemetry>,
}

impl AudioPluginRuntimeReadings {
    /// Captures one internally coherent monotonic telemetry snapshot.
    #[must_use]
    pub fn snapshot(&self) -> AudioPluginRuntimeSnapshot {
        let processed_blocks = self.telemetry.processed_blocks.load(Ordering::Acquire);
        AudioPluginRuntimeSnapshot {
            faulted: self.telemetry.faulted.load(Ordering::Acquire),
            processed_blocks,
            produced_blocks: self.telemetry.produced_blocks.load(Ordering::Relaxed),
            delayed_dry_blocks: self.telemetry.delayed_dry_blocks.load(Ordering::Relaxed),
            worker_faults: self.telemetry.worker_faults.load(Ordering::Relaxed),
            last_start_sample: (processed_blocks != 0)
                .then(|| self.telemetry.last_start_sample.load(Ordering::Relaxed)),
        }
    }
}

/// Immutable isolated audio plugin health counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioPluginRuntimeSnapshot {
    faulted: bool,
    processed_blocks: u64,
    produced_blocks: u64,
    delayed_dry_blocks: u64,
    worker_faults: u64,
    last_start_sample: Option<i64>,
}

impl AudioPluginRuntimeSnapshot {
    /// Returns whether the current worker generation faulted.
    #[must_use]
    pub const fn is_faulted(self) -> bool {
        self.faulted
    }

    /// Returns the number of callback blocks handled by this processor.
    #[must_use]
    pub const fn processed_blocks(self) -> u64 {
        self.processed_blocks
    }

    /// Returns the number of wet worker blocks published.
    #[must_use]
    pub const fn produced_blocks(self) -> u64 {
        self.produced_blocks
    }

    /// Returns the number of blocks served by timing-matched dry fallback.
    #[must_use]
    pub const fn delayed_dry_blocks(self) -> u64 {
        self.delayed_dry_blocks
    }

    /// Returns the number of worker faults contained by this processor.
    #[must_use]
    pub const fn worker_faults(self) -> u64 {
        self.worker_faults
    }

    /// Returns the first sample of the latest processed block.
    #[must_use]
    pub const fn last_start_sample(self) -> Option<i64> {
        self.last_start_sample
    }
}

/// Prepared graph processor for one isolated native plugin worker generation.
pub struct PreparedIsolatedAudioPlugin {
    bridge: Box<dyn IsolatedAudioPluginProcessBridge>,
    sample_rate: u32,
    layout: ChannelLayout,
    maximum_frames: usize,
    latency_samples: usize,
    delayed_dry_ring: Vec<f32>,
    delayed_dry_cursor: usize,
    wet_output: Vec<f32>,
    telemetry: Arc<RuntimeTelemetry>,
}

impl PreparedIsolatedAudioPlugin {
    /// Allocates all callback storage and fixes transport plus native latency.
    pub fn new(
        bridge: Box<dyn IsolatedAudioPluginProcessBridge>,
        sample_rate: u32,
        layout: ChannelLayout,
        maximum_frames: usize,
        plugin_latency_samples: usize,
    ) -> Result<(Self, AudioPluginRuntimeReadings)> {
        if sample_rate == 0 || maximum_frames == 0 || maximum_frames > MAXIMUM_FRAMES {
            return Err(plugin_error(
                ErrorCategory::InvalidInput,
                "prepare_isolated_plugin",
                "isolated plugin clock or process bound is invalid",
            ));
        }
        let transport_latency = bridge.fixed_transport_latency_samples();
        let latency_samples = transport_latency
            .checked_add(plugin_latency_samples)
            .ok_or_else(|| {
                plugin_error(
                    ErrorCategory::ResourceExhausted,
                    "prepare_isolated_plugin",
                    "isolated plugin total latency overflowed",
                )
            })?;
        let channel_count = layout.len();
        let delay_cells = latency_samples.checked_mul(channel_count).ok_or_else(|| {
            plugin_error(
                ErrorCategory::ResourceExhausted,
                "prepare_isolated_plugin",
                "isolated plugin dry-delay storage overflowed",
            )
        })?;
        let process_cells = maximum_frames.checked_mul(channel_count).ok_or_else(|| {
            plugin_error(
                ErrorCategory::ResourceExhausted,
                "prepare_isolated_plugin",
                "isolated plugin process storage overflowed",
            )
        })?;
        let telemetry = Arc::new(RuntimeTelemetry::new());
        let readings = AudioPluginRuntimeReadings {
            telemetry: Arc::clone(&telemetry),
        };
        Ok((
            Self {
                bridge,
                sample_rate,
                layout,
                maximum_frames,
                latency_samples,
                delayed_dry_ring: zeroed_samples(delay_cells)?,
                delayed_dry_cursor: 0,
                wet_output: zeroed_samples(process_cells)?,
                telemetry,
            },
            readings,
        ))
    }

    fn write_delayed_dry(&mut self, input: &[f32], output: &mut [f32]) {
        if self.delayed_dry_ring.is_empty() {
            output.copy_from_slice(input);
            return;
        }
        for (input, output) in input.iter().copied().zip(output.iter_mut()) {
            *output = self.delayed_dry_ring[self.delayed_dry_cursor];
            self.delayed_dry_ring[self.delayed_dry_cursor] = input;
            self.delayed_dry_cursor += 1;
            if self.delayed_dry_cursor == self.delayed_dry_ring.len() {
                self.delayed_dry_cursor = 0;
            }
        }
    }
}

impl AudioProcessor for PreparedIsolatedAudioPlugin {
    fn latency_samples(&self) -> usize {
        self.latency_samples
    }

    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        ExecutionDomain::Audio.require_current()?;
        let input = block.input.ok_or_else(|| {
            plugin_error(
                ErrorCategory::InvalidInput,
                "process_isolated_plugin",
                "isolated audio plugin requires one connected input",
            )
        })?;
        let expected_samples = block
            .frame_count
            .checked_mul(self.layout.len())
            .ok_or_else(|| {
                plugin_error(
                    ErrorCategory::InvalidInput,
                    "process_isolated_plugin",
                    "isolated audio plugin block sample count overflowed",
                )
            })?;
        if block.start_time.sample_rate() != self.sample_rate
            || block.input_layout != Some(&self.layout)
            || block.output_layout != &self.layout
            || block.frame_count == 0
            || block.frame_count > self.maximum_frames
            || input.len() != expected_samples
            || block.output.len() != expected_samples
        {
            return Err(plugin_error(
                ErrorCategory::InvalidInput,
                "process_isolated_plugin",
                "isolated audio plugin block does not match its prepared contract",
            ));
        }

        self.write_delayed_dry(input, block.output);
        let sample_count = block.output.len();
        let mut produced = false;
        let mut faulted_now = false;
        if !self.telemetry.faulted.load(Ordering::Acquire) {
            self.wet_output[..sample_count].fill(f32::NAN);
            match self.bridge.try_process(
                block.start_time,
                input,
                &mut self.wet_output[..sample_count],
            ) {
                Ok(AudioPluginBridgeStatus::Produced)
                    if self.wet_output[..sample_count]
                        .iter()
                        .all(|sample| sample.is_finite()) =>
                {
                    block
                        .output
                        .copy_from_slice(&self.wet_output[..sample_count]);
                    produced = true;
                }
                Ok(AudioPluginBridgeStatus::Unavailable) => {}
                Ok(AudioPluginBridgeStatus::Produced | AudioPluginBridgeStatus::Faulted)
                | Err(_) => {
                    self.telemetry.faulted.store(true, Ordering::Release);
                    self.telemetry.worker_faults.fetch_add(1, Ordering::Relaxed);
                    faulted_now = true;
                }
            }
        }
        if produced {
            self.telemetry
                .produced_blocks
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.telemetry
                .delayed_dry_blocks
                .fetch_add(1, Ordering::Relaxed);
        }
        if faulted_now {
            debug_assert!(self.telemetry.faulted.load(Ordering::Acquire));
        }
        self.telemetry
            .last_start_sample
            .store(block.start_time.sample(), Ordering::Relaxed);
        self.telemetry
            .processed_blocks
            .fetch_add(1, Ordering::Release);
        Ok(())
    }
}

fn validate_state_lengths(
    component_len: usize,
    controller_len: usize,
    category: ErrorCategory,
) -> Result<()> {
    let total = component_len.checked_add(controller_len).ok_or_else(|| {
        plugin_error(
            category,
            "validate_state",
            "audio plugin state payload size overflowed",
        )
    })?;
    if component_len > MAX_AUDIO_PLUGIN_STATE_BYTES
        || controller_len > MAX_AUDIO_PLUGIN_STATE_BYTES
        || total > MAX_AUDIO_PLUGIN_STATE_TOTAL_BYTES
    {
        return Err(plugin_error(
            category,
            "validate_state",
            "audio plugin state payload exceeds its explicit bound",
        ));
    }
    Ok(())
}

fn zeroed_samples(sample_count: usize) -> Result<Vec<f32>> {
    let mut samples = Vec::new();
    samples.try_reserve_exact(sample_count).map_err(|_| {
        plugin_error(
            ErrorCategory::ResourceExhausted,
            "prepare_isolated_plugin",
            "isolated audio plugin callback storage allocation failed",
        )
    })?;
    samples.resize(sample_count, 0.0);
    Ok(samples)
}

fn push_u16_length(encoded: &mut Vec<u8>, length: usize) {
    encoded.extend_from_slice(
        &u16::try_from(length)
            .expect("identity field bound fits u16")
            .to_le_bytes(),
    );
}

fn push_u32_length(encoded: &mut Vec<u8>, length: usize) {
    encoded.extend_from_slice(
        &u32::try_from(length)
            .expect("native state bound fits u32")
            .to_le_bytes(),
    );
}

fn read_u8(encoded: &[u8], cursor: &mut usize) -> Result<u8> {
    Ok(take(encoded, cursor, 1)?[0])
}

fn read_u16(encoded: &[u8], cursor: &mut usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        take(encoded, cursor, 2)?
            .try_into()
            .expect("requested exact u16 bytes"),
    ))
}

fn read_u32(encoded: &[u8], cursor: &mut usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        take(encoded, cursor, 4)?
            .try_into()
            .expect("requested exact u32 bytes"),
    ))
}

fn read_u64(encoded: &[u8], cursor: &mut usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        take(encoded, cursor, 8)?
            .try_into()
            .expect("requested exact u64 bytes"),
    ))
}

fn read_utf8(
    encoded: &[u8],
    cursor: &mut usize,
    length: usize,
    field: &'static str,
) -> Result<String> {
    String::from_utf8(take(encoded, cursor, length)?.to_vec()).map_err(|source| {
        Error::with_source(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "audio plugin state identity is not valid UTF-8",
            source,
        )
        .with_context(ErrorContext::new(COMPONENT, "decode_state").with_field("field", field))
    })
}

fn take<'a>(encoded: &'a [u8], cursor: &mut usize, length: usize) -> Result<&'a [u8]> {
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| corrupt_state("audio plugin state cursor overflowed"))?;
    let value = encoded
        .get(*cursor..end)
        .ok_or_else(|| corrupt_state("audio plugin state payload is truncated"))?;
    *cursor = end;
    Ok(value)
}

fn corrupt_state(message: &'static str) -> Error {
    plugin_error(ErrorCategory::CorruptData, "decode_state", message)
}

fn plugin_error(category: ErrorCategory, operation: &'static str, message: &'static str) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
