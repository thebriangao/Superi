use std::sync::{Arc, Mutex};

use superi_core::error::ErrorCategory;
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::resource::GpuResources;
use superi_gpu::submission::{GpuFence, GpuSubmissionQueue};
use superi_gpu::texture_pool::{TexturePool, TexturePoolConfig, TextureRequest};
use superi_gpu::wgpu;

fn assert_send_sync<T: Send + Sync>() {}

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(
        adapter.create_device(&DeviceRequest::default().with_label("superi submission contract")),
    )
    .ok()
}

fn texture_request() -> TextureRequest {
    TextureRequest::new(
        wgpu::Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    )
}

#[test]
fn one_submission_owner_orders_fences_and_rejects_foreign_state() {
    assert_send_sync::<GpuFence>();

    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping submission ownership contract");
        return;
    };
    let submissions = GpuSubmissionQueue::new(&device).unwrap();

    let duplicate = GpuSubmissionQueue::new(&device).unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::Conflict);

    let first = submissions
        .submit(std::iter::empty(), submissions.resources())
        .unwrap();
    let second = submissions
        .submit(std::iter::empty(), submissions.resources())
        .unwrap();
    assert_eq!(first.value(), 1);
    assert_eq!(second.value(), 2);
    assert_eq!(submissions.progress().last_submitted(), 2);

    let progress = submissions.wait(&second).unwrap();
    assert_eq!(progress.last_retired(), 2);
    assert_eq!(progress.in_flight(), 0);

    let stale_resources = submissions.resources();
    drop(submissions);
    let replacement = GpuSubmissionQueue::new(&device).unwrap();
    let stale = replacement.wait(&second).unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    let stale = replacement
        .submit(std::iter::empty(), stale_resources)
        .unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
}

#[test]
fn submitted_gpu_work_retains_pooled_owners_until_fence_retirement() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping resource retirement contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let pool = TexturePool::new(resources.clone(), TexturePoolConfig::new(2));
    let submissions = GpuSubmissionQueue::new(&device).unwrap();

    let texture = pool.acquire(&texture_request()).unwrap();
    let first_allocation = texture.allocation_id();
    let view = texture
        .raw()
        .create_view(&wgpu::TextureViewDescriptor::default());
    let readback = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("submission readback"),
            size: wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.into(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
        .unwrap();
    let mut encoder =
        device
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("submission render and readback"),
            });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("submission clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: texture.raw(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: readback.raw(),
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
                rows_per_image: Some(1),
            },
        },
        wgpu::Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
    );

    let mut retained = submissions.resources();
    retained.retain(texture);
    let fence = submissions.submit([encoder.finish()], retained).unwrap();
    assert_eq!(submissions.progress().retained_resources(), 1);
    assert_eq!(pool.stats().unwrap().checked_out(), 1);

    let competing = pool.acquire(&texture_request()).unwrap();
    assert_ne!(competing.allocation_id(), first_allocation);
    drop(competing);

    let progress = submissions.wait(&fence).unwrap();
    assert_eq!(progress.last_retired(), fence.value());
    assert_eq!(progress.retained_resources(), 0);
    assert_eq!(pool.stats().unwrap().checked_out(), 0);

    let mapped = Arc::new(Mutex::new(None));
    let mapped_result = Arc::clone(&mapped);
    readback
        .raw()
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            *mapped_result.lock().unwrap() = Some(result);
        });
    let _ = device.wgpu_device().poll(wgpu::Maintain::Wait);
    mapped.lock().unwrap().take().unwrap().unwrap();
    let bytes = readback.raw().slice(..).get_mapped_range();
    assert_eq!(&bytes[..8], &[255, 0, 0, 255, 255, 0, 0, 255]);
    drop(bytes);
    readback.raw().unmap();
}
