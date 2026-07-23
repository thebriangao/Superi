//! Explicit GPU-to-CPU boundaries for export, thumbnail, and private inspection pixels.
//!
//! Ordinary processing has no readback entrypoint. Callers must name either an
//! export, thumbnail, or private inspection boundary, provide the exact visible color-texture
//! region, and submit the resulting copy through [`GpuSubmissionQueue`]. The
//! source and staging allocations remain retained until the returned fence
//! retires. Polling then removes wgpu row padding and returns immutable,
//! tightly packed bytes.

use std::fmt;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::buffer::GpuBuffer;
use crate::pool::{GpuMemoryPool, MemoryClass, MemoryEvictor};
use crate::resource::{GpuResourceId, GpuResources};
use crate::submission::{GpuFence, GpuSubmissionQueue};
use crate::texture::GpuTexture;

const COMPONENT: &str = "superi-gpu.readback";
const WAIT_INTERVAL: Duration = Duration::from_millis(1);

/// The only operations permitted to move finished image pixels to the CPU.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReadbackBoundary {
    /// Delivery pixels requested by an encoder or image-sequence writer.
    Export,
    /// Display-ready pixels requested by a thumbnail cache or browser.
    Thumbnail,
    /// Product pixels requested by the private deterministic interface inspector.
    Inspection,
}

impl ReadbackBoundary {
    /// Returns the stable diagnostic code for this boundary.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Export => "export",
            Self::Thumbnail => "thumbnail",
            Self::Inspection => "inspection",
        }
    }
}

impl fmt::Display for ReadbackBoundary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// One exact managed color-texture region requested at a CPU boundary.
#[derive(Clone, Debug)]
pub struct TextureReadbackRequest {
    boundary: ReadbackBoundary,
    source: GpuTexture,
    origin: wgpu::Origin3d,
    extent: wgpu::Extent3d,
    mip_level: u32,
}

impl TextureReadbackRequest {
    /// Creates an explicit export readback for one exact texture region.
    #[must_use]
    pub const fn for_export(
        source: GpuTexture,
        origin: wgpu::Origin3d,
        extent: wgpu::Extent3d,
    ) -> Self {
        Self {
            boundary: ReadbackBoundary::Export,
            source,
            origin,
            extent,
            mip_level: 0,
        }
    }

    /// Creates an explicit thumbnail readback for one exact texture region.
    #[must_use]
    pub const fn for_thumbnail(
        source: GpuTexture,
        origin: wgpu::Origin3d,
        extent: wgpu::Extent3d,
    ) -> Self {
        Self {
            boundary: ReadbackBoundary::Thumbnail,
            source,
            origin,
            extent,
            mip_level: 0,
        }
    }

    /// Creates an explicit private-interface readback for one exact texture region.
    #[must_use]
    pub const fn for_inspection(
        source: GpuTexture,
        origin: wgpu::Origin3d,
        extent: wgpu::Extent3d,
    ) -> Self {
        Self {
            boundary: ReadbackBoundary::Inspection,
            source,
            origin,
            extent,
            mip_level: 0,
        }
    }

    /// Selects one allocated mip level. The default is the base level.
    #[must_use]
    pub const fn with_mip_level(mut self, mip_level: u32) -> Self {
        self.mip_level = mip_level;
        self
    }

    /// Returns the named CPU boundary.
    #[must_use]
    pub const fn boundary(&self) -> ReadbackBoundary {
        self.boundary
    }

    /// Returns the managed source texture retained by this request.
    #[must_use]
    pub const fn source(&self) -> &GpuTexture {
        &self.source
    }

    /// Returns the first copied texel or array layer.
    #[must_use]
    pub const fn origin(&self) -> wgpu::Origin3d {
        self.origin
    }

    /// Returns the exact copied extent.
    #[must_use]
    pub const fn extent(&self) -> wgpu::Extent3d {
        self.extent
    }

    /// Returns the copied mip level.
    #[must_use]
    pub const fn mip_level(&self) -> u32 {
        self.mip_level
    }
}

