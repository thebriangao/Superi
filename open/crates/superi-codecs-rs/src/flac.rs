//! Lossless FLAC decode and encode through the default pure Rust backend.

use std::collections::VecDeque;
use std::io::Cursor;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, MetadataBlockData};
use flacenc::error::Verify;
use flacenc::source::MemSource;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{ChannelLayout, ChannelPosition, SampleFormat};
use superi_core::time::{SampleTime, TimeRounding, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration, BackendTier,
    CodecCapability, CodecOperation, HardwareAcceleration, MediaBackend,
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

const COMPONENT: &str = "superi-codecs-rs.flac";

/// Default pure Rust FLAC backend.
pub struct FlacBackend {
    descriptor: BackendDescriptor,
}

impl FlacBackend {
    /// Creates the backend with stable public identity.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(BackendId::new("rust-flac")?, "Rust FLAC")?,
        })
    }

    /// Returns the stable codec identifier used by containers and selection.
    #[must_use]
    pub fn codec_id() -> CodecId {
        CodecId::new("flac").expect("static FLAC codec identifier is valid")
    }

    /// Builds the deterministic primary decode and encode registration.
    pub fn registration() -> Result<BackendRegistration> {
        let codec = Self::codec_id();
        let codec_capabilities = [CodecOperation::Decode, CodecOperation::Encode]
            .into_iter()
            .map(|operation| {
                CodecCapability::new(operation, codec.clone())
                    .with_profiles_not_applicable()
                    .with_levels_not_applicable()
                    .with_bit_depths([8, 12, 16, 20, 24])
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

impl MediaBackend for FlacBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_flac_source")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_flac_source")?;
        Err(unsupported(
            "open_flac_source",
            "the FLAC codec backend does not open media containers",
        ))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_flac_decoder")?;
        Ok(Box::new(FlacDecoder::new(config.clone())?))
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_flac_encoder")?;
        Ok(Box::new(FlacEncoder::new(config.clone())?))
    }
}

struct FlacDecoder {
    config: DecoderConfig,
    explicit_format: Option<AudioFormat>,
    codec_configuration: Option<Arc<[u8]>>,
    output: VecDeque<AudioBlock>,
    flushed: bool,
}

impl FlacDecoder {
    fn new(config: DecoderConfig) -> Result<Self> {
        if config.stream().kind() != StreamKind::Audio {
            return Err(invalid(
                "create_flac_decoder",
                "FLAC decoding requires an audio stream",
            ));
        }
        if config.stream().codec() != &FlacBackend::codec_id() {
            return Err(unsupported(
                "create_flac_decoder",
                "the requested codec is not FLAC",
            ));
        }
        let codec_configuration = match config.stream().metadata().get("codec.configuration") {
            Some(MetadataValue::Bytes(bytes)) => Some(bytes.clone()),
            Some(_) => {
                return Err(invalid(
                    "create_flac_decoder",
                    "FLAC codec.configuration metadata must contain bytes",
                ));
            }
            None => None,
        };
        Ok(Self {
            explicit_format: config.audio_format().cloned(),
            config,
            codec_configuration,
            output: VecDeque::new(),
            flushed: false,
        })
    }

    fn native_stream(&self, packet: &Packet) -> Result<Vec<u8>> {
        if packet.data().starts_with(b"fLaC") {
            return Ok(packet.data().to_vec());
        }
        let configuration = self.codec_configuration.as_deref().ok_or_else(|| {
            corrupt(
                "decode_flac_packet",
                "a headerless FLAC frame requires codec.configuration metadata",
            )
        })?;
        if !configuration.starts_with(b"fLaC") {
            return Err(corrupt(
                "decode_flac_packet",
                "FLAC codec.configuration does not begin with the native stream marker",
            ));
        }
        let capacity = configuration
            .len()
            .checked_add(packet.data().len())
            .ok_or_else(|| corrupt("decode_flac_packet", "FLAC packet size overflowed"))?;
        let mut native = Vec::new();
        native.try_reserve_exact(capacity).map_err(|_| {
            corrupt(
                "decode_flac_packet",
                "FLAC packet is too large to allocate safely",
            )
        })?;
        native.extend_from_slice(configuration);
        native.extend_from_slice(packet.data());
        Ok(native)
    }

