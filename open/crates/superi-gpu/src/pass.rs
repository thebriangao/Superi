//! Managed compute and render pass orchestration.
//!
//! Pass plans own every managed pipeline, binding, buffer, and attachment they
//! reference. [`GpuPassEncoder`] validates an entire plan before touching wgpu,
//! records accepted plans into one command encoder in call order, and finishes
//! into a single-use [`GpuPassBatch`]. Submission remains on [`GpuDevice`], so
//! callers cannot bypass device-lifetime ownership or create a competing queue.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::ops::Range;
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::binding::{GpuBindGroup, GpuBindingResource};
use crate::buffer::GpuBuffer;
use crate::device::GpuDevice;
use crate::pipeline::{
    GpuComputePipeline, GpuPipelineLayout, GpuRenderPipeline, GpuVertexBufferInfo,
};
use crate::resource::{GpuResourceId, GpuResources};
use crate::texture::GpuTextureView;

const COMPONENT: &str = "superi-gpu.pass";
const DISPATCH_INDIRECT_BYTES: wgpu::BufferAddress = 12;
const DRAW_INDIRECT_BYTES: wgpu::BufferAddress = 16;
const DRAW_INDEXED_INDIRECT_BYTES: wgpu::BufferAddress = 20;

/// Immutable pass capabilities for one acquired device and resource scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuPassCapabilities {
    features: wgpu::Features,
    limits: wgpu::Limits,
    resource_scope: u64,
}

impl GpuPassCapabilities {
    /// Returns the optional features enabled on the logical device.
    #[must_use]
    pub const fn features(&self) -> wgpu::Features {
        self.features
    }

    /// Returns the exact limits enabled on the logical device.
    #[must_use]
    pub const fn limits(&self) -> &wgpu::Limits {
        &self.limits
    }

    /// Returns the managed resource scope accepted by this encoder.
    #[must_use]
    pub const fn resource_scope(&self) -> u64 {
        self.resource_scope
    }

    /// Returns whether all requested optional features are enabled.
    #[must_use]
    pub fn supports(&self, features: wgpu::Features) -> bool {
        self.features.contains(features)
    }
}

/// The concrete kind of one encoded pass.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum GpuPassKind {
    /// A compute pass.
    Compute,
    /// A render pass.
    Render,
}

/// Stable metadata for one pass within a batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuPassInfo {
    sequence: u64,
    kind: GpuPassKind,
    label: Option<String>,
}

impl GpuPassInfo {
    /// Returns the zero-based pass order within the command buffer.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns whether this is a compute or render pass.
    #[must_use]
    pub const fn kind(&self) -> GpuPassKind {
        self.kind
    }

    /// Returns the owned diagnostic label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

/// One owned command in a compute pass plan.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum GpuComputePassCommand {
    /// Selects the active managed compute pipeline.
    SetPipeline(GpuComputePipeline),
    /// Binds one retained managed bind group.
    SetBindGroup {
        /// Bind-group index in the active pipeline layout.
        index: u32,
        /// Managed bind group retained by the plan.
        bind_group: GpuBindGroup,
        /// Dynamic buffer offsets in binding-number order.
        dynamic_offsets: Vec<wgpu::DynamicOffset>,
    },
    /// Writes push-constant bytes for the compute stage.
    SetPushConstants {
        /// Byte offset in push-constant storage.
        offset: u32,
        /// Owned bytes, aligned as required by wgpu.
        data: Vec<u8>,
    },
    /// Dispatches direct compute workgroups.
    Dispatch {
        /// Workgroup count in X.
        x: u32,
        /// Workgroup count in Y.
        y: u32,
        /// Workgroup count in Z.
        z: u32,
    },
    /// Dispatches using one managed indirect-argument buffer.
    DispatchIndirect {
        /// Buffer with [`wgpu::BufferUsages::INDIRECT`].
        buffer: GpuBuffer,
        /// Aligned byte offset of the dispatch arguments.
        offset: wgpu::BufferAddress,
    },
}

/// An inspectable, owned compute pass before encoding.
#[derive(Clone, Debug)]
pub struct GpuComputePassPlan {
    label: Option<String>,
    required_features: wgpu::Features,
    commands: Vec<GpuComputePassCommand>,
}

impl GpuComputePassPlan {
    /// Creates an empty compute plan with an owned diagnostic label.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            required_features: wgpu::Features::empty(),
            commands: Vec::new(),
        }
    }

    /// Creates an unlabeled empty compute plan.
    #[must_use]
    pub const fn unlabeled() -> Self {
        Self {
            label: None,
            required_features: wgpu::Features::empty(),
            commands: Vec::new(),
        }
    }

    /// Adds optional feature requirements checked before encoding.
    #[must_use]
    pub const fn with_required_features(mut self, features: wgpu::Features) -> Self {
        self.required_features = features;
        self
    }

    /// Appends one command in execution order.
    pub fn push_command(&mut self, command: GpuComputePassCommand) {
        self.commands.push(command);
    }

    /// Returns the diagnostic label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the optional feature set required by this plan.
    #[must_use]
    pub const fn required_features(&self) -> wgpu::Features {
        self.required_features
    }

    /// Returns commands in exact execution order.
    #[must_use]
    pub fn commands(&self) -> &[GpuComputePassCommand] {
        &self.commands
    }
}

/// One retained color attachment and its load and store behavior.
#[derive(Clone, Debug)]
pub struct GpuColorAttachment {
    view: GpuTextureView,
    resolve_target: Option<GpuTextureView>,
    operations: wgpu::Operations<wgpu::Color>,
}

impl GpuColorAttachment {
    /// Creates a color attachment without a multisample resolve target.
    #[must_use]
    pub const fn new(view: GpuTextureView, operations: wgpu::Operations<wgpu::Color>) -> Self {
        Self {
            view,
            resolve_target: None,
            operations,
        }
    }

    /// Adds a retained single-sample resolve target.
    #[must_use]
    pub fn with_resolve_target(mut self, target: GpuTextureView) -> Self {
        self.resolve_target = Some(target);
        self
    }

    /// Returns the rendered texture view.
    #[must_use]
    pub const fn view(&self) -> &GpuTextureView {
        &self.view
    }

    /// Returns the multisample resolve target, when present.
    #[must_use]
    pub const fn resolve_target(&self) -> Option<&GpuTextureView> {
        self.resolve_target.as_ref()
    }

    /// Returns the color load and store operations.
    #[must_use]
    pub const fn operations(&self) -> wgpu::Operations<wgpu::Color> {
        self.operations
    }
}

/// One retained depth and stencil attachment.
#[derive(Clone, Debug)]
pub struct GpuDepthStencilAttachment {
    view: GpuTextureView,
    depth_operations: Option<wgpu::Operations<f32>>,
    stencil_operations: Option<wgpu::Operations<u32>>,
}

impl GpuDepthStencilAttachment {
    /// Creates an attachment with no depth or stencil operations enabled.
    #[must_use]
    pub const fn new(view: GpuTextureView) -> Self {
        Self {
            view,
            depth_operations: None,
            stencil_operations: None,
        }
    }

    /// Sets depth load and store behavior.
    #[must_use]
    pub const fn with_depth_operations(mut self, operations: wgpu::Operations<f32>) -> Self {
        self.depth_operations = Some(operations);
        self
    }

    /// Sets stencil load and store behavior.
    #[must_use]
    pub const fn with_stencil_operations(mut self, operations: wgpu::Operations<u32>) -> Self {
        self.stencil_operations = Some(operations);
        self
    }

