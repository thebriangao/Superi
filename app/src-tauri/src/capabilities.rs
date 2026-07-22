//! Read-only native capability discovery and advisory cross-session retention.
//!
//! This owner projects existing GPU, audio, codec, and AI declarations. It owns no device,
//! stream, route, codec session, model, project, workspace, or user selection.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use superi_ai::capabilities::{
    discover_local_capabilities, AiRuntimeAvailability, AI_CAPABILITY_SCHEMA_VERSION,
};
use superi_api::api::{
    HardwareAcceleration, MediaBackendTier, MediaCapabilitiesApi, MediaOperation,
};
use superi_api::commands::GetMediaCapabilities;
use superi_audio::capture::{
    discover_input_devices, InputBufferSize, InputCapability, InputDevice, InputSampleFormat,
    InputStreamConfig,
};
use superi_audio::playback::{
    discover_output_devices, OutputBufferSize, OutputCapability, OutputDevice, OutputSampleFormat,
    OutputStreamConfig,
};
use superi_engine::introspection::MediaCapabilities;
use superi_engine::media::media_backend_registry;
use superi_gpu::device::{Backend, DeviceType, GpuInstance, InstanceOptions};
use tauri::State;

const CAPABILITY_SCHEMA_VERSION: u32 = 1;
const CACHE_FILE_NAME: &str = "capabilities-v1.json";
const MAX_CACHE_BYTES: u64 = 8 * 1_024 * 1_024;
const MAX_GPU_ADAPTERS: usize = 32;
const MAX_AUDIO_DEVICES: usize = 128;
const MAX_AUDIO_CAPABILITIES: usize = 1_024;
const MAX_CODEC_BACKENDS: usize = 1_024;
const MAX_CODEC_OPERATIONS: usize = 4_096;
const MAX_PIPELINES: usize = 64;
const MAX_IDENTITY_BYTES: usize = 32 * 1_024;
const MAX_NAME_BYTES: usize = 512;
const MAX_DETAIL_BYTES: usize = 1_024;
const MAX_FAILURE_CODE_BYTES: usize = 128;
const MAX_FAILURE_TITLE_BYTES: usize = 256;
const MAX_FAILURE_ACTION_BYTES: usize = 512;
const MAX_SAFE_JAVASCRIPT_INTEGER: u64 = 9_007_199_254_740_991;

/// Current availability of one capability domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCondition {
    /// Live discovery returned usable capabilities.
    Available,
    /// Discovery returned partial data or retained last-known data.
    Degraded,
    /// No executable capability is currently available.
    Unavailable,
}

/// Whether domain data was observed now or retained from an earlier process session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityFreshness {
    /// Data came from this discovery request.
    Live,
    /// Data is a visible last-known observation and is not current authority.
    Retained,
}

/// State of the advisory local capability cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCacheStatus {
    /// The returned snapshot was published to the cache.
    Current,
    /// At least one returned domain is retained from an earlier observation.
    Retained,
    /// Cache publication is unavailable, while live discovery may still be usable.
    Unavailable,
}

/// Bounded user-safe capability failure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityFailure {
    code: String,
    title: String,
    action: String,
}

impl CapabilityFailure {
    fn new(code: &str, title: &str, action: &str) -> Self {
        Self {
            code: code.to_owned(),
            title: title.to_owned(),
            action: action.to_owned(),
        }
    }

    fn state(operation: &str) -> Self {
        Self::new(
            &format!("capability_state_{operation}_failed"),
            "Hardware capability state is unavailable",
            "Restart Superi, then refresh capabilities.",
        )
    }

    fn persistence(operation: &str) -> Self {
        Self::new(
            &format!("capability_cache_{operation}_failed"),
            "Capability history could not be retained",
            "Continue with the live result, then refresh after restarting Superi.",
        )
    }
}

impl std::fmt::Display for CapabilityFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.title)
    }
}

impl std::error::Error for CapabilityFailure {}

/// One typed capability domain with explicit freshness and failure state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>")
)]
pub struct CapabilityDomain<T> {
    condition: CapabilityCondition,
    freshness: CapabilityFreshness,
    data: Option<T>,
    failure: Option<CapabilityFailure>,
}

impl<T> CapabilityDomain<T> {
    fn live(
        condition: CapabilityCondition,
        data: Option<T>,
        failure: Option<CapabilityFailure>,
    ) -> Self {
        Self {
            condition,
            freshness: CapabilityFreshness::Live,
            data,
            failure,
        }
    }
}

/// Serializable GPU adapter capability projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GpuAdapterCapability {
    id: String,
    name: String,
    backend: String,
    device_type: String,
    vendor_id: u32,
    device_id: u32,
    driver: String,
    driver_info: String,
    feature_bits: String,
    webgpu_compliant: bool,
    max_texture_dimension_2d: u32,
    max_bind_groups: u32,
    max_buffer_size: u64,
}

/// Bounded GPU catalog projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GpuCapabilities {
    adapters: Vec<GpuAdapterCapability>,
    skipped_adapters: u32,
}

/// Serializable callback-buffer constraint in frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioBufferSize {
    /// The operating-system backend reports no useful range.
    Unknown,
    /// Inclusive supported range.
    Range { min: u32, max: u32 },
}

/// Exact default stream configuration reported by an audio backend.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioStreamCapability {
    channels: u16,
    sample_rate: u32,
    sample_format: String,
    buffer_frames: Option<u32>,
}

/// Exact supported stream range reported by an audio backend.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioCapabilityRange {
    channels: u16,
    min_sample_rate: u32,
    max_sample_rate: u32,
    sample_format: String,
    buffer_size: AudioBufferSize,
}

/// One input or output device discovered without constructing a stream.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioDeviceCapability {
    id: String,
    name: String,
    is_default: bool,
    default_config: Option<AudioStreamCapability>,
    capabilities: Vec<AudioCapabilityRange>,
    channel_layout_known: bool,
}

