//! Lossless linear PCM decode and encode.
//!
//! Container backends retain the source byte order in the codec identifier. This backend converts
//! signed integer and IEEE float payloads to the canonical little-endian bytes used by decoded
//! [`AudioBlock`] storage, while preserving every sample bit. Packed PCM is interleaved in channel
//! layout order. Canonical planar packets concatenate complete channel planes in layout order.

use std::collections::VecDeque;
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::SampleFormat;
use superi_core::time::{Duration, SampleTime, TimeRounding, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration, BackendTier,
    MediaBackend,
};
use superi_media_io::decode::{DecodeOutput, Decoder, DecoderConfig};
use superi_media_io::demux::{
    BackendId, CodecId, MediaSource, Packet, PacketTiming, SourceRequest, StreamKind,
};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};

/// Stable linear PCM packet representations supported by the default backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PcmEncoding {
    /// Superi's canonical decoded byte layout, including packed or concatenated planar storage.
    Canonical,
    /// Unsigned 8-bit interleaved PCM.
    U8,
    /// Signed 16-bit little-endian interleaved PCM.
    I16LittleEndian,
    /// Signed 16-bit big-endian interleaved PCM.
    I16BigEndian,
    /// Signed 24-bit little-endian interleaved PCM.
    I24LittleEndian,
    /// Signed 24-bit big-endian interleaved PCM.
    I24BigEndian,
    /// Signed 32-bit little-endian interleaved PCM.
    I32LittleEndian,
    /// Signed 32-bit big-endian interleaved PCM.
    I32BigEndian,
    /// IEEE binary32 little-endian interleaved PCM.
    F32LittleEndian,
    /// IEEE binary32 big-endian interleaved PCM.
    F32BigEndian,
    /// IEEE binary64 little-endian interleaved PCM.
    F64LittleEndian,
    /// IEEE binary64 big-endian interleaved PCM.
    F64BigEndian,
}

impl PcmEncoding {
    /// Every PCM representation supported by this backend.
    pub const ALL: &'static [Self] = &[
        Self::Canonical,
        Self::U8,
        Self::I16LittleEndian,
        Self::I16BigEndian,
        Self::I24LittleEndian,
        Self::I24BigEndian,
        Self::I32LittleEndian,
        Self::I32BigEndian,
        Self::F32LittleEndian,
        Self::F32BigEndian,
        Self::F64LittleEndian,
        Self::F64BigEndian,
    ];

    /// Returns the stable codec identifier used by media streams and backend selection.
    #[must_use]
    pub fn codec_id(self) -> CodecId {
        CodecId::new(self.code()).expect("static PCM codec identifiers are valid")
    }

    /// Returns the packed decoded sample format implied by an explicit representation.
    ///
    /// Canonical PCM carries its packed or planar sample format in [`DecoderConfig`].
    #[must_use]
    pub const fn sample_format(self) -> Option<SampleFormat> {
        match self {
            Self::Canonical => None,
            Self::U8 => Some(SampleFormat::U8),
            Self::I16LittleEndian | Self::I16BigEndian => Some(SampleFormat::I16),
            Self::I24LittleEndian | Self::I24BigEndian => Some(SampleFormat::I24),
            Self::I32LittleEndian | Self::I32BigEndian => Some(SampleFormat::I32),
            Self::F32LittleEndian | Self::F32BigEndian => Some(SampleFormat::F32),
            Self::F64LittleEndian | Self::F64BigEndian => Some(SampleFormat::F64),
        }
    }

    fn code(self) -> &'static str {
        match self {
            Self::Canonical => "pcm",
            Self::U8 => "pcm_u8",
            Self::I16LittleEndian => "pcm_s16le",
            Self::I16BigEndian => "pcm_s16be",
            Self::I24LittleEndian => "pcm_s24le",
            Self::I24BigEndian => "pcm_s24be",
            Self::I32LittleEndian => "pcm_s32le",
            Self::I32BigEndian => "pcm_s32be",
            Self::F32LittleEndian => "pcm_f32le",
            Self::F32BigEndian => "pcm_f32be",
            Self::F64LittleEndian => "pcm_f64le",
            Self::F64BigEndian => "pcm_f64be",
        }
    }

    fn from_codec(codec: &CodecId) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|encoding| encoding.code() == codec.as_str())
    }

    fn is_big_endian(self) -> bool {
        matches!(
            self,
            Self::I16BigEndian
                | Self::I24BigEndian
                | Self::I32BigEndian
                | Self::F32BigEndian
                | Self::F64BigEndian
        )
    }

    fn accepts(self, format: SampleFormat) -> bool {
        self.sample_format()
            .map_or(true, |expected| base_sample_format(format) == expected)
    }
}