    fn decode_packet(
        &self,
        packet: &Packet,
        operation: &OperationContext,
    ) -> Result<Vec<AudioBlock>> {
        if packet.stream_id() != self.config.stream().id() {
            return Err(conflict(
                "decode_flac_packet",
                "FLAC packet belongs to a different stream",
            ));
        }
        if packet.timing().timebase() != self.config.stream().timebase() {
            return Err(corrupt(
                "decode_flac_packet",
                "FLAC packet timebase does not match its stream",
            ));
        }
        let native = self.native_stream(packet)?;
        let decoded = catch_unwind(AssertUnwindSafe(|| {
            decode_native_flac(&native, self.explicit_format.as_ref(), operation)
        }))
        .map_err(|_| {
            corrupt(
                "decode_flac_packet",
                "FLAC decoder rejected the packet without a recoverable diagnostic",
            )
        })??;
        let sample_timebase = Timebase::integer(decoded.format.sample_rate())?;
        let packet_start = packet
            .timing()
            .presentation_time()
            .map(|presentation| {
                let converted =
                    presentation.checked_rescale(sample_timebase, TimeRounding::TowardZero)?;
                if converted != presentation {
                    return Err(corrupt(
                        "decode_flac_packet",
                        "FLAC presentation time is not on an exact sample boundary",
                    ));
                }
                Ok(converted.value())
            })
            .transpose()?;
        let first_time = decoded.blocks.first().map_or(0, |block| block.time);
        let total_frames = decoded.blocks.iter().try_fold(0_u64, |total, block| {
            total
                .checked_add(block.frame_count)
                .ok_or_else(|| corrupt("decode_flac_packet", "FLAC packet duration overflowed"))
        })?;
        if let Some(duration) = packet.timing().duration() {
            let converted = duration.checked_rescale(sample_timebase, TimeRounding::TowardZero)?;
            if converted != duration || converted.value() != total_frames {
                return Err(corrupt(
                    "decode_flac_packet",
                    "FLAC packet duration does not match its decoded sample count",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "validate_flac_duration")
                        .with_field("packet_frames", converted.value().to_string())
                        .with_field("decoded_frames", total_frames.to_string()),
                ));
            }
        }

        decoded
            .blocks
            .into_iter()
            .map(|raw| {
                let delta = raw.time.checked_sub(first_time).ok_or_else(|| {
                    corrupt(
                        "decode_flac_packet",
                        "FLAC frame timestamps are not monotonic within the packet",
                    )
                })?;
                let timestamp = if let Some(start) = packet_start {
                    start
                        .checked_add(i64::try_from(delta).map_err(|_| {
                            corrupt("decode_flac_packet", "FLAC timestamp overflowed")
                        })?)
                        .ok_or_else(|| corrupt("decode_flac_packet", "FLAC timestamp overflowed"))?
                } else {
                    i64::try_from(raw.time)
                        .map_err(|_| corrupt("decode_flac_packet", "FLAC timestamp overflowed"))?
                };
                let mut block = AudioBlock::new(
                    decoded.format.clone(),
                    SampleTime::new(timestamp, decoded.format.sample_rate())?,
                    raw.frame_count,
                    raw.planes,
                )?;
                for (key, value) in decoded.metadata.iter() {
                    block = block.with_metadata(key, value.clone())?;
                }
                for (key, value) in packet.metadata().iter() {
                    block = block.with_metadata(key, value.clone())?;
                }
                block.with_metadata(
                    "flac.bits-per-sample",
                    MetadataValue::Unsigned(u64::from(decoded.bits_per_sample)),
                )
            })
            .collect()
    }
}

impl Decoder for FlacDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("send_flac_packet")?;
        if self.flushed {
            return Err(conflict(
                "send_flac_packet",
                "cannot send FLAC packets after flush without reset",
            ));
        }
        let blocks = self.decode_packet(&packet, operation)?;
        operation.check("send_flac_packet")?;
        self.output.extend(blocks);
        Ok(())
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("receive_flac_audio")?;
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
        operation.check("flush_flac_decoder")?;
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_flac_decoder")?;
        self.output.clear();
        self.flushed = false;
        Ok(())
    }
}