/// Bounded input and output catalog projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioCapabilities {
    outputs: Vec<AudioDeviceCapability>,
    inputs: Vec<AudioDeviceCapability>,
    skipped_output_devices: u32,
    skipped_input_devices: u32,
}

/// One flattened codec backend declaration from the canonical public API.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodecBackendCapability {
    id: String,
    display_name: String,
    priority: u16,
    tier: String,
    hardware_acceleration: String,
    operations: Vec<String>,
    codec_capability_count: u32,
}

/// Effective ranked backend support for one codec operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodecOperationCapability {
    operation: String,
    primary_backends: Vec<String>,
    fallback_backends: Vec<String>,
}

/// Strict shell projection of canonical engine and API codec declarations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodecCapabilities {
    schema_version: String,
    revision: u64,
    backends: Vec<CodecBackendCapability>,
    operations: Vec<CodecOperationCapability>,
}

/// Honest local AI capability boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiCapabilities {
    schema_version: u32,
    runtime: String,
    local_only: bool,
    requires_editable_artifacts: bool,
    available_pipelines: Vec<String>,
}

/// Complete four-domain shell capability snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopCapabilitySnapshot {
    schema_version: u32,
    revision: u64,
    observed_at_unix_ms: u64,
    cache_status: CapabilityCacheStatus,
    persistence_failure: Option<CapabilityFailure>,
    gpu: CapabilityDomain<GpuCapabilities>,
    audio: CapabilityDomain<AudioCapabilities>,
    codecs: CapabilityDomain<CodecCapabilities>,
    ai: CapabilityDomain<AiCapabilities>,
}

impl DesktopCapabilitySnapshot {
    /// Returns the monotonic shell-local observation revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns GPU capability state.
    #[must_use]
    pub const fn gpu(&self) -> &CapabilityDomain<GpuCapabilities> {
        &self.gpu
    }

    /// Returns audio capability state.
    #[must_use]
    pub const fn audio(&self) -> &CapabilityDomain<AudioCapabilities> {
        &self.audio
    }
}

#[derive(Clone)]
struct LiveCapabilities {
    gpu: CapabilityDomain<GpuCapabilities>,
    audio: CapabilityDomain<AudioCapabilities>,
    codecs: CapabilityDomain<CodecCapabilities>,
    ai: CapabilityDomain<AiCapabilities>,
}

#[derive(Default)]
struct CapabilityModel {
    last: Option<DesktopCapabilitySnapshot>,
}

/// Application-owned capability observation and advisory cache state.
#[derive(Clone)]
pub struct DesktopCapabilityState {
    model: Arc<Mutex<CapabilityModel>>,
    persistence_path: Arc<Mutex<Option<PathBuf>>>,
    refresh_gate: Arc<Mutex<()>>,
}

impl Default for DesktopCapabilityState {
    fn default() -> Self {
        Self {
            model: Arc::new(Mutex::new(CapabilityModel::default())),
            persistence_path: Arc::new(Mutex::new(None)),
            refresh_gate: Arc::new(Mutex::new(())),
        }
    }
}

impl DesktopCapabilityState {
    /// Initializes the non-authoritative cache without blocking application startup on failure.
    pub fn initialize(&self, recovery_root: &Path) {
        if std::fs::create_dir_all(recovery_root).is_err() {
            return;
        }
        let path = recovery_root.join(CACHE_FILE_NAME);
        if let Ok(mut slot) = self.persistence_path.lock() {
            *slot = Some(path.clone());
        }
        let Ok(Some(snapshot)) = read_cached_snapshot(&path) else {
            return;
        };
        if let Ok(mut model) = self.model.lock() {
            model.last = Some(mark_snapshot_retained(snapshot));
        }
    }

    fn discover(&self) -> Result<DesktopCapabilitySnapshot, CapabilityFailure> {
        let _gate = self
            .refresh_gate
            .lock()
            .map_err(|_| CapabilityFailure::state("refresh_lock"))?;
        self.commit_observation(discover_live_capabilities(), current_unix_millis())
    }

    fn commit_observation(
        &self,
        live: LiveCapabilities,
        observed_at_unix_ms: u64,
    ) -> Result<DesktopCapabilitySnapshot, CapabilityFailure> {
        let mut model = self
            .model
            .lock()
            .map_err(|_| CapabilityFailure::state("model_lock"))?;
        let previous = model.last.as_ref();
        let revision = previous
            .map_or(0, |snapshot| snapshot.revision)
            .checked_add(1)
            .filter(|revision| *revision <= MAX_SAFE_JAVASCRIPT_INTEGER)
            .ok_or_else(|| CapabilityFailure::state("revision"))?;
        let mut snapshot = DesktopCapabilitySnapshot {
            schema_version: CAPABILITY_SCHEMA_VERSION,
            revision,
            observed_at_unix_ms: observed_at_unix_ms.min(MAX_SAFE_JAVASCRIPT_INTEGER),
            cache_status: CapabilityCacheStatus::Current,
            persistence_failure: None,
            gpu: merge_domain(live.gpu, previous.map(|snapshot| &snapshot.gpu)),
            audio: merge_domain(live.audio, previous.map(|snapshot| &snapshot.audio)),
            codecs: merge_domain(live.codecs, previous.map(|snapshot| &snapshot.codecs)),
            ai: merge_domain(live.ai, previous.map(|snapshot| &snapshot.ai)),
        };
        if [
            snapshot.gpu.freshness,
            snapshot.audio.freshness,
            snapshot.codecs.freshness,
            snapshot.ai.freshness,
        ]
        .contains(&CapabilityFreshness::Retained)
        {
            snapshot.cache_status = CapabilityCacheStatus::Retained;
        }
        if let Err(failure) = self.persist(&snapshot) {
            snapshot.cache_status = CapabilityCacheStatus::Unavailable;
            snapshot.persistence_failure = Some(failure);
        }
        validate_snapshot(&snapshot)?;
        model.last = Some(snapshot.clone());
        Ok(snapshot)
    }

