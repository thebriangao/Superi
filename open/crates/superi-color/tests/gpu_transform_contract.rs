use std::num::NonZeroUsize;

use superi_color::gamut::{ChromaticAdaptation, GamutMapping, WideGamutTransform};
use superi_color::gpu_transform::GpuWideGamutTransform;
use superi_core::color_space::ColorPrimaries;
use superi_core::error::ErrorCategory;
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::readback::{TextureReadbackManager, TextureReadbackRequest};
use superi_gpu::resource::GpuResources;
use superi_gpu::shader::ShaderCache;
use superi_gpu::submission::GpuSubmissionQueue;
use superi_gpu::texture::GpuTexture;
use superi_gpu::wgpu;
use superi_image::value::ImageSamples;

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(adapter.create_device(&DeviceRequest::default())).ok()
}

#[test]
fn production_transform_is_built_from_the_cpu_reference_contract() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping native color transform proof");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let shaders = ShaderCache::new(&resources, NonZeroUsize::new(2).unwrap());
    let reference = WideGamutTransform::new(
        ColorPrimaries::AcesAp1,
        ColorPrimaries::Bt2020,
        ChromaticAdaptation::Bradford,
        GamutMapping::PreserveLuminance,
    )
    .unwrap();

    let production =
        pollster::block_on(GpuWideGamutTransform::new(&resources, &shaders, reference)).unwrap();

    assert_eq!(production.reference(), reference);
    assert_eq!(
        production.texture_format(),
        superi_gpu::wgpu::TextureFormat::Rgba16Float
    );
    assert_eq!(production.workgroup_size(), [8, 8, 1]);
}

#[test]
fn production_transform_rejects_noncanonical_source_textures() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping native color transform proof");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let shaders = ShaderCache::new(&resources, NonZeroUsize::new(2).unwrap());
    let reference = reference_transform();
    let production =
        pollster::block_on(GpuWideGamutTransform::new(&resources, &shaders, reference)).unwrap();
    let source = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("invalid color transform source"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
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

    let error = production.encode(&resources, source).unwrap_err();

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn gpu_output_matches_the_cpu_reference_after_rgba16f_quantization() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping native color transform proof");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let shaders = ShaderCache::new(&resources, NonZeroUsize::new(2).unwrap());
    let submissions = GpuSubmissionQueue::new(&device).unwrap();
    let reference = reference_transform();
    let production =
        pollster::block_on(GpuWideGamutTransform::new(&resources, &shaders, reference)).unwrap();
    let input = input_pixels();
    let (source, upload_fence) = upload_rgba16f(&device, &resources, &submissions, &input);

    let encoded = production.encode(&resources, source).unwrap();
    assert_eq!(encoded.batch().passes().len(), 1);
    let submitted = encoded.submit(&submissions).unwrap();
    assert!(submitted.submission().fence().value() > upload_fence.value());
    let (output, transform_submission) = submitted.into_parts();
    let readback = TextureReadbackManager::new(resources.clone())
        .encode(TextureReadbackRequest::for_export(
            output,
            wgpu::Origin3d::ZERO,
            wgpu::Extent3d {
                width: 32,
                height: 1,
                depth_or_array_layers: 1,
            },
        ))
        .unwrap();
    let result = submissions
        .submit_readback(readback)
        .unwrap()
        .wait(&submissions)
        .unwrap();
    assert!(result.bytes().len() == 32 * 4 * 2);
    assert!(transform_submission.fence().is_signaled());

    let actual_bits = result
        .bytes()
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    let actual = ImageSamples::from_f16_bits(actual_bits);
    let quantized_input = ImageSamples::f16_from_f32(input);
    for pixel in 0..32 {
        let start = pixel * 4;
        let rgba = std::array::from_fn(|channel| {
            f64::from(quantized_input.float_value(start + channel).unwrap())
        });
        let expected = reference.apply_premultiplied_rgba(rgba).unwrap();
        for (channel, expected) in expected.into_iter().enumerate() {
            let actual = f64::from(actual.float_value(start + channel).unwrap());
            assert!(
                (actual - expected).abs() <= 0.001,
                "pixel {pixel} channel {channel}: expected {expected}, got {actual}"
            );
        }
    }
}

fn reference_transform() -> WideGamutTransform {
    WideGamutTransform::new(
        ColorPrimaries::AcesAp1,
        ColorPrimaries::Bt2020,
        ChromaticAdaptation::Bradford,
        GamutMapping::PreserveLuminance,
    )
    .unwrap()
}

fn input_pixels() -> Vec<f32> {
    (0..32)
        .flat_map(|index| {
            if index == 0 {
                return [0.0, 0.0, 0.0, 0.0];
            }
            let alpha = [0.25, 0.5, 1.0][index % 3];
            let red = (index as f32 / 31.0) * alpha;
            let green = ((31 - index) as f32 / 31.0) * alpha;
            let blue = ((index * 7 % 31) as f32 / 31.0) * alpha;
            [red, green, blue, alpha]
        })
        .collect()
}

fn upload_rgba16f(
    device: &GpuDevice,
    resources: &GpuResources<'_>,
    submissions: &GpuSubmissionQueue<'_>,
    input: &[f32],
) -> (GpuTexture, superi_gpu::submission::GpuFence) {
    let samples = ImageSamples::f16_from_f32(input.iter().copied());
    let bytes = samples
        .f16_bits()
        .unwrap()
        .iter()
        .flat_map(|bits| bits.to_le_bytes())
        .collect::<Vec<_>>();
    assert_eq!(bytes.len(), 256);
    let staging = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("color transform test upload"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        })
        .unwrap();
    staging
        .raw()
        .slice(..)
        .get_mapped_range_mut()
        .copy_from_slice(&bytes);
    staging.raw().unmap();
    let source = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("color transform test source"),
            size: wgpu::Extent3d {
                width: 32,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .unwrap();
    let mut encoder =
        device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("color transform test upload"),
            });
    encoder.copy_buffer_to_texture(
        wgpu::ImageCopyBuffer {
            buffer: staging.raw(),
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        wgpu::ImageCopyTexture {
            texture: source.raw(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        source.info().size(),
    );
    let mut retained = submissions.resources();
    retained.retain(staging);
    retained.retain(source.clone());
    let fence = submissions.submit([encoder.finish()], retained).unwrap();
    (source, fence)
}