    /// Returns the depth and stencil texture view.
    #[must_use]
    pub const fn view(&self) -> &GpuTextureView {
        &self.view
    }

    /// Returns depth operations, when enabled.
    #[must_use]
    pub const fn depth_operations(&self) -> Option<wgpu::Operations<f32>> {
        self.depth_operations
    }

    /// Returns stencil operations, when enabled.
    #[must_use]
    pub const fn stencil_operations(&self) -> Option<wgpu::Operations<u32>> {
        self.stencil_operations
    }
}

/// One owned command in a render pass plan.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum GpuRenderPassCommand {
    /// Selects the active managed render pipeline.
    SetPipeline(GpuRenderPipeline),
    /// Binds one retained managed bind group.
    SetBindGroup {
        /// Bind-group index in the active pipeline layout.
        index: u32,
        /// Managed bind group retained by the plan.
        bind_group: GpuBindGroup,
        /// Dynamic buffer offsets in binding-number order.
        dynamic_offsets: Vec<wgpu::DynamicOffset>,
    },
    /// Selects one retained vertex buffer slice.
    SetVertexBuffer {
        /// Vertex buffer slot.
        slot: u32,
        /// Managed buffer retained by the plan.
        buffer: GpuBuffer,
        /// Half-open byte range within the buffer.
        range: Range<wgpu::BufferAddress>,
    },
    /// Selects one retained index buffer slice.
    SetIndexBuffer {
        /// Managed buffer retained by the plan.
        buffer: GpuBuffer,
        /// Half-open byte range within the buffer.
        range: Range<wgpu::BufferAddress>,
        /// Index element format.
        format: wgpu::IndexFormat,
    },
    /// Sets the floating-point viewport.
    SetViewport {
        /// Left coordinate in pixels.
        x: f32,
        /// Top coordinate in pixels.
        y: f32,
        /// Viewport width in pixels.
        width: f32,
        /// Viewport height in pixels.
        height: f32,
        /// Minimum depth value.
        min_depth: f32,
        /// Maximum depth value.
        max_depth: f32,
    },
    /// Sets the integer scissor rectangle.
    SetScissorRect {
        /// Left coordinate in pixels.
        x: u32,
        /// Top coordinate in pixels.
        y: u32,
        /// Scissor width in pixels.
        width: u32,
        /// Scissor height in pixels.
        height: u32,
    },
    /// Sets the blend constant.
    SetBlendConstant(wgpu::Color),
    /// Sets the stencil reference value.
    SetStencilReference(u32),
    /// Writes push-constant bytes for selected render stages.
    SetPushConstants {
        /// Vertex and fragment stages receiving the bytes.
        stages: wgpu::ShaderStages,
        /// Byte offset in push-constant storage.
        offset: u32,
        /// Owned bytes, aligned as required by wgpu.
        data: Vec<u8>,
    },
    /// Draws non-indexed geometry.
    Draw {
        /// Vertex range.
        vertices: Range<u32>,
        /// Instance range.
        instances: Range<u32>,
    },
    /// Draws indexed geometry.
    DrawIndexed {
        /// Index range.
        indices: Range<u32>,
        /// Value added to each index before vertex lookup.
        base_vertex: i32,
        /// Instance range.
        instances: Range<u32>,
    },
    /// Draws using one managed indirect-argument buffer.
    DrawIndirect {
        /// Buffer with [`wgpu::BufferUsages::INDIRECT`].
        buffer: GpuBuffer,
        /// Aligned byte offset of the draw arguments.
        offset: wgpu::BufferAddress,
    },
    /// Draws indexed geometry using one managed indirect-argument buffer.
    DrawIndexedIndirect {
        /// Buffer with [`wgpu::BufferUsages::INDIRECT`].
        buffer: GpuBuffer,
        /// Aligned byte offset of the indexed draw arguments.
        offset: wgpu::BufferAddress,
    },
}

/// An inspectable, owned render pass before encoding.
#[derive(Clone, Debug)]
pub struct GpuRenderPassPlan {
    label: Option<String>,
    required_features: wgpu::Features,
    color_attachments: Vec<Option<GpuColorAttachment>>,
    depth_stencil_attachment: Option<GpuDepthStencilAttachment>,
    commands: Vec<GpuRenderPassCommand>,
}

impl GpuRenderPassPlan {
    /// Creates an empty render plan with an owned diagnostic label.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            required_features: wgpu::Features::empty(),
            color_attachments: Vec::new(),
            depth_stencil_attachment: None,
            commands: Vec::new(),
        }
    }

    /// Creates an unlabeled empty render plan.
    #[must_use]
    pub const fn unlabeled() -> Self {
        Self {
            label: None,
            required_features: wgpu::Features::empty(),
            color_attachments: Vec::new(),
            depth_stencil_attachment: None,
            commands: Vec::new(),
        }
    }

    /// Adds optional feature requirements checked before encoding.
    #[must_use]
    pub const fn with_required_features(mut self, features: wgpu::Features) -> Self {
        self.required_features = features;
        self
    }

    /// Appends one occupied color-attachment slot.
    pub fn push_color_attachment(&mut self, attachment: GpuColorAttachment) {
        self.color_attachments.push(Some(attachment));
    }

    /// Appends an intentionally empty color-attachment slot.
    pub fn push_color_attachment_hole(&mut self) {
        self.color_attachments.push(None);
    }

    /// Sets or replaces the depth and stencil attachment.
    pub fn set_depth_stencil_attachment(&mut self, attachment: GpuDepthStencilAttachment) {
        self.depth_stencil_attachment = Some(attachment);
    }

    /// Appends one command in execution order.
    pub fn push_command(&mut self, command: GpuRenderPassCommand) {
        self.commands.push(command);
    }

    /// Returns the diagnostic label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the optional feature set required by this plan.
    #[must_use]
    pub const fn required_features(&self) -> wgpu::Features {
        self.required_features
    }

    /// Returns color attachments in exact slot order, including holes.
    #[must_use]
    pub fn color_attachments(&self) -> &[Option<GpuColorAttachment>] {
        &self.color_attachments
    }

    /// Returns the depth and stencil attachment, when present.
    #[must_use]
    pub const fn depth_stencil_attachment(&self) -> Option<&GpuDepthStencilAttachment> {
        self.depth_stencil_attachment.as_ref()
    }

    /// Returns commands in exact execution order.
    #[must_use]
    pub fn commands(&self) -> &[GpuRenderPassCommand] {
        &self.commands
    }
}

#[derive(Debug)]
enum RetainedPass {
    Compute { _plan: GpuComputePassPlan },
    Render { _plan: GpuRenderPassPlan },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RenderViewMetadata {
    extent: wgpu::Extent3d,
    samples: u32,
    multiview: Option<NonZeroU32>,
    base_mip_level: u32,
    base_array_layer: u32,
    array_layer_count: u32,
}

#[derive(Clone, Copy, Debug)]
struct BoundVertexBuffer {
    bytes: wgpu::BufferAddress,
}

#[derive(Clone, Copy, Debug)]
struct BoundIndexBuffer {
    entries: u64,
    format: wgpu::IndexFormat,
}

/// A command encoder dedicated to one ordered batch of managed passes.
#[derive(Debug)]
pub struct GpuPassEncoder<'device> {
    resources: GpuResources<'device>,
    capabilities: GpuPassCapabilities,
    encoder: wgpu::CommandEncoder,
    encoded: Vec<(GpuPassInfo, RetainedPass)>,
}