struct FlacEncoder {
    config: EncoderConfig,
    format: AudioFormat,
    samples: Vec<i32>,
    bits_per_sample: Option<u32>,
    metadata: Option<MediaMetadata>,
    first_timestamp: Option<i64>,
    next_timestamp: Option<i64>,
    output: VecDeque<Packet>,
    flushed: bool,
}

impl FlacEncoder {
    fn new(config: EncoderConfig) -> Result<Self> {
        if config.codec() != &FlacBackend::codec_id() {
            return Err(unsupported(
                "create_flac_encoder",
                "the requested codec is not FLAC",
            ));
        }
        let EncoderMediaFormat::Audio(format) = config.media_format() else {
            return Err(invalid(
                "create_flac_encoder",
                "FLAC encoding requires an audio format",
            ));
        };
        if !matches!(
            format.sample_format(),
            SampleFormat::I16
                | SampleFormat::I16Planar
                | SampleFormat::I24
                | SampleFormat::I24Planar
        ) {
            return Err(unsupported(
                "create_flac_encoder",
                "FLAC encoding requires signed 16-bit or signed 24-bit input containers",
            ));
        }
        let canonical_layout = flac_channel_layout(
            u32::try_from(format.channel_layout().len())
                .map_err(|_| invalid("create_flac_encoder", "FLAC channel count overflowed"))?,
        )?;
        if format.channel_layout() != &canonical_layout {
            return Err(unsupported(
                "create_flac_encoder",
                "FLAC encoding requires canonical FLAC channel order",
            ));
        }
        Ok(Self {
            format: format.clone(),
            config,
            samples: Vec::new(),
            bits_per_sample: None,
            metadata: None,
            first_timestamp: None,
            next_timestamp: None,
            output: VecDeque::new(),
            flushed: false,
        })
    }

    fn append_block(&mut self, block: AudioBlock, operation: &OperationContext) -> Result<()> {
        if block.format() != &self.format {
            return Err(conflict(
                "encode_flac_block",
                "FLAC audio block format does not match the encoder configuration",
            ));
        }
        if self
            .next_timestamp
            .is_some_and(|expected| block.timestamp().sample() != expected)
        {
            return Err(conflict(
                "encode_flac_block",
                "FLAC audio blocks must have contiguous sample timestamps",
            ));
        }
        if self
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata != block.metadata())
        {
            return Err(conflict(
                "encode_flac_block",
                "FLAC stream metadata must remain stable across audio blocks",
            ));
        }
        let bits_per_sample = flac_input_precision(&block)?;
        if self
            .bits_per_sample
            .is_some_and(|precision| precision != bits_per_sample)
        {
            return Err(conflict(
                "encode_flac_block",
                "FLAC sample precision must remain stable across audio blocks",
            ));
        }
        let appended = canonical_flac_samples(&block, bits_per_sample, operation)?;
        self.samples.try_reserve(appended.len()).map_err(|_| {
            invalid(
                "encode_flac_block",
                "FLAC input stream is too large to buffer safely",
            )
        })?;
        let next_timestamp =
            block
                .timestamp()
                .sample()
                .checked_add(i64::try_from(block.frame_count()).map_err(|_| {
                    invalid("encode_flac_block", "FLAC sample timestamp overflowed")
                })?)
                .ok_or_else(|| invalid("encode_flac_block", "FLAC sample timestamp overflowed"))?;
        if self.first_timestamp.is_none() {
            self.first_timestamp = Some(block.timestamp().sample());
            self.metadata = Some(block.metadata().clone());
            self.bits_per_sample = Some(bits_per_sample);
        }
        self.samples.extend_from_slice(&appended);
        self.next_timestamp = Some(next_timestamp);
        Ok(())
    }

    fn finish_packet(&self, operation: &OperationContext) -> Result<Option<Packet>> {
        let Some(bits_per_sample) = self.bits_per_sample else {
            return Ok(None);
        };
        let channels = self.format.channel_layout().len();
        let frame_count = self.samples.len() / channels;
        if frame_count == 0 {
            return Ok(None);
        }
        operation.check("encode_flac_stream")?;
        let metadata = self
            .metadata
            .as_ref()
            .expect("FLAC precision and metadata are initialized together");
        let encoded = catch_unwind(AssertUnwindSafe(|| {
            encode_native_flac(
                &self.samples,
                channels,
                bits_per_sample,
                self.format.sample_rate(),
                metadata,
            )
        }))
        .map_err(|_| {
            corrupt(
                "encode_flac_stream",
                "FLAC encoder failed without a recoverable diagnostic",
            )
        })??;
        operation.check("encode_flac_stream")?;
        let timestamp = self.first_timestamp.unwrap_or(0);
        let timing =
            PacketTiming::new(
                self.config.timebase(),
                Some(timestamp),
                Some(timestamp),
                Some(u64::try_from(frame_count).map_err(|_| {
                    invalid("encode_flac_stream", "FLAC output duration overflowed")
                })?),
            )?;
        let mut packet =
            Packet::new(self.config.stream_id(), Arc::from(encoded), timing).with_keyframe(true);
        for (key, value) in metadata.iter() {
            packet.metadata_mut().insert(key, value.clone())?;
        }
        packet.metadata_mut().insert(
            "flac.bits-per-sample",
            MetadataValue::Unsigned(u64::from(bits_per_sample)),
        )?;
        Ok(Some(packet))
    }
}

