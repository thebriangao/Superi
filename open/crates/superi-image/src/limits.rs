//! Finite resource policy and fallible allocation helpers for image work.
//!
//! A limit is checked before a source-sized allocation or loop begins. The
//! policy complements decoder-specific limits, which may be best effort, and
//! keeps constrained callers able to select a smaller deterministic ceiling.

use std::mem::size_of;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const COMPONENT: &str = "superi-image.limits";
const DEFAULT_MAX_WIDTH: u32 = 65_536;
const DEFAULT_MAX_HEIGHT: u32 = 65_536;
const DEFAULT_MAX_MEMORY_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_MAX_CHANNELS: usize = 1_024;
const DEFAULT_MAX_LAYERS: usize = 64;
const DEFAULT_MAX_METADATA_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_MAX_TILES: usize = 1_048_576;

/// Finite limits applied to one image-producing or image-decoding operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ImageLimits {
    max_width: u32,
    max_height: u32,
    max_memory_bytes: u64,
    max_channels: usize,
    max_layers: usize,
    max_metadata_bytes: u64,
    max_tiles: usize,
}

impl ImageLimits {
    /// Creates a finite policy with default structural ceilings.
    pub fn new(max_width: u32, max_height: u32, max_memory_bytes: u64) -> Result<Self> {
        if max_width == 0 || max_height == 0 || max_memory_bytes == 0 {
            return Err(invalid(
                "create_image_limits",
                "image dimensions and memory limit must be greater than zero",
            ));
        }
        Ok(Self {
            max_width,
            max_height,
            max_memory_bytes,
            ..Self::default()
        })
    }

    /// Replaces the maximum logical channel count.
    pub fn with_max_channels(mut self, max_channels: usize) -> Result<Self> {
        require_nonzero(max_channels, "set_max_image_channels")?;
        self.max_channels = max_channels;
        Ok(self)
    }

    /// Replaces the maximum still-image layer count.
    pub fn with_max_layers(mut self, max_layers: usize) -> Result<Self> {
        require_nonzero(max_layers, "set_max_image_layers")?;
        self.max_layers = max_layers;
        Ok(self)
    }

    /// Replaces the maximum metadata bytes parsed or retained by one operation.
    pub fn with_max_metadata_bytes(mut self, max_metadata_bytes: u64) -> Result<Self> {
        require_nonzero(max_metadata_bytes, "set_max_image_metadata_bytes")?;
        self.max_metadata_bytes = max_metadata_bytes;
        Ok(self)
    }

    /// Replaces the maximum independently described tile count.
    pub fn with_max_tiles(mut self, max_tiles: usize) -> Result<Self> {
        require_nonzero(max_tiles, "set_max_image_tiles")?;
        self.max_tiles = max_tiles;
        Ok(self)
    }

    /// Returns the largest accepted image width.
    #[must_use]
    pub const fn max_width(self) -> u32 {
        self.max_width
    }

    /// Returns the largest accepted image height.
    #[must_use]
    pub const fn max_height(self) -> u32 {
        self.max_height
    }

    /// Returns the maximum result or retained image bytes for one operation.
    #[must_use]
    pub const fn max_memory_bytes(self) -> u64 {
        self.max_memory_bytes
    }

    /// Returns the largest accepted logical channel count.
    #[must_use]
    pub const fn max_channels(self) -> usize {
        self.max_channels
    }

    /// Returns the largest accepted still-image layer count.
    #[must_use]
    pub const fn max_layers(self) -> usize {
        self.max_layers
    }

    /// Returns the maximum accepted metadata bytes.
    #[must_use]
    pub const fn max_metadata_bytes(self) -> u64 {
        self.max_metadata_bytes
    }

    /// Returns the largest accepted tile count.
    #[must_use]
    pub const fn max_tiles(self) -> usize {
        self.max_tiles
    }

    pub(crate) fn check_dimensions(
        self,
        width: u32,
        height: u32,
        operation: &'static str,
    ) -> Result<()> {
        if width > self.max_width || height > self.max_height {
            return Err(
                exhausted(operation, "image dimensions exceed the configured limits").with_context(
                    ErrorContext::new(COMPONENT, "image_dimensions")
                        .with_field("width", width.to_string())
                        .with_field("height", height.to_string())
                        .with_field("max_width", self.max_width.to_string())
                        .with_field("max_height", self.max_height.to_string()),
                ),
            );
        }
        Ok(())
    }

    pub(crate) fn check_channels(self, count: usize, operation: &'static str) -> Result<()> {
        check_count(count, self.max_channels, "channels", operation)
    }

    pub(crate) fn check_layers(self, count: usize, operation: &'static str) -> Result<()> {
        check_count(count, self.max_layers, "layers", operation)
    }

    pub(crate) fn check_tiles(self, count: usize, operation: &'static str) -> Result<()> {
        check_count(count, self.max_tiles, "tiles", operation)
    }

