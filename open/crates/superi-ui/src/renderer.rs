//! Shared wgpu compositor for native presentation and private headless capture.

use std::borrow::Cow;

use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::readback::{TextureReadbackManager, TextureReadbackRequest};
use superi_gpu::resource::GpuResources;
use superi_gpu::submission::{GpuSubmissionQueue, GpuSubmissionResources};
use superi_gpu::wgpu;

use crate::paint::{CpuPainter, RasterFrame};
use crate::scene::Scene;
use crate::{Result, UiError};

const COMPOSITOR_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let position = positions[index];
    var output: VertexOutput;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = vec2<f32>(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5);
    return output;
}

@group(0) @binding(0)
var source_texture: texture_2d<f32>;

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let dimensions = textureDimensions(source_texture);
    let maximum = vec2<f32>(dimensions) - vec2<f32>(1.0);
    let coordinate = vec2<i32>(clamp(input.uv * vec2<f32>(dimensions), vec2<f32>(0.0), maximum));
    return textureLoad(source_texture, coordinate, 0);
}
"#;

/// Product pixels returned by the private headless wgpu path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeadlessGpuFrame {
    frame: RasterFrame,
    adapter_name: String,
    target_format: &'static str,
}

impl HeadlessGpuFrame {
    /// Returns tightly packed rendered pixels.
    #[must_use]
    pub const fn frame(&self) -> &RasterFrame {
        &self.frame
    }

    /// Returns the selected adapter's public name.
    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Returns the pinned headless target format.
    #[must_use]
    pub const fn target_format(&self) -> &'static str {
        self.target_format
    }
}

/// One encoded product frame and every resource that must survive submission.
pub struct EncodedSceneFrame {
    command_buffer: wgpu::CommandBuffer,
    source_texture: wgpu::Texture,
    staging_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    pipeline_layout: wgpu::PipelineLayout,
    bind_group_layout: wgpu::BindGroupLayout,
    shader: wgpu::ShaderModule,
}

impl EncodedSceneFrame {
    /// Converts retained compositor resources into queue-scoped submission ownership.
    pub fn into_submission<'device>(
        self,
        submissions: &GpuSubmissionQueue<'device>,
    ) -> (wgpu::CommandBuffer, GpuSubmissionResources<'device>) {
        let mut retained = submissions.resources();
        retained.retain(self.source_texture);
        retained.retain(self.staging_buffer);
        retained.retain(self.bind_group);
        retained.retain(self.pipeline);
        retained.retain(self.pipeline_layout);
        retained.retain(self.bind_group_layout);
        retained.retain(self.shader);
        (self.command_buffer, retained)
    }
}

