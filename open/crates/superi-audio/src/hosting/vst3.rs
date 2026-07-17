//! Prepared VST3 audio-effect hosting for a dedicated plugin worker.
//!
//! This module exposes only safe Rust configuration, processing, automation, and monitoring values.
//! Native modules, COM pointers, and VST3 process structures remain private to [`native`]. Loading
//! this module in the main editor process is unsupported. A worker supervisor and production
//! transport belong to the later plugin lifecycle checkpoint.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;

use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::ChannelLayout;
use superi_core::time::SampleTime;

use crate::graph::{AudioProcessBlock, AudioProcessor};

mod native;

const COMPONENT: &str = "superi-audio.hosting.vst3";
const DEFAULT_AUTOMATION_CAPACITY: usize = 1_024;
const DEFAULT_MAXIMUM_AUTOMATION_POINTS_PER_BLOCK: usize = 256;
const DEFAULT_MONITORING_CAPACITY: usize = 1_024;
const MAXIMUM_PREPARED_PLANAR_SAMPLES: usize = 1_048_576;
const MAXIMUM_HANDOFF_POINTS: usize = 1_048_576;

/// Canonical VST3 mono arrangement containing one center speaker.
pub const VST3_SPEAKER_MONO: u64 = 524_288;
/// Canonical VST3 left and right stereo arrangement.
pub const VST3_SPEAKER_STEREO: u64 = 3;
/// Canonical VST3 left, right, left-surround, and right-surround quad arrangement.
pub const VST3_SPEAKER_QUAD: u64 = 51;
/// Canonical VST3 left, right, center, LFE, left-surround, and right-surround arrangement.
pub const VST3_SPEAKER_5_1: u64 = 63;
/// Canonical VST3 7.1 music arrangement with rear and side pairs.
pub const VST3_SPEAKER_7_1: u64 = 1_599;

/// One immutable VST3 class identifier represented by its four canonical words.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Vst3ClassId([u32; 4]);

impl Vst3ClassId {
    /// Creates an identifier from the four words used by VST3 `INLINE_UID` declarations.
    #[must_use]
    pub const fn new(a: u32, b: u32, c: u32, d: u32) -> Self {
        Self([a, b, c, d])
    }

    /// Returns the canonical four-word identity.
    #[must_use]
    pub const fn words(self) -> [u32; 4] {
        self.0
    }

    pub(crate) const fn tuid(self) -> [i8; 16] {
        vst3::uid(self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

impl FromStr for Vst3ClassId {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid(
                "parse_class_id",
                "VST3 class identity must contain exactly 32 hexadecimal characters",
            ));
        }

        let mut words = [0_u32; 4];
        for (index, word) in words.iter_mut().enumerate() {
            let start = index * 8;
            *word = u32::from_str_radix(&value[start..start + 8], 16).map_err(|_| {
                invalid(
                    "parse_class_id",
                    "VST3 class identity contains invalid hexadecimal data",
                )
            })?;
        }
        Ok(Self(words))
    }
}

impl fmt::Display for Vst3ClassId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:08X}{:08X}{:08X}{:08X}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}

/// VST3 processing behavior selected during preparation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Vst3ProcessMode {
    /// Deadline-sensitive playback processing.
    Realtime,
    /// Deterministic non-realtime render or export processing.
    Offline,
}

impl Vst3ProcessMode {
    pub(crate) const fn native_code(self) -> i32 {
        match self {
            Self::Realtime => 0,
            Self::Offline => 2,
        }
    }
}

/// Complete bounded preparation request for one explicit VST3 audio-effect class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Vst3EffectConfig {
    bundle_path: PathBuf,
    class_id: Vst3ClassId,
    sample_rate: u32,
    layout: ChannelLayout,
    maximum_frames: usize,
    speaker_arrangement: u64,
    process_mode: Vst3ProcessMode,
    automation_capacity: usize,
    maximum_automation_points_per_block: usize,
    monitoring_capacity: usize,
}