/// Checked row and layer layout for one texture readback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextureReadbackLayout {
    format: wgpu::TextureFormat,
    extent: wgpu::Extent3d,
    tight_bytes_per_row: u32,
    padded_bytes_per_row: u32,
    rows_per_image: u32,
    tight_bytes_per_image: u64,
    staging_bytes: u64,
}

impl TextureReadbackLayout {
    /// Returns the unchanged GPU storage format represented by the bytes.
    #[must_use]
    pub const fn format(self) -> wgpu::TextureFormat {
        self.format
    }

    /// Returns the exact logical region represented by the bytes.
    #[must_use]
    pub const fn extent(self) -> wgpu::Extent3d {
        self.extent
    }

    /// Returns useful bytes in each output row.
    #[must_use]
    pub const fn tight_bytes_per_row(self) -> u32 {
        self.tight_bytes_per_row
    }

    /// Returns wgpu's aligned staging row pitch.
    #[must_use]
    pub const fn padded_bytes_per_row(self) -> u32 {
        self.padded_bytes_per_row
    }

    /// Returns rows stored for each array layer.
    #[must_use]
    pub const fn rows_per_image(self) -> u32 {
        self.rows_per_image
    }

    /// Returns useful output bytes in each array layer.
    #[must_use]
    pub const fn tight_bytes_per_image(self) -> u64 {
        self.tight_bytes_per_image
    }

    /// Returns the exact managed staging allocation size.
    #[must_use]
    pub const fn staging_bytes(self) -> u64 {
        self.staging_bytes
    }
}

/// A validated, encoded, one-shot texture copy awaiting ordered submission.
#[derive(Debug)]
#[must_use = "submit the encoded readback through GpuSubmissionQueue"]
pub struct EncodedTextureReadback {
    device_identity: Arc<()>,
    source_id: GpuResourceId,
    source: GpuTexture,
    staging: GpuBuffer,
    command_buffer: wgpu::CommandBuffer,
    boundary: ReadbackBoundary,
    origin: wgpu::Origin3d,
    mip_level: u32,
    layout: TextureReadbackLayout,
}

impl EncodedTextureReadback {
    /// Returns the named CPU boundary.
    #[must_use]
    pub const fn boundary(&self) -> ReadbackBoundary {
        self.boundary
    }

    /// Returns the checked copy layout.
    #[must_use]
    pub const fn layout(&self) -> TextureReadbackLayout {
        self.layout
    }

    /// Returns the managed staging buffer's diagnostic identifier.
    #[must_use]
    pub fn staging_buffer_id(&self) -> GpuResourceId {
        self.staging.id()
    }
}

/// Immutable, tightly packed bytes returned by a completed boundary operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextureReadbackResult {
    source_id: GpuResourceId,
    boundary: ReadbackBoundary,
    origin: wgpu::Origin3d,
    mip_level: u32,
    layout: TextureReadbackLayout,
    bytes: Vec<u8>,
}

impl TextureReadbackResult {
    /// Returns the source texture's process-local diagnostic identifier.
    #[must_use]
    pub const fn source_id(&self) -> GpuResourceId {
        self.source_id
    }

    /// Returns the named CPU boundary.
    #[must_use]
    pub const fn boundary(&self) -> ReadbackBoundary {
        self.boundary
    }

    /// Returns the copied texture origin.
    #[must_use]
    pub const fn origin(&self) -> wgpu::Origin3d {
        self.origin
    }

    /// Returns the copied mip level.
    #[must_use]
    pub const fn mip_level(&self) -> u32 {
        self.mip_level
    }

    /// Returns the checked layout. Returned bytes never include staging padding.
    #[must_use]
    pub const fn layout(&self) -> TextureReadbackLayout {
        self.layout
    }