impl<'device> GpuPassEncoder<'device> {
    /// Returns the immutable device and resource capabilities for this encoder.
    #[must_use]
    pub const fn capabilities(&self) -> &GpuPassCapabilities {
        &self.capabilities
    }

    /// Returns the number of successfully encoded passes.
    #[must_use]
    pub fn pass_count(&self) -> usize {
        self.encoded.len()
    }

    /// Validates and records one complete compute pass.
    pub fn encode_compute(&mut self, plan: GpuComputePassPlan) -> Result<GpuPassInfo> {
        self.validate_compute(&plan)?;
        {
            let mut pass = self
                .encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: plan.label(),
                    timestamp_writes: None,
                });
            for command in plan.commands() {
                match command {
                    GpuComputePassCommand::SetPipeline(pipeline) => {
                        pass.set_pipeline(pipeline.raw());
                    }
                    GpuComputePassCommand::SetBindGroup {
                        index,
                        bind_group,
                        dynamic_offsets,
                    } => pass.set_bind_group(*index, bind_group.raw(), dynamic_offsets),
                    GpuComputePassCommand::SetPushConstants { offset, data } => {
                        pass.set_push_constants(*offset, data);
                    }
                    GpuComputePassCommand::Dispatch { x, y, z } => {
                        pass.dispatch_workgroups(*x, *y, *z);
                    }
                    GpuComputePassCommand::DispatchIndirect { buffer, offset } => {
                        pass.dispatch_workgroups_indirect(buffer.raw(), *offset);
                    }
                }
            }
        }

        let info = self.next_info(GpuPassKind::Compute, plan.label())?;
        self.encoded
            .push((info.clone(), RetainedPass::Compute { _plan: plan }));
        Ok(info)
    }

    /// Validates and records one complete render pass.
    pub fn encode_render(&mut self, plan: GpuRenderPassPlan) -> Result<GpuPassInfo> {
        self.validate_render(&plan)?;
        let color_attachments = plan
            .color_attachments()
            .iter()
            .map(|attachment| {
                attachment
                    .as_ref()
                    .map(|attachment| wgpu::RenderPassColorAttachment {
                        view: attachment.view().raw(),
                        resolve_target: attachment.resolve_target().map(GpuTextureView::raw),
                        ops: attachment.operations(),
                    })
            })
            .collect::<Vec<_>>();
        let depth_stencil_attachment = plan.depth_stencil_attachment().map(|attachment| {
            wgpu::RenderPassDepthStencilAttachment {
                view: attachment.view().raw(),
                depth_ops: attachment.depth_operations(),
                stencil_ops: attachment.stencil_operations(),
            }
        });
        {
            let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: plan.label(),
                color_attachments: &color_attachments,
                depth_stencil_attachment,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            for command in plan.commands() {
                match command {
                    GpuRenderPassCommand::SetPipeline(pipeline) => {
                        pass.set_pipeline(pipeline.raw());
                    }
                    GpuRenderPassCommand::SetBindGroup {
                        index,
                        bind_group,
                        dynamic_offsets,
                    } => pass.set_bind_group(*index, bind_group.raw(), dynamic_offsets),
                    GpuRenderPassCommand::SetVertexBuffer {
                        slot,
                        buffer,
                        range,
                    } => pass.set_vertex_buffer(*slot, buffer.raw().slice(range.clone())),
                    GpuRenderPassCommand::SetIndexBuffer {
                        buffer,
                        range,
                        format,
                    } => pass.set_index_buffer(buffer.raw().slice(range.clone()), *format),
                    GpuRenderPassCommand::SetViewport {
                        x,
                        y,
                        width,
                        height,
                        min_depth,
                        max_depth,
                    } => pass.set_viewport(*x, *y, *width, *height, *min_depth, *max_depth),
                    GpuRenderPassCommand::SetScissorRect {
                        x,
                        y,
                        width,
                        height,
                    } => pass.set_scissor_rect(*x, *y, *width, *height),
                    GpuRenderPassCommand::SetBlendConstant(color) => {
                        pass.set_blend_constant(*color);
                    }
                    GpuRenderPassCommand::SetStencilReference(reference) => {
                        pass.set_stencil_reference(*reference);
                    }
                    GpuRenderPassCommand::SetPushConstants {
                        stages,
                        offset,
                        data,
                    } => pass.set_push_constants(*stages, *offset, data),
                    GpuRenderPassCommand::Draw {
                        vertices,
                        instances,
                    } => pass.draw(vertices.clone(), instances.clone()),
                    GpuRenderPassCommand::DrawIndexed {
                        indices,
                        base_vertex,
                        instances,
                    } => pass.draw_indexed(indices.clone(), *base_vertex, instances.clone()),
                    GpuRenderPassCommand::DrawIndirect { buffer, offset } => {
                        pass.draw_indirect(buffer.raw(), *offset);
                    }
                    GpuRenderPassCommand::DrawIndexedIndirect { buffer, offset } => {
                        pass.draw_indexed_indirect(buffer.raw(), *offset);
                    }
                }
            }
        }

        let info = self.next_info(GpuPassKind::Render, plan.label())?;
        self.encoded
            .push((info.clone(), RetainedPass::Render { _plan: plan }));
        Ok(info)
    }

    /// Finishes a nonempty encoder into a single-use managed batch.
    pub fn finish(self) -> Result<GpuPassBatch> {
        if self.encoded.is_empty() {
            return Err(invalid(
                "finish_pass_batch",
                "a GPU pass batch must contain at least one accepted pass",
            ));
        }
        let (passes, retained) = self.encoded.into_iter().unzip();
        Ok(GpuPassBatch {
            device_identity: Arc::clone(self.resources.device_identity()),
            resource_scope: self.capabilities.resource_scope,
            command_buffer: self.encoder.finish(),
            passes,
            _retained: retained,
        })
    }

    fn next_info(&self, kind: GpuPassKind, label: Option<&str>) -> Result<GpuPassInfo> {
        let sequence = u64::try_from(self.encoded.len()).map_err(|_| {
            Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "GPU pass sequence identifiers are exhausted",
            )
            .with_context(ErrorContext::new(COMPONENT, "encode_pass"))
        })?;
        Ok(GpuPassInfo {
            sequence,
            kind,
            label: label.map(str::to_owned),
        })
    }

    fn validate_compute(&self, plan: &GpuComputePassPlan) -> Result<()> {
        self.validate_required_features(plan.required_features(), "encode_compute")?;
        if plan.commands().is_empty() {
            return Err(invalid(
                "encode_compute",
                "a compute pass must contain at least one command",
            ));
        }

        let mut active_pipeline = None;
        let mut bound_groups = BTreeMap::new();
        let mut dispatches = 0_u64;
        for command in plan.commands() {
            match command {
                GpuComputePassCommand::SetPipeline(pipeline) => {
                    self.resources
                        .ensure_owner(pipeline.lease(), "encode_compute")?;
                    active_pipeline = Some(pipeline);
                }
                GpuComputePassCommand::SetBindGroup {
                    index,
                    bind_group,
                    dynamic_offsets,
                } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid(
                            "encode_compute",
                            "a compute bind group requires an active pipeline",
                        )
                    })?;
                    self.validate_bind_group(
                        pipeline.layout(),
                        *index,
                        bind_group,
                        dynamic_offsets,
                        "encode_compute",
                    )?;
                    bound_groups.insert(*index, bind_group.layout().id());
                }
                GpuComputePassCommand::SetPushConstants { offset, data } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid(
                            "encode_compute",
                            "compute push constants require an active pipeline",
                        )
                    })?;
                    self.validate_push_constants(
                        pipeline.layout(),
                        wgpu::ShaderStages::COMPUTE,
                        *offset,
                        data,
                        "encode_compute",
                    )?;
                }
                GpuComputePassCommand::Dispatch { x, y, z } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid(
                            "encode_compute",
                            "compute dispatch requires an active pipeline",
                        )
                    })?;
                    validate_compute_bindings(pipeline, &bound_groups)?;
                    let limit = self
                        .capabilities
                        .limits
                        .max_compute_workgroups_per_dimension;
                    if *x > limit || *y > limit || *z > limit {
                        return Err(unsupported(
                            "encode_compute",
                            "compute dispatch exceeds the enabled workgroup-count limit",
                        ));
                    }
                    dispatches = dispatches
                        .checked_add(1)
                        .ok_or_else(|| exhausted("encode_compute", "compute dispatch count"))?;
                }
                GpuComputePassCommand::DispatchIndirect { buffer, offset } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid(
                            "encode_compute",
                            "compute dispatch requires an active pipeline",
                        )
                    })?;
                    validate_compute_bindings(pipeline, &bound_groups)?;
                    self.resources
                        .ensure_owner(buffer.lease(), "encode_compute")?;
                    validate_indirect_buffer(
                        buffer,
                        *offset,
                        DISPATCH_INDIRECT_BYTES,
                        "encode_compute",
                    )?;
                    dispatches = dispatches
                        .checked_add(1)
                        .ok_or_else(|| exhausted("encode_compute", "compute dispatch count"))?;
                }
            }
        }
        if dispatches == 0 {
            return Err(invalid(
                "encode_compute",
                "a compute pass must dispatch at least one workload",
            ));
        }
        Ok(())
    }

    fn validate_render(&self, plan: &GpuRenderPassPlan) -> Result<()> {
        self.validate_required_features(plan.required_features(), "encode_render")?;
        let render_metadata = self.validate_attachments(plan)?;
        if plan.commands().is_empty() {
            return Err(invalid(
                "encode_render",
                "a render pass must contain at least one command",
            ));
        }

        let mut active_pipeline = None;
        let mut bound_groups = BTreeMap::new();
        let mut vertex_buffers = Vec::<Option<BoundVertexBuffer>>::new();
        let mut index_buffer = None;
        let mut blend_constant_set = false;
        let mut draws = 0_u64;
        for command in plan.commands() {
            match command {
                GpuRenderPassCommand::SetPipeline(pipeline) => {
                    self.resources
                        .ensure_owner(pipeline.lease(), "encode_render")?;
                    validate_pipeline_attachments(pipeline, plan, render_metadata)?;
                    active_pipeline = Some(pipeline);
                }
                GpuRenderPassCommand::SetBindGroup {
                    index,
                    bind_group,
                    dynamic_offsets,
                } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid(
                            "encode_render",
                            "a render bind group requires an active pipeline",
                        )
                    })?;
                    self.validate_bind_group(
                        pipeline.layout(),
                        *index,
                        bind_group,
                        dynamic_offsets,
                        "encode_render",
                    )?;
                    bound_groups.insert(*index, bind_group.layout().id());
                }
                GpuRenderPassCommand::SetVertexBuffer {
                    slot,
                    buffer,
                    range,
                } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid(
                            "encode_render",
                            "a vertex buffer requires an active render pipeline",
                        )
                    })?;
                    if *slot >= pipeline.vertex_buffer_count()
                        || *slot >= self.capabilities.limits.max_vertex_buffers
                    {
                        return Err(invalid(
                            "encode_render",
                            "vertex buffer slot is not declared by the active pipeline",
                        ));
                    }
                    self.validate_buffer_range(
                        buffer,
                        range,
                        wgpu::BufferUsages::VERTEX,
                        "encode_render",
                    )?;
                    let slot = *slot as usize;
                    if vertex_buffers.len() <= slot {
                        vertex_buffers.resize(slot + 1, None);
                    }
                    vertex_buffers[slot] = Some(BoundVertexBuffer {
                        bytes: range.end - range.start,
                    });
                }
                GpuRenderPassCommand::SetIndexBuffer {
                    buffer,
                    range,
                    format,
                } => {
                    require_pipeline(active_pipeline, "encode_render", "index buffer")?;
                    self.validate_buffer_range(
                        buffer,
                        range,
                        wgpu::BufferUsages::INDEX,
                        "encode_render",
                    )?;
                    let bytes_per_index = match format {
                        wgpu::IndexFormat::Uint16 => 2,
                        wgpu::IndexFormat::Uint32 => 4,
                    };
                    let byte_count = range.end - range.start;
                    if range.start % bytes_per_index != 0 || byte_count % bytes_per_index != 0 {
                        return Err(invalid(
                            "encode_render",
                            "index buffer range must align to its index element size",
                        ));
                    }
                    index_buffer = Some(BoundIndexBuffer {
                        entries: byte_count / bytes_per_index,
                        format: *format,
                    });
                }
                GpuRenderPassCommand::SetViewport {
                    x,
                    y,
                    width,
                    height,
                    min_depth,
                    max_depth,
                } => validate_viewport(
                    *x,
                    *y,
                    *width,
                    *height,
                    *min_depth,
                    *max_depth,
                    render_metadata.extent,
                )?,
                GpuRenderPassCommand::SetScissorRect {
                    x,
                    y,
                    width,
                    height,
                } => validate_scissor(*x, *y, *width, *height, render_metadata.extent)?,
                GpuRenderPassCommand::SetBlendConstant(color) => {
                    if ![color.r, color.g, color.b, color.a]
                        .into_iter()
                        .all(f64::is_finite)
                    {
                        return Err(invalid(
                            "encode_render",
                            "blend constant components must be finite",
                        ));
                    }
                    blend_constant_set = true;
                }
                GpuRenderPassCommand::SetStencilReference(_) => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid(
                            "encode_render",
                            "a stencil reference requires an active render pipeline",
                        )
                    })?;
                    if match pipeline.depth_stencil_format() {
                        Some(format) => !format.has_stencil_aspect(),
                        None => true,
                    } {
                        return Err(invalid(
                            "encode_render",
                            "the active render pipeline has no stencil aspect",
                        ));
                    }
                }
                GpuRenderPassCommand::SetPushConstants {
                    stages,
                    offset,
                    data,
                } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid(
                            "encode_render",
                            "render push constants require an active pipeline",
                        )
                    })?;
                    let invalid_stages =
                        *stages - (wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT);
                    if stages.is_empty() || !invalid_stages.is_empty() {
                        return Err(invalid(
                            "encode_render",
                            "render push constants may target only vertex and fragment stages",
                        ));
                    }
                    self.validate_push_constants(
                        pipeline.layout(),
                        *stages,
                        *offset,
                        data,
                        "encode_render",
                    )?;
                }
                GpuRenderPassCommand::Draw {
                    vertices,
                    instances,
                } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid("encode_render", "draw requires an active pipeline")
                    })?;
                    validate_ordered_range(vertices, "vertex", "encode_render")?;
                    validate_ordered_range(instances, "instance", "encode_render")?;
                    validate_render_ready(
                        pipeline,
                        &bound_groups,
                        &vertex_buffers,
                        index_buffer,
                        blend_constant_set,
                        false,
                    )?;
                    validate_vertex_limits(
                        pipeline.vertex_buffers(),
                        &vertex_buffers,
                        u64::from(vertices.end),
                        u64::from(instances.end),
                    )?;
                    draws = draws
                        .checked_add(1)
                        .ok_or_else(|| exhausted("encode_render", "render draw count"))?;
                }
                GpuRenderPassCommand::DrawIndexed {
                    indices, instances, ..
                } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid("encode_render", "indexed draw requires an active pipeline")
                    })?;
                    validate_ordered_range(indices, "index", "encode_render")?;
                    validate_ordered_range(instances, "instance", "encode_render")?;
                    validate_render_ready(
                        pipeline,
                        &bound_groups,
                        &vertex_buffers,
                        index_buffer,
                        blend_constant_set,
                        true,
                    )?;
                    let index_buffer = index_buffer.expect("render readiness checked index state");
                    if u64::from(indices.end) > index_buffer.entries {
                        return Err(invalid(
                            "encode_render",
                            "indexed draw exceeds the active index buffer range",
                        ));
                    }
                    validate_vertex_limits(
                        pipeline.vertex_buffers(),
                        &vertex_buffers,
                        0,
                        u64::from(instances.end),
                    )?;
                    draws = draws
                        .checked_add(1)
                        .ok_or_else(|| exhausted("encode_render", "render draw count"))?;
                }
                GpuRenderPassCommand::DrawIndirect { buffer, offset } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid("encode_render", "indirect draw requires an active pipeline")
                    })?;
                    validate_render_ready(
                        pipeline,
                        &bound_groups,
                        &vertex_buffers,
                        index_buffer,
                        blend_constant_set,
                        false,
                    )?;
                    self.resources
                        .ensure_owner(buffer.lease(), "encode_render")?;
                    validate_indirect_buffer(
                        buffer,
                        *offset,
                        DRAW_INDIRECT_BYTES,
                        "encode_render",
                    )?;
                    draws = draws
                        .checked_add(1)
                        .ok_or_else(|| exhausted("encode_render", "render draw count"))?;
                }
                GpuRenderPassCommand::DrawIndexedIndirect { buffer, offset } => {
                    let pipeline = active_pipeline.ok_or_else(|| {
                        invalid(
                            "encode_render",
                            "indexed indirect draw requires an active pipeline",
                        )
                    })?;
                    validate_render_ready(
                        pipeline,
                        &bound_groups,
                        &vertex_buffers,
                        index_buffer,
                        blend_constant_set,
                        true,
                    )?;
                    self.resources
                        .ensure_owner(buffer.lease(), "encode_render")?;
                    validate_indirect_buffer(
                        buffer,
                        *offset,
                        DRAW_INDEXED_INDIRECT_BYTES,
                        "encode_render",
                    )?;
                    draws = draws
                        .checked_add(1)
                        .ok_or_else(|| exhausted("encode_render", "render draw count"))?;
                }
            }
        }
        if draws == 0 {
            return Err(invalid(
                "encode_render",
                "a render pass must draw at least one workload",
            ));
        }
        Ok(())
    }

    fn validate_required_features(
        &self,
        required: wgpu::Features,
        operation: &'static str,
    ) -> Result<()> {
        let missing = required - self.capabilities.features;
        if missing.is_empty() {
            return Ok(());
        }
        Err(unsupported(
            operation,
            format!("GPU pass requires unavailable device features {missing:?}"),
        ))
    }

    fn validate_bind_group(
        &self,
        pipeline_layout: Option<&GpuPipelineLayout>,
        index: u32,
        bind_group: &GpuBindGroup,
        dynamic_offsets: &[wgpu::DynamicOffset],
        operation: &'static str,
    ) -> Result<()> {
        self.resources.ensure_owner(bind_group.lease(), operation)?;
        if index >= self.capabilities.limits.max_bind_groups {
            return Err(unsupported(
                operation,
                "bind-group index exceeds the enabled device limit",
            ));
        }
        let layout = pipeline_layout.ok_or_else(|| {
            invalid(
                operation,
                "managed pass bindings require an explicit managed pipeline layout",
            )
        })?;
        let Some(expected) = layout.info().bind_group_layouts().get(index as usize) else {
            return Err(invalid(
                operation,
                "bind-group index is not declared by the active pipeline layout",
            ));
        };
        if expected.id() != bind_group.layout().id() {
            return Err(invalid(
                operation,
                "bind group does not match the active pipeline layout slot",
            ));
        }

        let mut dynamic_bindings = bind_group
            .layout()
            .info()
            .entries()
            .iter()
            .filter_map(|entry| match entry.ty {
                wgpu::BindingType::Buffer {
                    ty,
                    has_dynamic_offset: true,
                    ..
                } => Some((entry.binding, ty)),
                _ => None,
            })
            .collect::<Vec<_>>();
        dynamic_bindings.sort_by_key(|(binding, _)| *binding);
        if dynamic_offsets.len() != dynamic_bindings.len() {
            return Err(invalid(
                operation,
                "dynamic offset count does not match the bind-group layout",
            ));
        }
        for (offset, (binding, binding_type)) in dynamic_offsets.iter().zip(dynamic_bindings) {
            let alignment = match binding_type {
                wgpu::BufferBindingType::Uniform => {
                    self.capabilities.limits.min_uniform_buffer_offset_alignment
                }
                wgpu::BufferBindingType::Storage { .. } => {
                    self.capabilities.limits.min_storage_buffer_offset_alignment
                }
            };
            if *offset % alignment != 0 {
                return Err(invalid(
                    operation,
                    "dynamic buffer offset is not aligned to the enabled device limit",
                ));
            }
            let entry = bind_group
                .entries()
                .iter()
                .find(|entry| entry.binding() == binding)
                .ok_or_else(|| {
                    invalid(
                        operation,
                        "dynamic binding is missing from the managed bind group",
                    )
                })?;
            let GpuBindingResource::Buffer(buffer_binding) = entry.resource() else {
                return Err(invalid(
                    operation,
                    "dynamic bindings must retain one managed buffer range",
                ));
            };
            let buffer_size = buffer_binding.buffer().info().size();
            let binding_end = buffer_binding
                .size()
                .map_or(buffer_size, |size| buffer_binding.offset() + size.get());
            let maximum_offset = buffer_size.saturating_sub(binding_end);
            if u64::from(*offset) > maximum_offset {
                return Err(invalid(
                    operation,
                    "dynamic buffer offset moves the binding beyond its allocation",
                ));
            }
        }
        Ok(())
    }

    fn validate_push_constants(
        &self,
        pipeline_layout: Option<&GpuPipelineLayout>,
        stages: wgpu::ShaderStages,
        offset: u32,
        data: &[u8],
        operation: &'static str,
    ) -> Result<()> {
        if !self.capabilities.supports(wgpu::Features::PUSH_CONSTANTS) {
            return Err(unsupported(
                operation,
                "push constants are not enabled on this GPU device",
            ));
        }
        if data.is_empty() || offset % wgpu::PUSH_CONSTANT_ALIGNMENT != 0 {
            return Err(invalid(
                operation,
                "push-constant offset and nonempty data must use four-byte alignment",
            ));
        }
        let data_len = u32::try_from(data.len()).map_err(|_| {
            invalid(
                operation,
                "push-constant data length cannot be represented by wgpu",
            )
        })?;
        if data_len % wgpu::PUSH_CONSTANT_ALIGNMENT != 0 {
            return Err(invalid(
                operation,
                "push-constant data length must use four-byte alignment",
            ));
        }
        let end = offset
            .checked_add(data_len)
            .ok_or_else(|| invalid(operation, "push-constant byte range overflows"))?;
        if end > self.capabilities.limits.max_push_constant_size {
            return Err(unsupported(
                operation,
                "push-constant byte range exceeds the enabled device limit",
            ));
        }
        let layout = pipeline_layout.ok_or_else(|| {
            invalid(
                operation,
                "managed push constants require an explicit managed pipeline layout",
            )
        })?;
        let mut covered_stages = wgpu::ShaderStages::NONE;
        for range in layout.info().push_constant_ranges() {
            if stages.contains(range.stages) {
                if range.range.start > offset || end > range.range.end {
                    return Err(invalid(
                        operation,
                        "push-constant bytes exceed a declared stage range",
                    ));
                }
                covered_stages |= range.stages;
            } else if stages.intersects(range.stages)
                || (offset < range.range.end && range.range.start < end)
            {
                return Err(invalid(
                    operation,
                    "push-constant stages do not cover every overlapping layout range",
                ));
            }
        }
        if covered_stages != stages {
            return Err(invalid(
                operation,
                "push-constant byte range and stages are not declared by the pipeline layout",
            ));
        }
        Ok(())
    }

    fn validate_buffer_range(
        &self,
        buffer: &GpuBuffer,
        range: &Range<wgpu::BufferAddress>,
        usage: wgpu::BufferUsages,
        operation: &'static str,
    ) -> Result<()> {
        self.resources.ensure_owner(buffer.lease(), operation)?;
        if !buffer.info().usage().contains(usage) {
            return Err(invalid(
                operation,
                format!("GPU buffer is missing required usage {usage:?}"),
            ));
        }
        if range.start >= range.end || range.end > buffer.info().size() {
            return Err(invalid(
                operation,
                "GPU buffer range must be nonempty and within the allocation",
            ));
        }
        Ok(())
    }

    fn validate_attachments(&self, plan: &GpuRenderPassPlan) -> Result<RenderViewMetadata> {
        if !plan.color_attachments().iter().any(Option::is_some)
            && plan.depth_stencil_attachment().is_none()
        {
            return Err(invalid(
                "encode_render",
                "a render pass requires at least one color, depth, or stencil attachment",
            ));
        }
        if plan.color_attachments().len() > self.capabilities.limits.max_color_attachments as usize
        {
            return Err(unsupported(
                "encode_render",
                "render pass color attachments exceed the enabled device limit",
            ));
        }

        let mut attachment_ranges = Vec::new();
        let mut reference = None;
        for attachment in plan.color_attachments().iter().flatten() {
            if let wgpu::LoadOp::Clear(color) = attachment.operations().load {
                if ![color.r, color.g, color.b, color.a]
                    .into_iter()
                    .all(f64::is_finite)
                {
                    return Err(invalid(
                        "encode_render",
                        "color attachment clear components must be finite",
                    ));
                }
            }
            let source = self.validate_attachment_view(
                attachment.view(),
                false,
                &mut attachment_ranges,
                &mut reference,
            )?;
            if let Some(resolve) = attachment.resolve_target() {
                let target = self.render_view_metadata(resolve)?;
                validate_attachment_role(resolve, false)?;
                register_attachment_range(resolve, target, &mut attachment_ranges)?;
                if view_format(resolve) != view_format(attachment.view())
                    || target.extent != source.extent
                    || target.multiview != source.multiview
                    || target.samples != 1
                    || source.samples == 1
                {
                    return Err(invalid(
                        "encode_render",
                        "resolve target must be matching single-sample storage for a multisample attachment",
                    ));
                }
                let format_features = self.resources.texture_format_features(view_format(resolve));
                if !format_features
                    .flags
                    .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
                {
                    return Err(unsupported(
                        "encode_render",
                        "resolve target format does not support multisample resolve",
                    ));
                }
            }
        }

        if let Some(attachment) = plan.depth_stencil_attachment() {
            if attachment.depth_operations().is_none() && attachment.stencil_operations().is_none()
            {
                return Err(invalid(
                    "encode_render",
                    "a depth and stencil attachment must enable at least one aspect",
                ));
            }
            if let Some(operations) = attachment.depth_operations() {
                if let wgpu::LoadOp::Clear(value) = operations.load {
                    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                        return Err(invalid(
                            "encode_render",
                            "depth attachment clear value must be finite and within zero to one",
                        ));
                    }
                }
            }
            let format = view_format(attachment.view());
            if attachment.depth_operations().is_some() && !format.has_depth_aspect() {
                return Err(invalid(
                    "encode_render",
                    "depth operations require a depth texture format",
                ));
            }
            if attachment.stencil_operations().is_some() && !format.has_stencil_aspect() {
                return Err(invalid(
                    "encode_render",
                    "stencil operations require a stencil texture format",
                ));
            }
            self.validate_attachment_view(
                attachment.view(),
                true,
                &mut attachment_ranges,
                &mut reference,
            )?;
        }
        reference.ok_or_else(|| invalid("encode_render", "render pass attachment is missing"))
    }

    fn validate_attachment_view(
        &self,
        view: &GpuTextureView,
        depth_stencil: bool,
        ranges: &mut Vec<(GpuResourceId, u32, Range<u32>)>,
        reference: &mut Option<RenderViewMetadata>,
    ) -> Result<RenderViewMetadata> {
        let metadata = self.render_view_metadata(view)?;
        validate_attachment_role(view, depth_stencil)?;
        register_attachment_range(view, metadata, ranges)?;
        if reference.is_some_and(|expected| {
            expected.extent != metadata.extent
                || expected.samples != metadata.samples
                || expected.multiview != metadata.multiview
        }) {
            return Err(invalid(
                "encode_render",
                "render attachments must have matching extents, sample counts, and multiview layers",
            ));
        }
        *reference = Some(metadata);
        Ok(metadata)
    }

    fn render_view_metadata(&self, view: &GpuTextureView) -> Result<RenderViewMetadata> {
        self.resources.ensure_owner(view.lease(), "encode_render")?;
        let texture = view.texture().info();
        if !texture
            .usage()
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(invalid(
                "encode_render",
                "render texture is missing RENDER_ATTACHMENT usage",
            ));
        }

        let dimension = resolved_view_dimension(view);
        if dimension != wgpu::TextureViewDimension::D2
            && !(dimension == wgpu::TextureViewDimension::D2Array
                && self.capabilities.supports(wgpu::Features::MULTIVIEW))
        {
            return Err(unsupported(
                "encode_render",
                "render attachment view dimension is unsupported by this device",
            ));
        }
        let mip_level_count = view.info().mip_level_count().unwrap_or_else(|| {
            texture
                .mip_level_count()
                .saturating_sub(view.info().base_mip_level())
        });
        if mip_level_count != 1 {
            return Err(invalid(
                "encode_render",
                "render attachment views must select exactly one mip level",
            ));
        }
        let array_layer_count = resolved_view_array_layer_count(view, dimension);
        if array_layer_count > 1 && !self.capabilities.supports(wgpu::Features::MULTIVIEW) {
            return Err(unsupported(
                "encode_render",
                "multilayer render attachments require the MULTIVIEW feature",
            ));
        }
        if !view_uses_full_aspects(view) {
            return Err(invalid(
                "encode_render",
                "render attachment views must include every aspect of their texture format",
            ));
        }

        let mip = view.info().base_mip_level();
        Ok(RenderViewMetadata {
            extent: wgpu::Extent3d {
                width: texture.size().width.checked_shr(mip).unwrap_or(0).max(1),
                height: texture.size().height.checked_shr(mip).unwrap_or(0).max(1),
                depth_or_array_layers: 1,
            },
            samples: texture.sample_count(),
            multiview: if array_layer_count >= 2 {
                NonZeroU32::new(array_layer_count)
            } else {
                None
            },
            base_mip_level: mip,
            base_array_layer: view.info().base_array_layer(),
            array_layer_count,
        })
    }
}

