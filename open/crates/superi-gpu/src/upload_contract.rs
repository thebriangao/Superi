use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::pixel::PixelFormat;

use std::sync::mpsc;

use crate::device::{AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions};
use crate::texture_pool::{TextureAlignment, TexturePoolConfig};
use crate::upload::{
    DecodedFrameUpload, DecodedFrameUploader, DecodedPlane, PlaneUploadPath, UploadConfig,
    UploadedFrame,
};
use crate::wgpu;

fn assert_send_sync<T: Send + Sync>() {}

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(
        adapter
            .create_device(&DeviceRequest::default().with_label("superi decoded upload contract")),
    )
    .ok()
}

fn size(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

#[test]
fn decoded_layout_maps_packed_and_planar_storage_without_conversion() {
    let rgba_bytes = vec![0_u8; 2 * 2 * 4];
    let rgba = DecodedFrameUpload::new(
        2,
        2,
        PixelFormat::Rgba8Unorm,
        vec![DecodedPlane::new(&rgba_bytes, 8, 2).unwrap()],
    )
    .unwrap();
    assert_eq!(rgba.plane_layouts().len(), 1);
    assert_eq!(rgba.plane_layouts()[0].source_size(), size(2, 2));
    assert_eq!(rgba.plane_layouts()[0].texture_size(), size(2, 2));
    assert_eq!(
        rgba.plane_layouts()[0].texture_format(),
        wgpu::TextureFormat::Rgba8Unorm
    );
    assert_eq!(rgba.plane_layouts()[0].tight_bytes_per_row(), 8);

    let rgb_bytes = vec![0_u8; 2 * 2 * 3];
    let rgb = DecodedFrameUpload::new(
        2,
        2,
        PixelFormat::Rgb8Unorm,
        vec![DecodedPlane::new(&rgb_bytes, 6, 2).unwrap()],
    )
    .unwrap();
    assert_eq!(rgb.plane_layouts()[0].source_size(), size(2, 2));
    assert_eq!(rgb.plane_layouts()[0].texture_size(), size(6, 2));
    assert_eq!(
        rgb.plane_layouts()[0].texture_format(),
        wgpu::TextureFormat::R8Unorm
    );
    assert_eq!(rgb.plane_layouts()[0].tight_bytes_per_row(), 6);

    let nv12_luma = vec![0_u8; 4 * 4];
    let nv12_chroma = vec![0_u8; 4 * 2];
    let nv12 = DecodedFrameUpload::new(
        4,
        4,
        PixelFormat::Nv12,
        vec![
            DecodedPlane::new(&nv12_luma, 4, 4).unwrap(),
            DecodedPlane::new(&nv12_chroma, 4, 2).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(nv12.plane_layouts().len(), 2);
    assert_eq!(nv12.plane_layouts()[0].texture_size(), size(4, 4));
    assert_eq!(
        nv12.plane_layouts()[0].texture_format(),
        wgpu::TextureFormat::R8Unorm
    );
    assert_eq!(nv12.plane_layouts()[1].source_size(), size(2, 2));
    assert_eq!(nv12.plane_layouts()[1].texture_size(), size(2, 2));
    assert_eq!(
        nv12.plane_layouts()[1].texture_format(),
        wgpu::TextureFormat::Rg8Unorm
    );

    let p010_luma = vec![0_u8; 4 * 4 * 2];
    let p010_chroma = vec![0_u8; 4 * 2 * 2];
    let p010 = DecodedFrameUpload::new(
        4,
        4,
        PixelFormat::P010,
        vec![
            DecodedPlane::new(&p010_luma, 8, 4).unwrap(),
            DecodedPlane::new(&p010_chroma, 8, 2).unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        p010.plane_layouts()[0].texture_format(),
        wgpu::TextureFormat::R16Uint
    );
    assert_eq!(
        p010.plane_layouts()[1].texture_format(),
        wgpu::TextureFormat::Rg16Uint
    );
    assert_eq!(p010.plane_layouts()[1].source_size(), size(2, 2));
    assert_eq!(p010.plane_layouts()[1].tight_bytes_per_row(), 8);
}

#[test]
fn malformed_plane_geometry_is_classified_before_gpu_work() {
    let missing = DecodedFrameUpload::new(2, 2, PixelFormat::Rgba8Unorm, Vec::new()).unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::InvalidInput);
    assert_eq!(missing.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        missing.contexts().last().unwrap().component(),
        "superi-gpu.upload"
    );
    assert_eq!(
        missing.contexts().last().unwrap().operation(),
        "create_decoded_frame_upload"
    );

    let short_rows = vec![0_u8; 6];
    let short_stride = DecodedFrameUpload::new(
        2,
        1,
        PixelFormat::Rgba8Unorm,
        vec![DecodedPlane::new(&short_rows, 6, 1).unwrap()],
    )
    .unwrap_err();
    assert_eq!(short_stride.category(), ErrorCategory::InvalidInput);

    let wrong_chroma_rows = vec![0_u8; 4 * 3];
    let luma = vec![0_u8; 4 * 4];
    let wrong_rows = DecodedFrameUpload::new(
        4,
        4,
        PixelFormat::Nv12,
        vec![
            DecodedPlane::new(&luma, 4, 4).unwrap(),
            DecodedPlane::new(&wrong_chroma_rows, 4, 3).unwrap(),
        ],
    )
    .unwrap_err();
    assert_eq!(wrong_rows.category(), ErrorCategory::InvalidInput);
}

fn padded_rgba(rows: &[[u8; 16]], stride: usize, padding: u8) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(stride * rows.len());
    for row in rows {
        bytes.extend_from_slice(row);
        bytes.resize(bytes.len() + stride - row.len(), padding);
    }
    bytes
}

fn read_plane(device: &GpuDevice, frame: &UploadedFrame<'_>, plane_index: usize) -> Vec<u8> {
    let plane = &frame.planes()[plane_index];
    let texture_size = plane.texture_size();
    let bytes_per_texel = plane
        .texture_format()
        .block_copy_size(Some(wgpu::TextureAspect::All))
        .unwrap();
    let tight_bytes = texture_size.width * bytes_per_texel;
    let readback_bytes = u64::from(256 * texture_size.height);
    let readback = device.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("decoded upload readback"),
        size: readback_bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder =
        device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("decoded upload readback"),
            });
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: plane.texture().raw(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &readback,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(texture_size.height),
            },
        },
        texture_size,
    );
    device.submit_viewport([encoder.finish()]);

    let slice = readback.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).expect("map receiver remains alive");
    });
    let _ = device.wgpu_device().poll(wgpu::Maintain::Wait);
    receiver
        .recv()
        .expect("mapping callback must run")
        .expect("decoded upload readback must succeed");
    let mapped = slice.get_mapped_range();
    let mut result = Vec::with_capacity((tight_bytes * texture_size.height) as usize);
    for row in 0..texture_size.height as usize {
        let start = row * 256;
        result.extend_from_slice(&mapped[start..start + tight_bytes as usize]);
    }
    drop(mapped);
    readback.unmap();
    result
}

