//! Managed samplers, bind-group layouts, and retained bind groups.

use std::collections::BTreeSet;
use std::sync::Arc;

use superi_core::error::Result;

use crate::buffer::GpuBuffer;
use crate::resource::{invalid, GpuResourceId, GpuResourceKind, GpuResources, ResourceLease};
use crate::texture::GpuTextureView;

pub use crate::pipeline::GpuPipelineLayoutDescriptor;

/// An owned snapshot of a sampler descriptor.
#[derive(Clone, Debug, PartialEq)]
pub struct GpuSamplerInfo {
    label: Option<String>,
    address_mode_u: wgpu::AddressMode,
    address_mode_v: wgpu::AddressMode,
    address_mode_w: wgpu::AddressMode,
    mag_filter: wgpu::FilterMode,
    min_filter: wgpu::FilterMode,
    mipmap_filter: wgpu::FilterMode,
    lod_min_clamp: f32,
    lod_max_clamp: f32,
    compare: Option<wgpu::CompareFunction>,
    anisotropy_clamp: u16,
    border_color: Option<wgpu::SamplerBorderColor>,
}

impl GpuSamplerInfo {
    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the address mode for each texture axis.
    #[must_use]
    pub const fn address_modes(&self) -> (wgpu::AddressMode, wgpu::AddressMode, wgpu::AddressMode) {
        (
            self.address_mode_u,
            self.address_mode_v,
            self.address_mode_w,
        )
    }

    /// Returns the magnification, minification, and mip-map filters.
    #[must_use]
    pub const fn filters(&self) -> (wgpu::FilterMode, wgpu::FilterMode, wgpu::FilterMode) {
        (self.mag_filter, self.min_filter, self.mipmap_filter)
    }

    /// Returns the inclusive level-of-detail clamp range.
    #[must_use]
    pub const fn lod_clamp(&self) -> (f32, f32) {
        (self.lod_min_clamp, self.lod_max_clamp)
    }

    /// Returns the comparison function, when this is a comparison sampler.
    #[must_use]
    pub const fn compare(&self) -> Option<wgpu::CompareFunction> {
        self.compare
    }

    /// Returns the maximum anisotropy value.
    #[must_use]
    pub const fn anisotropy_clamp(&self) -> u16 {
        self.anisotropy_clamp
    }

    /// Returns the optional clamp-to-border color.
    #[must_use]
    pub const fn border_color(&self) -> Option<wgpu::SamplerBorderColor> {
        self.border_color
    }
}

#[derive(Debug)]
struct GpuSamplerInner {
    lease: ResourceLease,
    raw: wgpu::Sampler,
    info: GpuSamplerInfo,
}

/// A cloneable, device-scoped sampler owner.
#[derive(Clone, Debug)]
pub struct GpuSampler(Arc<GpuSamplerInner>);

impl GpuSampler {
    /// Returns this sampler's process-local diagnostic identifier.
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
    pub fn info(&self) -> &GpuSamplerInfo {
        &self.0.info
    }

    /// Borrows the raw wgpu sampler for bind-group creation.
    #[must_use]
    pub fn raw(&self) -> &wgpu::Sampler {
        &self.0.raw
    }

    pub(crate) fn lease(&self) -> &ResourceLease {
        &self.0.lease
    }
}

/// An owned snapshot of a bind-group layout descriptor.
#[derive(Clone, Debug)]
pub struct GpuBindGroupLayoutInfo {
    label: Option<String>,
    entries: Vec<wgpu::BindGroupLayoutEntry>,
}

impl GpuBindGroupLayoutInfo {
    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns layout entries in descriptor order.
    #[must_use]
    pub fn entries(&self) -> &[wgpu::BindGroupLayoutEntry] {
        &self.entries
    }
}

#[derive(Debug)]
struct GpuBindGroupLayoutInner {
    lease: ResourceLease,
    raw: wgpu::BindGroupLayout,
    info: GpuBindGroupLayoutInfo,
}

/// A cloneable explicit bind-group layout.
#[derive(Clone, Debug)]
pub struct GpuBindGroupLayout(Arc<GpuBindGroupLayoutInner>);

impl GpuBindGroupLayout {
    /// Returns this layout's process-local diagnostic identifier.
    #[must_use]
    pub fn id(&self) -> GpuResourceId {
        self.0.lease.id()
    }

    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.0.lease.label()
    }

    /// Returns the owned layout descriptor snapshot.
    #[must_use]
    pub fn info(&self) -> &GpuBindGroupLayoutInfo {
        &self.0.info
    }

    /// Borrows the raw wgpu layout for bind groups and pipeline layouts.
    #[must_use]
    pub fn raw(&self) -> &wgpu::BindGroupLayout {
        &self.0.raw
    }

    pub(crate) fn lease(&self) -> &ResourceLease {
        &self.0.lease
    }
}

