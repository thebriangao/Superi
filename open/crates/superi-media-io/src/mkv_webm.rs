//! In-tree Matroska MKV and WebM source probing and demuxing.

use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap};
use std::fs::File;
use std::io::{self, Read};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{Duration, RationalTime, TimeRounding, Timebase};

use crate::backend::{BackendDescriptor, MediaBackend};
use crate::decode::{Decoder, DecoderConfig};
use crate::demux::{
    BackendId, CodecId, ContainerId, MediaSource, MetadataValue, Packet, PacketTiming,
    ProbeConfidence, SeekMode, SeekRequest, SourceIdentity, SourceInfo, SourceLocation,
    SourceProbe, SourceProbeResult, SourceRequest, StreamId, StreamInfo, StreamKind,
};
use crate::encode::{Encoder, EncoderConfig};
use crate::matroska_parser::{self, ParsedDocument, ParsedFrame, ParsedTrack, ProbeDecision};
use crate::operation::OperationContext;
use crate::read::ReadOutcome;

const COMPONENT: &str = "superi-media-io.mkv-webm";
const READ_CHUNK_BYTES: usize = 64 * 1024;
const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;

/// The in-tree container backend for Matroska MKV and WebM sources.
pub struct MkvWebmBackend {
    descriptor: BackendDescriptor,
    mkv_container: ContainerId,
    webm_container: ContainerId,
}

impl MkvWebmBackend {
    /// Creates the Matroska MKV and WebM source backend.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(
                BackendId::new("mkv-webm")?,
                "Superi MKV and WebM demuxer",
            )?,
            mkv_container: ContainerId::new("mkv")?,
            webm_container: ContainerId::new("webm")?,
        })
    }

    /// Returns stable backend identity.
    #[must_use]
    pub const fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }
}

impl MediaBackend for MkvWebmBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_mkv_webm_source")?;
        match matroska_parser::inspect_prefix(
            probe.bytes(),
            probe.source_length(),
            probe.is_complete(),
        ) {
            ProbeDecision::NoMatch => Ok(SourceProbeResult::NoMatch),
            ProbeDecision::NeedMoreData(minimum_bytes) => {
                SourceProbeResult::need_more_data(minimum_bytes)
            }
            ProbeDecision::Match(kind) => {
                let container = match kind {
                    matroska_parser::DocumentType::Matroska => self.mkv_container.clone(),
                    matroska_parser::DocumentType::Webm => self.webm_container.clone(),
                };
                Ok(SourceProbeResult::matched(
                    container,
                    ProbeConfidence::new(100)?,
                ))
            }
        }
    }

    fn open_source(
        &self,
        request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_mkv_webm_source")?;
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

        operation.check("parse_mkv_webm_source")?;
        let parsed = catch_unwind(AssertUnwindSafe(|| {
            matroska_parser::parse(&data, operation)
        }))
        .map_err(|_| {
            corrupt(
                "parse_container",
                "EBML elements caused an invalid parser state",
            )
        })??;
        operation.check("build_mkv_webm_source")?;
        Ok(Box::new(MkvWebmSource::from_parsed(
            request.media_id(),
            fingerprint,
            data,
            parsed,
            operation,
        )?))
    }

    fn create_decoder(
        &self,
        _config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_mkv_webm_decoder")?;
        Err(unsupported_operation("create_decoder", "decode"))
    }

    fn create_encoder(
        &self,
        _config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_mkv_webm_encoder")?;
        Err(unsupported_operation("create_encoder", "encode"))
    }
}

struct MkvWebmSource {
    info: SourceInfo,
    data: Arc<[u8]>,
    tracks: Vec<TrackState>,
    frontier: BinaryHeap<Reverse<PacketFrontier>>,
}