    /// Returns all tightly packed layers in increasing layer and row order.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns one tightly packed row from one copied array layer.
    #[must_use]
    pub fn row(&self, layer: u32, row: u32) -> Option<&[u8]> {
        if layer >= self.layout.extent.depth_or_array_layers || row >= self.layout.rows_per_image {
            return None;
        }
        let row_index = u64::from(layer)
            .checked_mul(u64::from(self.layout.rows_per_image))?
            .checked_add(u64::from(row))?;
        let start = row_index.checked_mul(u64::from(self.layout.tight_bytes_per_row))?;
        let end = start.checked_add(u64::from(self.layout.tight_bytes_per_row))?;
        self.bytes
            .get(usize::try_from(start).ok()?..usize::try_from(end).ok()?)
    }
}

/// Device-scoped encoder for explicit texture readback operations.
#[derive(Debug)]
pub struct TextureReadbackManager<'device> {
    resources: GpuResources<'device>,
    memory: GpuMemoryPool,
}

impl<'device> TextureReadbackManager<'device> {
    /// Creates a manager with compatibility accounting and no practical limit.
    #[must_use]
    pub fn new(resources: GpuResources<'device>) -> Self {
        Self::with_memory_pool(resources, GpuMemoryPool::unbounded())
    }

    /// Creates a manager whose staging allocations share an explicit GPU budget.
    #[must_use]
    pub const fn with_memory_pool(resources: GpuResources<'device>, memory: GpuMemoryPool) -> Self {
        Self { resources, memory }
    }

    /// Returns the shared memory pool used by staging allocations.
    #[must_use]
    pub fn memory_pool(&self) -> GpuMemoryPool {
        self.memory.clone()
    }

    /// Validates and encodes one readback without implicit eviction.
    pub fn encode(&self, request: TextureReadbackRequest) -> Result<EncodedTextureReadback> {
        self.encode_with_eviction(request, &[])
    }

    /// Validates and encodes one readback with caller-ordered eviction cooperation.
    pub fn encode_with_eviction(
        &self,
        request: TextureReadbackRequest,
        evictors: &[&dyn MemoryEvictor],
    ) -> Result<EncodedTextureReadback> {
        let layout = validate_request(&self.resources, &request)?;
        let reservation = self
            .memory
            .reserve(layout.staging_bytes, MemoryClass::Buffer, evictors)
            .map_err(|error| error.with_context(request_context(&request, "reserve_readback")))?;
        let label = format!("{} texture readback staging", request.boundary.code());
        let staging = self.resources.create_buffer_with_reservation(
            &wgpu::BufferDescriptor {
                label: Some(&label),
                size: layout.staging_bytes,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
            reservation,
        )?;
        let encoder_label = format!("{} texture readback copy", request.boundary.code());
        let mut encoder =
            self.resources
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some(&encoder_label),
                });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: request.source.raw(),
                mip_level: request.mip_level,
                origin: request.origin,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: staging.raw(),
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(layout.padded_bytes_per_row),
                    rows_per_image: Some(layout.rows_per_image),
                },
            },
            request.extent,
        );

        Ok(EncodedTextureReadback {
            device_identity: Arc::clone(self.resources.device_identity()),
            source_id: request.source.id(),
            source: request.source,
            staging,
            command_buffer: encoder.finish(),
            boundary: request.boundary,
            origin: request.origin,
            mip_level: request.mip_level,
            layout,
        })
    }
}

type MapResult = std::result::Result<(), wgpu::BufferAsyncError>;

/// A submitted readback whose map callback is driven by the GPU submission owner.
#[must_use = "poll or wait for the submitted readback result"]
pub struct SubmittedTextureReadback {
    device_identity: Arc<()>,
    source_id: GpuResourceId,
    staging: Option<GpuBuffer>,
    fence: GpuFence,
    receiver: mpsc::Receiver<MapResult>,
    boundary: ReadbackBoundary,
    origin: wgpu::Origin3d,
    mip_level: u32,
    layout: TextureReadbackLayout,
    mapping_ready: bool,
    completed: bool,
}

impl fmt::Debug for SubmittedTextureReadback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubmittedTextureReadback")
            .field("source_id", &self.source_id)
            .field("boundary", &self.boundary)
            .field("origin", &self.origin)
            .field("mip_level", &self.mip_level)
            .field("layout", &self.layout)
            .field("fence", &self.fence)
            .field("mapping_ready", &self.mapping_ready)
            .field("completed", &self.completed)
            .finish_non_exhaustive()
    }
}

