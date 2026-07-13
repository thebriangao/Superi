//! Device-lifetime ownership and diagnostics for managed GPU resources.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::device::GpuDevice;

const RESOURCE_KIND_COUNT: usize = 10;
static NEXT_SCOPE: AtomicU64 = AtomicU64::new(1);

/// One managed GPU resource domain.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum GpuResourceKind {
    /// A GPU buffer.
    Buffer,
    /// A GPU texture allocation.
    Texture,
    /// A view into a managed texture.
    TextureView,
    /// A texture sampler.
    Sampler,
    /// A bind-group layout.
    BindGroupLayout,
    /// A bind group and its retained resources.
    BindGroup,
    /// A validated WGSL shader module.
    ShaderModule,
    /// An explicit pipeline layout.
    PipelineLayout,
    /// A render pipeline.
    RenderPipeline,
    /// A compute pipeline.
    ComputePipeline,
}

impl GpuResourceKind {
    /// Every resource kind managed by this resource owner.
    pub const ALL: &'static [Self] = &[
        Self::Buffer,
        Self::Texture,
        Self::TextureView,
        Self::Sampler,
        Self::BindGroupLayout,
        Self::BindGroup,
        Self::ShaderModule,
        Self::PipelineLayout,
        Self::RenderPipeline,
        Self::ComputePipeline,
    ];

    /// Returns the stable diagnostic code for this resource kind.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Buffer => "buffer",
            Self::Texture => "texture",
            Self::TextureView => "texture_view",
            Self::Sampler => "sampler",
            Self::BindGroupLayout => "bind_group_layout",
            Self::BindGroup => "bind_group",
            Self::ShaderModule => "shader_module",
            Self::PipelineLayout => "pipeline_layout",
            Self::RenderPipeline => "render_pipeline",
            Self::ComputePipeline => "compute_pipeline",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Buffer => 0,
            Self::Texture => 1,
            Self::TextureView => 2,
            Self::Sampler => 3,
            Self::BindGroupLayout => 4,
            Self::BindGroup => 5,
            Self::ShaderModule => 6,
            Self::PipelineLayout => 7,
            Self::RenderPipeline => 8,
            Self::ComputePipeline => 9,
        }
    }
}

impl fmt::Display for GpuResourceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// A process-local identifier for one resource in one device lifetime.
///
/// The identifier is diagnostic and must not be serialized into project state.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GpuResourceId {
    scope: u64,
    sequence: u64,
    kind: GpuResourceKind,
}

impl GpuResourceId {
    /// Returns the manager scope that owns this resource.
    #[must_use]
    pub const fn scope(self) -> u64 {
        self.scope
    }

    /// Returns the deterministic allocation order within the manager scope.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Returns the concrete managed resource kind.
    #[must_use]
    pub const fn kind(self) -> GpuResourceKind {
        self.kind
    }
}

impl fmt::Display for GpuResourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "gpu-resource:{}:{}:{}",
            self.scope,
            self.kind.code(),
            self.sequence
        )
    }
}

/// Exact live managed-resource counts for one device lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuResourceStats {
    counts: [u64; RESOURCE_KIND_COUNT],
}

impl GpuResourceStats {
    /// Returns the live count for one resource kind.
    #[must_use]
    pub const fn count(self, kind: GpuResourceKind) -> u64 {
        self.counts[kind.index()]
    }

    /// Returns the total number of live managed resources.
    #[must_use]
    pub fn total(self) -> u64 {
        self.counts.into_iter().sum()
    }
}

#[derive(Debug)]
pub(crate) struct ResourceContext {
    device_identity: Arc<()>,
    scope: u64,
    next_sequence: AtomicU64,
    live: [AtomicU64; RESOURCE_KIND_COUNT],
}

/// The owner and factory for resources belonging to one wgpu device lifetime.
///
/// Construct one manager for each acquired device. Device recovery constructs a
/// new manager, and cross-manager checks prevent old resources from being mixed
/// into the recovered device lifetime.
#[derive(Clone, Debug)]
pub struct GpuResources<'device> {
    device: &'device GpuDevice,
    pub(crate) context: Arc<ResourceContext>,
}

