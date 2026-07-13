//! Managed GPU textures and views with retained allocation lifetimes.

use std::sync::Arc;

use superi_core::error::Result;

use crate::pool::MemoryReservation;
use crate::resource::{invalid, GpuResourceId, GpuResourceKind, GpuResources, ResourceLease};

/// An owned snapshot of the descriptor used to create a texture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuTextureInfo {
    label: Option<String>,
    size: wgpu::Extent3d,
    mip_level_count: u32,
    sample_count: u32,
    dimension: wgpu::TextureDimension,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    view_formats: Vec<wgpu::TextureFormat>,
}

impl GpuTextureInfo {
    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the complete texture extent.
    #[must_use]
    pub const fn size(&self) -> wgpu::Extent3d {
        self.size
    }

    /// Returns the number of allocated mip levels.
    #[must_use]
    pub const fn mip_level_count(&self) -> u32 {
        self.mip_level_count
    }

    /// Returns the multisample count.
    #[must_use]
    pub const fn sample_count(&self) -> u32 {
        self.sample_count
    }

    /// Returns the texture dimension.
    #[must_use]
    pub const fn dimension(&self) -> wgpu::TextureDimension {
        self.dimension
    }

    /// Returns the base texture format.
    #[must_use]
    pub const fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Returns every permitted texture usage.
    #[must_use]
    pub const fn usage(&self) -> wgpu::TextureUsages {
        self.usage
    }

    /// Returns the additional permitted view formats.
    #[must_use]
    pub fn view_formats(&self) -> &[wgpu::TextureFormat] {
        &self.view_formats
    }
}

#[derive(Debug)]
struct GpuTextureInner {
    lease: ResourceLease,
    raw: wgpu::Texture,
    info: GpuTextureInfo,
    memory: Option<MemoryReservation>,
}

/// A cloneable, device-scoped owner for one texture allocation.
#[derive(Clone, Debug)]
pub struct GpuTexture(Arc<GpuTextureInner>);

impl GpuTexture {
    /// Returns this texture's process-local diagnostic identifier.
    #[must_use]
    pub fn id(&self) -> GpuResourceId {
        self.0.lease.id()
    }

    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.0.lease.label()
    }

    /// Returns the owned creation descriptor snapshot.
    #[must_use]
    pub fn info(&self) -> &GpuTextureInfo {
        &self.0.info
    }

    /// Borrows the raw wgpu texture for command encoding and queue operations.
    #[must_use]
    pub fn raw(&self) -> &wgpu::Texture {
        &self.0.raw
    }

    /// Returns managed payload bytes attached to this allocation, when budgeted.
    #[must_use]
    pub fn accounted_bytes(&self) -> Option<u64> {
        self.0.memory.as_ref().map(MemoryReservation::bytes)
    }

    pub(crate) fn has_unique_allocation_owner(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }

    pub(crate) fn lease(&self) -> &ResourceLease {
        &self.0.lease
    }
}

/// An owned snapshot of one managed texture-view descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuTextureViewInfo {
    label: Option<String>,
    format: Option<wgpu::TextureFormat>,
    dimension: Option<wgpu::TextureViewDimension>,
    aspect: wgpu::TextureAspect,
    base_mip_level: u32,
    mip_level_count: Option<u32>,
    base_array_layer: u32,
    array_layer_count: Option<u32>,
}

impl GpuTextureViewInfo {
    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the explicitly requested view format.
    #[must_use]
    pub const fn format(&self) -> Option<wgpu::TextureFormat> {
        self.format
    }

    /// Returns the explicitly requested view dimension.
    #[must_use]
    pub const fn dimension(&self) -> Option<wgpu::TextureViewDimension> {
        self.dimension
    }

    /// Returns the selected texture aspect.
    #[must_use]
    pub const fn aspect(&self) -> wgpu::TextureAspect {
        self.aspect
    }

    /// Returns the first visible mip level.
    #[must_use]
    pub const fn base_mip_level(&self) -> u32 {
        self.base_mip_level
    }

    /// Returns the explicit visible mip count, or None for the remaining levels.
    #[must_use]
    pub const fn mip_level_count(&self) -> Option<u32> {
        self.mip_level_count
    }

    /// Returns the first visible array layer.
    #[must_use]
    pub const fn base_array_layer(&self) -> u32 {
        self.base_array_layer
    }

    /// Returns the explicit visible layer count, or None for the remaining layers.
    #[must_use]
    pub const fn array_layer_count(&self) -> Option<u32> {
        self.array_layer_count
    }
}

#[derive(Debug)]
struct GpuTextureViewInner {
    lease: ResourceLease,
    raw: wgpu::TextureView,
    texture: GpuTexture,
    info: GpuTextureViewInfo,
}

/// A cloneable view that retains its managed texture allocation.
#[derive(Clone, Debug)]
pub struct GpuTextureView(Arc<GpuTextureViewInner>);

