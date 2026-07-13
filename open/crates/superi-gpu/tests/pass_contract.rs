use std::num::{NonZeroU64, NonZeroUsize};

use superi_core::error::{ErrorCategory, Recoverability};
use superi_gpu::binding::{
    GpuBindGroup, GpuBindGroupDescriptor, GpuBindGroupEntry, GpuBufferBinding,
};
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::pass::{
    GpuColorAttachment, GpuComputePassCommand, GpuComputePassPlan, GpuPassKind,
    GpuRenderPassCommand, GpuRenderPassPlan,
};
use superi_gpu::pipeline::{
    GpuComputePipeline, GpuComputePipelineDescriptor, GpuFragmentState,
    GpuPipelineLayoutDescriptor, GpuRenderPipeline, GpuRenderPipelineDescriptor, GpuVertexState,
};
use superi_gpu::resource::GpuResources;
use superi_gpu::shader::{GpuShaderModuleDescriptor, ShaderCache};
use superi_gpu::wgpu;

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(adapter.create_device(&DeviceRequest::default())).ok()
}

fn compute_pipeline(resources: &GpuResources<'_>) -> GpuComputePipeline {
    let layout = resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some("pass compute layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        })
        .unwrap();
    let cache = ShaderCache::new(resources, NonZeroUsize::new(1).unwrap());
    let shader = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("pass compute shader"),
        source: "@compute @workgroup_size(1) fn main() {}",
    }))
    .unwrap();
    pollster::block_on(
        resources.create_compute_pipeline(GpuComputePipelineDescriptor {
            label: Some("pass compute pipeline"),
            layout: Some(&layout),
            module: &shader,
            entry_point: "main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        }),
    )
    .unwrap()
}

fn dynamic_compute_pipeline(
    resources: &GpuResources<'_>,
    device: &GpuDevice,
) -> (GpuComputePipeline, GpuBindGroup, u32) {
    let alignment = device.enabled_limits().min_uniform_buffer_offset_alignment;
    let binding_size = u64::from(alignment);
    let buffer = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("dynamic uniform buffer"),
            size: binding_size * 2,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        })
        .unwrap();
    let bind_group_layout = resources
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dynamic uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(4),
                },
                count: None,
            }],
        })
        .unwrap();
    let entries = [GpuBindGroupEntry::buffer_range(
        0,
        GpuBufferBinding::new(buffer, 0, NonZeroU64::new(binding_size)),
    )];
    let bind_group = resources
        .create_bind_group(GpuBindGroupDescriptor {
            label: Some("dynamic uniform group"),
            layout: &bind_group_layout,
            entries: &entries,
        })
        .unwrap();
    let layouts = [&bind_group_layout];
    let layout = resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some("dynamic compute layout"),
            bind_group_layouts: &layouts,
            push_constant_ranges: &[],
        })
        .unwrap();
    let cache = ShaderCache::new(resources, NonZeroUsize::new(1).unwrap());
    let shader = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("dynamic compute shader"),
        source: r#"
@group(0) @binding(0) var<uniform> value: u32;

@compute @workgroup_size(1)
fn main() {
    _ = value;
}
"#,
    }))
    .unwrap();
    let pipeline = pollster::block_on(resources.create_compute_pipeline(
        GpuComputePipelineDescriptor {
            label: Some("dynamic compute pipeline"),
            layout: Some(&layout),
            module: &shader,
            entry_point: "main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        },
    ))
    .unwrap();
    (pipeline, bind_group, alignment * 2)
}

