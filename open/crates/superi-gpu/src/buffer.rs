//! Managed GPU buffers with owned descriptors and device-lifetime identity.

use std::sync::Arc;

use superi_core::error::Result;

use crate::resource::{invalid, GpuResourceId, GpuResourceKind, GpuResources, ResourceLease};

/// An owned snapshot of the descriptor used to create a managed buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuBufferInfo {
    label: Option<String>,
    size: wgpu::BufferAddress,
    usage: wgpu::BufferUsages,
    mapped_at_creation: bool,
}

impl GpuBufferInfo {
    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the allocation size in bytes.
    #[must_use]
    pub const fn size(&self) -> wgpu::BufferAddress {
        self.size
    }

    /// Returns every permitted buffer usage.
    #[must_use]
    pub const fn usage(&self) -> wgpu::BufferUsages {
        self.usage
    }

    /// Returns whether the buffer was mapped when created.
    #[must_use]
    pub const fn mapped_at_creation(&self) -> bool {
        self.mapped_at_creation
    }
}

#[derive(Debug)]
struct GpuBufferInner {
    lease: ResourceLease,
    raw: wgpu::Buffer,
    info: GpuBufferInfo,
}

/// A cloneable, device-scoped owner for one wgpu buffer.
#[derive(Clone, Debug)]
pub struct GpuBuffer(Arc<GpuBufferInner>);

impl GpuBuffer {
    /// Returns this buffer's process-local diagnostic identifier.
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
    pub fn info(&self) -> &GpuBufferInfo {
        &self.0.info
    }

    /// Borrows the raw wgpu buffer for command encoding and queue operations.
    #[must_use]
    pub fn raw(&self) -> &wgpu::Buffer {
        &self.0.raw
    }

    pub(crate) fn lease(&self) -> &ResourceLease {
        &self.0.lease
    }
}

impl GpuResources<'_> {
    /// Creates and tracks a buffer in this manager's device lifetime.
    pub fn create_buffer(&self, descriptor: &wgpu::BufferDescriptor<'_>) -> Result<GpuBuffer> {
        if descriptor.usage.is_empty() {
            return Err(invalid(
                "create_buffer",
                "GPU buffer usage must not be empty",
            ));
        }
        if descriptor.mapped_at_creation && descriptor.size % wgpu::COPY_BUFFER_ALIGNMENT != 0 {
            return Err(invalid(
                "create_buffer",
                "a buffer mapped at creation must have a size aligned to COPY_BUFFER_ALIGNMENT",
            ));
        }

        let info = GpuBufferInfo {
            label: descriptor.label.map(str::to_owned),
            size: descriptor.size,
            usage: descriptor.usage,
            mapped_at_creation: descriptor.mapped_at_creation,
        };
        let raw = self.wgpu_device().create_buffer(descriptor);
        let lease = self.lease(GpuResourceKind::Buffer, descriptor.label)?;
        Ok(GpuBuffer(Arc::new(GpuBufferInner { lease, raw, info })))
    }
}