impl<'device> GpuResources<'device> {
    /// Creates a managed pass encoder for this exact device lifetime.
    #[must_use]
    pub fn create_pass_encoder(&self, label: Option<&str>) -> GpuPassEncoder<'device> {
        GpuPassEncoder {
            resources: self.clone(),
            capabilities: GpuPassCapabilities {
                features: self.enabled_features(),
                limits: self.enabled_limits().clone(),
                resource_scope: self.scope_id(),
            },
            encoder: self
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label }),
            encoded: Vec::new(),
        }
    }
}

/// A finished command buffer that retains every managed pass dependency.
#[derive(Debug)]
pub struct GpuPassBatch {
    device_identity: Arc<()>,
    resource_scope: u64,
    command_buffer: wgpu::CommandBuffer,
    passes: Vec<GpuPassInfo>,
    _retained: Vec<RetainedPass>,
}

impl GpuPassBatch {
    /// Returns the resource manager scope that encoded this batch.
    #[must_use]
    pub const fn resource_scope(&self) -> u64 {
        self.resource_scope
    }

    /// Returns accepted passes in exact command-buffer order.
    #[must_use]
    pub fn passes(&self) -> &[GpuPassInfo] {
        &self.passes
    }
}

/// Metadata for one accepted queue submission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuPassSubmission {
    resource_scope: u64,
    passes: Vec<GpuPassInfo>,
}