impl SubmittedTextureReadback {
    /// Returns the named CPU boundary.
    #[must_use]
    pub const fn boundary(&self) -> ReadbackBoundary {
        self.boundary
    }

    /// Returns the fence governing source and staging retirement.
    pub const fn fence(&self) -> &GpuFence {
        &self.fence
    }

    /// Polls once on the submission thread and returns a result when mapping is ready.
    pub fn poll(
        &mut self,
        submissions: &GpuSubmissionQueue<'_>,
    ) -> Result<Option<TextureReadbackResult>> {
        if self.completed {
            return Err(invalid_state(
                "poll_texture_readback",
                "submitted texture readback has already completed",
                self.source_id,
                self.boundary,
            ));
        }
        submissions.ensure_device_identity(&self.device_identity, "poll_texture_readback")?;
        let progress = submissions.poll();

        if !self.mapping_ready {
            match self.receiver.try_recv() {
                Ok(Ok(())) => self.mapping_ready = true,
                Ok(Err(source)) => {
                    self.completed = true;
                    return Err(Error::with_source(
                        ErrorCategory::Unavailable,
                        Recoverability::Retryable,
                        "GPU texture readback mapping failed",
                        source,
                    )
                    .with_context(submitted_context(
                        self.source_id,
                        self.boundary,
                        "map_texture_readback",
                    )));
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.completed = true;
                    return Err(Error::new(
                        ErrorCategory::Internal,
                        Recoverability::Terminal,
                        "GPU texture readback map callback disconnected",
                    )
                    .with_context(submitted_context(
                        self.source_id,
                        self.boundary,
                        "map_texture_readback",
                    )));
                }
            }
        }

        if !self.mapping_ready || progress.last_retired() < self.fence.value() {
            return Ok(None);
        }

        let staging = self.staging.as_ref().expect("active readback owns staging");
        let bytes = copy_tight_bytes(staging, self.layout)?;
        staging.raw().unmap();
        self.staging = None;
        self.completed = true;
        Ok(Some(TextureReadbackResult {
            source_id: self.source_id,
            boundary: self.boundary,
            origin: self.origin,
            mip_level: self.mip_level,
            layout: self.layout,
            bytes,
        }))
    }

    /// Waits for completion on the dedicated GPU submission thread.
    ///
    /// Use [`Self::poll`] from event loops that must remain responsive. This
    /// blocking convenience must not run on UI, audio, playback, render
    /// coordinator, or background job threads.
    pub fn wait(mut self, submissions: &GpuSubmissionQueue<'_>) -> Result<TextureReadbackResult> {
        loop {
            if let Some(result) = self.poll(submissions)? {
                return Ok(result);
            }
            std::thread::park_timeout(WAIT_INTERVAL);
        }
    }
}

impl Drop for SubmittedTextureReadback {
    fn drop(&mut self) {
        if !self.completed {
            if let Some(staging) = &self.staging {
                staging.raw().unmap();
            }
        }
    }
}

impl GpuSubmissionQueue<'_> {
    /// Submits one readback after all earlier queue work and begins async mapping.
    ///
    /// The source texture and managed staging allocation remain retained until
    /// the returned fence retires, even when the submitted handle is dropped.
    pub fn submit_readback(
        &self,
        encoded: EncodedTextureReadback,
    ) -> Result<SubmittedTextureReadback> {
        self.ensure_device_identity(&encoded.device_identity, "submit_texture_readback")?;
        let EncodedTextureReadback {
            device_identity,
            source_id,
            source,
            staging,
            command_buffer,
            boundary,
            origin,
            mip_level,
            layout,
        } = encoded;
        let pending_staging = staging.clone();
        let mut retained = self.resources();
        retained.retain(source);
        retained.retain(staging);
        let fence = self.submit([command_buffer], retained)?;

        let (sender, receiver) = mpsc::channel();
        pending_staging
            .raw()
            .slice(..layout.staging_bytes)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });

        Ok(SubmittedTextureReadback {
            device_identity,
            source_id,
            staging: Some(pending_staging),
            fence,
            receiver,
            boundary,
            origin,
            mip_level,
            layout,
            mapping_ready: false,
            completed: false,
        })
    }
}

