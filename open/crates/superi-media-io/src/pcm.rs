//! Pure-Rust WAV and AIFF container sources for uncompressed audio.
//!
//! This module parses container structure without decoding samples. It preserves the source byte
//! order and precision in codec-neutral packets, carries Broadcast Wave time references into
//! packet timestamps, exposes channel meaning when the container defines it, retains ancillary
//! chunks in source order, and seeks at exact PCM sample-frame boundaries. PCM conversion into
//! [`crate::audio_io::AudioBlock`] belongs to a codec backend.

use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::{Duration, RationalTime, TimeRounding, Timebase};

use crate::backend::{BackendDescriptor, MediaBackend};
use crate::decode::{Decoder, DecoderConfig};
use crate::demux::{
    BackendId, CodecId, ContainerId, MediaMetadata, MediaSource, MetadataValue, Packet,
    PacketTiming, ProbeConfidence, SeekRequest, SourceIdentity, SourceInfo, SourceLocation,
    SourceProbe, SourceProbeResult, SourceRequest, StreamId, StreamInfo, StreamKind,
};
use crate::encode::{Encoder, EncoderConfig};
use crate::operation::OperationContext;
use crate::read::{read_exact_interruptible, CorruptionReport, ReadOutcome};

const STREAM_ID: StreamId = StreamId::new(0);
const PACKET_TARGET_BYTES: u64 = 1024 * 1024;
const READ_CHUNK_BYTES: usize = 64 * 1024;
const MAX_ANCILLARY_CHUNK_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ANCILLARY_CHUNKS: usize = 4_096;
const MAX_ANCILLARY_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_WAVE_FORMAT_BYTES: u64 = 40;
const AIFF_COMMON_BYTES: u64 = 18;
const MAX_RF64_SIZE_TABLE_ENTRIES: u64 = 4_096;
const MAX_RF64_DS64_BYTES: u64 = 28 + MAX_RF64_SIZE_TABLE_ENTRIES * 12;
const PCM_SUBFORMAT_GUID: [u8; 16] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];
const FLOAT_SUBFORMAT_GUID: [u8; 16] = [
    0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];

#[derive(Clone, Copy)]
struct AncillaryLimits {
    max_count: usize,
    max_bytes: u64,
}

impl AncillaryLimits {
    const DEFAULT: Self = Self::new(MAX_ANCILLARY_CHUNKS, MAX_ANCILLARY_TOTAL_BYTES);

    const fn new(max_count: usize, max_bytes: u64) -> Self {
        Self {
            max_count,
            max_bytes,
        }
    }
}

struct AncillaryBudget {
    limits: AncillaryLimits,
    count: usize,
    total_bytes: u64,
}

impl AncillaryBudget {
    const fn new(limits: AncillaryLimits) -> Self {
        Self {
            limits,
            count: 0,
            total_bytes: 0,
        }
    }

    fn reserve(
        &mut self,
        chunks: &mut Vec<AncillaryChunk>,
        size: u64,
        operation: &'static str,
    ) -> Result<()> {
        let count = self.count.checked_add(1).ok_or_else(|| {
            resource_exhausted(operation, "ancillary chunk count accounting overflowed")
        })?;
        let total_bytes = self.total_bytes.checked_add(size).ok_or_else(|| {
            resource_exhausted(operation, "ancillary chunk byte accounting overflowed")
        })?;
        if count > self.limits.max_count {
            return Err(resource_exhausted(
                operation,
                "ancillary chunk count exceeds the preservation limit",
            ));
        }
        if total_bytes > self.limits.max_bytes {
            return Err(resource_exhausted(
                operation,
                "aggregate ancillary chunk bytes exceed the preservation limit",
            ));
        }
        chunks.try_reserve(1).map_err(|_| {
            resource_exhausted(operation, "ancillary chunk index could not be allocated")
        })?;
        self.count = count;
        self.total_bytes = total_bytes;
        Ok(())
    }
}

/// In-tree source backend for uncompressed WAV and AIFF containers.
pub struct PcmContainerBackend {
    descriptor: BackendDescriptor,
    wave_container: ContainerId,
    aiff_container: ContainerId,
}

impl PcmContainerBackend {
    /// Creates the WAV and AIFF source backend.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(
                BackendId::new("pcm-containers")?,
                "Superi WAV and AIFF demuxer",
            )?,
            wave_container: ContainerId::new("wav")?,
            aiff_container: ContainerId::new("aiff")?,
        })
    }
}

impl MediaBackend for PcmContainerBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_pcm_container_source")?;
        if probe.bytes().len() < 12 {
            return if probe.is_complete() {
                Ok(SourceProbeResult::NoMatch)
            } else {
                SourceProbeResult::need_more_data(12)
            };
        }

        let container = match (&probe.bytes()[..4], &probe.bytes()[8..12]) {
            (b"RIFF" | b"RF64", b"WAVE") => Some(self.wave_container.clone()),
            (b"FORM", b"AIFF") => Some(self.aiff_container.clone()),
            _ => None,
        };
        operation.check("probe_pcm_container_source")?;
        Ok(match container {
            Some(container) => SourceProbeResult::matched(container, ProbeConfidence::new(100)?),
            None => SourceProbeResult::NoMatch,
        })
    }

    fn open_source(
        &self,
        request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        Ok(Box::new(PcmContainerSource::open(request, operation)?))
    }

    fn create_decoder(
        &self,
        _config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_pcm_container_decoder")?;
        Err(unsupported_backend("create_decoder", "decode"))
    }

    fn create_encoder(
        &self,
        _config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_pcm_container_encoder")?;
        Err(unsupported_backend("create_encoder", "encode"))
    }
}

/// Supported uncompressed audio container families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PcmContainerKind {
    /// Microsoft RIFF/WAVE, including WAVEFORMATEXTENSIBLE and Broadcast Wave metadata.
    Wave,
    /// Apple Audio Interchange File Format (AIFF).
    Aiff,
}

impl PcmContainerKind {
    /// Returns the stable container-family code used in source metadata.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Wave => "wav",
            Self::Aiff => "aiff",
        }
    }
}

/// Numeric interpretation of the sample payload stored by a container.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PcmEncoding {
    /// Signed or unsigned linear integer PCM, as determined by the format and precision.
    Integer,
    /// IEEE 754 floating-point PCM.
    Float,
}

impl PcmEncoding {
    /// Returns the stable numeric-encoding code used in stream metadata.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Float => "float",
        }
    }
}

/// Byte order of each multi-byte sample in the preserved packet payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ByteOrder {
    /// Least-significant byte first, used by RIFF/WAVE.
    LittleEndian,
    /// Most-significant byte first, used by AIFF.
    BigEndian,
}

impl ByteOrder {
    /// Returns the stable byte-order code used in stream metadata.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::LittleEndian => "little_endian",
            Self::BigEndian => "big_endian",
        }
    }
}

/// Exact stored PCM stream description before sample decoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcmStreamFormat {
    encoding: PcmEncoding,
    byte_order: ByteOrder,
    sample_rate: u32,
    bits_per_sample: u16,
    valid_bits_per_sample: u16,
    block_align: u16,
    channel_layout: ChannelLayout,
}

impl PcmStreamFormat {
    /// Returns whether samples are integer or floating point.
    #[must_use]
    pub const fn encoding(&self) -> PcmEncoding {
        self.encoding
    }

    /// Returns the byte order retained in packet payloads.
    #[must_use]
    pub const fn byte_order(&self) -> ByteOrder {
        self.byte_order
    }

    /// Returns sample frames per second.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the number of storage bits occupied by each sample container.
    #[must_use]
    pub const fn bits_per_sample(&self) -> u16 {
        self.bits_per_sample
    }

    /// Returns the meaningful sample precision, which can be smaller for extensible WAVE.
    #[must_use]
    pub const fn valid_bits_per_sample(&self) -> u16 {
        self.valid_bits_per_sample
    }

    /// Returns bytes occupied by one interleaved sample frame.
    #[must_use]
    pub const fn block_align(&self) -> u16 {
        self.block_align
    }

