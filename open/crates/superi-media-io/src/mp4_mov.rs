//! In-tree MP4 and QuickTime MOV source probing and demuxing.

use std::cmp::Ordering;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_core::error::{
    Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt as _,
};
use superi_core::time::{Duration, RationalTime, TimeRounding, Timebase};

use crate::backend::{BackendDescriptor, MediaBackend};
use crate::decode::{Decoder, DecoderConfig};
use crate::demux::{
    BackendId, CodecId, ContainerId, MediaSource, MetadataValue, Packet, PacketTiming,
    ProbeConfidence, SeekMode, SeekRequest, SourceIdentity, SourceInfo, SourceLocation,
    SourceProbe, SourceProbeResult, SourceRequest, StreamEdit, StreamId, StreamInfo, StreamKind,
};
use crate::encode::{Encoder, EncoderConfig};
use crate::mp4_parser::{self, ParsedMetadata, ParsedMovie, ParsedSample, ParsedTrack};
use crate::operation::OperationContext;
use crate::read::ReadOutcome;

const COMPONENT: &str = "superi-media-io.mp4-mov";

/// The in-tree container backend for MP4 and QuickTime MOV sources.
pub struct Mp4MovBackend {
    descriptor: BackendDescriptor,
    mp4_container: ContainerId,
    mov_container: ContainerId,
}

impl Mp4MovBackend {
    /// Creates the MP4 and MOV source backend.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(
                BackendId::new("mp4-mov")?,
                "Superi MP4 and MOV demuxer",
            )?,
            mp4_container: ContainerId::new("mp4")?,
            mov_container: ContainerId::new("mov")?,
        })
    }

    fn container_id(&self, kind: ContainerKind) -> ContainerId {
        match kind {
            ContainerKind::Mp4 => self.mp4_container.clone(),
            ContainerKind::Mov => self.mov_container.clone(),
        }
    }
}

impl MediaBackend for Mp4MovBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_mp4_mov_source")?;
        match inspect_container_prefix(probe.bytes(), probe.source_length(), probe.is_complete()) {
            ProbeDecision::NoMatch => Ok(SourceProbeResult::NoMatch),
            ProbeDecision::NeedMoreData(minimum_bytes) => {
                SourceProbeResult::need_more_data(minimum_bytes)
            }
            ProbeDecision::Match(kind, confidence) => Ok(SourceProbeResult::matched(
                self.container_id(kind),
                ProbeConfidence::new(confidence)?,
            )),
        }
    }

    fn open_source(
        &self,
        request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_mp4_mov_source")?;
        let data = read_source(request.location(), operation)?;
        let fingerprint = sha256_fingerprint_interruptible(&data, operation)?;
        if let Some(expected) = request.expected_fingerprint() {
            if expected != fingerprint {
                return Err(Error::new(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "media content does not match the expected relink fingerprint",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "verify_relink")
                        .with_field("media_id", format!("{:032x}", request.media_id().raw()))
                        .with_field("expected_fingerprint", expected)
                        .with_field("actual_fingerprint", &fingerprint),
                ));
            }
        }

        operation.check("open_mp4_mov_source")?;
        let kind = detect_complete_container(&data)?;
        operation.check("parse_mp4_mov_source")?;
        let parsed = catch_unwind(AssertUnwindSafe(|| mp4_parser::parse(&data)))
            .map_err(|_| {
                corrupt(
                    "parse_container",
                    "container tables caused an invalid parser state",
                )
            })?
            .map_err(map_parser_error)?;
        operation.check("parse_mp4_mov_source")?;

        let source = Mp4MovSource::from_parsed(
            request.media_id(),
            fingerprint,
            kind,
            Arc::clone(&data),
            parsed,
            operation,
        )?;
        Ok(Box::new(source))
    }

    fn create_decoder(
        &self,
        _config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_mp4_mov_decoder")?;
        Err(unsupported_operation("create_decoder", "decode"))
    }

    fn create_encoder(
        &self,
        _config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_mp4_mov_encoder")?;
        Err(unsupported_operation("create_encoder", "encode"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContainerKind {
    Mp4,
    Mov,
}

impl ContainerKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
        }
    }
}

