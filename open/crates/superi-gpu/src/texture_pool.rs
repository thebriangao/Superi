//! Aligned GPU texture allocation and exact compatible reuse.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::resource::{GpuResourceId, GpuResources};
use crate::texture::GpuTexture;

const COMPONENT: &str = "superi-gpu.texture_pool";
const ALLOCATION_LABEL: &str = "superi-gpu pooled texture";

/// Reuse granularity for the physical width and height of a texture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextureAlignment {
    width: u32,
    height: u32,
}

impl TextureAlignment {
    /// Creates a nonzero texture alignment in texels.
    pub fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(invalid(
                "create_texture_alignment",
                "texture alignment components must be greater than zero",
            ));
        }
        Ok(Self { width, height })
    }

    /// Returns the horizontal allocation granularity in texels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the vertical allocation granularity in texels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }
}

impl Default for TextureAlignment {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
        }
    }
}

/// One logical texture request and its physical reuse constraints.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextureRequest {
    label: Option<String>,
    logical_size: wgpu::Extent3d,
    alignment: TextureAlignment,
    mip_level_count: u32,
    sample_count: u32,
    dimension: wgpu::TextureDimension,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    view_formats: Vec<wgpu::TextureFormat>,
}

impl TextureRequest {
    /// Creates a single-sample two-dimensional texture request.
    #[must_use]
    pub fn new(
        logical_size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> Self {
        Self {
            label: None,
            logical_size,
            alignment: TextureAlignment::default(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: Vec::new(),
        }
    }

    /// Sets the diagnostic label for this checkout.
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the physical width and height reuse granularity.
    #[must_use]
    pub const fn with_alignment(mut self, alignment: TextureAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Sets the allocated mip level count.
    #[must_use]
    pub const fn with_mip_level_count(mut self, mip_level_count: u32) -> Self {
        self.mip_level_count = mip_level_count;
        self
    }

    /// Sets the allocated sample count.
    #[must_use]
    pub const fn with_sample_count(mut self, sample_count: u32) -> Self {
        self.sample_count = sample_count;
        self
    }

    /// Sets the texture dimension.
    #[must_use]
    pub const fn with_dimension(mut self, dimension: wgpu::TextureDimension) -> Self {
        self.dimension = dimension;
        self
    }

    /// Sets additional permitted texture view formats.
    #[must_use]
    pub fn with_view_formats(
        mut self,
        formats: impl IntoIterator<Item = wgpu::TextureFormat>,
    ) -> Self {
        self.view_formats.clear();
        for format in formats {
            if !self.view_formats.contains(&format) {
                self.view_formats.push(format);
            }
        }
        self
    }

    /// Returns the current checkout label.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the exact logical extent visible to the consumer.
    #[must_use]
    pub const fn logical_size(&self) -> wgpu::Extent3d {
        self.logical_size
    }

    /// Returns the configured physical reuse granularity.
    #[must_use]
    pub const fn alignment(&self) -> TextureAlignment {
        self.alignment
    }

    /// Returns the requested mip level count.
    #[must_use]
    pub const fn mip_level_count(&self) -> u32 {
        self.mip_level_count
    }

    /// Returns the requested sample count.
    #[must_use]
    pub const fn sample_count(&self) -> u32 {
        self.sample_count
    }

    /// Returns the requested texture dimension.
    #[must_use]
    pub const fn dimension(&self) -> wgpu::TextureDimension {
        self.dimension
    }

    /// Returns the requested base texture format.
    #[must_use]
    pub const fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Returns every requested texture usage.
    #[must_use]
    pub const fn usage(&self) -> wgpu::TextureUsages {
        self.usage
    }

    /// Returns the additional permitted view formats.
    #[must_use]
    pub fn view_formats(&self) -> &[wgpu::TextureFormat] {
        &self.view_formats
    }

    /// Calculates the physical allocation extent with checked arithmetic.
    ///
    /// Width and height combine the requested reuse granularity with the
    /// format's texel block or plane requirements. Logical depth and array
    /// layer counts are never silently padded.
    pub fn allocation_size(&self) -> Result<wgpu::Extent3d> {
        validate_logical_extent(self.logical_size, self.dimension)?;
        let (format_width, format_height) = self.format.size_multiple_requirement();
        let width_alignment = checked_lcm(self.alignment.width, format_width)?;
        let height_alignment = checked_lcm(self.alignment.height, format_height)?;
        let width = align_up(self.logical_size.width, width_alignment)?;
        let height = match self.dimension {
            wgpu::TextureDimension::D1 => 1,
            wgpu::TextureDimension::D2 | wgpu::TextureDimension::D3 => {
                align_up(self.logical_size.height, height_alignment)?
            }
        };
        Ok(wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: self.logical_size.depth_or_array_layers,
        })
    }
}

/// Bounded idle retention for one texture pool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TexturePoolConfig {
    max_idle_per_key: usize,
}

impl TexturePoolConfig {
    /// Creates a pool configuration. Zero disables idle retention.
    #[must_use]
    pub const fn new(max_idle_per_key: usize) -> Self {
        Self { max_idle_per_key }
    }

