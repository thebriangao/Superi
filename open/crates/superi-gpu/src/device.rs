//! Native wgpu adapter discovery, selection, capability reporting, and device creation.

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

pub use wgpu::{
    Backend, Backends, DeviceType, DownlevelCapabilities, Features, InstanceFlags, Limits,
    MemoryHints, PowerPreference,
};

const COMPONENT: &str = "superi-gpu.device";

/// Options used to create the native wgpu instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstanceOptions {
    backends: Backends,
    flags: InstanceFlags,
}

impl InstanceOptions {
    /// Creates options for an explicit set of wgpu backends.
    #[must_use]
    pub const fn new(backends: Backends) -> Self {
        Self {
            backends,
            flags: InstanceFlags::empty(),
        }
    }

    /// Enables explicit instance validation and debugging flags.
    #[must_use]
    pub const fn with_flags(mut self, flags: InstanceFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Returns the requested backend set.
    #[must_use]
    pub const fn backends(self) -> Backends {
        self.backends
    }

    /// Returns the requested wgpu instance flags.
    #[must_use]
    pub const fn flags(self) -> InstanceFlags {
        self.flags
    }
}

impl Default for InstanceOptions {
    fn default() -> Self {
        Self {
            backends: Backends::all(),
            flags: InstanceFlags::from_build_config(),
        }
    }
}

/// The process-local wgpu instance used to enumerate native adapters.
#[derive(Debug)]
pub struct GpuInstance {
    inner: wgpu::Instance,
    enabled_backends: Backends,
    identity: Arc<()>,
}

impl GpuInstance {
    /// Creates an instance after intersecting requested and compiled backends.
    pub fn new(options: InstanceOptions) -> Result<Self> {
        let compiled = wgpu::Instance::enabled_backend_features();
        let enabled_backends = options.backends & compiled;
        if enabled_backends.is_empty() {
            return Err(Error::new(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "no requested wgpu backend is compiled for this platform",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_instance")
                    .with_field(
                        "requested_backends",
                        format!("{:#x}", options.backends.bits()),
                    )
                    .with_field("compiled_backends", format!("{:#x}", compiled.bits())),
            ));
        }

        let inner = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: enabled_backends,
            flags: options.flags,
            ..Default::default()
        });
        Ok(Self {
            inner,
            enabled_backends,
            identity: Arc::new(()),
        })
    }

    /// Returns the backend set that this instance can actually enumerate.
    #[must_use]
    pub const fn enabled_backends(&self) -> Backends {
        self.enabled_backends
    }

    pub(crate) const fn wgpu_instance(&self) -> &wgpu::Instance {
        &self.inner
    }

    pub(crate) const fn identity(&self) -> &Arc<()> {
        &self.identity
    }

    /// Enumerates every native adapter exposed by the enabled backends.
    ///
    /// The resulting catalog has deterministic ordering and owns each adapter
    /// until one is selected. An empty catalog is valid and selection then
    /// reports a recoverable unavailable error.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn enumerate_adapters(&self) -> AdapterCatalog {
        let mut adapters = self
            .inner
            .enumerate_adapters(self.enabled_backends)
            .into_iter()
            .map(AdapterCandidate::new)
            .collect::<Vec<_>>();
        adapters.sort_by(|left, right| adapter_sort_key(left).cmp(&adapter_sort_key(right)));

        let mut previous_hardware = None;
        let mut ordinal = 0_u32;
        for candidate in &mut adapters {
            let hardware = (
                candidate.snapshot.info.backend as u8,
                candidate.snapshot.info.vendor,
                candidate.snapshot.info.device,
            );
            if previous_hardware == Some(hardware) {
                ordinal = ordinal.saturating_add(1);
            } else {
                previous_hardware = Some(hardware);
                ordinal = 0;
            }
            candidate.snapshot.id = AdapterId {
                backend: candidate.snapshot.info.backend,
                vendor: candidate.snapshot.info.vendor,
                device: candidate.snapshot.info.device,
                ordinal,
            };
        }

        AdapterCatalog { adapters }
    }
}

/// Process-local identity for one deterministically ordered adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AdapterId {
    backend: Backend,
    vendor: u32,
    device: u32,
    ordinal: u32,
}

impl AdapterId {
    /// Returns the wgpu backend that exposed the adapter.
    #[must_use]
    pub const fn backend(self) -> Backend {
        self.backend
    }