enum ProbeDecision {
    NoMatch,
    NeedMoreData(usize),
    Match(ContainerKind, u8),
}

fn inspect_container_prefix(bytes: &[u8], source_length: u64, complete: bool) -> ProbeDecision {
    if bytes.len() < 8 {
        return if complete {
            ProbeDecision::NoMatch
        } else {
            ProbeDecision::NeedMoreData(8)
        };
    }

    let mut offset = 0_usize;
    let mut saw_moov = false;
    let mut saw_mdat = false;
    while offset.saturating_add(8) <= bytes.len() {
        let size32 = u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("four bytes"));
        let kind: [u8; 4] = bytes[offset + 4..offset + 8]
            .try_into()
            .expect("four bytes");
        let (header_size, atom_size) = if size32 == 1 {
            if bytes.len() < offset.saturating_add(16) {
                return if complete {
                    ProbeDecision::NoMatch
                } else {
                    ProbeDecision::NeedMoreData(offset.saturating_add(16))
                };
            }
            (
                16_u64,
                u64::from_be_bytes(
                    bytes[offset + 8..offset + 16]
                        .try_into()
                        .expect("eight bytes"),
                ),
            )
        } else if size32 == 0 {
            (8_u64, source_length.saturating_sub(offset as u64))
        } else {
            (8_u64, u64::from(size32))
        };

        if atom_size < header_size {
            return ProbeDecision::NoMatch;
        }
        let Some(end_u64) = (offset as u64).checked_add(atom_size) else {
            return ProbeDecision::NoMatch;
        };
        if end_u64 > source_length {
            return ProbeDecision::NoMatch;
        }

        if kind == *b"ftyp" {
            let needed = offset
                .saturating_add(usize::try_from(header_size).unwrap_or(usize::MAX))
                .saturating_add(8);
            if bytes.len() < needed {
                return if complete {
                    ProbeDecision::NoMatch
                } else {
                    ProbeDecision::NeedMoreData(needed)
                };
            }
            let payload = offset + usize::try_from(header_size).expect("small atom header");
            let major_brand: [u8; 4] = bytes[payload..payload + 4].try_into().expect("four bytes");
            if !valid_brand(major_brand) {
                return ProbeDecision::NoMatch;
            }
            if let Some(kind) = container_for_brand(major_brand) {
                return ProbeDecision::Match(kind, 100);
            }
            let end = usize::try_from(end_u64).unwrap_or(usize::MAX);
            if end > bytes.len() {
                return if complete {
                    ProbeDecision::NoMatch
                } else {
                    ProbeDecision::NeedMoreData(end)
                };
            }
            let compatible_start = payload + 8;
            for brand in bytes[compatible_start..end].chunks_exact(4) {
                let brand: [u8; 4] = brand.try_into().expect("four-byte brand chunk");
                if let Some(kind) = container_for_brand(brand) {
                    return ProbeDecision::Match(kind, 98);
                }
            }
            return ProbeDecision::NoMatch;
        }

        saw_moov |= kind == *b"moov";
        saw_mdat |= kind == *b"mdat";
        let end = usize::try_from(end_u64).unwrap_or(usize::MAX);
        if end > bytes.len() {
            if kind == *b"mdat" {
                return ProbeDecision::Match(ContainerKind::Mov, 65);
            }
            return if complete {
                ProbeDecision::NoMatch
            } else {
                ProbeDecision::NeedMoreData(end)
            };
        }
        if atom_size == 0 {
            break;
        }
        offset = end;
    }

    if saw_moov {
        ProbeDecision::Match(ContainerKind::Mov, 85)
    } else if saw_mdat {
        ProbeDecision::Match(ContainerKind::Mov, 65)
    } else if !complete && (offset as u64) < source_length {
        ProbeDecision::NeedMoreData(bytes.len().saturating_add(8))
    } else {
        ProbeDecision::NoMatch
    }
}

fn valid_brand(brand: [u8; 4]) -> bool {
    brand
        .into_iter()
        .all(|byte| byte == b' ' || byte.is_ascii_graphic())
}