impl Encoder for FlacEncoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("send_flac_block")?;
        if self.flushed {
            return Err(conflict(
                "send_flac_block",
                "cannot send FLAC audio after flush without reset",
            ));
        }
        let EncodeInput::Audio(block) = input else {
            return Err(invalid(
                "send_flac_block",
                "FLAC encoders accept only audio blocks",
            ));
        };
        self.append_block(block, operation)
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("receive_flac_packet")?;
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
        operation.check("flush_flac_encoder")?;
        if !self.flushed {
            if let Some(packet) = self.finish_packet(operation)? {
                self.output.push_back(packet);
            }
            self.flushed = true;
        }
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_flac_encoder")?;
        self.samples.clear();
        self.bits_per_sample = None;
        self.metadata = None;
        self.first_timestamp = None;
        self.next_timestamp = None;
        self.output.clear();
        self.flushed = false;
        Ok(())
    }
}

fn flac_input_precision(block: &AudioBlock) -> Result<u32> {
    let default = u32::from(block.format().sample_format().bits_per_sample());
    let bits = match block.metadata().get("flac.bits-per-sample") {
        Some(MetadataValue::Unsigned(bits)) => u32::try_from(*bits).map_err(|_| {
            invalid(
                "encode_flac_block",
                "FLAC bits-per-sample metadata is too large",
            )
        })?,
        Some(_) => {
            return Err(invalid(
                "encode_flac_block",
                "FLAC bits-per-sample metadata must be an unsigned integer",
            ));
        }
        None => default,
    };
    let compatible = match block.format().sample_format() {
        SampleFormat::I16 | SampleFormat::I16Planar => matches!(bits, 8 | 12 | 16),
        SampleFormat::I24 | SampleFormat::I24Planar => matches!(bits, 20 | 24),
        _ => false,
    };
    if !compatible {
        return Err(unsupported(
            "encode_flac_block",
            "FLAC supports 8, 12, 16, 20, or 24 bit precision in matching signed containers",
        ));
    }
    Ok(bits)
}