    fn persist(&self, snapshot: &DesktopCapabilitySnapshot) -> Result<(), CapabilityFailure> {
        let path = self
            .persistence_path
            .lock()
            .map_err(|_| CapabilityFailure::persistence("path_lock"))?
            .clone()
            .ok_or_else(|| CapabilityFailure::persistence("not_initialized"))?;
        let bytes = serde_json::to_vec_pretty(snapshot)
            .map_err(|_| CapabilityFailure::persistence("serialize"))?;
        if bytes.len() as u64 > MAX_CACHE_BYTES {
            return Err(CapabilityFailure::persistence("capacity"));
        }
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes).map_err(|_| CapabilityFailure::persistence("write"))?;
        std::fs::rename(&temporary, &path).map_err(|_| CapabilityFailure::persistence("publish"))
    }
}

/// Runs all blocking native discovery outside the Tauri application thread.
#[tauri::command]
pub async fn desktop_capabilities_discover(
    state: State<'_, DesktopCapabilityState>,
) -> Result<DesktopCapabilitySnapshot, CapabilityFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.discover())
        .await
        .map_err(|_| CapabilityFailure::state("join"))?
}

fn discover_live_capabilities() -> LiveCapabilities {
    LiveCapabilities {
        gpu: discover_gpu_capabilities(),
        audio: discover_audio_capabilities(),
        codecs: discover_codec_capabilities(),
        ai: discover_ai_capabilities(),
    }
}

fn discover_gpu_capabilities() -> CapabilityDomain<GpuCapabilities> {
    let instance = match GpuInstance::new(InstanceOptions::default()) {
        Ok(instance) => instance,
        Err(_) => {
            return CapabilityDomain::live(
                CapabilityCondition::Unavailable,
                None,
                Some(CapabilityFailure::new(
                    "gpu_backend_unavailable",
                    "GPU discovery is unavailable",
                    "Update the graphics driver, then refresh capabilities.",
                )),
            )
        }
    };
    let catalog = instance.enumerate_adapters();
    let total = catalog.len();
    let adapters = catalog
        .snapshots()
        .take(MAX_GPU_ADAPTERS)
        .map(|snapshot| {
            let info = snapshot.info();
            let limits = snapshot.capabilities().limits();
            GpuAdapterCapability {
                id: bounded_text(&snapshot.id().to_string(), MAX_NAME_BYTES, false),
                name: bounded_text(&info.name, MAX_NAME_BYTES, false),
                backend: gpu_backend(info.backend).to_owned(),
                device_type: gpu_device_type(info.device_type).to_owned(),
                vendor_id: info.vendor,
                device_id: info.device,
                driver: bounded_text(&info.driver, MAX_NAME_BYTES, true),
                driver_info: bounded_text(&info.driver_info, MAX_DETAIL_BYTES, true),
                feature_bits: format!("{:#x}", snapshot.capabilities().features().bits()),
                webgpu_compliant: snapshot.capabilities().is_webgpu_compliant(),
                max_texture_dimension_2d: limits.max_texture_dimension_2d,
                max_bind_groups: limits.max_bind_groups,
                max_buffer_size: limits.max_buffer_size.min(MAX_SAFE_JAVASCRIPT_INTEGER),
            }
        })
        .collect::<Vec<_>>();
    let skipped = total.saturating_sub(adapters.len());
    let data = GpuCapabilities {
        adapters,
        skipped_adapters: saturating_u32(skipped),
    };
    if data.adapters.is_empty() {
        CapabilityDomain::live(
            CapabilityCondition::Unavailable,
            Some(data),
            Some(CapabilityFailure::new(
                "gpu_adapter_unavailable",
                "No GPU adapter is available",
                "Update the graphics driver or connect a supported GPU, then refresh.",
            )),
        )
    } else if skipped > 0 {
        CapabilityDomain::live(
            CapabilityCondition::Degraded,
            Some(data),
            Some(CapabilityFailure::new(
                "gpu_catalog_truncated",
                "Some GPU adapters were omitted",
                "Use the listed adapters or reduce the number of attached adapters.",
            )),
        )
    } else {
        CapabilityDomain::live(CapabilityCondition::Available, Some(data), None)
    }
}

fn discover_audio_capabilities() -> CapabilityDomain<AudioCapabilities> {
    let outputs = discover_output_devices();
    let inputs = discover_input_devices();
    if outputs.is_err() && inputs.is_err() {
        return CapabilityDomain::live(
            CapabilityCondition::Unavailable,
            None,
            Some(CapabilityFailure::new(
                "audio_discovery_unavailable",
                "Audio devices could not be inspected",
                "Start the system audio service, then refresh capabilities.",
            )),
        );
    }

    let mut output_devices = Vec::new();
    let mut input_devices = Vec::new();
    let mut skipped_outputs = 0_usize;
    let mut skipped_inputs = 0_usize;
    let mut truncated_capabilities = false;
    if let Ok(discovery) = outputs.as_ref() {
        skipped_outputs = discovery.skipped_devices.len();
        truncated_capabilities = discovery
            .devices
            .iter()
            .any(|device| device.capabilities.len() > MAX_AUDIO_CAPABILITIES);
        output_devices = discovery
            .devices
            .iter()
            .take(MAX_AUDIO_DEVICES)
            .map(project_output_device)
            .collect();
        skipped_outputs = skipped_outputs
            .saturating_add(discovery.devices.len().saturating_sub(output_devices.len()));
    }
    if let Ok(discovery) = inputs.as_ref() {
        skipped_inputs = discovery.skipped_devices.len();
        truncated_capabilities |= discovery
            .devices
            .iter()
            .any(|device| device.capabilities.len() > MAX_AUDIO_CAPABILITIES);
        input_devices = discovery
            .devices
            .iter()
            .take(MAX_AUDIO_DEVICES)
            .map(project_input_device)
            .collect();
        skipped_inputs = skipped_inputs
            .saturating_add(discovery.devices.len().saturating_sub(input_devices.len()));
    }

    let data = AudioCapabilities {
        outputs: output_devices,
        inputs: input_devices,
        skipped_output_devices: saturating_u32(skipped_outputs),
        skipped_input_devices: saturating_u32(skipped_inputs),
    };
    let partial_error = match (outputs.is_err(), inputs.is_err()) {
        (true, false) => Some(CapabilityFailure::new(
            "audio_output_discovery_unavailable",
            "Audio outputs could not be inspected",
            "Reconnect the output device, then refresh capabilities.",
        )),
        (false, true) => Some(CapabilityFailure::new(
            "audio_input_discovery_unavailable",
            "Audio inputs could not be inspected",
            "Reconnect the input device, then refresh capabilities.",
        )),
        _ if skipped_outputs > 0 || skipped_inputs > 0 => Some(CapabilityFailure::new(
            "audio_device_discovery_partial",
            "Some audio devices could not be inspected",
            "Reconnect the affected device, then refresh capabilities.",
        )),
        _ if truncated_capabilities => Some(CapabilityFailure::new(
            "audio_capability_catalog_truncated",
            "Some audio capability declarations were omitted",
            "Use the listed configurations or reduce duplicate device declarations.",
        )),
        _ => None,
    };
    if data.outputs.is_empty() && data.inputs.is_empty() {
        CapabilityDomain::live(
            CapabilityCondition::Unavailable,
            Some(data),
            partial_error.or_else(|| {
                Some(CapabilityFailure::new(
                    "audio_device_unavailable",
                    "No audio device is available",
                    "Connect an input or output device, then refresh capabilities.",
                ))
            }),
        )
    } else if partial_error.is_some() {
        CapabilityDomain::live(CapabilityCondition::Degraded, Some(data), partial_error)
    } else {
        CapabilityDomain::live(CapabilityCondition::Available, Some(data), None)
    }
}

