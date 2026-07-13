//! Opus audio decode and encode through the permissive libopus backend.
//!
//! Raw packets stay behind the codec-neutral media contracts. OpusHead carries stream layout,
//! pre-skip, input-rate metadata, and output gain. Decoded and encoded audio uses exact sample
//! timing, while the backend compensates libopus lookahead and preserves container metadata.

use std::collections::VecDeque;
use std::fmt;
use std::ptr::NonNull;
use std::sync::Arc;

use libopus_sys as ffi;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{ChannelLayout, ChannelPosition, SampleFormat};
use superi_core::time::{Duration, SampleTime, TimeRounding, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration, BackendTier,
    MediaBackend,
};
use superi_media_io::decode::{DecodeOutput, Decoder, DecoderConfig};
use superi_media_io::demux::{
    BackendId, CodecId, MediaMetadata, MediaSource, MetadataValue, Packet, PacketTiming,
    SourceProbe, SourceProbeResult, SourceRequest, StreamKind,
};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::OperationContext;

/// Stable codec identifier used by streams, registry selection, and capability introspection.
pub const OPUS_CODEC_ID: &str = "opus";

const COMPONENT: &str = "superi-codecs-rs.opus";
const OPUS_HEAD_MAGIC: &[u8; 8] = b"OpusHead";
const OPUS_HEAD_BASE_LEN: usize = 19;
const OPUS_CLOCK_RATE: u32 = 48_000;
const SEEK_PRE_ROLL_NS: u64 = 80_000_000;
const SUPPORTED_SAMPLE_RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];
const MAX_BYTES_PER_STREAM: usize = 1_275;

const FAMILY_MONO: [ChannelPosition; 1] = [ChannelPosition::FrontCenter];
const FAMILY_STEREO: [ChannelPosition; 2] =
    [ChannelPosition::FrontLeft, ChannelPosition::FrontRight];
const FAMILY_THREE: [ChannelPosition; 3] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontCenter,
    ChannelPosition::FrontRight,
];
const FAMILY_QUAD: [ChannelPosition; 4] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontRight,
    ChannelPosition::BackLeft,
    ChannelPosition::BackRight,
];
const FAMILY_FIVE: [ChannelPosition; 5] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontCenter,
    ChannelPosition::FrontRight,
    ChannelPosition::BackLeft,
    ChannelPosition::BackRight,
];
const FAMILY_FIVE_ONE: [ChannelPosition; 6] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontCenter,
    ChannelPosition::FrontRight,
    ChannelPosition::BackLeft,
    ChannelPosition::BackRight,
    ChannelPosition::LowFrequency,
];
const FAMILY_SIX_ONE: [ChannelPosition; 7] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontCenter,
    ChannelPosition::FrontRight,
    ChannelPosition::SideLeft,
    ChannelPosition::SideRight,
    ChannelPosition::BackCenter,
    ChannelPosition::LowFrequency,
];
const FAMILY_SEVEN_ONE: [ChannelPosition; 8] = [
    ChannelPosition::FrontLeft,
    ChannelPosition::FrontCenter,
    ChannelPosition::FrontRight,
    ChannelPosition::SideLeft,
    ChannelPosition::SideRight,
    ChannelPosition::BackLeft,
    ChannelPosition::BackRight,
    ChannelPosition::LowFrequency,
];

const FILE_MAPPING_MONO: [u8; 1] = [0];
const FILE_MAPPING_STEREO: [u8; 2] = [0, 1];
const FILE_MAPPING_THREE: [u8; 3] = [0, 2, 1];
const FILE_MAPPING_QUAD: [u8; 4] = [0, 1, 2, 3];
const FILE_MAPPING_FIVE: [u8; 5] = [0, 4, 1, 2, 3];
const FILE_MAPPING_FIVE_ONE: [u8; 6] = [0, 4, 1, 2, 3, 5];
const FILE_MAPPING_SIX_ONE: [u8; 7] = [0, 4, 1, 2, 3, 5, 6];
const FILE_MAPPING_SEVEN_ONE: [u8; 8] = [0, 6, 1, 2, 3, 4, 5, 7];

/// Default permissive Opus backend.
pub struct OpusBackend {
    descriptor: BackendDescriptor,
}

impl OpusBackend {
    /// Creates the backend with its stable identity.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(BackendId::new("rust-opus")?, "Rust Opus")?,
        })
    }

    /// Returns the stable codec identifier used by streams and backend selection.
    #[must_use]
    pub fn codec_id() -> CodecId {
        CodecId::new(OPUS_CODEC_ID).expect("static Opus codec identifier is valid")
    }

    /// Builds the deterministic primary decode and encode registration.
    pub fn registration() -> Result<BackendRegistration> {
        let codec = Self::codec_id();
        BackendRegistration::new(
            Arc::new(Self::new()?),
            BackendCapabilities::new([
                BackendCapability::Decode(codec.clone()),
                BackendCapability::Encode(codec),
            ]),
            100,
            BackendTier::Primary,
        )
    }
}

impl MediaBackend for OpusBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_opus_source")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_opus_source")?;
        Err(unsupported(
            "open_opus_source",
            "the Opus codec backend does not open media containers",
        ))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_opus_decoder")?;
        Ok(Box::new(OpusDecoder::new(config.clone())?))
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_opus_encoder")?;
        Ok(Box::new(OpusEncoder::new(config.clone())?))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MappingPlan {
    layout: ChannelLayout,
    family: u8,
    streams: u8,
    coupled_streams: u8,
    file_mapping: Vec<u8>,
    codec_mapping: Vec<u8>,
}

