//! Codec-neutral decoded frame and decoder contracts.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{AlphaMode, ChromaSubsampling, PixelFormat, PixelModel, PixelPacking};
use superi_core::time::{Duration, RationalTime};
use superi_image::metadata::{ColorPipelineMetadata, ImageColorTags};

use crate::audio_io::{AudioBlock, AudioFormat};
use crate::demux::{MediaMetadata, MetadataValue, Packet, StreamInfo, StreamKind};
use crate::operation::OperationContext;

/// Complete decoded video representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoFormat {
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    color_space: ColorSpace,
    alpha_mode: AlphaMode,
}

impl VideoFormat {
    /// Creates a decoded video format with explicit color and alpha meaning.
    pub fn new(
        width: u32,
        height: u32,
        pixel_format: PixelFormat,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(invalid(
                "create_video_format",
                "video dimensions must be greater than zero",
            ));
        }
        if !pixel_format.has_alpha() && alpha_mode != AlphaMode::Opaque {
            return Err(invalid(
                "create_video_format",
                "non-alpha pixel formats must use opaque alpha mode",
            ));
        }
        Ok(Self {
            width,
            height,
            pixel_format,
            color_space,
            alpha_mode,
        })
    }

    /// Returns stored width in pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns stored height in pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns the exact pixel representation.
    #[must_use]
    pub const fn pixel_format(self) -> PixelFormat {
        self.pixel_format
    }

    /// Returns source color interpretation.
    #[must_use]
    pub const fn color_space(self) -> ColorSpace {
        self.color_space
    }

    /// Returns alpha association semantics.
    #[must_use]
    pub const fn alpha_mode(self) -> AlphaMode {
        self.alpha_mode
    }
}

/// The ownership domain of decoded video storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameStorageKind {
    /// Immutable CPU-addressable planes.
    Cpu,
    /// A GPU-resident resource owned by a backend implementation.
    Gpu,
    /// Another backend-owned storage domain.
    External,
}

/// Object-safe decoded video storage boundary.
pub trait VideoFrameBuffer: Any + fmt::Debug + Send + Sync {
    /// Returns the storage ownership domain.
    fn storage_kind(&self) -> FrameStorageKind;

    /// Returns stored width.
    fn width(&self) -> u32;

    /// Returns stored height.
    fn height(&self) -> u32;

    /// Returns the exact pixel representation.
    fn pixel_format(&self) -> PixelFormat;

    /// Enables checked access to a concrete backend or CPU buffer type.
    fn as_any(&self) -> &dyn Any;
}

/// One immutable CPU video plane with explicit stride and row count.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoPlane {
    bytes: Arc<[u8]>,
    stride: usize,
    row_count: u32,
}

impl VideoPlane {
    /// Creates a plane whose byte length exactly covers all stored rows.
    pub fn new(bytes: Arc<[u8]>, stride: usize, row_count: u32) -> Result<Self> {
        if stride == 0 || row_count == 0 {
            return Err(invalid(
                "create_video_plane",
                "video plane stride and row count must be greater than zero",
            ));
        }
        let expected = stride
            .checked_mul(usize::try_from(row_count).map_err(|_| {
                invalid(
                    "create_video_plane",
                    "video plane row count cannot be represented on this platform",
                )
            })?)
            .ok_or_else(|| invalid("create_video_plane", "video plane byte size overflowed"))?;
        if bytes.len() != expected {
            return Err(invalid(
                "create_video_plane",
                "video plane bytes must exactly match stride times row count",
            ));
        }
        Ok(Self {
            bytes,
            stride,
            row_count,
        })
    }

    /// Returns immutable plane bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns bytes between adjacent row starts.
    #[must_use]
    pub const fn stride(&self) -> usize {
        self.stride
    }

    /// Returns stored rows.
    #[must_use]
    pub const fn row_count(&self) -> u32 {
        self.row_count
    }
}

/// Validated immutable CPU storage for packed, planar, or semiplanar pixels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CpuVideoBuffer {
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    planes: Vec<VideoPlane>,
}