    pub(crate) fn check_metadata_bytes(self, bytes: u64, operation: &'static str) -> Result<()> {
        check_bytes(bytes, self.max_metadata_bytes, "metadata_bytes", operation)
    }

    pub(crate) fn check_allocation<T>(
        self,
        elements: usize,
        operation: &'static str,
    ) -> Result<u64> {
        let bytes = allocation_bytes::<T>(elements, operation)?;
        check_bytes(bytes, self.max_memory_bytes, "allocation_bytes", operation)?;
        Ok(bytes)
    }

    pub(crate) fn try_reserve_exact<T>(
        self,
        values: &mut Vec<T>,
        additional: usize,
        operation: &'static str,
    ) -> Result<()> {
        let total = values.len().checked_add(additional).ok_or_else(|| {
            exhausted(
                operation,
                "image allocation element count overflows the host address space",
            )
        })?;
        self.check_allocation::<T>(total, operation)?;
        values.try_reserve_exact(additional).map_err(|_| {
            exhausted(operation, "image allocation could not be reserved").with_context(
                ErrorContext::new(COMPONENT, "allocation_reservation")
                    .with_field("elements", additional.to_string())
                    .with_field("element_bytes", size_of::<T>().to_string()),
            )
        })
    }

    pub(crate) fn try_reserve_string(
        self,
        value: &mut String,
        additional: usize,
        operation: &'static str,
    ) -> Result<()> {
        let total = value.len().checked_add(additional).ok_or_else(|| {
            exhausted(
                operation,
                "image string size overflows the host address space",
            )
        })?;
        self.check_allocation::<u8>(total, operation)?;
        value.try_reserve_exact(additional).map_err(|_| {
            exhausted(operation, "image string allocation could not be reserved").with_context(
                ErrorContext::new(COMPONENT, "string_allocation_reservation")
                    .with_field("bytes", additional.to_string()),
            )
        })
    }
}

impl Default for ImageLimits {
    fn default() -> Self {
        Self {
            max_width: DEFAULT_MAX_WIDTH,
            max_height: DEFAULT_MAX_HEIGHT,
            max_memory_bytes: DEFAULT_MAX_MEMORY_BYTES,
            max_channels: DEFAULT_MAX_CHANNELS,
            max_layers: DEFAULT_MAX_LAYERS,
            max_metadata_bytes: DEFAULT_MAX_METADATA_BYTES,
            max_tiles: DEFAULT_MAX_TILES,
        }
    }
}

pub(crate) fn try_clone_slice<T: Clone>(
    values: &[T],
    limits: ImageLimits,
    operation: &'static str,
) -> Result<Vec<T>> {
    let mut output = Vec::new();
    limits.try_reserve_exact(&mut output, values.len(), operation)?;
    output.extend_from_slice(values);
    Ok(output)
}

pub(crate) fn try_zeroed_bytes(
    len: usize,
    limits: ImageLimits,
    operation: &'static str,
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    limits.try_reserve_exact(&mut output, len, operation)?;
    output.resize(len, 0);
    Ok(output)
}

pub(crate) fn contextualize_limit_error(
    error: Error,
    component: &'static str,
    operation: &'static str,
) -> Error {
    let category = error.category();
    let recoverability = error.recoverability();
    let message = error.message().to_owned();
    Error::with_source(category, recoverability, message, error)
        .with_context(ErrorContext::new(component, operation))
}

fn allocation_bytes<T>(elements: usize, operation: &'static str) -> Result<u64> {
    let bytes = elements.checked_mul(size_of::<T>()).ok_or_else(|| {
        exhausted(
            operation,
            "image allocation size overflows the host address space",
        )
    })?;
    u64::try_from(bytes).map_err(|_| {
        exhausted(
            operation,
            "image allocation size exceeds the supported range",
        )
    })
}

fn check_count(
    count: usize,
    maximum: usize,
    field: &'static str,
    operation: &'static str,
) -> Result<()> {
    if count > maximum {
        return Err(
            exhausted(operation, "image structure exceeds the configured limit").with_context(
                ErrorContext::new(COMPONENT, "structural_limit")
                    .with_field("field", field)
                    .with_field("actual", count.to_string())
                    .with_field("maximum", maximum.to_string()),
            ),
        );
    }
    Ok(())
}

fn check_bytes(
    bytes: u64,
    maximum: u64,
    field: &'static str,
    operation: &'static str,
) -> Result<()> {
    if bytes > maximum {
        return Err(
            exhausted(operation, "image memory exceeds the configured limit").with_context(
                ErrorContext::new(COMPONENT, "memory_limit")
                    .with_field("field", field)
                    .with_field("actual", bytes.to_string())
                    .with_field("maximum", maximum.to_string()),
            ),
        );
    }
    Ok(())
}

fn require_nonzero<T>(value: T, operation: &'static str) -> Result<()>
where
    T: Default + PartialEq,
{
    if value == T::default() {
        Err(invalid(operation, "image limit must be greater than zero"))
    } else {
        Ok(())
    }
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
