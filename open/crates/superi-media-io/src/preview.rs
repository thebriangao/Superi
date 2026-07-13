//! Decoded-audio waveform previews outside playback and rendering.
//!
//! This adapter borrows immutable [`AudioBlock`] values, validates one exact
//! continuous sample clock and format, and summarizes every source frame once.
//! It never resamples, mixes, rewrites, or feeds audio back into playback. The
//! lower `superi-image` preview layer owns rasterization and the typed result.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::SampleFormat;
use superi_image::preview::{
    render_waveform_image, WaveformEnvelope, WaveformImage, WaveformPeak, WaveformRasterStyle,
};

use crate::audio_io::AudioBlock;

const COMPONENT: &str = "superi-media-io.preview";

/// Width and raster style for one decoded-audio waveform preview.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WaveformRequest {
    width: u32,
    style: WaveformRasterStyle,
}

impl WaveformRequest {
    /// Creates a request with a nonzero maximum width.
    pub fn new(width: u32, style: WaveformRasterStyle) -> Result<Self> {
        if width == 0 {
            return Err(invalid(
                "create_waveform_request",
                "waveform width must be greater than zero",
            ));
        }
        Ok(Self { width, style })
    }

    /// Returns the requested maximum width.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the requested channel-band raster style.
    #[must_use]
    pub const fn style(self) -> WaveformRasterStyle {
        self.style
    }
}

/// Generates a channel-separated waveform from one continuous decoded stream.
///
/// Every output column owns an exact half-open source frame range. All blocks
/// must use the same sample format, rate, and ordered channel layout, and each
/// timestamp must equal the preceding block's exclusive end. Float samples
/// outside full scale are clipped for display; nonfinite samples are rejected.
pub fn generate_audio_waveform_image(
    blocks: &[AudioBlock],
    request: WaveformRequest,
) -> Result<WaveformImage> {
    let first = blocks.first().ok_or_else(|| {
        invalid(
            "generate_audio_waveform",
            "waveform generation requires at least one decoded audio block",
        )
    })?;
    let format = first.format();
    let start = first.timestamp();
    let mut expected_sample = start.sample();
    let mut total_frames = 0_u64;
    for (index, block) in blocks.iter().enumerate() {
        if block.format() != format {
            return Err(invalid(
                "generate_audio_waveform",
                "waveform audio blocks must share one exact format and channel layout",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compare_audio_format")
                    .with_field("block_index", index.to_string()),
            ));
        }
        if block.timestamp().sample() != expected_sample {
            return Err(conflict(
                "generate_audio_waveform",
                "waveform audio blocks contain a sample gap or overlap",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compare_audio_continuity")
                    .with_field("block_index", index.to_string())
                    .with_field("expected_sample", expected_sample.to_string())
                    .with_field("actual_sample", block.timestamp().sample().to_string()),
            ));
        }
        let frame_count = i64::try_from(block.frame_count()).map_err(|_| {
            exhausted(
                "generate_audio_waveform",
                "audio block frame count exceeds the signed sample clock",
            )
        })?;
        expected_sample = expected_sample.checked_add(frame_count).ok_or_else(|| {
            exhausted(
                "generate_audio_waveform",
                "audio block range exceeds the signed sample clock",
            )
        })?;
        total_frames = total_frames
            .checked_add(block.frame_count())
            .ok_or_else(|| {
                exhausted(
                    "generate_audio_waveform",
                    "waveform source frame count overflowed",
                )
            })?;
    }
    if total_frames == 0 {
        return Err(invalid(
            "generate_audio_waveform",
            "waveform generation requires at least one decoded audio frame",
        ));
    }

    let width = u64::from(request.width).min(total_frames);
    let width_usize = usize::try_from(width).map_err(|_| {
        exhausted(
            "generate_audio_waveform",
            "waveform width cannot be represented on this platform",
        )
    })?;
    let channel_count = format.channel_layout().len();
    let mut columns = Vec::new();
    columns.try_reserve_exact(width_usize).map_err(|_| {
        exhausted(
            "generate_audio_waveform",
            "waveform envelope allocation exceeds available memory",
        )
    })?;
    let mut block_index = 0_usize;
    let mut block_start = 0_u64;

    for column in 0..width {
        let first_frame = partition_edge(column, total_frames, width)?;
        let end_frame = partition_edge(column + 1, total_frames, width)?;
        let mut minimums = vec![f32::INFINITY; channel_count];
        let mut maximums = vec![f32::NEG_INFINITY; channel_count];

        for frame in first_frame..end_frame {
            while frame
                >= block_start
                    .checked_add(blocks[block_index].frame_count())
                    .ok_or_else(|| {
                        exhausted(
                            "generate_audio_waveform",
                            "waveform block offset overflowed",
                        )
                    })?
            {
                block_start = block_start
                    .checked_add(blocks[block_index].frame_count())
                    .ok_or_else(|| {
                        exhausted(
                            "generate_audio_waveform",
                            "waveform block offset overflowed",
                        )
                    })?;
                block_index = block_index.checked_add(1).ok_or_else(|| {
                    exhausted("generate_audio_waveform", "waveform block index overflowed")
                })?;
            }
            let local_frame = frame.checked_sub(block_start).ok_or_else(|| {
                internal(
                    "generate_audio_waveform",
                    "waveform frame cursor moved before its block",
                )
            })?;
            for channel in 0..channel_count {
                let value = normalized_sample(&blocks[block_index], local_frame, channel)?;
                minimums[channel] = minimums[channel].min(value);
                maximums[channel] = maximums[channel].max(value);
            }
        }

        let mut peaks = Vec::new();
        peaks.try_reserve_exact(channel_count).map_err(|_| {
            exhausted(
                "generate_audio_waveform",
                "waveform peak allocation exceeds available memory",
            )
        })?;
        for channel in 0..channel_count {
            peaks.push(
                WaveformPeak::new(minimums[channel], maximums[channel])
                    .map_err(|error| with_context(error, "generate_audio_waveform"))?,
            );
        }
        columns.push(peaks);
    }

    let envelope = WaveformEnvelope::new(
        start,
        total_frames,
        format.channel_layout().clone(),
        columns,
    )
    .map_err(|error| with_context(error, "generate_audio_waveform"))?;
    render_waveform_image(envelope, request.style)
        .map_err(|error| with_context(error, "generate_audio_waveform"))
}

