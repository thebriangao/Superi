//! Decoded-frame upload into reusable GPU-resident textures.

use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{ChromaSubsampling, PixelFormat, PixelPacking};

use crate::device::GpuDevice;
use crate::pool::GpuMemoryPool;
use crate::resource::{GpuResourceId, GpuResources};
use crate::texture::GpuTexture;
use crate::texture_pool::{
    PooledTexture, TextureAlignment, TexturePool, TexturePoolConfig, TexturePoolStats,
    TextureRequest,
};

const COMPONENT: &str = "superi-gpu.upload";

/// One immutable decoded plane with its complete stored row layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedPlane<'a> {
    bytes: &'a [u8],
    bytes_per_row: usize,
    row_count: u32,
}

impl<'a> DecodedPlane<'a> {
    /// Creates a plane whose bytes exactly cover every stored row.
    pub fn new(bytes: &'a [u8], bytes_per_row: usize, row_count: u32) -> Result<Self> {
        if bytes_per_row == 0 || row_count == 0 {
            return Err(invalid(
                "create_decoded_plane",
                "decoded plane stride and row count must be greater than zero",
            ));
        }
        let rows = usize::try_from(row_count).map_err(|_| {
            invalid(
                "create_decoded_plane",
                "decoded plane row count cannot be represented on this platform",
            )
        })?;
        let expected = bytes_per_row.checked_mul(rows).ok_or_else(|| {
            exhausted(
                "create_decoded_plane",
                "decoded plane byte length overflowed",
            )
        })?;
        if bytes.len() != expected {
            return Err(invalid(
                "create_decoded_plane",
                "decoded plane bytes must exactly match stride times row count",
            ));
        }
        Ok(Self {
            bytes,
            bytes_per_row,
            row_count,
        })
    }

    /// Returns the immutable source bytes.
    #[must_use]
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the byte distance between adjacent source rows.
    #[must_use]
    pub const fn bytes_per_row(self) -> usize {
        self.bytes_per_row
    }

    /// Returns the number of stored source rows.
    #[must_use]
    pub const fn row_count(self) -> u32 {
        self.row_count
    }
}

/// Validated source and destination geometry for one decoded plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaneUploadLayout {
    source_size: wgpu::Extent3d,
    texture_size: wgpu::Extent3d,
    texture_format: wgpu::TextureFormat,
    tight_bytes_per_row: usize,
}

impl PlaneUploadLayout {
    /// Returns the source plane's logical sample extent.
    #[must_use]
    pub const fn source_size(self) -> wgpu::Extent3d {
        self.source_size
    }

    /// Returns the destination texture extent written by the upload.
    #[must_use]
    pub const fn texture_size(self) -> wgpu::Extent3d {
        self.texture_size
    }

    /// Returns the destination wgpu storage format.
    #[must_use]
    pub const fn texture_format(self) -> wgpu::TextureFormat {
        self.texture_format
    }

    /// Returns the minimum source bytes required for one row.
    #[must_use]
    pub const fn tight_bytes_per_row(self) -> usize {
        self.tight_bytes_per_row
    }
}

/// A decoded frame prepared for validated CPU-to-GPU plane upload.
#[derive(Clone, Debug)]
pub struct DecodedFrameUpload<'a> {
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    planes: Vec<DecodedPlane<'a>>,
    plane_layouts: Vec<PlaneUploadLayout>,
}

