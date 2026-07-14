//! Engine integration from codec-neutral decoded frames to GPU-resident planes.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{Duration, RationalTime};
use superi_gpu::device::GpuDevice;
use superi_gpu::pool::GpuMemoryPool;
use superi_gpu::upload::{
    DecodedFrameUpload, DecodedFrameUploader, DecodedPlane, UploadConfig, UploadedFrame,
};
use superi_image::metadata::ColorPipelineMetadata;
use superi_media_io::decode::{
    CpuVideoBuffer, FrameStorageKind, VideoFormat, VideoFrame, VideoFrameBuffer,
};
use superi_media_io::demux::MediaMetadata;

const COMPONENT: &str = "superi-engine.frame-upload";

/// A decoded video frame whose pixels are resident in the owning GPU device.
#[derive(Clone, Debug)]
pub struct UploadedVideoFrame<'device> {
    format: VideoFormat,
    timestamp: RationalTime,
    duration: Duration,
    metadata: MediaMetadata,
    color_pipeline: ColorPipelineMetadata,
    gpu_frame: UploadedFrame<'device>,
}

impl UploadedVideoFrame<'_> {
    /// Returns the exact decoded video representation.
    #[must_use]
    pub const fn format(&self) -> VideoFormat {
        self.format
    }

    /// Returns the exact presentation timestamp.
    #[must_use]
    pub const fn timestamp(&self) -> RationalTime {
        self.timestamp
    }

    /// Returns the exact presentation duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns preserved decoded-frame metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }

    /// Returns exact source color identity and ordered transform history.
    #[must_use]
    pub const fn color_pipeline(&self) -> &ColorPipelineMetadata {
        &self.color_pipeline
    }

    /// Returns the GPU-resident plane owner.
    #[must_use]
    pub const fn gpu_frame(&self) -> &UploadedFrame<'_> {
        &self.gpu_frame
    }
}

/// The engine-owned decoded-frame upload boundary for one GPU device lifetime.
#[derive(Debug)]
pub struct VideoFrameUploader<'device> {
    uploader: DecodedFrameUploader<'device>,
}

impl<'device> VideoFrameUploader<'device> {
    /// Creates an uploader with the default allocation and retention policy.
    pub fn new(device: &'device GpuDevice) -> Result<Self> {
        Ok(Self {
            uploader: DecodedFrameUploader::new(device)?,
        })
    }

    /// Creates an uploader with explicit allocation and retention policy.
    pub fn with_config(device: &'device GpuDevice, config: UploadConfig) -> Result<Self> {
        Ok(Self {
            uploader: DecodedFrameUploader::with_config(device, config)?,
        })
    }

    /// Creates an uploader with explicit reuse policy and shared GPU memory budget.
    pub fn with_memory_pool(
        device: &'device GpuDevice,
        config: UploadConfig,
        memory: GpuMemoryPool,
    ) -> Result<Self> {
        Ok(Self {
            uploader: DecodedFrameUploader::with_memory_pool(device, config, memory)?,
        })
    }

    /// Uploads one CPU-addressable decoded frame without flattening its planes.
    ///
    /// Backend GPU and external storage remain backend-owned. Until a safe,
    /// capability-checked wgpu import path exists, those storage kinds return
    /// a degraded unsupported error so decode selection can fall back to CPU
    /// output without an implicit download.
    pub fn upload(&self, source: &VideoFrame) -> Result<UploadedVideoFrame<'device>> {
        let buffer = source.buffer();
        if buffer.storage_kind() != FrameStorageKind::Cpu {
            return Err(unsupported_storage(buffer));
        }
        let cpu = buffer
            .as_any()
            .downcast_ref::<CpuVideoBuffer>()
            .ok_or_else(|| unsupported_storage(buffer))?;
        let planes = cpu
            .planes()
            .iter()
            .map(|plane| DecodedPlane::new(plane.bytes(), plane.stride(), plane.row_count()))
            .collect::<Result<Vec<_>>>()?;
        let format = source.format();
        let decoded = DecodedFrameUpload::new(
            format.width(),
            format.height(),
            format.pixel_format(),
            planes,
        )
        .map_err(|error| error.with_context(frame_context(buffer)))?;
        let gpu_frame = self
            .uploader
            .upload(&decoded)
            .map_err(|error| error.with_context(frame_context(buffer)))?;
        Ok(UploadedVideoFrame {
            format,
            timestamp: source.timestamp(),
            duration: source.duration(),
            metadata: source.metadata().clone(),
            color_pipeline: source.color_pipeline().clone(),
            gpu_frame,
        })
    }
}

fn unsupported_storage(buffer: &dyn VideoFrameBuffer) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        "decoded frame storage cannot be imported into the active wgpu device",
    )
    .with_context(frame_context(buffer))
}

fn frame_context(buffer: &dyn VideoFrameBuffer) -> ErrorContext {
    ErrorContext::new(COMPONENT, "upload_video_frame")
        .with_field("storage_kind", storage_kind_code(buffer.storage_kind()))
        .with_field("width", buffer.width().to_string())
        .with_field("height", buffer.height().to_string())
        .with_field("pixel_format", buffer.pixel_format().code())
}

const fn storage_kind_code(kind: FrameStorageKind) -> &'static str {
    match kind {
        FrameStorageKind::Cpu => "cpu",
        FrameStorageKind::Gpu => "gpu",
        FrameStorageKind::External => "external",
        _ => "unknown",
    }
}