impl MappingPlan {
    fn for_layout(layout: &ChannelLayout) -> Result<Self> {
        let channel_count = layout.len();
        let canonical = canonical_layout(channel_count)?;
        if layout != &canonical {
            return Err(unsupported(
                "map_opus_channels",
                "Opus encoding requires a canonical one through eight channel layout",
            ));
        }
        let (family, streams, coupled_streams, file_mapping) =
            standard_file_mapping(channel_count)?;
        let codec_mapping =
            reorder_family_mapping(channel_count, &file_mapping, canonical.positions())?;
        Ok(Self {
            layout: canonical,
            family,
            streams,
            coupled_streams,
            file_mapping,
            codec_mapping,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OpusHead {
    version: u8,
    channel_count: u8,
    pre_skip_48k: u16,
    input_sample_rate: u32,
    output_gain_q8: i16,
    mapping: MappingPlan,
}

impl OpusHead {
    fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < OPUS_HEAD_BASE_LEN || bytes.get(..8) != Some(OPUS_HEAD_MAGIC) {
            return Err(corrupt(
                "parse_opus_head",
                "OpusHead is truncated or has an invalid capture pattern",
            ));
        }
        let version = bytes[8];
        if version >= 16 {
            return Err(unsupported(
                "parse_opus_head",
                "OpusHead major version is not supported",
            ));
        }
        let channel_count = bytes[9];
        if channel_count == 0 {
            return Err(corrupt(
                "parse_opus_head",
                "OpusHead channel count must be greater than zero",
            ));
        }
        let pre_skip_48k = u16::from_le_bytes([bytes[10], bytes[11]]);
        let input_sample_rate = u32::from_le_bytes(bytes[12..16].try_into().expect("fixed range"));
        let output_gain_q8 = i16::from_le_bytes([bytes[16], bytes[17]]);
        let family = bytes[18];
        let mapping = if family == 0 {
            if channel_count > 2 {
                return Err(corrupt(
                    "parse_opus_head",
                    "Opus mapping family zero supports only mono or stereo",
                ));
            }
            let layout = canonical_layout(usize::from(channel_count))?;
            let file_mapping = (0..channel_count).collect::<Vec<_>>();
            MappingPlan {
                layout,
                family,
                streams: 1,
                coupled_streams: channel_count - 1,
                codec_mapping: file_mapping.clone(),
                file_mapping,
            }
        } else {
            let required = OPUS_HEAD_BASE_LEN
                .checked_add(2)
                .and_then(|length| length.checked_add(usize::from(channel_count)))
                .ok_or_else(|| corrupt("parse_opus_head", "OpusHead length overflowed"))?;
            if bytes.len() < required {
                return Err(corrupt(
                    "parse_opus_head",
                    "OpusHead channel mapping table is truncated",
                ));
            }
            let streams = bytes[19];
            let coupled_streams = bytes[20];
            if streams == 0
                || coupled_streams > streams
                || u16::from(streams) + u16::from(coupled_streams) > 255
            {
                return Err(corrupt(
                    "parse_opus_head",
                    "OpusHead stream counts are invalid",
                ));
            }
            let file_mapping = bytes[21..required].to_vec();
            let decoded_channels = streams + coupled_streams;
            if file_mapping
                .iter()
                .any(|index| *index != 255 && *index >= decoded_channels)
            {
                return Err(corrupt(
                    "parse_opus_head",
                    "OpusHead channel mapping references an unavailable decoded channel",
                ));
            }
            let (layout, codec_mapping) = if family == 1 {
                if channel_count > 8 {
                    return Err(corrupt(
                        "parse_opus_head",
                        "Opus mapping family one supports at most eight channels",
                    ));
                }
                let layout = canonical_layout(usize::from(channel_count))?;
                let codec_mapping = reorder_family_mapping(
                    usize::from(channel_count),
                    &file_mapping,
                    layout.positions(),
                )?;
                (layout, codec_mapping)
            } else {
                let layout = ChannelLayout::new(
                    (0..channel_count).map(|index| ChannelPosition::Discrete(u16::from(index))),
                )?;
                (layout, file_mapping.clone())
            };
            MappingPlan {
                layout,
                family,
                streams,
                coupled_streams,
                file_mapping,
                codec_mapping,
            }
        };
        Ok(Self {
            version,
            channel_count,
            pre_skip_48k,
            input_sample_rate,
            output_gain_q8,
            mapping,
        })
    }

    fn for_format(format: &AudioFormat, pre_skip_48k: u16) -> Result<Self> {
        let mapping = MappingPlan::for_layout(format.channel_layout())?;
        let channel_count = u8::try_from(format.channel_layout().len()).map_err(|_| {
            unsupported(
                "create_opus_head",
                "Opus channel count exceeds the supported range",
            )
        })?;
        Ok(Self {
            version: 1,
            channel_count,
            pre_skip_48k,
            input_sample_rate: format.sample_rate(),
            output_gain_q8: 0,
            mapping,
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(
            OPUS_HEAD_BASE_LEN
                + if self.mapping.family == 0 {
                    0
                } else {
                    2 + self.mapping.file_mapping.len()
                },
        );
        bytes.extend_from_slice(OPUS_HEAD_MAGIC);
        bytes.push(self.version);
        bytes.push(self.channel_count);
        bytes.extend_from_slice(&self.pre_skip_48k.to_le_bytes());
        bytes.extend_from_slice(&self.input_sample_rate.to_le_bytes());
        bytes.extend_from_slice(&self.output_gain_q8.to_le_bytes());
        bytes.push(self.mapping.family);
        if self.mapping.family != 0 {
            bytes.push(self.mapping.streams);
            bytes.push(self.mapping.coupled_streams);
            bytes.extend_from_slice(&self.mapping.file_mapping);
        }
        bytes
    }

    fn pre_skip_at_rate(&self, sample_rate: u32) -> Result<u64> {
        Duration::from_samples(u64::from(self.pre_skip_48k), OPUS_CLOCK_RATE)?
            .checked_rescale(
                Timebase::integer(sample_rate)?,
                TimeRounding::NearestTiesEven,
            )
            .map(|duration| duration.value())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LibOpusError(i32);

impl LibOpusError {
    const fn code(self) -> i32 {
        self.0
    }
}

impl fmt::Display for LibOpusError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "libopus error code {}", self.0)
    }
}

impl std::error::Error for LibOpusError {}

fn opus_status(code: i32) -> std::result::Result<(), LibOpusError> {
    if code < 0 {
        Err(LibOpusError(code))
    } else {
        Ok(())
    }
}

fn opus_count(code: i32) -> std::result::Result<usize, LibOpusError> {
    if code < 0 {
        Err(LibOpusError(code))
    } else {
        usize::try_from(code).map_err(|_| LibOpusError(ffi::OPUS_INTERNAL_ERROR))
    }
}

fn opus_i32(value: usize) -> std::result::Result<i32, LibOpusError> {
    i32::try_from(value).map_err(|_| LibOpusError(ffi::OPUS_BAD_ARG))
}

#[allow(unsafe_code)]
fn opus_packet_samples(
    packet: &[u8],
    sample_rate: u32,
) -> std::result::Result<usize, LibOpusError> {
    let length = opus_i32(packet.len())?;
    let sample_rate = i32::try_from(sample_rate).map_err(|_| LibOpusError(ffi::OPUS_BAD_ARG))?;
    // SAFETY: the packet pointer is valid for `length` bytes for the duration of the call.
    opus_count(unsafe { ffi::opus_packet_get_nb_samples(packet.as_ptr(), length, sample_rate) })
}

struct NativeDecoder {
    state: NonNull<ffi::OpusDecoder>,
    channels: usize,
}

// SAFETY: libopus decoder state has unique ownership here and is never accessed concurrently.
#[allow(unsafe_code)]
unsafe impl Send for NativeDecoder {}

#[allow(unsafe_code)]
impl NativeDecoder {
    fn new(sample_rate: u32, channels: usize) -> std::result::Result<Self, LibOpusError> {
        let sample_rate =
            i32::try_from(sample_rate).map_err(|_| LibOpusError(ffi::OPUS_BAD_ARG))?;
        let channel_count = opus_i32(channels)?;
        let mut error = 0;
        // SAFETY: libopus validates the scalar configuration and writes one error code.
        let pointer = unsafe { ffi::opus_decoder_create(sample_rate, channel_count, &mut error) };
        let state = NonNull::new(pointer).ok_or({
            LibOpusError(if error < 0 {
                error
            } else {
                ffi::OPUS_ALLOC_FAIL
            })
        })?;
        if error < 0 {
            // SAFETY: `state` came from opus_decoder_create and has not been freed.
            unsafe { ffi::opus_decoder_destroy(state.as_ptr()) };
            return Err(LibOpusError(error));
        }
        Ok(Self { state, channels })
    }

    fn set_gain(&mut self, gain: i32) -> std::result::Result<(), LibOpusError> {
        // SAFETY: the state is live and the request accepts one promoted signed integer.
        opus_status(unsafe {
            ffi::opus_decoder_ctl(self.state.as_ptr(), ffi::OPUS_SET_GAIN_REQUEST as i32, gain)
        })
    }

    fn decode_i16(
        &mut self,
        packet: &[u8],
        output: &mut [i16],
    ) -> std::result::Result<usize, LibOpusError> {
        if output.len() % self.channels != 0 {
            return Err(LibOpusError(ffi::OPUS_BAD_ARG));
        }
        let packet_len = opus_i32(packet.len())?;
        let frame_capacity = opus_i32(output.len() / self.channels)?;
        // SAFETY: state is live, input and output slices remain valid for the complete call, and
        // frame_capacity describes output samples per channel.
        opus_count(unsafe {
            ffi::opus_decode(
                self.state.as_ptr(),
                packet.as_ptr(),
                packet_len,
                output.as_mut_ptr(),
                frame_capacity,
                0,
            )
        })
    }

    fn decode_f32(
        &mut self,
        packet: &[u8],
        output: &mut [f32],
    ) -> std::result::Result<usize, LibOpusError> {
        if output.len() % self.channels != 0 {
            return Err(LibOpusError(ffi::OPUS_BAD_ARG));
        }
        let packet_len = opus_i32(packet.len())?;
        let frame_capacity = opus_i32(output.len() / self.channels)?;
        // SAFETY: state is live, input and output slices remain valid for the complete call, and
        // frame_capacity describes output samples per channel.
        opus_count(unsafe {
            ffi::opus_decode_float(
                self.state.as_ptr(),
                packet.as_ptr(),
                packet_len,
                output.as_mut_ptr(),
                frame_capacity,
                0,
            )
        })
    }

    fn reset(&mut self) -> std::result::Result<(), LibOpusError> {
        // SAFETY: the state is live and OPUS_RESET_STATE accepts no variadic argument.
        opus_status(unsafe {
            ffi::opus_decoder_ctl(self.state.as_ptr(), ffi::OPUS_RESET_STATE as i32)
        })
    }
}

#[allow(unsafe_code)]
impl Drop for NativeDecoder {
    fn drop(&mut self) {
        // SAFETY: this owner destroys its live state exactly once.
        unsafe { ffi::opus_decoder_destroy(self.state.as_ptr()) };
    }
}

struct NativeMsDecoder {
    state: NonNull<ffi::OpusMSDecoder>,
    channels: usize,
}

// SAFETY: libopus multistream decoder state has unique ownership and no concurrent access.
#[allow(unsafe_code)]
unsafe impl Send for NativeMsDecoder {}

#[allow(unsafe_code)]
impl NativeMsDecoder {
    fn new(
        sample_rate: u32,
        channels: usize,
        streams: u8,
        coupled_streams: u8,
        mapping: &[u8],
    ) -> std::result::Result<Self, LibOpusError> {
        if mapping.len() != channels {
            return Err(LibOpusError(ffi::OPUS_BAD_ARG));
        }
        let sample_rate =
            i32::try_from(sample_rate).map_err(|_| LibOpusError(ffi::OPUS_BAD_ARG))?;
        let channel_count = opus_i32(channels)?;
        let mut error = 0;
        // SAFETY: mapping has one entry per output channel and libopus validates all scalars.
        let pointer = unsafe {
            ffi::opus_multistream_decoder_create(
                sample_rate,
                channel_count,
                i32::from(streams),
                i32::from(coupled_streams),
                mapping.as_ptr(),
                &mut error,
            )
        };
        let state = NonNull::new(pointer).ok_or({
            LibOpusError(if error < 0 {
                error
            } else {
                ffi::OPUS_ALLOC_FAIL
            })
        })?;
        if error < 0 {
            // SAFETY: `state` came from the matching create call and remains live.
            unsafe { ffi::opus_multistream_decoder_destroy(state.as_ptr()) };
            return Err(LibOpusError(error));
        }
        Ok(Self { state, channels })
    }

    fn set_gain(&mut self, gain: i32) -> std::result::Result<(), LibOpusError> {
        // SAFETY: the state is live and the request accepts one promoted signed integer.
        opus_status(unsafe {
            ffi::opus_multistream_decoder_ctl(
                self.state.as_ptr(),
                ffi::OPUS_SET_GAIN_REQUEST as i32,
                gain,
            )
        })
    }

    fn decode_i16(
        &mut self,
        packet: &[u8],
        output: &mut [i16],
    ) -> std::result::Result<usize, LibOpusError> {
        if output.len() % self.channels != 0 {
            return Err(LibOpusError(ffi::OPUS_BAD_ARG));
        }
        let packet_len = opus_i32(packet.len())?;
        let frame_capacity = opus_i32(output.len() / self.channels)?;
        // SAFETY: state is live and both slices cover the lengths passed to libopus.
        opus_count(unsafe {
            ffi::opus_multistream_decode(
                self.state.as_ptr(),
                packet.as_ptr(),
                packet_len,
                output.as_mut_ptr(),
                frame_capacity,
                0,
            )
        })
    }

    fn decode_f32(
        &mut self,
        packet: &[u8],
        output: &mut [f32],
    ) -> std::result::Result<usize, LibOpusError> {
        if output.len() % self.channels != 0 {
            return Err(LibOpusError(ffi::OPUS_BAD_ARG));
        }
        let packet_len = opus_i32(packet.len())?;
        let frame_capacity = opus_i32(output.len() / self.channels)?;
        // SAFETY: state is live and both slices cover the lengths passed to libopus.
        opus_count(unsafe {
            ffi::opus_multistream_decode_float(
                self.state.as_ptr(),
                packet.as_ptr(),
                packet_len,
                output.as_mut_ptr(),
                frame_capacity,
                0,
            )
        })
    }

    fn reset(&mut self) -> std::result::Result<(), LibOpusError> {
        // SAFETY: the state is live and OPUS_RESET_STATE accepts no variadic argument.
        opus_status(unsafe {
            ffi::opus_multistream_decoder_ctl(self.state.as_ptr(), ffi::OPUS_RESET_STATE as i32)
        })
    }
}

#[allow(unsafe_code)]
impl Drop for NativeMsDecoder {
    fn drop(&mut self) {
        // SAFETY: this owner destroys its live state exactly once.
        unsafe { ffi::opus_multistream_decoder_destroy(self.state.as_ptr()) };
    }
}

enum DecoderState {
    Single(NativeDecoder),
    Multi(NativeMsDecoder),
}

impl DecoderState {
    fn new(head: &OpusHead, sample_rate: u32) -> Result<Self> {
        let mut state = if head.mapping.family == 0 {
            Self::Single(
                NativeDecoder::new(sample_rate, usize::from(head.channel_count))
                    .map_err(|source| opus_config_error(source, "create_opus_decoder"))?,
            )
        } else {
            Self::Multi(
                NativeMsDecoder::new(
                    sample_rate,
                    usize::from(head.channel_count),
                    head.mapping.streams,
                    head.mapping.coupled_streams,
                    &head.mapping.codec_mapping,
                )
                .map_err(|source| opus_config_error(source, "create_opus_decoder"))?,
            )
        };
        state.set_gain(i32::from(head.output_gain_q8))?;
        Ok(state)
    }

    fn set_gain(&mut self, gain: i32) -> Result<()> {
        match self {
            Self::Single(decoder) => decoder.set_gain(gain),
            Self::Multi(decoder) => decoder.set_gain(gain),
        }
        .map_err(|source| opus_config_error(source, "configure_opus_decoder_gain"))
    }

    fn decode_i16(&mut self, packet: &[u8], output: &mut [i16]) -> Result<usize> {
        match self {
            Self::Single(decoder) => decoder.decode_i16(packet, output),
            Self::Multi(decoder) => decoder.decode_i16(packet, output),
        }
        .map_err(|source| opus_decode_error(source, "decode_opus_packet"))
    }

    fn decode_f32(&mut self, packet: &[u8], output: &mut [f32]) -> Result<usize> {
        match self {
            Self::Single(decoder) => decoder.decode_f32(packet, output),
            Self::Multi(decoder) => decoder.decode_f32(packet, output),
        }
        .map_err(|source| opus_decode_error(source, "decode_opus_packet"))
    }

    fn reset(&mut self) -> Result<()> {
        match self {
            Self::Single(decoder) => decoder.reset(),
            Self::Multi(decoder) => decoder.reset(),
        }
        .map_err(|source| opus_config_error(source, "reset_opus_decoder"))
    }
}

enum DecodedSamples {
    I16(Vec<i16>),
    F32(Vec<f32>),
}

impl DecodedSamples {
    fn planes(
        &self,
        format: &AudioFormat,
        start_frame: usize,
        frame_count: usize,
    ) -> Result<Vec<AudioPlane>> {
        let channels = format.channel_layout().len();
        let end_frame = start_frame
            .checked_add(frame_count)
            .ok_or_else(|| corrupt("decode_opus_packet", "Opus output range overflowed"))?;
        match self {
            Self::I16(samples) => sample_planes(
                samples,
                channels,
                start_frame,
                end_frame,
                format.sample_format().is_planar(),
                i16::to_le_bytes,
            ),
            Self::F32(samples) => sample_planes(
                samples,
                channels,
                start_frame,
                end_frame,
                format.sample_format().is_planar(),
                f32::to_le_bytes,
            ),
        }
    }
}

struct OpusDecoder {
    config: DecoderConfig,
    format: AudioFormat,
    head: OpusHead,
    state: DecoderState,
    remaining_pre_skip: u64,
    output: VecDeque<AudioBlock>,
    next_sample: Option<i64>,
    saw_audio: bool,
    flushed: bool,
}

impl OpusDecoder {
    fn new(config: DecoderConfig) -> Result<Self> {
        validate_decoder_stream(&config)?;
        let configured_head = configured_opus_head(&config)?;
        let format = decoder_format(&config, configured_head.as_ref())?;
        validate_audio_format(&format, "create_opus_decoder")?;
        let head = configured_head.unwrap_or(OpusHead::for_format(
            &format,
            stream_delay_pre_skip(&config)?,
        )?);
        validate_head_format(&head, &format)?;
        let state = DecoderState::new(&head, format.sample_rate())?;
        let remaining_pre_skip = head.pre_skip_at_rate(format.sample_rate())?;
        Ok(Self {
            config,
            format,
            head,
            state,
            remaining_pre_skip,
            output: VecDeque::new(),
            next_sample: None,
            saw_audio: false,
            flushed: false,
        })
    }

    fn install_header(&mut self, bytes: &[u8]) -> Result<()> {
        if self.saw_audio {
            return Err(corrupt(
                "decode_opus_head",
                "OpusHead arrived after audio packets",
            ));
        }
        let head = OpusHead::parse(bytes)?;
        validate_head_format(&head, &self.format)?;
        self.state = DecoderState::new(&head, self.format.sample_rate())?;
        self.remaining_pre_skip = head.pre_skip_at_rate(self.format.sample_rate())?;
        self.head = head;
        self.next_sample = None;
        Ok(())
    }

    fn decode_packet(&mut self, packet: &Packet) -> Result<()> {
        if packet.stream_id() != self.config.stream().id() {
            return Err(conflict(
                "decode_opus_packet",
                "Opus packet belongs to a different stream",
            ));
        }
        if packet.timing().timebase() != self.config.stream().timebase() {
            return Err(corrupt(
                "decode_opus_packet",
                "Opus packet timebase does not match its stream",
            ));
        }
        if packet.data().is_empty() {
            return Err(corrupt(
                "decode_opus_packet",
                "Opus packet is empty and does not declare packet-loss concealment",
            ));
        }
        let measured_frames = opus_packet_samples(packet.data(), self.format.sample_rate())
            .map_err(|source| opus_decode_error(source, "measure_opus_packet"))?;
        let channel_count = self.format.channel_layout().len();
        let sample_count = measured_frames
            .checked_mul(channel_count)
            .ok_or_else(|| corrupt("decode_opus_packet", "Opus output size overflowed"))?;
        let decoded = if matches!(
            self.format.sample_format(),
            SampleFormat::I16 | SampleFormat::I16Planar
        ) {
            let mut samples = vec![0_i16; sample_count];
            let frames = self.state.decode_i16(packet.data(), &mut samples)?;
            if frames != measured_frames {
                return Err(internal(
                    "decode_opus_packet",
                    "libopus decoded a different sample count than its packet parser",
                ));
            }
            DecodedSamples::I16(samples)
        } else {
            let mut samples = vec![0_f32; sample_count];
            let frames = self.state.decode_f32(packet.data(), &mut samples)?;
            if frames != measured_frames {
                return Err(internal(
                    "decode_opus_packet",
                    "libopus decoded a different sample count than its packet parser",
                ));
            }
            DecodedSamples::F32(samples)
        };

        let pre_skip = usize::try_from(
            self.remaining_pre_skip
                .min(u64::try_from(measured_frames).unwrap_or(u64::MAX)),
        )
        .map_err(|_| corrupt("decode_opus_packet", "Opus pre-skip overflowed"))?;
        self.remaining_pre_skip -= u64::try_from(pre_skip).expect("usize fits u64");
        let mut start_frame = pre_skip;
        let mut frame_count = measured_frames - pre_skip;

        if let Some(duration) = packet_sample_duration(packet, self.format.sample_rate())? {
            let requested = usize::try_from(duration).map_err(|_| {
                corrupt(
                    "decode_opus_packet",
                    "Opus packet duration cannot be represented on this platform",
                )
            })?;
            if requested > frame_count {
                return Err(corrupt(
                    "decode_opus_packet",
                    "Opus packet duration exceeds decoded output after pre-skip",
                ));
            }
            frame_count = requested;
        }

        let (discard_start, discard_end) =
            discard_padding_frames(packet, self.format.sample_rate())?;
        if discard_start > frame_count || discard_end > frame_count - discard_start {
            return Err(corrupt(
                "decode_opus_packet",
                "Opus discard padding exceeds decoded packet duration",
            ));
        }
        start_frame = start_frame
            .checked_add(discard_start)
            .ok_or_else(|| corrupt("decode_opus_packet", "Opus trim range overflowed"))?;
        frame_count -= discard_start + discard_end;

        let mut timestamp =
            packet_sample_timestamp(packet, self.format.sample_rate(), self.next_sample)?;
        if discard_start > 0 {
            timestamp = SampleTime::new(
                timestamp
                    .sample()
                    .checked_add(
                        i64::try_from(discard_start).map_err(|_| {
                            corrupt("decode_opus_packet", "Opus timestamp overflowed")
                        })?,
                    )
                    .ok_or_else(|| corrupt("decode_opus_packet", "Opus timestamp overflowed"))?,
                self.format.sample_rate(),
            )?;
        }
        let next_sample = timestamp
            .sample()
            .checked_add(
                i64::try_from(frame_count)
                    .map_err(|_| corrupt("decode_opus_packet", "Opus sample cursor overflowed"))?,
            )
            .ok_or_else(|| corrupt("decode_opus_packet", "Opus sample cursor overflowed"))?;
        self.next_sample = Some(next_sample);
        self.saw_audio = true;
        if frame_count == 0 {
            return Ok(());
        }

        let planes = decoded.planes(&self.format, start_frame, frame_count)?;
        let mut block = AudioBlock::new(
            self.format.clone(),
            timestamp,
            u64::try_from(frame_count)
                .map_err(|_| corrupt("decode_opus_packet", "Opus frame count overflowed"))?,
            planes,
        )?;
        for (key, value) in packet.metadata().iter() {
            block = block.with_metadata(key, value.clone())?;
        }
        self.output.push_back(block);
        Ok(())
    }
}

impl Decoder for OpusDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("send_opus_packet")?;
        if self.flushed {
            return Err(conflict(
                "send_opus_packet",
                "cannot send Opus packets after flush without reset",
            ));
        }
        if packet.data().starts_with(OPUS_HEAD_MAGIC) {
            if packet.stream_id() != self.config.stream().id() {
                return Err(conflict(
                    "decode_opus_head",
                    "OpusHead belongs to a different stream",
                ));
            }
            self.install_header(packet.data())?;
        } else {
            self.decode_packet(&packet)?;
        }
        operation.check("send_opus_packet")
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("receive_opus_audio")?;
        if let Some(block) = self.output.pop_front() {
            return Ok(DecodeOutput::Audio(block));
        }
        if self.flushed {
            Ok(DecodeOutput::EndOfStream)
        } else {
            Ok(DecodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_opus_decoder")?;
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_opus_decoder")?;
        self.state.reset()?;
        self.remaining_pre_skip = 0;
        self.output.clear();
        self.next_sample = None;
        self.saw_audio = false;
        self.flushed = false;
        Ok(())
    }
}

struct NativeEncoder {
    state: NonNull<ffi::OpusEncoder>,
    channels: usize,
}

// SAFETY: libopus encoder state has unique ownership here and is never accessed concurrently.
#[allow(unsafe_code)]
unsafe impl Send for NativeEncoder {}

#[allow(unsafe_code)]
impl NativeEncoder {
    fn new(sample_rate: u32, channels: usize) -> std::result::Result<Self, LibOpusError> {
        let sample_rate =
            i32::try_from(sample_rate).map_err(|_| LibOpusError(ffi::OPUS_BAD_ARG))?;
        let channel_count = opus_i32(channels)?;
        let mut error = 0;
        // SAFETY: libopus validates the scalar configuration and writes one error code.
        let pointer = unsafe {
            ffi::opus_encoder_create(
                sample_rate,
                channel_count,
                ffi::OPUS_APPLICATION_AUDIO as i32,
                &mut error,
            )
        };
        let state = NonNull::new(pointer).ok_or({
            LibOpusError(if error < 0 {
                error
            } else {
                ffi::OPUS_ALLOC_FAIL
            })
        })?;
        if error < 0 {
            // SAFETY: `state` came from opus_encoder_create and has not been freed.
            unsafe { ffi::opus_encoder_destroy(state.as_ptr()) };
            return Err(LibOpusError(error));
        }
        Ok(Self { state, channels })
    }

    fn lookahead(&mut self) -> std::result::Result<i32, LibOpusError> {
        let mut samples = 0_i32;
        // SAFETY: the state is live and OPUS_GET_LOOKAHEAD writes one signed integer.
        opus_status(unsafe {
            ffi::opus_encoder_ctl(
                self.state.as_ptr(),
                ffi::OPUS_GET_LOOKAHEAD_REQUEST as i32,
                &mut samples,
            )
        })?;
        Ok(samples)
    }

    fn encode_f32(
        &mut self,
        input: &[f32],
        output: &mut [u8],
    ) -> std::result::Result<usize, LibOpusError> {
        if input.len() % self.channels != 0 {
            return Err(LibOpusError(ffi::OPUS_BAD_ARG));
        }
        let frame_size = opus_i32(input.len() / self.channels)?;
        let output_capacity = opus_i32(output.len())?;
        // SAFETY: state is live and the input and output slices cover every length passed.
        opus_count(unsafe {
            ffi::opus_encode_float(
                self.state.as_ptr(),
                input.as_ptr(),
                frame_size,
                output.as_mut_ptr(),
                output_capacity,
            )
        })
    }
}

#[allow(unsafe_code)]
impl Drop for NativeEncoder {
    fn drop(&mut self) {
        // SAFETY: this owner destroys its live state exactly once.
        unsafe { ffi::opus_encoder_destroy(self.state.as_ptr()) };
    }
}

struct NativeMsEncoder {
    state: NonNull<ffi::OpusMSEncoder>,
    channels: usize,
}

// SAFETY: libopus multistream encoder state has unique ownership and no concurrent access.
#[allow(unsafe_code)]
unsafe impl Send for NativeMsEncoder {}

#[allow(unsafe_code)]
impl NativeMsEncoder {
    fn new(
        sample_rate: u32,
        channels: usize,
        streams: u8,
        coupled_streams: u8,
        mapping: &[u8],
    ) -> std::result::Result<Self, LibOpusError> {
        if mapping.len() != channels {
            return Err(LibOpusError(ffi::OPUS_BAD_ARG));
        }
        let sample_rate =
            i32::try_from(sample_rate).map_err(|_| LibOpusError(ffi::OPUS_BAD_ARG))?;
        let channel_count = opus_i32(channels)?;
        let mut error = 0;
        // SAFETY: mapping has one entry per input channel and libopus validates all scalars.
        let pointer = unsafe {
            ffi::opus_multistream_encoder_create(
                sample_rate,
                channel_count,
                i32::from(streams),
                i32::from(coupled_streams),
                mapping.as_ptr(),
                ffi::OPUS_APPLICATION_AUDIO as i32,
                &mut error,
            )
        };
        let state = NonNull::new(pointer).ok_or({
            LibOpusError(if error < 0 {
                error
            } else {
                ffi::OPUS_ALLOC_FAIL
            })
        })?;
        if error < 0 {
            // SAFETY: `state` came from the matching create call and remains live.
            unsafe { ffi::opus_multistream_encoder_destroy(state.as_ptr()) };
            return Err(LibOpusError(error));
        }
        Ok(Self { state, channels })
    }

    fn lookahead(&mut self) -> std::result::Result<i32, LibOpusError> {
        let mut samples = 0_i32;
        // SAFETY: the state is live and OPUS_GET_LOOKAHEAD writes one signed integer.
        opus_status(unsafe {
            ffi::opus_multistream_encoder_ctl(
                self.state.as_ptr(),
                ffi::OPUS_GET_LOOKAHEAD_REQUEST as i32,
                &mut samples,
            )
        })?;
        Ok(samples)
    }

    fn encode_f32(
        &mut self,
        input: &[f32],
        output: &mut [u8],
    ) -> std::result::Result<usize, LibOpusError> {
        if input.len() % self.channels != 0 {
            return Err(LibOpusError(ffi::OPUS_BAD_ARG));
        }
        let frame_size = opus_i32(input.len() / self.channels)?;
        let output_capacity = opus_i32(output.len())?;
        // SAFETY: state is live and the input and output slices cover every length passed.
        opus_count(unsafe {
            ffi::opus_multistream_encode_float(
                self.state.as_ptr(),
                input.as_ptr(),
                frame_size,
                output.as_mut_ptr(),
                output_capacity,
            )
        })
    }
}

#[allow(unsafe_code)]
impl Drop for NativeMsEncoder {
    fn drop(&mut self) {
        // SAFETY: this owner destroys its live state exactly once.
        unsafe { ffi::opus_multistream_encoder_destroy(self.state.as_ptr()) };
    }
}

enum EncoderState {
    Single(NativeEncoder),
    Multi(NativeMsEncoder),
}

impl EncoderState {
    fn new(mapping: &MappingPlan, sample_rate: u32) -> Result<Self> {
        if mapping.family == 0 {
            NativeEncoder::new(sample_rate, mapping.layout.len())
                .map(Self::Single)
                .map_err(|source| opus_config_error(source, "create_opus_encoder"))
        } else {
            NativeMsEncoder::new(
                sample_rate,
                mapping.layout.len(),
                mapping.streams,
                mapping.coupled_streams,
                &mapping.codec_mapping,
            )
            .map(Self::Multi)
            .map_err(|source| opus_config_error(source, "create_opus_encoder"))
        }
    }

    fn lookahead(&mut self) -> Result<u64> {
        let samples = match self {
            Self::Single(encoder) => encoder.lookahead(),
            Self::Multi(encoder) => encoder.lookahead(),
        }
        .map_err(|source| opus_config_error(source, "read_opus_encoder_lookahead"))?;
        u64::try_from(samples).map_err(|_| {
            internal(
                "read_opus_encoder_lookahead",
                "libopus returned a negative lookahead",
            )
        })
    }

    fn encode_f32(&mut self, input: &[f32], output: &mut [u8]) -> Result<usize> {
        match self {
            Self::Single(encoder) => encoder.encode_f32(input, output),
            Self::Multi(encoder) => encoder.encode_f32(input, output),
        }
        .map_err(|source| opus_encode_error(source, "encode_opus_frame"))
    }
}

struct MetadataSpan {
    start: u64,
    end: u64,
    metadata: MediaMetadata,
}

struct OpusEncoder {
    config: EncoderConfig,
    format: AudioFormat,
    head: OpusHead,
    state: EncoderState,
    frame_size: usize,
    max_packet_bytes: usize,
    lookahead: u64,
    samples: VecDeque<f32>,
    output: VecDeque<Packet>,
    timeline_origin: Option<i64>,
    next_input_sample: Option<i64>,
    input_frame_count: u64,
    encoded_frame_count: u64,
    logical_packet_cursor: u64,
    metadata: Vec<MetadataSpan>,
    flushed: bool,
}

impl OpusEncoder {
    fn new(config: EncoderConfig) -> Result<Self> {
        if config.codec().as_str() != OPUS_CODEC_ID {
            return Err(unsupported(
                "create_opus_encoder",
                "the requested codec is not Opus",
            ));
        }
        let EncoderMediaFormat::Audio(format) = config.media_format() else {
            return Err(invalid(
                "create_opus_encoder",
                "Opus encoding requires an audio format",
            ));
        };
        validate_audio_format(format, "create_opus_encoder")?;
        let format = format.clone();
        let mapping = MappingPlan::for_layout(format.channel_layout())?;
        let mut state = EncoderState::new(&mapping, format.sample_rate())?;
        let lookahead = state.lookahead()?;
        let pre_skip_48k = lookahead
            .checked_mul(u64::from(OPUS_CLOCK_RATE))
            .and_then(|samples| samples.checked_div(u64::from(format.sample_rate())))
            .ok_or_else(|| internal("create_opus_encoder", "Opus pre-skip overflowed"))?;
        let expected_pre_skip_scale = lookahead
            .checked_mul(u64::from(OPUS_CLOCK_RATE))
            .ok_or_else(|| internal("create_opus_encoder", "Opus pre-skip overflowed"))?;
        if pre_skip_48k.checked_mul(u64::from(format.sample_rate()))
            != Some(expected_pre_skip_scale)
        {
            return Err(internal(
                "create_opus_encoder",
                "libopus lookahead does not map exactly to the 48 kHz Opus clock",
            ));
        }
        let pre_skip_48k = u16::try_from(pre_skip_48k)
            .map_err(|_| internal("create_opus_encoder", "Opus pre-skip exceeds OpusHead"))?;
        let head = OpusHead::for_format(&format, pre_skip_48k)?;
        let frame_size = usize::try_from(format.sample_rate() / 50)
            .map_err(|_| invalid("create_opus_encoder", "Opus frame size overflowed"))?;
        let max_packet_bytes = usize::from(mapping.streams)
            .checked_mul(MAX_BYTES_PER_STREAM)
            .and_then(|size| size.checked_add(usize::from(mapping.streams.saturating_sub(1)) * 2))
            .ok_or_else(|| internal("create_opus_encoder", "Opus packet capacity overflowed"))?;
        let mut encoder = Self {
            config,
            format,
            head,
            state,
            frame_size,
            max_packet_bytes,
            lookahead,
            samples: VecDeque::new(),
            output: VecDeque::new(),
            timeline_origin: None,
            next_input_sample: None,
            input_frame_count: 0,
            encoded_frame_count: 0,
            logical_packet_cursor: 0,
            metadata: Vec::new(),
            flushed: false,
        };
        encoder.queue_header()?;
        Ok(encoder)
    }

    fn queue_header(&mut self) -> Result<()> {
        let timing = PacketTiming::new(self.config.timebase(), None, None, None)?;
        let mut packet = Packet::new(
            self.config.stream_id(),
            Arc::from(self.head.to_bytes()),
            timing,
        )
        .with_keyframe(true);
        packet.metadata_mut().insert(
            "codec.header",
            MetadataValue::Text("identification".to_owned()),
        )?;
        packet.metadata_mut().insert(
            "codec.opus.pre-skip-48k",
            MetadataValue::Unsigned(u64::from(self.head.pre_skip_48k)),
        )?;
        packet.metadata_mut().insert(
            "codec.opus.mapping-family",
            MetadataValue::Unsigned(u64::from(self.head.mapping.family)),
        )?;
        packet.metadata_mut().insert(
            "codec.seek-pre-roll-ns",
            MetadataValue::Unsigned(SEEK_PRE_ROLL_NS),
        )?;
        self.output.push_back(packet);
        Ok(())
    }

    fn append_block(&mut self, block: AudioBlock, operation: &OperationContext) -> Result<()> {
        if block.format() != &self.format {
            return Err(invalid(
                "encode_opus_block",
                "Opus audio block format does not match the encoder configuration",
            ));
        }
        if self
            .next_input_sample
            .is_some_and(|expected| expected != block.timestamp().sample())
        {
            return Err(conflict(
                "encode_opus_block",
                "Opus input blocks must be contiguous in sample time",
            ));
        }
        let start = self.input_frame_count;
        let end = start
            .checked_add(block.frame_count())
            .ok_or_else(|| invalid("encode_opus_block", "Opus input duration overflowed"))?;
        let next_input_sample = block
            .timestamp()
            .sample()
            .checked_add(
                i64::try_from(block.frame_count())
                    .map_err(|_| invalid("encode_opus_block", "Opus input timestamp overflowed"))?,
            )
            .ok_or_else(|| invalid("encode_opus_block", "Opus input timestamp overflowed"))?;
        self.timeline_origin
            .get_or_insert(block.timestamp().sample());
        if start != end && !block.metadata().is_empty() {
            self.metadata.push(MetadataSpan {
                start,
                end,
                metadata: block.metadata().clone(),
            });
        }
        self.samples.extend(audio_block_to_f32(&block, operation)?);
        self.input_frame_count = end;
        self.next_input_sample = Some(next_input_sample);
        self.encode_ready(operation)
    }

    fn encode_ready(&mut self, operation: &OperationContext) -> Result<()> {
        let channels = self.format.channel_layout().len();
        let frame_samples = self
            .frame_size
            .checked_mul(channels)
            .ok_or_else(|| invalid("encode_opus_frame", "Opus frame allocation overflowed"))?;
        while self.samples.len() >= frame_samples {
            operation.check("encode_opus_frame")?;
            let frame = (0..frame_samples)
                .map(|_| {
                    self.samples
                        .pop_front()
                        .expect("complete Opus frame is buffered")
                })
                .collect::<Vec<_>>();
            self.encode_frame(&frame)?;
        }
        Ok(())
    }

    fn encode_frame(&mut self, frame: &[f32]) -> Result<()> {
        let mut encoded = vec![0_u8; self.max_packet_bytes];
        let byte_count = self.state.encode_f32(frame, &mut encoded)?;
        encoded.truncate(byte_count);
        let measured = opus_packet_samples(&encoded, self.format.sample_rate())
            .map_err(|source| opus_encode_error(source, "measure_encoded_opus_packet"))?;
        if measured != self.frame_size {
            return Err(internal(
                "measure_encoded_opus_packet",
                "libopus encoded an unexpected frame duration",
            ));
        }
        let raw_start = self.encoded_frame_count;
        let raw_end =
            raw_start
                .checked_add(u64::try_from(measured).map_err(|_| {
                    internal("encode_opus_frame", "Opus encoded duration overflowed")
                })?)
                .ok_or_else(|| internal("encode_opus_frame", "Opus encoded duration overflowed"))?;
        let logical_start = raw_start
            .saturating_sub(self.lookahead)
            .min(self.input_frame_count);
        let logical_end = raw_end
            .saturating_sub(self.lookahead)
            .min(self.input_frame_count);
        if logical_start != self.logical_packet_cursor || logical_end < logical_start {
            return Err(internal(
                "encode_opus_frame",
                "Opus logical packet timing became discontinuous",
            ));
        }
        let duration = logical_end - logical_start;
        let origin = self.timeline_origin.unwrap_or(0);
        let timestamp =
            origin
                .checked_add(i64::try_from(logical_start).map_err(|_| {
                    internal("encode_opus_frame", "Opus packet timestamp overflowed")
                })?)
                .ok_or_else(|| internal("encode_opus_frame", "Opus packet timestamp overflowed"))?;
        let timing = PacketTiming::new(
            self.config.timebase(),
            Some(timestamp),
            Some(timestamp),
            Some(duration),
        )?;
        let mut packet =
            Packet::new(self.config.stream_id(), Arc::from(encoded), timing).with_keyframe(true);
        if let Some(metadata) = self.metadata_for(logical_start) {
            for (key, value) in metadata.iter() {
                packet.metadata_mut().insert(key, value.clone())?;
            }
        }
        let decoded_after_pre_skip = raw_end
            .saturating_sub(self.lookahead)
            .saturating_sub(logical_start);
        let padding = decoded_after_pre_skip.saturating_sub(duration);
        if padding > 0 {
            packet.metadata_mut().insert(
                "codec.opus.padding-samples",
                MetadataValue::Unsigned(padding),
            )?;
        }
        self.encoded_frame_count = raw_end;
        self.logical_packet_cursor = logical_end;
        self.output.push_back(packet);
        Ok(())
    }

    fn metadata_for(&self, sample: u64) -> Option<&MediaMetadata> {
        self.metadata
            .iter()
            .find(|span| span.start <= sample && sample < span.end)
            .or_else(|| self.metadata.last())
            .map(|span| &span.metadata)
    }

    fn finish(&mut self, operation: &OperationContext) -> Result<()> {
        if self.input_frame_count == 0 {
            self.flushed = true;
            return Ok(());
        }
        let target = self
            .input_frame_count
            .checked_add(self.lookahead)
            .ok_or_else(|| invalid("flush_opus_encoder", "Opus padded duration overflowed"))?;
        let channels = self.format.channel_layout().len();
        let frame_samples = self
            .frame_size
            .checked_mul(channels)
            .ok_or_else(|| invalid("flush_opus_encoder", "Opus frame allocation overflowed"))?;
        while self.encoded_frame_count < target {
            operation.check("flush_opus_encoder")?;
            self.samples.resize(frame_samples, 0.0);
            self.encode_ready(operation)?;
        }
        if self.logical_packet_cursor != self.input_frame_count {
            return Err(internal(
                "flush_opus_encoder",
                "Opus encoded duration does not match submitted audio",
            ));
        }
        self.flushed = true;
        Ok(())
    }

    fn rebuild(&mut self) -> Result<()> {
        let mapping = MappingPlan::for_layout(self.format.channel_layout())?;
        self.state = EncoderState::new(&mapping, self.format.sample_rate())?;
        let lookahead = self.state.lookahead()?;
        if lookahead != self.lookahead {
            return Err(internal(
                "reset_opus_encoder",
                "libopus lookahead changed across encoder reset",
            ));
        }
        self.samples.clear();
        self.output.clear();
        self.timeline_origin = None;
        self.next_input_sample = None;
        self.input_frame_count = 0;
        self.encoded_frame_count = 0;
        self.logical_packet_cursor = 0;
        self.metadata.clear();
        self.flushed = false;
        self.queue_header()
    }
}

impl Encoder for OpusEncoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("send_opus_audio")?;
        if self.flushed {
            return Err(conflict(
                "send_opus_audio",
                "cannot send Opus audio after flush without reset",
            ));
        }
        let EncodeInput::Audio(block) = input else {
            return Err(invalid(
                "send_opus_audio",
                "Opus encoders accept only audio blocks",
            ));
        };
        self.append_block(block, operation)
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("receive_opus_packet")?;
        if let Some(packet) = self.output.pop_front() {
            return Ok(EncodeOutput::Packet(packet));
        }
        if self.flushed {
            Ok(EncodeOutput::EndOfStream)
        } else {
            Ok(EncodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_opus_encoder")?;
        if !self.flushed {
            self.finish(operation)?;
        }
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_opus_encoder")?;
        self.rebuild()
    }
}

fn validate_decoder_stream(config: &DecoderConfig) -> Result<()> {
    if config.stream().kind() != StreamKind::Audio {
        return Err(invalid(
            "create_opus_decoder",
            "Opus decoding requires an audio stream",
        ));
    }
    if config.stream().codec().as_str() != OPUS_CODEC_ID {
        return Err(unsupported(
            "create_opus_decoder",
            "the requested codec is not Opus",
        ));
    }
    Ok(())
}

fn configured_opus_head(config: &DecoderConfig) -> Result<Option<OpusHead>> {
    match config.stream().metadata().get("codec.configuration") {
        Some(MetadataValue::Bytes(bytes)) => OpusHead::parse(bytes).map(Some),
        Some(_) => Err(invalid(
            "create_opus_decoder",
            "Opus codec.configuration metadata must contain bytes",
        )),
        None => Ok(None),
    }
}

fn decoder_format(config: &DecoderConfig, head: Option<&OpusHead>) -> Result<AudioFormat> {
    if let Some(format) = config.audio_format() {
        if let Some(head) = head {
            validate_head_format(head, format)?;
        }
        return Ok(format.clone());
    }
    let head = head.ok_or_else(|| {
        invalid(
            "create_opus_decoder",
            "Opus decoding requires OpusHead or an explicit audio format",
        )
    })?;
    AudioFormat::new(
        OPUS_CLOCK_RATE,
        SampleFormat::F32Planar,
        head.mapping.layout.clone(),
    )
}

fn validate_head_format(head: &OpusHead, format: &AudioFormat) -> Result<()> {
    if format.channel_layout() != &head.mapping.layout {
        return Err(unsupported(
            "create_opus_decoder",
            "OpusHead channel meaning does not match the requested audio layout",
        ));
    }
    Ok(())
}

fn stream_delay_pre_skip(config: &DecoderConfig) -> Result<u16> {
    let Some(MetadataValue::Unsigned(delay_ns)) = config.stream().metadata().get("codec.delay-ns")
    else {
        return Ok(0);
    };
    let samples = Duration::new(*delay_ns, Timebase::NANOSECONDS)?
        .checked_rescale(
            Timebase::integer(OPUS_CLOCK_RATE)?,
            TimeRounding::NearestTiesEven,
        )?
        .value();
    u16::try_from(samples)
        .map_err(|_| invalid("create_opus_decoder", "Opus codec delay exceeds OpusHead"))
}

fn validate_audio_format(format: &AudioFormat, operation: &'static str) -> Result<()> {
    if !SUPPORTED_SAMPLE_RATES.contains(&format.sample_rate()) {
        return Err(unsupported(
            operation,
            "Opus supports 8, 12, 16, 24, or 48 kHz audio",
        ));
    }
    if !matches!(
        format.sample_format(),
        SampleFormat::I16 | SampleFormat::I16Planar | SampleFormat::F32 | SampleFormat::F32Planar
    ) {
        return Err(unsupported(
            operation,
            "Opus accepts signed 16-bit or 32-bit float packed or planar audio",
        ));
    }
    if format.channel_layout().len() > 255 {
        return Err(unsupported(
            operation,
            "Opus supports at most 255 output channels",
        ));
    }
    Ok(())
}

fn standard_file_mapping(channel_count: usize) -> Result<(u8, u8, u8, Vec<u8>)> {
    let result = match channel_count {
        1 => (0, 1, 0, FILE_MAPPING_MONO.as_slice()),
        2 => (0, 1, 1, FILE_MAPPING_STEREO.as_slice()),
        3 => (1, 2, 1, FILE_MAPPING_THREE.as_slice()),
        4 => (1, 2, 2, FILE_MAPPING_QUAD.as_slice()),
        5 => (1, 3, 2, FILE_MAPPING_FIVE.as_slice()),
        6 => (1, 4, 2, FILE_MAPPING_FIVE_ONE.as_slice()),
        7 => (1, 4, 3, FILE_MAPPING_SIX_ONE.as_slice()),
        8 => (1, 5, 3, FILE_MAPPING_SEVEN_ONE.as_slice()),
        _ => {
            return Err(unsupported(
                "map_opus_channels",
                "Opus encoding supports standard one through eight channel mappings",
            ));
        }
    };
    Ok((result.0, result.1, result.2, result.3.to_vec()))
}

fn family_positions(channel_count: usize) -> Result<&'static [ChannelPosition]> {
    match channel_count {
        1 => Ok(&FAMILY_MONO),
        2 => Ok(&FAMILY_STEREO),
        3 => Ok(&FAMILY_THREE),
        4 => Ok(&FAMILY_QUAD),
        5 => Ok(&FAMILY_FIVE),
        6 => Ok(&FAMILY_FIVE_ONE),
        7 => Ok(&FAMILY_SIX_ONE),
        8 => Ok(&FAMILY_SEVEN_ONE),
        _ => Err(unsupported(
            "map_opus_channels",
            "Opus family one supports one through eight channels",
        )),
    }
}

fn canonical_layout(channel_count: usize) -> Result<ChannelLayout> {
    match channel_count {
        1 => Ok(ChannelLayout::mono()),
        2 => Ok(ChannelLayout::stereo()),
        3 => ChannelLayout::new([
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::FrontCenter,
        ]),
        4 => Ok(ChannelLayout::quad()),
        5 => ChannelLayout::new([
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::FrontCenter,
            ChannelPosition::BackLeft,
            ChannelPosition::BackRight,
        ]),
        6 => Ok(ChannelLayout::surround_5_1()),
        7 => ChannelLayout::new([
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::FrontCenter,
            ChannelPosition::LowFrequency,
            ChannelPosition::BackCenter,
            ChannelPosition::SideLeft,
            ChannelPosition::SideRight,
        ]),
        8 => Ok(ChannelLayout::surround_7_1()),
        _ => Err(unsupported(
            "map_opus_channels",
            "Opus semantic channel mapping supports one through eight channels",
        )),
    }
}

fn reorder_family_mapping(
    channel_count: usize,
    file_mapping: &[u8],
    output_positions: &[ChannelPosition],
) -> Result<Vec<u8>> {
    let family = family_positions(channel_count)?;
    output_positions
        .iter()
        .map(|position| {
            family
                .iter()
                .position(|candidate| candidate == position)
                .and_then(|index| file_mapping.get(index).copied())
                .ok_or_else(|| {
                    unsupported(
                        "map_opus_channels",
                        "audio layout does not match the Opus mapping family",
                    )
                })
        })
        .collect()
}

fn audio_block_to_f32(block: &AudioBlock, operation: &OperationContext) -> Result<Vec<f32>> {
    let frames = usize::try_from(block.frame_count()).map_err(|_| {
        invalid(
            "encode_opus_block",
            "Opus input frame count cannot be represented on this platform",
        )
    })?;
    let channels = block.format().channel_layout().len();
    let mut output = Vec::with_capacity(
        frames
            .checked_mul(channels)
            .ok_or_else(|| invalid("encode_opus_block", "Opus input size overflowed"))?,
    );
    for frame in 0..frames {
        operation.check("convert_opus_audio")?;
        for channel in 0..channels {
            let sample = read_input_sample(block, frame, channel)?;
            if !sample.is_finite() {
                return Err(invalid(
                    "encode_opus_block",
                    "Opus floating-point input samples must be finite",
                ));
            }
            output.push(sample);
        }
    }
    Ok(output)
}

fn read_input_sample(block: &AudioBlock, frame: usize, channel: usize) -> Result<f32> {
    let format = block.format();
    let channels = format.channel_layout().len();
    let (plane, sample_index) = if format.sample_format().is_planar() {
        (&block.planes()[channel], frame)
    } else {
        (&block.planes()[0], frame * channels + channel)
    };
    match format.sample_format() {
        SampleFormat::I16 | SampleFormat::I16Planar => {
            let offset = sample_index
                .checked_mul(2)
                .ok_or_else(|| invalid("encode_opus_block", "Opus sample offset overflowed"))?;
            let bytes: [u8; 2] = plane.bytes()[offset..offset + 2]
                .try_into()
                .expect("validated audio plane range");
            Ok(f32::from(i16::from_le_bytes(bytes)) / 32_768.0)
        }
        SampleFormat::F32 | SampleFormat::F32Planar => {
            let offset = sample_index
                .checked_mul(4)
                .ok_or_else(|| invalid("encode_opus_block", "Opus sample offset overflowed"))?;
            let bytes: [u8; 4] = plane.bytes()[offset..offset + 4]
                .try_into()
                .expect("validated audio plane range");
            Ok(f32::from_le_bytes(bytes))
        }
        _ => Err(unsupported(
            "encode_opus_block",
            "Opus input sample format is unsupported",
        )),
    }
}

fn sample_planes<T, const N: usize>(
    samples: &[T],
    channels: usize,
    start_frame: usize,
    end_frame: usize,
    planar: bool,
    to_bytes: impl Fn(T) -> [u8; N],
) -> Result<Vec<AudioPlane>>
where
    T: Copy,
{
    if planar {
        Ok((0..channels)
            .map(|channel| {
                let bytes = (start_frame..end_frame)
                    .flat_map(|frame| to_bytes(samples[frame * channels + channel]))
                    .collect::<Vec<_>>();
                AudioPlane::new(Arc::from(bytes))
            })
            .collect())
    } else {
        let start = start_frame
            .checked_mul(channels)
            .ok_or_else(|| corrupt("decode_opus_packet", "Opus sample range overflowed"))?;
        let end = end_frame
            .checked_mul(channels)
            .ok_or_else(|| corrupt("decode_opus_packet", "Opus sample range overflowed"))?;
        let bytes = samples[start..end]
            .iter()
            .copied()
            .flat_map(to_bytes)
            .collect::<Vec<_>>();
        Ok(vec![AudioPlane::new(Arc::from(bytes))])
    }
}

fn packet_sample_duration(packet: &Packet, sample_rate: u32) -> Result<Option<u64>> {
    let Some(duration) = packet.timing().duration() else {
        return Ok(None);
    };
    duration
        .checked_rescale(
            Timebase::integer(sample_rate)?,
            TimeRounding::NearestTiesEven,
        )
        .map(|duration| Some(duration.value()))
        .map_err(|source| {
            corrupt_source(
                "decode_opus_packet",
                "Opus packet duration cannot be mapped to the sample timeline",
                source,
            )
        })
}

fn packet_sample_timestamp(
    packet: &Packet,
    sample_rate: u32,
    inferred: Option<i64>,
) -> Result<SampleTime> {
    if let Some(inferred) = inferred {
        return SampleTime::new(inferred, sample_rate);
    }
    let Some(presentation) = packet.timing().presentation_time() else {
        return SampleTime::new(0, sample_rate);
    };
    let converted = presentation
        .checked_rescale(
            Timebase::integer(sample_rate)?,
            TimeRounding::NearestTiesEven,
        )
        .map_err(|source| {
            corrupt_source(
                "decode_opus_packet",
                "Opus presentation time cannot be mapped to the sample timeline",
                source,
            )
        })?;
    SampleTime::new(converted.value(), sample_rate)
}

fn discard_padding_frames(packet: &Packet, sample_rate: u32) -> Result<(usize, usize)> {
    let Some(MetadataValue::Signed(padding_ns)) =
        packet.metadata().get("container.discard-padding-ns")
    else {
        return Ok((0, 0));
    };
    let magnitude = padding_ns.unsigned_abs();
    let frames = Duration::new(magnitude, Timebase::NANOSECONDS)?
        .checked_rescale(
            Timebase::integer(sample_rate)?,
            TimeRounding::NearestTiesEven,
        )?
        .value();
    let frames = usize::try_from(frames)
        .map_err(|_| corrupt("decode_opus_packet", "Opus discard padding overflowed"))?;
    if *padding_ns >= 0 {
        Ok((0, frames))
    } else {
        Ok((frames, 0))
    }
}

fn opus_config_error(source: LibOpusError, operation: &'static str) -> Error {
    let (category, recoverability, message) = match source.code() {
        ffi::OPUS_BAD_ARG | ffi::OPUS_UNIMPLEMENTED => (
            ErrorCategory::Unsupported,
            Recoverability::Degraded,
            "libopus rejected the codec configuration",
        ),
        ffi::OPUS_ALLOC_FAIL => (
            ErrorCategory::ResourceExhausted,
            Recoverability::Retryable,
            "libopus could not allocate codec state",
        ),
        _ => (
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "libopus failed to initialize codec state",
        ),
    };
    Error::with_source(category, recoverability, message, source).with_context(context(operation))
}

fn opus_decode_error(source: LibOpusError, operation: &'static str) -> Error {
    let (category, recoverability, message) = match source.code() {
        ffi::OPUS_INVALID_PACKET | ffi::OPUS_BAD_ARG => (
            ErrorCategory::CorruptData,
            Recoverability::Degraded,
            "Opus packet is corrupt",
        ),
        ffi::OPUS_ALLOC_FAIL => (
            ErrorCategory::ResourceExhausted,
            Recoverability::Retryable,
            "libopus could not allocate decode state",
        ),
        _ => (
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "libopus failed while decoding",
        ),
    };
    Error::with_source(category, recoverability, message, source).with_context(context(operation))
}

fn opus_encode_error(source: LibOpusError, operation: &'static str) -> Error {
    let (category, recoverability, message) = match source.code() {
        ffi::OPUS_BAD_ARG | ffi::OPUS_BUFFER_TOO_SMALL => (
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "libopus rejected the encoder input",
        ),
        ffi::OPUS_ALLOC_FAIL => (
            ErrorCategory::ResourceExhausted,
            Recoverability::Retryable,
            "libopus could not allocate encode state",
        ),
        _ => (
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "libopus failed while encoding",
        ),
    };
    Error::with_source(category, recoverability, message, source).with_context(context(operation))
}

fn context(operation: &'static str) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context(operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(context(operation))
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
        message,
    )
    .with_context(context(operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context(operation))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(context(operation))
}

fn corrupt_source<E>(operation: &'static str, message: &'static str, source: E) -> Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    Error::with_source(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
        message,
        source,
    )
    .with_context(context(operation))
}