struct TrackState {
    id: StreamId,
    kind: StreamKind,
    frames: Vec<ParsedFrame>,
    cursor: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PacketFrontier {
    sort_timestamp_ns: i64,
    block_offset: u64,
    lace_index: usize,
    stream_id: u32,
    track_index: usize,
    frame_index: usize,
}

impl MkvWebmSource {
    fn from_parsed(
        media_id: superi_core::ids::MediaId,
        fingerprint: String,
        data: Arc<[u8]>,
        parsed: ParsedDocument,
        operation: &OperationContext,
    ) -> Result<Self> {
        let ParsedDocument {
            document_type,
            document_type_version,
            document_read_version,
            timestamp_scale_ns,
            duration_ticks,
            duration_ns,
            segment_uid,
            title,
            muxing_app,
            writing_app,
            tracks: parsed_tracks,
            frames,
            cue_point_count,
        } = parsed;
        validate_frames(&frames, &data, operation)?;
        let mut frames_by_track = BTreeMap::<u32, Vec<ParsedFrame>>::new();
        for frame in frames {
            operation.check("group_mkv_webm_frames")?;
            let track_frames = frames_by_track.entry(frame.track_number).or_default();
            track_frames.try_reserve(1).map_err(|_| {
                resource_exhausted("group_frames", "track frame allocation could not grow")
            })?;
            track_frames.push(frame);
        }

        let mut stream_infos = Vec::with_capacity(parsed_tracks.len());
        let mut tracks = Vec::with_capacity(parsed_tracks.len());
        let mut derived_source_duration = 0_u64;
        for track in &parsed_tracks {
            operation.check("build_mkv_webm_stream")?;
            let mut frames = frames_by_track.remove(&track.number).unwrap_or_default();
            frames.sort_unstable_by_key(|frame| {
                (
                    frame.sort_timestamp_ns,
                    frame.block_offset,
                    frame.lace_index,
                )
            });
            let stream_duration = frames.iter().filter_map(frame_end_ns).max();
            if let Some(duration) = stream_duration {
                derived_source_duration = derived_source_duration.max(duration);
            }
            let stream_info = build_stream_info(track, stream_duration)?;
            tracks.push(TrackState {
                id: stream_info.id(),
                kind: stream_info.kind(),
                frames,
                cursor: 0,
            });
            stream_infos.push(stream_info);
        }
        if !frames_by_track.is_empty() {
            return Err(corrupt(
                "build_source",
                "parsed frame refers to a missing track",
            ));
        }

        let identity = SourceIdentity::new(media_id, fingerprint)?;
        let mut info = SourceInfo::new(identity, stream_infos)?;
        if let Some(source_duration) =
            duration_ns.or((derived_source_duration > 0).then_some(derived_source_duration))
        {
            info = info.with_duration(Duration::new(source_duration, Timebase::NANOSECONDS)?);
        }
        info = info.with_metadata(
            "container.kind",
            MetadataValue::Text(document_type.container_id().to_owned()),
        )?;
        info = info.with_metadata(
            "container.doc-type-version",
            MetadataValue::Unsigned(document_type_version),
        )?;
        info = info.with_metadata(
            "container.doc-read-version",
            MetadataValue::Unsigned(document_read_version),
        )?;
        info = info.with_metadata(
            "container.timestamp-scale-ns",
            MetadataValue::Unsigned(timestamp_scale_ns),
        )?;
        info = info.with_metadata(
            "container.cue-point-count",
            MetadataValue::Unsigned(cue_point_count as u64),
        )?;
        if let Some(duration) = duration_ticks {
            info = info.with_metadata(
                "container.duration-ticks",
                MetadataValue::Text(duration.to_string()),
            )?;
        }
        if let Some(uid) = segment_uid {
            info = info.with_metadata(
                "container.segment-uid",
                MetadataValue::Bytes(Arc::from(uid)),
            )?;
        }
        if let Some(title) = title {
            info = info.with_metadata("container.title", MetadataValue::Text(title))?;
        }
        if let Some(app) = muxing_app {
            info = info.with_metadata("container.muxing-app", MetadataValue::Text(app))?;
        }
        if let Some(app) = writing_app {
            info = info.with_metadata("container.writing-app", MetadataValue::Text(app))?;
        }
        operation.check("build_mkv_webm_source")?;
        let mut source = Self {
            info,
            data,
            tracks,
            frontier: BinaryHeap::new(),
        };
        source.rebuild_frontier();
        Ok(source)
    }

