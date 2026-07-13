//! Immutable CPU image storage with explicit channel and row layout.
//!
//! Storage is deliberately separate from channel names, alpha interpretation,
//! color metadata, and file-format rules. A channel slice identifies bytes by
//! logical channel order while [`ImageStorage`] validates the complete address
//! calculation before any consumer can observe the image.

use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;

const COMPONENT: &str = "superi-image.model";

/// A power-of-two byte alignment for logical row starts within a plane.
///
/// This value describes offsets relative to the start of [`StoragePlane::bytes`].
/// It does not claim that the untyped `Arc<[u8]>` allocation itself can be
/// dereferenced as a natively aligned typed pointer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ByteAlignment(usize);

impl ByteAlignment {
    /// One-byte row alignment.
    pub const ONE: Self = Self(1);

    /// Creates a nonzero power-of-two byte alignment.
    pub fn new(value: usize) -> Result<Self> {
        if value == 0 || !value.is_power_of_two() {
            return Err(invalid(
                "create_byte_alignment",
                "byte alignment must be a nonzero power of two",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "alignment_value")
                    .with_field("value", value.to_string()),
            ));
        }
        Ok(Self(value))
    }

    /// Returns the alignment in bytes.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// How logical image channels are distributed across byte planes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ChannelStorageLayout {
    /// All channels occupy one shared pixel record in one plane.
    Interleaved,
    /// Each logical channel occupies its own plane.
    Planar,
}

impl ChannelStorageLayout {
    const fn code(self) -> &'static str {
        match self {
            Self::Interleaved => "interleaved",
            Self::Planar => "planar",
        }
    }
}

/// One immutable byte plane and its complete row layout.
///
/// `origin` is the first logical row start. The allocation must contain every
/// full stored row, including padding at the end of the final row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoragePlane {
    bytes: Arc<[u8]>,
    origin: usize,
    row_stride: usize,
    row_alignment: ByteAlignment,
}

impl StoragePlane {
    /// Creates a plane with explicit origin, row stride, and row alignment.
    pub fn new(
        bytes: Arc<[u8]>,
        origin: usize,
        row_stride: usize,
        row_alignment: ByteAlignment,
    ) -> Result<Self> {
        if bytes.is_empty() {
            return Err(invalid(
                "create_storage_plane",
                "storage plane bytes must not be empty",
            ));
        }
        if origin >= bytes.len() {
            return Err(invalid(
                "create_storage_plane",
                "storage plane origin must lie inside its byte allocation",
            )
            .with_context(plane_context(
                "plane_origin",
                origin,
                row_stride,
                bytes.len(),
            )));
        }
        if row_stride == 0 {
            return Err(invalid(
                "create_storage_plane",
                "storage plane row stride must be greater than zero",
            ));
        }
        if origin % row_alignment.get() != 0 {
            return Err(invalid(
                "create_storage_plane",
                "storage plane origin does not satisfy its row alignment",
            )
            .with_context(
                plane_context("plane_origin", origin, row_stride, bytes.len())
                    .with_field("row_alignment", row_alignment.get().to_string()),
            ));
        }
        if row_stride % row_alignment.get() != 0 {
            return Err(invalid(
                "create_storage_plane",
                "storage plane row stride does not satisfy its row alignment",
            )
            .with_context(
                plane_context("plane_stride", origin, row_stride, bytes.len())
                    .with_field("row_alignment", row_alignment.get().to_string()),
            ));
        }
        Ok(Self {
            bytes,
            origin,
            row_stride,
            row_alignment,
        })
    }

    /// Returns the shared immutable byte allocation.
    #[must_use]
    pub const fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }

    /// Returns the byte offset of the first logical row.
    #[must_use]
    pub const fn origin(&self) -> usize {
        self.origin
    }

    /// Returns the byte distance between adjacent logical rows.
    #[must_use]
    pub const fn row_stride(&self) -> usize {
        self.row_stride
    }

    /// Returns the alignment guaranteed for every logical row offset.
    #[must_use]
    pub const fn row_alignment(&self) -> ByteAlignment {
        self.row_alignment
    }
}

/// The byte location and stored width of one logical channel sample.
///
/// A sample at local pixel `(x, y)` begins at:
///
/// `plane.origin + y * plane.row_stride + x * pixel_stride + byte_offset`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChannelSlice {
    plane_index: usize,
    byte_offset: usize,
    sample_bytes: usize,
    pixel_stride: usize,
}