fn validate_request(
    resources: &GpuResources<'_>,
    request: &TextureReadbackRequest,
) -> Result<TextureReadbackLayout> {
    resources
        .ensure_owner(request.source.lease(), "encode_texture_readback")
        .map_err(|error| error.with_context(request_context(request, "encode_texture_readback")))?;
    let info = request.source.info();
    if request.extent.width == 0
        || request.extent.height == 0
        || request.extent.depth_or_array_layers == 0
    {
        return Err(invalid_request(
            request,
            "texture readback extent components must all be greater than zero",
        ));
    }
    if info.dimension() != wgpu::TextureDimension::D2 {
        return Err(unsupported_request(
            request,
            "texture readback supports two-dimensional color textures only",
        ));
    }
    if info.sample_count() != 1 {
        return Err(invalid_request(
            request,
            "texture readback source must be single-sampled",
        ));
    }
    if !info.usage().contains(wgpu::TextureUsages::COPY_SRC) {
        return Err(invalid_request(
            request,
            "texture readback source must permit COPY_SRC use",
        ));
    }
    let format = info.format();
    if format.is_depth_stencil_format()
        || format.block_dimensions() != (1, 1)
        || format
            .block_copy_size(Some(wgpu::TextureAspect::All))
            .is_none()
    {
        return Err(unsupported_request(
            request,
            "texture readback supports uncompressed color formats only",
        ));
    }
    if request.mip_level >= info.mip_level_count() {
        return Err(invalid_request(
            request,
            "texture readback mip level is outside the source texture",
        ));
    }
    let mip_extent = info
        .size()
        .mip_level_size(request.mip_level, info.dimension());
    validate_axis(
        request,
        "x",
        request.origin.x,
        request.extent.width,
        mip_extent.width,
    )?;
    validate_axis(
        request,
        "y",
        request.origin.y,
        request.extent.height,
        mip_extent.height,
    )?;
    validate_axis(
        request,
        "z",
        request.origin.z,
        request.extent.depth_or_array_layers,
        mip_extent.depth_or_array_layers,
    )?;

    let bytes_per_texel = u64::from(
        format
            .block_copy_size(Some(wgpu::TextureAspect::All))
            .expect("validated color format has a copy size"),
    );
    let tight_bytes_per_row = u64::from(request.extent.width)
        .checked_mul(bytes_per_texel)
        .ok_or_else(|| exhausted_request(request, "texture readback row size is exhausted"))?;
    let alignment = u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let padded_bytes_per_row = tight_bytes_per_row
        .checked_add(alignment - 1)
        .map(|value| value / alignment * alignment)
        .ok_or_else(|| exhausted_request(request, "texture readback row alignment is exhausted"))?;
    let tight_bytes_per_image = tight_bytes_per_row
        .checked_mul(u64::from(request.extent.height))
        .ok_or_else(|| exhausted_request(request, "texture readback layer size is exhausted"))?;
    let staging_bytes = padded_bytes_per_row
        .checked_mul(u64::from(request.extent.height))
        .and_then(|bytes| bytes.checked_mul(u64::from(request.extent.depth_or_array_layers)))
        .ok_or_else(|| exhausted_request(request, "texture readback staging size is exhausted"))?;
    if staging_bytes > resources.enabled_limits().max_buffer_size {
        return Err(Error::new(
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            "texture readback exceeds the active device buffer limit",
        )
        .with_context(
            request_context(request, "encode_texture_readback")
                .with_field("staging_bytes", staging_bytes.to_string())
                .with_field(
                    "max_buffer_size",
                    resources.enabled_limits().max_buffer_size.to_string(),
                ),
        ));
    }

    Ok(TextureReadbackLayout {
        format,
        extent: request.extent,
        tight_bytes_per_row: u32::try_from(tight_bytes_per_row)
            .map_err(|_| exhausted_request(request, "texture readback row size is exhausted"))?,
        padded_bytes_per_row: u32::try_from(padded_bytes_per_row).map_err(|_| {
            exhausted_request(request, "texture readback aligned row size is exhausted")
        })?,
        rows_per_image: request.extent.height,
        tight_bytes_per_image,
        staging_bytes,
    })
}