    /// Returns the backend-specific vendor identifier.
    #[must_use]
    pub const fn vendor(self) -> u32 {
        self.vendor
    }

    /// Returns the backend-specific device identifier.
    #[must_use]
    pub const fn device(self) -> u32 {
        self.device
    }

    /// Distinguishes otherwise identical adapter records in this catalog.
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }

    fn sort_key(self) -> (u8, u32, u32, u32) {
        (self.backend as u8, self.vendor, self.device, self.ordinal)
    }
}

impl fmt::Display for AdapterId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{:08x}:{:08x}:{}",
            self.backend, self.vendor, self.device, self.ordinal
        )
    }
}

/// Immutable capability snapshot captured before device creation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterCapabilities {
    features: Features,
    limits: Limits,
    downlevel: DownlevelCapabilities,
}

impl AdapterCapabilities {
    fn from_adapter(adapter: &wgpu::Adapter) -> Self {
        Self {
            features: adapter.features(),
            limits: adapter.limits(),
            downlevel: adapter.get_downlevel_capabilities(),
        }
    }

    /// Returns every optional wgpu feature supported by the adapter.
    #[must_use]
    pub const fn features(&self) -> Features {
        self.features
    }

    /// Returns the best limits exposed by the adapter.
    #[must_use]
    pub const fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Returns the adapter's downlevel flags, limits, and shader model.
    #[must_use]
    pub const fn downlevel(&self) -> &DownlevelCapabilities {
        &self.downlevel
    }

    /// Returns whether the adapter supports the complete baseline WebGPU contract.
    #[must_use]
    pub fn is_webgpu_compliant(&self) -> bool {
        self.downlevel.is_webgpu_compliant()
    }
}

/// Immutable identity, driver information, and capabilities for one adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterSnapshot {
    id: AdapterId,
    info: wgpu::AdapterInfo,
    capabilities: AdapterCapabilities,
}

impl AdapterSnapshot {
    /// Returns the process-local deterministic adapter identity.
    #[must_use]
    pub const fn id(&self) -> AdapterId {
        self.id
    }

    /// Returns backend, device, vendor, driver, and device-type information.
    #[must_use]
    pub const fn info(&self) -> &wgpu::AdapterInfo {
        &self.info
    }

    /// Returns the capabilities captured from this adapter.
    #[must_use]
    pub const fn capabilities(&self) -> &AdapterCapabilities {
        &self.capabilities
    }
}

struct AdapterCandidate {
    adapter: wgpu::Adapter,
    snapshot: AdapterSnapshot,
}

impl AdapterCandidate {
    fn new(adapter: wgpu::Adapter) -> Self {
        let info = adapter.get_info();
        let capabilities = AdapterCapabilities::from_adapter(&adapter);
        Self {
            adapter,
            snapshot: AdapterSnapshot {
                id: AdapterId {
                    backend: info.backend,
                    vendor: info.vendor,
                    device: info.device,
                    ordinal: 0,
                },
                info,
                capabilities,
            },
        }
    }
}

fn adapter_sort_key(candidate: &AdapterCandidate) -> (u8, u32, u32, u8, &str, &str, &str) {
    let info = &candidate.snapshot.info;
    (
        info.backend as u8,
        info.vendor,
        info.device,
        device_type_code(info.device_type),
        info.name.as_str(),
        info.driver.as_str(),
        info.driver_info.as_str(),
    )
}

/// An owned set of native adapters and their immutable capability snapshots.
pub struct AdapterCatalog {
    adapters: Vec<AdapterCandidate>,
}

impl fmt::Debug for AdapterCatalog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdapterCatalog")
            .field("snapshots", &self.snapshots().collect::<Vec<_>>())
            .finish()
    }
}

impl AdapterCatalog {
    /// Returns the number of adapters in the catalog.
    #[must_use]
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// Returns whether no enabled backend exposed an adapter.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    /// Iterates immutable adapter snapshots in deterministic order.
    pub fn snapshots(&self) -> impl ExactSizeIterator<Item = &AdapterSnapshot> {
        self.adapters.iter().map(|candidate| &candidate.snapshot)
    }

    pub(crate) fn retain_surface_compatible(mut self, surface: &wgpu::Surface<'_>) -> Self {
        self.adapters.retain(|candidate| {
            let capabilities = surface.get_capabilities(&candidate.adapter);
            !capabilities.formats.is_empty()
                && capabilities
                    .present_modes
                    .contains(&wgpu::PresentMode::Fifo)
                && !capabilities.alpha_modes.is_empty()
                && capabilities
                    .usages
                    .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        });
        self
    }

