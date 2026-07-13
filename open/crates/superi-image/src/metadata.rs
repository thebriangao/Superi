//! Deterministic, lossless image metadata storage.
//!
//! This module provides the untyped attribute substrate used by the image data
//! model. Standard orientation, pixel-aspect, timecode, and color attributes
//! remain explicit future contracts rather than being inferred from these
//! values.

use std::collections::BTreeMap;
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

/// An exactly preserved floating-point metadata value.
///
/// The IEEE binary64 payload is stored verbatim so signed zero, infinities, and
/// distinct NaN payloads survive inspection and round trips.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImageMetadataFloat(u64);

impl ImageMetadataFloat {
    /// Stores the exact IEEE bits of `value`.
    #[must_use]
    pub fn new(value: f64) -> Self {
        Self(value.to_bits())
    }

    /// Constructs a value from its exact IEEE binary64 payload.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Returns the represented floating-point value.
    #[must_use]
    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Returns the exact stored IEEE binary64 payload.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

/// One losslessly retained image attribute value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ImageMetadataValue {
    /// UTF-8 text.
    Text(String),
    /// Signed integer data.
    Signed(i64),
    /// Unsigned integer data.
    Unsigned(u64),
    /// An exact IEEE binary64 value.
    Float(ImageMetadataFloat),
    /// Uninterpreted bytes for source attributes not understood by this build.
    Bytes(Arc<[u8]>),
}

impl ImageMetadataValue {
    /// Returns a floating-point value when this attribute stores one.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(value.value()),
            _ => None,
        }
    }
}

/// Deterministically ordered image attributes.
///
/// Keys may retain source-specific spelling and namespaces. Only empty keys and
/// embedded NUL bytes are rejected because they cannot be transported safely
/// through common image format APIs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImageMetadata(BTreeMap<String, ImageMetadataValue>);

impl ImageMetadata {
    /// Creates an empty metadata collection.
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Inserts an attribute and returns the prior value for its key.
    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: ImageMetadataValue,
    ) -> Result<Option<ImageMetadataValue>> {
        let key = key.into();
        if key.is_empty() || key.contains('\0') {
            return Err(invalid_metadata_key(&key));
        }
        Ok(self.0.insert(key, value))
    }

    /// Returns an attribute by exact key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&ImageMetadataValue> {
        self.0.get(key)
    }

    /// Iterates attributes in deterministic key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ImageMetadataValue)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }

    /// Returns the number of retained attributes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when no attributes are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn invalid_metadata_key(key: &str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "image metadata keys must be nonempty and contain no NUL bytes",
    )
    .with_context(
        ErrorContext::new("superi-image.metadata", "insert_metadata")
            .with_field("key_length", key.len().to_string()),
    )
}