impl ChannelSlice {
    /// Creates a channel slice with an explicit plane and pixel record layout.
    pub fn new(
        plane_index: usize,
        byte_offset: usize,
        sample_bytes: usize,
        pixel_stride: usize,
    ) -> Result<Self> {
        if sample_bytes == 0 {
            return Err(invalid(
                "create_channel_slice",
                "channel sample size must be greater than zero",
            ));
        }
        if pixel_stride < sample_bytes {
            return Err(invalid(
                "create_channel_slice",
                "channel pixel stride must cover one complete sample",
            )
            .with_context(channel_context(
                "channel_stride",
                plane_index,
                byte_offset,
                sample_bytes,
                pixel_stride,
            )));
        }
        Ok(Self {
            plane_index,
            byte_offset,
            sample_bytes,
            pixel_stride,
        })
    }

    /// Returns the source plane index.
    #[must_use]
    pub const fn plane_index(self) -> usize {
        self.plane_index
    }

    /// Returns the channel offset within the first pixel record.
    #[must_use]
    pub const fn byte_offset(self) -> usize {
        self.byte_offset
    }

    /// Returns the exact stored bytes occupied by one sample.
    #[must_use]
    pub const fn sample_bytes(self) -> usize {
        self.sample_bytes
    }

    /// Returns the byte distance between adjacent samples of this channel.
    #[must_use]
    pub const fn pixel_stride(self) -> usize {
        self.pixel_stride
    }

    fn checked_sample_end(self, operation: &'static str) -> Result<usize> {
        self.byte_offset
            .checked_add(self.sample_bytes)
            .ok_or_else(|| {
                exhausted(
                    operation,
                    "channel sample byte range exceeds the supported address space",
                )
                .with_context(channel_context(
                    "sample_range",
                    self.plane_index,
                    self.byte_offset,
                    self.sample_bytes,
                    self.pixel_stride,
                ))
            })
    }
}

/// Validated immutable CPU storage for an image's ordered logical channels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageStorage {
    bounds: PixelBounds,
    layout: ChannelStorageLayout,
    planes: Vec<StoragePlane>,
    channels: Vec<ChannelSlice>,
}

impl ImageStorage {
    /// Creates storage and validates every plane, channel, stride, and byte span.
    pub fn new(
        bounds: PixelBounds,
        layout: ChannelStorageLayout,
        planes: Vec<StoragePlane>,
        channels: Vec<ChannelSlice>,
    ) -> Result<Self> {
        if bounds.is_empty() {
            return Err(invalid(
                "create_image_storage",
                "image storage bounds must not be empty",
            )
            .with_context(bounds_context(bounds, layout)));
        }
        if planes.is_empty() || channels.is_empty() {
            return Err(invalid(
                "create_image_storage",
                "image storage requires at least one plane and one channel",
            )
            .with_context(bounds_context(bounds, layout)));
        }

        match layout {
            ChannelStorageLayout::Interleaved => {
                validate_interleaved(&planes, &channels, bounds)?;
            }
            ChannelStorageLayout::Planar => validate_planar(&planes, &channels, bounds)?,
        }
        validate_plane_spans(&planes, &channels, bounds, layout)?;

        Ok(Self {
            bounds,
            layout,
            planes,
            channels,
        })
    }

    /// Returns the signed half-open pixel data window.
    #[must_use]
    pub const fn bounds(&self) -> PixelBounds {
        self.bounds
    }

    /// Returns how channels are distributed across planes.
    #[must_use]
    pub const fn layout(&self) -> ChannelStorageLayout {
        self.layout
    }

    /// Returns the immutable byte planes in canonical plane order.
    #[must_use]
    pub fn planes(&self) -> &[StoragePlane] {
        &self.planes
    }

    /// Returns channel slices in logical channel order.
    #[must_use]
    pub fn channels(&self) -> &[ChannelSlice] {
        &self.channels
    }

    /// Returns the number of byte planes.
    #[must_use]
    pub fn plane_count(&self) -> usize {
        self.planes.len()
    }