fn discover_codec_capabilities() -> CapabilityDomain<CodecCapabilities> {
    let registry = match media_backend_registry() {
        Ok(registry) => registry,
        Err(_) => {
            return CapabilityDomain::live(
                CapabilityCondition::Unavailable,
                None,
                Some(CapabilityFailure::new(
                    "codec_registry_unavailable",
                    "Codec declarations are unavailable",
                    "Restart Superi, then refresh capabilities.",
                )),
            )
        }
    };
    let engine = match MediaCapabilities::from_registry(&registry) {
        Ok(engine) => engine,
        Err(_) => {
            return CapabilityDomain::live(
                CapabilityCondition::Unavailable,
                None,
                Some(CapabilityFailure::new(
                    "codec_declaration_unsupported",
                    "Codec declarations could not be interpreted",
                    "Update Superi, then refresh capabilities.",
                )),
            )
        }
    };
    let api = MediaCapabilitiesApi::new(&engine);
    let result = api.execute(GetMediaCapabilities::new());
    let snapshot = result.snapshot();
    let backends = snapshot
        .backends()
        .iter()
        .take(MAX_CODEC_BACKENDS)
        .map(|backend| CodecBackendCapability {
            id: bounded_text(backend.id(), MAX_NAME_BYTES, false),
            display_name: bounded_text(backend.display_name(), MAX_NAME_BYTES, false),
            priority: backend.priority(),
            tier: codec_tier(backend.tier()).to_owned(),
            hardware_acceleration: codec_acceleration(backend.hardware_acceleration()).to_owned(),
            operations: backend
                .capabilities()
                .iter()
                .take(MAX_CODEC_OPERATIONS)
                .map(codec_operation)
                .collect(),
            codec_capability_count: saturating_u32(backend.codec_capabilities().len()),
        })
        .collect::<Vec<_>>();
    let operations = snapshot
        .operations()
        .iter()
        .take(MAX_CODEC_OPERATIONS)
        .map(|operation| CodecOperationCapability {
            operation: codec_operation(operation.operation()),
            primary_backends: operation
                .primary_backends()
                .iter()
                .take(MAX_CODEC_BACKENDS)
                .map(|value| bounded_text(value, MAX_NAME_BYTES, false))
                .collect(),
            fallback_backends: operation
                .fallback_backends()
                .iter()
                .take(MAX_CODEC_BACKENDS)
                .map(|value| bounded_text(value, MAX_NAME_BYTES, false))
                .collect(),
        })
        .collect::<Vec<_>>();
    let truncated = snapshot.backends().len() > backends.len()
        || snapshot.operations().len() > operations.len();
    let data = CodecCapabilities {
        schema_version: snapshot.schema_version().to_string(),
        revision: snapshot.revision(),
        backends,
        operations,
    };
    if data.operations.is_empty() {
        CapabilityDomain::live(
            CapabilityCondition::Unavailable,
            Some(data),
            Some(CapabilityFailure::new(
                "codec_operation_unavailable",
                "No codec operation is declared",
                "Repair the Superi installation, then refresh capabilities.",
            )),
        )
    } else if truncated {
        CapabilityDomain::live(
            CapabilityCondition::Degraded,
            Some(data),
            Some(CapabilityFailure::new(
                "codec_catalog_truncated",
                "Some codec declarations were omitted",
                "Use the listed codecs or reduce installed codec extensions.",
            )),
        )
    } else {
        CapabilityDomain::live(CapabilityCondition::Available, Some(data), None)
    }
}

fn discover_ai_capabilities() -> CapabilityDomain<AiCapabilities> {
    let snapshot = discover_local_capabilities();
    let runtime = match snapshot.runtime() {
        AiRuntimeAvailability::Unavailable => "unavailable",
        _ => "unavailable",
    };
    let data = AiCapabilities {
        schema_version: snapshot.schema_version(),
        runtime: runtime.to_owned(),
        local_only: snapshot.local_only(),
        requires_editable_artifacts: snapshot.requires_editable_artifacts(),
        available_pipelines: snapshot
            .available_pipelines()
            .iter()
            .take(MAX_PIPELINES)
            .map(|pipeline| bounded_text(pipeline.id(), MAX_NAME_BYTES, false))
            .collect(),
    };
    CapabilityDomain::live(
        CapabilityCondition::Unavailable,
        Some(data),
        Some(CapabilityFailure::new(
            "ai_runtime_unavailable",
            "Local AI runtime is not installed",
            "Continue without AI tools.",
        )),
    )
}