/// Default in-tree PCM backend.
pub struct PcmBackend {
    descriptor: BackendDescriptor,
}

impl PcmBackend {
    /// Creates the backend with its stable identity.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(BackendId::new("rust-pcm")?, "Rust linear PCM")?,
        })
    }

    /// Builds the primary registration and all supported decode and encode capabilities.
    pub fn registration() -> Result<BackendRegistration> {
        let capabilities = PcmEncoding::ALL.iter().copied().flat_map(|encoding| {
            let codec = encoding.codec_id();
            [
                BackendCapability::Decode(codec.clone()),
                BackendCapability::Encode(codec),
            ]
        });
        BackendRegistration::new(
            Arc::new(Self::new()?),
            BackendCapabilities::new(capabilities),
            100,
            BackendTier::Primary,
        )
    }
}

impl MediaBackend for PcmBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn open_source(&self, _request: &SourceRequest) -> Result<Box<dyn MediaSource>> {
        Err(unsupported(
            "open_pcm_source",
            "the PCM codec backend does not open media containers",
        ))
    }

    fn create_decoder(&self, config: &DecoderConfig) -> Result<Box<dyn Decoder>> {
        Ok(Box::new(PcmDecoder::new(config.clone())?))
    }

    fn create_encoder(&self, config: &EncoderConfig) -> Result<Box<dyn Encoder>> {
        Ok(Box::new(PcmEncoder::new(config.clone())?))
    }
}

struct PcmDecoder {
    config: DecoderConfig,
    encoding: PcmEncoding,
    format: AudioFormat,
    output: VecDeque<AudioBlock>,
    next_sample: Option<i64>,
    flushed: bool,
}

impl PcmDecoder {
    fn new(config: DecoderConfig) -> Result<Self> {
        if config.stream().kind() != StreamKind::Audio {
            return Err(invalid(
                "create_pcm_decoder",
                "PCM decoding requires an audio stream",
            ));
        }
        let encoding = PcmEncoding::from_codec(config.stream().codec()).ok_or_else(|| {
            unsupported(
                "create_pcm_decoder",
                "the requested codec is not a supported PCM representation",
            )
        })?;
        let format = config.audio_format().cloned().ok_or_else(|| {
            invalid(
                "create_pcm_decoder",
                "PCM decoding requires an explicit audio format",
            )
        })?;
        if !encoding.accepts(format.sample_format()) {
            return Err(invalid(
                "create_pcm_decoder",
                "the PCM codec identifier and decoded sample format disagree",
            ));
        }
        Ok(Self {
            config,
            encoding,
            format,
            output: VecDeque::new(),
            next_sample: None,
            flushed: false,
        })
    }

    fn decode_packet(&self, packet: &Packet) -> Result<(AudioBlock, i64)> {
        if packet.stream_id() != self.config.stream().id() {
            return Err(conflict(
                "decode_pcm_packet",
                "PCM packet belongs to a different stream",
            ));
        }
        if packet.timing().timebase() != self.config.stream().timebase() {
            return Err(corrupt(
                "decode_pcm_packet",
                "PCM packet timebase does not match its stream",
            ));
        }

        let channel_count = self.format.channel_layout().len();
        let bytes_per_sample = usize::from(self.format.sample_format().bytes_per_sample());
        let bytes_per_frame = channel_count
            .checked_mul(bytes_per_sample)
            .ok_or_else(|| corrupt("decode_pcm_packet", "PCM frame size overflowed"))?;
        if packet.data().len() % bytes_per_frame != 0 {
            return Err(corrupt(
                "decode_pcm_packet",
                "PCM packet ends inside a sample frame",
            ));
        }
        let frame_count = u64::try_from(packet.data().len() / bytes_per_frame)
            .map_err(|_| corrupt("decode_pcm_packet", "PCM frame count overflowed"))?;
        let expected_duration = Duration::from_samples(frame_count, self.format.sample_rate())?;
        if packet
            .timing()
            .duration()
            .is_some_and(|duration| duration != expected_duration)
        {
            return Err(corrupt(
                "decode_pcm_packet",
                "PCM packet duration does not match its sample count",
            ));
        }

        let timestamp = self.packet_timestamp(packet)?;
        let planes = decode_planes(packet.data(), frame_count, &self.format, self.encoding)?;
        let mut block = AudioBlock::new(self.format.clone(), timestamp, frame_count, planes)?;
        for (key, value) in packet.metadata().iter() {
            block = block.with_metadata(key, value.clone())?;
        }
        let sample_count = i64::try_from(frame_count)
            .map_err(|_| corrupt("decode_pcm_packet", "PCM sample cursor overflowed"))?;
        let next_sample = timestamp
            .sample()
            .checked_add(sample_count)
            .ok_or_else(|| corrupt("decode_pcm_packet", "PCM sample cursor overflowed"))?;
        Ok((block, next_sample))
    }