    /// Selects and consumes one compatible adapter.
    ///
    /// An explicit adapter identity is exact and never falls back. Automatic
    /// selection filters capabilities first, then applies the requested power
    /// preference with the adapter identity as a deterministic tie breaker.
    pub fn select(mut self, selection: &AdapterSelection) -> Result<SelectedAdapter> {
        let index = select_adapter_index(&self.adapters, selection)?;
        let selected = self.adapters.remove(index);
        Ok(SelectedAdapter {
            adapter: selected.adapter,
            snapshot: selected.snapshot,
        })
    }

    /// Selects a primary adapter and distinct ordered additional adapters.
    ///
    /// Required slots fail when no remaining adapter satisfies their policy.
    /// Optional slots are omitted instead, which lets the same configuration
    /// degrade to one GPU on systems without additional compatible adapters.
    /// Every successful slot consumes one adapter record, so a device set never
    /// aliases one adapter handle or silently substitutes an exact preference.
    pub fn select_many(mut self, selection: &MultiAdapterSelection) -> Result<SelectedAdapters> {
        let mut selected = Vec::with_capacity(selection.slots.len().min(self.adapters.len()));
        for (slot_index, slot) in selection.slots.iter().enumerate() {
            let index = match select_adapter_index(&self.adapters, &slot.selection) {
                Ok(index) => index,
                Err(_) if !slot.required => continue,
                Err(error) => {
                    return Err(error.with_context(
                        ErrorContext::new(COMPONENT, "select_adapter_set")
                            .with_field("slot_index", slot_index.to_string())
                            .with_field("required_slots", selection.required_len().to_string())
                            .with_field("selected_adapters", selected.len().to_string()),
                    ));
                }
            };
            let candidate = self.adapters.remove(index);
            selected.push(SelectedAdapter {
                adapter: candidate.adapter,
                snapshot: candidate.snapshot,
            });
        }

        debug_assert!(!selected.is_empty(), "the primary adapter slot is required");
        Ok(SelectedAdapters { adapters: selected })
    }
}

/// Explicit requirements and ranking policy for adapter selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterSelection {
    preferred_adapter: Option<AdapterId>,
    power_preference: PowerPreference,
    required_features: Features,
    required_limits: Limits,
    allow_software_adapter: bool,
    require_webgpu_compliance: bool,
}

impl AdapterSelection {
    /// Requires one exact adapter identity with no implicit fallback.
    #[must_use]
    pub const fn with_preferred_adapter(mut self, adapter: AdapterId) -> Self {
        self.preferred_adapter = Some(adapter);
        self
    }

    /// Sets the automatic-selection power preference.
    #[must_use]
    pub const fn with_power_preference(mut self, preference: PowerPreference) -> Self {
        self.power_preference = preference;
        self
    }

    /// Requires every specified optional wgpu feature.
    #[must_use]
    pub const fn with_required_features(mut self, features: Features) -> Self {
        self.required_features = features;
        self
    }

    /// Requires the provided wgpu limits.
    #[must_use]
    pub fn with_required_limits(mut self, limits: Limits) -> Self {
        self.required_limits = limits;
        self
    }

    /// Controls whether CPU and software adapters may be selected.
    #[must_use]
    pub const fn allow_software_adapter(mut self, allow: bool) -> Self {
        self.allow_software_adapter = allow;
        self
    }

    /// Controls whether downlevel adapters are rejected.
    #[must_use]
    pub const fn require_webgpu_compliance(mut self, require: bool) -> Self {
        self.require_webgpu_compliance = require;
        self
    }

    /// Returns the exact preferred adapter, when configured.
    #[must_use]
    pub const fn preferred_adapter(&self) -> Option<AdapterId> {
        self.preferred_adapter
    }

    /// Returns the automatic-selection power preference.
    #[must_use]
    pub const fn power_preference(&self) -> PowerPreference {
        self.power_preference
    }

    /// Returns required optional features.
    #[must_use]
    pub const fn required_features(&self) -> Features {
        self.required_features
    }

    /// Returns required limits.
    #[must_use]
    pub const fn required_limits(&self) -> &Limits {
        &self.required_limits
    }

    /// Returns whether CPU and software adapters may be selected.
    #[must_use]
    pub const fn software_adapter_allowed(&self) -> bool {
        self.allow_software_adapter
    }