    fn rebuild_frontier(&mut self) {
        self.frontier.clear();
        for (track_index, track) in self.tracks.iter().enumerate() {
            if let Some(entry) = frontier_entry(track_index, track) {
                self.frontier.push(Reverse(entry));
            }
        }
    }
}

fn frontier_entry(track_index: usize, track: &TrackState) -> Option<PacketFrontier> {
    let frame = track.frames.get(track.cursor)?;
    Some(PacketFrontier {
        sort_timestamp_ns: frame.sort_timestamp_ns,
        block_offset: frame.block_offset,
        lace_index: frame.lace_index,
        stream_id: track.id.value(),
        track_index,
        frame_index: track.cursor,
    })
}

impl MediaSource for MkvWebmSource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        operation.check("read_mkv_webm_packet")?;
        let Some(Reverse(entry)) = self.frontier.peek().copied() else {
            return Ok(ReadOutcome::EndOfStream);
        };
        let packet = {
            let track = self.tracks.get(entry.track_index).ok_or_else(|| {
                internal("read_packet", "packet frontier refers to a missing track")
            })?;
            let frame = track.frames.get(entry.frame_index).ok_or_else(|| {
                internal("read_packet", "packet frontier refers to a missing frame")
            })?;
            if track.cursor != entry.frame_index {
                return Err(internal(
                    "read_packet",
                    "packet frontier is not aligned with the track cursor",
                ));
            }
            let start = usize::try_from(frame.data_offset).map_err(|_| {
                corrupt(
                    "read_packet",
                    "frame offset cannot be represented on this platform",
                )
            })?;
            let end_u64 = frame
                .data_offset
                .checked_add(frame.size)
                .ok_or_else(|| corrupt("read_packet", "frame byte range overflowed"))?;
            let end = usize::try_from(end_u64).map_err(|_| {
                corrupt(
                    "read_packet",
                    "frame end cannot be represented on this platform",
                )
            })?;
            let bytes = self.data.get(start..end).ok_or_else(|| {
                corrupt("read_packet", "frame byte range lies outside the source")
            })?;
            let data = copy_packet_bytes(bytes, operation)?;
            let timing = PacketTiming::new(
                Timebase::NANOSECONDS,
                frame.presentation_ns,
                None,
                frame.duration_ns,
            )?;
            let mut packet = Packet::new(track.id, data, timing)
                .with_keyframe(frame.keyframe)
                .with_metadata(
                    "container.offset",
                    MetadataValue::Unsigned(frame.data_offset),
                )?
                .with_metadata("container.size", MetadataValue::Unsigned(frame.size))?
                .with_metadata(
                    "container.block-offset",
                    MetadataValue::Unsigned(frame.block_offset),
                )?
                .with_metadata(
                    "container.cluster-timestamp-ticks",
                    MetadataValue::Unsigned(frame.cluster_timestamp_ticks),
                )?
                .with_metadata(
                    "container.relative-timestamp-ticks",
                    MetadataValue::Signed(i64::from(frame.relative_timestamp_ticks)),
                )?
                .with_metadata(
                    "container.lace-index",
                    MetadataValue::Unsigned(frame.lace_index as u64),
                )?
                .with_metadata(
                    "container.lace-count",
                    MetadataValue::Unsigned(frame.lace_count as u64),
                )?
                .with_metadata(
                    "container.invisible",
                    MetadataValue::Boolean(frame.invisible),
                )?
                .with_metadata(
                    "container.discardable",
                    MetadataValue::Boolean(frame.discardable),
                )?;
            if let Some(duration) = frame.block_duration_ticks {
                packet = packet.with_metadata(
                    "container.block-duration-ticks",
                    MetadataValue::Unsigned(duration),
                )?;
            }
            if !frame.reference_blocks.is_empty() {
                let mut references = String::new();
                for (index, reference) in frame.reference_blocks.iter().enumerate() {
                    operation.check("materialize_mkv_webm_packet_metadata")?;
                    if index > 0 {
                        references.push(',');
                    }
                    references.push_str(&reference.to_string());
                }
                packet = packet.with_metadata(
                    "container.reference-blocks",
                    MetadataValue::Text(references),
                )?;
            }
            if let Some(padding) = frame.discard_padding_ns {
                packet = packet.with_metadata(
                    "container.discard-padding-ns",
                    MetadataValue::Signed(padding),
                )?;
            }
            if let Some(state) = frame.codec_state.as_ref() {
                operation.check("materialize_mkv_webm_packet_metadata")?;
                packet =
                    packet.with_metadata("codec.state", MetadataValue::Bytes(Arc::clone(state)))?;
            }
            for (index, (id, data)) in frame.block_additions.iter().enumerate() {
                operation.check("materialize_mkv_webm_packet_metadata")?;
                packet.metadata_mut().insert(
                    format!("container.block-addition.{index}.id"),
                    MetadataValue::Unsigned(*id),
                )?;
                packet.metadata_mut().insert(
                    format!("container.block-addition.{index}.data"),
                    MetadataValue::Bytes(Arc::clone(data)),
                )?;
            }
            packet
        };
        operation.check("read_mkv_webm_packet")?;
        let popped = self.frontier.pop();
        debug_assert_eq!(popped, Some(Reverse(entry)));
        let track = &mut self.tracks[entry.track_index];
        track.cursor += 1;
        if let Some(next) = frontier_entry(entry.track_index, track) {
            self.frontier.push(Reverse(next));
        }
        Ok(ReadOutcome::Complete(packet))
    }

    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime> {
        operation.check("seek_mkv_webm_source")?;
        let anchor_index = self
            .tracks
            .iter()
            .position(|track| track.kind == StreamKind::Video && !track.frames.is_empty())
            .or_else(|| {
                self.tracks
                    .iter()
                    .position(|track| track.kind == StreamKind::Audio && !track.frames.is_empty())
            })
            .or_else(|| {
                self.tracks
                    .iter()
                    .position(|track| !track.frames.is_empty())
            })
            .ok_or_else(|| {
                unsupported_container("seek", "media source contains no seekable packets")
            })?;
        let selected = select_seek_frame(&self.tracks[anchor_index], request)?;
        let actual_ns = self.tracks[anchor_index].frames[selected]
            .presentation_ns
            .expect("seek selection only returns frames with stored or derived timestamps");
        let actual = RationalTime::new(actual_ns, Timebase::NANOSECONDS);

        let mut cursors = Vec::with_capacity(self.tracks.len());
        for (index, track) in self.tracks.iter().enumerate() {
            operation.check("seek_mkv_webm_source")?;
            let cursor = if index == anchor_index {
                selected
            } else {
                track
                    .frames
                    .iter()
                    .position(|frame| frame.sort_timestamp_ns >= actual_ns)
                    .unwrap_or(track.frames.len())
            };
            cursors.push(cursor);
        }
        operation.check("seek_mkv_webm_source")?;
        for (track, cursor) in self.tracks.iter_mut().zip(cursors) {
            track.cursor = cursor;
        }
        self.rebuild_frontier();
        Ok(actual)
    }
}