fn render_pipeline(resources: &GpuResources<'_>, format: wgpu::TextureFormat) -> GpuRenderPipeline {
    let layout = resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some("pass render layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        })
        .unwrap();
    let cache = ShaderCache::new(resources, NonZeroUsize::new(1).unwrap());
    let shader = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("pass render shader"),
        source: r#"
@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fragment_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.25, 0.5, 0.75, 1.0);
}
"#,
    }))
    .unwrap();
    let targets = [Some(wgpu::ColorTargetState {
        format,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    pollster::block_on(
        resources.create_render_pipeline(GpuRenderPipelineDescriptor {
            label: Some("pass render pipeline"),
            layout: Some(&layout),
            vertex: GpuVertexState {
                module: &shader,
                entry_point: "vertex_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(GpuFragmentState {
                module: &shader,
                entry_point: "fragment_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview: None,
            cache: None,
        }),
    )
    .unwrap()
}

fn target_view(resources: &GpuResources<'_>) -> superi_gpu::texture::GpuTextureView {
    let texture = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("pass target"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .unwrap();
    resources
        .create_texture_view(&texture, &wgpu::TextureViewDescriptor::default())
        .unwrap()
}

#[test]
fn pass_encoder_exposes_the_acquired_device_capabilities() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping native pass capabilities");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let empty = resources
        .create_pass_encoder(Some("empty pass batch"))
        .finish()
        .unwrap_err();
    assert_eq!(empty.category(), ErrorCategory::InvalidInput);
    let encoder = resources.create_pass_encoder(Some("capability contract"));

    assert_eq!(encoder.capabilities().features(), device.enabled_features());
    assert_eq!(encoder.capabilities().limits(), device.enabled_limits());
    assert_eq!(
        encoder.capabilities().resource_scope(),
        resources.scope_id()
    );
}

#[test]
fn compute_and_render_plans_are_preflighted_then_recorded_in_exact_order() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping native pass ordering");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let compute_pipeline = compute_pipeline(&resources);
    let render_pipeline = render_pipeline(&resources, wgpu::TextureFormat::Rgba8Unorm);
    let render_view = target_view(&resources);
    let mut encoder = resources.create_pass_encoder(Some("ordered passes"));

    let mut missing_pipeline = GpuComputePassPlan::new("missing pipeline");
    missing_pipeline.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    let error = encoder.encode_compute(missing_pipeline).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(encoder.pass_count(), 0);

    let mut compute = GpuComputePassPlan::new("compute first");
    compute.push_command(GpuComputePassCommand::SetPipeline(compute_pipeline.clone()));
    compute.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    let compute_info = encoder.encode_compute(compute).unwrap();
    assert_eq!(compute_info.sequence(), 0);
    assert_eq!(compute_info.kind(), GpuPassKind::Compute);

    let mut render = GpuRenderPassPlan::new("render second");
    render.push_color_attachment(GpuColorAttachment::new(
        render_view,
        wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        },
    ));
    render.push_command(GpuRenderPassCommand::SetPipeline(render_pipeline.clone()));
    render.push_command(GpuRenderPassCommand::Draw {
        vertices: 0..3,
        instances: 0..1,
    });
    let render_info = encoder.encode_render(render).unwrap();
    assert_eq!(render_info.sequence(), 1);
    assert_eq!(render_info.kind(), GpuPassKind::Render);

    drop(compute_pipeline);
    drop(render_pipeline);
    assert_eq!(
        resources
            .stats()
            .count(superi_gpu::resource::GpuResourceKind::ComputePipeline),
        1
    );
    assert_eq!(
        resources
            .stats()
            .count(superi_gpu::resource::GpuResourceKind::RenderPipeline),
        1
    );
    let batch = encoder.finish().unwrap();
    assert_eq!(batch.resource_scope(), resources.scope_id());
    assert_eq!(batch.passes(), &[compute_info, render_info]);
    let submission = device.submit_pass_batch(batch).unwrap();
    assert_eq!(submission.passes()[0].label(), Some("compute first"));
    assert_eq!(submission.passes()[1].label(), Some("render second"));
    assert_eq!(
        resources
            .stats()
            .count(superi_gpu::resource::GpuResourceKind::ComputePipeline),
        0
    );
    assert_eq!(
        resources
            .stats()
            .count(superi_gpu::resource::GpuResourceKind::RenderPipeline),
        0
    );
}

#[test]
fn deferred_wgpu_state_is_rejected_by_managed_preflight() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping managed state preflight");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let (dynamic_pipeline, dynamic_group, out_of_bounds_offset) =
        dynamic_compute_pipeline(&resources, &device);
    let mut encoder = resources.create_pass_encoder(Some("managed state preflight"));

    let mut missing_group = GpuComputePassPlan::new("missing required group");
    missing_group.push_command(GpuComputePassCommand::SetPipeline(dynamic_pipeline.clone()));
    missing_group.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    let error = encoder.encode_compute(missing_group).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(encoder.pass_count(), 0);

    let mut invalid_dynamic_offset = GpuComputePassPlan::new("dynamic offset overrun");
    invalid_dynamic_offset.push_command(GpuComputePassCommand::SetPipeline(dynamic_pipeline));
    invalid_dynamic_offset.push_command(GpuComputePassCommand::SetBindGroup {
        index: 0,
        bind_group: dynamic_group,
        dynamic_offsets: vec![out_of_bounds_offset],
    });
    invalid_dynamic_offset.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    let error = encoder.encode_compute(invalid_dynamic_offset).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(encoder.pass_count(), 0);

    let render_pipeline = render_pipeline(&resources, wgpu::TextureFormat::Rgba8Unorm);
    let mipmapped = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("multi-mip attachment"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 2,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .unwrap();
    let multi_mip_view = resources
        .create_texture_view(&mipmapped, &wgpu::TextureViewDescriptor::default())
        .unwrap();
    let mut invalid_attachment = GpuRenderPassPlan::new("multi-mip attachment");
    invalid_attachment.push_color_attachment(GpuColorAttachment::new(
        multi_mip_view,
        wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        },
    ));
    invalid_attachment.push_command(GpuRenderPassCommand::SetPipeline(render_pipeline.clone()));
    invalid_attachment.push_command(GpuRenderPassCommand::Draw {
        vertices: 0..3,
        instances: 0..1,
    });
    let error = encoder.encode_render(invalid_attachment).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let mut invalid_viewport = GpuRenderPassPlan::new("viewport outside target");
    invalid_viewport.push_color_attachment(GpuColorAttachment::new(
        target_view(&resources),
        wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        },
    ));
    invalid_viewport.push_command(GpuRenderPassCommand::SetPipeline(render_pipeline));
    invalid_viewport.push_command(GpuRenderPassCommand::SetViewport {
        x: 0.0,
        y: 0.0,
        width: 5.0,
        height: 4.0,
        min_depth: 0.0,
        max_depth: 1.0,
    });
    invalid_viewport.push_command(GpuRenderPassCommand::Draw {
        vertices: 0..3,
        instances: 0..1,
    });
    let error = encoder.encode_render(invalid_viewport).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(encoder.pass_count(), 0);
}

