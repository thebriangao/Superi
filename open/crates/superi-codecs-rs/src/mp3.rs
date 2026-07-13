//! MPEG Audio Layer III decode and encode through the permissive Rust backend.
//!
//! MP3 packets contain one complete MPEG audio frame. Decoded storage is signed 16-bit mono or
//! stereo, either packed or planar. The encoder retains input sample time and metadata while the
//! underlying codec buffers its complete output until flush.

use std::collections::VecDeque;
use std::sync::Arc;

use oxideav_core::{
    AudioFrame as OxideAudioFrame, CodecId as OxideCodecId,
    CodecParameters as OxideCodecParameters, Decoder as OxideDecoder, Encoder as OxideEncoder,
    Error as OxideError, Frame as OxideFrame, Packet as OxidePacket,
    SampleFormat as OxideSampleFormat, TimeBase as OxideTimeBase,
};
use oxideav_mp3::{make_decoder, make_encoder, parse_header, parse_side_info, Layer};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{ChannelLayout, SampleFormat};
use superi_core::time::{SampleTime, TimeRounding, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration, BackendTier,
    CodecCapability, CodecOperation, HardwareAcceleration, MediaBackend,
};
use superi_media_io::decode::{DecodeOutput, Decoder, DecoderConfig};
use superi_media_io::demux::{
    BackendId, CodecId, MediaMetadata, MediaSource, Packet, PacketTiming, SourceProbe,
    SourceProbeResult, SourceRequest, StreamKind,
};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::OperationContext;

const DEFAULT_BIT_RATE: u64 = 128_000;
const SUPPORTED_SAMPLE_RATES: &[u32] = &[
    8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000,
];

/// Default permissive MP3 backend.
pub struct Mp3Backend {
    descriptor: BackendDescriptor,
}

impl Mp3Backend {
    /// Returns the stable codec identifier used by streams and backend selection.
    #[must_use]
    pub fn codec_id() -> CodecId {
        CodecId::new("mp3").expect("static MP3 codec identifier is valid")
    }

    /// Creates the backend with its stable identity.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(
                BackendId::new("rust-mp3")?,
                "Rust MPEG Audio Layer III",
            )?,
        })
    }

    /// Builds the primary decode and encode registration.
    pub fn registration() -> Result<BackendRegistration> {
        let codec = Self::codec_id();
        let codec_capabilities = [CodecOperation::Decode, CodecOperation::Encode]
            .into_iter()
            .map(|operation| {
                CodecCapability::new(operation, codec.clone())
                    .with_profiles_not_applicable()
                    .with_levels_not_applicable()
                    .with_bit_depths([16])
                    .map(CodecCapability::with_chroma_sampling_not_applicable)
            })
            .collect::<Result<Vec<_>>>()?;
        BackendRegistration::new(
            Arc::new(Self::new()?),
            BackendCapabilities::new([
                BackendCapability::Decode(codec.clone()),
                BackendCapability::Encode(codec),
            ])
            .with_hardware_acceleration(HardwareAcceleration::Software)
            .with_codec_capabilities(codec_capabilities)?,
            100,
            BackendTier::Primary,
        )
    }
}

impl MediaBackend for Mp3Backend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_mp3_source")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_mp3_source")?;
        Err(unsupported(
            "open_mp3_source",
            "the MP3 codec backend does not open media containers",
        ))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_mp3_decoder")?;
        Ok(Box::new(Mp3Decoder::new(config.clone())?))
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_mp3_encoder")?;
        Ok(Box::new(Mp3Encoder::new(config.clone())?))
    }
}

struct Mp3Decoder {
    config: DecoderConfig,
    format: AudioFormat,
    inner: Box<dyn OxideDecoder>,
    output: VecDeque<AudioBlock>,
    next_sample: Option<i64>,
    flushed: bool,
}

impl Mp3Decoder {
    fn new(config: DecoderConfig) -> Result<Self> {
        if config.stream().kind() != StreamKind::Audio {
            return Err(invalid(
                "create_mp3_decoder",
                "MP3 decoding requires an audio stream",
            ));
        }
        if config.stream().codec() != &Mp3Backend::codec_id() {
            return Err(unsupported(
                "create_mp3_decoder",
                "the requested codec is not MP3",
            ));
        }
        let format = config.audio_format().cloned().ok_or_else(|| {
            invalid(
                "create_mp3_decoder",
                "MP3 decoding requires an explicit audio format",
            )
        })?;
        validate_format(&format, "create_mp3_decoder")?;
        let inner = build_decoder(&format)?;
        Ok(Self {
            config,
            format,
            inner,
            output: VecDeque::new(),
            next_sample: None,
            flushed: false,
        })
    }

