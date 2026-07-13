use std::num::NonZeroUsize;
use std::sync::mpsc;
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::pixel::{AlphaMode, ChromaSubsampling, PixelFormat, PixelModel};

use crate::buffer::GpuBuffer;
use crate::convert::{
    ChromaLocation, GpuConversionPlan, GpuFrameDescriptor, GpuPixelConverter, GpuPixelFrame,
};
use crate::device::{AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions};
use crate::resource::{GpuResourceKind, GpuResources};
use crate::shader::ShaderCache;
use crate::texture::GpuTextureView;
use crate::texture_pool::{TextureAlignment, TexturePoolConfig};
use crate::upload::{DecodedFrameUpload, DecodedFrameUploader, DecodedPlane, UploadConfig};
use crate::wgpu;

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(
        adapter.create_device(
            &DeviceRequest::default().with_label("superi pixel conversion contract"),
        ),
    )
    .ok()
}

fn pixel_converter<'device>(
    resources: GpuResources<'device>,
    plan: GpuConversionPlan,
) -> superi_core::error::Result<GpuPixelConverter<'device>> {
    pollster::block_on(GpuPixelConverter::new(resources, plan))
}

fn frame<'device>(
    resources: &GpuResources<'device>,
    descriptor: GpuFrameDescriptor,
    usage: wgpu::TextureUsages,
) -> GpuPixelFrame<'device> {
    let views = descriptor
        .plane_layouts()
        .iter()
        .enumerate()
        .map(|(index, layout)| {
            let label = format!("conversion contract plane {index}");
            let texture = resources
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(&label),
                    size: wgpu::Extent3d {
                        width: layout.texture_width(),
                        height: layout.texture_height(),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: layout.texture_format(),
                    usage,
                    view_formats: &[],
                })
                .unwrap();
            resources
                .create_texture_view(
                    &texture,
                    &wgpu::TextureViewDescriptor {
                        label: Some(&label),
                        ..Default::default()
                    },
                )
                .unwrap()
        })
        .collect();
    GpuPixelFrame::new(descriptor, views).unwrap()
}

fn upload_frame(
    resources: &GpuResources<'_>,
    encoder: &mut wgpu::CommandEncoder,
    frame: &GpuPixelFrame<'_>,
    plane_bytes: &[Vec<u8>],
) -> Vec<GpuBuffer> {
    assert_eq!(plane_bytes.len(), frame.planes().len());
    frame
        .planes()
        .iter()
        .zip(frame.descriptor().plane_layouts())
        .zip(plane_bytes)
        .enumerate()
        .map(|(index, ((view, layout), bytes))| {
            let bytes_per_texel = layout
                .texture_format()
                .block_copy_size(None)
                .expect("conversion planes use uncompressed formats");
            let tight_row = layout.texture_width() * bytes_per_texel;
            assert_eq!(bytes.len(), (tight_row * layout.texture_height()) as usize);
            let padded_row = tight_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
                * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
            let size = u64::from(padded_row) * u64::from(layout.texture_height());
            let label = format!("conversion upload plane {index}");
            let staging = resources
                .create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&label),
                    size,
                    usage: wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                })
                .unwrap();
            {
                let mut mapped = staging.raw().slice(..).get_mapped_range_mut();
                for row in 0..layout.texture_height() as usize {
                    let source = row * tight_row as usize;
                    let destination = row * padded_row as usize;
                    mapped[destination..destination + tight_row as usize]
                        .copy_from_slice(&bytes[source..source + tight_row as usize]);
                }
            }
            staging.raw().unmap();
            encoder.copy_buffer_to_texture(
                wgpu::ImageCopyBuffer {
                    buffer: staging.raw(),
                    layout: wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row),
                        rows_per_image: Some(layout.texture_height()),
                    },
                },
                wgpu::ImageCopyTexture {
                    texture: view.texture().raw(),
                    mip_level: view.info().base_mip_level(),
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: view.info().base_array_layer(),
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: layout.texture_width(),
                    height: layout.texture_height(),
                    depth_or_array_layers: 1,
                },
            );
            staging
        })
        .collect()
}

struct PlaneReadback {
    buffer: GpuBuffer,
    tight_row: u32,
    padded_row: u32,
    height: u32,
}

fn copy_plane_to_readback(
    resources: &GpuResources<'_>,
    encoder: &mut wgpu::CommandEncoder,
    frame: &GpuPixelFrame<'_>,
    plane: usize,
) -> PlaneReadback {
    let view = &frame.planes()[plane];
    let layout = frame.descriptor().plane_layouts()[plane];
    let bytes_per_texel = layout
        .texture_format()
        .block_copy_size(None)
        .expect("conversion planes use uncompressed formats");
    let tight_row = layout.texture_width() * bytes_per_texel;
    let padded_row =
        tight_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("conversion plane readback"),
            size: u64::from(padded_row) * u64::from(layout.texture_height()),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
        .unwrap();
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: view.texture().raw(),
            mip_level: view.info().base_mip_level(),
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: view.info().base_array_layer(),
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: buffer.raw(),
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(layout.texture_height()),
            },
        },
        wgpu::Extent3d {
            width: layout.texture_width(),
            height: layout.texture_height(),
            depth_or_array_layers: 1,
        },
    );
    PlaneReadback {
        buffer,
        tight_row,
        padded_row,
        height: layout.texture_height(),
    }
}