fn container_for_brand(brand: [u8; 4]) -> Option<ContainerKind> {
    if brand == *b"qt  " {
        return Some(ContainerKind::Mov);
    }
    let iso_brand = brand[..3] == *b"iso" && brand[3].is_ascii_alphanumeric();
    let mobile_brand = brand[..3] == *b"3gp" || brand[..3] == *b"3g2";
    let named_brand = matches!(
        &brand,
        b"mp41"
            | b"mp42"
            | b"avc1"
            | b"dash"
            | b"cmfc"
            | b"cmfs"
            | b"M4V "
            | b"M4A "
            | b"F4V "
            | b"F4A "
            | b"MSNV"
    );
    (iso_brand || mobile_brand || named_brand).then_some(ContainerKind::Mp4)
}

fn detect_complete_container(bytes: &[u8]) -> Result<ContainerKind> {
    match inspect_container_prefix(bytes, bytes.len() as u64, true) {
        ProbeDecision::Match(kind, _) => Ok(kind),
        ProbeDecision::NoMatch | ProbeDecision::NeedMoreData(_) => Err(unsupported_container(
            "open_source",
            "source bytes are not a recognized MP4 or QuickTime MOV container",
        )),
    }
}

fn read_source(location: &SourceLocation, operation: &OperationContext) -> Result<Arc<[u8]>> {
    operation.check("read_mp4_mov_source")?;
    match location {
        SourceLocation::Memory { data, .. } => Ok(Arc::clone(data)),
        SourceLocation::Path(path) => {
            let mut file = File::open(path).map_err(source_read_error)?;
            let mut data = Vec::new();
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                operation.check("read_mp4_mov_source")?;
                match file.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => data.extend_from_slice(&buffer[..count]),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(source_read_error(error)),
                }
                operation.check("read_mp4_mov_source")?;
            }
            Ok(Arc::from(data))
        }
    }
}

fn source_read_error(source: io::Error) -> Error {
    let (category, recoverability) = match source.kind() {
        io::ErrorKind::NotFound => (ErrorCategory::NotFound, Recoverability::UserCorrectable),
        io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
        ),
        io::ErrorKind::InvalidInput => {
            (ErrorCategory::InvalidInput, Recoverability::UserCorrectable)
        }
        _ => (ErrorCategory::Unavailable, Recoverability::Retryable),
    };
    Error::with_source(
        category,
        recoverability,
        "media source could not be read",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, "read_source"))
}

fn sha256_fingerprint_interruptible(data: &[u8], operation: &OperationContext) -> Result<String> {
    let mut hasher = Sha256::new();
    for chunk in data.chunks(64 * 1024) {
        operation.check("fingerprint_mp4_mov_source")?;
        hasher.update(chunk);
    }
    operation.check("fingerprint_mp4_mov_source")?;
    Ok(format_fingerprint(hasher.finalize()))
}

#[cfg(test)]
fn sha256_fingerprint(data: &[u8]) -> String {
    format_fingerprint(Sha256::digest(data))
}

fn format_fingerprint(digest: impl AsRef<[u8]>) -> String {
    let digest = digest.as_ref();
    let mut fingerprint = String::with_capacity(7 + digest.len() * 2);
    fingerprint.push_str("sha256:");
    for byte in digest {
        write!(&mut fingerprint, "{byte:02x}").expect("writing to a String cannot fail");
    }
    fingerprint
}

fn map_parser_error(source: mp4_parser::ParseError) -> Error {
    Error::with_source(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        "MP4 or MOV container could not be parsed",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, "parse_container"))
}

struct Mp4MovSource {
    info: SourceInfo,
    data: Arc<[u8]>,
    tracks: Vec<TrackState>,
}

struct TrackState {
    id: StreamId,
    kind: StreamKind,
    timebase: Timebase,
    samples: Vec<ParsedSample>,
    cursor: usize,
}