impl Vst3EffectConfig {
    /// Creates a validated configuration for one canonical-layout f32 audio effect.
    pub fn new(
        bundle_path: impl Into<PathBuf>,
        class_id: Vst3ClassId,
        sample_rate: u32,
        layout: ChannelLayout,
        maximum_frames: usize,
    ) -> Result<Self> {
        let bundle_path = bundle_path.into();
        if bundle_path.as_os_str().is_empty() {
            return Err(invalid(
                "create_config",
                "VST3 bundle path must not be empty",
            ));
        }
        if sample_rate == 0 {
            return Err(invalid(
                "create_config",
                "VST3 sample rate must be positive",
            ));
        }
        if maximum_frames == 0 || maximum_frames > i32::MAX as usize {
            return Err(invalid(
                "create_config",
                "VST3 maximum frame count must fit a positive signed 32-bit value",
            ));
        }
        let planar_samples = maximum_frames.checked_mul(layout.len()).ok_or_else(|| {
            invalid(
                "create_config",
                "VST3 planar sample storage exceeds the supported size",
            )
        })?;
        if planar_samples > MAXIMUM_PREPARED_PLANAR_SAMPLES {
            return Err(invalid(
                "create_config",
                "VST3 planar sample storage exceeds the explicit preparation bound",
            ));
        }
        let speaker_arrangement = speaker_arrangement(&layout)?;

        Ok(Self {
            bundle_path,
            class_id,
            sample_rate,
            layout,
            maximum_frames,
            speaker_arrangement,
            process_mode: Vst3ProcessMode::Realtime,
            automation_capacity: DEFAULT_AUTOMATION_CAPACITY,
            maximum_automation_points_per_block: DEFAULT_MAXIMUM_AUTOMATION_POINTS_PER_BLOCK,
            monitoring_capacity: DEFAULT_MONITORING_CAPACITY,
        })
    }

    /// Selects real-time or offline VST3 process behavior.
    #[must_use]
    pub const fn with_process_mode(mut self, process_mode: Vst3ProcessMode) -> Self {
        self.process_mode = process_mode;
        self
    }

    /// Sets the bounded control-to-audio queue and per-block automation capacities.
    pub fn with_automation_limits(
        mut self,
        automation_capacity: usize,
        maximum_points_per_block: usize,
    ) -> Result<Self> {
        if automation_capacity == 0 || maximum_points_per_block == 0 {
            return Err(invalid(
                "configure_automation",
                "VST3 automation capacities must be positive",
            ));
        }
        if automation_capacity > MAXIMUM_HANDOFF_POINTS
            || maximum_points_per_block > MAXIMUM_HANDOFF_POINTS
            || maximum_points_per_block > automation_capacity
        {
            return Err(invalid(
                "configure_automation",
                "VST3 automation capacities exceed the bounded handoff or queue relationship",
            ));
        }
        self.automation_capacity = automation_capacity;
        self.maximum_automation_points_per_block = maximum_points_per_block;
        Ok(self)
    }

    /// Sets the bounded audio-to-control output-parameter queue capacity.
    pub fn with_monitoring_capacity(mut self, monitoring_capacity: usize) -> Result<Self> {
        if monitoring_capacity == 0 || monitoring_capacity > MAXIMUM_HANDOFF_POINTS {
            return Err(invalid(
                "configure_monitoring",
                "VST3 monitoring capacity must fit the positive bounded handoff",
            ));
        }
        self.monitoring_capacity = monitoring_capacity;
        Ok(self)
    }

    /// Returns the explicit plugin bundle or module path.
    #[must_use]
    pub fn bundle_path(&self) -> &Path {
        &self.bundle_path
    }

    /// Returns the exact requested class identity.
    #[must_use]
    pub const fn class_id(&self) -> Vst3ClassId {
        self.class_id
    }

    /// Returns the integral processing sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the exact ordered Superi channel layout.
    #[must_use]
    pub const fn layout(&self) -> &ChannelLayout {
        &self.layout
    }

    /// Returns the positive maximum callback frame count.
    #[must_use]
    pub const fn maximum_frames(&self) -> usize {
        self.maximum_frames
    }

    /// Returns the exact canonical VST3 speaker mask for the prepared layout.
    #[must_use]
    pub const fn speaker_arrangement(&self) -> u64 {
        self.speaker_arrangement
    }