fn read_plane(device: &GpuDevice, readback: &PlaneReadback) -> Vec<u8> {
    let slice = readback.buffer.raw().slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).expect("map receiver remains alive");
    });
    let _ = device.wgpu_device().poll(wgpu::Maintain::Wait);
    receiver
        .recv()
        .expect("mapping callback must run")
        .expect("readback mapping must succeed");
    let mapped = slice.get_mapped_range();
    let mut tight = Vec::with_capacity((readback.tight_row * readback.height) as usize);
    for row in 0..readback.height as usize {
        let start = row * readback.padded_row as usize;
        tight.extend_from_slice(&mapped[start..start + readback.tight_row as usize]);
    }
    drop(mapped);
    readback.buffer.raw().unmap();
    tight
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = (bits >> 10) & 0x1f;
    let mut mantissa = u32::from(bits & 0x03ff);
    let value = if exponent == 0 {
        if mantissa == 0 {
            sign
        } else {
            let mut unbiased = -14_i32;
            while mantissa & 0x0400 == 0 {
                mantissa <<= 1;
                unbiased -= 1;
            }
            mantissa &= 0x03ff;
            sign | (u32::try_from(unbiased + 127).unwrap() << 23) | (mantissa << 13)
        }
    } else if exponent == 0x1f {
        sign | 0x7f80_0000 | (mantissa << 13)
    } else {
        sign | ((u32::from(exponent) + 112) << 23) | (mantissa << 13)
    };
    f32::from_bits(value)
}

fn rgba16f(bytes: &[u8], pixel: usize) -> [f32; 4] {
    std::array::from_fn(|component| {
        let offset = pixel * 8 + component * 2;
        half_to_f32(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
    })
}

fn u16_bytes(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn assert_near(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected} within {tolerance}, found {actual}"
    );
}

fn rgb_color() -> ColorSpace {
    ColorSpace::new(
        ColorPrimaries::Bt709,
        TransferFunction::Srgb,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    )
}

fn yuv_color(matrix: MatrixCoefficients, range: ColorRange) -> ColorSpace {
    ColorSpace::new(
        ColorPrimaries::Bt709,
        TransferFunction::Bt709,
        matrix,
        range,
    )
}

fn descriptor(format: PixelFormat) -> GpuFrameDescriptor {
    let color = if format.model() == PixelModel::Yuv {
        yuv_color(MatrixCoefficients::Bt709, ColorRange::Limited)
    } else {
        rgb_color()
    };
    let chroma = match format.chroma_subsampling() {
        Some(ChromaSubsampling::Cs420 | ChromaSubsampling::Cs422) => Some(ChromaLocation::Left),
        Some(ChromaSubsampling::Cs444) | None => None,
        _ => unreachable!("all current chroma forms are covered"),
    };
    GpuFrameDescriptor::new(
        5,
        3,
        format,
        color,
        if format.has_alpha() {
            AlphaMode::Straight
        } else {
            AlphaMode::Opaque
        },
        chroma,
    )
    .unwrap()
}

