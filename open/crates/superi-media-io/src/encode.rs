//! Codec-neutral frame-to-packet encoder contracts.

use superi_core::error::Result;
use superi_core::time::Timebase;

use crate::audio_io::{AudioBlock, AudioFormat};
use crate::decode::{VideoFormat, VideoFrame};
use crate::demux::{CodecId, Packet, StreamId};
use crate::operation::OperationContext;

/// Exact uncompressed media accepted by an encoder.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EncoderMediaFormat {
    /// Decoded video representation.
    Video(VideoFormat),
    /// Decoded audio representation.
    Audio(AudioFormat),
}

/// Immutable configuration for one encoded output stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncoderConfig {
    stream_id: StreamId,
    codec: CodecId,
    timebase: Timebase,
    media_format: EncoderMediaFormat,
}

impl EncoderConfig {
    /// Creates a video encoder configuration.
    #[must_use]
    pub fn video(
        stream_id: StreamId,
        codec: CodecId,
        timebase: Timebase,
        format: VideoFormat,
    ) -> Self {
        Self {
            stream_id,
            codec,
            timebase,
            media_format: EncoderMediaFormat::Video(format),
        }
    }

    /// Creates an audio encoder configuration.
    #[must_use]
    pub fn audio(stream_id: StreamId, codec: CodecId, format: AudioFormat) -> Self {
        let timebase =
            Timebase::integer(format.sample_rate()).expect("validated audio format sample rate");
        Self {
            stream_id,
            codec,
            timebase,
            media_format: EncoderMediaFormat::Audio(format),
        }
    }

    /// Returns the output stream identifier.
    #[must_use]
    pub const fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// Returns the selected codec identifier.
    #[must_use]
    pub const fn codec(&self) -> &CodecId {
        &self.codec
    }

    /// Returns the exact packet timestamp timebase.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns the accepted uncompressed representation.
    #[must_use]
    pub const fn media_format(&self) -> &EncoderMediaFormat {
        &self.media_format
    }
}

/// One encoder input item.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum EncodeInput {
    /// A decoded video frame.
    Video(VideoFrame),
    /// A decoded audio block.
    Audio(AudioBlock),
}

/// One nonblocking encoder receive result.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EncodeOutput {
    /// One compressed packet is ready.
    Packet(Packet),
    /// The encoder needs another frame or audio block.
    NeedInput,
    /// All delayed packets have been returned after flush.
    EndOfStream,
}

/// Codec-neutral frame-to-packet encoder lifecycle.
pub trait Encoder: Send {
    /// Returns immutable encoder configuration.
    fn config(&self) -> &EncoderConfig;

    /// Supplies one decoded frame or audio block in presentation order.
    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()>;

    /// Receives one compressed packet or an explicit lifecycle state.
    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput>;

    /// Signals end of input while retaining delayed packets for draining.
    fn flush(&mut self, operation: &OperationContext) -> Result<()>;

    /// Discards buffered state so this encoder can start a new stream lifetime.
    fn reset(&mut self, operation: &OperationContext) -> Result<()>;
}