    /// Returns channels in their stored routing order.
    #[must_use]
    pub const fn channel_layout(&self) -> &ChannelLayout {
        &self.channel_layout
    }
}

/// One non-structural container chunk retained byte-for-byte in source order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AncillaryChunk {
    id: [u8; 4],
    payload_offset: u64,
    data: Arc<[u8]>,
}

impl AncillaryChunk {
    /// Returns the original four-byte chunk identifier.
    #[must_use]
    pub const fn id(&self) -> [u8; 4] {
        self.id
    }

    /// Returns the absolute source byte offset of the first payload byte.
    #[must_use]
    pub const fn payload_offset(&self) -> u64 {
        self.payload_offset
    }

    /// Returns the exact chunk payload, excluding its header and alignment pad byte.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// An opened WAV or AIFF source that yields bounded, sample-aligned PCM packets.
#[derive(Debug)]
pub struct PcmContainerSource {
    storage: Storage,
    info: SourceInfo,
    kind: PcmContainerKind,
    format: PcmStreamFormat,
    data_offset: u64,
    frame_count: u64,
    presentation_origin: u64,
    cursor_frame: u64,
    ancillary_chunks: Vec<AncillaryChunk>,
}

impl PcmContainerSource {
    /// Opens and validates a RIFF/WAVE or AIFF source from a path or immutable memory.
    pub fn open(request: &SourceRequest, operation: &OperationContext) -> Result<Self> {
        operation.check("open_pcm_container_source")?;
        let mut storage = Storage::open(request.location(), operation)?;
        let parsed = parse_container(&mut storage, operation)?;
        let fingerprint = fingerprint(&mut storage, operation)?;

        if let Some(expected) = request.expected_fingerprint() {
            if expected != fingerprint {
                return Err(Error::new(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "relinked audio container does not match the expected content",
                )
                .with_context(
                    ErrorContext::new("superi-media-io.pcm", "verify_relink")
                        .with_field("expected_fingerprint", expected)
                        .with_field("actual_fingerprint", fingerprint),
                ));
            }
        }

        let codec = codec_id(parsed.kind, &parsed.format)?;
        let timebase = Timebase::integer(parsed.format.sample_rate)?;
        let mut stream = StreamInfo::new(STREAM_ID, StreamKind::Audio, codec, timebase);
        stream = add_format_metadata_to_stream(stream, &parsed.format)?;

        let identity = SourceIdentity::new(request.media_id(), fingerprint)?;
        let duration = Duration::from_samples(parsed.frame_count, parsed.format.sample_rate)?;
        let mut info = SourceInfo::new(identity, vec![stream])?.with_duration(duration);
        info = add_container_metadata(info, &parsed)?;
        operation.check("open_pcm_container_source")?;

        Ok(Self {
            storage,
            info,
            kind: parsed.kind,
            format: parsed.format,
            data_offset: parsed.data_offset,
            frame_count: parsed.frame_count,
            presentation_origin: parsed.presentation_origin,
            cursor_frame: 0,
            ancillary_chunks: parsed.ancillary_chunks,
        })
    }

    /// Returns the parsed container family.
    #[must_use]
    pub const fn container_kind(&self) -> PcmContainerKind {
        self.kind
    }

    /// Returns the exact stored stream description.
    #[must_use]
    pub const fn format(&self) -> &PcmStreamFormat {
        &self.format
    }

    /// Returns the total number of complete interleaved sample frames.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Returns the absolute byte offset of the first stored audio sample.
    #[must_use]
    pub const fn audio_data_offset(&self) -> u64 {
        self.data_offset
    }

    /// Returns all non-structural chunks in original source order.
    #[must_use]
    pub fn ancillary_chunks(&self) -> &[AncillaryChunk] {
        &self.ancillary_chunks
    }

    fn packet(&self, bytes: Vec<u8>, frame_count: u64, byte_offset: u64) -> Result<Packet> {
        let presentation = self
            .presentation_origin
            .checked_add(self.cursor_frame)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| corrupt("read_packet", "audio packet timestamp overflowed", None))?;
        let timing = PacketTiming::new(
            Timebase::integer(self.format.sample_rate)?,
            Some(presentation),
            Some(presentation),
            Some(frame_count),
        )?;
        let mut packet = Packet::new(STREAM_ID, Arc::from(bytes), timing).with_keyframe(true);
        packet = packet.with_metadata("container.offset", MetadataValue::Unsigned(byte_offset))?;
        packet = packet.with_metadata(
            "container.sample_offset",
            MetadataValue::Unsigned(self.cursor_frame),
        )?;
        Ok(packet)
    }
}

impl MediaSource for PcmContainerSource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        operation.check("read_pcm_packet")?;
        if self.cursor_frame == self.frame_count {
            return Ok(ReadOutcome::EndOfStream);
        }

        let block_align = u64::from(self.format.block_align);
        let packet_frames = (PACKET_TARGET_BYTES / block_align).max(1);
        let frame_count = packet_frames.min(self.frame_count - self.cursor_frame);
        let byte_offset = self
            .cursor_frame
            .checked_mul(block_align)
            .and_then(|value| self.data_offset.checked_add(value))
            .ok_or_else(|| corrupt("read_packet", "audio packet offset overflowed", None))?;
        let byte_count = frame_count
            .checked_mul(block_align)
            .ok_or_else(|| corrupt("read_packet", "audio packet size overflowed", None))?;
        let read =
            self.storage
                .read_bytes(byte_offset, byte_count, "read_pcm_packet", operation)?;
        match read {
            ReadOutcome::Complete(bytes) => {
                let packet = self.packet(bytes, frame_count, byte_offset)?;
                operation.check("read_pcm_packet")?;
                self.cursor_frame += frame_count;
                Ok(ReadOutcome::Complete(packet))
            }
            ReadOutcome::Partial { mut value, report } => {
                let report = report.with_stream(STREAM_ID);
                let block_align = usize::from(self.format.block_align);
                let usable_bytes = value.len() / block_align * block_align;
                if usable_bytes == 0 {
                    return Err(report.to_error("read_pcm_packet"));
                }
                value.truncate(usable_bytes);
                let usable_frames = u64::try_from(usable_bytes / block_align)
                    .expect("bounded packet byte count fits in u64");
                let packet = self.packet(value, usable_frames, byte_offset)?;
                operation.check("read_pcm_packet")?;
                self.cursor_frame += usable_frames;
                Ok(ReadOutcome::Partial {
                    value: packet,
                    report,
                })
            }
            ReadOutcome::EndOfStream => {
                let expected = usize::try_from(byte_count).map_err(|_| {
                    resource_exhausted("read_pcm_packet", "packet size is too large to report")
                })?;
                Err(CorruptionReport::truncated(byte_offset, expected, 0)?
                    .with_stream(STREAM_ID)
                    .to_error("read_pcm_packet"))
            }
        }
    }

    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime> {
        operation.check("seek_pcm_source")?;
        let timebase = Timebase::integer(self.format.sample_rate)?;
        let target = request
            .target()
            .checked_rescale(timebase, TimeRounding::Exact)?;
        let origin = i64::try_from(self.presentation_origin)
            .map_err(|_| corrupt("seek", "audio presentation origin overflowed", None))?;
        let end = self
            .presentation_origin
            .checked_add(self.frame_count)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| corrupt("seek", "audio presentation range overflowed", None))?;
        if target.value() < origin || target.value() > end {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "audio seek target is outside the source presentation range",
            )
            .with_context(
                ErrorContext::new("superi-media-io.pcm", "seek")
                    .with_field("target", target.value().to_string())
                    .with_field("range_start", origin.to_string())
                    .with_field("range_end", end.to_string()),
            ));
        }
        let cursor_frame = u64::try_from(target.value() - origin)
            .expect("validated nonnegative PCM seek coordinate");
        operation.check("seek_pcm_source")?;
        self.cursor_frame = cursor_frame;
        Ok(target)
    }
}

#[derive(Debug)]
enum Storage {
    File { file: File, len: u64 },
    Memory(Arc<[u8]>),
}