#[test]
fn capabilities_usages_ranges_and_recovered_device_lifetimes_fail_before_encoding() {
    let Some(first_device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping pass validation");
        return;
    };
    let Some(second_device) = test_device() else {
        eprintln!("a second wgpu device is unavailable, skipping recovery validation");
        return;
    };
    let first = GpuResources::new(&first_device).unwrap();
    let second = GpuResources::new(&second_device).unwrap();
    let first_pipeline = compute_pipeline(&first);
    let second_pipeline = compute_pipeline(&second);

    let mut encoder = first.create_pass_encoder(Some("validation"));
    let mut unsupported = GpuComputePassPlan::new("unsupported feature")
        .with_required_features(wgpu::Features::PUSH_CONSTANTS);
    unsupported.push_command(GpuComputePassCommand::SetPipeline(first_pipeline.clone()));
    unsupported.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    let error = encoder.encode_compute(unsupported).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);

    let wrong_usage = first
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("not indirect"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        })
        .unwrap();
    let mut invalid_usage = GpuComputePassPlan::new("invalid indirect usage");
    invalid_usage.push_command(GpuComputePassCommand::SetPipeline(first_pipeline.clone()));
    invalid_usage.push_command(GpuComputePassCommand::DispatchIndirect {
        buffer: wrong_usage,
        offset: 0,
    });
    let error = encoder.encode_compute(invalid_usage).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let short_indirect = first
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("short indirect"),
            size: 8,
            usage: wgpu::BufferUsages::INDIRECT,
            mapped_at_creation: false,
        })
        .unwrap();
    let mut invalid_range = GpuComputePassPlan::new("invalid indirect range");
    invalid_range.push_command(GpuComputePassCommand::SetPipeline(first_pipeline.clone()));
    invalid_range.push_command(GpuComputePassCommand::DispatchIndirect {
        buffer: short_indirect,
        offset: 0,
    });
    let error = encoder.encode_compute(invalid_range).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let invalid_target = first
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("not a render attachment"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .unwrap();
    let invalid_view = first
        .create_texture_view(&invalid_target, &wgpu::TextureViewDescriptor::default())
        .unwrap();
    let first_render_pipeline = render_pipeline(&first, wgpu::TextureFormat::Rgba8Unorm);
    let mut invalid_attachment = GpuRenderPassPlan::new("invalid attachment usage");
    invalid_attachment.push_color_attachment(GpuColorAttachment::new(
        invalid_view,
        wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        },
    ));
    invalid_attachment.push_command(GpuRenderPassCommand::SetPipeline(first_render_pipeline));
    invalid_attachment.push_command(GpuRenderPassCommand::Draw {
        vertices: 0..3,
        instances: 0..1,
    });
    let error = encoder.encode_render(invalid_attachment).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let mut foreign_pipeline = GpuComputePassPlan::new("foreign pipeline");
    foreign_pipeline.push_command(GpuComputePassCommand::SetPipeline(second_pipeline));
    foreign_pipeline.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    let error = encoder.encode_compute(foreign_pipeline).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(encoder.pass_count(), 0);

    let mut valid = GpuComputePassPlan::new("old device work");
    valid.push_command(GpuComputePassCommand::SetPipeline(first_pipeline));
    valid.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    encoder.encode_compute(valid).unwrap();
    let old_batch = encoder.finish().unwrap();
    let error = second_device.submit_pass_batch(old_batch).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
}