    /// Returns whether complete baseline WebGPU support is required.
    #[must_use]
    pub const fn webgpu_compliance_required(&self) -> bool {
        self.require_webgpu_compliance
    }
}

impl Default for AdapterSelection {
    fn default() -> Self {
        Self {
            preferred_adapter: None,
            power_preference: PowerPreference::HighPerformance,
            required_features: Features::empty(),
            required_limits: Limits::default(),
            allow_software_adapter: false,
            require_webgpu_compliance: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AdapterSlotSelection {
    selection: AdapterSelection,
    required: bool,
}

/// Ordered policies for selecting one primary adapter and additional GPUs.
///
/// The primary slot is always required. Additional slots can be required for
/// workflows that need a specific device count or optional for portable
/// acceleration that degrades cleanly to the adapters the host exposes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiAdapterSelection {
    slots: Vec<AdapterSlotSelection>,
}

impl MultiAdapterSelection {
    /// Creates a selection with one required primary adapter policy.
    #[must_use]
    pub fn new(primary: AdapterSelection) -> Self {
        Self {
            slots: vec![AdapterSlotSelection {
                selection: primary,
                required: true,
            }],
        }
    }

    /// Adds one required distinct adapter slot.
    #[must_use]
    pub fn with_required_adapter(mut self, selection: AdapterSelection) -> Self {
        self.slots.push(AdapterSlotSelection {
            selection,
            required: true,
        });
        self
    }

    /// Adds one optional distinct adapter slot.
    #[must_use]
    pub fn with_optional_adapter(mut self, selection: AdapterSelection) -> Self {
        self.slots.push(AdapterSlotSelection {
            selection,
            required: false,
        });
        self
    }

    /// Returns the total configured primary and additional slots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Returns whether the selection has no slots.
    ///
    /// This is always false because construction requires a primary policy.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Returns the number of slots that must be satisfied.
    #[must_use]
    pub fn required_len(&self) -> usize {
        self.slots.iter().filter(|slot| slot.required).count()
    }

    /// Returns the primary adapter policy.
    #[must_use]
    pub fn primary(&self) -> &AdapterSelection {
        &self.slots[0].selection
    }
}

/// One selected physical adapter that may be consumed to create a device.
pub struct SelectedAdapter {
    adapter: wgpu::Adapter,
    snapshot: AdapterSnapshot,
}

impl fmt::Debug for SelectedAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectedAdapter")
            .field("snapshot", &self.snapshot)
            .finish_non_exhaustive()
    }
}

impl SelectedAdapter {
    /// Returns the immutable selected-adapter snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &AdapterSnapshot {
        &self.snapshot
    }

    /// Creates one logical device and its single owned submission queue.
    ///
    /// Requirements are checked before entering wgpu so unsupported features
    /// and limits become classified Superi errors rather than wgpu panics.
    pub async fn create_device(self, request: &DeviceRequest) -> Result<GpuDevice> {
        let problems = device_request_problems(&self.snapshot, request);
        if !problems.is_empty() {
            return Err(incompatible_adapter_error(
                "the selected GPU adapter cannot create the requested device",
                &self.snapshot,
                "create_device",
                problems,
            ));
        }

        let descriptor = wgpu::DeviceDescriptor {
            label: request.label.as_deref(),
            required_features: request.required_features,
            required_limits: request.required_limits.clone(),
            memory_hints: request.memory_hints.clone(),
        };
        let (device, queue) = self
            .adapter
            .request_device(&descriptor, None)
            .await
            .map_err(|source| {
                Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "wgpu could not create the requested device",
                    source,
                )
                .with_context(
                    adapter_context(&self.snapshot, "create_device")
                        .with_field("required_features", feature_bits(request.required_features)),
                )
            })?;

        Ok(GpuDevice {
            raw_adapter: self.adapter,
            adapter: self.snapshot,
            label: request.label.clone(),
            enabled_features: request.required_features,
            enabled_limits: request.required_limits.clone(),
            queue,
            device,
            identity: Arc::new(()),
            error_scope_gate: ErrorScopeGate::default(),
            submission_owner: AtomicBool::new(false),
        })
    }
}

/// Selected native adapters in primary-first device assignment order.
///
/// Each entry owns a distinct wgpu adapter handle. Creating devices preserves
/// this order and never introduces implicit resource transfer between GPUs.
pub struct SelectedAdapters {
    adapters: Vec<SelectedAdapter>,
}