impl GpuPassSubmission {
    /// Returns the resource manager scope that produced this submission.
    #[must_use]
    pub const fn resource_scope(&self) -> u64 {
        self.resource_scope
    }

    /// Returns submitted passes in exact queue order.
    #[must_use]
    pub fn passes(&self) -> &[GpuPassInfo] {
        &self.passes
    }
}

impl GpuDevice {
    /// Consumes and submits one managed pass batch through this device's private queue.
    ///
    /// A batch from an obsolete device lifetime is rejected before queue access.
    pub fn submit_pass_batch(&self, batch: GpuPassBatch) -> Result<GpuPassSubmission> {
        if !Arc::ptr_eq(self.identity(), &batch.device_identity) {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "GPU pass batch belongs to a different device lifetime",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "submit_pass_batch")
                    .with_field("resource_scope", batch.resource_scope.to_string()),
            ));
        }
        let GpuPassBatch {
            resource_scope,
            command_buffer,
            passes,
            _retained,
            ..
        } = batch;
        self.submit_viewport([command_buffer]);
        Ok(GpuPassSubmission {
            resource_scope,
            passes,
        })
    }
}

fn require_pipeline<T>(
    pipeline: Option<&T>,
    operation: &'static str,
    action: &'static str,
) -> Result<()> {
    if pipeline.is_none() {
        return Err(invalid(
            operation,
            format!("{action} requires an active pipeline"),
        ));
    }
    Ok(())
}