impl CpuVideoBuffer {
    /// Creates CPU video storage and validates plane count, rows, and minimum stride.
    pub fn new(
        width: u32,
        height: u32,
        pixel_format: PixelFormat,
        planes: Vec<VideoPlane>,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(invalid(
                "create_cpu_video_buffer",
                "video dimensions must be greater than zero",
            ));
        }
        let requirements = plane_requirements(width, height, pixel_format)?;
        if planes.len() != requirements.len() {
            return Err(invalid(
                "create_cpu_video_buffer",
                "video plane count does not match the pixel format",
            ));
        }
        for (plane, (minimum_stride, rows)) in planes.iter().zip(requirements) {
            if plane.row_count != rows || plane.stride < minimum_stride {
                return Err(invalid(
                    "create_cpu_video_buffer",
                    "video plane geometry does not match the pixel format",
                ));
            }
        }
        Ok(Self {
            width,
            height,
            pixel_format,
            planes,
        })
    }

    /// Returns planes in canonical component order.
    #[must_use]
    pub fn planes(&self) -> &[VideoPlane] {
        &self.planes
    }
}

impl VideoFrameBuffer for CpuVideoBuffer {
    fn storage_kind(&self) -> FrameStorageKind {
        FrameStorageKind::Cpu
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// One decoded video frame with exact presentation timing and metadata.
#[derive(Clone, Debug)]
pub struct VideoFrame {
    format: VideoFormat,
    timestamp: RationalTime,
    duration: Duration,
    buffer: Arc<dyn VideoFrameBuffer>,
    metadata: MediaMetadata,
    color_pipeline: ColorPipelineMetadata,
}

impl VideoFrame {
    /// Creates a frame and verifies timing and storage descriptors agree.
    pub fn new(
        format: VideoFormat,
        timestamp: RationalTime,
        duration: Duration,
        buffer: Arc<dyn VideoFrameBuffer>,
    ) -> Result<Self> {
        if timestamp.timebase() != duration.timebase() {
            return Err(invalid(
                "create_video_frame",
                "video timestamp and duration must use the same timebase",
            ));
        }
        if format.width != buffer.width()
            || format.height != buffer.height()
            || format.pixel_format != buffer.pixel_format()
        {
            return Err(invalid(
                "create_video_frame",
                "video format and buffer descriptors do not match",
            ));
        }
        let color_pipeline = ColorPipelineMetadata::new(ImageColorTags::new(format.color_space()))?;
        Ok(Self {
            format,
            timestamp,
            duration,
            buffer,
            metadata: MediaMetadata::new(),
            color_pipeline,
        })
    }

    /// Adds preserved decoded-frame metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: MetadataValue) -> Result<Self> {
        self.metadata.insert(key, value)?;
        Ok(self)
    }

    /// Replaces typed color history after validating the decoded source interpretation.
    pub fn with_color_pipeline(mut self, color_pipeline: ColorPipelineMetadata) -> Result<Self> {
        if color_pipeline.source_tags().interpretation() != self.format.color_space {
            return Err(invalid(
                "set_video_frame_color_pipeline",
                "color pipeline source interpretation must match the decoded video format",
            ));
        }
        self.color_pipeline = color_pipeline;
        Ok(self)
    }

    /// Returns complete video representation.
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

    /// Returns shared immutable storage.
    #[must_use]
    pub fn buffer(&self) -> &dyn VideoFrameBuffer {
        self.buffer.as_ref()
    }

    /// Clones the shared storage owner without copying pixels.
    #[must_use]
    pub fn shared_buffer(&self) -> Arc<dyn VideoFrameBuffer> {
        Arc::clone(&self.buffer)
    }

    /// Returns preserved frame metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }

    /// Returns exact source color identity and ordered transform history.
    #[must_use]
    pub const fn color_pipeline(&self) -> &ColorPipelineMetadata {
        &self.color_pipeline
    }
}

/// Immutable decoder configuration for one source stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecoderConfig {
    stream: StreamInfo,
    audio_format: Option<AudioFormat>,
}

impl DecoderConfig {
    /// Creates a decoder configuration.
    #[must_use]
    pub const fn new(stream: StreamInfo) -> Self {
        Self {
            stream,
            audio_format: None,
        }
    }

    /// Adds the explicit decoded audio representation required by headerless codecs.
    pub fn with_audio_format(mut self, audio_format: AudioFormat) -> Result<Self> {
        if self.stream.kind() != StreamKind::Audio {
            return Err(invalid(
                "configure_audio_decoder",
                "an explicit audio format requires an audio stream",
            ));
        }
        self.audio_format = Some(audio_format);
        Ok(self)
    }