    /// Returns the selected process mode.
    #[must_use]
    pub const fn process_mode(&self) -> Vst3ProcessMode {
        self.process_mode
    }

    /// Returns the control-to-audio automation queue capacity.
    #[must_use]
    pub const fn automation_capacity(&self) -> usize {
        self.automation_capacity
    }

    /// Returns the maximum automation points admitted to one process block.
    #[must_use]
    pub const fn maximum_automation_points_per_block(&self) -> usize {
        self.maximum_automation_points_per_block
    }

    /// Returns the audio-to-control output-parameter queue capacity.
    #[must_use]
    pub const fn monitoring_capacity(&self) -> usize {
        self.monitoring_capacity
    }
}

/// Immutable parameter metadata reported by the prepared VST3 controller.
#[derive(Clone, Debug, PartialEq)]
pub struct Vst3ParameterInfo {
    id: u32,
    title: String,
    default_normalized_value: f64,
    automatable: bool,
    read_only: bool,
}

impl Vst3ParameterInfo {
    /// Returns the VST3 parameter identity.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the controller-supplied parameter title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the controller-supplied normalized default value.
    #[must_use]
    pub const fn default_normalized_value(&self) -> f64 {
        self.default_normalized_value
    }

    /// Returns whether the parameter accepts sample-offset process automation.
    #[must_use]
    pub const fn is_automatable(&self) -> bool {
        self.automatable
    }

    /// Returns whether the controller reports the parameter as read-only.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }
}

/// Immutable prepared VST3 factory, component, and process metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Vst3EffectMetadata {
    factory_vendor: String,
    component_name: String,
    class_id: Vst3ClassId,
    sample_rate: u32,
    layout: ChannelLayout,
    process_mode: Vst3ProcessMode,
    latency_samples: u32,
    tail_samples: u32,
    parameters: Arc<[Vst3ParameterInfo]>,
}

impl Vst3EffectMetadata {
    /// Returns the factory vendor string.
    #[must_use]
    pub fn factory_vendor(&self) -> &str {
        &self.factory_vendor
    }

    /// Returns the selected component name.
    #[must_use]
    pub fn component_name(&self) -> &str {
        &self.component_name
    }

    /// Returns the selected component class identity.
    #[must_use]
    pub const fn class_id(&self) -> Vst3ClassId {
        self.class_id
    }

    /// Returns the exact prepared sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the exact prepared semantic layout.
    #[must_use]
    pub const fn layout(&self) -> &ChannelLayout {
        &self.layout
    }

    /// Returns the selected real-time or offline process mode.
    #[must_use]
    pub const fn process_mode(&self) -> Vst3ProcessMode {
        self.process_mode
    }

    /// Returns plugin-reported processing latency without applying compensation.
    #[must_use]
    pub const fn latency_samples(&self) -> u32 {
        self.latency_samples
    }

    /// Returns plugin-reported tail length without changing graph duration.
    #[must_use]
    pub const fn tail_samples(&self) -> u32 {
        self.tail_samples
    }

    /// Returns controller parameters in stable controller order.
    #[must_use]
    pub fn parameters(&self) -> &[Vst3ParameterInfo] {
        &self.parameters
    }
}

/// One normalized VST3 parameter value at an exact absolute sample coordinate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vst3AutomationPoint {
    parameter_id: u32,
    sample_time: SampleTime,
    normalized_value: f64,
}

impl Vst3AutomationPoint {
    /// Creates a finite normalized automation point.
    pub fn new(parameter_id: u32, sample_time: SampleTime, normalized_value: f64) -> Result<Self> {
        if !normalized_value.is_finite() || !(0.0..=1.0).contains(&normalized_value) {
            return Err(invalid(
                "create_automation_point",
                "VST3 automation values must be finite and normalized to the inclusive range 0 to 1",
            ));
        }
        Ok(Self {
            parameter_id,
            sample_time,
            normalized_value,
        })
    }

    /// Returns the VST3 parameter identity.
    #[must_use]
    pub const fn parameter_id(self) -> u32 {
        self.parameter_id
    }

    /// Returns the exact absolute sample coordinate.
    #[must_use]
    pub const fn sample_time(self) -> SampleTime {
        self.sample_time
    }