impl Storage {
    fn open(location: &SourceLocation, operation: &OperationContext) -> Result<Self> {
        operation.check("open_pcm_storage")?;
        match location {
            SourceLocation::Path(path) => {
                let file = File::open(path).map_err(|error| open_error(error, "open_source"))?;
                let len = file
                    .metadata()
                    .map_err(|error| open_error(error, "read_source_metadata"))?
                    .len();
                operation.check("open_pcm_storage")?;
                Ok(Self::File { file, len })
            }
            SourceLocation::Memory { data, .. } => {
                operation.check("open_pcm_storage")?;
                Ok(Self::Memory(Arc::clone(data)))
            }
        }
    }

    fn len(&self) -> u64 {
        match self {
            Self::File { len, .. } => *len,
            Self::Memory(data) => data.len() as u64,
        }
    }

    fn read_bytes(
        &mut self,
        offset: u64,
        len: u64,
        operation_name: &'static str,
        operation: &OperationContext,
    ) -> Result<ReadOutcome<Vec<u8>>> {
        operation.check(operation_name)?;
        let end = offset.checked_add(len).ok_or_else(|| {
            corrupt(
                operation_name,
                "container byte range overflowed",
                Some(offset),
            )
        })?;
        if end > self.len() {
            return Err(corrupt(
                operation_name,
                "container byte range extends past the source",
                Some(offset),
            ));
        }
        let len = usize::try_from(len).map_err(|_| {
            resource_exhausted(
                operation_name,
                "container byte range is too large to address",
            )
        })?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(len).map_err(|_| {
            resource_exhausted(
                operation_name,
                "container byte range could not be allocated",
            )
        })?;
        bytes.resize(len, 0);
        match self {
            Self::File { file, .. } => {
                file.seek(SeekFrom::Start(offset))
                    .map_err(|error| read_error(error, operation_name, offset))?;
                let mut actual = 0_usize;
                while actual < len {
                    let chunk_end = (actual + READ_CHUNK_BYTES).min(len);
                    let chunk_offset = offset
                        .checked_add(actual as u64)
                        .expect("validated byte range contains every chunk offset");
                    match read_exact_interruptible(
                        file,
                        &mut bytes[actual..chunk_end],
                        chunk_offset,
                        operation,
                    )? {
                        ReadOutcome::Complete(read) => actual += read,
                        ReadOutcome::Partial { value, .. } => {
                            actual += value;
                            bytes.truncate(actual);
                            let report = CorruptionReport::truncated(offset, len, actual)?;
                            return Ok(ReadOutcome::Partial {
                                value: bytes,
                                report,
                            });
                        }
                        ReadOutcome::EndOfStream if actual == 0 => {
                            return Ok(ReadOutcome::EndOfStream);
                        }
                        ReadOutcome::EndOfStream => {
                            bytes.truncate(actual);
                            let report = CorruptionReport::truncated(offset, len, actual)?;
                            return Ok(ReadOutcome::Partial {
                                value: bytes,
                                report,
                            });
                        }
                    }
                }
                Ok(ReadOutcome::Complete(bytes))
            }
            Self::Memory(data) => {
                let start = usize::try_from(offset).map_err(|_| {
                    resource_exhausted(operation_name, "container offset is too large to address")
                })?;
                for (index, chunk) in bytes.chunks_mut(READ_CHUNK_BYTES).enumerate() {
                    operation.check(operation_name)?;
                    let source_start = start + index * READ_CHUNK_BYTES;
                    chunk.copy_from_slice(&data[source_start..source_start + chunk.len()]);
                }
                operation.check(operation_name)?;
                Ok(ReadOutcome::Complete(bytes))
            }
        }
    }
}

struct ParsedContainer {
    kind: PcmContainerKind,
    format: PcmStreamFormat,
    data_offset: u64,
    data_size: u64,
    frame_count: u64,
    presentation_origin: u64,
    ancillary_chunks: Vec<AncillaryChunk>,
    metadata: MediaMetadata,
}

fn parse_container(storage: &mut Storage, operation: &OperationContext) -> Result<ParsedContainer> {
    parse_container_with_limits(storage, operation, AncillaryLimits::DEFAULT)
}

fn parse_container_with_limits(
    storage: &mut Storage,
    operation: &OperationContext,
    ancillary_limits: AncillaryLimits,
) -> Result<ParsedContainer> {
    let header = read_complete_bytes(storage, 0, 12, "read_container_header", operation)?;
    match (&header[..4], &header[8..12]) {
        (b"RIFF" | b"RF64", b"WAVE") => parse_wave(storage, &header, operation, ancillary_limits),
        (b"RIFX", b"WAVE") => Err(unsupported(
            "parse_wave",
            "big-endian RIFX/WAVE is not supported; use standard RIFF/WAVE",
        )),
        (b"FORM", b"AIFF") => parse_aiff(storage, &header, operation, ancillary_limits),
        (b"FORM", b"AIFC") => Err(unsupported(
            "parse_aiff",
            "compressed AIFF-C is outside WAV and AIFF PCM container support",
        )),
        _ => Err(unsupported(
            "parse_container",
            "source is not a supported RIFF/WAVE or AIFF container",
        )),
    }
}

