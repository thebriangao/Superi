use std::any::Any;
use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_engine::frame_upload::{UploadedVideoFrame, VideoFrameUploader};
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::pool::{GpuMemoryPool, MemoryBudget};
use superi_gpu::upload::UploadConfig;
use superi_media_io::decode::{
    CpuVideoBuffer, FrameStorageKind, VideoFormat, VideoFrame, VideoFrameBuffer, VideoPlane,
};
use superi_media_io::demux::MetadataValue;

fn assert_send_sync<T: Send + Sync>() {}

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(
        adapter.create_device(
            &DeviceRequest::default().with_label("superi engine frame upload contract"),
        ),
    )
    .ok()
}

#[test]
fn decoded_video_frame_upload_preserves_semantics_without_retaining_cpu_pixels() {
    assert_send_sync::<UploadedVideoFrame<'static>>();

    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping frame upload consumer");
        return;
    };
    let uploader = VideoFrameUploader::new(&device).unwrap();
    let format = VideoFormat::new(
        2,
        2,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap();
    let timebase = Timebase::integer(24).unwrap();
    let timestamp = RationalTime::new(7, timebase);
    let duration = Duration::new(1, timebase).unwrap();
    let pixels: Arc<[u8]> = Arc::from([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
    let plane = VideoPlane::new(Arc::clone(&pixels), 8, 2).unwrap();
    let buffer = Arc::new(CpuVideoBuffer::new(2, 2, PixelFormat::Rgba8Unorm, vec![plane]).unwrap());
    let source = VideoFrame::new(format, timestamp, duration, buffer)
        .unwrap()
        .with_metadata("source.frame", MetadataValue::Unsigned(7))
        .unwrap();
    assert_eq!(Arc::strong_count(&pixels), 2);

    let uploaded = uploader.upload(&source).unwrap();
    assert_eq!(uploaded.format(), format);
    assert_eq!(uploaded.timestamp(), timestamp);
    assert_eq!(uploaded.duration(), duration);
    assert_eq!(uploaded.metadata(), source.metadata());
    assert_eq!(uploaded.color_pipeline(), source.color_pipeline());
    assert_eq!(uploaded.gpu_frame().width(), 2);
    assert_eq!(uploaded.gpu_frame().height(), 2);
    assert_eq!(uploaded.gpu_frame().pixel_format(), PixelFormat::Rgba8Unorm);
    assert_eq!(uploaded.gpu_frame().planes().len(), 1);
    assert_eq!(uploaded.gpu_frame().report().direct_planes(), 1);
    let allocation_id = uploaded.gpu_frame().planes()[0].allocation_id();

    let retained = uploaded.clone();
    drop(uploaded);
    assert_eq!(
        retained.gpu_frame().planes()[0].allocation_id(),
        allocation_id
    );
    drop(source);
    assert_eq!(Arc::strong_count(&pixels), 1);
}

#[derive(Debug)]
struct ExternalFrame;

impl VideoFrameBuffer for ExternalFrame {
    fn storage_kind(&self) -> FrameStorageKind {
        FrameStorageKind::External
    }

    fn width(&self) -> u32 {
        2
    }

    fn height(&self) -> u32 {
        2
    }

    fn pixel_format(&self) -> PixelFormat {
        PixelFormat::Rgba8Unorm
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[test]
fn non_importable_decoder_storage_returns_a_degraded_fallback_error() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping storage fallback contract");
        return;
    };
    let uploader = VideoFrameUploader::new(&device).unwrap();
    let format = VideoFormat::new(
        2,
        2,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap();
    let timebase = Timebase::integer(24).unwrap();
    let source = VideoFrame::new(
        format,
        RationalTime::new(0, timebase),
        Duration::new(1, timebase).unwrap(),
        Arc::new(ExternalFrame),
    )
    .unwrap();

    let error = uploader.upload(&source).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(error.recoverability(), Recoverability::Degraded);
    assert_eq!(
        error.contexts().last().unwrap().component(),
        "superi-engine.frame-upload"
    );
    assert_eq!(
        error.contexts().last().unwrap().operation(),
        "upload_video_frame"
    );
}

#[test]
fn decoded_frame_upload_obeys_the_shared_gpu_memory_budget() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping budgeted frame upload consumer");
        return;
    };
    let memory = GpuMemoryPool::new(MemoryBudget::new(8, 8).unwrap());
    let uploader =
        VideoFrameUploader::with_memory_pool(&device, UploadConfig::default(), memory).unwrap();
    let format = VideoFormat::new(
        2,
        2,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap();
    let timebase = Timebase::integer(24).unwrap();
    let plane = VideoPlane::new(Arc::from([0_u8; 16]), 8, 2).unwrap();
    let buffer = Arc::new(CpuVideoBuffer::new(2, 2, PixelFormat::Rgba8Unorm, vec![plane]).unwrap());
    let source = VideoFrame::new(
        format,
        RationalTime::new(0, timebase),
        Duration::new(1, timebase).unwrap(),
        buffer,
    )
    .unwrap();

    let error = uploader.upload(&source).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert!(error
        .contexts()
        .iter()
        .any(|context| context.component() == "superi-gpu.memory-pool"));
}