    fn packet_timestamp(&self, packet: &Packet) -> Result<SampleTime> {
        let Some(presentation) = packet.timing().presentation_time() else {
            return SampleTime::new(self.next_sample.unwrap_or(0), self.format.sample_rate());
        };
        let target = Timebase::integer(self.format.sample_rate())?;
        let converted = presentation.checked_rescale(target, TimeRounding::TowardZero)?;
        if converted != presentation {
            return Err(corrupt(
                "decode_pcm_packet",
                "PCM presentation time is not on an exact sample boundary",
            ));
        }
        SampleTime::new(converted.value(), self.format.sample_rate())
    }
}

impl Decoder for PcmDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet) -> Result<()> {
        if self.flushed {
            return Err(conflict(
                "send_pcm_packet",
                "cannot send PCM packets after flush without reset",
            ));
        }
        let (block, next_sample) = self.decode_packet(&packet)?;
        self.next_sample = Some(next_sample);
        self.output.push_back(block);
        Ok(())
    }

    fn receive(&mut self) -> Result<DecodeOutput> {
        if let Some(block) = self.output.pop_front() {
            return Ok(DecodeOutput::Audio(block));
        }
        if self.flushed {
            Ok(DecodeOutput::EndOfStream)
        } else {
            Ok(DecodeOutput::NeedInput)
        }
    }

    fn flush(&mut self) -> Result<()> {
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.output.clear();
        self.next_sample = None;
        self.flushed = false;
        Ok(())
    }
}

struct PcmEncoder {
    config: EncoderConfig,
    encoding: PcmEncoding,
    format: AudioFormat,
    output: VecDeque<Packet>,
    flushed: bool,
}

impl PcmEncoder {
    fn new(config: EncoderConfig) -> Result<Self> {
        let encoding = PcmEncoding::from_codec(config.codec()).ok_or_else(|| {
            unsupported(
                "create_pcm_encoder",
                "the requested codec is not a supported PCM representation",
            )
        })?;
        let EncoderMediaFormat::Audio(format) = config.media_format() else {
            return Err(invalid(
                "create_pcm_encoder",
                "PCM encoding requires an audio format",
            ));
        };
        if !encoding.accepts(format.sample_format()) {
            return Err(invalid(
                "create_pcm_encoder",
                "the PCM codec identifier and input sample format disagree",
            ));
        }
        let format = format.clone();
        Ok(Self {
            config,
            encoding,
            format,
            output: VecDeque::new(),
            flushed: false,
        })
    }

    fn encode_block(&self, block: AudioBlock) -> Result<Packet> {
        if block.format() != &self.format {
            return Err(invalid(
                "encode_pcm_block",
                "PCM audio block format does not match the encoder configuration",
            ));
        }
        let data = encode_planes(&block, self.encoding)?;
        let timestamp = block.timestamp().sample();
        let timing = PacketTiming::new(
            self.config.timebase(),
            Some(timestamp),
            Some(timestamp),
            Some(block.frame_count()),
        )?;
        let mut packet =
            Packet::new(self.config.stream_id(), Arc::from(data), timing).with_keyframe(true);
        for (key, value) in block.metadata().iter() {
            packet.metadata_mut().insert(key, value.clone())?;
        }
        Ok(packet)
    }
}

impl Encoder for PcmEncoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput) -> Result<()> {
        if self.flushed {
            return Err(conflict(
                "send_pcm_block",
                "cannot send PCM audio after flush without reset",
            ));
        }
        let EncodeInput::Audio(block) = input else {
            return Err(invalid(
                "send_pcm_block",
                "PCM encoders accept only audio blocks",
            ));
        };
        let packet = self.encode_block(block)?;
        self.output.push_back(packet);
        Ok(())
    }

    fn receive(&mut self) -> Result<EncodeOutput> {
        if let Some(packet) = self.output.pop_front() {
            return Ok(EncodeOutput::Packet(packet));
        }
        if self.flushed {
            Ok(EncodeOutput::EndOfStream)
        } else {
            Ok(EncodeOutput::NeedInput)
        }
    }

    fn flush(&mut self) -> Result<()> {
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.output.clear();
        self.flushed = false;
        Ok(())
    }
}