/// Encodes one already prepared product frame into any compatible target view.
pub fn encode_raster_to_view(
    device: &GpuDevice,
    frame: &RasterFrame,
    target: &wgpu::TextureView,
    target_format: wgpu::TextureFormat,
) -> Result<EncodedSceneFrame> {
    device
        .ensure_available()
        .map_err(|error| UiError::Gpu(error.to_string()))?;
    let width = frame.width();
    let height = frame.height();
    let tight_row = width
        .checked_mul(4)
        .ok_or_else(|| UiError::Invalid("compositor row size is exhausted".to_owned()))?;
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_row = tight_row
        .checked_add(alignment - 1)
        .map(|value| value / alignment * alignment)
        .ok_or_else(|| UiError::Invalid("compositor row alignment is exhausted".to_owned()))?;
    let staging_size = u64::from(padded_row)
        .checked_mul(u64::from(height))
        .ok_or_else(|| UiError::Invalid("compositor staging size is exhausted".to_owned()))?;
    let mut padded = vec![
        0_u8;
        usize::try_from(staging_size).map_err(|_| {
            UiError::Invalid("compositor staging allocation exceeds this process".to_owned())
        })?
    ];
    for row in 0..height as usize {
        let source_start = row * tight_row as usize;
        let target_start = row * padded_row as usize;
        padded[target_start..target_start + tight_row as usize]
            .copy_from_slice(&frame.pixels()[source_start..source_start + tight_row as usize]);
    }

    let raw_device = device.wgpu_device();
    let staging_buffer = raw_device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("superi-ui retained rgba staging"),
        size: staging_size,
        usage: wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: true,
    });
    {
        let mut mapped = staging_buffer.slice(..).get_mapped_range_mut();
        mapped.copy_from_slice(&padded);
    }
    staging_buffer.unmap();
    let source_texture = raw_device.create_texture(&wgpu::TextureDescriptor {
        label: Some("superi-ui retained rgba source"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let source_view = source_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group_layout = raw_device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("superi-ui compositor bind group layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let bind_group = raw_device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("superi-ui compositor bind group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&source_view),
        }],
    });
    let pipeline_layout = raw_device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("superi-ui compositor pipeline layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });
    let shader = raw_device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("superi-ui compositor shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(COMPOSITOR_SHADER)),
    });
    let pipeline = raw_device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("superi-ui compositor pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    });

    let mut encoder = raw_device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("superi-ui compositor encoder"),
    });
    encoder.copy_buffer_to_texture(
        wgpu::ImageCopyBuffer {
            buffer: &staging_buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::ImageCopyTexture {
            texture: &source_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("superi-ui compositor pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    Ok(EncodedSceneFrame {
        command_buffer: encoder.finish(),
        source_texture,
        staging_buffer,
        bind_group,
        pipeline,
        pipeline_layout,
        bind_group_layout,
        shader,
    })
}

/// Paints and encodes one retained scene for a native surface target.
pub fn encode_scene_to_view(
    device: &GpuDevice,
    scene: &Scene,
    target: &wgpu::TextureView,
    target_format: wgpu::TextureFormat,
) -> Result<EncodedSceneFrame> {
    let frame = CpuPainter::new().paint(scene)?;
    encode_raster_to_view(device, &frame, target, target_format)
}

/// Renders through the real wgpu compositor and reads back through the explicit inspection boundary.
pub fn render_headless(scene: &Scene) -> Result<HeadlessGpuFrame> {
    let prepared = CpuPainter::new().paint(scene)?;
    let instance = GpuInstance::new(InstanceOptions::default())
        .map_err(|error| UiError::Gpu(error.to_string()))?;
    let selected = instance
        .enumerate_adapters()
        .select(
            &AdapterSelection::default()
                .allow_software_adapter(true)
                .require_webgpu_compliance(false),
        )
        .map_err(|error| UiError::Gpu(error.to_string()))?;
    let adapter_name = selected.snapshot().info().name.clone();
    let device = pollster::block_on(
        selected.create_device(&DeviceRequest::default().with_label("superi-ui private capture")),
    )
    .map_err(|error| UiError::Gpu(error.to_string()))?;
    let resources = GpuResources::new(&device).map_err(|error| UiError::Gpu(error.to_string()))?;
    let submissions =
        GpuSubmissionQueue::new(&device).map_err(|error| UiError::Gpu(error.to_string()))?;
    let extent = wgpu::Extent3d {
        width: prepared.width(),
        height: prepared.height(),
        depth_or_array_layers: 1,
    };
    let output = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("superi-ui private capture target"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .map_err(|error| UiError::Gpu(error.to_string()))?;
    let output_view = output
        .raw()
        .create_view(&wgpu::TextureViewDescriptor::default());
    let encoded = encode_raster_to_view(
        &device,
        &prepared,
        &output_view,
        wgpu::TextureFormat::Rgba8Unorm,
    )?;
    let (command_buffer, retained) = encoded.into_submission(&submissions);
    let fence = submissions
        .submit([command_buffer], retained)
        .map_err(|error| UiError::Gpu(error.to_string()))?;
    submissions
        .wait(&fence)
        .map_err(|error| UiError::Gpu(error.to_string()))?;

    let manager = TextureReadbackManager::new(resources);
    let readback = manager
        .encode(TextureReadbackRequest::for_inspection(
            output,
            wgpu::Origin3d::ZERO,
            extent,
        ))
        .map_err(|error| UiError::Gpu(error.to_string()))?;
    let submitted = submissions
        .submit_readback(readback)
        .map_err(|error| UiError::Gpu(error.to_string()))?;
    let result = submitted
        .wait(&submissions)
        .map_err(|error| UiError::Gpu(error.to_string()))?;
    let frame =
        RasterFrame::from_rgba(prepared.width(), prepared.height(), result.bytes().to_vec())?;
    Ok(HeadlessGpuFrame {
        frame,
        adapter_name,
        target_format: "rgba8unorm",
    })
}