#[test]
fn every_public_pixel_format_has_a_portable_exact_plane_layout() {
    use crate::wgpu::TextureFormat as W;

    for &format in PixelFormat::ALL {
        let descriptor = descriptor(format);
        let layouts = descriptor.plane_layouts();
        assert_eq!(
            layouts.len(),
            usize::from(format.plane_count()),
            "{format:?}"
        );

        let expected_formats: &[W] = match format {
            PixelFormat::R8Unorm => &[W::R8Unorm],
            PixelFormat::R16Unorm => &[W::R16Uint],
            PixelFormat::R16Float => &[W::R16Float],
            PixelFormat::R32Float => &[W::R32Float],
            PixelFormat::Rg8Unorm => &[W::Rg8Unorm],
            PixelFormat::Rg16Unorm => &[W::Rg16Uint],
            PixelFormat::Rg16Float => &[W::Rg16Float],
            PixelFormat::Rg32Float => &[W::Rg32Float],
            PixelFormat::Rgb8Unorm | PixelFormat::Bgr8Unorm => &[W::R8Unorm],
            PixelFormat::Rgba8Unorm => &[W::Rgba8Unorm],
            PixelFormat::Bgra8Unorm => &[W::Bgra8Unorm],
            PixelFormat::Rgba16Unorm => &[W::Rgba16Uint],
            PixelFormat::Rgba16Float => &[W::Rgba16Float],
            PixelFormat::Rgba32Float => &[W::Rgba32Float],
            PixelFormat::Yuv420p8 | PixelFormat::Yuv422p8 | PixelFormat::Yuv444p8 => {
                &[W::R8Unorm, W::R8Unorm, W::R8Unorm]
            }
            PixelFormat::Yuv420p10 | PixelFormat::Yuv422p10 | PixelFormat::Yuv444p10 => {
                &[W::R16Uint, W::R16Uint, W::R16Uint]
            }
            PixelFormat::Nv12 => &[W::R8Unorm, W::Rg8Unorm],
            PixelFormat::P010 => &[W::R16Uint, W::Rg16Uint],
            _ => unreachable!("PixelFormat::ALL contains only formats supported by this build"),
        };
        assert_eq!(
            layouts
                .iter()
                .map(|layout| layout.texture_format())
                .collect::<Vec<_>>(),
            expected_formats,
            "{format:?}"
        );

        assert_eq!((layouts[0].width(), layouts[0].height()), (5, 3));
        if matches!(format, PixelFormat::Rgb8Unorm | PixelFormat::Bgr8Unorm) {
            assert_eq!(
                (layouts[0].texture_width(), layouts[0].texture_height()),
                (15, 3)
            );
        } else {
            assert_eq!(
                (layouts[0].texture_width(), layouts[0].texture_height()),
                (5, 3)
            );
        }
        match format.chroma_subsampling() {
            Some(ChromaSubsampling::Cs420) => {
                assert_eq!((layouts[1].width(), layouts[1].height()), (3, 2));
            }
            Some(ChromaSubsampling::Cs422) => {
                assert_eq!((layouts[1].width(), layouts[1].height()), (3, 3));
            }
            Some(ChromaSubsampling::Cs444) => {
                assert_eq!((layouts[1].width(), layouts[1].height()), (5, 3));
            }
            None => {}
            _ => unreachable!("all current chroma forms are covered"),
        }
        let expected_shift = if format == PixelFormat::P010 { 6 } else { 0 };
        assert!(layouts
            .iter()
            .all(|layout| layout.stored_bit_shift() == expected_shift));
    }
}