    /// Returns the normalized value.
    #[must_use]
    pub const fn normalized_value(self) -> f64 {
        self.normalized_value
    }
}

/// One plugin-originated parameter value mapped back to an absolute sample coordinate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vst3OutputParameterPoint {
    parameter_id: u32,
    sample_time: SampleTime,
    normalized_value: f64,
}

impl Vst3OutputParameterPoint {
    /// Returns the VST3 parameter identity.
    #[must_use]
    pub const fn parameter_id(self) -> u32 {
        self.parameter_id
    }

    /// Returns the exact absolute sample coordinate.
    #[must_use]
    pub const fn sample_time(self) -> SampleTime {
        self.sample_time
    }

    /// Returns the normalized value.
    #[must_use]
    pub const fn normalized_value(self) -> f64 {
        self.normalized_value
    }
}

#[derive(Debug)]
struct Vst3Telemetry {
    processed_blocks: AtomicU64,
    last_start_sample: AtomicI64,
    automation_rejections: AtomicU64,
    monitoring_overflow: AtomicU64,
    process_failures: AtomicU64,
    nonfinite_output_failures: AtomicU64,
    restart_flags: AtomicU64,
    shutdown_order_violations: AtomicU64,
}

impl Vst3Telemetry {
    fn new() -> Self {
        Self {
            processed_blocks: AtomicU64::new(0),
            last_start_sample: AtomicI64::new(0),
            automation_rejections: AtomicU64::new(0),
            monitoring_overflow: AtomicU64::new(0),
            process_failures: AtomicU64::new(0),
            nonfinite_output_failures: AtomicU64::new(0),
            restart_flags: AtomicU64::new(0),
            shutdown_order_violations: AtomicU64::new(0),
        }
    }
}

/// Coherent scalar telemetry sampled from the control side.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Vst3EffectTelemetrySnapshot {
    processed_blocks: u64,
    last_start_sample: Option<i64>,
    automation_rejections: u64,
    monitoring_overflow: u64,
    process_failures: u64,
    nonfinite_output_failures: u64,
    restart_flags: u64,
    shutdown_order_violations: u64,
}

impl Vst3EffectTelemetrySnapshot {
    /// Returns the number of successful process blocks.
    #[must_use]
    pub const fn processed_blocks(self) -> u64 {
        self.processed_blocks
    }

    /// Returns the exact start sample of the last successful block.
    #[must_use]
    pub const fn last_start_sample(self) -> Option<i64> {
        self.last_start_sample
    }

    /// Returns rejected stale or over-capacity automation block count.
    #[must_use]
    pub const fn automation_rejections(self) -> u64 {
        self.automation_rejections
    }

    /// Returns plugin output points dropped by bounded monitoring backpressure.
    #[must_use]
    pub const fn monitoring_overflow(self) -> u64 {
        self.monitoring_overflow
    }

    /// Returns VST3 process-call failures.
    #[must_use]
    pub const fn process_failures(self) -> u64 {
        self.process_failures
    }

    /// Returns blocks rejected because the plugin produced nonfinite audio.
    #[must_use]
    pub const fn nonfinite_output_failures(self) -> u64 {
        self.nonfinite_output_failures
    }

    /// Returns all component restart flags observed so far.
    #[must_use]
    pub const fn restart_flags(self) -> u64 {
        self.restart_flags
    }

    /// Returns attempts to shut down while the prepared graph node remained leased.
    #[must_use]
    pub const fn shutdown_order_violations(self) -> u64 {
        self.shutdown_order_violations
    }
}

/// Single-producer control-side automation sink for one prepared VST3 effect.
pub struct Vst3AutomationWriter {
    sample_rate: u32,
    parameter_ids: Arc<[u32]>,
    ring: HeapProd<Vst3AutomationPoint>,
    last_submitted_sample: Option<i64>,
}