fn canonical_flac_samples(
    block: &AudioBlock,
    bits_per_sample: u32,
    operation: &OperationContext,
) -> Result<Vec<i32>> {
    let format = block.format().sample_format();
    let channels = block.format().channel_layout().len();
    let frames = usize::try_from(block.frame_count())
        .map_err(|_| invalid("encode_flac_block", "FLAC frame count overflowed"))?;
    let bytes_per_sample = usize::from(format.bytes_per_sample());
    let sample_count = frames
        .checked_mul(channels)
        .ok_or_else(|| invalid("encode_flac_block", "FLAC sample count overflowed"))?;
    let mut samples = Vec::new();
    samples.try_reserve_exact(sample_count).map_err(|_| {
        invalid(
            "encode_flac_block",
            "FLAC sample buffer is too large to allocate safely",
        )
    })?;
    let minimum = -(1_i32 << (bits_per_sample - 1));
    let maximum = (1_i32 << (bits_per_sample - 1)) - 1;
    for frame in 0..frames {
        if frame % 4_096 == 0 {
            operation.check("convert_flac_samples")?;
        }
        for channel in 0..channels {
            let (plane, offset) = if format.is_planar() {
                (&block.planes()[channel], frame * bytes_per_sample)
            } else {
                (
                    &block.planes()[0],
                    (frame * channels + channel) * bytes_per_sample,
                )
            };
            let source = &plane.bytes()[offset..offset + bytes_per_sample];
            let sample = match format {
                SampleFormat::I16 | SampleFormat::I16Planar => {
                    i32::from(i16::from_le_bytes([source[0], source[1]]))
                }
                SampleFormat::I24 | SampleFormat::I24Planar => i32::from_le_bytes([
                    source[0],
                    source[1],
                    source[2],
                    if source[2] & 0x80 == 0 { 0 } else { 0xff },
                ]),
                _ => {
                    return Err(unsupported(
                        "encode_flac_block",
                        "FLAC input requires signed integer sample containers",
                    ));
                }
            };
            if !(minimum..=maximum).contains(&sample) {
                return Err(invalid(
                    "encode_flac_block",
                    "FLAC sample exceeds the declared source precision",
                ));
            }
            samples.push(sample);
        }
    }
    Ok(samples)
}

fn encode_native_flac(
    samples: &[i32],
    channels: usize,
    bits_per_sample: u32,
    sample_rate: u32,
    metadata: &MediaMetadata,
) -> Result<Vec<u8>> {
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, source)| flac_encode_error(format!("{source}")))?;
    let frame_count = samples.len() / channels;
    let block_size = config.block_size.min(frame_count.max(64));
    let source = MemSource::from_samples(
        samples,
        channels,
        usize::try_from(bits_per_sample)
            .map_err(|_| invalid("encode_flac_stream", "FLAC precision overflowed"))?,
        usize::try_from(sample_rate)
            .map_err(|_| invalid("encode_flac_stream", "FLAC sample rate overflowed"))?,
    );
    let mut stream = flacenc::encode_with_fixed_block_size(&config, source, block_size)
        .map_err(|source| flac_encode_error(format!("{source:?}")))?;
    let comments = encode_vorbis_comments(metadata)?;
    stream.add_metadata_block(
        MetadataBlockData::new_unknown(4, &comments)
            .map_err(|source| flac_encode_error(source.to_string()))?,
    );
    let mut sink = ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|source| flac_encode_error(source.to_string()))?;
    Ok(sink.as_slice().to_vec())
}

fn encode_vorbis_comments(metadata: &MediaMetadata) -> Result<Vec<u8>> {
    const VENDOR: &[u8] = b"superi-codecs-rs";
    let mut comments = Vec::new();
    for (key, value) in metadata.iter() {
        if key == "flac.bits-per-sample" {
            continue;
        }
        let field = format!(
            "SUPERI_{}={}",
            hex_encode(key.as_bytes()),
            encode_metadata_value(value)?
        );
        comments.push(field.into_bytes());
    }
    let mut payload = Vec::new();
    append_vorbis_bytes(&mut payload, VENDOR)?;
    payload.extend_from_slice(
        &u32::try_from(comments.len())
            .map_err(|_| invalid("encode_flac_metadata", "too many FLAC metadata values"))?
            .to_le_bytes(),
    );
    for comment in comments {
        append_vorbis_bytes(&mut payload, &comment)?;
    }
    if payload.len() > 0x00ff_ffff {
        return Err(invalid(
            "encode_flac_metadata",
            "FLAC Vorbis comment metadata exceeds the format limit",
        ));
    }
    Ok(payload)
}

