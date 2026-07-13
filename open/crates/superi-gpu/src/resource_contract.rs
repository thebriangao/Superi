use std::num::NonZeroU64;
use std::sync::mpsc;

use crate::binding::{GpuBindGroupDescriptor, GpuBindGroupEntry, GpuPipelineLayoutDescriptor};
use crate::device::{AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions};
use crate::pipeline::{GpuComputePipelineDescriptor, GpuRenderPipelineDescriptor};
use crate::resource::{GpuResourceKind, GpuResources};
use crate::wgpu;
use superi_core::error::{ErrorCategory, Recoverability};

fn assert_send_sync<T: Send + Sync>() {}

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(
        adapter.create_device(&DeviceRequest::default().with_label("superi-gpu resource contract")),
    )
    .ok()
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
fn managed_resources_execute_compute_and_render_with_retained_lifetimes() {
    assert_send_sync::<GpuResources<'static>>();
    assert_send_sync::<crate::buffer::GpuBuffer>();
    assert_send_sync::<crate::texture::GpuTexture>();
    assert_send_sync::<crate::binding::GpuBindGroup>();
    assert_send_sync::<crate::pipeline::GpuRenderPipeline>();

    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping hardware execution");
        return;
    };
    let adapter_info = device.adapter().info();
    assert!(!adapter_info.name.is_empty());
    #[cfg(target_os = "macos")]
    assert_eq!(adapter_info.backend, wgpu::Backend::Metal);
    let resources = GpuResources::new(&device).unwrap();
    let raw_device = device.wgpu_device();

    let storage = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("sample result"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
        .unwrap();
    let compute_readback = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("compute readback"),
            size: 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
        .unwrap();
    assert_eq!(storage.info().size(), 4);
    assert!(storage.info().usage().contains(wgpu::BufferUsages::STORAGE));
    assert_eq!(storage.id().kind(), GpuResourceKind::Buffer);
    assert!(storage.id().sequence() < compute_readback.id().sequence());

    let source_texture = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("sample source"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
        .unwrap();
    let source_view = resources
        .create_texture_view(&source_texture, &wgpu::TextureViewDescriptor::default())
        .unwrap();
    let sampler = resources
        .create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sample source"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        })
        .unwrap();
    assert_eq!(
        source_texture.info().format(),
        wgpu::TextureFormat::Rgba8Unorm
    );
    assert_eq!(source_view.texture().id(), source_texture.id());
    assert_eq!(
        sampler.info().filters(),
        (
            wgpu::FilterMode::Nearest,
            wgpu::FilterMode::Nearest,
            wgpu::FilterMode::Nearest
        )
    );
    let upload = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("sample source upload"),
            size: 256,
            usage: wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        })
        .unwrap();
    upload.raw().slice(..4).get_mapped_range_mut()[..4].copy_from_slice(&[255, 0, 0, 255]);
    upload.raw().unmap();
    let mut upload_encoder = raw_device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("sample source upload"),
    });
    upload_encoder.copy_buffer_to_texture(
        wgpu::ImageCopyBuffer {
            buffer: upload.raw(),
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        wgpu::ImageCopyTexture {
            texture: source_texture.raw(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        source_texture.info().size(),
    );
    device.submit_viewport([upload_encoder.finish()]);
    drop(upload);

    let bind_group_layout = resources
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sample bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(4),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
        .unwrap();
    assert_eq!(bind_group_layout.info().entries().len(), 3);
    let bind_group_entries = [
        GpuBindGroupEntry::buffer(0, storage.clone()),
        GpuBindGroupEntry::texture_view(1, source_view.clone()),
        GpuBindGroupEntry::sampler(2, sampler.clone()),
    ];
    let bind_group = resources
        .create_bind_group(GpuBindGroupDescriptor {
            label: Some("sample bindings"),
            layout: &bind_group_layout,
            entries: &bind_group_entries,
        })
        .unwrap();
    let compute_layouts = [&bind_group_layout];
    let compute_layout = resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some("sample compute layout"),
            bind_group_layouts: &compute_layouts,
            push_constant_ranges: &[],
        })
        .unwrap();
    assert_eq!(
        compute_layout.info().bind_group_layouts()[0].id(),
        bind_group_layout.id()
    );

    drop(bind_group_entries);
    drop(source_view);
    drop(source_texture);
    drop(sampler);
    assert_eq!(resources.stats().count(GpuResourceKind::Texture), 1);
    assert_eq!(resources.stats().count(GpuResourceKind::TextureView), 1);
    assert_eq!(resources.stats().count(GpuResourceKind::Sampler), 1);

    let compute_shader = raw_device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sample compute"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
@group(0) @binding(0) var<storage, read_write> result: array<u32>;
@group(0) @binding(1) var source_texture: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;

@compute @workgroup_size(1)
fn main() {
    let sampled = textureSampleLevel(source_texture, source_sampler, vec2<f32>(0.5, 0.5), 0.0);
    result[0] = bitcast<u32>(sampled.r);
}
"#
            .into(),
        ),
    });
    let compute_pipeline = resources
        .create_compute_pipeline(GpuComputePipelineDescriptor {
            label: Some("sample compute"),
            layout: Some(&compute_layout),
            module: &compute_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
        .unwrap();
    assert_eq!(
        compute_pipeline.info().explicit_layout(),
        Some(compute_layout.id())
    );
    drop(bind_group_layout);
    assert_eq!(resources.stats().count(GpuResourceKind::BindGroupLayout), 1);

    let mut compute_encoder = raw_device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("managed compute contract"),
    });
    {
        let mut pass = compute_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("sample compute"),
            timestamp_writes: None,
        });
        pass.set_pipeline(compute_pipeline.raw());
        pass.set_bind_group(0, bind_group.raw(), &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    compute_encoder.copy_buffer_to_buffer(storage.raw(), 0, compute_readback.raw(), 0, 4);
    device.submit_viewport([compute_encoder.finish()]);

    let compute_bytes = read_buffer(&device, &compute_readback);
    assert_eq!(
        u32::from_le_bytes(compute_bytes[..4].try_into().unwrap()),
        1.0_f32.to_bits()
    );
    drop(bind_group);
    assert_eq!(resources.stats().count(GpuResourceKind::Texture), 0);
    assert_eq!(resources.stats().count(GpuResourceKind::TextureView), 0);
    assert_eq!(resources.stats().count(GpuResourceKind::Sampler), 0);
    drop(compute_layout);
    assert_eq!(resources.stats().count(GpuResourceKind::PipelineLayout), 1);
    assert_eq!(resources.stats().count(GpuResourceKind::BindGroupLayout), 1);
    drop(compute_pipeline);
    assert_eq!(resources.stats().count(GpuResourceKind::PipelineLayout), 0);
    assert_eq!(resources.stats().count(GpuResourceKind::BindGroupLayout), 0);

    let render_target = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("render target"),
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
            label: Some("render readback"),
            size: 256 * 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
        .unwrap();
    let render_layout = resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some("render layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        })
        .unwrap();
    let render_shader = raw_device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("solid render"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

@fragment
fn fragment_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.25, 0.5, 0.75, 1.0);
}
"#
            .into(),
        ),
    });
    let color_targets = [Some(wgpu::ColorTargetState {
        format: wgpu::TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    let render_pipeline = resources
        .create_render_pipeline(GpuRenderPipelineDescriptor {
            label: Some("solid render"),
            layout: Some(&render_layout),
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &color_targets,
            }),
            multiview: None,
            cache: None,
        })
        .unwrap();
    drop(render_layout);
    assert_eq!(resources.stats().count(GpuResourceKind::PipelineLayout), 1);

    let mut encoder = raw_device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("managed render contract"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("solid render"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: render_view.raw(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(render_pipeline.raw());
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
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
    device.submit_viewport([encoder.finish()]);

    let render_bytes = read_buffer(&device, &render_readback);
    assert_eq!(&render_bytes[..4], &[64, 128, 191, 255]);
    drop(render_pipeline);
    assert_eq!(resources.stats().count(GpuResourceKind::PipelineLayout), 0);
}

#[test]
fn resources_from_another_device_lifetime_are_rejected_before_wgpu_submission() {
    let Some(first_device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping hardware ownership check");
        return;
    };
    let Some(second_device) = test_device() else {
        eprintln!("a second wgpu device is unavailable, skipping recovery ownership check");
        return;
    };
    let first = GpuResources::new(&first_device).unwrap();
    let second = GpuResources::new(&second_device).unwrap();
    let foreign_buffer = first
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("foreign"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        })
        .unwrap();
    let layout = second
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("local layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(16),
                },
                count: None,
            }],
        })
        .unwrap();
    let entries = [GpuBindGroupEntry::buffer(0, foreign_buffer)];
    let error = second
        .create_bind_group(GpuBindGroupDescriptor {
            label: Some("invalid mixed lifetime"),
            layout: &layout,
            entries: &entries,
        })
        .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        error.contexts().last().unwrap().component(),
        "superi-gpu.resource"
    );
}

#[test]
fn invalid_managed_descriptors_return_shared_classified_errors() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping hardware validation check");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();

    let empty_usage = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("invalid empty usage"),
            size: 4,
            usage: wgpu::BufferUsages::empty(),
            mapped_at_creation: false,
        })
        .unwrap_err();
    assert_eq!(empty_usage.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        empty_usage.recoverability(),
        Recoverability::UserCorrectable
    );

    let invalid_sampler = resources
        .create_sampler(&wgpu::SamplerDescriptor {
            label: Some("invalid lod"),
            lod_min_clamp: 2.0,
            lod_max_clamp: 1.0,
            ..wgpu::SamplerDescriptor::default()
        })
        .unwrap_err();
    assert_eq!(invalid_sampler.category(), ErrorCategory::InvalidInput);

    let duplicate_entry = wgpu::BindGroupLayoutEntry {
        binding: 7,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(4),
        },
        count: None,
    };
    let duplicate_layout = resources
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("duplicate binding"),
            entries: &[duplicate_entry, duplicate_entry],
        })
        .unwrap_err();
    assert_eq!(duplicate_layout.category(), ErrorCategory::InvalidInput);
    assert_eq!(resources.stats().total(), 0);
}