fn validate_compute_bindings(
    pipeline: &GpuComputePipeline,
    bound_groups: &BTreeMap<u32, GpuResourceId>,
) -> Result<()> {
    validate_pipeline_bind_groups(
        pipeline.layout(),
        !pipeline.module().reflection().bindings().is_empty(),
        bound_groups,
        "encode_compute",
    )
}

fn validate_render_bindings(
    pipeline: &GpuRenderPipeline,
    bound_groups: &BTreeMap<u32, GpuResourceId>,
) -> Result<()> {
    let automatic_bindings = !pipeline.vertex_module().reflection().bindings().is_empty()
        || pipeline
            .fragment_module()
            .is_some_and(|module| !module.reflection().bindings().is_empty());
    validate_pipeline_bind_groups(
        pipeline.layout(),
        automatic_bindings,
        bound_groups,
        "encode_render",
    )
}

fn validate_pipeline_bind_groups(
    layout: Option<&GpuPipelineLayout>,
    automatic_bindings: bool,
    bound_groups: &BTreeMap<u32, GpuResourceId>,
    operation: &'static str,
) -> Result<()> {
    let Some(layout) = layout else {
        if automatic_bindings {
            return Err(invalid(
                operation,
                "managed passes require an explicit pipeline layout when shaders declare bindings",
            ));
        }
        return Ok(());
    };
    for (index, expected) in layout.info().bind_group_layouts().iter().enumerate() {
        let index = u32::try_from(index)
            .map_err(|_| invalid(operation, "pipeline bind-group index is not representable"))?;
        if bound_groups.get(&index) != Some(&expected.id()) {
            return Err(invalid(
                operation,
                "every active pipeline bind-group slot must be set before dispatch or draw",
            ));
        }
    }
    Ok(())
}