fn append_vorbis_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    let length = u32::try_from(bytes.len())
        .map_err(|_| invalid("encode_flac_metadata", "FLAC metadata value is too large"))?;
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn encode_metadata_value(value: &MetadataValue) -> Result<String> {
    let encoded = match value {
        MetadataValue::Text(value) => format!("t:{}", hex_encode(value.as_bytes())),
        MetadataValue::Signed(value) => format!("i:{value}"),
        MetadataValue::Unsigned(value) => format!("u:{value}"),
        MetadataValue::Boolean(value) => format!("b:{}", u8::from(*value)),
        MetadataValue::Bytes(value) => format!("x:{}", hex_encode(value)),
        _ => {
            return Err(unsupported(
                "encode_flac_metadata",
                "FLAC metadata contains a value type this backend does not support",
            ));
        }
    };
    Ok(encoded)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn flac_encode_error(detail: String) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Degraded,
        format!("FLAC bitstream encoding failed: {detail}"),
    )
    .with_context(ErrorContext::new(COMPONENT, "encode_flac_stream"))
}

struct NativeDecode {
    format: AudioFormat,
    bits_per_sample: u32,
    metadata: MediaMetadata,
    blocks: Vec<RawAudioBlock>,
}

struct RawAudioBlock {
    time: u64,
    frame_count: u64,
    planes: Vec<AudioPlane>,
}

fn decode_native_flac(
    bytes: &[u8],
    explicit_format: Option<&AudioFormat>,
    operation: &OperationContext,
) -> Result<NativeDecode> {
    let mut reader = claxon::FlacReader::new(Cursor::new(bytes)).map_err(flac_decode_error)?;
    let stream = reader.streaminfo();
    let metadata = decode_vorbis_comments(&reader)?;
    if !matches!(stream.bits_per_sample, 8 | 12 | 16 | 20 | 24) {
        return Err(unsupported(
            "decode_flac_packet",
            "FLAC decoding supports 8, 12, 16, 20, or 24 bits per sample",
        ));
    }
    let layout = flac_channel_layout(stream.channels)?;
    let default_sample_format = if stream.bits_per_sample <= 16 {
        SampleFormat::I16
    } else {
        SampleFormat::I24
    };
    let format = if let Some(format) = explicit_format {
        let accepted_sample_format = match default_sample_format {
            SampleFormat::I16 => matches!(
                format.sample_format(),
                SampleFormat::I16 | SampleFormat::I16Planar
            ),
            SampleFormat::I24 => matches!(
                format.sample_format(),
                SampleFormat::I24 | SampleFormat::I24Planar
            ),
            _ => false,
        };
        if format.sample_rate() != stream.sample_rate
            || format.channel_layout() != &layout
            || !accepted_sample_format
        {
            return Err(conflict(
                "decode_flac_packet",
                "FLAC stream information disagrees with the requested audio format",
            ));
        }
        format.clone()
    } else {
        AudioFormat::new(stream.sample_rate, default_sample_format, layout)?
    };

    let mut blocks = Vec::new();
    let mut buffer = Vec::new();
    let mut frame_reader = reader.blocks();
    loop {
        operation.check("decode_flac_frame")?;
        let Some(block) = frame_reader
            .read_next_or_eof(buffer)
            .map_err(flac_decode_error)?
        else {
            break;
        };
        if block.channels() != stream.channels {
            return Err(corrupt(
                "decode_flac_packet",
                "FLAC frame channel count changed within the stream",
            ));
        }
        let time = block.time();
        let frame_count = stream
            .samples
            .map_or(u64::from(block.duration()), |samples| {
                samples
                    .saturating_sub(time)
                    .min(u64::from(block.duration()))
            });
        let planes = flac_audio_planes(
            &block,
            u32::try_from(frame_count)
                .map_err(|_| corrupt("decode_flac_packet", "FLAC frame duration overflowed"))?,
            format.sample_format(),
            operation,
        )?;
        buffer = block.into_buffer();
        if frame_count != 0 {
            blocks.push(RawAudioBlock {
                time,
                frame_count,
                planes,
            });
        }
    }
    if blocks.is_empty() && stream.samples != Some(0) {
        return Err(corrupt(
            "decode_flac_packet",
            "FLAC packet contains no decodable audio frames",
        ));
    }
    operation.check("decode_flac_packet")?;
    Ok(NativeDecode {
        format,
        bits_per_sample: stream.bits_per_sample,
        metadata,
        blocks,
    })
}