impl<'a> DecodedFrameUpload<'a> {
    /// Creates and validates the complete decoded-frame storage layout.
    pub fn new(
        width: u32,
        height: u32,
        pixel_format: PixelFormat,
        planes: Vec<DecodedPlane<'a>>,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(frame_invalid(
                width,
                height,
                pixel_format,
                "decoded frame dimensions must be greater than zero",
            ));
        }
        let plane_layouts = layouts_for(width, height, pixel_format)?;
        if planes.len() != plane_layouts.len() {
            return Err(frame_invalid(
                width,
                height,
                pixel_format,
                "decoded plane count does not match the pixel format",
            ));
        }
        for (index, (plane, layout)) in planes.iter().zip(&plane_layouts).enumerate() {
            if plane.row_count != layout.source_size.height {
                return Err(plane_invalid(
                    width,
                    height,
                    pixel_format,
                    index,
                    "decoded plane row count does not match its logical extent",
                ));
            }
            if plane.bytes_per_row < layout.tight_bytes_per_row {
                return Err(plane_invalid(
                    width,
                    height,
                    pixel_format,
                    index,
                    "decoded plane stride is shorter than its logical row",
                ));
            }
        }
        Ok(Self {
            width,
            height,
            pixel_format,
            planes,
            plane_layouts,
        })
    }

    /// Returns the decoded frame width.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the decoded frame height.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns the unchanged source pixel representation.
    #[must_use]
    pub fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    /// Returns source planes in canonical component order.
    #[must_use]
    pub fn planes(&self) -> &[DecodedPlane<'a>] {
        &self.planes
    }

    /// Returns validated upload layouts in source plane order.
    #[must_use]
    pub fn plane_layouts(&self) -> &[PlaneUploadLayout] {
        &self.plane_layouts
    }
}

/// Allocation and idle-retention policy for decoded-frame upload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadConfig {
    alignment: TextureAlignment,
    pool: TexturePoolConfig,
}

impl UploadConfig {
    /// Creates an upload configuration from explicit alignment and pool policy.
    #[must_use]
    pub const fn new(alignment: TextureAlignment, pool: TexturePoolConfig) -> Self {
        Self { alignment, pool }
    }

    /// Returns the physical texture reuse granularity.
    #[must_use]
    pub const fn alignment(self) -> TextureAlignment {
        self.alignment
    }

    /// Returns the idle texture retention policy.
    #[must_use]
    pub const fn pool(self) -> TexturePoolConfig {
        self.pool
    }
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self::new(TextureAlignment::default(), TexturePoolConfig::default())
    }
}

/// Whether one source plane could use its decoder-provided row layout directly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaneUploadPath {
    /// The source slice and stride were passed directly to wgpu staging.
    Direct,
    /// Rows required a tightly packed temporary because the source stride was not copy-compatible.
    Repacked,
}

/// Exact CPU-side work scheduled by one decoded-frame upload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UploadReport {
    queue_writes: u32,
    direct_planes: u32,
    repacked_planes: u32,
    repacked_bytes: u64,
}

impl UploadReport {
    /// Returns the number of texture writes scheduled on the owned queue.
    #[must_use]
    pub const fn queue_writes(self) -> u32 {
        self.queue_writes
    }

    /// Returns the number of planes passed directly to wgpu staging.
    #[must_use]
    pub const fn direct_planes(self) -> u32 {
        self.direct_planes
    }

    /// Returns the number of planes that required a caller-side row repack.
    #[must_use]
    pub const fn repacked_planes(self) -> u32 {
        self.repacked_planes
    }

    /// Returns the exact number of bytes copied by caller-side row repacking.
    #[must_use]
    pub const fn repacked_bytes(self) -> u64 {
        self.repacked_bytes
    }
}

/// One uploaded plane and its retained pooled allocation.
#[derive(Debug)]
pub struct UploadedPlane<'device> {
    texture: PooledTexture<'device>,
    layout: PlaneUploadLayout,
    upload_path: PlaneUploadPath,
}

impl UploadedPlane<'_> {
    /// Returns the physical allocation's process-local identifier.
    #[must_use]
    pub fn allocation_id(&self) -> GpuResourceId {
        self.texture.allocation_id()
    }

    /// Returns the source plane's logical sample extent.
    #[must_use]
    pub const fn source_size(&self) -> wgpu::Extent3d {
        self.layout.source_size
    }

    /// Returns the destination texture extent initialized by this upload.
    #[must_use]
    pub const fn texture_size(&self) -> wgpu::Extent3d {
        self.layout.texture_size
    }

    /// Returns the aligned physical allocation extent.
    #[must_use]
    pub const fn allocation_size(&self) -> wgpu::Extent3d {
        self.texture.allocation_size()
    }

    /// Returns the lossless wgpu storage format used for this plane.
    #[must_use]
    pub const fn texture_format(&self) -> wgpu::TextureFormat {
        self.layout.texture_format
    }

    /// Returns whether a caller-side row repack was necessary.
    #[must_use]
    pub const fn upload_path(&self) -> PlaneUploadPath {
        self.upload_path
    }

    /// Borrows the managed texture for subsequent GPU processing.
    #[must_use]
    pub fn texture(&self) -> &GpuTexture {
        self.texture.texture()
    }
}