    /// Returns the selected source stream.
    #[must_use]
    pub const fn stream(&self) -> &StreamInfo {
        &self.stream
    }

    /// Returns the externally described audio representation when required by the codec.
    #[must_use]
    pub const fn audio_format(&self) -> Option<&AudioFormat> {
        self.audio_format.as_ref()
    }
}

/// One nonblocking decoder receive result.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DecodeOutput {
    /// A decoded video frame is ready.
    Frame(VideoFrame),
    /// A decoded audio block is ready.
    Audio(AudioBlock),
    /// The decoder needs another compressed packet.
    NeedInput,
    /// All buffered output has been returned after flush.
    EndOfStream,
}

/// Codec-neutral packet-to-frame decoder lifecycle.
pub trait Decoder: Send {
    /// Returns immutable decoder configuration.
    fn config(&self) -> &DecoderConfig;

    /// Supplies one compressed packet in decode order.
    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()>;

    /// Receives one output item or an explicit lifecycle state.
    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput>;

    /// Signals end of input while retaining buffered output for draining.
    fn flush(&mut self, operation: &OperationContext) -> Result<()>;

    /// Discards buffered state so decoding can resume after a seek.
    fn reset(&mut self, operation: &OperationContext) -> Result<()>;
}

fn plane_requirements(
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
) -> Result<Vec<(usize, u32)>> {
    let width = usize::try_from(width).map_err(|_| {
        invalid(
            "create_cpu_video_buffer",
            "video width cannot be represented on this platform",
        )
    })?;
    if pixel_format.packing() == PixelPacking::Packed {
        let bytes_per_pixel =
            usize::from(pixel_format.packed_bytes_per_pixel().ok_or_else(|| {
                invalid(
                    "create_cpu_video_buffer",
                    "packed pixel format does not expose bytes per pixel",
                )
            })?);
        let stride = width
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| invalid("create_cpu_video_buffer", "video row byte size overflowed"))?;
        return Ok(vec![(stride, height)]);
    }

    let bytes = if matches!(
        pixel_format,
        PixelFormat::Yuv420p10
            | PixelFormat::Yuv422p10
            | PixelFormat::Yuv444p10
            | PixelFormat::P010
    ) {
        2
    } else {
        1
    };
    if pixel_format.packing() == PixelPacking::Semiplanar {
        let chroma_rows = ceil_div_u32(height, 2);
        let stride = width
            .checked_mul(bytes)
            .ok_or_else(|| invalid("create_cpu_video_buffer", "video row byte size overflowed"))?;
        return Ok(vec![(stride, height), (stride, chroma_rows)]);
    }

    let component_count = match pixel_format.model() {
        PixelModel::Gray => 1,
        PixelModel::Rg => 2,
        PixelModel::Rgb | PixelModel::Bgr | PixelModel::Yuv => 3,
        PixelModel::Rgba | PixelModel::Bgra => 4,
        _ => {
            return Err(invalid(
                "create_cpu_video_buffer",
                "unsupported planar pixel model",
            ));
        }
    };
    let luma_stride = width
        .checked_mul(bytes)
        .ok_or_else(|| invalid("create_cpu_video_buffer", "video row byte size overflowed"))?;
    let mut result = vec![(luma_stride, height)];
    if component_count == 1 {
        return Ok(result);
    }
    let (chroma_width, chroma_height) = match pixel_format.chroma_subsampling() {
        Some(ChromaSubsampling::Cs420) => (width.div_ceil(2), ceil_div_u32(height, 2)),
        Some(ChromaSubsampling::Cs422) => (width.div_ceil(2), height),
        Some(ChromaSubsampling::Cs444) | None => (width, height),
        _ => (width, height),
    };
    let chroma_stride = chroma_width
        .checked_mul(bytes)
        .ok_or_else(|| invalid("create_cpu_video_buffer", "video row byte size overflowed"))?;
    result.extend((1..component_count).map(|_| (chroma_stride, chroma_height)));
    Ok(result)
}

const fn ceil_div_u32(value: u32, divisor: u32) -> u32 {
    value / divisor + if value % divisor == 0 { 0 } else { 1 }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-media-io.decode", operation))
}