fn select_seek_frame(track: &TrackState, request: SeekRequest) -> Result<usize> {
    match request.mode() {
        SeekMode::Exact => track
            .frames
            .iter()
            .position(|frame| {
                frame.presentation_ns.is_some_and(|value| {
                    time_cmp(
                        RationalTime::new(value, Timebase::NANOSECONDS),
                        request.target(),
                    ) == Ordering::Equal
                })
            })
            .ok_or_else(|| invalid_seek("exact seek target is not a packet boundary")),
        SeekMode::PreviousKeyframe => track
            .frames
            .iter()
            .enumerate()
            .filter(|(_, frame)| {
                frame.keyframe
                    && frame.presentation_ns.is_some_and(|value| {
                        time_cmp(
                            RationalTime::new(value, Timebase::NANOSECONDS),
                            request.target(),
                        ) != Ordering::Greater
                    })
            })
            .max_by_key(|(_, frame)| frame.presentation_ns)
            .map(|(index, _)| index)
            .ok_or_else(|| invalid_seek("no keyframe exists at or before the seek target")),
        SeekMode::NearestKeyframe => {
            let target = request
                .target()
                .checked_rescale(Timebase::NANOSECONDS, TimeRounding::NearestTiesEven)?
                .value();
            track
                .frames
                .iter()
                .enumerate()
                .filter_map(|(index, frame)| {
                    frame
                        .keyframe
                        .then_some(frame.presentation_ns)
                        .flatten()
                        .map(|timestamp| (index, timestamp))
                })
                .min_by_key(|(_, timestamp)| (timestamp.abs_diff(target), *timestamp))
                .map(|(index, _)| index)
                .ok_or_else(|| invalid_seek("media source contains no keyframes"))
        }
    }
}

