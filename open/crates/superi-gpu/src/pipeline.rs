//! Managed explicit pipeline layouts and wgpu compute and render pipelines.

use std::sync::Arc;

use superi_core::error::Result;

use crate::binding::GpuBindGroupLayout;
use crate::resource::{GpuResourceId, GpuResourceKind, GpuResources, ResourceLease};

/// A managed explicit pipeline-layout creation descriptor.
#[derive(Clone, Copy, Debug)]
pub struct GpuPipelineLayoutDescriptor<'a> {
    /// Debug label forwarded to wgpu.
    pub label: Option<&'a str>,
    /// Bind-group layouts in shader group-number order.
    pub bind_group_layouts: &'a [&'a GpuBindGroupLayout],
    /// Explicit push-constant ranges.
    pub push_constant_ranges: &'a [wgpu::PushConstantRange],
}

/// An owned snapshot of an explicit pipeline layout.
#[derive(Clone, Debug)]
pub struct GpuPipelineLayoutInfo {
    label: Option<String>,
    bind_group_layouts: Vec<GpuBindGroupLayout>,
    push_constant_ranges: Vec<wgpu::PushConstantRange>,
}

impl GpuPipelineLayoutInfo {
    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns retained bind-group layouts in shader group-number order.
    #[must_use]
    pub fn bind_group_layouts(&self) -> &[GpuBindGroupLayout] {
        &self.bind_group_layouts
    }

    /// Returns the explicit push-constant ranges.
    #[must_use]
    pub fn push_constant_ranges(&self) -> &[wgpu::PushConstantRange] {
        &self.push_constant_ranges
    }
}

#[derive(Debug)]
struct GpuPipelineLayoutInner {
    lease: ResourceLease,
    raw: wgpu::PipelineLayout,
    info: GpuPipelineLayoutInfo,
}

/// A cloneable pipeline layout that retains all bind-group layouts.
#[derive(Clone, Debug)]
pub struct GpuPipelineLayout(Arc<GpuPipelineLayoutInner>);

impl GpuPipelineLayout {
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

    /// Returns the owned creation descriptor snapshot.
    #[must_use]
    pub fn info(&self) -> &GpuPipelineLayoutInfo {
        &self.0.info
    }

    /// Borrows the raw wgpu pipeline layout.
    #[must_use]
    pub fn raw(&self) -> &wgpu::PipelineLayout {
        &self.0.raw
    }

    pub(crate) fn lease(&self) -> &ResourceLease {
        &self.0.lease
    }
}

/// A managed compute-pipeline creation descriptor.
#[derive(Clone, Debug)]
pub struct GpuComputePipelineDescriptor<'a> {
    /// Debug label forwarded to wgpu.
    pub label: Option<&'a str>,
    /// Explicit managed layout, or None for wgpu automatic layout derivation.
    pub layout: Option<&'a GpuPipelineLayout>,
    /// Compiled compute shader module.
    pub module: &'a wgpu::ShaderModule,
    /// Compute entry point, or None when the module has exactly one candidate.
    pub entry_point: Option<&'a str>,
    /// Shader compilation constants and zero-initialization policy.
    pub compilation_options: wgpu::PipelineCompilationOptions<'a>,
    /// Optional backend pipeline cache.
    pub cache: Option<&'a wgpu::PipelineCache>,
}

/// A managed render-pipeline creation descriptor.
#[derive(Clone, Debug)]
pub struct GpuRenderPipelineDescriptor<'a> {
    /// Debug label forwarded to wgpu.
    pub label: Option<&'a str>,
    /// Explicit managed layout, or None for wgpu automatic layout derivation.
    pub layout: Option<&'a GpuPipelineLayout>,
    /// Vertex-stage state.
    pub vertex: wgpu::VertexState<'a>,
    /// Primitive assembly and rasterization state.
    pub primitive: wgpu::PrimitiveState,
    /// Optional depth and stencil state.
    pub depth_stencil: Option<wgpu::DepthStencilState>,
    /// Multisample state.
    pub multisample: wgpu::MultisampleState,
    /// Optional fragment-stage state.
    pub fragment: Option<wgpu::FragmentState<'a>>,
    /// Optional multiview layer count.
    pub multiview: Option<std::num::NonZeroU32>,
    /// Optional backend pipeline cache.
    pub cache: Option<&'a wgpu::PipelineCache>,
}

/// Common managed pipeline metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuPipelineInfo {
    label: Option<String>,
    explicit_layout: Option<GpuResourceId>,
}

impl GpuPipelineInfo {
    /// Returns the wgpu debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the retained explicit layout identifier, or None for automatic layout.
    #[must_use]
    pub const fn explicit_layout(&self) -> Option<GpuResourceId> {
        self.explicit_layout
    }
}

#[derive(Debug)]
struct GpuComputePipelineInner {
    lease: ResourceLease,
    raw: wgpu::ComputePipeline,
    layout: Option<GpuPipelineLayout>,
    info: GpuPipelineInfo,
}

/// A cloneable managed compute pipeline.
#[derive(Clone, Debug)]
pub struct GpuComputePipeline(Arc<GpuComputePipelineInner>);

impl GpuComputePipeline {
    /// Returns this pipeline's process-local diagnostic identifier.
    #[must_use]
    pub fn id(&self) -> GpuResourceId {
        self.0.lease.id()
    }