/// One managed buffer range used by a bind-group entry.
#[derive(Clone, Debug)]
pub struct GpuBufferBinding {
    buffer: GpuBuffer,
    offset: wgpu::BufferAddress,
    size: Option<wgpu::BufferSize>,
}

impl GpuBufferBinding {
    /// Binds the entire managed buffer.
    #[must_use]
    pub fn whole(buffer: GpuBuffer) -> Self {
        Self {
            buffer,
            offset: 0,
            size: None,
        }
    }

    /// Binds an explicit byte range within a managed buffer.
    #[must_use]
    pub fn new(
        buffer: GpuBuffer,
        offset: wgpu::BufferAddress,
        size: Option<wgpu::BufferSize>,
    ) -> Self {
        Self {
            buffer,
            offset,
            size,
        }
    }

    /// Returns the managed buffer.
    #[must_use]
    pub const fn buffer(&self) -> &GpuBuffer {
        &self.buffer
    }

    /// Returns the starting byte offset.
    #[must_use]
    pub const fn offset(&self) -> wgpu::BufferAddress {
        self.offset
    }

    /// Returns the explicit byte size, or None for the remaining buffer.
    #[must_use]
    pub const fn size(&self) -> Option<wgpu::BufferSize> {
        self.size
    }

    fn raw(&self) -> wgpu::BufferBinding<'_> {
        wgpu::BufferBinding {
            buffer: self.buffer.raw(),
            offset: self.offset,
            size: self.size,
        }
    }
}

/// One owned resource or resource array used by a bind group.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum GpuBindingResource {
    /// One buffer range.
    Buffer(GpuBufferBinding),
    /// An array of buffer ranges.
    BufferArray(Vec<GpuBufferBinding>),
    /// One sampler.
    Sampler(GpuSampler),
    /// An array of samplers.
    SamplerArray(Vec<GpuSampler>),
    /// One texture view.
    TextureView(GpuTextureView),
    /// An array of texture views.
    TextureViewArray(Vec<GpuTextureView>),
}

/// One numbered, owned bind-group entry.
#[derive(Clone, Debug)]
pub struct GpuBindGroupEntry {
    binding: u32,
    resource: GpuBindingResource,
}

impl GpuBindGroupEntry {
    /// Creates an entry that binds the entire buffer.
    #[must_use]
    pub fn buffer(binding: u32, buffer: GpuBuffer) -> Self {
        Self {
            binding,
            resource: GpuBindingResource::Buffer(GpuBufferBinding::whole(buffer)),
        }
    }

    /// Creates an entry for an explicit buffer range.
    #[must_use]
    pub const fn buffer_range(binding: u32, buffer: GpuBufferBinding) -> Self {
        Self {
            binding,
            resource: GpuBindingResource::Buffer(buffer),
        }
    }

    /// Creates an entry for one sampler.
    #[must_use]
    pub const fn sampler(binding: u32, sampler: GpuSampler) -> Self {
        Self {
            binding,
            resource: GpuBindingResource::Sampler(sampler),
        }
    }

    /// Creates an entry for one texture view.
    #[must_use]
    pub const fn texture_view(binding: u32, view: GpuTextureView) -> Self {
        Self {
            binding,
            resource: GpuBindingResource::TextureView(view),
        }
    }

    /// Creates an entry for any supported managed resource form.
    #[must_use]
    pub const fn new(binding: u32, resource: GpuBindingResource) -> Self {
        Self { binding, resource }
    }

    /// Returns the shader binding number.
    #[must_use]
    pub const fn binding(&self) -> u32 {
        self.binding
    }

    /// Returns the retained managed resource.
    #[must_use]
    pub const fn resource(&self) -> &GpuBindingResource {
        &self.resource
    }
}

/// A managed bind-group creation descriptor.
#[derive(Clone, Copy, Debug)]
pub struct GpuBindGroupDescriptor<'a> {
    /// Debug label forwarded to wgpu.
    pub label: Option<&'a str>,
    /// Explicit managed layout for the entries.
    pub layout: &'a GpuBindGroupLayout,
    /// Owned-resource entries to retain in the resulting group.
    pub entries: &'a [GpuBindGroupEntry],
}

#[derive(Debug)]
struct GpuBindGroupInner {
    lease: ResourceLease,
    raw: wgpu::BindGroup,
    layout: GpuBindGroupLayout,
    entries: Vec<GpuBindGroupEntry>,
}

/// A cloneable bind group that retains its layout and every bound resource.
#[derive(Clone, Debug)]
pub struct GpuBindGroup(Arc<GpuBindGroupInner>);