fn project_output_device(device: &OutputDevice) -> AudioDeviceCapability {
    AudioDeviceCapability {
        id: bounded_text(device.id.as_str(), MAX_IDENTITY_BYTES, false),
        name: bounded_text(&device.name, MAX_NAME_BYTES, false),
        is_default: device.is_default,
        default_config: device.default_config.map(project_output_config),
        capabilities: device
            .capabilities
            .iter()
            .take(MAX_AUDIO_CAPABILITIES)
            .map(project_output_capability)
            .collect(),
        channel_layout_known: device.channel_layout_known,
    }
}

fn project_input_device(device: &InputDevice) -> AudioDeviceCapability {
    AudioDeviceCapability {
        id: bounded_text(device.id.as_str(), MAX_IDENTITY_BYTES, false),
        name: bounded_text(&device.name, MAX_NAME_BYTES, false),
        is_default: device.is_default,
        default_config: device.default_config.map(project_input_config),
        capabilities: device
            .capabilities
            .iter()
            .take(MAX_AUDIO_CAPABILITIES)
            .map(project_input_capability)
            .collect(),
        channel_layout_known: device.channel_layout_known,
    }
}

fn project_output_config(config: OutputStreamConfig) -> AudioStreamCapability {
    AudioStreamCapability {
        channels: config.channels,
        sample_rate: config.sample_rate,
        sample_format: output_sample_format(config.sample_format).to_owned(),
        buffer_frames: config.buffer_frames,
    }
}

fn project_input_config(config: InputStreamConfig) -> AudioStreamCapability {
    AudioStreamCapability {
        channels: config.channels,
        sample_rate: config.sample_rate,
        sample_format: input_sample_format(config.sample_format).to_owned(),
        buffer_frames: config.buffer_frames,
    }
}

fn project_output_capability(capability: &OutputCapability) -> AudioCapabilityRange {
    AudioCapabilityRange {
        channels: capability.channels,
        min_sample_rate: capability.min_sample_rate,
        max_sample_rate: capability.max_sample_rate,
        sample_format: output_sample_format(capability.sample_format).to_owned(),
        buffer_size: match capability.buffer_size {
            OutputBufferSize::Unknown => AudioBufferSize::Unknown,
            OutputBufferSize::Range { min, max } => AudioBufferSize::Range { min, max },
            _ => AudioBufferSize::Unknown,
        },
    }
}

fn project_input_capability(capability: &InputCapability) -> AudioCapabilityRange {
    AudioCapabilityRange {
        channels: capability.channels,
        min_sample_rate: capability.min_sample_rate,
        max_sample_rate: capability.max_sample_rate,
        sample_format: input_sample_format(capability.sample_format).to_owned(),
        buffer_size: match capability.buffer_size {
            InputBufferSize::Unknown => AudioBufferSize::Unknown,
            InputBufferSize::Range { min, max } => AudioBufferSize::Range { min, max },
            _ => AudioBufferSize::Unknown,
        },
    }
}

const fn output_sample_format(format: OutputSampleFormat) -> &'static str {
    match format {
        OutputSampleFormat::I8 => "i8",
        OutputSampleFormat::I16 => "i16",
        OutputSampleFormat::I24 => "i24",
        OutputSampleFormat::I32 => "i32",
        OutputSampleFormat::I64 => "i64",
        OutputSampleFormat::U8 => "u8",
        OutputSampleFormat::U16 => "u16",
        OutputSampleFormat::U24 => "u24",
        OutputSampleFormat::U32 => "u32",
        OutputSampleFormat::U64 => "u64",
        OutputSampleFormat::F32 => "f32",
        OutputSampleFormat::F64 => "f64",
        OutputSampleFormat::Other => "other",
        _ => "other",
    }
}

const fn input_sample_format(format: InputSampleFormat) -> &'static str {
    match format {
        InputSampleFormat::I8 => "i8",
        InputSampleFormat::I16 => "i16",
        InputSampleFormat::I24 => "i24",
        InputSampleFormat::I32 => "i32",
        InputSampleFormat::I64 => "i64",
        InputSampleFormat::U8 => "u8",
        InputSampleFormat::U16 => "u16",
        InputSampleFormat::U24 => "u24",
        InputSampleFormat::U32 => "u32",
        InputSampleFormat::U64 => "u64",
        InputSampleFormat::F32 => "f32",
        InputSampleFormat::F64 => "f64",
        InputSampleFormat::Other => "other",
        _ => "other",
    }
}

fn merge_domain<T: Clone>(
    live: CapabilityDomain<T>,
    previous: Option<&CapabilityDomain<T>>,
) -> CapabilityDomain<T> {
    if live.data.is_some() {
        return live;
    }
    let Some(previous_data) = previous.and_then(|domain| domain.data.clone()) else {
        return live;
    };
    CapabilityDomain {
        condition: CapabilityCondition::Degraded,
        freshness: CapabilityFreshness::Retained,
        data: Some(previous_data),
        failure: live.failure,
    }
}

fn mark_snapshot_retained(mut snapshot: DesktopCapabilitySnapshot) -> DesktopCapabilitySnapshot {
    snapshot.cache_status = CapabilityCacheStatus::Retained;
    snapshot.persistence_failure = None;
    mark_domain_retained(&mut snapshot.gpu);
    mark_domain_retained(&mut snapshot.audio);
    mark_domain_retained(&mut snapshot.codecs);
    mark_domain_retained(&mut snapshot.ai);
    snapshot
}

fn mark_domain_retained<T>(domain: &mut CapabilityDomain<T>) {
    if domain.data.is_some() {
        domain.freshness = CapabilityFreshness::Retained;
    }
}