#[derive(Debug)]
struct UploadedFrameInner<'device> {
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    planes: Vec<UploadedPlane<'device>>,
    report: UploadReport,
}

/// A cloneable GPU-resident decoded frame.
///
/// The final clone returns every plane to its originating texture pool.
#[derive(Clone, Debug)]
pub struct UploadedFrame<'device>(Arc<UploadedFrameInner<'device>>);

impl UploadedFrame<'_> {
    /// Returns the decoded frame width.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.0.width
    }

    /// Returns the decoded frame height.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.0.height
    }

    /// Returns the unchanged source pixel representation.
    #[must_use]
    pub fn pixel_format(&self) -> PixelFormat {
        self.0.pixel_format
    }

    /// Returns GPU-resident planes in canonical source order.
    #[must_use]
    pub fn planes(&self) -> &[UploadedPlane<'_>] {
        &self.0.planes
    }

    /// Returns exact copy-path diagnostics for this upload.
    #[must_use]
    pub fn report(&self) -> UploadReport {
        self.0.report
    }
}

/// Device-bound decoded-frame uploader with aligned allocation reuse.
#[derive(Debug)]
pub struct DecodedFrameUploader<'device> {
    device: &'device GpuDevice,
    pool: TexturePool<'device>,
    alignment: TextureAlignment,
}

impl<'device> DecodedFrameUploader<'device> {
    /// Creates an uploader using the default exact-size pool policy.
    pub fn new(device: &'device GpuDevice) -> Result<Self> {
        Self::with_config(device, UploadConfig::default())
    }

    /// Creates an uploader with explicit alignment and idle retention.
    pub fn with_config(device: &'device GpuDevice, config: UploadConfig) -> Result<Self> {
        let resources = GpuResources::new(device)?;
        Ok(Self {
            device,
            pool: TexturePool::new(resources, config.pool),
            alignment: config.alignment,
        })
    }

    /// Creates an uploader with explicit reuse policy and shared GPU memory budget.
    pub fn with_memory_pool(
        device: &'device GpuDevice,
        config: UploadConfig,
        memory: GpuMemoryPool,
    ) -> Result<Self> {
        let resources = GpuResources::new(device)?;
        Ok(Self {
            device,
            pool: TexturePool::with_memory_pool(resources, config.pool, memory),
            alignment: config.alignment,
        })
    }