impl Vst3AutomationWriter {
    /// Atomically admits a complete nondecreasing slice or leaves the queue unchanged.
    pub fn submit(&mut self, points: &[Vst3AutomationPoint]) -> Result<()> {
        let mut previous = self.last_submitted_sample;
        for point in points {
            if point.sample_time.sample_rate() != self.sample_rate {
                return Err(invalid(
                    "submit_automation",
                    "VST3 automation must use the prepared sample rate",
                ));
            }
            if self
                .parameter_ids
                .binary_search(&point.parameter_id)
                .is_err()
            {
                return Err(Error::new(
                    ErrorCategory::NotFound,
                    Recoverability::UserCorrectable,
                    "VST3 automation does not identify an automatable writable controller parameter",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "submit_automation")
                        .with_field("parameter_id", point.parameter_id.to_string()),
                ));
            }
            if !point.normalized_value.is_finite() || !(0.0..=1.0).contains(&point.normalized_value)
            {
                return Err(invalid(
                    "submit_automation",
                    "VST3 automation values must remain finite and normalized",
                ));
            }
            if previous.is_some_and(|previous| point.sample_time.sample() < previous) {
                return Err(invalid(
                    "submit_automation",
                    "VST3 automation must be submitted in nondecreasing absolute sample order",
                ));
            }
            previous = Some(point.sample_time.sample());
        }
        if self.ring.vacant_len() < points.len() {
            return Err(Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Retryable,
                "VST3 automation queue does not have capacity for the complete slice",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "submit_automation")
                    .with_field("point_count", points.len().to_string()),
            ));
        }
        for point in points.iter().copied() {
            let admitted = self.ring.try_push(point);
            debug_assert!(admitted.is_ok());
        }
        self.last_submitted_sample = previous;
        Ok(())
    }

    /// Returns the exact prepared sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns parameter IDs accepted by this sink in ascending order.
    #[must_use]
    pub fn parameter_ids(&self) -> &[u32] {
        &self.parameter_ids
    }
}

/// Control-side metadata, output-parameter reader, and scalar telemetry.
pub struct Vst3EffectReadings {
    metadata: Arc<Vst3EffectMetadata>,
    output_ring: HeapCons<Vst3OutputParameterPoint>,
    telemetry: Arc<Vst3Telemetry>,
}

impl Vst3EffectReadings {
    /// Returns immutable preparation metadata.
    #[must_use]
    pub fn metadata(&self) -> &Vst3EffectMetadata {
        &self.metadata
    }

    /// Drains at most `maximum_points` plugin-originated output points.
    #[must_use]
    pub fn drain_output_points(&mut self, maximum_points: usize) -> Vec<Vst3OutputParameterPoint> {
        let count = self.output_ring.occupied_len().min(maximum_points);
        let mut points = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(point) = self.output_ring.try_pop() {
                points.push(point);
            }
        }
        points
    }

    /// Samples lock-free scalar telemetry.
    #[must_use]
    pub fn telemetry(&self) -> Vst3EffectTelemetrySnapshot {
        let processed_blocks = self.telemetry.processed_blocks.load(Ordering::Acquire);
        Vst3EffectTelemetrySnapshot {
            processed_blocks,
            last_start_sample: (processed_blocks != 0)
                .then(|| self.telemetry.last_start_sample.load(Ordering::Acquire)),
            automation_rejections: self.telemetry.automation_rejections.load(Ordering::Relaxed),
            monitoring_overflow: self.telemetry.monitoring_overflow.load(Ordering::Relaxed),
            process_failures: self.telemetry.process_failures.load(Ordering::Relaxed),
            nonfinite_output_failures: self
                .telemetry
                .nonfinite_output_failures
                .load(Ordering::Relaxed),
            restart_flags: self.telemetry.restart_flags.load(Ordering::Relaxed),
            shutdown_order_violations: self
                .telemetry
                .shutdown_order_violations
                .load(Ordering::Relaxed),
        }
    }
}

/// Worker-control owner of one loaded VST3 native module and initialized effect.
///
/// The session must be explicitly shut down after its prepared graph node is dropped. If the
/// session is dropped early, the native lease intentionally remains loaded until worker exit rather
/// than risking callback code executing from an unloaded module.
pub struct Vst3WorkerSession {
    native: Option<Arc<native::NativeLease>>,
    metadata: Arc<Vst3EffectMetadata>,
    telemetry: Arc<Vst3Telemetry>,
}