fn read_cached_snapshot(
    path: &Path,
) -> Result<Option<DesktopCapabilitySnapshot>, CapabilityFailure> {
    if !path.exists() {
        return Ok(None);
    }
    let file = std::fs::File::open(path).map_err(|_| CapabilityFailure::persistence("read"))?;
    let mut bytes = Vec::new();
    file.take(MAX_CACHE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CapabilityFailure::persistence("read"))?;
    if bytes.len() as u64 > MAX_CACHE_BYTES {
        return Err(CapabilityFailure::persistence("capacity"));
    }
    let snapshot = serde_json::from_slice::<DesktopCapabilitySnapshot>(&bytes)
        .map_err(|_| CapabilityFailure::persistence("invalid"))?;
    validate_snapshot(&snapshot)?;
    Ok(Some(snapshot))
}

fn validate_snapshot(snapshot: &DesktopCapabilitySnapshot) -> Result<(), CapabilityFailure> {
    if snapshot.schema_version != CAPABILITY_SCHEMA_VERSION
        || snapshot.revision == 0
        || snapshot.revision > MAX_SAFE_JAVASCRIPT_INTEGER
        || snapshot.observed_at_unix_ms > MAX_SAFE_JAVASCRIPT_INTEGER
    {
        return Err(CapabilityFailure::persistence("invalid"));
    }
    let retained = [
        snapshot.gpu.freshness,
        snapshot.audio.freshness,
        snapshot.codecs.freshness,
        snapshot.ai.freshness,
    ]
    .contains(&CapabilityFreshness::Retained);
    match snapshot.cache_status {
        CapabilityCacheStatus::Current if retained || snapshot.persistence_failure.is_some() => {
            return Err(CapabilityFailure::persistence("invalid"));
        }
        CapabilityCacheStatus::Retained if !retained || snapshot.persistence_failure.is_some() => {
            return Err(CapabilityFailure::persistence("invalid"));
        }
        CapabilityCacheStatus::Unavailable if snapshot.persistence_failure.is_none() => {
            return Err(CapabilityFailure::persistence("invalid"));
        }
        _ => {}
    }
    validate_failure(snapshot.persistence_failure.as_ref())?;
    validate_gpu_domain(&snapshot.gpu)?;
    validate_audio_domain(&snapshot.audio)?;
    validate_codec_domain(&snapshot.codecs)?;
    validate_ai_domain(&snapshot.ai)
}

fn validate_gpu_domain(
    domain: &CapabilityDomain<GpuCapabilities>,
) -> Result<(), CapabilityFailure> {
    validate_domain_state(domain)?;
    validate_failure(domain.failure.as_ref())?;
    let Some(data) = domain.data.as_ref() else {
        return Ok(());
    };
    if data.adapters.len() > MAX_GPU_ADAPTERS {
        return Err(CapabilityFailure::persistence("invalid"));
    }
    for adapter in &data.adapters {
        validate_text(&adapter.id, MAX_NAME_BYTES, false)?;
        validate_text(&adapter.name, MAX_NAME_BYTES, false)?;
        validate_text(&adapter.backend, 64, false)?;
        validate_text(&adapter.device_type, 64, false)?;
        validate_text(&adapter.driver, MAX_NAME_BYTES, true)?;
        validate_text(&adapter.driver_info, MAX_DETAIL_BYTES, true)?;
        if adapter.feature_bits.len() <= 2
            || !adapter.feature_bits.starts_with("0x")
            || !adapter.feature_bits[2..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || adapter.max_buffer_size > MAX_SAFE_JAVASCRIPT_INTEGER
        {
            return Err(CapabilityFailure::persistence("invalid"));
        }
    }
    Ok(())
}

fn validate_audio_domain(
    domain: &CapabilityDomain<AudioCapabilities>,
) -> Result<(), CapabilityFailure> {
    validate_domain_state(domain)?;
    validate_failure(domain.failure.as_ref())?;
    let Some(data) = domain.data.as_ref() else {
        return Ok(());
    };
    if data.outputs.len() > MAX_AUDIO_DEVICES || data.inputs.len() > MAX_AUDIO_DEVICES {
        return Err(CapabilityFailure::persistence("invalid"));
    }
    for device in data.outputs.iter().chain(&data.inputs) {
        validate_text(&device.id, MAX_IDENTITY_BYTES, false)?;
        validate_text(&device.name, MAX_NAME_BYTES, false)?;
        if device.capabilities.len() > MAX_AUDIO_CAPABILITIES {
            return Err(CapabilityFailure::persistence("invalid"));
        }
        if let Some(config) = &device.default_config {
            if config.channels == 0 || config.sample_rate == 0 {
                return Err(CapabilityFailure::persistence("invalid"));
            }
            validate_text(&config.sample_format, 32, false)?;
        }
        for capability in &device.capabilities {
            if capability.channels == 0
                || capability.min_sample_rate == 0
                || capability.max_sample_rate < capability.min_sample_rate
            {
                return Err(CapabilityFailure::persistence("invalid"));
            }
            validate_text(&capability.sample_format, 32, false)?;
            if let AudioBufferSize::Range { min, max } = capability.buffer_size {
                if min == 0 || max < min {
                    return Err(CapabilityFailure::persistence("invalid"));
                }
            }
        }
    }
    Ok(())
}

fn validate_codec_domain(
    domain: &CapabilityDomain<CodecCapabilities>,
) -> Result<(), CapabilityFailure> {
    validate_domain_state(domain)?;
    validate_failure(domain.failure.as_ref())?;
    let Some(data) = domain.data.as_ref() else {
        return Ok(());
    };
    if data.backends.len() > MAX_CODEC_BACKENDS || data.operations.len() > MAX_CODEC_OPERATIONS {
        return Err(CapabilityFailure::persistence("invalid"));
    }
    validate_semantic_version(&data.schema_version)?;
    if data.revision > MAX_SAFE_JAVASCRIPT_INTEGER {
        return Err(CapabilityFailure::persistence("invalid"));
    }
    for backend in &data.backends {
        validate_text(&backend.id, MAX_NAME_BYTES, false)?;
        validate_text(&backend.display_name, MAX_NAME_BYTES, false)?;
        validate_text(&backend.tier, 32, false)?;
        validate_text(&backend.hardware_acceleration, 64, false)?;
        if backend.operations.len() > MAX_CODEC_OPERATIONS {
            return Err(CapabilityFailure::persistence("invalid"));
        }
        for operation in &backend.operations {
            validate_text(operation, MAX_NAME_BYTES, false)?;
        }
    }
    for operation in &data.operations {
        validate_text(&operation.operation, MAX_NAME_BYTES, false)?;
        if operation.primary_backends.len() > MAX_CODEC_BACKENDS
            || operation.fallback_backends.len() > MAX_CODEC_BACKENDS
        {
            return Err(CapabilityFailure::persistence("invalid"));
        }
        for backend in operation
            .primary_backends
            .iter()
            .chain(&operation.fallback_backends)
        {
            validate_text(backend, MAX_NAME_BYTES, false)?;
        }
    }
    Ok(())
}

fn validate_ai_domain(domain: &CapabilityDomain<AiCapabilities>) -> Result<(), CapabilityFailure> {
    validate_domain_state(domain)?;
    validate_failure(domain.failure.as_ref())?;
    let Some(data) = domain.data.as_ref() else {
        return Ok(());
    };
    if data.schema_version != AI_CAPABILITY_SCHEMA_VERSION
        || data.runtime != "unavailable"
        || !data.local_only
        || !data.requires_editable_artifacts
        || !data.available_pipelines.is_empty()
    {
        return Err(CapabilityFailure::persistence("invalid"));
    }
    for pipeline in &data.available_pipelines {
        validate_text(pipeline, MAX_NAME_BYTES, false)?;
    }
    Ok(())
}

fn validate_domain_state<T>(domain: &CapabilityDomain<T>) -> Result<(), CapabilityFailure> {
    let valid = match domain.condition {
        CapabilityCondition::Available => domain.data.is_some() && domain.failure.is_none(),
        CapabilityCondition::Degraded => domain.data.is_some() && domain.failure.is_some(),
        CapabilityCondition::Unavailable => domain.failure.is_some(),
    } && (domain.freshness != CapabilityFreshness::Retained || domain.data.is_some());
    if !valid {
        return Err(CapabilityFailure::persistence("invalid"));
    }
    Ok(())
}

fn validate_failure(failure: Option<&CapabilityFailure>) -> Result<(), CapabilityFailure> {
    let Some(failure) = failure else {
        return Ok(());
    };
    validate_text(&failure.code, MAX_FAILURE_CODE_BYTES, false)?;
    validate_text(&failure.title, MAX_FAILURE_TITLE_BYTES, false)?;
    validate_text(&failure.action, MAX_FAILURE_ACTION_BYTES, false)
}

fn validate_text(value: &str, maximum: usize, allow_empty: bool) -> Result<(), CapabilityFailure> {
    if (!allow_empty && value.is_empty())
        || value.len() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(CapabilityFailure::persistence("invalid"));
    }
    Ok(())
}