    fn decode_packet(&mut self, packet: &Packet) -> Result<Vec<AudioBlock>> {
        if packet.stream_id() != self.config.stream().id() {
            return Err(conflict(
                "decode_mp3_packet",
                "MP3 packet belongs to a different stream",
            ));
        }
        if packet.timing().timebase() != self.config.stream().timebase() {
            return Err(corrupt(
                "decode_mp3_packet",
                "MP3 packet timebase does not match its stream",
            ));
        }
        let header = parse_header(packet.data()).map_err(|source| {
            Error::with_source(
                ErrorCategory::CorruptData,
                Recoverability::Degraded,
                "MP3 packet has an invalid frame header",
                source,
            )
            .with_context(context("decode_mp3_packet"))
        })?;
        if header.layer != Layer::LayerIII {
            return Err(corrupt(
                "decode_mp3_packet",
                "MPEG audio packet is not Layer III",
            ));
        }
        if header.sample_rate_hz != self.format.sample_rate()
            || usize::from(header.channel_count()) != self.format.channel_layout().len()
        {
            return Err(corrupt(
                "decode_mp3_packet",
                "MP3 frame parameters changed or disagree with the configured audio format",
            ));
        }
        if header
            .frame_len()
            .is_some_and(|expected| expected != packet.data().len())
        {
            return Err(corrupt(
                "decode_mp3_packet",
                "MP3 packet does not contain exactly one complete frame",
            ));
        }
        let expected_duration = u64::from(header.samples_per_frame());
        if let Some(duration) = packet.timing().duration() {
            let sample_timebase = Timebase::integer(self.format.sample_rate())?;
            let Ok(duration) = duration.checked_rescale(sample_timebase, TimeRounding::Exact)
            else {
                return Err(corrupt(
                    "decode_mp3_packet",
                    "MP3 packet duration is not on an exact sample boundary",
                ));
            };
            if duration.value() != expected_duration {
                return Err(corrupt(
                    "decode_mp3_packet",
                    "MP3 packet duration does not match its frame header",
                ));
            }
        }

        let timestamp = self.packet_timestamp(packet)?;
        let mut oxide_packet = OxidePacket::new(
            packet.stream_id().value(),
            oxide_timebase(packet.timing().timebase()),
            packet.data().to_vec(),
        );
        oxide_packet.pts = packet.timing().presentation_time().map(|time| time.value());
        oxide_packet.dts = packet.timing().decode_time().map(|time| time.value());
        oxide_packet.duration = Some(i64::from(header.samples_per_frame()));
        oxide_packet.flags.keyframe = packet.is_keyframe();
        self.inner
            .send_packet(&oxide_packet)
            .map_err(|error| map_decode_error(error, "decode_mp3_packet"))?;

        let mut blocks = Vec::new();
        loop {
            match self.inner.receive_frame() {
                Ok(OxideFrame::Audio(frame)) => {
                    let block = self.audio_block(frame, timestamp, packet.metadata())?;
                    self.next_sample = Some(
                        block
                            .timestamp()
                            .sample()
                            .checked_add(i64::try_from(block.frame_count()).map_err(|_| {
                                corrupt("decode_mp3_packet", "MP3 sample cursor overflowed")
                            })?)
                            .ok_or_else(|| {
                                corrupt("decode_mp3_packet", "MP3 sample cursor overflowed")
                            })?,
                    );
                    blocks.push(block);
                }
                Ok(_) => {
                    return Err(internal(
                        "decode_mp3_packet",
                        "MP3 decoder returned non-audio output",
                    ));
                }
                Err(OxideError::NeedMore) => break,
                Err(OxideError::Eof) => {
                    return Err(internal(
                        "decode_mp3_packet",
                        "MP3 decoder ended before flush",
                    ));
                }
                Err(error) => return Err(map_decode_error(error, "decode_mp3_packet")),
            }
        }
        Ok(blocks)
    }