fn decode_planes(
    data: &[u8],
    frame_count: u64,
    format: &AudioFormat,
    encoding: PcmEncoding,
) -> Result<Vec<AudioPlane>> {
    let channels = format.channel_layout().len();
    let bytes_per_sample = usize::from(format.sample_format().bytes_per_sample());
    let frames = usize::try_from(frame_count)
        .map_err(|_| corrupt("decode_pcm_packet", "PCM frame count overflowed"))?;
    if encoding == PcmEncoding::Canonical {
        if format.sample_format().is_planar() {
            let bytes_per_plane = frames
                .checked_mul(bytes_per_sample)
                .ok_or_else(|| corrupt("decode_pcm_packet", "PCM plane size overflowed"))?;
            if bytes_per_plane == 0 {
                return Ok((0..channels)
                    .map(|_| AudioPlane::new(Arc::from([])))
                    .collect());
            }
            return Ok(data
                .chunks_exact(bytes_per_plane)
                .map(|plane| AudioPlane::new(Arc::from(plane)))
                .collect());
        }
        return Ok(vec![AudioPlane::new(Arc::from(data))]);
    }

    if format.sample_format().is_planar() {
        let bytes_per_plane = frames
            .checked_mul(bytes_per_sample)
            .ok_or_else(|| corrupt("decode_pcm_packet", "PCM plane size overflowed"))?;
        let mut planes = (0..channels)
            .map(|_| Vec::with_capacity(bytes_per_plane))
            .collect::<Vec<_>>();
        for frame in 0..frames {
            for (channel, plane) in planes.iter_mut().enumerate() {
                let offset = (frame * channels + channel) * bytes_per_sample;
                append_canonical_sample(plane, &data[offset..offset + bytes_per_sample], encoding);
            }
        }
        Ok(planes
            .into_iter()
            .map(|plane| AudioPlane::new(Arc::from(plane)))
            .collect())
    } else {
        let mut bytes = Vec::with_capacity(data.len());
        for sample in data.chunks_exact(bytes_per_sample) {
            append_canonical_sample(&mut bytes, sample, encoding);
        }
        Ok(vec![AudioPlane::new(Arc::from(bytes))])
    }
}

fn encode_planes(block: &AudioBlock, encoding: PcmEncoding) -> Result<Vec<u8>> {
    if encoding == PcmEncoding::Canonical {
        return Ok(block
            .planes()
            .iter()
            .flat_map(|plane| plane.bytes().iter().copied())
            .collect());
    }

    let format = block.format();
    let channels = format.channel_layout().len();
    let bytes_per_sample = usize::from(format.sample_format().bytes_per_sample());
    let frames = usize::try_from(block.frame_count())
        .map_err(|_| invalid("encode_pcm_block", "PCM frame count overflowed"))?;
    let capacity = frames
        .checked_mul(channels)
        .and_then(|value| value.checked_mul(bytes_per_sample))
        .ok_or_else(|| invalid("encode_pcm_block", "PCM packet size overflowed"))?;
    let mut data = Vec::with_capacity(capacity);
    for frame in 0..frames {
        for channel in 0..channels {
            let (plane, offset) = if format.sample_format().is_planar() {
                (&block.planes()[channel], frame * bytes_per_sample)
            } else {
                (
                    &block.planes()[0],
                    (frame * channels + channel) * bytes_per_sample,
                )
            };
            append_encoded_sample(
                &mut data,
                &plane.bytes()[offset..offset + bytes_per_sample],
                encoding,
            );
        }
    }
    Ok(data)
}

fn append_canonical_sample(output: &mut Vec<u8>, encoded: &[u8], encoding: PcmEncoding) {
    if encoding.is_big_endian() {
        output.extend(encoded.iter().rev());
    } else {
        output.extend_from_slice(encoded);
    }
}

fn append_encoded_sample(output: &mut Vec<u8>, canonical: &[u8], encoding: PcmEncoding) {
    if encoding.is_big_endian() {
        output.extend(canonical.iter().rev());
    } else {
        output.extend_from_slice(canonical);
    }
}

fn base_sample_format(format: SampleFormat) -> SampleFormat {
    match format {
        SampleFormat::U8 | SampleFormat::U8Planar => SampleFormat::U8,
        SampleFormat::I16 | SampleFormat::I16Planar => SampleFormat::I16,
        SampleFormat::I24 | SampleFormat::I24Planar => SampleFormat::I24,
        SampleFormat::I32 | SampleFormat::I32Planar => SampleFormat::I32,
        SampleFormat::F32 | SampleFormat::F32Planar => SampleFormat::F32,
        SampleFormat::F64 | SampleFormat::F64Planar => SampleFormat::F64,
        _ => format,
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.pcm", operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.pcm", operation))
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.pcm", operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.pcm", operation))
}