impl<'device> GpuResources<'device> {
    /// Creates a resource manager for an acquired Superi device lifetime.
    pub fn new(device: &'device GpuDevice) -> Result<Self> {
        device.ensure_available_for("create_resource_manager")?;
        let scope = NEXT_SCOPE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| exhausted("create_manager", "GPU resource scope identifiers"))?;
        Ok(Self {
            device,
            context: Arc::new(ResourceContext {
                device_identity: Arc::clone(device.identity()),
                scope,
                next_sequence: AtomicU64::new(1),
                live: std::array::from_fn(|_| AtomicU64::new(0)),
            }),
        })
    }

    pub(crate) const fn wgpu_device(&self) -> &wgpu::Device {
        self.device.wgpu_device()
    }

    pub(crate) const fn device(&self) -> &GpuDevice {
        self.device
    }

    pub(crate) const fn enabled_features(&self) -> wgpu::Features {
        self.device.enabled_features()
    }

    pub(crate) const fn enabled_limits(&self) -> &wgpu::Limits {
        self.device.enabled_limits()
    }

    pub(crate) const fn device_identity(&self) -> &Arc<()> {
        self.device.identity()
    }

    pub(crate) fn texture_format_features(
        &self,
        format: wgpu::TextureFormat,
    ) -> wgpu::TextureFormatFeatures {
        if self
            .device
            .enabled_features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            self.device
                .wgpu_adapter()
                .get_texture_format_features(format)
        } else {
            format.guaranteed_format_features(self.device.enabled_features())
        }
    }

    /// Returns this device lifetime's process-local ownership scope.
    #[must_use]
    pub fn scope_id(&self) -> u64 {
        self.context.scope
    }

    /// Returns exact current live counts for managed handles.
    #[must_use]
    pub fn stats(&self) -> GpuResourceStats {
        GpuResourceStats {
            counts: std::array::from_fn(|index| self.context.live[index].load(Ordering::Acquire)),
        }
    }

    pub(crate) fn lease(
        &self,
        kind: GpuResourceKind,
        label: Option<&str>,
    ) -> Result<ResourceLease> {
        self.device
            .ensure_available_for("allocate_managed_resource")?;
        let sequence = self
            .context
            .next_sequence
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| exhausted("allocate_resource", "GPU resource identifiers"))?;
        self.context.live[kind.index()].fetch_add(1, Ordering::Release);
        Ok(ResourceLease {
            context: Arc::clone(&self.context),
            id: GpuResourceId {
                scope: self.context.scope,
                sequence,
                kind,
            },
            label: label.map(Arc::<str>::from),
        })
    }

    pub(crate) fn ensure_owner(
        &self,
        lease: &ResourceLease,
        operation: &'static str,
    ) -> Result<()> {
        if Arc::ptr_eq(
            &self.context.device_identity,
            &lease.context.device_identity,
        ) {
            return Ok(());
        }
        Err(Error::new(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "GPU resource belongs to a different device lifetime",
        )
        .with_context(
            ErrorContext::new("superi-gpu.resource", operation)
                .with_field("expected_scope", self.context.scope.to_string())
                .with_field("resource_id", lease.id.to_string()),
        ))
    }
}

#[derive(Debug)]
pub(crate) struct ResourceLease {
    context: Arc<ResourceContext>,
    id: GpuResourceId,
    label: Option<Arc<str>>,
}

impl ResourceLease {
    pub(crate) const fn id(&self) -> GpuResourceId {
        self.id
    }

    pub(crate) fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

impl Drop for ResourceLease {
    fn drop(&mut self) {
        self.context.live[self.id.kind.index()].fetch_sub(1, Ordering::Release);
    }
}

pub(crate) fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-gpu.resource", operation))
}

fn exhausted(operation: &'static str, resource: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Terminal,
        format!("{resource} are exhausted"),
    )
    .with_context(ErrorContext::new("superi-gpu.resource", operation))
}