    /// Uploads every decoded plane and retains the resulting GPU allocations.
    ///
    /// Compatible source strides are passed directly to `Queue::write_texture`.
    /// Wgpu performs the one required immediate staging copy. A temporary row
    /// repack is created only when the decoder stride is not divisible by the
    /// destination texel block size.
    pub fn upload(&self, source: &DecodedFrameUpload<'_>) -> Result<UploadedFrame<'device>> {
        let write_plans = source
            .planes
            .iter()
            .zip(&source.plane_layouts)
            .map(|(plane, layout)| plane_write_plan(*plane, *layout))
            .collect::<Result<Vec<_>>>()?;
        let report = upload_report(&write_plans)?;

        let mut checkouts = Vec::with_capacity(source.plane_layouts.len());
        for (index, layout) in source.plane_layouts.iter().enumerate() {
            let request = TextureRequest::new(
                layout.texture_size,
                layout.texture_format,
                wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            )
            .with_alignment(self.alignment)
            .with_label(format!(
                "decoded {} plane {index}",
                source.pixel_format.code()
            ));
            checkouts.push(self.pool.acquire(&request)?);
        }

        let mut uploaded_planes = Vec::with_capacity(checkouts.len());
        for ((plane, layout), (texture, write_plan)) in source
            .planes
            .iter()
            .zip(&source.plane_layouts)
            .zip(checkouts.into_iter().zip(write_plans))
        {
            let path = if write_plan.direct {
                self.schedule_write(&texture, *layout, plane.bytes, write_plan.bytes_per_row)?;
                PlaneUploadPath::Direct
            } else {
                let packed = repack_rows(*plane, *layout);
                self.schedule_write(&texture, *layout, &packed, write_plan.bytes_per_row)?;
                PlaneUploadPath::Repacked
            };
            uploaded_planes.push(UploadedPlane {
                texture,
                layout: *layout,
                upload_path: path,
            });
        }

        Ok(UploadedFrame(Arc::new(UploadedFrameInner {
            width: source.width,
            height: source.height,
            pixel_format: source.pixel_format,
            planes: uploaded_planes,
            report,
        })))
    }

    /// Returns the current allocation and reuse counters for this uploader.
    pub fn pool_stats(&self) -> Result<TexturePoolStats> {
        self.pool.stats()
    }

    fn schedule_write(
        &self,
        texture: &PooledTexture<'_>,
        layout: PlaneUploadLayout,
        bytes: &[u8],
        bytes_per_row: u32,
    ) -> Result<()> {
        self.device.write_texture(
            wgpu::ImageCopyTexture {
                texture: texture.raw(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(layout.texture_size.height),
            },
            layout.texture_size,
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct PlaneWritePlan {
    direct: bool,
    bytes_per_row: u32,
    repacked_bytes: u64,
}

fn upload_report(write_plans: &[PlaneWritePlan]) -> Result<UploadReport> {
    let queue_writes = u32::try_from(write_plans.len()).map_err(|_| {
        exhausted(
            "plan_decoded_frame_upload",
            "decoded frame plane count exceeds diagnostics range",
        )
    })?;
    let direct_planes = u32::try_from(write_plans.iter().filter(|plan| plan.direct).count())
        .map_err(|_| {
            exhausted(
                "plan_decoded_frame_upload",
                "direct plane count exceeds diagnostics range",
            )
        })?;
    let repacked_planes = queue_writes
        .checked_sub(direct_planes)
        .expect("direct planes are a subset of queue writes");
    let repacked_bytes = write_plans.iter().try_fold(0_u64, |total, plan| {
        total.checked_add(plan.repacked_bytes).ok_or_else(|| {
            exhausted(
                "plan_decoded_frame_upload",
                "decoded upload repack byte count overflowed",
            )
        })
    })?;
    Ok(UploadReport {
        queue_writes,
        direct_planes,
        repacked_planes,
        repacked_bytes,
    })
}

fn plane_write_plan(plane: DecodedPlane<'_>, layout: PlaneUploadLayout) -> Result<PlaneWritePlan> {
    let block_size = layout
        .texture_format
        .block_copy_size(Some(wgpu::TextureAspect::All))
        .ok_or_else(|| {
            unsupported(
                "plan_decoded_plane_upload",
                "decoded plane texture format has no copy block size",
            )
        })?;
    let source_stride = u32::try_from(plane.bytes_per_row).ok();
    let direct = source_stride.is_some_and(|stride| stride % block_size == 0);
    let bytes_per_row = if direct {
        source_stride.expect("direct upload has a representable source stride")
    } else {
        u32::try_from(layout.tight_bytes_per_row).map_err(|_| {
            exhausted(
                "plan_decoded_plane_upload",
                "decoded plane row exceeds wgpu layout range",
            )
        })?
    };
    let repacked_bytes = if direct {
        0
    } else {
        u64::try_from(layout.tight_bytes_per_row)
            .ok()
            .and_then(|row| row.checked_mul(u64::from(layout.source_size.height)))
            .ok_or_else(|| {
                exhausted(
                    "plan_decoded_plane_upload",
                    "decoded plane repack byte count overflowed",
                )
            })?
    };
    Ok(PlaneWritePlan {
        direct,
        bytes_per_row,
        repacked_bytes,
    })
}

fn repack_rows(plane: DecodedPlane<'_>, layout: PlaneUploadLayout) -> Vec<u8> {
    let rows = usize::try_from(layout.source_size.height)
        .expect("validated decoded plane rows fit this platform");
    let mut packed = Vec::with_capacity(layout.tight_bytes_per_row * rows);
    for row in plane.bytes.chunks_exact(plane.bytes_per_row) {
        packed.extend_from_slice(&row[..layout.tight_bytes_per_row]);
    }
    packed
}

fn layouts_for(
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
) -> Result<Vec<PlaneUploadLayout>> {
    match pixel_format.packing() {
        PixelPacking::Packed => {
            packed_layout(width, height, pixel_format).map(|layout| vec![layout])
        }
        PixelPacking::Planar => planar_layouts(width, height, pixel_format),
        PixelPacking::Semiplanar => semiplanar_layouts(width, height, pixel_format),
        _ => Err(unsupported(
            "create_decoded_frame_upload",
            "pixel packing is not supported for decoded-frame upload",
        )),
    }
}

fn packed_layout(width: u32, height: u32, pixel_format: PixelFormat) -> Result<PlaneUploadLayout> {
    let bytes_per_pixel = usize::from(pixel_format.packed_bytes_per_pixel().ok_or_else(|| {
        frame_invalid(
            width,
            height,
            pixel_format,
            "packed pixel format does not expose bytes per pixel",
        )
    })?);
    let tight_bytes_per_row = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(bytes_per_pixel))
        .ok_or_else(|| {
            exhausted(
                "create_decoded_frame_upload",
                "decoded packed row byte length overflowed",
            )
        })?;
    let (texture_format, texture_width) = match pixel_format {
        PixelFormat::R8Unorm => (wgpu::TextureFormat::R8Unorm, width),
        PixelFormat::R16Unorm => (wgpu::TextureFormat::R16Uint, width),
        PixelFormat::R16Float => (wgpu::TextureFormat::R16Float, width),
        PixelFormat::R32Float => (wgpu::TextureFormat::R32Float, width),
        PixelFormat::Rg8Unorm => (wgpu::TextureFormat::Rg8Unorm, width),
        PixelFormat::Rg16Unorm => (wgpu::TextureFormat::Rg16Uint, width),
        PixelFormat::Rg16Float => (wgpu::TextureFormat::Rg16Float, width),
        PixelFormat::Rg32Float => (wgpu::TextureFormat::Rg32Float, width),
        PixelFormat::Rgb8Unorm | PixelFormat::Bgr8Unorm => (
            wgpu::TextureFormat::R8Unorm,
            width.checked_mul(3).ok_or_else(|| {
                exhausted(
                    "create_decoded_frame_upload",
                    "decoded RGB texture width overflowed",
                )
            })?,
        ),
        PixelFormat::Rgba8Unorm => (wgpu::TextureFormat::Rgba8Unorm, width),
        PixelFormat::Bgra8Unorm => (wgpu::TextureFormat::Bgra8Unorm, width),
        PixelFormat::Rgba16Unorm => (wgpu::TextureFormat::Rgba16Uint, width),
        PixelFormat::Rgba16Float => (wgpu::TextureFormat::Rgba16Float, width),
        PixelFormat::Rgba32Float => (wgpu::TextureFormat::Rgba32Float, width),
        _ => {
            return Err(frame_unsupported(
                width,
                height,
                pixel_format,
                "packed pixel format has no lossless upload representation",
            ));
        }
    };
    Ok(layout(
        width,
        height,
        texture_width,
        height,
        texture_format,
        tight_bytes_per_row,
    ))
}

fn planar_layouts(
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
) -> Result<Vec<PlaneUploadLayout>> {
    let bytes_per_sample = if pixel_format.bits_per_component() > 8 {
        2_usize
    } else {
        1_usize
    };
    let texture_format = if bytes_per_sample == 1 {
        wgpu::TextureFormat::R8Unorm
    } else {
        wgpu::TextureFormat::R16Uint
    };
    let (chroma_width, chroma_height) = chroma_size(width, height, pixel_format)?;
    let luma_row = row_bytes(width, bytes_per_sample)?;
    let chroma_row = row_bytes(chroma_width, bytes_per_sample)?;
    Ok(vec![
        layout(width, height, width, height, texture_format, luma_row),
        layout(
            chroma_width,
            chroma_height,
            chroma_width,
            chroma_height,
            texture_format,
            chroma_row,
        ),
        layout(
            chroma_width,
            chroma_height,
            chroma_width,
            chroma_height,
            texture_format,
            chroma_row,
        ),
    ])
}

fn semiplanar_layouts(
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
) -> Result<Vec<PlaneUploadLayout>> {
    let (bytes_per_sample, luma_format, chroma_format) = match pixel_format {
        PixelFormat::Nv12 => (
            1_usize,
            wgpu::TextureFormat::R8Unorm,
            wgpu::TextureFormat::Rg8Unorm,
        ),
        PixelFormat::P010 => (
            2_usize,
            wgpu::TextureFormat::R16Uint,
            wgpu::TextureFormat::Rg16Uint,
        ),
        _ => {
            return Err(frame_unsupported(
                width,
                height,
                pixel_format,
                "semiplanar pixel format has no lossless upload representation",
            ));
        }
    };
    let chroma_width = ceil_div_two(width);
    let chroma_height = ceil_div_two(height);
    let luma_row = row_bytes(width, bytes_per_sample)?;
    let chroma_row = row_bytes(
        chroma_width,
        bytes_per_sample.checked_mul(2).ok_or_else(|| {
            exhausted(
                "create_decoded_frame_upload",
                "decoded chroma sample size overflowed",
            )
        })?,
    )?;
    Ok(vec![
        layout(width, height, width, height, luma_format, luma_row),
        layout(
            chroma_width,
            chroma_height,
            chroma_width,
            chroma_height,
            chroma_format,
            chroma_row,
        ),
    ])
}

fn chroma_size(width: u32, height: u32, pixel_format: PixelFormat) -> Result<(u32, u32)> {
    match pixel_format.chroma_subsampling() {
        Some(ChromaSubsampling::Cs420) => Ok((ceil_div_two(width), ceil_div_two(height))),
        Some(ChromaSubsampling::Cs422) => Ok((ceil_div_two(width), height)),
        Some(ChromaSubsampling::Cs444) => Ok((width, height)),
        _ => Err(frame_unsupported(
            width,
            height,
            pixel_format,
            "planar pixel format has no supported chroma geometry",
        )),
    }
}

fn row_bytes(width: u32, bytes_per_texel: usize) -> Result<usize> {
    usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(bytes_per_texel))
        .ok_or_else(|| {
            exhausted(
                "create_decoded_frame_upload",
                "decoded plane row byte length overflowed",
            )
        })
}

const fn layout(
    source_width: u32,
    source_height: u32,
    texture_width: u32,
    texture_height: u32,
    texture_format: wgpu::TextureFormat,
    tight_bytes_per_row: usize,
) -> PlaneUploadLayout {
    PlaneUploadLayout {
        source_size: extent(source_width, source_height),
        texture_size: extent(texture_width, texture_height),
        texture_format,
        tight_bytes_per_row,
    }
}

const fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

const fn ceil_div_two(value: u32) -> u32 {
    value / 2 + value % 2
}

fn frame_context(width: u32, height: u32, pixel_format: PixelFormat) -> ErrorContext {
    ErrorContext::new(COMPONENT, "create_decoded_frame_upload")
        .with_field("width", width.to_string())
        .with_field("height", height.to_string())
        .with_field("pixel_format", pixel_format.code())
}

fn frame_invalid(
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    message: impl Into<String>,
) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(frame_context(width, height, pixel_format))
}

fn plane_invalid(
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    plane: usize,
    message: impl Into<String>,
) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(frame_context(width, height, pixel_format).with_field("plane", plane.to_string()))
}

fn frame_unsupported(
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    message: impl Into<String>,
) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(frame_context(width, height, pixel_format))
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn exhausted(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