impl GpuBindGroup {
    /// Returns this bind group's process-local diagnostic identifier.
    #[must_use]
    pub fn id(&self) -> GpuResourceId {
        self.0.lease.id()
    }

    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.0.lease.label()
    }

    /// Returns the retained explicit layout.
    #[must_use]
    pub fn layout(&self) -> &GpuBindGroupLayout {
        &self.0.layout
    }

    /// Returns the retained entries in descriptor order.
    #[must_use]
    pub fn entries(&self) -> &[GpuBindGroupEntry] {
        &self.0.entries
    }

    /// Borrows the raw wgpu bind group for render and compute passes.
    #[must_use]
    pub fn raw(&self) -> &wgpu::BindGroup {
        &self.0.raw
    }
}

impl GpuResources<'_> {
    /// Creates and tracks a sampler in this manager's device lifetime.
    pub fn create_sampler(&self, descriptor: &wgpu::SamplerDescriptor<'_>) -> Result<GpuSampler> {
        validate_sampler(descriptor)?;
        let info = GpuSamplerInfo {
            label: descriptor.label.map(str::to_owned),
            address_mode_u: descriptor.address_mode_u,
            address_mode_v: descriptor.address_mode_v,
            address_mode_w: descriptor.address_mode_w,
            mag_filter: descriptor.mag_filter,
            min_filter: descriptor.min_filter,
            mipmap_filter: descriptor.mipmap_filter,
            lod_min_clamp: descriptor.lod_min_clamp,
            lod_max_clamp: descriptor.lod_max_clamp,
            compare: descriptor.compare,
            anisotropy_clamp: descriptor.anisotropy_clamp,
            border_color: descriptor.border_color,
        };
        let raw = self.wgpu_device().create_sampler(descriptor);
        let lease = self.lease(GpuResourceKind::Sampler, descriptor.label)?;
        Ok(GpuSampler(Arc::new(GpuSamplerInner { lease, raw, info })))
    }

    /// Creates and tracks an explicit bind-group layout.
    pub fn create_bind_group_layout(
        &self,
        descriptor: &wgpu::BindGroupLayoutDescriptor<'_>,
    ) -> Result<GpuBindGroupLayout> {
        unique_bindings(
            descriptor.entries.iter().map(|entry| entry.binding),
            "create_bind_group_layout",
        )?;
        let info = GpuBindGroupLayoutInfo {
            label: descriptor.label.map(str::to_owned),
            entries: descriptor.entries.to_vec(),
        };
        let raw = self.wgpu_device().create_bind_group_layout(descriptor);
        let lease = self.lease(GpuResourceKind::BindGroupLayout, descriptor.label)?;
        Ok(GpuBindGroupLayout(Arc::new(GpuBindGroupLayoutInner {
            lease,
            raw,
            info,
        })))
    }

    /// Creates a bind group and retains all managed dependencies until it drops.
    pub fn create_bind_group(
        &self,
        descriptor: GpuBindGroupDescriptor<'_>,
    ) -> Result<GpuBindGroup> {
        self.ensure_owner(descriptor.layout.lease(), "create_bind_group")?;
        unique_bindings(
            descriptor.entries.iter().map(GpuBindGroupEntry::binding),
            "create_bind_group",
        )?;
        for entry in descriptor.entries {
            self.validate_binding_resource(&entry.resource)?;
        }

        let buffer_arrays = descriptor
            .entries
            .iter()
            .map(|entry| match &entry.resource {
                GpuBindingResource::BufferArray(bindings) => {
                    Some(bindings.iter().map(GpuBufferBinding::raw).collect())
                }
                _ => None,
            })
            .collect::<Vec<Option<Vec<_>>>>();
        let sampler_arrays = descriptor
            .entries
            .iter()
            .map(|entry| match &entry.resource {
                GpuBindingResource::SamplerArray(samplers) => {
                    Some(samplers.iter().map(GpuSampler::raw).collect())
                }
                _ => None,
            })
            .collect::<Vec<Option<Vec<_>>>>();
        let texture_arrays = descriptor
            .entries
            .iter()
            .map(|entry| match &entry.resource {
                GpuBindingResource::TextureViewArray(views) => {
                    Some(views.iter().map(GpuTextureView::raw).collect())
                }
                _ => None,
            })
            .collect::<Vec<Option<Vec<_>>>>();
        let raw_entries = descriptor
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| wgpu::BindGroupEntry {
                binding: entry.binding,
                resource: match &entry.resource {
                    GpuBindingResource::Buffer(binding) => {
                        wgpu::BindingResource::Buffer(binding.raw())
                    }
                    GpuBindingResource::BufferArray(_) => wgpu::BindingResource::BufferArray(
                        buffer_arrays[index]
                            .as_deref()
                            .expect("buffer-array backing is aligned with entries"),
                    ),
                    GpuBindingResource::Sampler(sampler) => {
                        wgpu::BindingResource::Sampler(sampler.raw())
                    }
                    GpuBindingResource::SamplerArray(_) => wgpu::BindingResource::SamplerArray(
                        sampler_arrays[index]
                            .as_deref()
                            .expect("sampler-array backing is aligned with entries"),
                    ),
                    GpuBindingResource::TextureView(view) => {
                        wgpu::BindingResource::TextureView(view.raw())
                    }
                    GpuBindingResource::TextureViewArray(_) => {
                        wgpu::BindingResource::TextureViewArray(
                            texture_arrays[index]
                                .as_deref()
                                .expect("texture-array backing is aligned with entries"),
                        )
                    }
                },
            })
            .collect::<Vec<_>>();
        let raw = self
            .wgpu_device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: descriptor.label,
                layout: descriptor.layout.raw(),
                entries: &raw_entries,
            });
        let lease = self.lease(GpuResourceKind::BindGroup, descriptor.label)?;
        Ok(GpuBindGroup(Arc::new(GpuBindGroupInner {
            lease,
            raw,
            layout: descriptor.layout.clone(),
            entries: descriptor.entries.to_vec(),
        })))
    }

    fn validate_binding_resource(&self, resource: &GpuBindingResource) -> Result<()> {
        match resource {
            GpuBindingResource::Buffer(binding) => self.validate_buffer_binding(binding),
            GpuBindingResource::BufferArray(bindings) => {
                nonempty(bindings, "buffer")?;
                for binding in bindings {
                    self.validate_buffer_binding(binding)?;
                }
                Ok(())
            }
            GpuBindingResource::Sampler(sampler) => {
                self.ensure_owner(sampler.lease(), "create_bind_group")
            }
            GpuBindingResource::SamplerArray(samplers) => {
                nonempty(samplers, "sampler")?;
                for sampler in samplers {
                    self.ensure_owner(sampler.lease(), "create_bind_group")?;
                }
                Ok(())
            }
            GpuBindingResource::TextureView(view) => {
                self.ensure_owner(view.lease(), "create_bind_group")
            }
            GpuBindingResource::TextureViewArray(views) => {
                nonempty(views, "texture-view")?;
                for view in views {
                    self.ensure_owner(view.lease(), "create_bind_group")?;
                }
                Ok(())
            }
        }
    }

    fn validate_buffer_binding(&self, binding: &GpuBufferBinding) -> Result<()> {
        self.ensure_owner(binding.buffer.lease(), "create_bind_group")?;
        let buffer_size = binding.buffer.info().size();
        let end = match binding.size {
            Some(size) => binding.offset.checked_add(size.get()),
            None => Some(buffer_size),
        };
        if binding.offset > buffer_size || end.map_or(true, |end| end > buffer_size) {
            return Err(invalid(
                "create_bind_group",
                "buffer binding range is outside the managed buffer",
            ));
        }
        Ok(())
    }
}