fn build_stream_info(track: &ParsedTrack, duration_ns: Option<u64>) -> Result<StreamInfo> {
    let kind = stream_kind(track.track_type);
    let codec = CodecId::new(codec_id(&track.codec_id))?;
    let mut info = StreamInfo::new(
        StreamId::new(track.number),
        kind,
        codec,
        Timebase::NANOSECONDS,
    );
    if let Some(duration) = duration_ns {
        info = info.with_duration(Duration::new(duration, Timebase::NANOSECONDS)?)?;
    }
    info = info.with_metadata("track.uid", MetadataValue::Unsigned(track.uid))?;
    info = info.with_metadata(
        "track.codec-id",
        MetadataValue::Text(track.codec_id.clone()),
    )?;
    info = info.with_metadata("track.enabled", MetadataValue::Boolean(track.enabled))?;
    info = info.with_metadata("track.default", MetadataValue::Boolean(track.default))?;
    info = info.with_metadata("track.forced", MetadataValue::Boolean(track.forced))?;
    info = info.with_metadata("track.lacing", MetadataValue::Boolean(track.lacing))?;
    info = info.with_metadata(
        "track.language",
        MetadataValue::Text(track.language.clone()),
    )?;
    info = info.with_metadata(
        "codec.delay-ns",
        MetadataValue::Unsigned(track.codec_delay_ns),
    )?;
    info = info.with_metadata(
        "codec.seek-pre-roll-ns",
        MetadataValue::Unsigned(track.seek_pre_roll_ns),
    )?;
    if let Some(duration) = track.default_duration_ns {
        info = info.with_metadata(
            "track.default-duration-ns",
            MetadataValue::Unsigned(duration),
        )?;
    }
    if let Some(name) = track.name.as_ref() {
        info = info.with_metadata("track.name", MetadataValue::Text(name.clone()))?;
    }
    if let Some(private) = track.codec_private.as_ref() {
        info = info.with_metadata(
            "codec.configuration",
            MetadataValue::Bytes(Arc::clone(private)),
        )?;
    }
    for (key, value) in [
        ("video.pixel-width", track.video.pixel_width),
        ("video.pixel-height", track.video.pixel_height),
        ("video.display-width", track.video.display_width),
        ("video.display-height", track.video.display_height),
        ("video.stereo-mode", track.video.stereo_mode),
        ("video.alpha-mode", track.video.alpha_mode),
        ("audio.channels", track.audio.channels),
        ("audio.bit-depth", track.audio.bit_depth),
    ] {
        if let Some(value) = value {
            info = info.with_metadata(key, MetadataValue::Unsigned(value))?;
        }
    }
    for (key, value) in [
        ("audio.sampling-frequency", track.audio.sampling_frequency),
        (
            "audio.output-sampling-frequency",
            track.audio.output_sampling_frequency,
        ),
    ] {
        if let Some(value) = value {
            info = info.with_metadata(key, MetadataValue::Text(value.to_string()))?;
        }
    }
    Ok(info)
}

fn stream_kind(track_type: u64) -> StreamKind {
    match track_type {
        1 => StreamKind::Video,
        2 => StreamKind::Audio,
        17 => StreamKind::Subtitle,
        _ => StreamKind::Data,
    }
}

fn codec_id(matroska_id: &str) -> String {
    match matroska_id {
        "V_AV1" => "av1".to_owned(),
        "V_MPEG4/ISO/AVC" => "h264".to_owned(),
        "V_MPEGH/ISO/HEVC" => "hevc".to_owned(),
        "V_MPEGI/ISO/VVC" => "vvc".to_owned(),
        "V_VP8" => "vp8".to_owned(),
        "V_VP9" => "vp9".to_owned(),
        "A_AAC" => "aac".to_owned(),
        "A_MPEG/L3" => "mp3".to_owned(),
        "A_FLAC" => "flac".to_owned(),
        "A_VORBIS" => "vorbis".to_owned(),
        "A_OPUS" => "opus".to_owned(),
        "A_PCM/INT/LIT" => "pcm".to_owned(),
        _ => {
            let normalized = matroska_id
                .bytes()
                .map(|byte| {
                    if byte.is_ascii_alphanumeric() {
                        byte.to_ascii_lowercase() as char
                    } else {
                        '-'
                    }
                })
                .collect::<String>();
            format!("matroska-{normalized}")
        }
    }
}

fn frame_end_ns(frame: &ParsedFrame) -> Option<u64> {
    let start = u64::try_from(frame.presentation_ns?).ok()?;
    start.checked_add(frame.duration_ns.unwrap_or(0))
}