impl Vst3WorkerSession {
    /// Loads and prepares one explicit class inside the current dedicated plugin worker.
    pub fn load(
        config: Vst3EffectConfig,
    ) -> Result<(
        Self,
        PreparedVst3WorkerEffect,
        Vst3AutomationWriter,
        Vst3EffectReadings,
    )> {
        let (native, native_metadata) = native::NativeLease::load(&config)?;
        let parameters: Arc<[Vst3ParameterInfo]> = native_metadata
            .parameters
            .into_iter()
            .map(|parameter| Vst3ParameterInfo {
                id: parameter.id,
                title: parameter.title,
                default_normalized_value: parameter.default_normalized_value,
                automatable: parameter.automatable,
                read_only: parameter.read_only,
            })
            .collect::<Vec<_>>()
            .into();
        let mut parameter_ids = parameters
            .iter()
            .filter(|parameter| parameter.is_automatable() && !parameter.is_read_only())
            .map(Vst3ParameterInfo::id)
            .collect::<Vec<_>>();
        parameter_ids.sort_unstable();
        parameter_ids.dedup();
        let parameter_ids: Arc<[u32]> = parameter_ids.into();
        let metadata = Arc::new(Vst3EffectMetadata {
            factory_vendor: native_metadata.factory_vendor,
            component_name: native_metadata.component_name,
            class_id: config.class_id,
            sample_rate: config.sample_rate,
            layout: config.layout.clone(),
            process_mode: config.process_mode,
            latency_samples: native_metadata.latency_samples,
            tail_samples: native_metadata.tail_samples,
            parameters,
        });
        let automation_ring = HeapRb::new(config.automation_capacity);
        let (automation_writer_ring, automation_reader_ring) = automation_ring.split();
        let output_ring = HeapRb::new(config.monitoring_capacity);
        let (output_writer_ring, output_reader_ring) = output_ring.split();
        let telemetry = Arc::new(Vst3Telemetry::new());
        let channels = config.layout.len();
        let planar_samples = channels
            .checked_mul(config.maximum_frames)
            .ok_or_else(|| invalid("load", "VST3 planar sample storage overflowed"))?;
        let dummy_point = Vst3AutomationPoint {
            parameter_id: 0,
            sample_time: SampleTime::new(0, config.sample_rate)
                .expect("configuration validated a nonzero sample rate"),
            normalized_value: 0.0,
        };
        let prepared = PreparedVst3WorkerEffect {
            native: Arc::clone(&native),
            config: config.clone(),
            automation_ring: automation_reader_ring,
            output_ring: output_writer_ring,
            telemetry: Arc::clone(&telemetry),
            input_planar: vec![0.0; planar_samples],
            output_planar: vec![0.0; planar_samples],
            automation_peek: vec![dummy_point; config.maximum_automation_points_per_block + 1],
            automation_block: vec![
                native::AutomationPoint::EMPTY;
                config.maximum_automation_points_per_block
            ],
        };
        let writer = Vst3AutomationWriter {
            sample_rate: config.sample_rate,
            parameter_ids,
            ring: automation_writer_ring,
            last_submitted_sample: None,
        };
        let readings = Vst3EffectReadings {
            metadata: Arc::clone(&metadata),
            output_ring: output_reader_ring,
            telemetry: Arc::clone(&telemetry),
        };
        let session = Self {
            native: Some(native),
            metadata,
            telemetry,
        };
        Ok((session, prepared, writer, readings))
    }

    /// Returns immutable metadata for the loaded effect.
    #[must_use]
    pub fn metadata(&self) -> &Vst3EffectMetadata {
        &self.metadata
    }

    /// Performs reverse lifecycle teardown after the prepared node lease has returned.
    pub fn shutdown(&mut self) -> Result<()> {
        let Some(native) = self.native.as_ref() else {
            return Ok(());
        };
        if Arc::strong_count(native) != 1 {
            self.telemetry
                .shutdown_order_violations
                .fetch_add(1, Ordering::Relaxed);
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "VST3 session cannot shut down while its prepared graph node remains leased",
            )
            .with_context(ErrorContext::new(COMPONENT, "shutdown")));
        }
        native.shutdown()?;
        self.native = None;
        Ok(())
    }

    /// Returns true after explicit reverse lifecycle teardown succeeds.
    #[must_use]
    pub const fn is_shutdown(&self) -> bool {
        self.native.is_none()
    }
}