fn validate_semantic_version(value: &str) -> Result<(), CapabilityFailure> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(CapabilityFailure::persistence("invalid"));
    }
    Ok(())
}

fn bounded_text(value: &str, maximum: usize, allow_empty: bool) -> String {
    let mut result = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    while result.len() > maximum {
        result.pop();
    }
    if !allow_empty && result.trim().is_empty() {
        return "Unavailable".to_owned();
    }
    result
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
        .min(MAX_SAFE_JAVASCRIPT_INTEGER)
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

const fn gpu_backend(backend: Backend) -> &'static str {
    match backend {
        Backend::Empty => "empty",
        Backend::Vulkan => "vulkan",
        Backend::Metal => "metal",
        Backend::Dx12 => "dx12",
        Backend::Gl => "gl",
        Backend::BrowserWebGpu => "browser_webgpu",
    }
}

const fn gpu_device_type(device_type: DeviceType) -> &'static str {
    match device_type {
        DeviceType::Other => "other",
        DeviceType::IntegratedGpu => "integrated_gpu",
        DeviceType::DiscreteGpu => "discrete_gpu",
        DeviceType::VirtualGpu => "virtual_gpu",
        DeviceType::Cpu => "cpu",
    }
}

const fn codec_tier(tier: MediaBackendTier) -> &'static str {
    match tier {
        MediaBackendTier::Primary => "primary",
        MediaBackendTier::Fallback => "fallback",
        _ => "unknown",
    }
}

const fn codec_acceleration(acceleration: HardwareAcceleration) -> &'static str {
    match acceleration {
        HardwareAcceleration::Unreported => "unreported",
        HardwareAcceleration::Software => "software",
        HardwareAcceleration::Hardware => "hardware",
        HardwareAcceleration::PlatformManaged => "platform_managed",
        _ => "unknown",
    }
}