fn parse_wave(
    storage: &mut Storage,
    header: &[u8],
    operation: &OperationContext,
    ancillary_limits: AncillaryLimits,
) -> Result<ParsedContainer> {
    let is_rf64 = &header[..4] == b"RF64";
    let declared_size = u64::from(le_u32(&header[4..8]));
    if is_rf64 && declared_size != u64::from(u32::MAX) {
        return Err(corrupt(
            "parse_wave",
            "RF64 must use the RIFF size sentinel",
            Some(4),
        ));
    }

    let mut container_end = if is_rf64 {
        storage.len()
    } else {
        declared_size
            .checked_add(8)
            .ok_or_else(|| corrupt("parse_wave", "RIFF size overflowed", Some(4)))?
    };
    validate_container_end(storage, container_end, "parse_wave")?;

    let mut cursor = 12_u64;
    let mut format = None;
    let mut data = None;
    let mut data_size_override = None;
    let mut sample_count_hint = None;
    let mut rf64_size_table = Vec::new();
    let mut saw_ds64 = false;
    let mut presentation_origin = 0_u64;
    let mut saw_broadcast_extension = false;
    let mut metadata = MediaMetadata::new();
    let mut ancillary_chunks = Vec::new();
    let mut ancillary_budget = AncillaryBudget::new(ancillary_limits);

    while cursor < container_end {
        operation.check("parse_wave")?;
        let chunk_header =
            read_complete_bytes(storage, cursor, 8, "read_wave_chunk_header", operation)?;
        let id: [u8; 4] = chunk_header[..4].try_into().expect("four-byte chunk id");
        let size32 = le_u32(&chunk_header[4..8]);
        let mut size = u64::from(size32);
        let payload_offset = cursor + 8;

        if is_rf64 && cursor == 12 && id != *b"ds64" {
            return Err(corrupt(
                "parse_wave",
                "RF64 is missing the mandatory first ds64 chunk",
                Some(cursor),
            ));
        }
        if id == *b"ds64" {
            if !is_rf64 {
                return Err(corrupt(
                    "parse_wave",
                    "ds64 is only valid in an RF64 container",
                    Some(cursor),
                ));
            }
            if saw_ds64 {
                return Err(corrupt(
                    "parse_wave",
                    "RF64 contains more than one ds64 chunk",
                    Some(cursor),
                ));
            }
            if cursor != 12 {
                return Err(corrupt(
                    "parse_wave",
                    "RF64 ds64 chunk is not the first child chunk",
                    Some(cursor),
                ));
            }
            if !(28..=MAX_RF64_DS64_BYTES).contains(&size) {
                return Err(corrupt(
                    "parse_wave",
                    "RF64 ds64 chunk size is outside the supported schema bound",
                    Some(cursor),
                ));
            }
            let ds64 =
                read_complete_bytes(storage, payload_offset, size, "read_rf64_sizes", operation)?;
            let riff_size = le_u64(&ds64[0..8]);
            let data_size = le_u64(&ds64[8..16]);
            let sample_count = le_u64(&ds64[16..24]);
            let table_length = u64::from(le_u32(&ds64[24..28]));
            if table_length > MAX_RF64_SIZE_TABLE_ENTRIES {
                return Err(resource_exhausted(
                    "parse_wave",
                    "RF64 ds64 size table exceeds the entry limit",
                ));
            }
            let required_size = table_length
                .checked_mul(12)
                .and_then(|value| value.checked_add(28))
                .ok_or_else(|| corrupt("parse_wave", "RF64 size table overflowed", Some(cursor)))?;
            if required_size != size {
                return Err(corrupt(
                    "parse_wave",
                    "RF64 ds64 chunk size does not match its size table",
                    Some(cursor),
                ));
            }
            let table_capacity =
                usize::try_from(table_length).expect("bounded RF64 table length fits in usize");
            rf64_size_table
                .try_reserve_exact(table_capacity)
                .map_err(|_| {
                    resource_exhausted("parse_wave", "RF64 size table could not be allocated")
                })?;
            for index in 0..table_length {
                let entry =
                    usize::try_from(28 + index * 12).expect("bounded RF64 size-table offset");
                let chunk_id: [u8; 4] = ds64[entry..entry + 4]
                    .try_into()
                    .expect("four-byte RF64 chunk id");
                rf64_size_table.push((chunk_id, le_u64(&ds64[entry + 4..entry + 12])));
            }
            container_end = riff_size
                .checked_add(8)
                .ok_or_else(|| corrupt("parse_wave", "RF64 size overflowed", Some(cursor)))?;
            validate_container_end(storage, container_end, "parse_wave")?;
            data_size_override = Some(data_size);
            sample_count_hint = Some(sample_count);
            saw_ds64 = true;
            metadata.insert(
                "container.rf64.ds64_raw",
                MetadataValue::Bytes(Arc::from(ds64)),
            )?;
            metadata.insert(
                "container.rf64.sample_count",
                MetadataValue::Unsigned(sample_count),
            )?;
        } else {
            if size32 == u32::MAX {
                if is_rf64 && id == *b"data" {
                    size = data_size_override.ok_or_else(|| {
                        corrupt(
                            "parse_wave",
                            "RF64 data sentinel has no ds64 size",
                            Some(cursor),
                        )
                    })?;
                } else if is_rf64 {
                    let Some(index) = rf64_size_table
                        .iter()
                        .position(|(chunk_id, _)| *chunk_id == id)
                    else {
                        return Err(corrupt(
                            "parse_wave",
                            "RF64 chunk sentinel has no ds64 table entry",
                            Some(cursor),
                        ));
                    };
                    size = rf64_size_table.remove(index).1;
                } else {
                    return Err(corrupt(
                        "parse_wave",
                        "chunk uses an unresolved RF64 size sentinel",
                        Some(cursor),
                    ));
                }
            }
            match id {
                [b'f', b'm', b't', b' '] => {
                    if format.is_some() {
                        return Err(corrupt(
                            "parse_wave",
                            "WAVE contains more than one format chunk",
                            Some(cursor),
                        ));
                    }
                    if size > MAX_WAVE_FORMAT_BYTES {
                        return Err(corrupt(
                            "parse_wave",
                            "WAVE format chunk exceeds the supported PCM schema size",
                            Some(cursor),
                        ));
                    }
                    let bytes = read_complete_bytes(
                        storage,
                        payload_offset,
                        size,
                        "read_wave_format",
                        operation,
                    )?;
                    let parsed_format = parse_wave_format(&bytes, cursor)?;
                    metadata.insert(
                        "container.wav.format_raw",
                        MetadataValue::Bytes(Arc::from(bytes.clone())),
                    )?;
                    if le_u16(&bytes[0..2]) == 0xfffe {
                        metadata.insert(
                            "container.wav.channel_mask",
                            MetadataValue::Unsigned(u64::from(le_u32(&bytes[20..24]))),
                        )?;
                    }
                    format = Some(parsed_format);
                }
                [b'd', b'a', b't', b'a'] => {
                    if data.replace((payload_offset, size)).is_some() {
                        return Err(corrupt(
                            "parse_wave",
                            "WAVE contains more than one data chunk",
                            Some(cursor),
                        ));
                    }
                }
                _ => {
                    ancillary_budget.reserve(
                        &mut ancillary_chunks,
                        size,
                        "preserve_wave_ancillary_chunk",
                    )?;
                    let bytes = read_ancillary(
                        storage,
                        payload_offset,
                        size,
                        "read_wave_chunk",
                        operation,
                    )?;
                    if id == *b"bext" {
                        if saw_broadcast_extension {
                            return Err(corrupt(
                                "parse_wave",
                                "WAVE contains more than one Broadcast Wave extension",
                                Some(cursor),
                            ));
                        }
                        presentation_origin = parse_broadcast_extension(&bytes, &mut metadata)?;
                        saw_broadcast_extension = true;
                    }
                    ancillary_chunks.push(AncillaryChunk {
                        id,
                        payload_offset,
                        data: Arc::from(bytes),
                    });
                }
            }
        }

        cursor = next_chunk(cursor, size, container_end, "parse_wave")?;
    }
    operation.check("parse_wave")?;
    if cursor != container_end {
        return Err(corrupt(
            "parse_wave",
            "RIFF chunks do not end at the declared container boundary",
            Some(cursor),
        ));
    }
    if !rf64_size_table.is_empty() {
        return Err(corrupt(
            "parse_wave",
            "RF64 ds64 table contains entries with no matching chunk sentinel",
            None,
        ));
    }
    if is_rf64 && !saw_ds64 {
        return Err(corrupt(
            "parse_wave",
            "RF64 is missing its mandatory ds64 chunk",
            Some(12),
        ));
    }

    let format = format.ok_or_else(|| {
        corrupt(
            "parse_wave",
            "WAVE is missing its required format chunk",
            None,
        )
    })?;
    let (data_offset, data_size) = data.ok_or_else(|| {
        corrupt(
            "parse_wave",
            "WAVE is missing its required data chunk",
            None,
        )
    })?;
    if data_size % u64::from(format.block_align) != 0 {
        return Err(corrupt(
            "parse_wave",
            "WAVE data size is not a whole number of sample frames",
            Some(data_offset),
        ));
    }
    let frame_count = data_size / u64::from(format.block_align);
    if sample_count_hint.is_some_and(|hint| hint != 0 && hint != frame_count) {
        return Err(corrupt(
            "parse_wave",
            "RF64 sample count contradicts the PCM data size",
            Some(data_offset),
        ));
    }
    validate_presentation_range(presentation_origin, frame_count, "parse_wave")?;

    Ok(ParsedContainer {
        kind: PcmContainerKind::Wave,
        format,
        data_offset,
        data_size,
        frame_count,
        presentation_origin,
        ancillary_chunks,
        metadata,
    })
}