fn validate_render_ready(
    pipeline: &GpuRenderPipeline,
    bound_groups: &BTreeMap<u32, GpuResourceId>,
    vertex_buffers: &[Option<BoundVertexBuffer>],
    index_buffer: Option<BoundIndexBuffer>,
    blend_constant_set: bool,
    indexed: bool,
) -> Result<()> {
    validate_render_bindings(pipeline, bound_groups)?;
    if pipeline.requires_blend_constant() && !blend_constant_set {
        return Err(invalid(
            "encode_render",
            "the active render pipeline requires a blend constant before drawing",
        ));
    }
    if pipeline
        .vertex_buffers()
        .iter()
        .enumerate()
        .any(|(slot, _)| vertex_buffers.get(slot).and_then(Option::as_ref).is_none())
    {
        return Err(invalid(
            "encode_render",
            "every vertex-buffer slot declared by the active pipeline must be bound before drawing",
        ));
    }
    if indexed {
        let index_buffer = index_buffer.ok_or_else(|| {
            invalid(
                "encode_render",
                "an indexed draw requires an active index buffer",
            )
        })?;
        if pipeline
            .strip_index_format()
            .is_some_and(|required| required != index_buffer.format)
        {
            return Err(invalid(
                "encode_render",
                "index buffer format does not match the active strip pipeline",
            ));
        }
    }
    Ok(())
}

fn validate_vertex_limits(
    layouts: &[GpuVertexBufferInfo],
    buffers: &[Option<BoundVertexBuffer>],
    last_vertex: u64,
    last_instance: u64,
) -> Result<()> {
    for (slot, layout) in layouts.iter().enumerate() {
        let buffer = buffers
            .get(slot)
            .and_then(Option::as_ref)
            .expect("render readiness checked every vertex buffer");
        let limit = if buffer.bytes < layout.last_stride() {
            0
        } else if layout.array_stride() == 0 {
            u64::from(u32::MAX)
        } else {
            (buffer.bytes - layout.last_stride()) / layout.array_stride() + 1
        };
        let requested = match layout.step_mode() {
            wgpu::VertexStepMode::Vertex => last_vertex,
            wgpu::VertexStepMode::Instance => last_instance,
        };
        if requested > limit {
            return Err(invalid(
                "encode_render",
                "draw range exceeds an active vertex-buffer binding",
            ));
        }
    }
    Ok(())
}