fn validate_sampler(descriptor: &wgpu::SamplerDescriptor<'_>) -> Result<()> {
    if !descriptor.lod_min_clamp.is_finite()
        || !descriptor.lod_max_clamp.is_finite()
        || descriptor.lod_min_clamp > descriptor.lod_max_clamp
    {
        return Err(invalid(
            "create_sampler",
            "sampler level-of-detail clamps must be finite and ordered",
        ));
    }
    if descriptor.anisotropy_clamp == 0 {
        return Err(invalid(
            "create_sampler",
            "sampler anisotropy must be at least one",
        ));
    }
    if descriptor.anisotropy_clamp > 1
        && (descriptor.mag_filter != wgpu::FilterMode::Linear
            || descriptor.min_filter != wgpu::FilterMode::Linear
            || descriptor.mipmap_filter != wgpu::FilterMode::Linear)
    {
        return Err(invalid(
            "create_sampler",
            "anisotropic samplers require linear filtering for every filter",
        ));
    }
    Ok(())
}

fn unique_bindings(bindings: impl Iterator<Item = u32>, operation: &'static str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for binding in bindings {
        if !seen.insert(binding) {
            return Err(invalid(
                operation,
                format!("binding {binding} is declared more than once"),
            ));
        }
    }
    Ok(())
}

fn nonempty<T>(resources: &[T], kind: &'static str) -> Result<()> {
    if resources.is_empty() {
        return Err(invalid(
            "create_bind_group",
            format!("{kind} binding arrays must not be empty"),
        ));
    }
    Ok(())
}