    /// Returns the maximum idle allocations retained for one compatibility key.
    #[must_use]
    pub const fn max_idle_per_key(self) -> usize {
        self.max_idle_per_key
    }
}

impl Default for TexturePoolConfig {
    fn default() -> Self {
        Self::new(2)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TextureKey {
    size: wgpu::Extent3d,
    mip_level_count: u32,
    sample_count: u32,
    dimension: wgpu::TextureDimension,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    view_formats: Vec<wgpu::TextureFormat>,
}

impl TextureKey {
    fn from_request(request: &TextureRequest, size: wgpu::Extent3d) -> Self {
        Self {
            size,
            mip_level_count: request.mip_level_count,
            sample_count: request.sample_count,
            dimension: request.dimension,
            format: request.format,
            usage: request.usage,
            view_formats: request.view_formats.clone(),
        }
    }

    fn descriptor(&self) -> wgpu::TextureDescriptor<'_> {
        wgpu::TextureDescriptor {
            label: Some(ALLOCATION_LABEL),
            size: self.size,
            mip_level_count: self.mip_level_count,
            sample_count: self.sample_count,
            dimension: self.dimension,
            format: self.format,
            usage: self.usage,
            view_formats: &self.view_formats,
        }
    }
}

#[derive(Debug, Default)]
struct TexturePoolState {
    idle: HashMap<TextureKey, Vec<GpuTexture>>,
    allocations: u64,
    reuses: u64,
    checked_out: u64,
    discarded: u64,
}

#[derive(Debug)]
struct TexturePoolInner<'device> {
    resources: GpuResources<'device>,
    config: TexturePoolConfig,
    state: Mutex<TexturePoolState>,
}

impl TexturePoolInner<'_> {
    fn lock(&self, operation: &'static str) -> Result<MutexGuard<'_, TexturePoolState>> {
        self.state.lock().map_err(|_| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "texture pool state is unavailable after a panic",
            )
            .with_context(ErrorContext::new(COMPONENT, operation))
        })
    }

    fn release(&self, key: TextureKey, texture: GpuTexture) {
        let reusable = texture.has_unique_allocation_owner();
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.checked_out = state.checked_out.saturating_sub(1);
        if reusable && self.config.max_idle_per_key > 0 {
            let idle = state.idle.entry(key).or_default();
            if idle.len() < self.config.max_idle_per_key {
                idle.push(texture);
                return;
            }
        }
        state.discarded = state.discarded.saturating_add(1);
    }
}

/// Exact process-local counters for one texture pool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TexturePoolStats {
    allocations: u64,
    reuses: u64,
    checked_out: u64,
    idle: u64,
    discarded: u64,
}

impl TexturePoolStats {
    /// Returns the number of physical wgpu allocations created by this pool.
    #[must_use]
    pub const fn allocations(self) -> u64 {
        self.allocations
    }

