use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use superi_core::error::{Error, ErrorCategory, Recoverability};
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuDeviceLossReason, GpuDeviceStatus, GpuInstance,
    InstanceOptions,
};
use superi_gpu::recovery::{
    GpuRecoveryPhase, GpuRecoveryPlan, GpuRecoveryTextureWrite, RecoveredGpu,
};
use superi_gpu::resource::GpuResources;
use superi_gpu::submission::GpuSubmissionQueue;
use superi_gpu::wgpu;

fn assert_send_sync<T: Send + Sync>() {}

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(
        adapter.create_device(&DeviceRequest::default().with_label("superi recovery contract")),
    )
    .ok()
}

fn destroy_and_detect(device: &GpuDevice) {
    device.wgpu_device().destroy();
    let _ = device.wgpu_device().poll(wgpu::Maintain::Wait);
    let status = device.status();
    let GpuDeviceStatus::Lost(loss) = status else {
        panic!("destroyed device must report loss");
    };
    assert_eq!(loss.reason(), GpuDeviceLossReason::Destroyed);
    assert_eq!(loss.generation(), device.generation());
}

#[test]
fn native_device_loss_is_detected_and_blocks_obsolete_gpu_work() {
    assert_send_sync::<GpuDeviceStatus>();

    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping device-loss contract");
        return;
    };
    assert_eq!(device.generation(), 1);
    assert_eq!(
        device.status(),
        GpuDeviceStatus::Available { generation: 1 }
    );

    destroy_and_detect(&device);

    let error = device.ensure_available().unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert_eq!(error.contexts()[0].field("device_generation"), Some("1"));

    let error = GpuResources::new(&device).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unavailable);
    let error = GpuSubmissionQueue::new(&device).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unavailable);
}