impl Mp4MovSource {
    fn from_parsed(
        media_id: superi_core::ids::MediaId,
        fingerprint: String,
        container_kind: ContainerKind,
        data: Arc<[u8]>,
        parsed: ParsedMovie,
        operation: &OperationContext,
    ) -> Result<Self> {
        operation.check("build_mp4_mov_source")?;
        let movie_timescale = timebase(parsed.timescale, "movie_timescale")?;
        let source_duration = Duration::new(parsed.duration, movie_timescale)
            .with_error_context(ErrorContext::new(COMPONENT, "read_movie_duration"))?;

        let mut stream_infos = Vec::with_capacity(parsed.tracks.len());
        let mut tracks = Vec::with_capacity(parsed.tracks.len());
        for track in &parsed.tracks {
            operation.check("build_mp4_mov_source")?;
            validate_samples(track, &data)?;
            let stream_info = build_stream_info(track, movie_timescale)?;
            tracks.push(TrackState {
                id: stream_info.id(),
                kind: stream_info.kind(),
                timebase: stream_info.timebase(),
                samples: track.samples.clone(),
                cursor: 0,
            });
            stream_infos.push(stream_info);
        }

        let identity = SourceIdentity::new(media_id, fingerprint)?;
        let mut info = SourceInfo::new(identity, stream_infos)?.with_duration(source_duration);
        info = info.with_metadata(
            "container.kind",
            MetadataValue::Text(container_kind.as_str().to_owned()),
        )?;
        info = info.with_metadata(
            "container.major-brand",
            MetadataValue::Text(fourcc_text(parsed.major_brand)),
        )?;
        info = info.with_metadata(
            "container.minor-version",
            MetadataValue::Unsigned(u64::from(parsed.minor_version)),
        )?;
        info = info.with_metadata(
            "container.compatible-brands",
            MetadataValue::Text(
                parsed
                    .compatible_brands
                    .iter()
                    .map(|brand| fourcc_text(*brand))
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        )?;
        info = info.with_metadata(
            "container.creation-time",
            MetadataValue::Unsigned(parsed.creation_time),
        )?;
        info = info.with_metadata(
            "container.modification-time",
            MetadataValue::Unsigned(parsed.modification_time),
        )?;
        info = info.with_metadata(
            "container.movie-timescale",
            MetadataValue::Unsigned(u64::from(parsed.timescale)),
        )?;
        info = info.with_metadata(
            "container.fragmented",
            MetadataValue::Boolean(parsed.fragmented),
        )?;
        info = info.with_metadata(
            "container.event-message-count",
            MetadataValue::Unsigned(parsed.event_message_count as u64),
        )?;
        info = add_source_meta(info, &parsed.metadata)?;
        operation.check("build_mp4_mov_source")?;

        Ok(Self { info, data, tracks })
    }
}

impl MediaSource for Mp4MovSource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        operation.check("read_mp4_mov_packet")?;
        let Some(track_index) = self.next_track_index() else {
            return Ok(ReadOutcome::EndOfStream);
        };
        let track = &mut self.tracks[track_index];
        let sample = track.samples[track.cursor];