impl fmt::Debug for SelectedAdapters {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectedAdapters")
            .field("snapshots", &self.snapshots().collect::<Vec<_>>())
            .finish()
    }
}

impl SelectedAdapters {
    /// Returns the number of selected adapters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    /// Returns whether no adapter is selected.
    ///
    /// This is always false because the primary slot is required.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    /// Returns the primary adapter snapshot.
    #[must_use]
    pub fn primary_snapshot(&self) -> &AdapterSnapshot {
        &self.adapters[0].snapshot
    }

    /// Iterates immutable snapshots in device assignment order.
    pub fn snapshots(&self) -> impl ExactSizeIterator<Item = &AdapterSnapshot> {
        self.adapters.iter().map(|selected| &selected.snapshot)
    }

    /// Finds one selected adapter snapshot by its process-local identity.
    #[must_use]
    pub fn adapter(&self, id: AdapterId) -> Option<&AdapterSnapshot> {
        self.adapters
            .iter()
            .find(|selected| selected.snapshot.id == id)
            .map(|selected| &selected.snapshot)
    }

    /// Creates one independent logical device and queue per selected adapter.
    ///
    /// The set is returned only after every selected adapter succeeds. If one
    /// device request fails, already-created devices are dropped and the error
    /// identifies the failed primary-first slot.
    pub async fn create_devices(self, request: &DeviceRequest) -> Result<GpuDeviceSet> {
        let mut devices = Vec::with_capacity(self.adapters.len());
        for (index, selected) in self.adapters.into_iter().enumerate() {
            let adapter = selected.snapshot.id;
            let device = selected.create_device(request).await.map_err(|error| {
                error.with_context(
                    ErrorContext::new(COMPONENT, "create_device_set")
                        .with_field("adapter_index", index.to_string())
                        .with_field("adapter", adapter.to_string()),
                )
            })?;
            devices.push(device);
        }
        Ok(GpuDeviceSet { devices })
    }
}

/// Explicit logical-device creation requirements.
#[derive(Clone, Debug)]
pub struct DeviceRequest {
    label: Option<String>,
    required_features: Features,
    required_limits: Limits,
    memory_hints: MemoryHints,
}

impl DeviceRequest {
    /// Adds an owned diagnostic label to the created device.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Requires every specified optional wgpu feature.
    #[must_use]
    pub const fn with_required_features(mut self, features: Features) -> Self {
        self.required_features = features;
        self
    }

    /// Requires the provided wgpu limits.
    #[must_use]
    pub fn with_required_limits(mut self, limits: Limits) -> Self {
        self.required_limits = limits;
        self
    }

    /// Sets the wgpu memory-allocation hint.
    #[must_use]
    pub fn with_memory_hints(mut self, hints: MemoryHints) -> Self {
        self.memory_hints = hints;
        self
    }

    /// Returns the optional diagnostic label.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns required optional features.
    #[must_use]
    pub const fn required_features(&self) -> Features {
        self.required_features
    }

    /// Returns required limits.
    #[must_use]
    pub const fn required_limits(&self) -> &Limits {
        &self.required_limits
    }

    /// Returns the wgpu memory-allocation hint.
    #[must_use]
    pub const fn memory_hints(&self) -> &MemoryHints {
        &self.memory_hints
    }
}

impl Default for DeviceRequest {
    fn default() -> Self {
        Self {
            label: None,
            required_features: Features::empty(),
            required_limits: Limits::default(),
            memory_hints: MemoryHints::Performance,
        }
    }
}

/// One logical wgpu device with exclusive ownership of its submission queue.
///
/// The raw device is borrowable for resource creation. The queue remains
/// private so later submission orchestration can enforce ordering and thread
/// ownership without a competing public submission path.
pub struct GpuDevice {
    raw_adapter: wgpu::Adapter,
    adapter: AdapterSnapshot,
    label: Option<String>,
    enabled_features: Features,
    enabled_limits: Limits,
    queue: wgpu::Queue,
    device: wgpu::Device,
    identity: Arc<()>,
    error_scope_gate: ErrorScopeGate,
    submission_owner: AtomicBool,
}

impl fmt::Debug for GpuDevice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GpuDevice")
            .field("adapter", &self.adapter)
            .field("label", &self.label)
            .field("enabled_features", &self.enabled_features)
            .field("enabled_limits", &self.enabled_limits)
            .finish_non_exhaustive()
    }
}