#[test]
fn recovery_recreates_the_device_and_reconstructs_real_dependent_resources() {
    assert_send_sync::<GpuRecoveryPlan>();
    assert_send_sync::<RecoveredGpu>();

    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping resource recovery contract");
        return;
    };
    let old_resources = GpuResources::new(&device).unwrap();
    let old_texture = old_resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("obsolete texture"),
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

    let mut plan = GpuRecoveryPlan::new().unwrap();
    let seed = plan
        .register("seed-buffer", |resources, _| {
            let buffer = resources.create_buffer(&wgpu::BufferDescriptor {
                label: Some("recovered seed"),
                size: 4,
                usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })?;
            resources.write_buffer(&buffer, 0, &[9, 8, 7, 6])?;
            Ok(buffer)
        })
        .unwrap();
    let seed_dependency = seed;
    let dependent = plan
        .register("dependent-texture", move |resources, reconstructed| {
            let seed = reconstructed.get(&seed_dependency)?;
            assert_eq!(seed.info().size(), 4);
            let texture = resources.create_texture(&wgpu::TextureDescriptor {
                label: Some("recovered dependent"),
                size: wgpu::Extent3d {
                    width: seed.info().size() as u32,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Uint,
                usage: wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })?;
            resources.write_texture(
                &texture,
                &[1, 2, 3, 4],
                GpuRecoveryTextureWrite::new(
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(4),
                        rows_per_image: Some(1),
                    },
                    wgpu::Extent3d {
                        width: 4,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                ),
            )?;
            Ok(texture)
        })
        .unwrap();

    destroy_and_detect(&device);
    let mut notices = Vec::new();
    let recovered = pollster::block_on(
        plan.recover_with_observer(&device, |notice| notices.push(notice.clone())),
    )
    .unwrap();

    assert_eq!(recovered.device().generation(), 2);
    assert_eq!(recovered.device().adapter(), device.adapter());
    assert_eq!(recovered.report().previous_generation(), 1);
    assert_eq!(recovered.report().generation(), 2);
    assert_eq!(recovered.report().reconstructed_resources(), 2);
    assert_eq!(
        recovered.report().resource_labels(),
        &["seed-buffer", "dependent-texture"]
    );
    assert_eq!(
        notices.first().unwrap().phase(),
        GpuRecoveryPhase::DeviceLost
    );
    assert_eq!(notices.last().unwrap().phase(), GpuRecoveryPhase::Recovered);
    assert_eq!(
        notices.first().unwrap().recoverability(),
        Some(Recoverability::Retryable)
    );
    assert_eq!(notices.last().unwrap().recoverability(), None);
    assert!(notices
        .iter()
        .all(|notice| !notice.user_message().contains("Device destroyed")));

    let seed_buffer = recovered.resources().get(&seed).unwrap().clone();
    let texture = recovered.resources().get(&dependent).unwrap().clone();
    assert_eq!(texture.info().size().width, 4);

    let resources = GpuResources::new(recovered.device()).unwrap();
    let obsolete = resources
        .create_texture_view(&old_texture, &wgpu::TextureViewDescriptor::default())
        .unwrap_err();
    assert_eq!(obsolete.category(), ErrorCategory::Conflict);

    let readback = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("recovery readback"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
        .unwrap();
    let texture_readback = resources
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("recovered texture readback"),
            size: wgpu::COPY_BYTES_PER_ROW_ALIGNMENT.into(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
        .unwrap();
    let mut encoder =
        recovered
            .device()
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("recovery readback"),
            });
    encoder.copy_buffer_to_buffer(seed_buffer.raw(), 0, readback.raw(), 0, 4);
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: texture.raw(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: texture_readback.raw(),
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
                rows_per_image: Some(1),
            },
        },
        wgpu::Extent3d {
            width: 4,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let submissions = GpuSubmissionQueue::new(recovered.device()).unwrap();
    let mut retained = submissions.resources();
    retained.retain(seed_buffer);
    retained.retain(readback.clone());
    retained.retain(texture);
    retained.retain(texture_readback.clone());
    let fence = submissions.submit([encoder.finish()], retained).unwrap();
    submissions.wait(&fence).unwrap();

    let mapped = Arc::new(Mutex::new(None));
    let mapped_result = Arc::clone(&mapped);
    readback
        .raw()
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            *mapped_result.lock().unwrap() = Some(result);
        });
    let _ = recovered.device().wgpu_device().poll(wgpu::Maintain::Wait);
    mapped.lock().unwrap().take().unwrap().unwrap();
    assert_eq!(&*readback.raw().slice(..).get_mapped_range(), &[9, 8, 7, 6]);
    readback.raw().unmap();

    let mapped = Arc::new(Mutex::new(None));
    let mapped_result = Arc::clone(&mapped);
    texture_readback
        .raw()
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            *mapped_result.lock().unwrap() = Some(result);
        });
    let _ = recovered.device().wgpu_device().poll(wgpu::Maintain::Wait);
    mapped.lock().unwrap().take().unwrap().unwrap();
    let texture_bytes = texture_readback.raw().slice(..).get_mapped_range();
    assert_eq!(&texture_bytes[..4], &[1, 2, 3, 4]);
    drop(texture_bytes);
    texture_readback.raw().unmap();
}

struct DropSignal(Arc<AtomicBool>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[test]
fn recovery_failure_discards_partial_results_and_reports_safe_actionable_state() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping recovery failure contract");
        return;
    };

    let mut healthy_plan = GpuRecoveryPlan::new().unwrap();
    let healthy = pollster::block_on(healthy_plan.recover(&device)).unwrap_err();
    assert_eq!(healthy.category(), ErrorCategory::Conflict);
    assert_eq!(healthy.recoverability(), Recoverability::UserCorrectable);

    let dropped = Arc::new(AtomicBool::new(false));
    let drop_signal = Arc::clone(&dropped);
    let first = healthy_plan
        .register("prepared-owner", move |_, _| {
            Ok(DropSignal(Arc::clone(&drop_signal)))
        })
        .unwrap();
    healthy_plan
        .register("failing-dependent", move |_, reconstructed| {
            let _ = reconstructed.get(&first)?;
            Err::<u32, _>(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "private reconstruction detail",
            ))
        })
        .unwrap();

    destroy_and_detect(&device);
    let mut notices = Vec::new();
    let error = pollster::block_on(
        healthy_plan.recover_with_observer(&device, |notice| notices.push(notice.clone())),
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert_eq!(
        error.contexts().last().unwrap().operation(),
        "reconstruct_resource"
    );
    assert!(dropped.load(Ordering::Acquire));
    let failed = notices.last().unwrap();
    assert_eq!(failed.phase(), GpuRecoveryPhase::Failed);
    assert_eq!(failed.recoverability(), Some(Recoverability::Retryable));
    assert!(!failed
        .user_message()
        .contains("private reconstruction detail"));
    assert!(failed.diagnostic().is_some());
}