    fn packet_timestamp(&self, packet: &Packet) -> Result<SampleTime> {
        let Some(presentation) = packet.timing().presentation_time() else {
            return SampleTime::new(self.next_sample.unwrap_or(0), self.format.sample_rate());
        };
        let target = Timebase::integer(self.format.sample_rate())?;
        let converted = presentation
            .checked_rescale(target, TimeRounding::Exact)
            .map_err(|_| {
                corrupt(
                    "decode_mp3_packet",
                    "MP3 presentation time is not on an exact sample boundary",
                )
            })?;
        SampleTime::new(converted.value(), self.format.sample_rate())
    }

    fn audio_block(
        &self,
        frame: OxideAudioFrame,
        timestamp: SampleTime,
        metadata: &MediaMetadata,
    ) -> Result<AudioBlock> {
        let frame_count = u64::from(frame.samples);
        let channels = self.format.channel_layout().len();
        if frame.data.len() != channels {
            return Err(corrupt(
                "decode_mp3_packet",
                "MP3 decoder returned an unexpected plane count",
            ));
        }
        let expected_plane_bytes = usize::try_from(frame_count)
            .ok()
            .and_then(|frames| frames.checked_mul(2))
            .ok_or_else(|| corrupt("decode_mp3_packet", "MP3 audio plane size overflowed"))?;
        if frame
            .data
            .iter()
            .any(|plane| plane.len() != expected_plane_bytes)
        {
            return Err(corrupt(
                "decode_mp3_packet",
                "MP3 decoder returned an invalid audio plane size",
            ));
        }
        let planes = if self.format.sample_format().is_planar() {
            frame
                .data
                .into_iter()
                .map(|bytes| AudioPlane::new(Arc::from(bytes)))
                .collect()
        } else {
            let frames = usize::try_from(frame_count)
                .map_err(|_| corrupt("decode_mp3_packet", "MP3 frame count overflowed"))?;
            let mut packed = Vec::with_capacity(
                frames
                    .checked_mul(channels)
                    .and_then(|value| value.checked_mul(2))
                    .ok_or_else(|| {
                        corrupt("decode_mp3_packet", "MP3 packed audio size overflowed")
                    })?,
            );
            for sample in 0..frames {
                for plane in &frame.data {
                    packed.extend_from_slice(&plane[sample * 2..sample * 2 + 2]);
                }
            }
            vec![AudioPlane::new(Arc::from(packed))]
        };
        let mut block = AudioBlock::new(self.format.clone(), timestamp, frame_count, planes)?;
        for (key, value) in metadata.iter() {
            block = block.with_metadata(key, value.clone())?;
        }
        Ok(block)
    }
}

impl Decoder for Mp3Decoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("send_mp3_packet")?;
        if self.flushed {
            return Err(conflict(
                "send_mp3_packet",
                "cannot send MP3 packets after flush without reset",
            ));
        }
        let decoded = self.decode_packet(&packet)?;
        self.output.extend(decoded);
        Ok(())
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("receive_mp3_audio")?;
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
        operation.check("flush_mp3_decoder")?;
        self.inner
            .flush()
            .map_err(|error| map_decode_error(error, "flush_mp3_decoder"))?;
        match self.inner.receive_frame() {
            Err(OxideError::Eof | OxideError::NeedMore) => {}
            Ok(OxideFrame::Audio(_)) => {
                return Err(internal(
                    "flush_mp3_decoder",
                    "MP3 decoder produced output without packet timing",
                ));
            }
            Ok(_) => {
                return Err(internal(
                    "flush_mp3_decoder",
                    "MP3 decoder returned non-audio output",
                ));
            }
            Err(error) => return Err(map_decode_error(error, "flush_mp3_decoder")),
        }
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_mp3_decoder")?;
        self.inner
            .reset()
            .map_err(|error| map_decode_error(error, "reset_mp3_decoder"))?;
        self.output.clear();
        self.next_sample = None;
        self.flushed = false;
        Ok(())
    }
}

struct MetadataSpan {
    start: i64,
    end: i64,
    metadata: MediaMetadata,
}

struct Mp3Encoder {
    config: EncoderConfig,
    format: AudioFormat,
    inner: Box<dyn OxideEncoder>,
    origin_sample: Option<i64>,
    next_input_sample: Option<i64>,
    next_packet_sample: i64,
    metadata: Vec<MetadataSpan>,
    flushed: bool,
}