impl GpuDevice {
    /// Returns the adapter that owns this device.
    #[must_use]
    pub const fn adapter(&self) -> &AdapterSnapshot {
        &self.adapter
    }

    /// Returns the optional diagnostic label.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns exactly the optional features enabled on this logical device.
    #[must_use]
    pub const fn enabled_features(&self) -> Features {
        self.enabled_features
    }

    /// Returns exactly the limits enabled on this logical device.
    #[must_use]
    pub const fn enabled_limits(&self) -> &Limits {
        &self.enabled_limits
    }

    /// Borrows the raw wgpu device for resource creation.
    ///
    /// Queue submission is intentionally not exposed through this boundary.
    #[must_use]
    pub const fn wgpu_device(&self) -> &wgpu::Device {
        &self.device
    }

    pub(crate) const fn wgpu_adapter(&self) -> &wgpu::Adapter {
        &self.raw_adapter
    }

    pub(crate) fn lock_error_scopes(&self) -> ErrorScopeLockFuture<'_> {
        self.error_scope_gate.lock()
    }

    pub(crate) const fn identity(&self) -> &Arc<()> {
        &self.identity
    }

    pub(crate) fn claim_submission_owner(&self) -> Result<()> {
        self.submission_owner
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| {
                Error::new(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "GPU device already has a submission owner",
                )
                .with_context(ErrorContext::new(
                    "superi-gpu.submission",
                    "claim_submission_owner",
                ))
            })
    }

    pub(crate) fn release_submission_owner(&self) {
        let claimed = self.submission_owner.swap(false, Ordering::AcqRel);
        debug_assert!(
            claimed,
            "submission ownership must be released exactly once"
        );
    }

    pub(crate) fn write_texture(
        &self,
        texture: wgpu::ImageCopyTexture<'_>,
        data: &[u8],
        data_layout: wgpu::ImageDataLayout,
        size: wgpu::Extent3d,
    ) {
        self.queue.write_texture(texture, data, data_layout, size);
    }

    #[cfg(test)]
    pub(crate) fn submit_viewport<I>(&self, command_buffers: I)
    where
        I: IntoIterator<Item = wgpu::CommandBuffer>,
    {
        let _ = self.submit_commands(command_buffers);
    }

    pub(crate) fn submit_commands<I>(&self, command_buffers: I) -> wgpu::SubmissionIndex
    where
        I: IntoIterator<Item = wgpu::CommandBuffer>,
    {
        self.queue.submit(command_buffers)
    }

    pub(crate) fn on_submitted_work_done(&self, callback: impl FnOnce() + Send + 'static) {
        self.queue.on_submitted_work_done(callback);
    }

    pub(crate) fn poll_submissions(&self) -> wgpu::MaintainResult {
        self.device.poll(wgpu::Maintain::Poll)
    }
}

/// Independently owned logical GPU devices in primary-first assignment order.
///
/// Every entry retains its own adapter, device, private queue, identity, error
/// scopes, and submission ownership. Managed resources remain valid only on
/// their owning entry. Cross-adapter transfers require a future explicit
/// boundary and are never performed by this set.
pub struct GpuDeviceSet {
    devices: Vec<GpuDevice>,
}

impl fmt::Debug for GpuDeviceSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GpuDeviceSet")
            .field(
                "adapters",
                &self
                    .devices
                    .iter()
                    .map(GpuDevice::adapter)
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl GpuDeviceSet {
    /// Returns the number of logical devices.
    #[must_use]
    pub fn len(&self) -> usize {
        self.devices.len()
    }

    /// Returns whether no logical device exists.
    ///
    /// This is always false because the primary adapter is required.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Returns the primary device used for default processing and presentation.
    #[must_use]
    pub fn primary(&self) -> &GpuDevice {
        &self.devices[0]
    }

    /// Iterates additional devices in configured assignment order.
    pub fn additional(&self) -> impl ExactSizeIterator<Item = &GpuDevice> {
        self.devices[1..].iter()
    }

    /// Iterates every device in primary-first assignment order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &GpuDevice> {
        self.devices.iter()
    }

    /// Finds one device by its selected adapter identity.
    #[must_use]
    pub fn device(&self, adapter: AdapterId) -> Option<&GpuDevice> {
        self.devices
            .iter()
            .find(|device| device.adapter.id == adapter)
    }

    /// Consumes the owner and returns devices in primary-first order.
    #[must_use]
    pub fn into_devices(self) -> Vec<GpuDevice> {
        self.devices
    }
}