fn codec_operation(operation: &MediaOperation) -> String {
    let value = match operation {
        MediaOperation::Source => "source".to_owned(),
        MediaOperation::Decode { codec } => format!("decode:{codec}"),
        MediaOperation::Encode { codec } => format!("encode:{codec}"),
        _ => "unknown".to_owned(),
    };
    bounded_text(&value, MAX_NAME_BYTES, false)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn temporary_root(label: &str) -> PathBuf {
        let ordinal = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "superi-capabilities-{label}-{}-{ordinal}",
            std::process::id()
        ))
    }

    fn fixture_live() -> LiveCapabilities {
        LiveCapabilities {
            gpu: CapabilityDomain::live(
                CapabilityCondition::Available,
                Some(GpuCapabilities {
                    adapters: vec![GpuAdapterCapability {
                        id: "metal:0000106b:00000000:0".to_owned(),
                        name: "Fixture GPU".to_owned(),
                        backend: "metal".to_owned(),
                        device_type: "integrated_gpu".to_owned(),
                        vendor_id: 4_203,
                        device_id: 0,
                        driver: "Metal".to_owned(),
                        driver_info: "fixture".to_owned(),
                        feature_bits: "0x40".to_owned(),
                        webgpu_compliant: true,
                        max_texture_dimension_2d: 16_384,
                        max_bind_groups: 8,
                        max_buffer_size: 1_073_741_824,
                    }],
                    skipped_adapters: 0,
                }),
                None,
            ),
            audio: CapabilityDomain::live(
                CapabilityCondition::Available,
                Some(AudioCapabilities {
                    outputs: Vec::new(),
                    inputs: Vec::new(),
                    skipped_output_devices: 0,
                    skipped_input_devices: 0,
                }),
                None,
            ),
            codecs: CapabilityDomain::live(
                CapabilityCondition::Available,
                Some(CodecCapabilities {
                    schema_version: "2.0.0".to_owned(),
                    revision: 0,
                    backends: Vec::new(),
                    operations: vec![CodecOperationCapability {
                        operation: "source".to_owned(),
                        primary_backends: vec!["fixture".to_owned()],
                        fallback_backends: Vec::new(),
                    }],
                }),
                None,
            ),
            ai: discover_ai_capabilities(),
        }
    }

    #[test]
    fn failed_live_domain_retains_visible_data_without_claiming_freshness() {
        let root = temporary_root("retained");
        let state = DesktopCapabilityState::default();
        state.initialize(&root);
        let first = state.commit_observation(fixture_live(), 10).unwrap();
        assert_eq!(first.revision(), 1);

        let mut failed = fixture_live();
        failed.gpu = CapabilityDomain::live(
            CapabilityCondition::Unavailable,
            None,
            Some(CapabilityFailure::new(
                "gpu_backend_unavailable",
                "GPU discovery is unavailable",
                "Refresh capabilities.",
            )),
        );
        let second = state.commit_observation(failed, 11).unwrap();
        assert_eq!(second.revision(), 2);
        assert_eq!(second.gpu.condition, CapabilityCondition::Degraded);
        assert_eq!(second.gpu.freshness, CapabilityFreshness::Retained);
        assert_eq!(second.gpu.data.as_ref().unwrap().adapters.len(), 1);
        assert_eq!(second.cache_status, CapabilityCacheStatus::Retained);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn valid_cache_restores_and_corrupt_cache_is_replaced_by_fresh_state() {
        let root = temporary_root("cache");
        let first = DesktopCapabilityState::default();
        first.initialize(&root);
        first.commit_observation(fixture_live(), 20).unwrap();
        drop(first);

        let restored = DesktopCapabilityState::default();
        restored.initialize(&root);
        let model = restored.model.lock().unwrap();
        let retained = model.last.as_ref().unwrap();
        assert_eq!(retained.revision(), 1);
        assert_eq!(retained.gpu.freshness, CapabilityFreshness::Retained);
        drop(model);

        std::fs::write(root.join(CACHE_FILE_NAME), b"not json").unwrap();
        let recovered = DesktopCapabilityState::default();
        recovered.initialize(&root);
        assert!(recovered.model.lock().unwrap().last.is_none());
        recovered.commit_observation(fixture_live(), 21).unwrap();
        assert!(read_cached_snapshot(&root.join(CACHE_FILE_NAME))
            .unwrap()
            .is_some());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn output_projection_keeps_exact_clock_range_and_unknown_channel_meaning() {
        let device = OutputDevice {
            id: "fixture:output".parse().unwrap(),
            name: "Fixture Output".to_owned(),
            is_default: true,
            default_config: Some(OutputStreamConfig {
                channels: 2,
                sample_rate: 48_000,
                sample_format: OutputSampleFormat::F32,
                buffer_frames: None,
            }),
            capabilities: vec![OutputCapability {
                channels: 2,
                min_sample_rate: 44_100,
                max_sample_rate: 96_000,
                sample_format: OutputSampleFormat::F32,
                buffer_size: OutputBufferSize::Range {
                    min: 64,
                    max: 1_024,
                },
            }],
            channel_layout_known: false,
        };

        let projected = project_output_device(&device);
        assert!(!projected.channel_layout_known);
        assert_eq!(projected.default_config.unwrap().sample_rate, 48_000);
        assert_eq!(projected.capabilities[0].min_sample_rate, 44_100);
        assert_eq!(projected.capabilities[0].max_sample_rate, 96_000);
        assert_eq!(
            projected.capabilities[0].buffer_size,
            AudioBufferSize::Range {
                min: 64,
                max: 1_024
            }
        );
    }

    #[test]
    fn cache_validation_rejects_semantic_state_and_unsafe_nested_revisions() {
        let root = temporary_root("semantic-validation");
        let state = DesktopCapabilityState::default();
        state.initialize(&root);
        let snapshot = state.commit_observation(fixture_live(), 30).unwrap();

        let mut missing_data = snapshot.clone();
        missing_data.gpu.data = None;
        assert!(validate_snapshot(&missing_data).is_err());

        let mut false_ai_boundary = snapshot.clone();
        false_ai_boundary.ai.data.as_mut().unwrap().local_only = false;
        assert!(validate_snapshot(&false_ai_boundary).is_err());

        let mut unsafe_codec_revision = snapshot;
        unsafe_codec_revision.codecs.data.as_mut().unwrap().revision =
            MAX_SAFE_JAVASCRIPT_INTEGER + 1;
        assert!(validate_snapshot(&unsafe_codec_revision).is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn live_providers_return_one_strict_observation_on_the_current_host() {
        let root = temporary_root("live");
        let state = DesktopCapabilityState::default();
        state.initialize(&root);

        let snapshot = state.discover().unwrap();

        assert_eq!(snapshot.schema_version, CAPABILITY_SCHEMA_VERSION);
        assert_eq!(snapshot.revision(), 1);
        assert_eq!(snapshot.ai.data.as_ref().unwrap().runtime, "unavailable");
        validate_snapshot(&snapshot).unwrap();

        std::fs::remove_dir_all(root).unwrap();
    }
}