fn validate_frames(
    frames: &[ParsedFrame],
    data: &[u8],
    operation: &OperationContext,
) -> Result<()> {
    for frame in frames {
        operation.check("validate_mkv_webm_frames")?;
        let end = frame
            .data_offset
            .checked_add(frame.size)
            .ok_or_else(|| corrupt("validate_frames", "frame byte range overflowed"))?;
        if end > data.len() as u64
            || usize::try_from(frame.data_offset).is_err()
            || usize::try_from(end).is_err()
        {
            return Err(corrupt(
                "validate_frames",
                "frame byte range lies outside the source",
            ));
        }
    }
    Ok(())
}

fn read_source(location: &SourceLocation, operation: &OperationContext) -> Result<Arc<[u8]>> {
    operation.check("read_mkv_webm_source")?;
    match location {
        SourceLocation::Memory { data, .. } => {
            check_source_length(data.len() as u64)?;
            Ok(Arc::clone(data))
        }
        SourceLocation::Path(path) => {
            let mut file =
                File::open(path).map_err(|source| source_io_error("open_source", source))?;
            let expected = file
                .metadata()
                .map_err(|source| source_io_error("inspect_source", source))?
                .len();
            check_source_length(expected)?;
            let expected = usize::try_from(expected).map_err(|_| {
                resource_exhausted(
                    "read_source",
                    "source length cannot be represented on this platform",
                )
            })?;
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(expected).map_err(|_| {
                resource_exhausted("read_source", "source allocation could not be reserved")
            })?;
            let mut chunk = [0_u8; READ_CHUNK_BYTES];
            loop {
                operation.check("read_mkv_webm_source")?;
                let count = file
                    .read(&mut chunk)
                    .map_err(|source| source_io_error("read_source", source))?;
                if count == 0 {
                    break;
                }
                let next_length = bytes.len().checked_add(count).ok_or_else(|| {
                    resource_exhausted("read_source", "source length overflowed while reading")
                })?;
                check_source_length(next_length as u64)?;
                bytes.try_reserve(count).map_err(|_| {
                    resource_exhausted("read_source", "source growth could not be reserved")
                })?;
                bytes.extend_from_slice(&chunk[..count]);
            }
            Ok(Arc::from(bytes))
        }
    }
}

fn check_source_length(length: u64) -> Result<()> {
    if length > MAX_SOURCE_BYTES {
        return Err(resource_exhausted(
            "read_source",
            "source exceeds the bounded in-memory demuxing limit",
        ));
    }
    Ok(())
}

fn copy_packet_bytes(bytes: &[u8], operation: &OperationContext) -> Result<Arc<[u8]>> {
    if bytes.len() > matroska_parser::MAX_PACKET_BYTES {
        return Err(resource_exhausted(
            "read_packet",
            "packet exceeds the bounded delivery limit",
        ));
    }
    let mut copied = Vec::new();
    copied.try_reserve_exact(bytes.len()).map_err(|_| {
        resource_exhausted("read_packet", "packet allocation could not be reserved")
    })?;
    for chunk in bytes.chunks(READ_CHUNK_BYTES) {
        operation.check("copy_mkv_webm_packet")?;
        copied.extend_from_slice(chunk);
    }
    operation.check("copy_mkv_webm_packet")?;
    Ok(Arc::from(copied))
}

fn sha256_fingerprint_interruptible(data: &[u8], operation: &OperationContext) -> Result<String> {
    let mut digest = Sha256::new();
    for chunk in data.chunks(READ_CHUNK_BYTES) {
        operation.check("fingerprint_mkv_webm_source")?;
        digest.update(chunk);
    }
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn time_cmp(left: RationalTime, right: RationalTime) -> Ordering {
    left.partial_cmp(&right)
        .expect("validated timebases always have a total ordering")
}

fn source_io_error(operation: &'static str, source: io::Error) -> Error {
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
        "Matroska source could not be read",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
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

fn resource_exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("limit", "bounded-memory"))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
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
        "the MKV and WebM container backend does not implement codec processing",
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("capability", capability))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_sources_beyond_the_residency_bound() {
        assert!(check_source_length(MAX_SOURCE_BYTES).is_ok());
        let error = check_source_length(MAX_SOURCE_BYTES + 1).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    }

    #[test]
    fn vvc_track_selects_the_vvc_decoder() {
        assert_eq!(codec_id("V_MPEGI/ISO/VVC"), "vvc");
    }
}
