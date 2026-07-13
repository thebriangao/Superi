//! Managed explicit pipeline layouts and wgpu compute and render pipelines.

use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, Recoverability, Result};

use crate::binding::GpuBindGroupLayout;
use crate::resource::{GpuResourceId, GpuResourceKind, GpuResources, ResourceLease};
use crate::shader::{shader_context, wgpu_error, GpuShaderModule, ShaderStage};

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
    pub module: &'a GpuShaderModule,
    /// Exact compute entry point.
    pub entry_point: &'a str,
    /// Shader compilation constants and zero-initialization policy.
    pub compilation_options: wgpu::PipelineCompilationOptions<'a>,
    /// Optional backend pipeline cache.
    pub cache: Option<&'a wgpu::PipelineCache>,
}

/// Managed vertex-stage state.
#[derive(Clone, Debug)]
pub struct GpuVertexState<'a> {
    /// Validated vertex shader module.
    pub module: &'a GpuShaderModule,
    /// Exact vertex entry point.
    pub entry_point: &'a str,
    /// Shader compilation constants and zero-initialization policy.
    pub compilation_options: wgpu::PipelineCompilationOptions<'a>,
    /// Vertex-buffer layouts.
    pub buffers: &'a [wgpu::VertexBufferLayout<'a>],
}

/// Managed fragment-stage state.
#[derive(Clone, Debug)]
pub struct GpuFragmentState<'a> {
    /// Validated fragment shader module.
    pub module: &'a GpuShaderModule,
    /// Exact fragment entry point.
    pub entry_point: &'a str,
    /// Shader compilation constants and zero-initialization policy.
    pub compilation_options: wgpu::PipelineCompilationOptions<'a>,
    /// Color-target states by fragment output location.
    pub targets: &'a [Option<wgpu::ColorTargetState>],
}