    /// Returns the number of ordered logical channels.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Returns the exact stored bytes for one channel sample at `(x, y)`.
    ///
    /// `None` means the channel index or pixel coordinate is outside this
    /// storage. Sample interpretation remains the responsibility of the image
    /// data model layered above this byte substrate.
    #[must_use]
    pub fn sample_bytes(&self, channel_index: usize, x: i32, y: i32) -> Option<&[u8]> {
        if !self.bounds.contains(x, y) {
            return None;
        }
        let channel = *self.channels.get(channel_index)?;
        let plane = self.planes.get(channel.plane_index)?;
        let local_x = usize::try_from(i64::from(x) - i64::from(self.bounds.min_x())).ok()?;
        let local_y = usize::try_from(i64::from(y) - i64::from(self.bounds.min_y())).ok()?;
        let offset = plane
            .origin
            .checked_add(local_y.checked_mul(plane.row_stride)?)?
            .checked_add(local_x.checked_mul(channel.pixel_stride)?)?
            .checked_add(channel.byte_offset)?;
        let end = offset.checked_add(channel.sample_bytes)?;
        plane.bytes.get(offset..end)
    }

    /// Returns one complete stored row, including its padding bytes.
    #[must_use]
    pub fn plane_row(&self, plane_index: usize, y: i32) -> Option<&[u8]> {
        if y < self.bounds.min_y() || y >= self.bounds.max_y() {
            return None;
        }
        let plane = self.planes.get(plane_index)?;
        let local_y = usize::try_from(i64::from(y) - i64::from(self.bounds.min_y())).ok()?;
        let start = plane
            .origin
            .checked_add(local_y.checked_mul(plane.row_stride)?)?;
        let end = start.checked_add(plane.row_stride)?;
        plane.bytes.get(start..end)
    }
}

fn validate_interleaved(
    planes: &[StoragePlane],
    channels: &[ChannelSlice],
    bounds: PixelBounds,
) -> Result<()> {
    if planes.len() != 1 {
        return Err(invalid(
            "create_image_storage",
            "interleaved image storage requires exactly one plane",
        )
        .with_context(bounds_context(bounds, ChannelStorageLayout::Interleaved)));
    }
    let pixel_stride = channels[0].pixel_stride;
    for (index, channel) in channels.iter().copied().enumerate() {
        if channel.plane_index != 0 || channel.pixel_stride != pixel_stride {
            return Err(invalid(
                "create_image_storage",
                "interleaved channels must share one plane and pixel stride",
            )
            .with_context(indexed_channel_context(index, channel)));
        }
        let end = channel.checked_sample_end("create_image_storage")?;
        if end > pixel_stride {
            return Err(invalid(
                "create_image_storage",
                "interleaved channel sample exceeds its pixel record",
            )
            .with_context(indexed_channel_context(index, channel)));
        }
    }
    for left in 0..channels.len() {
        let left_end = channels[left].checked_sample_end("create_image_storage")?;
        for right in left + 1..channels.len() {
            let right_end = channels[right].checked_sample_end("create_image_storage")?;
            if channels[left].byte_offset < right_end && channels[right].byte_offset < left_end {
                return Err(invalid(
                    "create_image_storage",
                    "interleaved channel byte ranges must not overlap",
                )
                .with_context(
                    indexed_channel_context(left, channels[left])
                        .with_field("overlaps_channel", right.to_string()),
                ));
            }
        }
    }
    Ok(())
}

fn validate_planar(
    planes: &[StoragePlane],
    channels: &[ChannelSlice],
    bounds: PixelBounds,
) -> Result<()> {
    if planes.len() != channels.len() {
        return Err(invalid(
            "create_image_storage",
            "planar image storage requires exactly one plane per channel",
        )
        .with_context(bounds_context(bounds, ChannelStorageLayout::Planar)));
    }
    for (index, channel) in channels.iter().copied().enumerate() {
        if channel.plane_index != index || channel.byte_offset != 0 {
            return Err(invalid(
                "create_image_storage",
                "planar channels must map in order to separate plane origins",
            )
            .with_context(indexed_channel_context(index, channel)));
        }
    }
    Ok(())
}