fn parse_wave_format(bytes: &[u8], chunk_offset: u64) -> Result<PcmStreamFormat> {
    if bytes.len() < 16 {
        return Err(corrupt(
            "parse_wave_format",
            "WAVE format chunk is shorter than PCM format fields",
            Some(chunk_offset),
        ));
    }
    let tag = le_u16(&bytes[0..2]);
    let channel_count = le_u16(&bytes[2..4]);
    let sample_rate = le_u32(&bytes[4..8]);
    let average_bytes_per_second = le_u32(&bytes[8..12]);
    let block_align = le_u16(&bytes[12..14]);
    let bits_per_sample = le_u16(&bytes[14..16]);

    let (encoding, valid_bits_per_sample, channel_mask) = match tag {
        0x0001 | 0x0003 => {
            if bytes.len() != 16 && bytes.len() != 18 {
                return Err(corrupt(
                    "parse_wave_format",
                    "basic PCM WAVE format chunk has an unsupported schema size",
                    Some(chunk_offset),
                ));
            }
            if bytes.len() == 18 && le_u16(&bytes[16..18]) != 0 {
                return Err(corrupt(
                    "parse_wave_format",
                    "basic PCM WAVE format chunk declares unexpected extra data",
                    Some(chunk_offset),
                ));
            }
            let encoding = if tag == 0x0001 {
                PcmEncoding::Integer
            } else {
                PcmEncoding::Float
            };
            (encoding, bits_per_sample, None)
        }
        0xfffe => {
            if bytes.len() != 40 {
                return Err(corrupt(
                    "parse_wave_format",
                    "WAVEFORMATEXTENSIBLE chunk must match its fixed schema size",
                    Some(chunk_offset),
                ));
            }
            let extra_size = usize::from(le_u16(&bytes[16..18]));
            if extra_size != 22 {
                return Err(corrupt(
                    "parse_wave_format",
                    "WAVEFORMATEXTENSIBLE extra-data size must be 22 bytes",
                    Some(chunk_offset),
                ));
            }
            let valid = le_u16(&bytes[18..20]);
            let mask = le_u32(&bytes[20..24]);
            let guid: [u8; 16] = bytes[24..40].try_into().expect("validated format length");
            let encoding = if guid == PCM_SUBFORMAT_GUID {
                PcmEncoding::Integer
            } else if guid == FLOAT_SUBFORMAT_GUID {
                PcmEncoding::Float
            } else {
                return Err(unsupported(
                    "parse_wave_format",
                    "WAVEFORMATEXTENSIBLE subformat is not integer or floating-point PCM",
                ));
            };
            (encoding, valid, Some(mask))
        }
        _ => {
            return Err(unsupported(
                "parse_wave_format",
                "WAVE format tag is not integer or floating-point PCM",
            ));
        }
    };

    validate_pcm_fields(
        encoding,
        channel_count,
        sample_rate,
        bits_per_sample,
        valid_bits_per_sample,
        block_align,
        Some(average_bytes_per_second),
        chunk_offset,
    )?;
    let channel_layout = wave_channel_layout(channel_count, channel_mask, chunk_offset)?;
    Ok(PcmStreamFormat {
        encoding,
        byte_order: ByteOrder::LittleEndian,
        sample_rate,
        bits_per_sample,
        valid_bits_per_sample,
        block_align,
        channel_layout,
    })
}

fn parse_broadcast_extension(bytes: &[u8], metadata: &mut MediaMetadata) -> Result<u64> {
    if bytes.len() < 602 {
        return Err(corrupt(
            "parse_broadcast_extension",
            "Broadcast Wave extension is shorter than its fixed fields",
            None,
        ));
    }
    insert_text(metadata, "container.bwf.description", &bytes[0..256])?;
    insert_text(metadata, "container.bwf.originator", &bytes[256..288])?;
    insert_text(
        metadata,
        "container.bwf.originator_reference",
        &bytes[288..320],
    )?;
    insert_text(metadata, "container.bwf.origination_date", &bytes[320..330])?;
    insert_text(metadata, "container.bwf.origination_time", &bytes[330..338])?;
    let time_reference = le_u64(&bytes[338..346]);
    metadata.insert(
        "container.bwf.time_reference",
        MetadataValue::Unsigned(time_reference),
    )?;
    let version = le_u16(&bytes[346..348]);
    metadata.insert(
        "container.bwf.version",
        MetadataValue::Unsigned(u64::from(version)),
    )?;
    if bytes[348..412].iter().any(|byte| *byte != 0) {
        metadata.insert(
            "container.bwf.umid",
            MetadataValue::Bytes(Arc::from(&bytes[348..412])),
        )?;
    }
    if version >= 2 {
        for (key, range) in [
            ("loudness_value", 412..414),
            ("loudness_range", 414..416),
            ("max_true_peak_level", 416..418),
            ("max_momentary_loudness", 418..420),
            ("max_short_term_loudness", 420..422),
        ] {
            metadata.insert(
                format!("container.bwf.{key}"),
                MetadataValue::Signed(i64::from(le_i16(&bytes[range]))),
            )?;
        }
    }
    insert_text(metadata, "container.bwf.coding_history", &bytes[602..])?;
    Ok(time_reference)
}

fn parse_aiff(
    storage: &mut Storage,
    header: &[u8],
    operation: &OperationContext,
    ancillary_limits: AncillaryLimits,
) -> Result<ParsedContainer> {
    let container_end = u64::from(be_u32(&header[4..8]))
        .checked_add(8)
        .ok_or_else(|| corrupt("parse_aiff", "AIFF size overflowed", Some(4)))?;
    validate_container_end(storage, container_end, "parse_aiff")?;

    let mut cursor = 12_u64;
    let mut common = None;
    let mut sound = None;
    let mut metadata = MediaMetadata::new();
    let mut ancillary_chunks = Vec::new();
    let mut ancillary_budget = AncillaryBudget::new(ancillary_limits);
    while cursor < container_end {
        operation.check("parse_aiff")?;
        let chunk_header =
            read_complete_bytes(storage, cursor, 8, "read_aiff_chunk_header", operation)?;
        let id: [u8; 4] = chunk_header[..4].try_into().expect("four-byte chunk id");
        let size = u64::from(be_u32(&chunk_header[4..8]));
        let payload_offset = cursor + 8;
        match id {
            [b'C', b'O', b'M', b'M'] => {
                if common.is_some() {
                    return Err(corrupt(
                        "parse_aiff",
                        "AIFF contains more than one Common Chunk",
                        Some(cursor),
                    ));
                }
                if size != AIFF_COMMON_BYTES {
                    return Err(corrupt(
                        "parse_aiff",
                        "AIFF Common Chunk must match its fixed schema size",
                        Some(cursor),
                    ));
                }
                let bytes = read_complete_bytes(
                    storage,
                    payload_offset,
                    size,
                    "read_aiff_common",
                    operation,
                )?;
                metadata.insert(
                    "container.aiff.common_raw",
                    MetadataValue::Bytes(Arc::from(bytes.clone())),
                )?;
                common = Some(parse_aiff_common(&bytes, cursor)?);
            }
            [b'S', b'S', b'N', b'D'] => {
                if sound.is_some() {
                    return Err(corrupt(
                        "parse_aiff",
                        "AIFF contains more than one Sound Data Chunk",
                        Some(cursor),
                    ));
                }
                if size < 8 {
                    return Err(corrupt(
                        "parse_aiff",
                        "AIFF Sound Data Chunk is shorter than its offset fields",
                        Some(cursor),
                    ));
                }
                let fields = read_complete_bytes(
                    storage,
                    payload_offset,
                    8,
                    "read_aiff_sound_fields",
                    operation,
                )?;
                let offset = u64::from(be_u32(&fields[0..4]));
                let block_size = u64::from(be_u32(&fields[4..8]));
                if offset > size - 8 {
                    return Err(corrupt(
                        "parse_aiff",
                        "AIFF sound-data offset extends past the Sound Data Chunk",
                        Some(cursor),
                    ));
                }
                let data_offset = payload_offset
                    .checked_add(8)
                    .and_then(|value| value.checked_add(offset))
                    .ok_or_else(|| {
                        corrupt(
                            "parse_aiff",
                            "AIFF sound-data offset overflowed",
                            Some(cursor),
                        )
                    })?;
                if offset != 0 {
                    let offset_data = read_ancillary(
                        storage,
                        payload_offset + 8,
                        offset,
                        "read_aiff_sound_offset_data",
                        operation,
                    )?;
                    metadata.insert(
                        "container.aiff.ssnd_offset_data",
                        MetadataValue::Bytes(Arc::from(offset_data)),
                    )?;
                }
                sound = Some((data_offset, size - 8 - offset, offset, block_size));
            }
            _ => {
                ancillary_budget.reserve(
                    &mut ancillary_chunks,
                    size,
                    "preserve_aiff_ancillary_chunk",
                )?;
                let bytes =
                    read_ancillary(storage, payload_offset, size, "read_aiff_chunk", operation)?;
                match id {
                    [b'N', b'A', b'M', b'E'] => {
                        insert_text(&mut metadata, "container.aiff.name", &bytes)?;
                    }
                    [b'A', b'U', b'T', b'H'] => {
                        insert_text(&mut metadata, "container.aiff.author", &bytes)?;
                    }
                    [b'A', b'N', b'N', b'O'] => {
                        insert_text(&mut metadata, "container.aiff.annotation", &bytes)?;
                    }
                    _ => {}
                }
                ancillary_chunks.push(AncillaryChunk {
                    id,
                    payload_offset,
                    data: Arc::from(bytes),
                });
            }
        }
        cursor = next_chunk(cursor, size, container_end, "parse_aiff")?;
    }
    operation.check("parse_aiff")?;
    if cursor != container_end {
        return Err(corrupt(
            "parse_aiff",
            "AIFF chunks do not end at the declared container boundary",
            Some(cursor),
        ));
    }

    let (format, frame_count) = common.ok_or_else(|| {
        corrupt(
            "parse_aiff",
            "AIFF is missing its required Common Chunk",
            None,
        )
    })?;
    let (data_offset, data_size, ssnd_offset, block_size) = sound.ok_or_else(|| {
        corrupt(
            "parse_aiff",
            "AIFF is missing its required Sound Data Chunk",
            None,
        )
    })?;
    let expected_size = frame_count
        .checked_mul(u64::from(format.block_align))
        .ok_or_else(|| corrupt("parse_aiff", "AIFF audio size overflowed", None))?;
    if data_size != expected_size {
        return Err(corrupt(
            "parse_aiff",
            "AIFF frame count contradicts the Sound Data Chunk size",
            Some(data_offset),
        ));
    }
    metadata.insert(
        "container.aiff.ssnd_offset",
        MetadataValue::Unsigned(ssnd_offset),
    )?;
    metadata.insert(
        "container.aiff.ssnd_block_size",
        MetadataValue::Unsigned(block_size),
    )?;
    validate_presentation_range(0, frame_count, "parse_aiff")?;

    Ok(ParsedContainer {
        kind: PcmContainerKind::Aiff,
        format,
        data_offset,
        data_size,
        frame_count,
        presentation_origin: 0,
        ancillary_chunks,
        metadata,
    })
}

