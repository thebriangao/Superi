use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::mpsc;

use crate::binding::{GpuBindGroupDescriptor, GpuBindGroupEntry};
use crate::device::{AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions};
use crate::pass::{
    GpuColorAttachment, GpuComputePassCommand, GpuComputePassPlan, GpuPassKind,
    GpuRenderPassCommand, GpuRenderPassPlan,
};
use crate::pipeline::{
    GpuComputePipelineDescriptor, GpuFragmentState, GpuPipelineLayoutDescriptor,
    GpuRenderPipelineDescriptor, GpuVertexState,
};
use crate::resource::GpuResources;
use crate::shader::{GpuShaderModuleDescriptor, ShaderCache};
use crate::submission::GpuSubmissionQueue;
use crate::wgpu;

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(adapter.create_device(&DeviceRequest::default())).ok()
}

fn read_buffer(device: &GpuDevice, buffer: &crate::buffer::GpuBuffer) -> Vec<u8> {
    let slice = buffer.raw().slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).expect("map receiver remains alive");
    });
    let _ = device.wgpu_device().poll(wgpu::Maintain::Wait);
    receiver
        .recv()
        .expect("mapping callback must run")
        .expect("readback mapping must succeed");
    let bytes = slice.get_mapped_range().to_vec();
    buffer.raw().unmap();
    bytes
}

#[test]
fn ordered_managed_compute_and_render_reach_real_native_outputs() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping native pass output proof");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let submissions = GpuSubmissionQueue::new(&device).unwrap();
    let raw_device = device.wgpu_device();
    let shader_cache = ShaderCache::new(&resources, NonZeroUsize::new(2).unwrap());

    let storage = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("pass compute output"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
        .unwrap();
    let compute_readback = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("pass compute readback"),
            size: 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
        .unwrap();
    let compute_bind_group_layout = resources
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pass compute bindings"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(4),
                },
                count: None,
            }],
        })
        .unwrap();
    let compute_entries = [GpuBindGroupEntry::buffer(0, storage.clone())];
    let compute_bind_group = resources
        .create_bind_group(GpuBindGroupDescriptor {
            label: Some("pass compute bindings"),
            layout: &compute_bind_group_layout,
            entries: &compute_entries,
        })
        .unwrap();
    let compute_layouts = [&compute_bind_group_layout];
    let compute_layout = resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some("pass compute layout"),
            bind_group_layouts: &compute_layouts,
            push_constant_ranges: &[],
        })
        .unwrap();
    let compute_shader = pollster::block_on(shader_cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("pass compute shader"),
        source: r#"
@group(0) @binding(0) var<storage, read_write> result: array<u32>;

@compute @workgroup_size(1)
fn main() {
    result[0] = 0x12345678u;
}
"#,
    }))
    .unwrap();
    let compute_pipeline = pollster::block_on(resources.create_compute_pipeline(
        GpuComputePipelineDescriptor {
            label: Some("pass compute pipeline"),
            layout: Some(&compute_layout),
            module: &compute_shader,
            entry_point: "main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        },
    ))
    .unwrap();

    let render_target = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("pass render target"),
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
    let render_view = resources
        .create_texture_view(&render_target, &wgpu::TextureViewDescriptor::default())
        .unwrap();
    let render_readback = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("pass render readback"),
            size: 256 * 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
        .unwrap();
    let render_layout = resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some("pass render layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        })
        .unwrap();
    let render_shader = pollster::block_on(shader_cache.compile_wgsl(GpuShaderModuleDescriptor {
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
    let color_targets = [Some(wgpu::ColorTargetState {
        format: wgpu::TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    let render_pipeline = pollster::block_on(resources.create_render_pipeline(
        GpuRenderPipelineDescriptor {
            label: Some("pass render pipeline"),
            layout: Some(&render_layout),
            vertex: GpuVertexState {
                module: &render_shader,
                entry_point: "vertex_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(GpuFragmentState {
                module: &render_shader,
                entry_point: "fragment_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &color_targets,
            }),
            multiview: None,
            cache: None,
        },
    ))
    .unwrap();

    let mut compute = GpuComputePassPlan::new("compute output");
    compute.push_command(GpuComputePassCommand::SetPipeline(compute_pipeline));
    compute.push_command(GpuComputePassCommand::SetBindGroup {
        index: 0,
        bind_group: compute_bind_group,
        dynamic_offsets: Vec::new(),
    });
    compute.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });

    let mut render = GpuRenderPassPlan::new("render output");
    render.push_color_attachment(GpuColorAttachment::new(
        render_view,
        wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        },
    ));
    render.push_command(GpuRenderPassCommand::SetPipeline(render_pipeline));
    render.push_command(GpuRenderPassCommand::Draw {
        vertices: 0..3,
        instances: 0..1,
    });

    let mut encoder = resources.create_pass_encoder(Some("native output batch"));
    encoder.encode_compute(compute).unwrap();
    encoder.encode_render(render).unwrap();
    let submission = submissions
        .submit_pass_batch(encoder.finish().unwrap())
        .unwrap();
    assert_eq!(submission.passes()[0].kind(), GpuPassKind::Compute);
    assert_eq!(submission.passes()[1].kind(), GpuPassKind::Render);

    let mut readback_encoder = raw_device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("pass output readback"),
    });
    readback_encoder.copy_buffer_to_buffer(storage.raw(), 0, compute_readback.raw(), 0, 4);
    readback_encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: render_target.raw(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: render_readback.raw(),
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(4),
            },
        },
        render_target.info().size(),
    );
    let readback_fence = submissions
        .submit([readback_encoder.finish()], submissions.resources())
        .unwrap();
    submissions.wait(&readback_fence).unwrap();

    assert_eq!(
        &read_buffer(&device, &compute_readback)[..4],
        &[0x78, 0x56, 0x34, 0x12]
    );
    assert_eq!(
        &read_buffer(&device, &render_readback)[..4],
        &[64, 128, 191, 255]
    );
}