    /// Returns the number of checkouts satisfied by an idle allocation.
    #[must_use]
    pub const fn reuses(self) -> u64 {
        self.reuses
    }

    /// Returns the current number of checked out allocations.
    #[must_use]
    pub const fn checked_out(self) -> u64 {
        self.checked_out
    }

    /// Returns the current number of idle allocations retained for reuse.
    #[must_use]
    pub const fn idle(self) -> u64 {
        self.idle
    }

    /// Returns the number of returns not retained because they were busy or over capacity.
    #[must_use]
    pub const fn discarded(self) -> u64 {
        self.discarded
    }
}

/// A device-scoped cache of aligned, exactly compatible texture allocations.
#[derive(Clone, Debug)]
pub struct TexturePool<'device> {
    inner: Arc<TexturePoolInner<'device>>,
}

impl<'device> TexturePool<'device> {
    /// Creates a pool for one managed wgpu device lifetime.
    #[must_use]
    pub fn new(resources: GpuResources<'device>, config: TexturePoolConfig) -> Self {
        Self {
            inner: Arc::new(TexturePoolInner {
                resources,
                config,
                state: Mutex::new(TexturePoolState::default()),
            }),
        }
    }

    /// Acquires one exclusively checked out physical texture allocation.
    ///
    /// Every checkout requires complete initialization of the logical region
    /// before any read. Reused allocations intentionally retain no content
    /// validity promise.
    pub fn acquire(&self, request: &TextureRequest) -> Result<PooledTexture<'device>> {
        let allocation_size = request.allocation_size()?;
        validate_device_request(&self.inner.resources, request, allocation_size)?;
        let key = TextureKey::from_request(request, allocation_size);

        let reused = {
            let mut state = self.inner.lock("acquire_texture")?;
            let texture = state.idle.get_mut(&key).and_then(Vec::pop);
            if texture.is_some() {
                state.reuses = state.reuses.saturating_add(1);
                state.checked_out = state.checked_out.saturating_add(1);
            }
            texture
        };

        let texture = if let Some(texture) = reused {
            texture
        } else {
            let texture = self.inner.resources.create_texture(&key.descriptor())?;
            let mut state = self.inner.lock("record_texture_allocation")?;
            state.allocations = state.allocations.saturating_add(1);
            state.checked_out = state.checked_out.saturating_add(1);
            texture
        };

        Ok(PooledTexture {
            pool: Arc::clone(&self.inner),
            key,
            texture: Some(texture),
            logical_size: request.logical_size,
            label: request.label.clone(),
        })
    }

    /// Returns a consistent snapshot of current pool counters.
    pub fn stats(&self) -> Result<TexturePoolStats> {
        let state = self.inner.lock("read_texture_pool_stats")?;
        let idle = state.idle.values().try_fold(0_u64, |total, textures| {
            let count = u64::try_from(textures.len()).map_err(|_| {
                internal(
                    "read_texture_pool_stats",
                    "idle texture count does not fit in diagnostics",
                )
            })?;
            total.checked_add(count).ok_or_else(|| {
                internal(
                    "read_texture_pool_stats",
                    "idle texture count exceeds diagnostics range",
                )
            })
        })?;
        Ok(TexturePoolStats {
            allocations: state.allocations,
            reuses: state.reuses,
            checked_out: state.checked_out,
            idle,
            discarded: state.discarded,
        })
    }

    /// Drops every currently idle allocation and returns the number released.
    pub fn drain_idle(&self) -> Result<u64> {
        let drained = {
            let mut state = self.inner.lock("drain_idle_textures")?;
            std::mem::take(&mut state.idle)
        };
        drained.values().try_fold(0_u64, |total, textures| {
            let count = u64::try_from(textures.len()).map_err(|_| {
                internal(
                    "drain_idle_textures",
                    "idle texture count does not fit in diagnostics",
                )
            })?;
            total.checked_add(count).ok_or_else(|| {
                internal(
                    "drain_idle_textures",
                    "idle texture count exceeds diagnostics range",
                )
            })
        })
    }
}