fn parse_aiff_common(bytes: &[u8], chunk_offset: u64) -> Result<(PcmStreamFormat, u64)> {
    if bytes.len() != AIFF_COMMON_BYTES as usize {
        return Err(corrupt(
            "parse_aiff_common",
            "AIFF Common Chunk must match its fixed schema size",
            Some(chunk_offset),
        ));
    }
    let channel_count = be_u16(&bytes[0..2]);
    let frame_count = u64::from(be_u32(&bytes[2..6]));
    let valid_bits_per_sample = be_u16(&bytes[6..8]);
    let sample_rate = parse_extended_sample_rate(&bytes[8..18], chunk_offset)?;
    if valid_bits_per_sample == 0 || valid_bits_per_sample > 32 {
        return Err(unsupported(
            "parse_aiff_common",
            "AIFF integer sample precision must be between 1 and 32 bits",
        ));
    }
    let bits_per_sample = valid_bits_per_sample.checked_add(7).ok_or_else(|| {
        corrupt(
            "parse_aiff_common",
            "AIFF sample width overflowed",
            Some(chunk_offset),
        )
    })? / 8
        * 8;
    let bytes_per_sample = bits_per_sample / 8;
    let block_align = channel_count.checked_mul(bytes_per_sample).ok_or_else(|| {
        corrupt(
            "parse_aiff_common",
            "AIFF block alignment overflowed",
            Some(chunk_offset),
        )
    })?;
    validate_pcm_fields(
        PcmEncoding::Integer,
        channel_count,
        sample_rate,
        bits_per_sample,
        valid_bits_per_sample,
        block_align,
        None,
        chunk_offset,
    )?;
    Ok((
        PcmStreamFormat {
            encoding: PcmEncoding::Integer,
            byte_order: ByteOrder::BigEndian,
            sample_rate,
            bits_per_sample,
            valid_bits_per_sample,
            block_align,
            channel_layout: default_channel_layout(channel_count)?,
        },
        frame_count,
    ))
}

fn parse_extended_sample_rate(bytes: &[u8], chunk_offset: u64) -> Result<u32> {
    let sign_exponent = be_u16(&bytes[0..2]);
    if sign_exponent & 0x8000 != 0 {
        return Err(corrupt(
            "parse_aiff_sample_rate",
            "AIFF sample rate must be positive",
            Some(chunk_offset),
        ));
    }
    let exponent = i32::from(sign_exponent & 0x7fff);
    let mantissa = be_u64(&bytes[2..10]);
    if exponent == 0 || exponent == 0x7fff || mantissa & (1_u64 << 63) == 0 {
        return Err(corrupt(
            "parse_aiff_sample_rate",
            "AIFF sample rate is zero, non-finite, or non-normalized",
            Some(chunk_offset),
        ));
    }
    let shift = exponent - 16_383 - 63;
    let value = if shift >= 0 {
        u128::from(mantissa)
            .checked_shl(shift as u32)
            .ok_or_else(|| {
                corrupt(
                    "parse_aiff_sample_rate",
                    "AIFF sample rate overflowed",
                    Some(chunk_offset),
                )
            })?
    } else {
        let right = u32::try_from(-shift).expect("negative shift magnitude");
        if right >= 64 || mantissa & ((1_u64 << right) - 1) != 0 {
            return Err(unsupported(
                "parse_aiff_sample_rate",
                "fractional AIFF sample rates cannot map to the shared integer audio timebase",
            ));
        }
        u128::from(mantissa >> right)
    };
    u32::try_from(value)
        .ok()
        .filter(|value| *value != 0)
        .ok_or_else(|| {
            corrupt(
                "parse_aiff_sample_rate",
                "AIFF sample rate is outside the supported integer range",
                Some(chunk_offset),
            )
        })
}

#[allow(clippy::too_many_arguments)]
fn validate_pcm_fields(
    encoding: PcmEncoding,
    channel_count: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    valid_bits_per_sample: u16,
    block_align: u16,
    average_bytes_per_second: Option<u32>,
    chunk_offset: u64,
) -> Result<()> {
    if channel_count == 0 || sample_rate == 0 {
        return Err(corrupt(
            "validate_pcm_format",
            "PCM channel count and sample rate must be nonzero",
            Some(chunk_offset),
        ));
    }
    let width_supported = match encoding {
        PcmEncoding::Integer => matches!(bits_per_sample, 8 | 16 | 24 | 32),
        PcmEncoding::Float => matches!(bits_per_sample, 32 | 64),
    };
    if !width_supported {
        return Err(unsupported(
            "validate_pcm_format",
            "PCM sample width is not supported by the shared sample representation",
        ));
    }
    if valid_bits_per_sample == 0 || valid_bits_per_sample > bits_per_sample {
        return Err(corrupt(
            "validate_pcm_format",
            "valid PCM precision exceeds its sample container",
            Some(chunk_offset),
        ));
    }
    if encoding == PcmEncoding::Float && valid_bits_per_sample != bits_per_sample {
        return Err(corrupt(
            "validate_pcm_format",
            "floating-point PCM precision must equal its sample container",
            Some(chunk_offset),
        ));
    }
    let expected_align = channel_count
        .checked_mul(bits_per_sample / 8)
        .ok_or_else(|| {
            corrupt(
                "validate_pcm_format",
                "PCM block alignment overflowed",
                None,
            )
        })?;
    if block_align == 0 || block_align != expected_align {
        return Err(corrupt(
            "validate_pcm_format",
            "PCM block alignment contradicts channel count and sample width",
            Some(chunk_offset),
        ));
    }
    if let Some(average) = average_bytes_per_second {
        let expected = sample_rate
            .checked_mul(u32::from(block_align))
            .ok_or_else(|| {
                corrupt(
                    "validate_pcm_format",
                    "PCM average byte rate overflowed",
                    Some(chunk_offset),
                )
            })?;
        if average != expected {
            return Err(corrupt(
                "validate_pcm_format",
                "PCM average byte rate contradicts sample rate and block alignment",
                Some(chunk_offset),
            ));
        }
    }
    Ok(())
}