        let start = usize::try_from(sample.offset).map_err(|_| {
            corrupt(
                "read_packet",
                "sample offset cannot be represented on this platform",
            )
        })?;
        let end_u64 = sample.offset.checked_add(sample.size).ok_or_else(|| {
            corrupt(
                "read_packet",
                "sample byte range overflows the source address space",
            )
        })?;
        let end = usize::try_from(end_u64).map_err(|_| {
            corrupt(
                "read_packet",
                "sample end cannot be represented on this platform",
            )
        })?;
        let bytes = self
            .data
            .get(start..end)
            .ok_or_else(|| corrupt("read_packet", "sample byte range lies outside the source"))?;
        let timing = PacketTiming::new(
            track.timebase,
            Some(sample.composition_timestamp),
            Some(sample.decode_timestamp),
            Some(sample.duration),
        )?;
        let composition_offset = sample
            .composition_timestamp
            .checked_sub(sample.decode_timestamp)
            .ok_or_else(|| {
                corrupt(
                    "read_packet",
                    "sample composition offset exceeds the timestamp domain",
                )
            })?;
        let packet = Packet::new(track.id, Arc::from(bytes), timing)
            .with_keyframe(sample.is_sync)
            .with_metadata("container.offset", MetadataValue::Unsigned(sample.offset))?
            .with_metadata("container.size", MetadataValue::Unsigned(sample.size))?
            .with_metadata(
                "container.sample-id",
                MetadataValue::Unsigned(u64::from(sample.id)),
            )?
            .with_metadata(
                "container.composition-offset",
                MetadataValue::Signed(composition_offset),
            )?;
        operation.check("read_mp4_mov_packet")?;
        track.cursor += 1;
        Ok(ReadOutcome::Complete(packet))
    }

    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime> {
        operation.check("seek_mp4_mov_source")?;
        let anchor_index = self
            .tracks
            .iter()
            .position(|track| track.kind == StreamKind::Video && !track.samples.is_empty())
            .or_else(|| {
                self.tracks
                    .iter()
                    .position(|track| track.kind == StreamKind::Audio && !track.samples.is_empty())
            })
            .or_else(|| {
                self.tracks
                    .iter()
                    .position(|track| !track.samples.is_empty())
            })
            .ok_or_else(|| {
                unsupported_container("seek", "media source contains no seekable packets")
            })?;
        let anchor = &self.tracks[anchor_index];
        let selected = select_seek_sample(anchor, request)?;
        let actual = RationalTime::new(
            anchor.samples[selected].composition_timestamp,
            anchor.timebase,
        );

        let mut cursors = Vec::with_capacity(self.tracks.len());
        for (index, track) in self.tracks.iter().enumerate() {
            operation.check("seek_mp4_mov_source")?;
            let cursor = if index == anchor_index {
                selected
            } else {
                track
                    .samples
                    .iter()
                    .position(|sample| {
                        time_cmp(
                            RationalTime::new(sample.composition_timestamp, track.timebase),
                            actual,
                        ) != Ordering::Less
                    })
                    .unwrap_or(track.samples.len())
            };
            cursors.push(cursor);
        }
        operation.check("seek_mp4_mov_source")?;
        for (track, cursor) in self.tracks.iter_mut().zip(cursors) {
            track.cursor = cursor;
        }
        Ok(actual)
    }
}

impl Mp4MovSource {
    fn next_track_index(&self) -> Option<usize> {
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| track.cursor < track.samples.len())
            .min_by(|(_, left), (_, right)| {
                let left_sample = &left.samples[left.cursor];
                let right_sample = &right.samples[right.cursor];
                time_cmp(
                    RationalTime::new(left_sample.decode_timestamp, left.timebase),
                    RationalTime::new(right_sample.decode_timestamp, right.timebase),
                )
                .then_with(|| left.id.cmp(&right.id))
            })
            .map(|(index, _)| index)
    }
}

fn select_seek_sample(track: &TrackState, request: SeekRequest) -> Result<usize> {
    match request.mode() {
        SeekMode::Exact => track
            .samples
            .iter()
            .position(|sample| {
                RationalTime::new(sample.composition_timestamp, track.timebase) == request.target()
            })
            .ok_or_else(|| invalid_seek("exact seek target is not a packet boundary")),
        SeekMode::PreviousKeyframe => track
            .samples
            .iter()
            .enumerate()
            .filter(|(_, sample)| {
                sample.is_sync
                    && time_cmp(
                        RationalTime::new(sample.composition_timestamp, track.timebase),
                        request.target(),
                    ) != Ordering::Greater
            })
            .map(|(index, _)| index)
            .next_back()
            .ok_or_else(|| invalid_seek("no keyframe exists at or before the seek target")),
        SeekMode::NearestKeyframe => {
            let target = request
                .target()
                .checked_rescale(track.timebase, TimeRounding::NearestTiesEven)?
                .value();
            track
                .samples
                .iter()
                .enumerate()
                .filter(|(_, sample)| sample.is_sync)
                .min_by_key(|(_, sample)| {
                    (
                        sample.composition_timestamp.abs_diff(target),
                        sample.composition_timestamp,
                    )
                })
                .map(|(index, _)| index)
                .ok_or_else(|| invalid_seek("media source contains no keyframes"))
        }
    }
}