fn validate_axis(
    request: &TextureReadbackRequest,
    axis: &'static str,
    origin: u32,
    extent: u32,
    limit: u32,
) -> Result<()> {
    let end = origin.checked_add(extent).ok_or_else(|| {
        invalid_request(request, format!("texture readback {axis} range overflowed"))
    })?;
    if end > limit {
        return Err(invalid_request(
            request,
            format!("texture readback {axis} range is outside the source mip"),
        ));
    }
    Ok(())
}

fn copy_tight_bytes(staging: &GpuBuffer, layout: TextureReadbackLayout) -> Result<Vec<u8>> {
    let output_bytes = layout
        .tight_bytes_per_image
        .checked_mul(u64::from(layout.extent.depth_or_array_layers))
        .ok_or_else(|| {
            Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "completed texture readback output size is exhausted",
            )
            .with_context(ErrorContext::new(COMPONENT, "copy_texture_readback_bytes"))
        })?;
    let capacity = usize::try_from(output_bytes).map_err(|_| {
        Error::new(
            ErrorCategory::ResourceExhausted,
            Recoverability::Terminal,
            "completed texture readback does not fit host address space",
        )
        .with_context(ErrorContext::new(COMPONENT, "copy_texture_readback_bytes"))
    })?;
    let mapped = staging
        .raw()
        .slice(..layout.staging_bytes)
        .get_mapped_range();
    let mut bytes = Vec::with_capacity(capacity);
    for layer in 0..layout.extent.depth_or_array_layers {
        for row in 0..layout.rows_per_image {
            let row_index = u64::from(layer)
                .checked_mul(u64::from(layout.rows_per_image))
                .and_then(|index| index.checked_add(u64::from(row)))
                .expect("validated row index remains in range");
            let start = row_index
                .checked_mul(u64::from(layout.padded_bytes_per_row))
                .expect("validated staging row remains in range");
            let end = start
                .checked_add(u64::from(layout.tight_bytes_per_row))
                .expect("validated tight row remains in range");
            bytes.extend_from_slice(
                mapped
                    .get(
                        usize::try_from(start).expect("mapped row start fits host address space")
                            ..usize::try_from(end).expect("mapped row end fits host address space"),
                    )
                    .expect("validated mapped row remains inside staging buffer"),
            );
        }
    }
    drop(mapped);
    Ok(bytes)
}

fn request_context(request: &TextureReadbackRequest, operation: &'static str) -> ErrorContext {
    submitted_context(request.source.id(), request.boundary, operation)
        .with_field("format", format!("{:?}", request.source.info().format()))
        .with_field("mip_level", request.mip_level.to_string())
        .with_field(
            "origin",
            format!(
                "{},{},{}",
                request.origin.x, request.origin.y, request.origin.z
            ),
        )
        .with_field(
            "extent",
            format!(
                "{}x{}x{}",
                request.extent.width, request.extent.height, request.extent.depth_or_array_layers
            ),
        )
}

fn submitted_context(
    source_id: GpuResourceId,
    boundary: ReadbackBoundary,
    operation: &'static str,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
        .with_field("source_id", source_id.to_string())
        .with_field("boundary", boundary.code())
}

fn invalid_request(request: &TextureReadbackRequest, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(request_context(request, "encode_texture_readback"))
}

fn unsupported_request(request: &TextureReadbackRequest, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(request_context(request, "encode_texture_readback"))
}

fn exhausted_request(request: &TextureReadbackRequest, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(request_context(request, "encode_texture_readback"))
}

fn invalid_state(
    operation: &'static str,
    message: impl Into<String>,
    source_id: GpuResourceId,
    boundary: ReadbackBoundary,
) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(submitted_context(source_id, boundary, operation))
}