fn decode_vorbis_comments<R: std::io::Read>(
    reader: &claxon::FlacReader<R>,
) -> Result<MediaMetadata> {
    let mut metadata = MediaMetadata::new();
    if let Some(vendor) = reader.vendor() {
        metadata.insert("flac.vendor", MetadataValue::Text(vendor.to_owned()))?;
    }
    for (field, value) in reader.tags() {
        if let Some(encoded_key) = field.strip_prefix("SUPERI_") {
            let key_bytes = hex_decode(encoded_key).ok_or_else(|| {
                corrupt(
                    "decode_flac_metadata",
                    "FLAC Superi metadata contains an invalid key encoding",
                )
            })?;
            let key = String::from_utf8(key_bytes).map_err(|_| {
                corrupt(
                    "decode_flac_metadata",
                    "FLAC Superi metadata key is not valid UTF-8",
                )
            })?;
            let decoded = decode_metadata_value(value)?;
            metadata.insert(key, decoded)?;
            continue;
        }
        let field = field
            .bytes()
            .map(|byte| match byte.to_ascii_lowercase() {
                b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => {
                    char::from(byte.to_ascii_lowercase())
                }
                _ => '_',
            })
            .collect::<String>();
        let base = format!("flac.tag.{field}");
        let mut key = base.clone();
        let mut suffix = 2_u32;
        while metadata.get(&key).is_some() {
            key = format!("{base}.{suffix}");
            suffix = suffix.checked_add(1).ok_or_else(|| {
                corrupt(
                    "decode_flac_metadata",
                    "FLAC tag duplicate count overflowed",
                )
            })?;
        }
        metadata.insert(key, MetadataValue::Text(value.to_owned()))?;
    }
    Ok(metadata)
}

fn decode_metadata_value(value: &str) -> Result<MetadataValue> {
    let (kind, encoded) = value.split_once(':').ok_or_else(|| {
        corrupt(
            "decode_flac_metadata",
            "FLAC Superi metadata value is missing its type",
        )
    })?;
    match kind {
        "t" => {
            let bytes = hex_decode(encoded).ok_or_else(|| {
                corrupt(
                    "decode_flac_metadata",
                    "FLAC text metadata contains invalid hexadecimal data",
                )
            })?;
            let text = String::from_utf8(bytes).map_err(|_| {
                corrupt(
                    "decode_flac_metadata",
                    "FLAC text metadata is not valid UTF-8",
                )
            })?;
            Ok(MetadataValue::Text(text))
        }
        "i" => encoded
            .parse::<i64>()
            .map(MetadataValue::Signed)
            .map_err(|_| {
                corrupt(
                    "decode_flac_metadata",
                    "FLAC signed metadata is not a valid integer",
                )
            }),
        "u" => encoded
            .parse::<u64>()
            .map(MetadataValue::Unsigned)
            .map_err(|_| {
                corrupt(
                    "decode_flac_metadata",
                    "FLAC unsigned metadata is not a valid integer",
                )
            }),
        "b" => match encoded {
            "0" => Ok(MetadataValue::Boolean(false)),
            "1" => Ok(MetadataValue::Boolean(true)),
            _ => Err(corrupt(
                "decode_flac_metadata",
                "FLAC Boolean metadata must be zero or one",
            )),
        },
        "x" => hex_decode(encoded)
            .map(|bytes| MetadataValue::Bytes(Arc::from(bytes)))
            .ok_or_else(|| {
                corrupt(
                    "decode_flac_metadata",
                    "FLAC byte metadata contains invalid hexadecimal data",
                )
            }),
        _ => Err(corrupt(
            "decode_flac_metadata",
            "FLAC Superi metadata uses an unknown value type",
        )),
    }
}