fn build_stream_info(track: &ParsedTrack, movie_timebase: Timebase) -> Result<StreamInfo> {
    let stream_timebase = timebase(track.timescale, "stream_timescale")?;
    let kind = stream_kind(track.handler_type);
    let codec = CodecId::new(codec_id(track.codec))?;
    let duration = Duration::new(track.duration, stream_timebase)
        .with_error_context(ErrorContext::new(COMPONENT, "read_stream_duration"))?;
    let edits = build_edits(track, movie_timebase, stream_timebase)?;
    let mut info = StreamInfo::new(StreamId::new(track.id), kind, codec, stream_timebase)
        .with_duration(duration)?
        .with_edits(edits)?;
    info = info.with_metadata(
        "container.handler-type",
        MetadataValue::Text(fourcc_text(track.handler_type)),
    )?;
    info = info.with_metadata(
        "container.handler-name",
        MetadataValue::Text(track.handler_name.clone()),
    )?;
    info = info.with_metadata(
        "container.sample-entry",
        MetadataValue::Text(fourcc_text(track.codec)),
    )?;
    info = info.with_metadata(
        "track.language",
        MetadataValue::Text(track.language.clone()),
    )?;
    info = info.with_metadata(
        "track.creation-time",
        MetadataValue::Unsigned(track.creation_time),
    )?;
    info = info.with_metadata(
        "track.modification-time",
        MetadataValue::Unsigned(track.modification_time),
    )?;
    info = info.with_metadata(
        "media.creation-time",
        MetadataValue::Unsigned(track.media_creation_time),
    )?;
    info = info.with_metadata(
        "media.modification-time",
        MetadataValue::Unsigned(track.media_modification_time),
    )?;
    info = info.with_metadata(
        "track.header-duration",
        MetadataValue::Unsigned(track.header_duration),
    )?;
    info = info.with_metadata(
        "track.flags",
        MetadataValue::Unsigned(u64::from(track.flags)),
    )?;
    info = info.with_metadata("track.layer", MetadataValue::Signed(i64::from(track.layer)))?;
    info = info.with_metadata(
        "track.alternate-group",
        MetadataValue::Unsigned(u64::from(track.alternate_group)),
    )?;
    info = info.with_metadata(
        "track.volume-fixed-8-8",
        MetadataValue::Unsigned(u64::from(track.volume)),
    )?;
    info = info.with_metadata(
        "track.matrix",
        MetadataValue::Text(format!(
            "{:#x} {:#x} {:#x} {:#x} {:#x} {:#x} {:#x} {:#x} {:#x}",
            track.matrix[0],
            track.matrix[1],
            track.matrix[2],
            track.matrix[3],
            track.matrix[4],
            track.matrix[5],
            track.matrix[6],
            track.matrix[7],
            track.matrix[8]
        )),
    )?;
    info = info.with_metadata(
        "track.sample-count",
        MetadataValue::Unsigned(track.samples.len() as u64),
    )?;
    if track.width != 0 {
        info = info.with_metadata(
            "video.width",
            MetadataValue::Unsigned(u64::from(track.width)),
        )?;
    }
    if track.height != 0 {
        info = info.with_metadata(
            "video.height",
            MetadataValue::Unsigned(u64::from(track.height)),
        )?;
    }
    if let Some(codec_string) = track.codec_string.as_ref() {
        info = info.with_metadata("codec.rfc6381", MetadataValue::Text(codec_string.clone()))?;
    }
    if let Some(config) = track.codec_configuration.as_ref() {
        info = info.with_metadata(
            "codec.configuration",
            MetadataValue::Bytes(Arc::from(config.clone())),
        )?;
    }
    info = add_stream_meta(info, &track.metadata)?;
    Ok(info)
}

fn build_edits(
    track: &ParsedTrack,
    movie_timebase: Timebase,
    stream_timebase: Timebase,
) -> Result<Vec<StreamEdit>> {
    track
        .edits
        .iter()
        .map(|entry| {
            let segment_duration = Duration::new(entry.segment_duration, movie_timebase)
                .with_error_context(ErrorContext::new(COMPONENT, "read_stream_edit"))?;
            Ok(StreamEdit::new(
                segment_duration,
                (entry.media_time != -1)
                    .then_some(RationalTime::new(entry.media_time, stream_timebase)),
                entry.rate_integer,
                entry.rate_fraction,
            ))
        })
        .collect()
}