#[derive(Debug, Default)]
struct ErrorScopeGate {
    state: Mutex<ErrorScopeGateState>,
}

impl ErrorScopeGate {
    fn lock(&self) -> ErrorScopeLockFuture<'_> {
        ErrorScopeLockFuture {
            gate: self,
            waiter_id: None,
        }
    }

    fn state(&self) -> MutexGuard<'_, ErrorScopeGateState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Debug, Default)]
struct ErrorScopeGateState {
    locked: bool,
    next_waiter: u64,
    waiters: VecDeque<ErrorScopeWaiter>,
}

#[derive(Debug)]
struct ErrorScopeWaiter {
    id: u64,
    waker: Waker,
}

pub(crate) struct ErrorScopeLockFuture<'a> {
    gate: &'a ErrorScopeGate,
    waiter_id: Option<u64>,
}

impl<'a> Future for ErrorScopeLockFuture<'a> {
    type Output = ErrorScopeGuard<'a>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let mut state = this.gate.state();
        if let Some(waiter_id) = this.waiter_id {
            let is_front = state.waiters.front().map(|waiter| waiter.id) == Some(waiter_id);
            if !state.locked && is_front {
                state.waiters.pop_front();
                state.locked = true;
                this.waiter_id = None;
                return Poll::Ready(ErrorScopeGuard { gate: this.gate });
            }
            if let Some(waiter) = state
                .waiters
                .iter_mut()
                .find(|waiter| waiter.id == waiter_id)
            {
                if !waiter.waker.will_wake(context.waker()) {
                    waiter.waker = context.waker().clone();
                }
            }
            return Poll::Pending;
        }

        if !state.locked && state.waiters.is_empty() {
            state.locked = true;
            return Poll::Ready(ErrorScopeGuard { gate: this.gate });
        }

        let waiter_id = state.next_waiter;
        state.next_waiter = state.next_waiter.wrapping_add(1);
        state.waiters.push_back(ErrorScopeWaiter {
            id: waiter_id,
            waker: context.waker().clone(),
        });
        this.waiter_id = Some(waiter_id);
        Poll::Pending
    }
}

impl Drop for ErrorScopeLockFuture<'_> {
    fn drop(&mut self) {
        let Some(waiter_id) = self.waiter_id else {
            return;
        };
        let mut state = self.gate.state();
        let was_front = state.waiters.front().map(|waiter| waiter.id) == Some(waiter_id);
        if let Some(position) = state
            .waiters
            .iter()
            .position(|waiter| waiter.id == waiter_id)
        {
            state.waiters.remove(position);
        }
        let wake = (!state.locked && was_front)
            .then(|| state.waiters.front().map(|waiter| waiter.waker.clone()))
            .flatten();
        drop(state);
        if let Some(waker) = wake {
            waker.wake();
        }
    }
}

pub(crate) struct ErrorScopeGuard<'a> {
    gate: &'a ErrorScopeGate,
}

impl Drop for ErrorScopeGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.gate.state();
        state.locked = false;
        let wake = state.waiters.front().map(|waiter| waiter.waker.clone());
        drop(state);
        if let Some(waker) = wake {
            waker.wake();
        }
    }
}

fn selection_problems(snapshot: &AdapterSnapshot, selection: &AdapterSelection) -> Vec<String> {
    let mut problems = requirement_problems(
        snapshot,
        selection.required_features,
        &selection.required_limits,
    );
    if !selection.allow_software_adapter && snapshot.info.device_type == DeviceType::Cpu {
        problems.push("software adapter is not allowed".to_owned());
    }
    if selection.require_webgpu_compliance && !snapshot.capabilities.is_webgpu_compliant() {
        problems.push("adapter is not fully WebGPU compliant".to_owned());
    }
    problems
}