/// One non-cloneable texture checkout that returns to its pool on drop.
pub struct PooledTexture<'device> {
    pool: Arc<TexturePoolInner<'device>>,
    key: TextureKey,
    texture: Option<GpuTexture>,
    logical_size: wgpu::Extent3d,
    label: Option<String>,
}

impl fmt::Debug for PooledTexture<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PooledTexture")
            .field("allocation_id", &self.allocation_id())
            .field("logical_size", &self.logical_size)
            .field("allocation_size", &self.key.size)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl PooledTexture<'_> {
    /// Returns the process-local identifier of the physical allocation.
    #[must_use]
    pub fn allocation_id(&self) -> GpuResourceId {
        self.texture().id()
    }

    /// Returns the current checkout label.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the exact logical extent requested by the consumer.
    #[must_use]
    pub const fn logical_size(&self) -> wgpu::Extent3d {
        self.logical_size
    }

    /// Returns the padded physical extent of the reusable allocation.
    #[must_use]
    pub const fn allocation_size(&self) -> wgpu::Extent3d {
        self.key.size
    }

    /// Returns true because every checkout must initialize its logical region.
    #[must_use]
    pub const fn requires_full_initialization(&self) -> bool {
        true
    }

    /// Borrows the managed texture for views, bindings, and ownership checks.
    ///
    /// If a clone or dependent resource outlives this checkout, the allocation
    /// is discarded instead of returning to the idle reuse set.
    #[must_use]
    pub fn texture(&self) -> &GpuTexture {
        self.texture
            .as_ref()
            .expect("pooled texture remains present until drop")
    }

    /// Borrows the raw wgpu texture for command recording and queue operations.
    #[must_use]
    pub fn raw(&self) -> &wgpu::Texture {
        self.texture().raw()
    }
}

impl Drop for PooledTexture<'_> {
    fn drop(&mut self) {
        if let Some(texture) = self.texture.take() {
            self.pool.release(self.key.clone(), texture);
        }
    }
}

fn validate_logical_extent(size: wgpu::Extent3d, dimension: wgpu::TextureDimension) -> Result<()> {
    if size.width == 0 || size.height == 0 || size.depth_or_array_layers == 0 {
        return Err(invalid(
            "align_texture_extent",
            "logical texture extent components must be greater than zero",
        ));
    }
    if dimension == wgpu::TextureDimension::D1
        && (size.height != 1 || size.depth_or_array_layers != 1)
    {
        return Err(invalid(
            "align_texture_extent",
            "one-dimensional textures require height and depth to equal one",
        ));
    }
    Ok(())
}