impl Mp3Encoder {
    fn new(config: EncoderConfig) -> Result<Self> {
        if config.codec() != &Mp3Backend::codec_id() {
            return Err(unsupported(
                "create_mp3_encoder",
                "the requested codec is not MP3",
            ));
        }
        let EncoderMediaFormat::Audio(format) = config.media_format() else {
            return Err(invalid(
                "create_mp3_encoder",
                "MP3 encoding requires an audio format",
            ));
        };
        validate_format(format, "create_mp3_encoder")?;
        let format = format.clone();
        let inner = build_encoder(&format)?;
        Ok(Self {
            config,
            format,
            inner,
            origin_sample: None,
            next_input_sample: None,
            next_packet_sample: 0,
            metadata: Vec::new(),
            flushed: false,
        })
    }

    fn send_block(&mut self, block: AudioBlock) -> Result<()> {
        if block.format() != &self.format {
            return Err(invalid(
                "encode_mp3_block",
                "MP3 audio block format does not match the encoder configuration",
            ));
        }
        let start = block.timestamp().sample();
        if self
            .next_input_sample
            .is_some_and(|expected| expected != start)
        {
            return Err(conflict(
                "encode_mp3_block",
                "MP3 audio blocks must be contiguous in sample time",
            ));
        }
        let frame_count = i64::try_from(block.frame_count())
            .map_err(|_| invalid("encode_mp3_block", "MP3 frame count overflowed"))?;
        let end = start
            .checked_add(frame_count)
            .ok_or_else(|| invalid("encode_mp3_block", "MP3 sample cursor overflowed"))?;
        let origin = *self.origin_sample.get_or_insert(start);
        let relative_start = start
            .checked_sub(origin)
            .ok_or_else(|| invalid("encode_mp3_block", "MP3 relative timestamp overflowed"))?;
        let relative_end = end
            .checked_sub(origin)
            .ok_or_else(|| invalid("encode_mp3_block", "MP3 relative timestamp overflowed"))?;
        if frame_count > 0 {
            self.metadata.push(MetadataSpan {
                start: relative_start,
                end: relative_end,
                metadata: block.metadata().clone(),
            });
        }
        let samples = u32::try_from(block.frame_count()).map_err(|_| {
            invalid(
                "encode_mp3_block",
                "MP3 block is too large for the codec frame interface",
            )
        })?;
        let frame = OxideFrame::Audio(OxideAudioFrame {
            samples,
            pts: Some(relative_start),
            data: vec![interleave_i16(&block)?],
        });
        self.inner
            .send_frame(&frame)
            .map_err(|error| map_encode_error(error, "encode_mp3_block"))?;
        self.next_input_sample = Some(end);
        Ok(())
    }

    fn packet_from_oxide(&mut self, packet: OxidePacket) -> Result<Packet> {
        let expected_timebase = OxideTimeBase::from_rate(self.format.sample_rate());
        if packet.time_base != expected_timebase {
            return Err(internal(
                "receive_mp3_packet",
                "MP3 encoder returned an unexpected packet timebase",
            ));
        }
        let relative_pts = packet.pts.unwrap_or(self.next_packet_sample);
        let relative_dts = packet.dts.unwrap_or(relative_pts);
        if relative_pts != self.next_packet_sample || relative_dts != relative_pts {
            return Err(internal(
                "receive_mp3_packet",
                "MP3 encoder returned discontinuous packet timestamps",
            ));
        }
        let duration = packet.duration.ok_or_else(|| {
            internal(
                "receive_mp3_packet",
                "MP3 encoder returned a packet without duration",
            )
        })?;
        let duration = u64::try_from(duration).map_err(|_| {
            internal(
                "receive_mp3_packet",
                "MP3 encoder returned a negative packet duration",
            )
        })?;
        self.next_packet_sample =
            relative_pts
                .checked_add(i64::try_from(duration).map_err(|_| {
                    internal("receive_mp3_packet", "MP3 packet duration overflowed")
                })?)
                .ok_or_else(|| internal("receive_mp3_packet", "MP3 packet timestamp overflowed"))?;
        let origin = self.origin_sample.unwrap_or(0);
        let pts = origin
            .checked_add(relative_pts)
            .ok_or_else(|| internal("receive_mp3_packet", "MP3 packet timestamp overflowed"))?;
        let dts = origin
            .checked_add(relative_dts)
            .ok_or_else(|| internal("receive_mp3_packet", "MP3 packet timestamp overflowed"))?;
        let timing =
            PacketTiming::new(self.config.timebase(), Some(pts), Some(dts), Some(duration))?;
        let keyframe = encoded_keyframe(&packet.data)?;
        let mut output = Packet::new(self.config.stream_id(), Arc::from(packet.data), timing)
            .with_keyframe(keyframe);
        if let Some(metadata) = self.metadata_for(relative_pts) {
            for (key, value) in metadata.iter() {
                output.metadata_mut().insert(key, value.clone())?;
            }
        }
        Ok(output)
    }