fn wave_channel_layout(
    channel_count: u16,
    channel_mask: Option<u32>,
    chunk_offset: u64,
) -> Result<ChannelLayout> {
    let Some(mask) = channel_mask.filter(|mask| *mask != 0) else {
        return default_channel_layout(channel_count);
    };
    if mask.count_ones() != u32::from(channel_count) {
        return Err(corrupt(
            "parse_wave_channel_layout",
            "WAVE channel mask does not match the channel count",
            Some(chunk_offset),
        ));
    }
    let known = [
        ChannelPosition::FrontLeft,
        ChannelPosition::FrontRight,
        ChannelPosition::FrontCenter,
        ChannelPosition::LowFrequency,
        ChannelPosition::BackLeft,
        ChannelPosition::BackRight,
        ChannelPosition::FrontLeftOfCenter,
        ChannelPosition::FrontRightOfCenter,
        ChannelPosition::BackCenter,
        ChannelPosition::SideLeft,
        ChannelPosition::SideRight,
        ChannelPosition::TopCenter,
        ChannelPosition::TopFrontLeft,
        ChannelPosition::TopFrontCenter,
        ChannelPosition::TopFrontRight,
        ChannelPosition::TopBackLeft,
        ChannelPosition::TopBackCenter,
        ChannelPosition::TopBackRight,
    ];
    let mut positions = Vec::with_capacity(usize::from(channel_count));
    for bit in 0..32 {
        if mask & (1_u32 << bit) != 0 {
            positions.push(
                known
                    .get(bit)
                    .copied()
                    .unwrap_or(ChannelPosition::Discrete(bit as u16)),
            );
        }
    }
    ChannelLayout::new(positions)
}

fn default_channel_layout(channel_count: u16) -> Result<ChannelLayout> {
    match channel_count {
        1 => Ok(ChannelLayout::mono()),
        2 => Ok(ChannelLayout::stereo()),
        count => ChannelLayout::new((0..count).map(ChannelPosition::Discrete)),
    }
}

fn codec_id(kind: PcmContainerKind, format: &PcmStreamFormat) -> Result<CodecId> {
    let code = match (format.encoding, format.bits_per_sample, kind) {
        (PcmEncoding::Integer, 8, PcmContainerKind::Wave) => "pcm_u8",
        (PcmEncoding::Integer, 8, PcmContainerKind::Aiff) => "pcm_s8",
        (PcmEncoding::Integer, 16, PcmContainerKind::Wave) => "pcm_s16le",
        (PcmEncoding::Integer, 16, PcmContainerKind::Aiff) => "pcm_s16be",
        (PcmEncoding::Integer, 24, PcmContainerKind::Wave) => "pcm_s24le",
        (PcmEncoding::Integer, 24, PcmContainerKind::Aiff) => "pcm_s24be",
        (PcmEncoding::Integer, 32, PcmContainerKind::Wave) => "pcm_s32le",
        (PcmEncoding::Integer, 32, PcmContainerKind::Aiff) => "pcm_s32be",
        (PcmEncoding::Float, 32, PcmContainerKind::Wave) => "pcm_f32le",
        (PcmEncoding::Float, 64, PcmContainerKind::Wave) => "pcm_f64le",
        _ => {
            return Err(unsupported(
                "create_pcm_codec_id",
                "container PCM representation has no supported codec identifier",
            ));
        }
    };
    CodecId::new(code)
}

fn add_format_metadata_to_stream(
    mut stream: StreamInfo,
    format: &PcmStreamFormat,
) -> Result<StreamInfo> {
    stream = stream.with_metadata(
        "audio.sample_rate",
        MetadataValue::Unsigned(u64::from(format.sample_rate)),
    )?;
    stream = stream.with_metadata(
        "audio.channel_count",
        MetadataValue::Unsigned(format.channel_layout.len() as u64),
    )?;
    stream = stream.with_metadata(
        "audio.channel_layout",
        MetadataValue::Text(channel_layout_code(&format.channel_layout)),
    )?;
    stream = stream.with_metadata(
        "audio.bits_per_sample",
        MetadataValue::Unsigned(u64::from(format.bits_per_sample)),
    )?;
    stream = stream.with_metadata(
        "audio.valid_bits_per_sample",
        MetadataValue::Unsigned(u64::from(format.valid_bits_per_sample)),
    )?;
    stream = stream.with_metadata(
        "audio.block_align",
        MetadataValue::Unsigned(u64::from(format.block_align)),
    )?;
    stream = stream.with_metadata(
        "audio.encoding",
        MetadataValue::Text(format.encoding.code().into()),
    )?;
    stream.with_metadata(
        "audio.byte_order",
        MetadataValue::Text(format.byte_order.code().into()),
    )
}

fn add_container_metadata(mut info: SourceInfo, parsed: &ParsedContainer) -> Result<SourceInfo> {
    info = info.with_metadata(
        "container.format",
        MetadataValue::Text(parsed.kind.code().into()),
    )?;
    info = info.with_metadata(
        "container.data_offset",
        MetadataValue::Unsigned(parsed.data_offset),
    )?;
    info = info.with_metadata(
        "container.data_size",
        MetadataValue::Unsigned(parsed.data_size),
    )?;
    info = info.with_metadata(
        "container.frame_count",
        MetadataValue::Unsigned(parsed.frame_count),
    )?;
    info = info.with_metadata(
        "container.presentation_origin",
        MetadataValue::Unsigned(parsed.presentation_origin),
    )?;
    for (key, value) in parsed.metadata.iter() {
        info = info.with_metadata(key, value.clone())?;
    }
    Ok(info)
}

fn channel_layout_code(layout: &ChannelLayout) -> String {
    layout
        .positions()
        .iter()
        .map(|position| match position {
            ChannelPosition::FrontLeft => "front_left".into(),
            ChannelPosition::FrontRight => "front_right".into(),
            ChannelPosition::FrontCenter => "front_center".into(),
            ChannelPosition::LowFrequency => "low_frequency".into(),
            ChannelPosition::BackLeft => "back_left".into(),
            ChannelPosition::BackRight => "back_right".into(),
            ChannelPosition::FrontLeftOfCenter => "front_left_of_center".into(),
            ChannelPosition::FrontRightOfCenter => "front_right_of_center".into(),
            ChannelPosition::BackCenter => "back_center".into(),
            ChannelPosition::SideLeft => "side_left".into(),
            ChannelPosition::SideRight => "side_right".into(),
            ChannelPosition::TopCenter => "top_center".into(),
            ChannelPosition::TopFrontLeft => "top_front_left".into(),
            ChannelPosition::TopFrontCenter => "top_front_center".into(),
            ChannelPosition::TopFrontRight => "top_front_right".into(),
            ChannelPosition::TopBackLeft => "top_back_left".into(),
            ChannelPosition::TopBackCenter => "top_back_center".into(),
            ChannelPosition::TopBackRight => "top_back_right".into(),
            ChannelPosition::Discrete(index) => format!("discrete:{index}"),
            _ => "unrecognized".into(),
        })
        .collect::<Vec<String>>()
        .join(",")
}

fn fingerprint(storage: &mut Storage, operation: &OperationContext) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut offset = 0_u64;
    while offset < storage.len() {
        operation.check("fingerprint_pcm_source")?;
        let size = (64 * 1024).min(storage.len() - offset);
        let bytes =
            read_complete_bytes(storage, offset, size, "fingerprint_pcm_source", operation)?;
        hasher.update(bytes);
        offset += size;
    }
    operation.check("fingerprint_pcm_source")?;
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn read_ancillary(
    storage: &mut Storage,
    offset: u64,
    size: u64,
    operation_name: &'static str,
    operation: &OperationContext,
) -> Result<Vec<u8>> {
    if size > MAX_ANCILLARY_CHUNK_BYTES {
        return Err(resource_exhausted(
            operation_name,
            "ancillary metadata chunk exceeds the bounded preservation limit",
        ));
    }
    read_complete_bytes(storage, offset, size, operation_name, operation)
}