fn hex_decode(encoded: &str) -> Option<Vec<u8>> {
    if encoded.len() % 2 != 0 {
        return None;
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|digits| {
            let high = hex_digit(digits[0])?;
            let low = hex_digit(digits[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

const fn hex_digit(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

fn flac_audio_planes(
    block: &claxon::Block,
    frame_count: u32,
    sample_format: SampleFormat,
    operation: &OperationContext,
) -> Result<Vec<AudioPlane>> {
    let channels = usize::try_from(block.channels())
        .map_err(|_| corrupt("decode_flac_packet", "FLAC channel count overflowed"))?;
    let frames = usize::try_from(frame_count)
        .map_err(|_| corrupt("decode_flac_packet", "FLAC frame size overflowed"))?;
    let bytes_per_sample = usize::from(sample_format.bytes_per_sample());
    let planar = sample_format.is_planar();
    let mut planes = if planar {
        (0..channels)
            .map(|_| Vec::with_capacity(frames.saturating_mul(bytes_per_sample)))
            .collect::<Vec<_>>()
    } else {
        vec![Vec::with_capacity(
            frames
                .saturating_mul(channels)
                .saturating_mul(bytes_per_sample),
        )]
    };
    for frame in 0..frames {
        if frame % 4_096 == 0 {
            operation.check("decode_flac_samples")?;
        }
        for channel in 0..channels {
            let sample = block.channel(channel as u32)[frame];
            let output = if planar {
                &mut planes[channel]
            } else {
                &mut planes[0]
            };
            match sample_format {
                SampleFormat::I16 | SampleFormat::I16Planar => {
                    let value = i16::try_from(sample).map_err(|_| {
                        corrupt(
                            "decode_flac_packet",
                            "FLAC sample exceeds its signed 16-bit output container",
                        )
                    })?;
                    output.extend_from_slice(&value.to_le_bytes());
                }
                SampleFormat::I24 | SampleFormat::I24Planar => {
                    if !(-8_388_608..=8_388_607).contains(&sample) {
                        return Err(corrupt(
                            "decode_flac_packet",
                            "FLAC sample exceeds its signed 24-bit output container",
                        ));
                    }
                    output.extend_from_slice(&sample.to_le_bytes()[..3]);
                }
                _ => {
                    return Err(unsupported(
                        "decode_flac_packet",
                        "FLAC output requires signed 16-bit or signed 24-bit samples",
                    ));
                }
            }
        }
    }
    Ok(planes
        .into_iter()
        .map(|plane| AudioPlane::new(Arc::from(plane)))
        .collect())
}

fn flac_channel_layout(channels: u32) -> Result<ChannelLayout> {
    use ChannelPosition::{
        BackCenter, BackLeft, BackRight, FrontCenter, FrontLeft, FrontRight, LowFrequency,
        SideLeft, SideRight,
    };
    let positions: &[ChannelPosition] = match channels {
        1 => &[FrontCenter],
        2 => &[FrontLeft, FrontRight],
        3 => &[FrontLeft, FrontRight, FrontCenter],
        4 => &[FrontLeft, FrontRight, BackLeft, BackRight],
        5 => &[FrontLeft, FrontRight, FrontCenter, BackLeft, BackRight],
        6 => &[
            FrontLeft,
            FrontRight,
            FrontCenter,
            LowFrequency,
            BackLeft,
            BackRight,
        ],
        7 => &[
            FrontLeft,
            FrontRight,
            FrontCenter,
            LowFrequency,
            BackCenter,
            SideLeft,
            SideRight,
        ],
        8 => &[
            FrontLeft,
            FrontRight,
            FrontCenter,
            LowFrequency,
            BackLeft,
            BackRight,
            SideLeft,
            SideRight,
        ],
        _ => {
            return Err(unsupported(
                "decode_flac_packet",
                "FLAC supports between one and eight audio channels",
            ));
        }
    };
    ChannelLayout::new(positions.iter().copied())
}

fn flac_decode_error(source: claxon::Error) -> Error {
    let category = if matches!(source, claxon::Error::Unsupported(_)) {
        ErrorCategory::Unsupported
    } else {
        ErrorCategory::CorruptData
    };
    Error::with_source(
        category,
        Recoverability::Degraded,
        "FLAC bitstream decoding failed",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, "decode_flac_packet"))
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

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
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
