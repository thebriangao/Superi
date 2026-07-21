use superi_color::gamut::{ChromaticAdaptation, GamutMapping, LinearRgb, WideGamutTransform};
use superi_color::gpu_display::{DisplayViewport, GpuDisplayPresenter, GpuDisplayView};
use superi_color::transform_out::{OutputColorTransform, OutputTargetKind, OutputTransformOptions};
use superi_color::working_space::WorkingSpace;
use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::readback::{TextureReadbackManager, TextureReadbackRequest};
use superi_gpu::resource::GpuResources;
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

fn display_transform() -> OutputColorTransform {
    OutputColorTransform::new(
        OutputTargetKind::Display,
        WorkingSpace::ACESCG,
        ColorSpace::SRGB,
        OutputTransformOptions::new(),
    )
    .unwrap()
}

#[test]
fn aspect_fit_is_resolution_independent_and_centered() {
    let fit = DisplayViewport::aspect_fit(
        wgpu::Extent3d {
            width: 7_680,
            height: 4_320,
            depth_or_array_layers: 1,
        },
        wgpu::Extent3d {
            width: 2_048,
            height: 2_048,
            depth_or_array_layers: 1,
        },
    )
    .unwrap();

    assert_eq!(fit.x(), 0.0);
    assert_eq!(fit.y(), 448.0);
    assert_eq!(fit.width(), 2_048.0);
    assert_eq!(fit.height(), 1_152.0);

    let error = DisplayViewport::aspect_fit(
        wgpu::Extent3d {
            width: 0,
            height: 1,
            depth_or_array_layers: 1,
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn presenter_is_derived_from_one_explicit_display_transform() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping native display presenter proof");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let reference = display_transform();

    let presenter =
        GpuDisplayPresenter::new(&resources, reference, wgpu::TextureFormat::Bgra8UnormSrgb)
            .unwrap();

    assert_eq!(presenter.reference(), reference);
    assert_eq!(
        presenter.target_format(),
        wgpu::TextureFormat::Bgra8UnormSrgb
    );
    assert_eq!(presenter.source_format(), wgpu::TextureFormat::Rgba16Float);
    assert_eq!(presenter.view(), GpuDisplayView::Image);
}

#[test]
fn display_views_have_stable_codes_and_analysis_stages() {
    assert_eq!(
        GpuDisplayView::ALL,
        &[
            GpuDisplayView::Image,
            GpuDisplayView::Alpha,
            GpuDisplayView::Red,
            GpuDisplayView::Green,
            GpuDisplayView::Blue,
            GpuDisplayView::Luminance,
            GpuDisplayView::FalseColor,
            GpuDisplayView::Clipping,
        ]
    );
    assert_eq!(
        GpuDisplayView::ALL
            .iter()
            .map(|view| view.code())
            .collect::<Vec<_>>(),
        [
            "image",
            "alpha",
            "red",
            "green",
            "blue",
            "luminance",
            "false_color",
            "clipping",
        ]
    );
    assert_eq!(
        GpuDisplayView::Image.analysis_stage(),
        "source_scene_linear"
    );
    assert_eq!(GpuDisplayView::Clipping.analysis_stage(), "display_linear");
}

#[test]
fn every_analysis_view_matches_the_cpu_reference_through_the_real_presenter() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping display analysis pixel proof");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let submissions = GpuSubmissionQueue::new(&device).unwrap();
    let reference = display_transform();
    let input = analysis_pixels();
    let quantized_input = ImageSamples::f16_from_f32(input.iter().copied());
    let source = upload_rgba16f(&device, &resources, &submissions, &input);

    for view in GpuDisplayView::ALL {
        let presenter = GpuDisplayPresenter::new_with_view(
            &resources,
            reference,
            wgpu::TextureFormat::Rgba8Unorm,
            *view,
        )
        .unwrap();
        assert_eq!(presenter.view(), *view);
        let actual = render_and_read(&resources, &submissions, &presenter, source.clone());
        for (pixel, actual_pixel) in actual.iter().enumerate() {
            let rgba = std::array::from_fn(|channel| {
                f64::from(quantized_input.float_value(pixel * 4 + channel).unwrap())
            });
            let expected = reference_pixel(reference, *view, rgba);
            for channel in 0..4 {
                assert!(
                    actual_pixel[channel].abs_diff(expected[channel]) <= 2,
                    "view {} pixel {pixel} channel {channel}: expected {}, got {}",
                    view.code(),
                    expected[channel],
                    actual_pixel[channel],
                );
            }
        }
    }
}

fn analysis_pixels() -> Vec<f32> {
    let straight = [
        [0.0, 0.0, 0.0, 0.0],
        [0.2, 0.4, 0.6, 0.25],
        [-0.25, -0.25, -0.25, 1.0],
        [2.0, 2.0, 2.0, 1.0],
        [-0.5, 2.0, 0.2, 1.0],
        [0.01, 0.01, 0.01, 1.0],
        [0.03, 0.03, 0.03, 1.0],
        [0.1, 0.1, 0.1, 1.0],
        [0.5, 0.5, 0.5, 1.0],
        [1.5, 1.5, 1.5, 1.0],
        [3.0, 3.0, 3.0, 1.0],
        [5.0, 5.0, 5.0, 1.0],
        [0.18, 0.18, 0.18, 1.0],
        [0.9, 0.1, 0.3, 0.5],
        [0.001, 0.7, 0.2, 0.75],
        [0.4, 0.2, 0.8, 1.0],
    ];
    straight
        .into_iter()
        .cycle()
        .take(32)
        .flat_map(|rgba| {
            let alpha = rgba[3];
            [rgba[0] * alpha, rgba[1] * alpha, rgba[2] * alpha, alpha]
        })
        .collect()
}

fn reference_pixel(
    reference: OutputColorTransform,
    view: GpuDisplayView,
    rgba: [f64; 4],
) -> [u8; 4] {
    let alpha = rgba[3];
    let straight = if alpha > 0.0 {
        [rgba[0] / alpha, rgba[1] / alpha, rgba[2] / alpha]
    } else {
        [0.0; 3]
    };
    let source_luma = WideGamutTransform::new(
        reference.source().color_space().primaries(),
        reference.source().color_space().primaries(),
        ChromaticAdaptation::None,
        GamutMapping::Preserve,
    )
    .unwrap()
    .destination_luma();
    let luminance = dot(source_luma, straight);
    let analyzed = match view {
        GpuDisplayView::Image | GpuDisplayView::Clipping => straight,
        GpuDisplayView::Alpha => [alpha; 3],
        GpuDisplayView::Red => [straight[0]; 3],
        GpuDisplayView::Green => [straight[1]; 3],
        GpuDisplayView::Blue => [straight[2]; 3],
        GpuDisplayView::Luminance => [luminance; 3],
        GpuDisplayView::FalseColor => false_color(luminance),
    };
    let mut display = reference
        .gamut_transform()
        .apply_rgb(LinearRgb::new(analyzed).unwrap())
        .unwrap()
        .values();
    if view == GpuDisplayView::Clipping {
        let under = display.into_iter().any(|component| component < 0.0);
        let over = display.into_iter().any(|component| component > 1.0);
        display = match (under, over) {
            (true, true) => [1.0, 0.0, 1.0],
            (true, false) => [0.0, 0.0, 1.0],
            (false, true) => [1.0, 0.0, 0.0],
            (false, false) => {
                let luminance =
                    dot(reference.gamut_transform().destination_luma(), display).clamp(0.0, 1.0);
                [luminance; 3]
            }
        };
    }
    let coverage = if view == GpuDisplayView::Image {
        alpha
    } else {
        1.0
    };
    let mut output = [0_u8; 4];
    for channel in 0..3 {
        output[channel] = unorm8(srgb_encode(display[channel]) * coverage);
    }
    output[3] = 255;
    output
}

fn false_color(luminance: f64) -> [f64; 3] {
    if luminance < 1.0 / 64.0 {
        [0.25, 0.0, 0.5]
    } else if luminance < 1.0 / 16.0 {
        [0.0, 0.0, 1.0]
    } else if luminance < 1.0 / 4.0 {
        [0.0, 1.0, 1.0]
    } else if luminance < 1.0 {
        [0.0, 1.0, 0.0]
    } else if luminance < 2.0 {
        [1.0, 1.0, 0.0]
    } else if luminance < 4.0 {
        [1.0, 0.25, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    }
}

fn srgb_encode(value: f64) -> f64 {
    if value.abs() <= 0.003_130_8 {
        12.92 * value
    } else {
        value.signum() * (1.055 * value.abs().powf(1.0 / 2.4) - 0.055)
    }
}

fn unorm8(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn upload_rgba16f(
    device: &GpuDevice,
    resources: &GpuResources<'_>,
    submissions: &GpuSubmissionQueue<'_>,
    input: &[f32],
) -> GpuTexture {
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
            label: Some("display analysis test upload"),
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
            label: Some("display analysis test source"),
            size: extent(),
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
                label: Some("display analysis test upload"),
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
        extent(),
    );
    let mut retained = submissions.resources();
    retained.retain(staging);
    retained.retain(source.clone());
    let fence = submissions.submit([encoder.finish()], retained).unwrap();
    submissions.wait(&fence).unwrap();
    source
}

fn render_and_read(
    resources: &GpuResources<'_>,
    submissions: &GpuSubmissionQueue<'_>,
    presenter: &GpuDisplayPresenter<'_>,
    source: GpuTexture,
) -> Vec<[u8; 4]> {
    let target = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("display analysis test target"),
            size: extent(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .unwrap();
    let target_view = target
        .raw()
        .create_view(&wgpu::TextureViewDescriptor::default());
    let prepared = presenter.prepare_source(source).unwrap();
    let encoded = presenter.encode(&prepared, &target_view, extent()).unwrap();
    let fence = encoded.submit(submissions).unwrap();
    submissions.wait(&fence).unwrap();
    let readback = TextureReadbackManager::new(resources.clone())
        .encode(TextureReadbackRequest::for_export(
            target,
            wgpu::Origin3d::ZERO,
            extent(),
        ))
        .unwrap();
    let result = submissions
        .submit_readback(readback)
        .unwrap()
        .wait(submissions)
        .unwrap();
    result
        .bytes()
        .chunks_exact(4)
        .map(|rgba| [rgba[0], rgba[1], rgba[2], rgba[3]])
        .collect()
}

const fn extent() -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: 32,
        height: 1,
        depth_or_array_layers: 1,
    }
}