fn validate_device_request(
    resources: &GpuResources<'_>,
    request: &TextureRequest,
    size: wgpu::Extent3d,
) -> Result<()> {
    if request.usage.is_empty() {
        return Err(invalid(
            "acquire_texture",
            "texture usage must not be empty",
        ));
    }
    if request.mip_level_count == 0 || request.sample_count == 0 {
        return Err(invalid(
            "acquire_texture",
            "texture mip and sample counts must be greater than zero",
        ));
    }

    let device = resources.wgpu_device();
    let limits = device.limits();
    let within_limits = match request.dimension {
        wgpu::TextureDimension::D1 => size.width <= limits.max_texture_dimension_1d,
        wgpu::TextureDimension::D2 => {
            size.width <= limits.max_texture_dimension_2d
                && size.height <= limits.max_texture_dimension_2d
                && size.depth_or_array_layers <= limits.max_texture_array_layers
        }
        wgpu::TextureDimension::D3 => {
            size.width <= limits.max_texture_dimension_3d
                && size.height <= limits.max_texture_dimension_3d
                && size.depth_or_array_layers <= limits.max_texture_dimension_3d
        }
    };
    if !within_limits {
        return Err(Error::new(
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            "aligned texture extent exceeds enabled device limits",
        )
        .with_context(request_context(request, "acquire_texture", size)));
    }

    let maximum_mips = size.max_mips(request.dimension);
    if request.mip_level_count > maximum_mips {
        return Err(invalid_with_request(
            request,
            "acquire_texture",
            size,
            format!(
                "texture requests {} mip levels but the aligned extent supports {maximum_mips}",
                request.mip_level_count
            ),
        ));
    }

    let missing_features = request.format.required_features() - device.features();
    if !missing_features.is_empty() {
        return Err(unsupported_with_request(
            request,
            "acquire_texture",
            size,
            format!("texture format requires unavailable features {missing_features:?}"),
        ));
    }
    let format_features = resources.texture_format_features(request.format);
    if !format_features.allowed_usages.contains(request.usage) {
        return Err(unsupported_with_request(
            request,
            "acquire_texture",
            size,
            "texture format does not support every requested usage",
        ));
    }
    if !format_features
        .flags
        .sample_count_supported(request.sample_count)
    {
        return Err(unsupported_with_request(
            request,
            "acquire_texture",
            size,
            "texture format does not support the requested sample count",
        ));
    }

    if request.sample_count > 1
        && (request.dimension != wgpu::TextureDimension::D2
            || size.depth_or_array_layers != 1
            || request.mip_level_count != 1
            || request.usage.contains(wgpu::TextureUsages::STORAGE_BINDING))
    {
        return Err(invalid_with_request(
            request,
            "acquire_texture",
            size,
            "multisampled textures must be two-dimensional, single-layer, single-mip, and non-storage",
        ));
    }
    if request.dimension != wgpu::TextureDimension::D2
        && (request.format.is_depth_stencil_format()
            || request
                .usage
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT))
    {
        return Err(invalid_with_request(
            request,
            "acquire_texture",
            size,
            "depth and render-attachment textures must be two-dimensional",
        ));
    }
    if request.format.is_compressed() && request.dimension == wgpu::TextureDimension::D1 {
        return Err(invalid_with_request(
            request,
            "acquire_texture",
            size,
            "compressed textures cannot be one-dimensional",
        ));
    }
    if request.format.is_compressed()
        && request.dimension == wgpu::TextureDimension::D3
        && (!request.format.is_bcn()
            || !device
                .features()
                .contains(wgpu::Features::TEXTURE_COMPRESSION_BC_SLICED_3D))
    {
        return Err(unsupported_with_request(
            request,
            "acquire_texture",
            size,
            "three-dimensional compressed textures require sliced BC support",
        ));
    }

    for view_format in &request.view_formats {
        if *view_format != request.format
            && view_format.remove_srgb_suffix() != request.format.remove_srgb_suffix()
        {
            return Err(invalid_with_request(
                request,
                "acquire_texture",
                size,
                "texture view formats may differ from the base format only by srgb encoding",
            ));
        }
    }
    Ok(())
}

fn checked_lcm(left: u32, right: u32) -> Result<u32> {
    let divisor = gcd(left, right);
    left.checked_div(divisor)
        .and_then(|reduced| reduced.checked_mul(right))
        .ok_or_else(|| {
            exhausted(
                "align_texture_extent",
                "combined texture alignment exceeds the supported range",
            )
        })
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn align_up(value: u32, alignment: u32) -> Result<u32> {
    let remainder = value % alignment;
    if remainder == 0 {
        return Ok(value);
    }
    value.checked_add(alignment - remainder).ok_or_else(|| {
        exhausted(
            "align_texture_extent",
            "aligned texture extent exceeds the supported range",
        )
    })
}

fn request_context(
    request: &TextureRequest,
    operation: &'static str,
    allocation_size: wgpu::Extent3d,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
        .with_field("logical_width", request.logical_size.width.to_string())
        .with_field("logical_height", request.logical_size.height.to_string())
        .with_field(
            "logical_depth_or_layers",
            request.logical_size.depth_or_array_layers.to_string(),
        )
        .with_field("allocation_width", allocation_size.width.to_string())
        .with_field("allocation_height", allocation_size.height.to_string())
        .with_field(
            "allocation_depth_or_layers",
            allocation_size.depth_or_array_layers.to_string(),
        )
        .with_field("format", format!("{:?}", request.format))
        .with_field("usage", format!("{:#x}", request.usage.bits()))
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn invalid_with_request(
    request: &TextureRequest,
    operation: &'static str,
    size: wgpu::Extent3d,
    message: impl Into<String>,
) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(request_context(request, operation, size))
}

fn unsupported_with_request(
    request: &TextureRequest,
    operation: &'static str,
    size: wgpu::Extent3d,
    message: impl Into<String>,
) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(request_context(request, operation, size))
}