/// A managed render-pipeline creation descriptor.
#[derive(Clone, Debug)]
pub struct GpuRenderPipelineDescriptor<'a> {
    /// Debug label forwarded to wgpu.
    pub label: Option<&'a str>,
    /// Explicit managed layout, or None for wgpu automatic layout derivation.
    pub layout: Option<&'a GpuPipelineLayout>,
    /// Vertex-stage state.
    pub vertex: GpuVertexState<'a>,
    /// Primitive assembly and rasterization state.
    pub primitive: wgpu::PrimitiveState,
    /// Optional depth and stencil state.
    pub depth_stencil: Option<wgpu::DepthStencilState>,
    /// Multisample state.
    pub multisample: wgpu::MultisampleState,
    /// Optional fragment-stage state.
    pub fragment: Option<GpuFragmentState<'a>>,
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
    module: GpuShaderModule,
    entry_point: String,
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

    /// Returns the retained compute shader module.
    #[must_use]
    pub fn module(&self) -> &GpuShaderModule {
        &self.0.module
    }

    /// Returns the exact compute entry point.
    #[must_use]
    pub fn entry_point(&self) -> &str {
        &self.0.entry_point
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
    vertex_module: GpuShaderModule,
    vertex_entry_point: String,
    fragment_module: Option<GpuShaderModule>,
    fragment_entry_point: Option<String>,
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

    /// Returns the retained vertex shader module.
    #[must_use]
    pub fn vertex_module(&self) -> &GpuShaderModule {
        &self.0.vertex_module
    }

    /// Returns the exact vertex entry point.
    #[must_use]
    pub fn vertex_entry_point(&self) -> &str {
        &self.0.vertex_entry_point
    }

    /// Returns the retained fragment shader module, when present.
    #[must_use]
    pub fn fragment_module(&self) -> Option<&GpuShaderModule> {
        self.0.fragment_module.as_ref()
    }

    /// Returns the exact fragment entry point, when present.
    #[must_use]
    pub fn fragment_entry_point(&self) -> Option<&str> {
        self.0.fragment_entry_point.as_deref()
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
    pub async fn create_compute_pipeline(
        &self,
        descriptor: GpuComputePipelineDescriptor<'_>,
    ) -> Result<GpuComputePipeline> {
        if let Some(layout) = descriptor.layout {
            self.ensure_owner(layout.lease(), "create_compute_pipeline")?;
        }
        self.ensure_owner(descriptor.module.lease(), "create_compute_pipeline")?;
        ensure_stage(
            descriptor.module,
            descriptor.entry_point,
            ShaderStage::Compute,
            "create_compute_pipeline",
        )?;
        let _scope_guard = self.device().lock_error_scopes().await;
        push_pipeline_error_scopes(self.wgpu_device());
        let raw = self
            .wgpu_device()
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: descriptor.label,
                layout: descriptor.layout.map(GpuPipelineLayout::raw),
                module: descriptor.module.raw(),
                entry_point: Some(descriptor.entry_point),
                compilation_options: descriptor.compilation_options,
                cache: descriptor.cache,
            });
        if let Some(error) = pop_pipeline_error_scopes(self.wgpu_device()).await {
            return Err(wgpu_error(
                error,
                "create_compute_pipeline",
                descriptor.module.info().source_digest(),
                descriptor.label.or(descriptor.module.info().label()),
            ));
        }
        let lease = self.lease(GpuResourceKind::ComputePipeline, descriptor.label)?;
        Ok(GpuComputePipeline(Arc::new(GpuComputePipelineInner {
            lease,
            raw,
            layout: descriptor.layout.cloned(),
            module: descriptor.module.clone(),
            entry_point: descriptor.entry_point.to_owned(),
            info: GpuPipelineInfo {
                label: descriptor.label.map(str::to_owned),
                explicit_layout: descriptor.layout.map(GpuPipelineLayout::id),
            },
        })))
    }

    /// Creates a render pipeline and retains its explicit layout when supplied.
    pub async fn create_render_pipeline(
        &self,
        descriptor: GpuRenderPipelineDescriptor<'_>,
    ) -> Result<GpuRenderPipeline> {
        if let Some(layout) = descriptor.layout {
            self.ensure_owner(layout.lease(), "create_render_pipeline")?;
        }
        self.ensure_owner(descriptor.vertex.module.lease(), "create_render_pipeline")?;
        ensure_stage(
            descriptor.vertex.module,
            descriptor.vertex.entry_point,
            ShaderStage::Vertex,
            "create_render_pipeline",
        )?;
        if let Some(fragment) = descriptor.fragment.as_ref() {
            self.ensure_owner(fragment.module.lease(), "create_render_pipeline")?;
            ensure_stage(
                fragment.module,
                fragment.entry_point,
                ShaderStage::Fragment,
                "create_render_pipeline",
            )?;
        }
        let vertex = wgpu::VertexState {
            module: descriptor.vertex.module.raw(),
            entry_point: Some(descriptor.vertex.entry_point),
            compilation_options: descriptor.vertex.compilation_options.clone(),
            buffers: descriptor.vertex.buffers,
        };
        let fragment = descriptor
            .fragment
            .as_ref()
            .map(|stage| wgpu::FragmentState {
                module: stage.module.raw(),
                entry_point: Some(stage.entry_point),
                compilation_options: stage.compilation_options.clone(),
                targets: stage.targets,
            });
        let _scope_guard = self.device().lock_error_scopes().await;
        push_pipeline_error_scopes(self.wgpu_device());
        let raw = self
            .wgpu_device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: descriptor.label,
                layout: descriptor.layout.map(GpuPipelineLayout::raw),
                vertex,
                primitive: descriptor.primitive,
                depth_stencil: descriptor.depth_stencil,
                multisample: descriptor.multisample,
                fragment,
                multiview: descriptor.multiview,
                cache: descriptor.cache,
            });
        if let Some(error) = pop_pipeline_error_scopes(self.wgpu_device()).await {
            return Err(wgpu_error(
                error,
                "create_render_pipeline",
                descriptor.vertex.module.info().source_digest(),
                descriptor.label.or(descriptor.vertex.module.info().label()),
            ));
        }
        let lease = self.lease(GpuResourceKind::RenderPipeline, descriptor.label)?;
        Ok(GpuRenderPipeline(Arc::new(GpuRenderPipelineInner {
            lease,
            raw,
            layout: descriptor.layout.cloned(),
            vertex_module: descriptor.vertex.module.clone(),
            vertex_entry_point: descriptor.vertex.entry_point.to_owned(),
            fragment_module: descriptor
                .fragment
                .as_ref()
                .map(|stage| stage.module.clone()),
            fragment_entry_point: descriptor
                .fragment
                .as_ref()
                .map(|stage| stage.entry_point.to_owned()),
            info: GpuPipelineInfo {
                label: descriptor.label.map(str::to_owned),
                explicit_layout: descriptor.layout.map(GpuPipelineLayout::id),
            },
        })))
    }
}

fn ensure_stage(
    module: &GpuShaderModule,
    entry_point: &str,
    expected: ShaderStage,
    operation: &'static str,
) -> Result<()> {
    if module
        .reflection()
        .entry_point(entry_point, expected)
        .is_some()
    {
        return Ok(());
    }
    let expected_stage = match expected {
        ShaderStage::Vertex => "vertex",
        ShaderStage::Fragment => "fragment",
        ShaderStage::Compute => "compute",
    };
    let mut context = shader_context(
        operation,
        module.info().source_digest(),
        module.info().label(),
    );
    context.insert_field("entry_point", entry_point);
    context.insert_field("expected_stage", expected_stage);
    Err(Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        format!("shader entry point '{entry_point}' is not a {expected_stage} entry point"),
    )
    .with_context(context))
}

fn push_pipeline_error_scopes(device: &wgpu::Device) {
    device.push_error_scope(wgpu::ErrorFilter::Internal);
    device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    device.push_error_scope(wgpu::ErrorFilter::Validation);
}

async fn pop_pipeline_error_scopes(device: &wgpu::Device) -> Option<wgpu::Error> {
    let validation_error = device.pop_error_scope().await;
    let memory_error = device.pop_error_scope().await;
    let internal_error = device.pop_error_scope().await;
    validation_error.or(memory_error).or(internal_error)
}