fn read_complete_bytes(
    storage: &mut Storage,
    offset: u64,
    size: u64,
    operation_name: &'static str,
    operation: &OperationContext,
) -> Result<Vec<u8>> {
    match storage.read_bytes(offset, size, operation_name, operation)? {
        ReadOutcome::Complete(bytes) => Ok(bytes),
        ReadOutcome::Partial { report, .. } => Err(report.to_error(operation_name)),
        ReadOutcome::EndOfStream => {
            let expected = usize::try_from(size).map_err(|_| {
                resource_exhausted(operation_name, "byte range is too large to report")
            })?;
            Err(CorruptionReport::truncated(offset, expected, 0)?.to_error(operation_name))
        }
    }
}

fn next_chunk(
    chunk_offset: u64,
    payload_size: u64,
    container_end: u64,
    operation: &'static str,
) -> Result<u64> {
    let next = chunk_offset
        .checked_add(8)
        .and_then(|value| value.checked_add(payload_size))
        .and_then(|value| value.checked_add(payload_size & 1))
        .ok_or_else(|| {
            corrupt(
                operation,
                "container chunk range overflowed",
                Some(chunk_offset),
            )
        })?;
    if next > container_end {
        return Err(corrupt(
            operation,
            "container chunk extends past the declared boundary",
            Some(chunk_offset),
        ));
    }
    Ok(next)
}

fn validate_container_end(
    storage: &Storage,
    container_end: u64,
    operation: &'static str,
) -> Result<()> {
    if container_end < 12 || container_end > storage.len() {
        return Err(corrupt(
            operation,
            "declared container size extends past the available source",
            Some(4),
        ));
    }
    Ok(())
}

fn validate_presentation_range(origin: u64, frames: u64, operation: &'static str) -> Result<()> {
    if origin
        .checked_add(frames)
        .map_or(true, |end| end > i64::MAX as u64)
    {
        return Err(corrupt(
            operation,
            "PCM presentation range cannot be represented by shared packet timestamps",
            None,
        ));
    }
    Ok(())
}

fn insert_text(metadata: &mut MediaMetadata, key: &str, bytes: &[u8]) -> Result<()> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let text = String::from_utf8_lossy(&bytes[..end]).trim().to_owned();
    if !text.is_empty() {
        metadata.insert(key, MetadataValue::Text(text))?;
    }
    Ok(())
}

fn le_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes.try_into().expect("two-byte integer"))
}

fn le_i16(bytes: &[u8]) -> i16 {
    i16::from_le_bytes(bytes.try_into().expect("two-byte integer"))
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("four-byte integer"))
}

fn le_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("eight-byte integer"))
}

fn be_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes(bytes.try_into().expect("two-byte integer"))
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("four-byte integer"))
}

fn be_u64(bytes: &[u8]) -> u64 {
    u64::from_be_bytes(bytes.try_into().expect("eight-byte integer"))
}

fn open_error(error: io::Error, operation: &'static str) -> Error {
    let (category, recoverability) = match error.kind() {
        io::ErrorKind::NotFound => (ErrorCategory::NotFound, Recoverability::UserCorrectable),
        io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
        ),
        _ => (ErrorCategory::Unavailable, Recoverability::Retryable),
    };
    Error::with_source(
        category,
        recoverability,
        "audio container could not be opened",
        error,
    )
    .with_context(ErrorContext::new("superi-media-io.pcm", operation))
}

fn read_error(error: io::Error, operation: &'static str, offset: u64) -> Error {
    let (category, recoverability, message) = if error.kind() == io::ErrorKind::UnexpectedEof {
        (
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "audio container ended during a declared byte range",
        )
    } else {
        (
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "audio container could not be read",
        )
    };
    Error::with_source(category, recoverability, message, error).with_context(
        ErrorContext::new("superi-media-io.pcm", operation)
            .with_field("byte_offset", offset.to_string()),
    )
}

fn corrupt(operation: &'static str, message: &'static str, offset: Option<u64>) -> Error {
    let mut context = ErrorContext::new("superi-media-io.pcm", operation);
    if let Some(offset) = offset {
        context.insert_field("byte_offset", offset.to_string());
    }
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context)
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-media-io.pcm", operation))
}

fn unsupported_backend(operation: &'static str, capability: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        "the WAV and AIFF container backend does not implement PCM codec processing",
    )
    .with_context(
        ErrorContext::new("superi-media-io.pcm", operation).with_field("capability", capability),
    )
}

fn resource_exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-media-io.pcm", operation))
}

#[cfg(test)]
mod tests {
    use super::{
        parse_container_with_limits, AncillaryBudget, AncillaryChunk, AncillaryLimits, Storage,
    };
    use crate::operation::{MediaPriority, OperationContext};
    use std::sync::Arc;
    use superi_core::error::ErrorCategory;

    fn operation() -> OperationContext {
        OperationContext::new(MediaPriority::Interactive)
    }

    fn chunk(id: [u8; 4], data: &[u8], little_endian: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&id);
        let size = data.len() as u32;
        let encoded_size = if little_endian {
            size.to_le_bytes()
        } else {
            size.to_be_bytes()
        };
        bytes.extend_from_slice(&encoded_size);
        bytes.extend_from_slice(data);
        if data.len() & 1 == 1 {
            bytes.push(0);
        }
        bytes
    }

    fn container(form: [u8; 4], kind: [u8; 4], chunks: Vec<Vec<u8>>) -> Vec<u8> {
        let little_endian = form == *b"RIFF";
        let mut body = kind.to_vec();
        for chunk in chunks {
            body.extend_from_slice(&chunk);
        }
        let mut bytes = form.to_vec();
        let size = body.len() as u32;
        let encoded_size = if little_endian {
            size.to_le_bytes()
        } else {
            size.to_be_bytes()
        };
        bytes.extend_from_slice(&encoded_size);
        bytes.extend_from_slice(&body);
        bytes
    }

    fn parse_with_limits(bytes: Vec<u8>, limits: AncillaryLimits) -> ErrorCategory {
        let mut storage = Storage::Memory(Arc::from(bytes));
        match parse_container_with_limits(&mut storage, &operation(), limits) {
            Ok(_) => panic!("hostile ancillary input unexpectedly parsed"),
            Err(error) => error.category(),
        }
    }

    #[test]
    fn wave_and_aiff_reject_hostile_ancillary_counts() {
        let limits = AncillaryLimits::new(2, u64::MAX);
        let wave = container(
            *b"RIFF",
            *b"WAVE",
            vec![
                chunk(*b"JUNK", &[], true),
                chunk(*b"JUNK", &[], true),
                chunk(*b"JUNK", &[], true),
            ],
        );
        let aiff = container(
            *b"FORM",
            *b"AIFF",
            vec![
                chunk(*b"NAME", &[], false),
                chunk(*b"AUTH", &[], false),
                chunk(*b"ANNO", &[], false),
            ],
        );
        assert_eq!(
            parse_with_limits(wave, limits),
            ErrorCategory::ResourceExhausted
        );
        assert_eq!(
            parse_with_limits(aiff, limits),
            ErrorCategory::ResourceExhausted
        );
    }

    #[test]
    fn wave_and_aiff_reject_hostile_aggregate_ancillary_bytes() {
        let limits = AncillaryLimits::new(8, 2);
        let wave = container(*b"RIFF", *b"WAVE", vec![chunk(*b"JUNK", b"abc", true)]);
        let aiff = container(*b"FORM", *b"AIFF", vec![chunk(*b"NAME", b"abc", false)]);
        assert_eq!(
            parse_with_limits(wave, limits),
            ErrorCategory::ResourceExhausted
        );
        assert_eq!(
            parse_with_limits(aiff, limits),
            ErrorCategory::ResourceExhausted
        );
    }

    #[test]
    fn ancillary_accounting_rejects_integer_overflow() {
        let limits = AncillaryLimits::new(usize::MAX, u64::MAX);

        let mut budget = AncillaryBudget::new(limits);
        budget.count = usize::MAX;
        let mut chunks: Vec<AncillaryChunk> = Vec::new();
        let error = budget
            .reserve(&mut chunks, 0, "test_ancillary_budget")
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::ResourceExhausted);

        let mut budget = AncillaryBudget::new(limits);
        budget.total_bytes = u64::MAX;
        let error = budget
            .reserve(&mut chunks, 1, "test_ancillary_budget")
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    }
}
