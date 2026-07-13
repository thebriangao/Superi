//! Codec-neutral decoded audio blocks.

use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{ChannelLayout, SampleFormat};
use superi_core::time::{Duration, SampleTime};

use crate::demux::{MediaMetadata, MetadataValue};

/// Exact decoded audio representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioFormat {
    sample_rate: u32,
    sample_format: SampleFormat,
    channel_layout: ChannelLayout,
}

impl AudioFormat {
    /// Creates a decoded audio format.
    pub fn new(
        sample_rate: u32,
        sample_format: SampleFormat,
        channel_layout: ChannelLayout,
    ) -> Result<Self> {
        if sample_rate == 0 {
            return Err(invalid(
                "create_audio_format",
                "audio sample rate must be greater than zero",
            ));
        }
        Ok(Self {
            sample_rate,
            sample_format,
            channel_layout,
        })
    }

    /// Returns samples per second.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the numeric and memory representation.
    #[must_use]
    pub const fn sample_format(&self) -> SampleFormat {
        self.sample_format
    }

    /// Returns channels in routing order.
    #[must_use]
    pub const fn channel_layout(&self) -> &ChannelLayout {
        &self.channel_layout
    }
}

/// One immutable audio memory plane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPlane(Arc<[u8]>);

impl AudioPlane {
    /// Creates an audio plane from immutable bytes.
    #[must_use]
    pub const fn new(bytes: Arc<[u8]>) -> Self {
        Self(bytes)
    }

    /// Returns the stored bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// One decoded audio block with exact sample timing and channel meaning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioBlock {
    format: AudioFormat,
    timestamp: SampleTime,
    frame_count: u64,
    planes: Vec<AudioPlane>,
    metadata: MediaMetadata,
}

impl AudioBlock {
    /// Creates and validates a packed or planar audio block.
    pub fn new(
        format: AudioFormat,
        timestamp: SampleTime,
        frame_count: u64,
        planes: Vec<AudioPlane>,
    ) -> Result<Self> {
        if timestamp.sample_rate() != format.sample_rate {
            return Err(invalid(
                "create_audio_block",
                "audio timestamp and format must use the same sample rate",
            ));
        }
        let channel_count = format.channel_layout.len();
        let bytes_per_sample = usize::from(format.sample_format.bytes_per_sample());
        let frames = usize::try_from(frame_count).map_err(|_| {
            invalid(
                "create_audio_block",
                "audio frame count cannot be represented on this platform",
            )
        })?;
        if format.sample_format.is_planar() {
            if planes.len() != channel_count {
                return Err(invalid(
                    "create_audio_block",
                    "planar audio requires one plane per channel",
                ));
            }
            let expected = frames
                .checked_mul(bytes_per_sample)
                .ok_or_else(|| invalid("create_audio_block", "audio plane byte size overflowed"))?;
            if planes.iter().any(|plane| plane.bytes().len() != expected) {
                return Err(invalid(
                    "create_audio_block",
                    "planar audio plane size does not match the frame count",
                ));
            }
        } else {
            if planes.len() != 1 {
                return Err(invalid(
                    "create_audio_block",
                    "packed audio requires exactly one interleaved plane",
                ));
            }
            let expected = frames
                .checked_mul(channel_count)
                .and_then(|value| value.checked_mul(bytes_per_sample))
                .ok_or_else(|| invalid("create_audio_block", "audio plane byte size overflowed"))?;
            if planes[0].bytes().len() != expected {
                return Err(invalid(
                    "create_audio_block",
                    "packed audio plane size does not match the frame count and channel layout",
                ));
            }
        }
        Duration::from_samples(frame_count, format.sample_rate)?;
        Ok(Self {
            format,
            timestamp,
            frame_count,
            planes,
            metadata: MediaMetadata::new(),
        })
    }

    /// Adds preserved frame metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: MetadataValue) -> Result<Self> {
        self.metadata.insert(key, value)?;
        Ok(self)
    }

    /// Returns the decoded representation.
    #[must_use]
    pub const fn format(&self) -> &AudioFormat {
        &self.format
    }

    /// Returns the exact first-sample position.
    #[must_use]
    pub const fn timestamp(&self) -> SampleTime {
        self.timestamp
    }

    /// Returns the exact sample duration.
    #[must_use]
    pub fn duration(&self) -> Duration {
        Duration::from_samples(self.frame_count, self.format.sample_rate)
            .expect("validated audio block duration")
    }

    /// Returns the number of sample frames per channel.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Returns packed or channel-ordered planar storage.
    #[must_use]
    pub fn planes(&self) -> &[AudioPlane] {
        &self.planes
    }

    /// Returns preserved frame metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-media-io.audio", operation))
}