impl GpuTextureView {
    /// Returns this view's process-local diagnostic identifier.
    #[must_use]
    pub fn id(&self) -> GpuResourceId {
        self.0.lease.id()
    }

    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.0.lease.label()
    }

    /// Returns the owned view descriptor snapshot.
    #[must_use]
    pub fn info(&self) -> &GpuTextureViewInfo {
        &self.0.info
    }

    /// Returns the texture retained by this view.
    #[must_use]
    pub fn texture(&self) -> &GpuTexture {
        &self.0.texture
    }

    /// Borrows the raw wgpu texture view for bindings and render passes.
    #[must_use]
    pub fn raw(&self) -> &wgpu::TextureView {
        &self.0.raw
    }

    pub(crate) fn lease(&self) -> &ResourceLease {
        &self.0.lease
    }
}

impl GpuResources<'_> {
    /// Creates and tracks a texture in this manager's device lifetime.
    pub fn create_texture(&self, descriptor: &wgpu::TextureDescriptor<'_>) -> Result<GpuTexture> {
        self.create_texture_inner(descriptor, None)
    }

    pub(crate) fn create_texture_with_reservation(
        &self,
        descriptor: &wgpu::TextureDescriptor<'_>,
        memory: MemoryReservation,
    ) -> Result<GpuTexture> {
        self.create_texture_inner(descriptor, Some(memory))
    }

    fn create_texture_inner(
        &self,
        descriptor: &wgpu::TextureDescriptor<'_>,
        memory: Option<MemoryReservation>,
    ) -> Result<GpuTexture> {
        if descriptor.size.width == 0
            || descriptor.size.height == 0
            || descriptor.size.depth_or_array_layers == 0
        {
            return Err(invalid(
                "create_texture",
                "GPU texture extent components must all be greater than zero",
            ));
        }
        if descriptor.mip_level_count == 0 || descriptor.sample_count == 0 {
            return Err(invalid(
                "create_texture",
                "GPU texture mip and sample counts must be greater than zero",
            ));
        }
        if descriptor.usage.is_empty() {
            return Err(invalid(
                "create_texture",
                "GPU texture usage must not be empty",
            ));
        }

        let info = GpuTextureInfo {
            label: descriptor.label.map(str::to_owned),
            size: descriptor.size,
            mip_level_count: descriptor.mip_level_count,
            sample_count: descriptor.sample_count,
            dimension: descriptor.dimension,
            format: descriptor.format,
            usage: descriptor.usage,
            view_formats: descriptor.view_formats.to_vec(),
        };
        let raw = self.wgpu_device().create_texture(descriptor);
        let lease = self.lease(GpuResourceKind::Texture, descriptor.label)?;
        Ok(GpuTexture(Arc::new(GpuTextureInner {
            lease,
            raw,
            info,
            memory,
        })))
    }

    /// Creates a managed view and retains its parent texture for the view lifetime.
    pub fn create_texture_view(
        &self,
        texture: &GpuTexture,
        descriptor: &wgpu::TextureViewDescriptor<'_>,
    ) -> Result<GpuTextureView> {
        self.ensure_owner(texture.lease(), "create_texture_view")?;
        validate_view(texture.info(), descriptor)?;

        let info = GpuTextureViewInfo {
            label: descriptor.label.map(str::to_owned),
            format: descriptor.format,
            dimension: descriptor.dimension,
            aspect: descriptor.aspect,
            base_mip_level: descriptor.base_mip_level,
            mip_level_count: descriptor.mip_level_count,
            base_array_layer: descriptor.base_array_layer,
            array_layer_count: descriptor.array_layer_count,
        };
        let raw = texture.raw().create_view(descriptor);
        let lease = self.lease(GpuResourceKind::TextureView, descriptor.label)?;
        Ok(GpuTextureView(Arc::new(GpuTextureViewInner {
            lease,
            raw,
            texture: texture.clone(),
            info,
        })))
    }
}

fn validate_view(
    texture: &GpuTextureInfo,
    descriptor: &wgpu::TextureViewDescriptor<'_>,
) -> Result<()> {
    let view_format = descriptor.format.unwrap_or(texture.format);
    if view_format != texture.format && !texture.view_formats.contains(&view_format) {
        return Err(invalid(
            "create_texture_view",
            "texture view format was not declared by the texture",
        ));
    }

    let mip_count = descriptor.mip_level_count.unwrap_or_else(|| {
        texture
            .mip_level_count
            .saturating_sub(descriptor.base_mip_level)
    });
    if mip_count == 0
        || descriptor
            .base_mip_level
            .checked_add(mip_count)
            .map_or(true, |end| end > texture.mip_level_count)
    {
        return Err(invalid(
            "create_texture_view",
            "texture view mip range is outside the texture",
        ));
    }

    let layers = texture.size.depth_or_array_layers;
    let layer_count = descriptor
        .array_layer_count
        .unwrap_or_else(|| layers.saturating_sub(descriptor.base_array_layer));
    if layer_count == 0
        || descriptor
            .base_array_layer
            .checked_add(layer_count)
            .map_or(true, |end| end > layers)
    {
        return Err(invalid(
            "create_texture_view",
            "texture view array-layer range is outside the texture",
        ));
    }
    Ok(())
}