    fn metadata_for(&self, sample: i64) -> Option<&MediaMetadata> {
        self.metadata
            .iter()
            .find(|span| sample >= span.start && sample < span.end)
            .or_else(|| self.metadata.last())
            .map(|span| &span.metadata)
    }

    fn rebuild(&mut self) -> Result<()> {
        self.inner = build_encoder(&self.format)?;
        self.origin_sample = None;
        self.next_input_sample = None;
        self.next_packet_sample = 0;
        self.metadata.clear();
        self.flushed = false;
        Ok(())
    }
}

impl Encoder for Mp3Encoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("send_mp3_audio")?;
        if self.flushed {
            return Err(conflict(
                "send_mp3_audio",
                "cannot send MP3 audio after flush without reset",
            ));
        }
        let EncodeInput::Audio(block) = input else {
            return Err(invalid(
                "send_mp3_audio",
                "MP3 encoders accept only audio blocks",
            ));
        };
        self.send_block(block)
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("receive_mp3_packet")?;
        match self.inner.receive_packet() {
            Ok(packet) => Ok(EncodeOutput::Packet(self.packet_from_oxide(packet)?)),
            Err(OxideError::NeedMore) if !self.flushed => Ok(EncodeOutput::NeedInput),
            Err(OxideError::Eof) if self.flushed => Ok(EncodeOutput::EndOfStream),
            Err(error) => Err(map_encode_error(error, "receive_mp3_packet")),
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_mp3_encoder")?;
        self.inner
            .flush()
            .map_err(|error| map_encode_error(error, "flush_mp3_encoder"))?;
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_mp3_encoder")?;
        self.rebuild()
    }
}

fn build_decoder(format: &AudioFormat) -> Result<Box<dyn OxideDecoder>> {
    let params = oxide_params(format);
    make_decoder(&params).map_err(|error| map_config_error(error, "create_mp3_decoder"))
}

fn build_encoder(format: &AudioFormat) -> Result<Box<dyn OxideEncoder>> {
    let mut params = oxide_params(format);
    params.bit_rate = Some(DEFAULT_BIT_RATE);
    make_encoder(&params).map_err(|error| map_config_error(error, "create_mp3_encoder"))
}

fn oxide_params(format: &AudioFormat) -> OxideCodecParameters {
    let mut params = OxideCodecParameters::audio(OxideCodecId::new("mp3"));
    params.sample_rate = Some(format.sample_rate());
    params.channels = Some(format.channel_layout().len() as u16);
    params.sample_format = Some(OxideSampleFormat::S16);
    params
}

fn oxide_timebase(timebase: Timebase) -> OxideTimeBase {
    OxideTimeBase::new(
        i64::from(timebase.denominator()),
        i64::from(timebase.numerator()),
    )
}

fn validate_format(format: &AudioFormat, operation: &'static str) -> Result<()> {
    if !SUPPORTED_SAMPLE_RATES.contains(&format.sample_rate()) {
        return Err(unsupported(
            operation,
            "MP3 supports 8, 11.025, 12, 16, 22.05, 24, 32, 44.1, or 48 kHz audio",
        ));
    }
    if !matches!(
        format.sample_format(),
        SampleFormat::I16 | SampleFormat::I16Planar
    ) {
        return Err(unsupported(
            operation,
            "MP3 accepts signed 16-bit packed or planar audio",
        ));
    }
    let expected_layout = match format.channel_layout().len() {
        1 => ChannelLayout::mono(),
        2 => ChannelLayout::stereo(),
        _ => {
            return Err(unsupported(
                operation,
                "MP3 supports only mono or stereo audio",
            ));
        }
    };
    if format.channel_layout() != &expected_layout {
        return Err(unsupported(
            operation,
            "MP3 supports canonical mono or left-right stereo channel order",
        ));
    }
    Ok(())
}