fn exhausted(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn internal(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use crate::device::{AdapterSelection, DeviceRequest, GpuInstance, InstanceOptions};

    use super::*;

    fn test_device() -> Option<crate::device::GpuDevice> {
        let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
        let adapter = instance
            .enumerate_adapters()
            .select(&AdapterSelection::default())
            .ok()?;
        pollster::block_on(adapter.create_device(
            &DeviceRequest::default().with_label("superi texture pool render contract"),
        ))
        .ok()
    }

    #[test]
    fn alignment_math_handles_coprime_format_and_reuse_multiples() {
        assert_eq!(gcd(6, 4), 2);
        assert_eq!(checked_lcm(6, 4).unwrap(), 12);
        assert_eq!(align_up(13, 12).unwrap(), 24);
    }

    #[test]
    fn one_dimensional_requests_preserve_required_unit_extent() {
        let request = TextureRequest::new(
            wgpu::Extent3d {
                width: 17,
                height: 1,
                depth_or_array_layers: 1,
            },
            wgpu::TextureFormat::R8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING,
        )
        .with_dimension(wgpu::TextureDimension::D1)
        .with_alignment(TextureAlignment::new(8, 16).unwrap());
        assert_eq!(
            request.allocation_size().unwrap(),
            wgpu::Extent3d {
                width: 24,
                height: 1,
                depth_or_array_layers: 1,
            }
        );
    }

    #[test]
    fn pooled_texture_executes_a_render_then_reuses_after_dependents_drop() {
        let Some(device) = test_device() else {
            eprintln!("no wgpu adapter is available, skipping texture render contract");
            return;
        };
        let resources = GpuResources::new(&device).unwrap();
        let pool = TexturePool::new(resources.clone(), TexturePoolConfig::new(1));
        let request = TextureRequest::new(
            wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        )
        .with_alignment(TextureAlignment::new(8, 8).unwrap())
        .with_label("render frame");
        let texture = pool.acquire(&request).unwrap();
        let allocation_id = texture.allocation_id();
        let view = resources
            .create_texture_view(texture.texture(), &wgpu::TextureViewDescriptor::default())
            .unwrap();
        let raw_device = device.wgpu_device();
        let readback = raw_device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture pool readback"),
            size: 256 * 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = raw_device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("texture pool render"),
        });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("texture pool clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: view.raw(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.25,
                            g: 0.5,
                            b: 0.75,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: texture.raw(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &readback,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(4),
                },
            },
            texture.logical_size(),
        );
        device.submit_viewport([encoder.finish()]);

        let slice = readback.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).expect("map receiver remains alive");
        });
        let _ = raw_device.poll(wgpu::Maintain::Wait);
        receiver
            .recv()
            .expect("mapping callback must run")
            .expect("readback mapping must succeed");
        assert_eq!(&slice.get_mapped_range()[..4], &[64, 128, 191, 255]);
        readback.unmap();

        drop(view);
        drop(texture);
        let reused = pool.acquire(&request.with_label("next frame")).unwrap();
        assert_eq!(reused.allocation_id(), allocation_id);
    }
}
