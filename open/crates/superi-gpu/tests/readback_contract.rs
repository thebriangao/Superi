use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::pixel::PixelFormat;
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::pool::{GpuMemoryPool, MemoryBudget, MemoryClass};
use superi_gpu::readback::{ReadbackBoundary, TextureReadbackManager, TextureReadbackRequest};
use superi_gpu::resource::{GpuResourceKind, GpuResources};
use superi_gpu::submission::GpuSubmissionQueue;
use superi_gpu::texture::GpuTexture;
use superi_gpu::upload::{DecodedFrameUpload, DecodedFrameUploader, DecodedPlane};
use superi_gpu::wgpu;

fn test_device(label: &str) -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(adapter.create_device(&DeviceRequest::default().with_label(label))).ok()
}

fn size(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

fn color_texture(
    resources: &GpuResources<'_>,
    label: &str,
    extent: wgpu::Extent3d,
    usage: wgpu::TextureUsages,
) -> GpuTexture {
    resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage,
            view_formats: &[],
        })
        .unwrap()
}

#[test]
fn explicit_requests_preflight_layout_usage_format_region_and_budget() {
    let Some(device) = test_device("readback validation contract") else {
        eprintln!("no wgpu adapter is available, skipping readback validation contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let memory = GpuMemoryPool::new(MemoryBudget::new(512, 512).unwrap());
    let manager = TextureReadbackManager::with_memory_pool(resources.clone(), memory.clone());
    let source = color_texture(
        &resources,
        "three by two export source",
        size(3, 2),
        wgpu::TextureUsages::COPY_SRC,
    );

    let request =
        TextureReadbackRequest::for_export(source.clone(), wgpu::Origin3d::ZERO, size(3, 2));
    assert_eq!(request.boundary(), ReadbackBoundary::Export);
    assert_eq!(request.origin(), wgpu::Origin3d::ZERO);
    assert_eq!(request.extent(), size(3, 2));
    assert_eq!(request.mip_level(), 0);

    let encoded = manager.encode(request).unwrap();
    assert_eq!(encoded.boundary(), ReadbackBoundary::Export);
    assert_eq!(encoded.layout().format(), wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(encoded.layout().tight_bytes_per_row(), 12);
    assert_eq!(encoded.layout().padded_bytes_per_row(), 256);
    assert_eq!(encoded.layout().rows_per_image(), 2);
    assert_eq!(encoded.layout().tight_bytes_per_image(), 24);
    assert_eq!(encoded.layout().staging_bytes(), 512);
    assert_eq!(
        memory
            .stats()
            .unwrap()
            .resident_bytes_for(MemoryClass::Buffer),
        512
    );
    assert_eq!(
        resources.stats().count(GpuResourceKind::Buffer),
        1,
        "the encoded operation owns one managed staging buffer"
    );
    drop(encoded);
    assert_eq!(memory.stats().unwrap().resident_bytes(), 0);
    assert_eq!(resources.stats().count(GpuResourceKind::Buffer), 0);

    let no_copy = color_texture(
        &resources,
        "not copyable",
        size(3, 2),
        wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let error = manager
        .encode(TextureReadbackRequest::for_thumbnail(
            no_copy,
            wgpu::Origin3d::ZERO,
            size(3, 2),
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        error.contexts().last().unwrap().operation(),
        "encode_texture_readback"
    );

    let error = manager
        .encode(TextureReadbackRequest::for_thumbnail(
            source.clone(),
            wgpu::Origin3d { x: 2, y: 0, z: 0 },
            size(2, 2),
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let multisampled = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("multisampled readback source"),
            size: size(4, 4),
            mip_level_count: 1,
            sample_count: 4,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .unwrap();
    let error = manager
        .encode(TextureReadbackRequest::for_export(
            multisampled,
            wgpu::Origin3d::ZERO,
            size(4, 4),
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let depth = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth readback source"),
            size: size(3, 2),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .unwrap();
    let error = manager
        .encode(TextureReadbackRequest::for_export(
            depth,
            wgpu::Origin3d::ZERO,
            size(3, 2),
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);

    let constrained = GpuMemoryPool::new(MemoryBudget::new(511, 511).unwrap());
    let constrained_manager =
        TextureReadbackManager::with_memory_pool(resources.clone(), constrained.clone());
    let error = constrained_manager
        .encode(TextureReadbackRequest::for_export(
            source,
            wgpu::Origin3d::ZERO,
            size(3, 2),
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(constrained.stats().unwrap().resident_bytes(), 0);
}

#[test]
fn native_export_and_thumbnail_readbacks_preserve_exact_pixels_and_ordering() {
    let Some(device) = test_device("native readback contract") else {
        eprintln!("no wgpu adapter is available, skipping native readback contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let submissions = GpuSubmissionQueue::new(&device).unwrap();
    let memory = GpuMemoryPool::new(MemoryBudget::new(1, 4096).unwrap());
    let manager = TextureReadbackManager::with_memory_pool(resources.clone(), memory.clone());

    let rendered = color_texture(
        &resources,
        "rendered export source",
        size(3, 2),
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let view = resources
        .create_texture_view(&rendered, &wgpu::TextureViewDescriptor::default())
        .unwrap();
    let mut encoder =
        device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render before export readback"),
            });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear visible export output"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: view.raw(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.25,
                        g: 0.5,
                        b: 0.75,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    let mut retained = submissions.resources();
    retained.retain(view);
    retained.retain(rendered.clone());
    let render_fence = submissions.submit([encoder.finish()], retained).unwrap();

    let export = manager
        .encode(TextureReadbackRequest::for_export(
            rendered,
            wgpu::Origin3d::ZERO,
            size(3, 2),
        ))
        .unwrap();
    let export_staging_id = export.staging_buffer_id();
    let submitted_export = submissions.submit_readback(export).unwrap();
    assert!(submitted_export.fence().value() > render_fence.value());
    assert_eq!(submitted_export.boundary(), ReadbackBoundary::Export);
    assert_eq!(submissions.progress().retained_resources(), 4);
    let export_result = submitted_export.wait(&submissions).unwrap();
    assert_eq!(export_result.boundary(), ReadbackBoundary::Export);
    assert_eq!(export_result.layout().tight_bytes_per_row(), 12);
    assert_eq!(export_result.bytes().len(), 24);
    assert_eq!(
        export_result.bytes(),
        [64_u8, 128, 191, 255].repeat(6).as_slice()
    );
    assert_eq!(memory.stats().unwrap().resident_bytes(), 0);
    assert_eq!(resources.stats().count(GpuResourceKind::Buffer), 0);
    assert_eq!(export_staging_id.kind(), GpuResourceKind::Buffer);

    let pixels = [
        1_u8, 2, 3, 255, 11, 12, 13, 255, 21, 22, 23, 255, 31, 32, 33, 255, 41, 42, 43, 255, 51,
        52, 53, 255,
    ];
    let upload = DecodedFrameUpload::new(
        3,
        2,
        PixelFormat::Rgba8Unorm,
        vec![DecodedPlane::new(&pixels, 12, 2).unwrap()],
    )
    .unwrap();
    let uploader = DecodedFrameUploader::new(&device).unwrap();
    let uploaded = uploader.upload(&upload).unwrap();
    let thumbnail = manager
        .encode(TextureReadbackRequest::for_thumbnail(
            uploaded.planes()[0].texture().clone(),
            wgpu::Origin3d { x: 1, y: 0, z: 0 },
            size(2, 2),
        ))
        .unwrap();
    let mut submitted_thumbnail = submissions.submit_readback(thumbnail).unwrap();
    let thumbnail_result = loop {
        if let Some(result) = submitted_thumbnail.poll(&submissions).unwrap() {
            break result;
        }
        std::thread::yield_now();
    };
    assert_eq!(thumbnail_result.boundary(), ReadbackBoundary::Thumbnail);
    assert_eq!(thumbnail_result.origin().x, 1);
    assert_eq!(thumbnail_result.layout().tight_bytes_per_row(), 8);
    assert_eq!(
        thumbnail_result.bytes(),
        &[11_u8, 12, 13, 255, 21, 22, 23, 255, 41, 42, 43, 255, 51, 52, 53, 255,]
    );
    assert_eq!(
        thumbnail_result.row(0, 0).unwrap(),
        &[11, 12, 13, 255, 21, 22, 23, 255]
    );
    assert_eq!(
        thumbnail_result.row(0, 1).unwrap(),
        &[41, 42, 43, 255, 51, 52, 53, 255]
    );
    assert!(thumbnail_result.row(0, 2).is_none());
    assert_eq!(submissions.progress().in_flight(), 0);
    assert_eq!(submissions.progress().retained_resources(), 0);

    let half_pixels = [
        0x00_u8, 0x3c, 0x00, 0x40, 0x00, 0x42, 0x00, 0x3c, 0x00, 0x44, 0x00, 0x45, 0x00, 0x46,
        0x00, 0x3c,
    ];
    let half_upload = DecodedFrameUpload::new(
        2,
        1,
        PixelFormat::Rgba16Float,
        vec![DecodedPlane::new(&half_pixels, 16, 1).unwrap()],
    )
    .unwrap();
    let half_frame = uploader.upload(&half_upload).unwrap();
    let precise = manager
        .encode(TextureReadbackRequest::for_export(
            half_frame.planes()[0].texture().clone(),
            wgpu::Origin3d::ZERO,
            size(2, 1),
        ))
        .unwrap();
    let precise_result = submissions
        .submit_readback(precise)
        .unwrap()
        .wait(&submissions)
        .unwrap();
    assert_eq!(
        precise_result.layout().format(),
        wgpu::TextureFormat::Rgba16Float
    );
    assert_eq!(precise_result.bytes(), &half_pixels);

    let cancelled = manager
        .encode(TextureReadbackRequest::for_thumbnail(
            half_frame.planes()[0].texture().clone(),
            wgpu::Origin3d::ZERO,
            size(2, 1),
        ))
        .unwrap();
    let cancelled = submissions.submit_readback(cancelled).unwrap();
    let cancelled_fence = cancelled.fence().clone();
    assert_eq!(
        memory
            .stats()
            .unwrap()
            .resident_bytes_for(MemoryClass::Buffer),
        256
    );
    drop(cancelled);
    assert_eq!(
        memory
            .stats()
            .unwrap()
            .resident_bytes_for(MemoryClass::Buffer),
        256,
        "the queue retains staging after the submitted handle is dropped"
    );
    submissions.wait(&cancelled_fence).unwrap();
    assert_eq!(memory.stats().unwrap().resident_bytes(), 0);
    assert_eq!(resources.stats().count(GpuResourceKind::Buffer), 0);
}

#[test]
fn recovered_device_queue_rejects_an_obsolete_encoded_readback() {
    let Some(first_device) = test_device("obsolete readback source") else {
        eprintln!("no wgpu adapter is available, skipping recovered readback contract");
        return;
    };
    let Some(second_device) = test_device("recovered readback destination") else {
        eprintln!("no second wgpu device is available, skipping recovered readback contract");
        return;
    };
    let first_resources = GpuResources::new(&first_device).unwrap();
    let source = color_texture(
        &first_resources,
        "obsolete readback texture",
        size(2, 2),
        wgpu::TextureUsages::COPY_SRC,
    );
    let encoded = TextureReadbackManager::new(first_resources)
        .encode(TextureReadbackRequest::for_thumbnail(
            source,
            wgpu::Origin3d::ZERO,
            size(2, 2),
        ))
        .unwrap();
    let recovered_submissions = GpuSubmissionQueue::new(&second_device).unwrap();
    let error = recovered_submissions.submit_readback(encoded).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
}