/// Prepared single-owner graph processor backed by one worker-local VST3 instance.
pub struct PreparedVst3WorkerEffect {
    native: Arc<native::NativeLease>,
    config: Vst3EffectConfig,
    automation_ring: HeapCons<Vst3AutomationPoint>,
    output_ring: HeapProd<Vst3OutputParameterPoint>,
    telemetry: Arc<Vst3Telemetry>,
    input_planar: Vec<f32>,
    output_planar: Vec<f32>,
    automation_peek: Vec<Vst3AutomationPoint>,
    automation_block: Vec<native::AutomationPoint>,
}

impl PreparedVst3WorkerEffect {
    fn fail_automation(&self, message: &'static str) -> Error {
        self.telemetry
            .automation_rejections
            .fetch_add(1, Ordering::Relaxed);
        invalid("process_automation", message)
    }
}

impl AudioProcessor for PreparedVst3WorkerEffect {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        ExecutionDomain::Audio.require_current()?;
        let input = block.input.ok_or_else(|| {
            invalid(
                "process",
                "prepared VST3 audio effects require one connected input",
            )
        })?;
        if block.start_time.sample_rate() != self.config.sample_rate {
            return Err(invalid(
                "process",
                "VST3 process block must use the prepared sample rate",
            ));
        }
        if block.input_layout != Some(&self.config.layout)
            || block.output_layout != &self.config.layout
        {
            return Err(invalid(
                "process",
                "VST3 process block layouts must exactly match the prepared semantic layout",
            ));
        }
        if block.frame_count == 0 || block.frame_count > self.config.maximum_frames {
            return Err(invalid(
                "process",
                "VST3 process frame count must be positive and within the prepared bound",
            ));
        }
        let channels = self.config.layout.len();
        let expected_samples = block
            .frame_count
            .checked_mul(channels)
            .ok_or_else(|| invalid("process", "VST3 process sample count overflowed"))?;
        if input.len() != expected_samples || block.output.len() != expected_samples {
            return Err(invalid(
                "process",
                "VST3 process buffers must exactly match frame and channel counts",
            ));
        }
        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(invalid("process", "VST3 input samples must all be finite"));
        }
        let block_end = block
            .start_time
            .sample()
            .checked_add(i64::try_from(block.frame_count).map_err(|_| {
                invalid(
                    "process",
                    "VST3 frame count exceeds the sample coordinate range",
                )
            })?)
            .ok_or_else(|| invalid("process", "VST3 sample coordinate overflowed"))?;

        let peeked = self.automation_ring.peek_slice(&mut self.automation_peek);
        let mut block_automation_count = 0_usize;
        for point in &self.automation_peek[..peeked] {
            let sample = point.sample_time.sample();
            if sample < block.start_time.sample() {
                block.output.fill(0.0);
                return Err(self.fail_automation(
                    "VST3 automation queue contains a stale absolute sample coordinate",
                ));
            }
            if sample >= block_end {
                break;
            }
            if block_automation_count == self.config.maximum_automation_points_per_block {
                block.output.fill(0.0);
                return Err(self.fail_automation(
                    "VST3 automation exceeds the prepared per-block point capacity",
                ));
            }
            block_automation_count += 1;
        }
        for destination in &mut self.automation_block[..block_automation_count] {
            let point = self
                .automation_ring
                .try_pop()
                .expect("peeked single-consumer VST3 automation point remains available");
            *destination = native::AutomationPoint {
                parameter_id: point.parameter_id,
                sample_offset: i32::try_from(
                    point.sample_time.sample() - block.start_time.sample(),
                )
                .expect("in-block offset fits the configured signed frame bound"),
                normalized_value: point.normalized_value,
            };
        }

        let maximum_frames = self.config.maximum_frames;
        let mut input_silence_flags = 0_u64;
        for channel in 0..channels {
            let input_channel = &mut self.input_planar
                [channel * maximum_frames..channel * maximum_frames + block.frame_count];
            let output_channel = &mut self.output_planar
                [channel * maximum_frames..channel * maximum_frames + block.frame_count];
            let mut silent = true;
            for (frame, destination) in input_channel.iter_mut().enumerate() {
                let sample = input[frame * channels + channel];
                *destination = sample;
                silent &= sample == 0.0;
            }
            output_channel.fill(0.0);
            if silent {
                input_silence_flags |= 1_u64 << channel;
            }
        }

        let outcome = match self.native.process(native::ProcessBlock {
            sample_rate: self.config.sample_rate,
            start_sample: block.start_time.sample(),
            frame_count: block.frame_count,
            channel_count: channels,
            channel_stride: maximum_frames,
            process_mode: self.config.process_mode.native_code(),
            input_silence_flags,
            input_planar: &mut self.input_planar,
            output_planar: &mut self.output_planar,
            automation: &self.automation_block[..block_automation_count],
        }) {
            Ok(outcome) => outcome,
            Err(error) => {
                block.output.fill(0.0);
                self.telemetry
                    .process_failures
                    .fetch_add(1, Ordering::Relaxed);
                return Err(error);
            }
        };

        for frame in 0..block.frame_count {
            for channel in 0..channels {
                let sample = if outcome.output_silence_flags & (1_u64 << channel) != 0 {
                    0.0
                } else {
                    self.output_planar[channel * maximum_frames + frame]
                };
                if !sample.is_finite() {
                    block.output.fill(0.0);
                    self.telemetry
                        .nonfinite_output_failures
                        .fetch_add(1, Ordering::Relaxed);
                    return Err(Error::new(
                        ErrorCategory::CorruptData,
                        Recoverability::Degraded,
                        "VST3 plugin produced a nonfinite output sample",
                    )
                    .with_context(ErrorContext::new(COMPONENT, "process")));
                }
                block.output[frame * channels + channel] = sample;
            }
        }

        let output_ring = &mut self.output_ring;
        let telemetry = &self.telemetry;
        let start_sample = block.start_time.sample();
        let sample_rate = self.config.sample_rate;
        let frame_count = block.frame_count;
        self.native.visit_output_points(|point| {
            if point.sample_offset < 0
                || usize::try_from(point.sample_offset)
                    .ok()
                    .map_or(true, |offset| offset >= frame_count)
                || !point.normalized_value.is_finite()
                || !(0.0..=1.0).contains(&point.normalized_value)
            {
                return;
            }
            let absolute_sample = start_sample + i64::from(point.sample_offset);
            let monitored = Vst3OutputParameterPoint {
                parameter_id: point.parameter_id,
                sample_time: SampleTime::new(absolute_sample, sample_rate)
                    .expect("prepared sample rate remains nonzero"),
                normalized_value: point.normalized_value,
            };
            if output_ring.try_push(monitored).is_err() {
                telemetry
                    .monitoring_overflow
                    .fetch_add(1, Ordering::Relaxed);
            }
        });
        self.telemetry
            .restart_flags
            .fetch_or(outcome.restart_flags, Ordering::Relaxed);
        self.telemetry
            .last_start_sample
            .store(block.start_time.sample(), Ordering::Release);
        self.telemetry
            .processed_blocks
            .fetch_add(1, Ordering::Release);
        Ok(())
    }
}

fn speaker_arrangement(layout: &ChannelLayout) -> Result<u64> {
    if layout == &ChannelLayout::mono() {
        Ok(VST3_SPEAKER_MONO)
    } else if layout == &ChannelLayout::stereo() {
        Ok(VST3_SPEAKER_STEREO)
    } else if layout == &ChannelLayout::quad() {
        Ok(VST3_SPEAKER_QUAD)
    } else if layout == &ChannelLayout::surround_5_1() {
        Ok(VST3_SPEAKER_5_1)
    } else if layout == &ChannelLayout::surround_7_1() {
        Ok(VST3_SPEAKER_7_1)
    } else {
        Err(Error::new(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "VST3 hosting supports only canonical mono, stereo, quad, 5.1, and 7.1 layouts",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "map_speaker_arrangement")
                .with_field("channel_count", layout.len().to_string()),
        ))
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