    /// Returns common managed pipeline metadata.
    #[must_use]
    pub fn info(&self) -> &GpuPipelineInfo {
        &self.0.info
    }

    /// Returns the retained explicit layout, or None for an automatic layout.
    #[must_use]
    pub fn layout(&self) -> Option<&GpuPipelineLayout> {
        self.0.layout.as_ref()
    }

    /// Borrows the raw wgpu compute pipeline for compute passes.
    #[must_use]
    pub fn raw(&self) -> &wgpu::ComputePipeline {
        &self.0.raw
    }
}

#[derive(Debug)]
struct GpuRenderPipelineInner {
    lease: ResourceLease,
    raw: wgpu::RenderPipeline,
    layout: Option<GpuPipelineLayout>,
    info: GpuPipelineInfo,
}

/// A cloneable managed render pipeline.
#[derive(Clone, Debug)]
pub struct GpuRenderPipeline(Arc<GpuRenderPipelineInner>);

impl GpuRenderPipeline {
    /// Returns this pipeline's process-local diagnostic identifier.
    #[must_use]
    pub fn id(&self) -> GpuResourceId {
        self.0.lease.id()
    }

    /// Returns common managed pipeline metadata.
    #[must_use]
    pub fn info(&self) -> &GpuPipelineInfo {
        &self.0.info
    }

    /// Returns the retained explicit layout, or None for an automatic layout.
    #[must_use]
    pub fn layout(&self) -> Option<&GpuPipelineLayout> {
        self.0.layout.as_ref()
    }

    /// Borrows the raw wgpu render pipeline for render passes.
    #[must_use]
    pub fn raw(&self) -> &wgpu::RenderPipeline {
        &self.0.raw
    }
}

impl GpuResources<'_> {
    /// Creates an explicit pipeline layout and retains all group layouts.
    pub fn create_pipeline_layout(
        &self,
        descriptor: GpuPipelineLayoutDescriptor<'_>,
    ) -> Result<GpuPipelineLayout> {
        for layout in descriptor.bind_group_layouts {
            self.ensure_owner(layout.lease(), "create_pipeline_layout")?;
        }
        let raw_layouts = descriptor
            .bind_group_layouts
            .iter()
            .map(|layout| layout.raw())
            .collect::<Vec<_>>();
        let raw = self
            .wgpu_device()
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: descriptor.label,
                bind_group_layouts: &raw_layouts,
                push_constant_ranges: descriptor.push_constant_ranges,
            });
        let lease = self.lease(GpuResourceKind::PipelineLayout, descriptor.label)?;
        Ok(GpuPipelineLayout(Arc::new(GpuPipelineLayoutInner {
            lease,
            raw,
            info: GpuPipelineLayoutInfo {
                label: descriptor.label.map(str::to_owned),
                bind_group_layouts: descriptor
                    .bind_group_layouts
                    .iter()
                    .map(|layout| (*layout).clone())
                    .collect(),
                push_constant_ranges: descriptor.push_constant_ranges.to_vec(),
            },
        })))
    }

    /// Creates a compute pipeline and retains its explicit layout when supplied.
    pub fn create_compute_pipeline(
        &self,
        descriptor: GpuComputePipelineDescriptor<'_>,
    ) -> Result<GpuComputePipeline> {
        if let Some(layout) = descriptor.layout {
            self.ensure_owner(layout.lease(), "create_compute_pipeline")?;
        }
        let raw = self
            .wgpu_device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: descriptor.label,
                layout: descriptor.layout.map(GpuPipelineLayout::raw),
                module: descriptor.module,
                entry_point: descriptor.entry_point,
                compilation_options: descriptor.compilation_options,
                cache: descriptor.cache,
            });
        let lease = self.lease(GpuResourceKind::ComputePipeline, descriptor.label)?;
        Ok(GpuComputePipeline(Arc::new(GpuComputePipelineInner {
            lease,
            raw,
            layout: descriptor.layout.cloned(),
            info: GpuPipelineInfo {
                label: descriptor.label.map(str::to_owned),
                explicit_layout: descriptor.layout.map(GpuPipelineLayout::id),
            },
        })))
    }

    /// Creates a render pipeline and retains its explicit layout when supplied.
    pub fn create_render_pipeline(
        &self,
        descriptor: GpuRenderPipelineDescriptor<'_>,
    ) -> Result<GpuRenderPipeline> {
        if let Some(layout) = descriptor.layout {
            self.ensure_owner(layout.lease(), "create_render_pipeline")?;
        }
        let raw = self
            .wgpu_device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: descriptor.label,
                layout: descriptor.layout.map(GpuPipelineLayout::raw),
                vertex: descriptor.vertex,
                primitive: descriptor.primitive,
                depth_stencil: descriptor.depth_stencil,
                multisample: descriptor.multisample,
                fragment: descriptor.fragment,
                multiview: descriptor.multiview,
                cache: descriptor.cache,
            });
        let lease = self.lease(GpuResourceKind::RenderPipeline, descriptor.label)?;
        Ok(GpuRenderPipeline(Arc::new(GpuRenderPipelineInner {
            lease,
            raw,
            layout: descriptor.layout.cloned(),
            info: GpuPipelineInfo {
                label: descriptor.label.map(str::to_owned),
                explicit_layout: descriptor.layout.map(GpuPipelineLayout::id),
            },
        })))
    }
}