#[test]
fn direct_and_required_repack_paths_share_reusable_gpu_allocations() {
    assert_send_sync::<DecodedFrameUploader<'static>>();
    assert_send_sync::<UploadedFrame<'static>>();

    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping decoded upload execution");
        return;
    };
    let uploader = DecodedFrameUploader::with_config(
        &device,
        UploadConfig::new(
            TextureAlignment::new(8, 8).unwrap(),
            TexturePoolConfig::new(2),
        ),
    )
    .unwrap();

    let first_rows = [
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        [
            17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
        ],
    ];
    let first_bytes = padded_rgba(&first_rows, 20, 0xee);
    let first_source = DecodedFrameUpload::new(
        4,
        2,
        PixelFormat::Rgba8Unorm,
        vec![DecodedPlane::new(&first_bytes, 20, 2).unwrap()],
    )
    .unwrap();
    let first = uploader.upload(&first_source).unwrap();
    assert_eq!(first.report().queue_writes(), 1);
    assert_eq!(first.report().direct_planes(), 1);
    assert_eq!(first.report().repacked_planes(), 0);
    assert_eq!(first.planes()[0].upload_path(), PlaneUploadPath::Direct);
    assert_eq!(first.planes()[0].texture_size(), size(4, 2));
    assert_eq!(first.planes()[0].allocation_size(), size(8, 8));
    assert_eq!(
        read_plane(&device, &first, 0),
        first_rows.into_iter().flatten().collect::<Vec<_>>()
    );
    let allocation_id = first.planes()[0].allocation_id();

    let retained = first.clone();
    drop(first);
    assert_eq!(uploader.pool_stats().unwrap().checked_out(), 1);
    assert_eq!(uploader.pool_stats().unwrap().idle(), 0);
    drop(retained);
    assert_eq!(uploader.pool_stats().unwrap().checked_out(), 0);
    assert_eq!(uploader.pool_stats().unwrap().idle(), 1);

    let second_rows = [
        [
            32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17,
        ],
        [16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1],
    ];
    let second_bytes = padded_rgba(&second_rows, 18, 0xdd);
    let second_source = DecodedFrameUpload::new(
        4,
        2,
        PixelFormat::Rgba8Unorm,
        vec![DecodedPlane::new(&second_bytes, 18, 2).unwrap()],
    )
    .unwrap();
    let second = uploader.upload(&second_source).unwrap();
    assert_eq!(second.planes()[0].allocation_id(), allocation_id);
    assert_eq!(second.report().queue_writes(), 1);
    assert_eq!(second.report().direct_planes(), 0);
    assert_eq!(second.report().repacked_planes(), 1);
    assert_eq!(second.report().repacked_bytes(), 32);
    assert_eq!(second.planes()[0].upload_path(), PlaneUploadPath::Repacked);
    assert_eq!(
        read_plane(&device, &second, 0),
        second_rows.into_iter().flatten().collect::<Vec<_>>()
    );
    let stats = uploader.pool_stats().unwrap();
    assert_eq!(stats.allocations(), 1);
    assert_eq!(stats.reuses(), 1);
}