fn interleave_i16(block: &AudioBlock) -> Result<Vec<u8>> {
    let frames = usize::try_from(block.frame_count())
        .map_err(|_| invalid("encode_mp3_block", "MP3 frame count overflowed"))?;
    let channels = block.format().channel_layout().len();
    let capacity = frames
        .checked_mul(channels)
        .and_then(|value| value.checked_mul(2))
        .ok_or_else(|| invalid("encode_mp3_block", "MP3 input size overflowed"))?;
    if !block.format().sample_format().is_planar() {
        return Ok(block.planes()[0].bytes().to_vec());
    }
    let mut output = Vec::with_capacity(capacity);
    for frame in 0..frames {
        for plane in block.planes() {
            output.extend_from_slice(&plane.bytes()[frame * 2..frame * 2 + 2]);
        }
    }
    Ok(output)
}

fn encoded_keyframe(data: &[u8]) -> Result<bool> {
    let header = parse_header(data).map_err(|source| {
        Error::with_source(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "MP3 encoder returned an invalid frame header",
            source,
        )
        .with_context(context("receive_mp3_packet"))
    })?;
    let side_info_start = 4 + usize::from(header.crc_protected) * 2;
    let side_info = parse_side_info(&header, data.get(side_info_start..).unwrap_or_default())
        .map_err(|source| {
            Error::with_source(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "MP3 encoder returned invalid side information",
                source,
            )
            .with_context(context("receive_mp3_packet"))
        })?;
    Ok(side_info.main_data_begin == 0)
}

fn map_config_error(error: OxideError, operation: &'static str) -> Error {
    match error {
        OxideError::Unsupported(_) | OxideError::CodecNotFound(_) => sourced(
            ErrorCategory::Unsupported,
            Recoverability::Degraded,
            operation,
            error,
        ),
        OxideError::InvalidData(_) | OxideError::Other(_) => sourced(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            error,
        ),
        other => map_unexpected_error(other, operation),
    }
}

fn map_decode_error(error: OxideError, operation: &'static str) -> Error {
    match error {
        OxideError::Unsupported(_) | OxideError::CodecNotFound(_) => sourced(
            ErrorCategory::Unsupported,
            Recoverability::Degraded,
            operation,
            error,
        ),
        OxideError::InvalidData(_) | OxideError::Other(_) => sourced(
            ErrorCategory::CorruptData,
            Recoverability::Degraded,
            operation,
            error,
        ),
        other => map_unexpected_error(other, operation),
    }
}

fn map_encode_error(error: OxideError, operation: &'static str) -> Error {
    match error {
        OxideError::Unsupported(_) | OxideError::CodecNotFound(_) => sourced(
            ErrorCategory::Unsupported,
            Recoverability::Degraded,
            operation,
            error,
        ),
        OxideError::InvalidData(_) | OxideError::Other(_) => sourced(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            error,
        ),
        other => map_unexpected_error(other, operation),
    }
}

fn map_unexpected_error(error: OxideError, operation: &'static str) -> Error {
    match error {
        OxideError::Io(_) => sourced(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            operation,
            error,
        ),
        OxideError::ResourceExhausted(_) => sourced(
            ErrorCategory::ResourceExhausted,
            Recoverability::Degraded,
            operation,
            error,
        ),
        OxideError::FormatNotFound(_) => sourced(
            ErrorCategory::Unsupported,
            Recoverability::Degraded,
            operation,
            error,
        ),
        other => sourced(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            operation,
            other,
        ),
    }
}

fn sourced(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    source: OxideError,
) -> Error {
    Error::with_source(
        category,
        recoverability,
        "MP3 codec operation failed",
        source,
    )
    .with_context(context(operation))
}

fn context(operation: &'static str) -> ErrorContext {
    ErrorContext::new("superi-codecs-rs.mp3", operation)
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
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

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
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

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(context(operation))
}
