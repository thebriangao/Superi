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

/// An owned validation snapshot for one render-pipeline vertex-buffer slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuVertexBufferInfo {
    array_stride: wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode,
    last_stride: wgpu::BufferAddress,
}

impl GpuVertexBufferInfo {
    /// Returns the byte stride between consecutive vertex or instance records.
    #[must_use]
    pub const fn array_stride(&self) -> wgpu::BufferAddress {
        self.array_stride
    }

    /// Returns whether records advance per vertex or per instance.
    #[must_use]
    pub const fn step_mode(&self) -> wgpu::VertexStepMode {
        self.step_mode
    }

    /// Returns the minimum bytes needed for the final record in this slot.
    #[must_use]
    pub const fn last_stride(&self) -> wgpu::BufferAddress {
        self.last_stride
    }
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

    pub(crate) fn lease(&self) -> &ResourceLease {
        &self.0.lease
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
    color_target_formats: Vec<Option<wgpu::TextureFormat>>,
    depth_stencil_format: Option<wgpu::TextureFormat>,
    sample_count: u32,
    multiview: Option<std::num::NonZeroU32>,
    vertex_buffers: Vec<GpuVertexBufferInfo>,
    strip_index_format: Option<wgpu::IndexFormat>,
    requires_blend_constant: bool,
    writes_depth: bool,
    writes_stencil: bool,
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

    /// Returns fragment output formats in color-attachment slot order.
    #[must_use]
    pub fn color_target_formats(&self) -> &[Option<wgpu::TextureFormat>] {
        &self.0.color_target_formats
    }

    /// Returns the depth and stencil attachment format required by this pipeline.
    #[must_use]
    pub fn depth_stencil_format(&self) -> Option<wgpu::TextureFormat> {
        self.0.depth_stencil_format
    }

    /// Returns the render pipeline multisample count.
    #[must_use]
    pub fn sample_count(&self) -> u32 {
        self.0.sample_count
    }

    /// Returns the required multiview layer count, when enabled.
    #[must_use]
    pub fn multiview(&self) -> Option<std::num::NonZeroU32> {
        self.0.multiview
    }

    /// Returns validation metadata for each declared vertex-buffer slot.
    #[must_use]
    pub fn vertex_buffers(&self) -> &[GpuVertexBufferInfo] {
        &self.0.vertex_buffers
    }

    /// Returns the number of declared vertex-buffer slots.
    #[must_use]
    pub fn vertex_buffer_count(&self) -> u32 {
        u32::try_from(self.0.vertex_buffers.len()).unwrap_or(u32::MAX)
    }

    /// Returns the index format required by triangle or line strips.
    #[must_use]
    pub fn strip_index_format(&self) -> Option<wgpu::IndexFormat> {
        self.0.strip_index_format
    }

    /// Returns whether a draw requires a previously selected blend constant.
    #[must_use]
    pub fn requires_blend_constant(&self) -> bool {
        self.0.requires_blend_constant
    }

    /// Returns whether this pipeline can write the depth aspect.
    #[must_use]
    pub fn writes_depth(&self) -> bool {
        self.0.writes_depth
    }

    /// Returns whether this pipeline can write the stencil aspect.
    #[must_use]
    pub fn writes_stencil(&self) -> bool {
        self.0.writes_stencil
    }

    pub(crate) fn lease(&self) -> &ResourceLease {
        &self.0.lease
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
        let color_target_formats = descriptor
            .fragment
            .as_ref()
            .map_or_else(Vec::new, |fragment| {
                fragment
                    .targets
                    .iter()
                    .map(|target| target.as_ref().map(|target| target.format))
                    .collect()
            });
        let depth_stencil_format = descriptor.depth_stencil.as_ref().map(|state| state.format);
        let sample_count = descriptor.multisample.count;
        let multiview = descriptor.multiview;
        let vertex_buffers = descriptor
            .vertex
            .buffers
            .iter()
            .map(|buffer| GpuVertexBufferInfo {
                array_stride: buffer.array_stride,
                step_mode: buffer.step_mode,
                last_stride: buffer
                    .attributes
                    .iter()
                    .map(|attribute| attribute.offset + attribute.format.size())
                    .max()
                    .unwrap_or(0),
            })
            .collect();
        let strip_index_format = descriptor.primitive.strip_index_format;
        let requires_blend_constant = descriptor.fragment.as_ref().is_some_and(|fragment| {
            fragment.targets.iter().flatten().any(|target| {
                target
                    .blend
                    .as_ref()
                    .is_some_and(|blend| blend.color.uses_constant() || blend.alpha.uses_constant())
            })
        });
        let writes_depth = descriptor
            .depth_stencil
            .as_ref()
            .is_some_and(|state| !state.is_depth_read_only());
        let writes_stencil = descriptor
            .depth_stencil
            .as_ref()
            .is_some_and(|state| !state.is_stencil_read_only(descriptor.primitive.cull_mode));
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
            color_target_formats,
            depth_stencil_format,
            sample_count,
            multiview,
            vertex_buffers,
            strip_index_format,
            requires_blend_constant,
            writes_depth,
            writes_stencil,
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