#[test]
fn plans_reject_hidden_color_assumptions_geometry_changes_and_alpha_loss() {
    let unresolved = GpuFrameDescriptor::new(
        4,
        4,
        PixelFormat::Nv12,
        ColorSpace::UNSPECIFIED,
        AlphaMode::Opaque,
        Some(ChromaLocation::Left),
    )
    .unwrap();
    let target = GpuFrameDescriptor::new(
        4,
        4,
        PixelFormat::Rgba16Float,
        ColorSpace::new(
            ColorPrimaries::Bt709,
            TransferFunction::Bt709,
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        ),
        AlphaMode::Premultiplied,
        None,
    )
    .unwrap();
    let error = GpuConversionPlan::new(unresolved, target.clone()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        error.contexts().last().unwrap().operation(),
        "create_conversion_plan"
    );

    let different_primaries = GpuFrameDescriptor::new(
        4,
        4,
        PixelFormat::Rgba16Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
        None,
    )
    .unwrap();
    assert!(GpuConversionPlan::new(target.clone(), different_primaries).is_err());

    let different_size = GpuFrameDescriptor::new(
        5,
        4,
        PixelFormat::Rgba16Float,
        target.color_space(),
        AlphaMode::Premultiplied,
        None,
    )
    .unwrap();
    assert!(GpuConversionPlan::new(target.clone(), different_size).is_err());

    let straight = GpuFrameDescriptor::new(
        4,
        4,
        PixelFormat::Rgba8Unorm,
        rgb_color(),
        AlphaMode::Straight,
        None,
    )
    .unwrap();
    let opaque_rgb = GpuFrameDescriptor::new(
        4,
        4,
        PixelFormat::Rgb8Unorm,
        rgb_color(),
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    assert!(GpuConversionPlan::new(straight, opaque_rgb).is_err());
}

#[test]
fn every_format_plans_to_and_from_rgba16f_and_constant_luminance_is_explicit() {
    for &format in PixelFormat::ALL {
        let source = descriptor(format);
        let working_color = ColorSpace::new(
            source.color_space().primaries(),
            source.color_space().transfer(),
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        );
        let working = GpuFrameDescriptor::new(
            source.width(),
            source.height(),
            PixelFormat::Rgba16Float,
            working_color,
            if source.alpha_mode() == AlphaMode::Opaque {
                AlphaMode::Opaque
            } else {
                AlphaMode::Premultiplied
            },
            None,
        )
        .unwrap();
        assert!(
            GpuConversionPlan::new(source.clone(), working.clone()).is_ok(),
            "{format:?}"
        );
        assert!(
            GpuConversionPlan::new(working, source).is_ok(),
            "{format:?}"
        );
    }

    let source = GpuFrameDescriptor::new(
        4,
        4,
        PixelFormat::Yuv444p10,
        yuv_color(MatrixCoefficients::Bt2020Constant, ColorRange::Limited),
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    let target = GpuFrameDescriptor::new(
        4,
        4,
        PixelFormat::Rgba16Float,
        ColorSpace::new(
            ColorPrimaries::Bt709,
            TransferFunction::Bt709,
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        ),
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    assert!(GpuConversionPlan::new(source, target).is_ok());
}

#[test]
fn managed_frames_validate_exact_plane_format_geometry_and_usage_before_encoding() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping managed conversion frame contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let descriptor = descriptor(PixelFormat::Nv12);
    let valid = frame(
        &resources,
        descriptor.clone(),
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    assert_eq!(valid.descriptor(), &descriptor);
    assert_eq!(valid.planes().len(), 2);

    let wrong_texture = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("wrong conversion plane"),
            size: wgpu::Extent3d {
                width: 5,
                height: 3,
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
    let wrong_view = resources
        .create_texture_view(&wrong_texture, &wgpu::TextureViewDescriptor::default())
        .unwrap();
    let error =
        GpuPixelFrame::new(descriptor, vec![wrong_view, valid.planes()[1].clone()]).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn managed_converter_compiles_every_public_format_to_and_from_rgba16f() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping conversion pipeline contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    for &format in PixelFormat::ALL {
        let source = descriptor(format);
        let working = GpuFrameDescriptor::new(
            source.width(),
            source.height(),
            PixelFormat::Rgba16Float,
            ColorSpace::new(
                source.color_space().primaries(),
                source.color_space().transfer(),
                MatrixCoefficients::Rgb,
                ColorRange::Full,
            ),
            if source.alpha_mode() == AlphaMode::Opaque {
                AlphaMode::Opaque
            } else {
                AlphaMode::Premultiplied
            },
            None,
        )
        .unwrap();
        for plan in [
            GpuConversionPlan::new(source.clone(), working.clone()).unwrap(),
            GpuConversionPlan::new(working.clone(), source.clone()).unwrap(),
        ] {
            pixel_converter(resources.clone(), plan).unwrap();
        }
    }

    for matrix in [
        MatrixCoefficients::Bt601,
        MatrixCoefficients::Bt709,
        MatrixCoefficients::Bt2020NonConstant,
        MatrixCoefficients::Bt2020Constant,
    ] {
        for range in [ColorRange::Full, ColorRange::Limited] {
            let source = GpuFrameDescriptor::new(
                2,
                2,
                PixelFormat::Yuv444p10,
                yuv_color(matrix, range),
                AlphaMode::Opaque,
                None,
            )
            .unwrap();
            let target = GpuFrameDescriptor::new(
                2,
                2,
                PixelFormat::Rgba16Float,
                ColorSpace::new(
                    ColorPrimaries::Bt709,
                    TransferFunction::Bt709,
                    MatrixCoefficients::Rgb,
                    ColorRange::Full,
                ),
                AlphaMode::Opaque,
                None,
            )
            .unwrap();
            pixel_converter(
                resources.clone(),
                GpuConversionPlan::new(source, target).unwrap(),
            )
            .unwrap();
        }
    }

    let limited_rgb = GpuFrameDescriptor::new(
        2,
        2,
        PixelFormat::Rgba8Unorm,
        ColorSpace::new(
            ColorPrimaries::Bt709,
            TransferFunction::Srgb,
            MatrixCoefficients::Rgb,
            ColorRange::Limited,
        ),
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    let full_float = GpuFrameDescriptor::new(
        2,
        2,
        PixelFormat::Rgba16Float,
        rgb_color(),
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    pixel_converter(
        resources,
        GpuConversionPlan::new(limited_rgb, full_float).unwrap(),
    )
    .unwrap();
}

#[test]
fn conversion_reuses_canonical_managed_shaders_from_a_caller_cache() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping conversion shader cache contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let cache = ShaderCache::new(&resources, NonZeroUsize::new(2).unwrap());
    let source = GpuFrameDescriptor::new(
        2,
        2,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
        None,
    )
    .unwrap();
    let destination = GpuFrameDescriptor::new(
        2,
        2,
        PixelFormat::Rgba16Float,
        ColorSpace::SRGB,
        AlphaMode::Premultiplied,
        None,
    )
    .unwrap();
    let plan = GpuConversionPlan::new(source, destination).unwrap();

    let first = pollster::block_on(GpuPixelConverter::with_shader_cache(
        resources.clone(),
        &cache,
        plan.clone(),
    ))
    .unwrap();
    assert_eq!(cache.stats().entries(), 1);
    assert_eq!(cache.stats().misses(), 1);
    assert_eq!(cache.stats().hits(), 0);
    assert_eq!(resources.stats().count(GpuResourceKind::ShaderModule), 1);

    let second = pollster::block_on(GpuPixelConverter::with_shader_cache(
        resources.clone(),
        &cache,
        plan,
    ))
    .unwrap();
    assert_eq!(cache.stats().entries(), 1);
    assert_eq!(cache.stats().misses(), 1);
    assert_eq!(cache.stats().hits(), 1);
    assert_eq!(resources.stats().count(GpuResourceKind::ShaderModule), 1);

    drop((first, second));
    cache.clear();
    assert_eq!(resources.stats().count(GpuResourceKind::ShaderModule), 0);
}

#[test]
fn encoding_rejects_missing_usage_and_foreign_device_lifetimes() {
    let Some(first_device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping conversion ownership contract");
        return;
    };
    let Some(second_device) = test_device() else {
        eprintln!("a second wgpu device is unavailable, skipping conversion ownership contract");
        return;
    };
    let first = GpuResources::new(&first_device).unwrap();
    let second = GpuResources::new(&second_device).unwrap();
    let source_descriptor = GpuFrameDescriptor::new(
        1,
        1,
        PixelFormat::Rgba8Unorm,
        rgb_color(),
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    let target_descriptor = GpuFrameDescriptor::new(
        1,
        1,
        PixelFormat::Rgba16Float,
        rgb_color(),
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    let converter = pixel_converter(
        first.clone(),
        GpuConversionPlan::new(source_descriptor.clone(), target_descriptor.clone()).unwrap(),
    )
    .unwrap();
    let target = frame(
        &first,
        target_descriptor,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let mut encoder =
        first_device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("invalid conversion ownership"),
            });

    let missing_usage = frame(
        &first,
        source_descriptor.clone(),
        wgpu::TextureUsages::COPY_DST,
    );
    let error = converter
        .encode(&mut encoder, &missing_usage, &target)
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let foreign = frame(
        &second,
        source_descriptor,
        wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let error = converter
        .encode(&mut encoder, &foreign, &target)
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
}

#[test]
fn limited_bt709_yuv420_with_odd_extent_renders_expected_rgba16f() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping YUV conversion execution");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let source_descriptor = GpuFrameDescriptor::new(
        3,
        3,
        PixelFormat::Yuv420p8,
        ColorSpace::BT709,
        AlphaMode::Opaque,
        Some(ChromaLocation::Left),
    )
    .unwrap();
    let destination_descriptor = GpuFrameDescriptor::new(
        3,
        3,
        PixelFormat::Rgba16Float,
        ColorSpace::new(
            ColorPrimaries::Bt709,
            TransferFunction::Bt709,
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        ),
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    let source = frame(
        &resources,
        source_descriptor.clone(),
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let destination = frame(
        &resources,
        destination_descriptor.clone(),
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let converter = pixel_converter(
        resources.clone(),
        GpuConversionPlan::new(source_descriptor, destination_descriptor).unwrap(),
    )
    .unwrap();

    let mut encoder =
        device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("odd YUV conversion"),
            });
    let uploads = upload_frame(
        &resources,
        &mut encoder,
        &source,
        &[
            vec![16, 16, 16, 16, 235, 16, 16, 16, 16],
            vec![128; 4],
            vec![128; 4],
        ],
    );
    let lease = converter
        .encode(&mut encoder, &source, &destination)
        .unwrap();
    let readback = copy_plane_to_readback(&resources, &mut encoder, &destination, 0);
    device.submit_viewport([encoder.finish()]);
    let bytes = read_plane(&device, &readback);
    let black = rgba16f(&bytes, 0);
    let white = rgba16f(&bytes, 4);
    for value in &black[..3] {
        assert_near(*value, 0.0, 0.002);
    }
    for value in &white[..3] {
        assert_near(*value, 1.0, 0.002);
    }
    assert_eq!(black[3], 1.0);
    assert_eq!(white[3], 1.0);
    drop((lease, uploads));
}

#[test]
fn packed_bgra_alpha_round_trip_preserves_channel_order_and_association() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping packed alpha conversion execution");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let packed_descriptor = GpuFrameDescriptor::new(
        1,
        1,
        PixelFormat::Bgra8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
        None,
    )
    .unwrap();
    let working_descriptor = GpuFrameDescriptor::new(
        1,
        1,
        PixelFormat::Rgba16Float,
        ColorSpace::SRGB,
        AlphaMode::Premultiplied,
        None,
    )
    .unwrap();
    let source = frame(
        &resources,
        packed_descriptor.clone(),
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let working = frame(
        &resources,
        working_descriptor.clone(),
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
    );
    let restored = frame(
        &resources,
        packed_descriptor.clone(),
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let to_working = pixel_converter(
        resources.clone(),
        GpuConversionPlan::new(packed_descriptor.clone(), working_descriptor.clone()).unwrap(),
    )
    .unwrap();
    let from_working = pixel_converter(
        resources.clone(),
        GpuConversionPlan::new(working_descriptor, packed_descriptor).unwrap(),
    )
    .unwrap();

    let mut encoder =
        device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("packed alpha round trip"),
            });
    let uploads = upload_frame(
        &resources,
        &mut encoder,
        &source,
        &[vec![64, 128, 255, 128]],
    );
    let first_lease = to_working.encode(&mut encoder, &source, &working).unwrap();
    let second_lease = from_working
        .encode(&mut encoder, &working, &restored)
        .unwrap();
    let working_readback = copy_plane_to_readback(&resources, &mut encoder, &working, 0);
    let restored_readback = copy_plane_to_readback(&resources, &mut encoder, &restored, 0);
    device.submit_viewport([encoder.finish()]);

    let working_bytes = read_plane(&device, &working_readback);
    let value = rgba16f(&working_bytes, 0);
    let alpha = 128.0 / 255.0;
    assert_near(value[0], alpha, 0.002);
    assert_near(value[1], (128.0 / 255.0) * alpha, 0.002);
    assert_near(value[2], (64.0 / 255.0) * alpha, 0.002);
    assert_near(value[3], alpha, 0.002);

    let restored_bytes = read_plane(&device, &restored_readback);
    assert_eq!(restored_bytes, vec![64, 128, 255, 128]);
    drop((first_lease, second_lease, uploads));
}

#[test]
fn packed_three_byte_rgb_and_bgr_round_trip_without_cpu_repacking() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping packed RGB conversion execution");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    for format in [PixelFormat::Rgb8Unorm, PixelFormat::Bgr8Unorm] {
        let packed_descriptor =
            GpuFrameDescriptor::new(2, 1, format, ColorSpace::SRGB, AlphaMode::Opaque, None)
                .unwrap();
        let working_descriptor = GpuFrameDescriptor::new(
            2,
            1,
            PixelFormat::Rgba16Float,
            ColorSpace::SRGB,
            AlphaMode::Opaque,
            None,
        )
        .unwrap();
        let source = frame(
            &resources,
            packed_descriptor.clone(),
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let working = frame(
            &resources,
            working_descriptor.clone(),
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let restored = frame(
            &resources,
            packed_descriptor.clone(),
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let to_working = pixel_converter(
            resources.clone(),
            GpuConversionPlan::new(packed_descriptor.clone(), working_descriptor.clone()).unwrap(),
        )
        .unwrap();
        let from_working = pixel_converter(
            resources.clone(),
            GpuConversionPlan::new(working_descriptor, packed_descriptor).unwrap(),
        )
        .unwrap();
        let input = vec![255, 0, 64, 10, 20, 30];
        let mut encoder =
            device
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("packed three byte round trip"),
                });
        let uploads = upload_frame(
            &resources,
            &mut encoder,
            &source,
            std::slice::from_ref(&input),
        );
        let first_lease = to_working.encode(&mut encoder, &source, &working).unwrap();
        let second_lease = from_working
            .encode(&mut encoder, &working, &restored)
            .unwrap();
        let readback = copy_plane_to_readback(&resources, &mut encoder, &restored, 0);
        device.submit_viewport([encoder.finish()]);
        assert_eq!(read_plane(&device, &readback), input, "{format:?}");
        drop((first_lease, second_lease, uploads));
    }
}

#[test]
fn uploaded_rgb_frame_enters_conversion_without_copy_and_returns_to_its_pool() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping upload conversion integration");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let uploader = DecodedFrameUploader::new(&device).unwrap();
    let input = vec![255, 0, 64, 10, 20, 30];
    let upload = DecodedFrameUpload::new(
        2,
        1,
        PixelFormat::Rgb8Unorm,
        vec![DecodedPlane::new(&input, 6, 1).unwrap()],
    )
    .unwrap();
    let uploaded = uploader.upload(&upload).unwrap();
    let allocation = uploaded.planes()[0].allocation_id();
    let source = GpuPixelFrame::from_uploaded(
        &resources,
        uploaded,
        ColorSpace::SRGB,
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    assert_eq!(
        source.retained_upload().unwrap().planes()[0].allocation_id(),
        allocation
    );
    assert_eq!(uploader.pool_stats().unwrap().checked_out(), 1);

    let destination_descriptor = GpuFrameDescriptor::new(
        2,
        1,
        PixelFormat::Rgba16Float,
        ColorSpace::SRGB,
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    let converter = pixel_converter(
        resources.clone(),
        GpuConversionPlan::new(source.descriptor().clone(), destination_descriptor.clone())
            .unwrap(),
    )
    .unwrap();
    let destination = frame(
        &resources,
        destination_descriptor,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let mut encoder =
        device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("uploaded RGB conversion"),
            });
    let lease = converter
        .encode(&mut encoder, &source, &destination)
        .unwrap();
    assert!(lease.retained_uploads().0.is_some());
    let readback = copy_plane_to_readback(&resources, &mut encoder, &destination, 0);
    device.submit_viewport([encoder.finish()]);
    let output = read_plane(&device, &readback);
    let first = rgba16f(&output, 0);
    let second = rgba16f(&output, 1);
    assert_near(first[0], 1.0, 0.002);
    assert_near(first[1], 0.0, 0.002);
    assert_near(first[2], 64.0 / 255.0, 0.002);
    assert_near(second[0], 10.0 / 255.0, 0.002);
    assert_near(second[1], 20.0 / 255.0, 0.002);
    assert_near(second[2], 30.0 / 255.0, 0.002);
    drop(source);
    assert_eq!(uploader.pool_stats().unwrap().checked_out(), 1);
    drop(lease);
    let stats = uploader.pool_stats().unwrap();
    assert_eq!(stats.checked_out(), 0);
    assert_eq!(stats.idle(), 1);
}

#[test]
fn uploaded_yuv_clamps_chroma_to_logical_extent_inside_aligned_allocations() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping aligned upload conversion");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let uploader = DecodedFrameUploader::with_config(
        &device,
        UploadConfig::new(
            TextureAlignment::new(8, 8).unwrap(),
            TexturePoolConfig::new(1),
        ),
    )
    .unwrap();
    let luma = vec![16; 4 * 4];
    let chroma = vec![128; 2 * 2 * 2];
    let upload = DecodedFrameUpload::new(
        4,
        4,
        PixelFormat::Nv12,
        vec![
            DecodedPlane::new(&luma, 4, 4).unwrap(),
            DecodedPlane::new(&chroma, 4, 2).unwrap(),
        ],
    )
    .unwrap();
    let uploaded = uploader.upload(&upload).unwrap();
    assert_eq!(uploaded.planes()[1].texture_size().width, 2);
    assert_eq!(uploaded.planes()[1].allocation_size().width, 8);
    let source = GpuPixelFrame::from_uploaded(
        &resources,
        uploaded,
        ColorSpace::BT709,
        AlphaMode::Opaque,
        Some(ChromaLocation::Left),
    )
    .unwrap();

    let destination_descriptor = GpuFrameDescriptor::new(
        4,
        4,
        PixelFormat::Rgba16Float,
        ColorSpace::new(
            ColorPrimaries::Bt709,
            TransferFunction::Bt709,
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        ),
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    let converter = pixel_converter(
        resources.clone(),
        GpuConversionPlan::new(source.descriptor().clone(), destination_descriptor.clone())
            .unwrap(),
    )
    .unwrap();
    let destination = frame(
        &resources,
        destination_descriptor,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let mut encoder =
        device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("aligned NV12 conversion"),
            });
    let lease = converter
        .encode(&mut encoder, &source, &destination)
        .unwrap();
    let readback = copy_plane_to_readback(&resources, &mut encoder, &destination, 0);
    device.submit_viewport([encoder.finish()]);
    let output = read_plane(&device, &readback);
    for index in 0..16 {
        let pixel = rgba16f(&output, index);
        for value in &pixel[..3] {
            assert_near(*value, 0.0, 0.002);
        }
        assert_eq!(pixel[3], 1.0);
    }
    drop((lease, source));
    assert_eq!(uploader.pool_stats().unwrap().checked_out(), 0);
}

#[test]
fn planar_and_semiplanar_ten_bit_round_trips_keep_exact_code_alignment() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping high-bit-depth conversion execution");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();

    for (format, width, height, chroma, input) in [
        (
            PixelFormat::Yuv444p10,
            2,
            1,
            None,
            vec![
                u16_bytes(&[64, 940]),
                u16_bytes(&[512, 512]),
                u16_bytes(&[512, 512]),
            ],
        ),
        (
            PixelFormat::P010,
            3,
            3,
            Some(ChromaLocation::Left),
            vec![
                u16_bytes(&[
                    64 << 6,
                    940 << 6,
                    64 << 6,
                    940 << 6,
                    64 << 6,
                    940 << 6,
                    64 << 6,
                    940 << 6,
                    64 << 6,
                ]),
                u16_bytes(&[
                    512 << 6,
                    512 << 6,
                    512 << 6,
                    512 << 6,
                    512 << 6,
                    512 << 6,
                    512 << 6,
                    512 << 6,
                ]),
            ],
        ),
    ] {
        let encoded_descriptor = GpuFrameDescriptor::new(
            width,
            height,
            format,
            ColorSpace::BT2020,
            AlphaMode::Opaque,
            chroma,
        )
        .unwrap();
        let working_descriptor = GpuFrameDescriptor::new(
            width,
            height,
            PixelFormat::Rgba16Float,
            ColorSpace::new(
                ColorPrimaries::Bt2020,
                TransferFunction::Bt2020TenBit,
                MatrixCoefficients::Rgb,
                ColorRange::Full,
            ),
            AlphaMode::Opaque,
            None,
        )
        .unwrap();
        let source = frame(
            &resources,
            encoded_descriptor.clone(),
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let working = frame(
            &resources,
            working_descriptor.clone(),
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let restored = frame(
            &resources,
            encoded_descriptor.clone(),
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let to_working = pixel_converter(
            resources.clone(),
            GpuConversionPlan::new(encoded_descriptor.clone(), working_descriptor.clone()).unwrap(),
        )
        .unwrap();
        let from_working = pixel_converter(
            resources.clone(),
            GpuConversionPlan::new(working_descriptor, encoded_descriptor).unwrap(),
        )
        .unwrap();
        let mut encoder =
            device
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("ten bit round trip"),
                });
        let uploads = upload_frame(&resources, &mut encoder, &source, &input);
        let first_lease = to_working.encode(&mut encoder, &source, &working).unwrap();
        let second_lease = from_working
            .encode(&mut encoder, &working, &restored)
            .unwrap();
        let readbacks = (0..restored.planes().len())
            .map(|plane| copy_plane_to_readback(&resources, &mut encoder, &restored, plane))
            .collect::<Vec<_>>();
        device.submit_viewport([encoder.finish()]);
        for (expected, readback) in input.iter().zip(&readbacks) {
            let actual = read_plane(&device, readback);
            assert_eq!(actual.len(), expected.len());
            for (actual, expected) in actual.chunks_exact(2).zip(expected.chunks_exact(2)) {
                let actual = u16::from_le_bytes(actual.try_into().unwrap());
                let expected = u16::from_le_bytes(expected.try_into().unwrap());
                let tolerance = if format == PixelFormat::P010 { 64 } else { 1 };
                assert!(
                    actual.abs_diff(expected) <= tolerance,
                    "{format:?}: {actual} vs {expected}"
                );
            }
        }
        drop((first_lease, second_lease, uploads));
    }
}

#[test]
fn limited_rgba16_uses_sixteen_bit_legal_code_points() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping limited RGB conversion");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let limited = ColorSpace::new(
        ColorPrimaries::Bt709,
        TransferFunction::Srgb,
        MatrixCoefficients::Rgb,
        ColorRange::Limited,
    );
    let source_descriptor = GpuFrameDescriptor::new(
        2,
        1,
        PixelFormat::Rgba16Unorm,
        limited,
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    let destination_descriptor = GpuFrameDescriptor::new(
        2,
        1,
        PixelFormat::Rgba16Float,
        ColorSpace::SRGB,
        AlphaMode::Opaque,
        None,
    )
    .unwrap();
    let source = frame(
        &resources,
        source_descriptor.clone(),
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let destination = frame(
        &resources,
        destination_descriptor.clone(),
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let converter = pixel_converter(
        resources.clone(),
        GpuConversionPlan::new(source_descriptor, destination_descriptor).unwrap(),
    )
    .unwrap();
    let mut encoder =
        device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("limited sixteen bit RGB conversion"),
            });
    let uploads = upload_frame(
        &resources,
        &mut encoder,
        &source,
        &[u16_bytes(&[
            4096,
            4096,
            4096,
            u16::MAX,
            60160,
            60160,
            60160,
            u16::MAX,
        ])],
    );
    let lease = converter
        .encode(&mut encoder, &source, &destination)
        .unwrap();
    let readback = copy_plane_to_readback(&resources, &mut encoder, &destination, 0);
    device.submit_viewport([encoder.finish()]);
    let output = read_plane(&device, &readback);
    let black = rgba16f(&output, 0);
    let white = rgba16f(&output, 1);
    for value in &black[..3] {
        assert_near(*value, 0.0, 0.001);
    }
    for value in &white[..3] {
        assert_near(*value, 1.0, 0.001);
    }
    assert_eq!(black[3], 1.0);
    assert_eq!(white[3], 1.0);
    drop((lease, uploads));
}