fn validate_samples(track: &ParsedTrack, data: &[u8]) -> Result<()> {
    for sample in &track.samples {
        if sample.duration > i64::MAX as u64 {
            return Err(corrupt(
                "validate_samples",
                "sample timing is inconsistent with its stream",
            ));
        }
        let end = sample.offset.checked_add(sample.size).ok_or_else(|| {
            corrupt(
                "validate_samples",
                "sample byte range overflows the source address space",
            )
        })?;
        if end > data.len() as u64
            || usize::try_from(sample.offset).is_err()
            || usize::try_from(end).is_err()
        {
            return Err(corrupt(
                "validate_samples",
                "sample byte range lies outside the source",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "identify_sample")
                    .with_field("track_id", track.id.to_string())
                    .with_field("sample_id", sample.id.to_string()),
            ));
        }
    }
    Ok(())
}

fn codec_id(fourcc: [u8; 4]) -> String {
    match &fourcc {
        b"av01" => "av1".to_owned(),
        b"avc1" | b"avc2" | b"avc3" | b"avc4" => "h264".to_owned(),
        b"hvc1" | b"hev1" => "hevc".to_owned(),
        b"vp08" => "vp8".to_owned(),
        b"vp09" => "vp9".to_owned(),
        b"mp4a" => "aac".to_owned(),
        b"tx3g" => "tx3g".to_owned(),
        _ => format!("fourcc-{:08x}", u32::from_be_bytes(fourcc)),
    }
}

fn stream_kind(handler: [u8; 4]) -> StreamKind {
    match &handler {
        b"vide" => StreamKind::Video,
        b"soun" => StreamKind::Audio,
        b"sbtl" | b"subt" | b"text" | b"clcp" => StreamKind::Subtitle,
        _ => StreamKind::Data,
    }
}

fn add_source_meta(mut info: SourceInfo, meta: &ParsedMetadata) -> Result<SourceInfo> {
    for (key, value) in metadata_values(meta, "metadata") {
        info = info.with_metadata(key, value)?;
    }
    Ok(info)
}

fn add_stream_meta(mut info: StreamInfo, meta: &ParsedMetadata) -> Result<StreamInfo> {
    for (key, value) in metadata_values(meta, "track.metadata") {
        info = info.with_metadata(key, value)?;
    }
    Ok(info)
}

fn metadata_values(meta: &ParsedMetadata, prefix: &str) -> Vec<(String, MetadataValue)> {
    let mut values = Vec::new();
    if let Some(title) = meta.title.as_ref() {
        values.push((
            format!("{prefix}.title"),
            MetadataValue::Text(title.clone()),
        ));
    }
    if let Some(year) = meta.year {
        values.push((
            format!("{prefix}.year"),
            MetadataValue::Unsigned(u64::from(year)),
        ));
    }
    if let Some(summary) = meta.summary.as_ref() {
        values.push((
            format!("{prefix}.summary"),
            MetadataValue::Text(summary.clone()),
        ));
    }
    if let Some(poster) = meta.poster.as_ref() {
        values.push((
            format!("{prefix}.poster"),
            MetadataValue::Bytes(Arc::from(poster.clone())),
        ));
    }
    values
}

fn fourcc_text(value: [u8; 4]) -> String {
    String::from_utf8_lossy(&value).into_owned()
}

fn timebase(value: u32, field: &'static str) -> Result<Timebase> {
    if value == 0 {
        return Err(corrupt(
            "read_timebase",
            "container timescale must be greater than zero",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "identify_timebase").with_field("field", field),
        ));
    }
    Timebase::integer(value)
}

fn time_cmp(left: RationalTime, right: RationalTime) -> Ordering {
    left.partial_cmp(&right)
        .expect("validated timebases always have a total ordering")
}

fn invalid_seek(message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, "seek"))
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported_container(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported_operation(operation: &'static str, capability: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        "the MP4 and MOV container backend does not implement codec processing",
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("capability", capability))
}

#[cfg(test)]
mod tests {
    use super::sha256_fingerprint;

    #[test]
    fn content_fingerprint_uses_canonical_sha256() {
        assert_eq!(
            sha256_fingerprint(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