#[test]
fn planar_and_semiplanar_upload_preserves_yuv_nv12_and_p010_bytes() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping planar upload execution");
        return;
    };
    let uploader = DecodedFrameUploader::new(&device).unwrap();

    let yuv_luma = (16_u8..32).collect::<Vec<_>>();
    let yuv_u = vec![41, 42, 43, 44];
    let yuv_v = vec![81, 82, 83, 84];
    let yuv_source = DecodedFrameUpload::new(
        4,
        4,
        PixelFormat::Yuv420p8,
        vec![
            DecodedPlane::new(&yuv_luma, 4, 4).unwrap(),
            DecodedPlane::new(&yuv_u, 2, 2).unwrap(),
            DecodedPlane::new(&yuv_v, 2, 2).unwrap(),
        ],
    )
    .unwrap();
    let yuv = uploader.upload(&yuv_source).unwrap();
    assert_eq!(yuv.report().queue_writes(), 3);
    assert_eq!(yuv.report().direct_planes(), 3);
    assert_eq!(read_plane(&device, &yuv, 0), yuv_luma);
    assert_eq!(read_plane(&device, &yuv, 1), yuv_u);
    assert_eq!(read_plane(&device, &yuv, 2), yuv_v);

    let nv12_luma = (0_u8..16).collect::<Vec<_>>();
    let nv12_chroma = vec![101, 151, 102, 152, 103, 153, 104, 154];
    let nv12_source = DecodedFrameUpload::new(
        4,
        4,
        PixelFormat::Nv12,
        vec![
            DecodedPlane::new(&nv12_luma, 4, 4).unwrap(),
            DecodedPlane::new(&nv12_chroma, 4, 2).unwrap(),
        ],
    )
    .unwrap();
    let nv12 = uploader.upload(&nv12_source).unwrap();
    assert_eq!(nv12.report().queue_writes(), 2);
    assert_eq!(nv12.report().direct_planes(), 2);
    assert_eq!(read_plane(&device, &nv12, 0), nv12_luma);
    assert_eq!(read_plane(&device, &nv12, 1), nv12_chroma);

    let p010_luma_words = [0_u16, 64, 128, 256, 512, 768, 960, 1023];
    let p010_chroma_words = [128_u16, 896, 256, 768];
    let p010_luma = p010_luma_words
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let p010_chroma = p010_chroma_words
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let p010_source = DecodedFrameUpload::new(
        4,
        2,
        PixelFormat::P010,
        vec![
            DecodedPlane::new(&p010_luma, 8, 2).unwrap(),
            DecodedPlane::new(&p010_chroma, 8, 1).unwrap(),
        ],
    )
    .unwrap();
    let p010 = uploader.upload(&p010_source).unwrap();
    assert_eq!(p010.report().queue_writes(), 2);
    assert_eq!(p010.report().direct_planes(), 2);
    assert_eq!(read_plane(&device, &p010, 0), p010_luma);
    assert_eq!(read_plane(&device, &p010, 1), p010_chroma);
}