#[test]
fn rgba32_float_hdr_values_survive_half_float_conversion_and_alpha_association() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping float conversion execution");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let source_descriptor = GpuFrameDescriptor::new(
        1,
        1,
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Straight,
        None,
    )
    .unwrap();
    let destination_descriptor = GpuFrameDescriptor::new(
        1,
        1,
        PixelFormat::Rgba16Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
        None,
    )
    .unwrap();
    let source = frame(
        &resources,
        source_descriptor.clone(),
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let destination = frame(
        &resources,
        destination_descriptor.clone(),
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let converter = pixel_converter(
        resources.clone(),
        GpuConversionPlan::new(source_descriptor, destination_descriptor).unwrap(),
    )
    .unwrap();
    let mut encoder =
        device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("float HDR conversion"),
            });
    let uploads = upload_frame(
        &resources,
        &mut encoder,
        &source,
        &[f32_bytes(&[-0.25, 2.0, 0.5, 0.5])],
    );
    let lease = converter
        .encode(&mut encoder, &source, &destination)
        .unwrap();
    let readback = copy_plane_to_readback(&resources, &mut encoder, &destination, 0);
    device.submit_viewport([encoder.finish()]);
    let bytes = read_plane(&device, &readback);
    let value = rgba16f(&bytes, 0);
    assert_near(value[0], -0.125, 0.001);
    assert_near(value[1], 1.0, 0.001);
    assert_near(value[2], 0.25, 0.001);
    assert_near(value[3], 0.5, 0.001);
    drop((lease, uploads));
}

fn _views_are_managed(values: Vec<GpuTextureView>) -> Vec<GpuTextureView> {
    values
}