fn partition_edge(column: u64, frames: u64, width: u64) -> Result<u64> {
    let numerator = u128::from(column)
        .checked_mul(u128::from(frames))
        .ok_or_else(|| {
            exhausted(
                "partition_waveform_frames",
                "waveform column range overflowed",
            )
        })?;
    u64::try_from(numerator / u128::from(width)).map_err(|_| {
        exhausted(
            "partition_waveform_frames",
            "waveform column range cannot be represented",
        )
    })
}

fn normalized_sample(block: &AudioBlock, frame: u64, channel: usize) -> Result<f32> {
    let format = block.format().sample_format();
    let bytes_per_sample = usize::from(format.bytes_per_sample());
    let frame = usize::try_from(frame).map_err(|_| {
        exhausted(
            "read_waveform_sample",
            "audio frame index cannot be represented on this platform",
        )
    })?;
    let (plane, sample_index) = if format.is_planar() {
        (channel, frame)
    } else {
        (
            0,
            frame
                .checked_mul(block.format().channel_layout().len())
                .and_then(|index| index.checked_add(channel))
                .ok_or_else(|| {
                    exhausted(
                        "read_waveform_sample",
                        "packed audio sample index overflowed",
                    )
                })?,
        )
    };
    let offset = sample_index.checked_mul(bytes_per_sample).ok_or_else(|| {
        exhausted(
            "read_waveform_sample",
            "audio sample byte offset overflowed",
        )
    })?;
    let end = offset
        .checked_add(bytes_per_sample)
        .ok_or_else(|| exhausted("read_waveform_sample", "audio sample byte range overflowed"))?;
    let bytes = block
        .planes()
        .get(plane)
        .and_then(|plane| plane.bytes().get(offset..end))
        .ok_or_else(|| {
            internal(
                "read_waveform_sample",
                "validated audio block storage is missing a requested sample",
            )
        })?;

    let value = match format {
        SampleFormat::U8 | SampleFormat::U8Planar => (f32::from(bytes[0]) - 128.0) / 128.0,
        SampleFormat::I16 | SampleFormat::I16Planar => {
            f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32_768.0
        }
        SampleFormat::I24 | SampleFormat::I24Planar => {
            let raw =
                i32::from(bytes[0]) | (i32::from(bytes[1]) << 8) | (i32::from(bytes[2]) << 16);
            let signed = if raw & 0x0080_0000 == 0 {
                raw
            } else {
                raw | !0x00ff_ffff
            };
            signed as f32 / 8_388_608.0
        }
        SampleFormat::I32 | SampleFormat::I32Planar => {
            i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32 / 2_147_483_648.0
        }
        SampleFormat::F32 | SampleFormat::F32Planar => {
            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }
        SampleFormat::F64 | SampleFormat::F64Planar => {
            let value = f64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]);
            if !value.is_finite() {
                return Err(nonfinite_sample(format, frame, channel));
            }
            value.clamp(-1.0, 1.0) as f32
        }
        _ => {
            return Err(unsupported(
                "read_waveform_sample",
                "decoded audio sample format is not supported for waveform generation",
            ));
        }
    };
    if !value.is_finite() {
        return Err(nonfinite_sample(format, frame, channel));
    }
    Ok(value.clamp(-1.0, 1.0))
}

fn nonfinite_sample(format: SampleFormat, frame: usize, channel: usize) -> Error {
    corrupt(
        "read_waveform_sample",
        "decoded audio contains a nonfinite waveform sample",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "inspect_waveform_sample")
            .with_field("sample_format", format.code())
            .with_field("frame", frame.to_string())
            .with_field("channel", channel.to_string()),
    )
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
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

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn with_context(error: Error, operation: &'static str) -> Error {
    error.with_context(ErrorContext::new(COMPONENT, operation))
}