fn select_adapter_index(
    adapters: &[AdapterCandidate],
    selection: &AdapterSelection,
) -> Result<usize> {
    if let Some(preferred) = selection.preferred_adapter {
        let index = adapters
            .iter()
            .position(|candidate| candidate.snapshot.id == preferred)
            .ok_or_else(|| {
                Error::new(
                    ErrorCategory::NotFound,
                    Recoverability::UserCorrectable,
                    "the requested GPU adapter is not present in this catalog",
                )
                .with_context(
                    selection_context(selection)
                        .with_field("preferred_adapter", preferred.to_string()),
                )
            })?;
        let problems = selection_problems(&adapters[index].snapshot, selection);
        if !problems.is_empty() {
            return Err(incompatible_adapter_error(
                "the requested GPU adapter does not satisfy selection requirements",
                &adapters[index].snapshot,
                "select_adapter",
                problems,
            ));
        }
        return Ok(index);
    }

    if adapters.is_empty() {
        return Err(Error::new(
            ErrorCategory::Unavailable,
            Recoverability::UserCorrectable,
            "no native GPU adapter is available",
        )
        .with_context(selection_context(selection)));
    }

    adapters
        .iter()
        .enumerate()
        .filter(|(_, candidate)| selection_problems(&candidate.snapshot, selection).is_empty())
        .min_by_key(|(_, candidate)| {
            automatic_rank(&candidate.snapshot, selection.power_preference)
        })
        .map(|(index, _)| index)
        .ok_or_else(|| {
            Error::new(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "no GPU adapter satisfies the selection requirements",
            )
            .with_context(selection_context(selection))
        })
}

fn device_request_problems(snapshot: &AdapterSnapshot, request: &DeviceRequest) -> Vec<String> {
    requirement_problems(
        snapshot,
        request.required_features,
        &request.required_limits,
    )
}

fn requirement_problems(
    snapshot: &AdapterSnapshot,
    required_features: Features,
    required_limits: &Limits,
) -> Vec<String> {
    let mut problems = Vec::new();
    let missing = required_features - snapshot.capabilities.features;
    if !missing.is_empty() {
        problems.push(format!("missing features {}", feature_bits(missing)));
    }
    required_limits.check_limits_with_fail_fn(
        &snapshot.capabilities.limits,
        false,
        |name, required, supported| {
            problems.push(format!(
                "limit {name} requires {required} but adapter exposes {supported}"
            ));
        },
    );
    problems
}

fn automatic_rank(
    snapshot: &AdapterSnapshot,
    preference: PowerPreference,
) -> (u8, u8, u32, u32, u32) {
    let device_rank = match preference {
        PowerPreference::HighPerformance => match snapshot.info.device_type {
            DeviceType::DiscreteGpu => 0,
            DeviceType::IntegratedGpu => 1,
            DeviceType::VirtualGpu => 2,
            DeviceType::Other => 3,
            DeviceType::Cpu => 4,
        },
        PowerPreference::LowPower => match snapshot.info.device_type {
            DeviceType::IntegratedGpu => 0,
            DeviceType::DiscreteGpu => 1,
            DeviceType::Other => 2,
            DeviceType::VirtualGpu => 3,
            DeviceType::Cpu => 4,
        },
        PowerPreference::None => match snapshot.info.device_type {
            DeviceType::Cpu => 1,
            DeviceType::Other
            | DeviceType::IntegratedGpu
            | DeviceType::DiscreteGpu
            | DeviceType::VirtualGpu => 0,
        },
    };
    let (backend, vendor, device, ordinal) = snapshot.id.sort_key();
    (device_rank, backend, vendor, device, ordinal)
}

const fn device_type_code(device_type: DeviceType) -> u8 {
    match device_type {
        DeviceType::Other => 0,
        DeviceType::IntegratedGpu => 1,
        DeviceType::DiscreteGpu => 2,
        DeviceType::VirtualGpu => 3,
        DeviceType::Cpu => 4,
    }
}

fn selection_context(selection: &AdapterSelection) -> ErrorContext {
    ErrorContext::new(COMPONENT, "select_adapter")
        .with_field(
            "power_preference",
            format!("{:?}", selection.power_preference),
        )
        .with_field(
            "required_features",
            feature_bits(selection.required_features),
        )
        .with_field(
            "allow_software_adapter",
            selection.allow_software_adapter.to_string(),
        )
        .with_field(
            "require_webgpu_compliance",
            selection.require_webgpu_compliance.to_string(),
        )
}

fn adapter_context(snapshot: &AdapterSnapshot, operation: &'static str) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
        .with_field("adapter", snapshot.id.to_string())
        .with_field("adapter_name", snapshot.info.name.clone())
        .with_field("backend", snapshot.info.backend.to_string())
}

fn incompatible_adapter_error(
    message: &'static str,
    snapshot: &AdapterSnapshot,
    operation: &'static str,
    problems: Vec<String>,
) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        adapter_context(snapshot, operation).with_field("requirements", problems.join("; ")),
    )
}

fn feature_bits(features: Features) -> String {
    format!("{:#018x}", features.bits())
}