fn validate_plane_spans(
    planes: &[StoragePlane],
    channels: &[ChannelSlice],
    bounds: PixelBounds,
    layout: ChannelStorageLayout,
) -> Result<()> {
    let width = usize::try_from(bounds.width()).map_err(|_| {
        exhausted(
            "create_image_storage",
            "image width cannot be represented on this platform",
        )
        .with_context(bounds_context(bounds, layout))
    })?;
    let height = usize::try_from(bounds.height()).map_err(|_| {
        exhausted(
            "create_image_storage",
            "image height cannot be represented on this platform",
        )
        .with_context(bounds_context(bounds, layout))
    })?;

    for (index, channel) in channels.iter().copied().enumerate() {
        let plane = planes.get(channel.plane_index).ok_or_else(|| {
            invalid(
                "create_image_storage",
                "channel references a storage plane that does not exist",
            )
            .with_context(indexed_channel_context(index, channel))
        })?;
        let last_pixel = width
            .checked_sub(1)
            .and_then(|value| value.checked_mul(channel.pixel_stride))
            .ok_or_else(|| layout_overflow(bounds, layout, index, channel))?;
        let row_end = last_pixel
            .checked_add(channel.checked_sample_end("create_image_storage")?)
            .ok_or_else(|| layout_overflow(bounds, layout, index, channel))?;
        if row_end > plane.row_stride {
            return Err(invalid(
                "create_image_storage",
                "channel row span exceeds the storage plane row stride",
            )
            .with_context(
                indexed_channel_context(index, channel)
                    .with_field("row_span", row_end.to_string())
                    .with_field("row_stride", plane.row_stride.to_string()),
            ));
        }
    }

    for (index, plane) in planes.iter().enumerate() {
        let stored_rows = height
            .checked_mul(plane.row_stride)
            .ok_or_else(|| plane_overflow(bounds, layout, index, plane))?;
        let required = plane
            .origin
            .checked_add(stored_rows)
            .ok_or_else(|| plane_overflow(bounds, layout, index, plane))?;
        if required > plane.bytes.len() {
            return Err(invalid(
                "create_image_storage",
                "storage plane allocation does not contain every complete row",
            )
            .with_context(
                plane_context(
                    "plane_span",
                    plane.origin,
                    plane.row_stride,
                    plane.bytes.len(),
                )
                .with_field("plane_index", index.to_string())
                .with_field("required_bytes", required.to_string()),
            ));
        }
    }
    Ok(())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn bounds_context(bounds: PixelBounds, layout: ChannelStorageLayout) -> ErrorContext {
    ErrorContext::new(COMPONENT, "image_layout")
        .with_field("layout", layout.code())
        .with_field("min_x", bounds.min_x().to_string())
        .with_field("min_y", bounds.min_y().to_string())
        .with_field("width", bounds.width().to_string())
        .with_field("height", bounds.height().to_string())
}

fn plane_context(
    operation: &'static str,
    origin: usize,
    row_stride: usize,
    byte_len: usize,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
        .with_field("origin", origin.to_string())
        .with_field("row_stride", row_stride.to_string())
        .with_field("byte_len", byte_len.to_string())
}

fn channel_context(
    operation: &'static str,
    plane_index: usize,
    byte_offset: usize,
    sample_bytes: usize,
    pixel_stride: usize,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
        .with_field("plane_index", plane_index.to_string())
        .with_field("byte_offset", byte_offset.to_string())
        .with_field("sample_bytes", sample_bytes.to_string())
        .with_field("pixel_stride", pixel_stride.to_string())
}

fn indexed_channel_context(index: usize, channel: ChannelSlice) -> ErrorContext {
    channel_context(
        "channel_layout",
        channel.plane_index,
        channel.byte_offset,
        channel.sample_bytes,
        channel.pixel_stride,
    )
    .with_field("channel_index", index.to_string())
}

fn layout_overflow(
    bounds: PixelBounds,
    layout: ChannelStorageLayout,
    index: usize,
    channel: ChannelSlice,
) -> Error {
    exhausted(
        "create_image_storage",
        "channel layout exceeds the supported address space",
    )
    .with_context(bounds_context(bounds, layout))
    .with_context(indexed_channel_context(index, channel))
}

fn plane_overflow(
    bounds: PixelBounds,
    layout: ChannelStorageLayout,
    index: usize,
    plane: &StoragePlane,
) -> Error {
    exhausted(
        "create_image_storage",
        "storage plane layout exceeds the supported address space",
    )
    .with_context(bounds_context(bounds, layout))
    .with_context(
        plane_context(
            "plane_layout",
            plane.origin,
            plane.row_stride,
            plane.bytes.len(),
        )
        .with_field("plane_index", index.to_string()),
    )
}