fn validate_ordered_range(
    range: &Range<u32>,
    kind: &'static str,
    operation: &'static str,
) -> Result<()> {
    if range.start > range.end {
        return Err(invalid(operation, format!("{kind} range must be ordered")));
    }
    Ok(())
}

fn validate_pipeline_attachments(
    pipeline: &GpuRenderPipeline,
    plan: &GpuRenderPassPlan,
    metadata: RenderViewMetadata,
) -> Result<()> {
    let actual_formats = plan
        .color_attachments()
        .iter()
        .map(|attachment| {
            attachment
                .as_ref()
                .map(|attachment| view_format(attachment.view()))
        })
        .collect::<Vec<_>>();
    if pipeline.color_target_formats() != actual_formats {
        return Err(invalid(
            "encode_render",
            "render pipeline color targets do not match pass attachment formats",
        ));
    }
    let depth_format = plan
        .depth_stencil_attachment()
        .map(|attachment| view_format(attachment.view()));
    if pipeline.depth_stencil_format() != depth_format {
        return Err(invalid(
            "encode_render",
            "render pipeline depth format does not match the pass attachment",
        ));
    }
    let sample_count = plan
        .color_attachments()
        .iter()
        .flatten()
        .next()
        .map_or_else(
            || {
                plan.depth_stencil_attachment()
                    .map(|attachment| attachment.view().texture().info().sample_count())
                    .unwrap_or(1)
            },
            |attachment| attachment.view().texture().info().sample_count(),
        );
    if pipeline.sample_count() != sample_count {
        return Err(invalid(
            "encode_render",
            "render pipeline sample count does not match pass attachments",
        ));
    }
    if pipeline.multiview() != metadata.multiview {
        return Err(invalid(
            "encode_render",
            "render pipeline multiview count does not match pass attachments",
        ));
    }
    if let Some(attachment) = plan.depth_stencil_attachment() {
        if pipeline.writes_depth() && attachment.depth_operations().is_none() {
            return Err(invalid(
                "encode_render",
                "render pipeline writes depth but the pass depth aspect is read only",
            ));
        }
        if pipeline.writes_stencil() && attachment.stencil_operations().is_none() {
            return Err(invalid(
                "encode_render",
                "render pipeline writes stencil but the pass stencil aspect is read only",
            ));
        }
    }
    Ok(())
}

fn validate_attachment_role(view: &GpuTextureView, depth_stencil: bool) -> Result<()> {
    if view_format(view).is_depth_stencil_format() != depth_stencil {
        return Err(invalid(
            "encode_render",
            "render attachment format does not match its color or depth role",
        ));
    }
    Ok(())
}

fn register_attachment_range(
    view: &GpuTextureView,
    metadata: RenderViewMetadata,
    ranges: &mut Vec<(GpuResourceId, u32, Range<u32>)>,
) -> Result<()> {
    let layers = metadata.base_array_layer
        ..metadata
            .base_array_layer
            .checked_add(metadata.array_layer_count)
            .ok_or_else(|| invalid("encode_render", "attachment layer range overflows"))?;
    if ranges.iter().any(|(texture, mip, existing)| {
        *texture == view.texture().id()
            && *mip == metadata.base_mip_level
            && layers.start < existing.end
            && existing.start < layers.end
    }) {
        return Err(invalid(
            "encode_render",
            "render attachment subresources must not alias within one pass",
        ));
    }
    ranges.push((view.texture().id(), metadata.base_mip_level, layers));
    Ok(())
}

fn resolved_view_dimension(view: &GpuTextureView) -> wgpu::TextureViewDimension {
    view.info()
        .dimension()
        .unwrap_or_else(|| match view.texture().info().dimension() {
            wgpu::TextureDimension::D1 => wgpu::TextureViewDimension::D1,
            wgpu::TextureDimension::D2 => {
                if view.texture().info().size().depth_or_array_layers == 1 {
                    wgpu::TextureViewDimension::D2
                } else {
                    wgpu::TextureViewDimension::D2Array
                }
            }
            wgpu::TextureDimension::D3 => wgpu::TextureViewDimension::D3,
        })
}

fn resolved_view_array_layer_count(
    view: &GpuTextureView,
    dimension: wgpu::TextureViewDimension,
) -> u32 {
    view.info()
        .array_layer_count()
        .unwrap_or_else(|| match dimension {
            wgpu::TextureViewDimension::D1
            | wgpu::TextureViewDimension::D2
            | wgpu::TextureViewDimension::D3 => 1,
            wgpu::TextureViewDimension::Cube => 6,
            wgpu::TextureViewDimension::D2Array | wgpu::TextureViewDimension::CubeArray => view
                .texture()
                .info()
                .size()
                .depth_or_array_layers
                .saturating_sub(view.info().base_array_layer()),
        })
}

fn view_uses_full_aspects(view: &GpuTextureView) -> bool {
    let format = view.texture().info().format();
    match view.info().aspect() {
        wgpu::TextureAspect::All => true,
        wgpu::TextureAspect::DepthOnly => format.has_depth_aspect() && !format.has_stencil_aspect(),
        wgpu::TextureAspect::StencilOnly => {
            format.has_stencil_aspect() && !format.has_depth_aspect()
        }
        wgpu::TextureAspect::Plane0 | wgpu::TextureAspect::Plane1 | wgpu::TextureAspect::Plane2 => {
            false
        }
    }
}

fn validate_indirect_buffer(
    buffer: &GpuBuffer,
    offset: wgpu::BufferAddress,
    required_bytes: wgpu::BufferAddress,
    operation: &'static str,
) -> Result<()> {
    if !buffer.info().usage().contains(wgpu::BufferUsages::INDIRECT) {
        return Err(invalid(
            operation,
            "indirect argument buffer is missing INDIRECT usage",
        ));
    }
    let end = offset
        .checked_add(required_bytes)
        .ok_or_else(|| invalid(operation, "indirect argument byte range overflows"))?;
    if offset % 4 != 0 || end > buffer.info().size() {
        return Err(invalid(
            operation,
            "indirect argument range must be four-byte aligned and within the buffer",
        ));
    }
    Ok(())
}

fn validate_viewport(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    min_depth: f32,
    max_depth: f32,
    extent: wgpu::Extent3d,
) -> Result<()> {
    if ![x, y, width, height, min_depth, max_depth]
        .into_iter()
        .all(f32::is_finite)
        || x < 0.0
        || y < 0.0
        || width <= 0.0
        || height <= 0.0
        || x + width > extent.width as f32
        || y + height > extent.height as f32
        || !(0.0..=1.0).contains(&min_depth)
        || !(0.0..=1.0).contains(&max_depth)
        || min_depth > max_depth
    {
        return Err(invalid(
            "encode_render",
            "viewport must be finite, within the pass extent, and use ordered unit depth",
        ));
    }
    Ok(())
}

fn validate_scissor(x: u32, y: u32, width: u32, height: u32, extent: wgpu::Extent3d) -> Result<()> {
    if x.checked_add(width).map_or(true, |end| end > extent.width)
        || y.checked_add(height)
            .map_or(true, |end| end > extent.height)
    {
        return Err(invalid(
            "encode_render",
            "scissor rectangle must remain within the pass extent",
        ));
    }
    Ok(())
}

fn view_format(view: &GpuTextureView) -> wgpu::TextureFormat {
    view.info()
        .format()
        .unwrap_or_else(|| view.texture().info().format())
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn exhausted(operation: &'static str, resource: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Terminal,
        format!("{resource} is exhausted"),
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
