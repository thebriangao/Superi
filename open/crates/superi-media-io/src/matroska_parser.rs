use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::operation::OperationContext;

const COMPONENT: &str = "superi-media-io.matroska-parser";
const MAX_ELEMENTS: usize = 2_000_000;
const MAX_TRACKS: usize = 1_024;
const MAX_FRAMES: usize = 2_000_000;
pub(crate) const MAX_PACKET_BYTES: usize = 64 * 1024 * 1024;
const MAX_REFERENCE_BLOCKS: usize = 256;
const MAX_BLOCK_ADDITIONS: usize = 256;
const MAX_BLOCK_METADATA_BYTES: usize = 4 * 1024 * 1024;
const MAX_CODEC_PRIVATE_BYTES: usize = 16 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const READ_POLL_BYTES: usize = 64 * 1024;
const U64_EXCLUSIVE_F64: f64 = 18_446_744_073_709_551_616.0;

const ID_EBML: u32 = 0x1A45_DFA3;
const ID_SEGMENT: u32 = 0x1853_8067;
const ID_SEEK_HEAD: u32 = 0x114D_9B74;
const ID_INFO: u32 = 0x1549_A966;
const ID_TRACKS: u32 = 0x1654_AE6B;
const ID_CLUSTER: u32 = 0x1F43_B675;
const ID_CUES: u32 = 0x1C53_BB6B;
const ID_ATTACHMENTS: u32 = 0x1941_A469;
const ID_CHAPTERS: u32 = 0x1043_A770;
const ID_TAGS: u32 = 0x1254_C367;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentType {
    Matroska,
    Webm,
}

impl DocumentType {
    pub(crate) const fn container_id(self) -> &'static str {
        match self {
            Self::Matroska => "mkv",
            Self::Webm => "webm",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedDocument {
    pub(crate) document_type: DocumentType,
    pub(crate) document_type_version: u64,
    pub(crate) document_read_version: u64,
    pub(crate) timestamp_scale_ns: u64,
    pub(crate) duration_ticks: Option<f64>,
    pub(crate) duration_ns: Option<u64>,
    pub(crate) segment_uid: Option<Vec<u8>>,
    pub(crate) title: Option<String>,
    pub(crate) muxing_app: Option<String>,
    pub(crate) writing_app: Option<String>,
    pub(crate) tracks: Vec<ParsedTrack>,
    pub(crate) frames: Vec<ParsedFrame>,
    pub(crate) cue_point_count: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedVideo {
    pub(crate) pixel_width: Option<u64>,
    pub(crate) pixel_height: Option<u64>,
    pub(crate) display_width: Option<u64>,
    pub(crate) display_height: Option<u64>,
    pub(crate) stereo_mode: Option<u64>,
    pub(crate) alpha_mode: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedAudio {
    pub(crate) sampling_frequency: Option<f64>,
    pub(crate) output_sampling_frequency: Option<f64>,
    pub(crate) channels: Option<u64>,
    pub(crate) bit_depth: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedTrack {
    pub(crate) number: u32,
    pub(crate) uid: u64,
    pub(crate) track_type: u64,
    pub(crate) enabled: bool,
    pub(crate) default: bool,
    pub(crate) forced: bool,
    pub(crate) lacing: bool,
    pub(crate) default_duration_ns: Option<u64>,
    pub(crate) name: Option<String>,
    pub(crate) language: String,
    pub(crate) codec_id: String,
    pub(crate) codec_private: Option<Arc<[u8]>>,
    pub(crate) codec_delay_ns: u64,
    pub(crate) seek_pre_roll_ns: u64,
    pub(crate) video: ParsedVideo,
    pub(crate) audio: ParsedAudio,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedFrame {
    pub(crate) track_number: u32,
    pub(crate) data_offset: u64,
    pub(crate) size: u64,
    pub(crate) block_offset: u64,
    pub(crate) cluster_timestamp_ticks: u64,
    pub(crate) relative_timestamp_ticks: i16,
    pub(crate) presentation_ns: Option<i64>,
    pub(crate) sort_timestamp_ns: i64,
    pub(crate) duration_ns: Option<u64>,
    pub(crate) block_duration_ticks: Option<u64>,
    pub(crate) keyframe: bool,
    pub(crate) invisible: bool,
    pub(crate) discardable: bool,
    pub(crate) lace_index: usize,
    pub(crate) lace_count: usize,
    pub(crate) reference_blocks: Arc<[i64]>,
    pub(crate) discard_padding_ns: Option<i64>,
    pub(crate) codec_state: Option<Arc<[u8]>>,
    pub(crate) block_additions: Arc<[(u64, Arc<[u8]>)]>,
}

pub(crate) enum ProbeDecision {
    NoMatch,
    NeedMoreData(usize),
    Match(DocumentType),
}

pub(crate) fn inspect_prefix(bytes: &[u8], source_length: u64, complete: bool) -> ProbeDecision {
    let signature = ID_EBML.to_be_bytes();
    if bytes.len() < signature.len() {
        return if signature.starts_with(bytes) && !complete {
            ProbeDecision::NeedMoreData(signature.len())
        } else {
            ProbeDecision::NoMatch
        };
    }
    if bytes[..4] != signature {
        return ProbeDecision::NoMatch;
    }

    let Ok((size, size_width, unknown)) = read_size(bytes, 4, bytes.len()) else {
        return if !complete {
            ProbeDecision::NeedMoreData(bytes.len().saturating_add(1))
        } else {
            ProbeDecision::NoMatch
        };
    };
    if unknown {
        return ProbeDecision::NoMatch;
    }
    let header_start = 4 + size_width;
    let Ok(size) = usize::try_from(size) else {
        return ProbeDecision::NoMatch;
    };
    let Some(header_end) = header_start.checked_add(size) else {
        return ProbeDecision::NoMatch;
    };
    if header_end > bytes.len() {
        let available_source = usize::try_from(source_length).unwrap_or(usize::MAX);
        return if !complete && header_end <= available_source {
            ProbeDecision::NeedMoreData(header_end)
        } else {
            ProbeDecision::NoMatch
        };
    }

    let mut cursor = header_start;
    while cursor < header_end {
        let Ok((id, id_width)) = read_id(bytes, cursor, header_end) else {
            return ProbeDecision::NoMatch;
        };
        let size_position = cursor + id_width;
        let Ok((child_size, child_size_width, child_unknown)) =
            read_size(bytes, size_position, header_end)
        else {
            return ProbeDecision::NoMatch;
        };
        if child_unknown {
            return ProbeDecision::NoMatch;
        }
        let data_start = size_position + child_size_width;
        let Ok(child_size) = usize::try_from(child_size) else {
            return ProbeDecision::NoMatch;
        };
        let Some(end) = data_start.checked_add(child_size) else {
            return ProbeDecision::NoMatch;
        };
        if end > header_end {
            return ProbeDecision::NoMatch;
        }
        if id == 0x4282 {
            return match &bytes[data_start..end] {
                b"matroska" => ProbeDecision::Match(DocumentType::Matroska),
                b"webm" => ProbeDecision::Match(DocumentType::Webm),
                _ => ProbeDecision::NoMatch,
            };
        }
        cursor = end;
    }
    ProbeDecision::NoMatch
}

pub(crate) fn parse(data: &[u8], operation: &OperationContext) -> Result<ParsedDocument> {
    Parser::new(data, operation).parse()
}

struct Parser<'a> {
    data: &'a [u8],
    operation: &'a OperationContext,
    element_count: usize,
    frame_count: usize,
}

#[derive(Clone, Copy)]
struct Element {
    id: u32,
    header_start: usize,
    data_start: usize,
    end: usize,
    unknown_size: bool,
}

#[derive(Default)]
struct ParsedInfo {
    timestamp_scale_ns: u64,
    duration_ticks: Option<f64>,
    segment_uid: Option<Vec<u8>>,
    title: Option<String>,
    muxing_app: Option<String>,
    writing_app: Option<String>,
}

#[derive(Default)]
struct PendingBlock {
    payload_start: usize,
    payload_end: usize,
    block_offset: usize,
    simple: bool,
    duration_ticks: Option<u64>,
    references: Vec<i64>,
    discard_padding_ns: Option<i64>,
    codec_state: Option<Arc<[u8]>>,
    block_additions: Vec<(u64, Arc<[u8]>)>,
}

impl<'a> Parser<'a> {
    fn new(data: &'a [u8], operation: &'a OperationContext) -> Self {
        Self {
            data,
            operation,
            element_count: 0,
            frame_count: 0,
        }
    }

    fn parse(mut self) -> Result<ParsedDocument> {
        self.operation.check("parse_matroska")?;
        let header = self.element(0, self.data.len())?;
        if header.id != ID_EBML || header.unknown_size {
            return Err(corrupt(
                "parse_header",
                "source does not begin with a bounded EBML header",
            ));
        }
        let (document_type, document_type_version, document_read_version) =
            self.parse_header(header)?;
        let segment = self.element(header.end, self.data.len())?;
        if segment.id != ID_SEGMENT {
            return Err(corrupt(
                "parse_segment",
                "EBML body does not begin with a Segment element",
            ));
        }

        let segment_end = if segment.unknown_size {
            self.data.len()
        } else {
            segment.end
        };
        if !segment.unknown_size && segment.end != self.data.len() {
            return Err(unsupported(
                "parse_segment",
                "multiple EBML documents are not supported by the single-source media contract",
            ));
        }
        let mut info = ParsedInfo {
            timestamp_scale_ns: 1_000_000,
            ..ParsedInfo::default()
        };
        let mut tracks = Vec::new();
        let mut frames = Vec::new();
        let mut cue_point_count = 0_usize;
        let mut info_seen = false;
        let mut tracks_seen = false;
        let mut cursor = segment.data_start;
        while cursor < segment_end {
            let child = self.element(cursor, segment_end)?;
            match child.id {
                ID_INFO => {
                    require_known(child, "Info")?;
                    if info_seen {
                        return Err(corrupt(
                            "parse_segment",
                            "Segment contains multiple Info elements",
                        ));
                    }
                    info_seen = true;
                    info = self.parse_info(child)?;
                    cursor = child.end;
                }
                ID_TRACKS => {
                    require_known(child, "Tracks")?;
                    if tracks_seen {
                        return Err(corrupt(
                            "parse_segment",
                            "Segment contains multiple Tracks elements",
                        ));
                    }
                    tracks_seen = true;
                    tracks = self.parse_tracks(child)?;
                    if tracks.len() > MAX_TRACKS {
                        return Err(resource_limit(
                            "parse_tracks",
                            "track count exceeds the parser limit",
                        ));
                    }
                    cursor = child.end;
                }
                ID_CLUSTER => {
                    let next = self.parse_cluster(child, segment_end, &mut frames)?;
                    cursor = next;
                }
                ID_CUES => {
                    require_known(child, "Cues")?;
                    cue_point_count = cue_point_count
                        .checked_add(self.count_children(child, 0xBB)?)
                        .ok_or_else(|| {
                            resource_limit("parse_cues", "cue point count overflowed")
                        })?;
                    cursor = child.end;
                }
                _ => {
                    if child.unknown_size {
                        return Err(corrupt(
                            "skip_element",
                            "unsupported element has an unknown data size",
                        ));
                    }
                    cursor = child.end;
                }
            }
        }
        if cursor != segment_end {
            return Err(corrupt(
                "parse_segment",
                "segment elements do not end on a valid boundary",
            ));
        }
        if !info_seen || !tracks_seen {
            return Err(corrupt(
                "parse_segment",
                "Segment must contain exactly one Info and one Tracks element",
            ));
        }

        self.finalize(
            document_type,
            document_type_version,
            document_read_version,
            info,
            tracks,
            frames,
            cue_point_count,
        )
    }

    fn parse_header(&mut self, header: Element) -> Result<(DocumentType, u64, u64)> {
        let mut ebml_version = 1;
        let mut ebml_read_version = 1;
        let mut doc_type = None;
        let mut doc_type_version = 1;
        let mut doc_read_version = 1;
        let mut max_id_length = 4;
        let mut max_size_length = 8;
        self.for_each_child(header, |parser, child| {
            match child.id {
                0x4286 => ebml_version = parser.unsigned(child, "EBMLVersion")?,
                0x42F7 => ebml_read_version = parser.unsigned(child, "EBMLReadVersion")?,
                0x4282 => doc_type = Some(parser.ascii(child, "DocType")?),
                0x4287 => doc_type_version = parser.unsigned(child, "DocTypeVersion")?,
                0x4285 => doc_read_version = parser.unsigned(child, "DocTypeReadVersion")?,
                0x42F2 => max_id_length = parser.unsigned(child, "EBMLMaxIDLength")?,
                0x42F3 => max_size_length = parser.unsigned(child, "EBMLMaxSizeLength")?,
                _ => {}
            }
            Ok(())
        })?;
        if ebml_version != 1 || ebml_read_version != 1 {
            return Err(unsupported(
                "parse_header",
                "Matroska requires EBML version 1",
            ));
        }
        if max_id_length != 4 || !(1..=8).contains(&max_size_length) {
            return Err(corrupt(
                "parse_header",
                "EBML width declarations are not valid for Matroska",
            ));
        }
        if doc_type_version == 0 || doc_read_version == 0 || doc_read_version > doc_type_version {
            return Err(corrupt(
                "parse_header",
                "DocType versions must be nonzero and the read version cannot exceed the document version",
            ));
        }
        if doc_read_version > 4 {
            return Err(unsupported(
                "parse_header",
                "Matroska document requires a newer read version",
            ));
        }
        let document_type = match doc_type.as_deref() {
            Some("matroska") => DocumentType::Matroska,
            Some("webm") => DocumentType::Webm,
            _ => {
                return Err(corrupt(
                    "parse_header",
                    "EBML DocType is not matroska or webm",
                ))
            }
        };
        Ok((document_type, doc_type_version, doc_read_version))
    }

    fn parse_info(&mut self, info: Element) -> Result<ParsedInfo> {
        let mut parsed = ParsedInfo {
            timestamp_scale_ns: 1_000_000,
            ..ParsedInfo::default()
        };
        self.for_each_child(info, |parser, child| {
            match child.id {
                0x2AD7B1 => parsed.timestamp_scale_ns = parser.unsigned(child, "TimestampScale")?,
                0x4489 => parsed.duration_ticks = Some(parser.float(child, "Duration")?),
                0x73A4 => {
                    let uid = parser.binary(child);
                    if uid.len() != 16 {
                        return Err(corrupt(
                            "parse_info",
                            "SegmentUUID must contain exactly sixteen bytes",
                        ));
                    }
                    parsed.segment_uid = Some(uid.to_vec());
                }
                0x7BA9 => parsed.title = Some(parser.utf8(child, "Title")?),
                0x4D80 => parsed.muxing_app = Some(parser.utf8(child, "MuxingApp")?),
                0x5741 => parsed.writing_app = Some(parser.utf8(child, "WritingApp")?),
                _ => {}
            }
            Ok(())
        })?;
        if parsed.timestamp_scale_ns == 0 {
            return Err(corrupt(
                "parse_info",
                "TimestampScale must be greater than zero",
            ));
        }
        if let Some(duration) = parsed.duration_ticks {
            if !duration.is_finite() || duration < 0.0 {
                return Err(corrupt(
                    "parse_info",
                    "Duration must be finite and nonnegative",
                ));
            }
        }
        Ok(parsed)
    }

    fn parse_tracks(&mut self, tracks: Element) -> Result<Vec<ParsedTrack>> {
        let mut parsed = Vec::new();
        self.for_each_child(tracks, |parser, child| {
            if child.id == 0xAE {
                parsed.try_reserve(1).map_err(|_| {
                    resource_limit("parse_tracks", "track allocation could not grow")
                })?;
                parsed.push(parser.parse_track(child)?);
            }
            Ok(())
        })?;
        Ok(parsed)
    }

    fn parse_track(&mut self, track: Element) -> Result<ParsedTrack> {
        let mut number = None;
        let mut uid = None;
        let mut track_type = None;
        let mut enabled = true;
        let mut default = true;
        let mut forced = false;
        let mut lacing = true;
        let mut default_duration_ns = None;
        let mut name = None;
        let mut language = "eng".to_owned();
        let mut codec_id = None;
        let mut codec_private = None;
        let mut codec_delay_ns = 0;
        let mut seek_pre_roll_ns = 0;
        let mut video = ParsedVideo::default();
        let mut audio = ParsedAudio::default();
        self.for_each_child(track, |parser, child| {
            match child.id {
                0xD7 => number = Some(parser.unsigned(child, "TrackNumber")?),
                0x73C5 => uid = Some(parser.unsigned(child, "TrackUID")?),
                0x83 => track_type = Some(parser.unsigned(child, "TrackType")?),
                0xB9 => enabled = parser.boolean(child, "FlagEnabled")?,
                0x88 => default = parser.boolean(child, "FlagDefault")?,
                0x55AA => forced = parser.boolean(child, "FlagForced")?,
                0x9C => lacing = parser.boolean(child, "FlagLacing")?,
                0x23E383 => default_duration_ns = Some(parser.unsigned(child, "DefaultDuration")?),
                0x536E => name = Some(parser.utf8(child, "Name")?),
                0x22B59C => language = parser.ascii(child, "Language")?,
                0x86 => codec_id = Some(parser.ascii(child, "CodecID")?),
                0x63A2 => {
                    codec_private = Some(parser.copy_bounded_binary(
                        child,
                        MAX_CODEC_PRIVATE_BYTES,
                        "copy_codec_private",
                        "CodecPrivate exceeds the bounded copy limit",
                    )?)
                }
                0x56AA => codec_delay_ns = parser.unsigned(child, "CodecDelay")?,
                0x56BB => seek_pre_roll_ns = parser.unsigned(child, "SeekPreRoll")?,
                0x23314F => {
                    let scale = parser.float(child, "TrackTimestampScale")?;
                    if scale != 1.0 {
                        return Err(unsupported(
                            "parse_track",
                            "non-default TrackTimestampScale cannot be represented exactly",
                        ));
                    }
                }
                0xE0 => video = parser.parse_video(child)?,
                0xE1 => audio = parser.parse_audio(child)?,
                0xE2 => {
                    return Err(unsupported(
                        "parse_track",
                        "TrackOperation relationships are not supported by the current media contract",
                    ));
                }
                0x6D80 => {
                    return Err(unsupported(
                        "parse_track",
                        "compressed or encrypted ContentEncodings require an explicit transform stage",
                    ));
                }
                _ => {}
            }
            Ok(())
        })?;
        let number =
            number.ok_or_else(|| corrupt("parse_track", "TrackEntry is missing TrackNumber"))?;
        let number = u32::try_from(number).map_err(|_| {
            corrupt(
                "parse_track",
                "TrackNumber exceeds the supported stream identifier range",
            )
        })?;
        let uid = uid.ok_or_else(|| corrupt("parse_track", "TrackEntry is missing TrackUID"))?;
        let track_type =
            track_type.ok_or_else(|| corrupt("parse_track", "TrackEntry is missing TrackType"))?;
        let codec_id =
            codec_id.ok_or_else(|| corrupt("parse_track", "TrackEntry is missing CodecID"))?;
        if number == 0 || uid == 0 || track_type == 0 || codec_id.is_empty() {
            return Err(corrupt(
                "parse_track",
                "TrackEntry contains a forbidden zero or empty identity",
            ));
        }
        if track_type == 1 {
            let pixel_width = video
                .pixel_width
                .ok_or_else(|| corrupt("parse_track", "video track is missing PixelWidth"))?;
            let pixel_height = video
                .pixel_height
                .ok_or_else(|| corrupt("parse_track", "video track is missing PixelHeight"))?;
            if pixel_width == 0 || pixel_height == 0 {
                return Err(corrupt(
                    "parse_track",
                    "video pixel dimensions must be greater than zero",
                ));
            }
            video.display_width.get_or_insert(pixel_width);
            video.display_height.get_or_insert(pixel_height);
            if video.display_width == Some(0) || video.display_height == Some(0) {
                return Err(corrupt(
                    "parse_track",
                    "video display dimensions must be greater than zero",
                ));
            }
        }
        if track_type == 2 {
            let sampling_frequency = *audio.sampling_frequency.get_or_insert(8_000.0);
            audio
                .output_sampling_frequency
                .get_or_insert(sampling_frequency);
            audio.channels.get_or_insert(1);
            if audio.channels == Some(0) || audio.bit_depth == Some(0) {
                return Err(corrupt(
                    "parse_track",
                    "audio channel count and bit depth must be greater than zero",
                ));
            }
        }
        Ok(ParsedTrack {
            number,
            uid,
            track_type,
            enabled,
            default,
            forced,
            lacing,
            default_duration_ns,
            name,
            language,
            codec_id,
            codec_private,
            codec_delay_ns,
            seek_pre_roll_ns,
            video,
            audio,
        })
    }

    fn parse_video(&mut self, video: Element) -> Result<ParsedVideo> {
        let mut parsed = ParsedVideo::default();
        self.for_each_child(video, |parser, child| {
            match child.id {
                0xB0 => parsed.pixel_width = Some(parser.unsigned(child, "PixelWidth")?),
                0xBA => parsed.pixel_height = Some(parser.unsigned(child, "PixelHeight")?),
                0x54B0 => parsed.display_width = Some(parser.unsigned(child, "DisplayWidth")?),
                0x54BA => parsed.display_height = Some(parser.unsigned(child, "DisplayHeight")?),
                0x53B8 => parsed.stereo_mode = Some(parser.unsigned(child, "StereoMode")?),
                0x53C0 => parsed.alpha_mode = Some(parser.unsigned(child, "AlphaMode")?),
                _ => {}
            }
            Ok(())
        })?;
        Ok(parsed)
    }

    fn parse_audio(&mut self, audio: Element) -> Result<ParsedAudio> {
        let mut parsed = ParsedAudio::default();
        self.for_each_child(audio, |parser, child| {
            match child.id {
                0xB5 => parsed.sampling_frequency = Some(parser.float(child, "SamplingFrequency")?),
                0x78B5 => {
                    parsed.output_sampling_frequency =
                        Some(parser.float(child, "OutputSamplingFrequency")?)
                }
                0x9F => parsed.channels = Some(parser.unsigned(child, "Channels")?),
                0x6264 => parsed.bit_depth = Some(parser.unsigned(child, "BitDepth")?),
                _ => {}
            }
            Ok(())
        })?;
        for value in [parsed.sampling_frequency, parsed.output_sampling_frequency]
            .into_iter()
            .flatten()
        {
            if !value.is_finite() || value <= 0.0 {
                return Err(corrupt(
                    "parse_audio",
                    "audio sampling frequency must be finite and positive",
                ));
            }
        }
        Ok(parsed)
    }

    fn parse_cluster(
        &mut self,
        cluster: Element,
        segment_end: usize,
        frames: &mut Vec<ParsedFrame>,
    ) -> Result<usize> {
        let limit = if cluster.unknown_size {
            segment_end
        } else {
            cluster.end
        };
        let mut timestamp = None;
        let mut pending = Vec::new();
        let mut cursor = cluster.data_start;
        while cursor < limit {
            if cluster.unknown_size && is_top_level(peek_id(self.data, cursor, limit)?) {
                break;
            }
            let child = self.element(cursor, limit)?;
            if child.unknown_size {
                return Err(corrupt(
                    "parse_cluster",
                    "cluster child has an unknown data size",
                ));
            }
            match child.id {
                0xE7 => timestamp = Some(self.unsigned(child, "Cluster Timestamp")?),
                0xA3 => {
                    pending.try_reserve(1).map_err(|_| {
                        resource_limit("parse_cluster", "block allocation could not grow")
                    })?;
                    pending.push(PendingBlock {
                        payload_start: child.data_start,
                        payload_end: child.end,
                        block_offset: child.header_start,
                        simple: true,
                        ..PendingBlock::default()
                    });
                }
                0xA0 => {
                    pending.try_reserve(1).map_err(|_| {
                        resource_limit("parse_cluster", "block allocation could not grow")
                    })?;
                    pending.push(self.parse_block_group(child)?);
                }
                _ => {}
            }
            cursor = child.end;
        }
        let timestamp =
            timestamp.ok_or_else(|| corrupt("parse_cluster", "Cluster is missing Timestamp"))?;
        for block in pending {
            self.parse_block(timestamp, block, frames)?;
        }
        Ok(if cluster.unknown_size {
            cursor
        } else {
            cluster.end
        })
    }

    fn parse_block_group(&mut self, group: Element) -> Result<PendingBlock> {
        let mut parsed = PendingBlock::default();
        let mut metadata_bytes = 0_usize;
        let mut additions_seen = false;
        self.for_each_child(group, |parser, child| {
            match child.id {
                0xA1 => {
                    if parsed.payload_end != 0 {
                        return Err(corrupt(
                            "parse_block_group",
                            "BlockGroup contains multiple Block elements",
                        ));
                    }
                    parsed.payload_start = child.data_start;
                    parsed.payload_end = child.end;
                    parsed.block_offset = child.header_start;
                }
                0x9B => parsed.duration_ticks = Some(parser.unsigned(child, "BlockDuration")?),
                0xFB => {
                    if parsed.references.len() >= MAX_REFERENCE_BLOCKS {
                        return Err(resource_limit(
                            "parse_block_group",
                            "ReferenceBlock count exceeds the per-block limit",
                        ));
                    }
                    parsed
                        .references
                        .push(parser.signed(child, "ReferenceBlock")?);
                }
                0x75A2 => parsed.discard_padding_ns = Some(parser.signed(child, "DiscardPadding")?),
                0xA4 => {
                    if parsed.codec_state.is_some() {
                        return Err(corrupt(
                            "parse_block_group",
                            "BlockGroup contains multiple CodecState elements",
                        ));
                    }
                    parsed.codec_state = Some(parser.copy_block_metadata(
                        child,
                        &mut metadata_bytes,
                        "CodecState",
                    )?);
                }
                0x75A1 => {
                    if additions_seen {
                        return Err(corrupt(
                            "parse_block_group",
                            "BlockGroup contains multiple BlockAdditions elements",
                        ));
                    }
                    additions_seen = true;
                    parsed.block_additions =
                        parser.parse_block_additions(child, &mut metadata_bytes)?;
                }
                _ => {}
            }
            Ok(())
        })?;
        if parsed.payload_end == 0 {
            return Err(corrupt("parse_block_group", "BlockGroup is missing Block"));
        }
        Ok(parsed)
    }

    fn parse_block_additions(
        &mut self,
        additions: Element,
        metadata_bytes: &mut usize,
    ) -> Result<Vec<(u64, Arc<[u8]>)>> {
        let mut parsed = Vec::new();
        self.for_each_child(additions, |parser, child| {
            if child.id != 0xA6 {
                return Ok(());
            }
            if parsed.len() >= MAX_BLOCK_ADDITIONS {
                return Err(resource_limit(
                    "parse_block_additions",
                    "BlockMore count exceeds the per-block limit",
                ));
            }
            let mut id = 1_u64;
            let mut data = None;
            let mut id_seen = false;
            parser.for_each_child(child, |parser, value| {
                match value.id {
                    0xEE => {
                        if id_seen {
                            return Err(corrupt(
                                "parse_block_additions",
                                "BlockMore contains multiple BlockAddID elements",
                            ));
                        }
                        id_seen = true;
                        id = parser.unsigned(value, "BlockAddID")?;
                    }
                    0xA5 => {
                        if data.is_some() {
                            return Err(corrupt(
                                "parse_block_additions",
                                "BlockMore contains multiple BlockAdditional elements",
                            ));
                        }
                        data = Some(parser.copy_block_metadata(
                            value,
                            metadata_bytes,
                            "BlockAdditional",
                        )?);
                    }
                    _ => {}
                }
                Ok(())
            })?;
            let data = data.ok_or_else(|| {
                corrupt(
                    "parse_block_additions",
                    "BlockMore is missing BlockAdditional data",
                )
            })?;
            parsed.try_reserve(1).map_err(|_| {
                resource_limit(
                    "parse_block_additions",
                    "BlockMore allocation could not grow",
                )
            })?;
            parsed.push((id, data));
            Ok(())
        })?;
        Ok(parsed)
    }

    fn copy_block_metadata(
        &self,
        element: Element,
        total_bytes: &mut usize,
        field: &'static str,
    ) -> Result<Arc<[u8]>> {
        let bytes = self.binary(element);
        *total_bytes =
            checked_block_metadata_total(*total_bytes, bytes.len()).map_err(|error| {
                error.with_context(
                    ErrorContext::new(COMPONENT, "identify_field").with_field("field", field),
                )
            })?;
        self.copy_bounded_binary(
            element,
            MAX_BLOCK_METADATA_BYTES,
            "copy_block_metadata",
            "metadata allocation exceeds the per-block byte limit",
        )
    }

    fn copy_bounded_binary(
        &self,
        element: Element,
        limit: usize,
        operation_name: &'static str,
        limit_message: &'static str,
    ) -> Result<Arc<[u8]>> {
        let bytes = self.binary(element);
        if bytes.len() > limit {
            return Err(resource_limit(operation_name, limit_message));
        }
        let mut copied = Vec::new();
        copied.try_reserve_exact(bytes.len()).map_err(|_| {
            resource_limit(operation_name, "binary allocation could not be reserved")
        })?;
        for chunk in bytes.chunks(READ_POLL_BYTES) {
            self.operation.check(operation_name)?;
            copied.extend_from_slice(chunk);
        }
        self.operation.check(operation_name)?;
        Ok(Arc::from(copied))
    }

    fn parse_block(
        &mut self,
        cluster_timestamp: u64,
        block: PendingBlock,
        frames: &mut Vec<ParsedFrame>,
    ) -> Result<()> {
        let payload = &self.data[block.payload_start..block.payload_end];
        let (track_number, track_width) = read_data_vint(payload, 0, payload.len())?;
        if track_number == 0 {
            return Err(corrupt(
                "parse_block",
                "Block contains an invalid Track Number",
            ));
        }
        let track_number = u32::try_from(track_number).map_err(|_| {
            corrupt(
                "parse_block",
                "Block Track Number exceeds the supported range",
            )
        })?;
        if payload.len() < track_width + 3 {
            return Err(corrupt("parse_block", "Block header is truncated"));
        }
        let relative_timestamp =
            i16::from_be_bytes([payload[track_width], payload[track_width + 1]]);
        let flags = payload[track_width + 2];
        if block.simple && flags & 0x70 != 0 {
            return Err(corrupt(
                "parse_block",
                "SimpleBlock reserved flags are not zero",
            ));
        }
        let data_start = track_width + 3;
        let (frame_ranges, lace_count) = parse_lacing(payload, data_start, flags & 0x06)?;
        let keyframe = if block.simple {
            flags & 0x80 != 0
        } else {
            block.references.is_empty()
        };
        let reference_blocks: Arc<[i64]> = Arc::from(block.references);
        let block_additions: Arc<[(u64, Arc<[u8]>)]> = Arc::from(block.block_additions);
        let codec_state = block.codec_state;
        for (lace_index, range) in frame_ranges.into_iter().enumerate() {
            validate_packet_size(range.end - range.start)?;
            self.frame_count = self.frame_count.checked_add(1).ok_or_else(|| {
                resource_limit("parse_block", "frame count overflowed the parser limit")
            })?;
            if self.frame_count > MAX_FRAMES {
                return Err(resource_limit(
                    "parse_block",
                    "frame count exceeds the parser limit",
                ));
            }
            frames
                .try_reserve(1)
                .map_err(|_| resource_limit("parse_block", "frame allocation could not grow"))?;
            let absolute_start = block
                .payload_start
                .checked_add(range.start)
                .ok_or_else(|| corrupt("parse_block", "frame byte offset overflowed"))?;
            frames.push(ParsedFrame {
                track_number,
                data_offset: absolute_start as u64,
                size: (range.end - range.start) as u64,
                block_offset: block.block_offset as u64,
                cluster_timestamp_ticks: cluster_timestamp,
                relative_timestamp_ticks: relative_timestamp,
                presentation_ns: None,
                sort_timestamp_ns: 0,
                duration_ns: None,
                block_duration_ticks: block.duration_ticks,
                keyframe,
                invisible: flags & 0x08 != 0,
                discardable: block.simple && flags & 0x01 != 0,
                lace_index,
                lace_count,
                reference_blocks: Arc::clone(&reference_blocks),
                discard_padding_ns: block.discard_padding_ns,
                codec_state: codec_state.clone(),
                block_additions: Arc::clone(&block_additions),
            });
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn finalize(
        &mut self,
        document_type: DocumentType,
        document_type_version: u64,
        document_read_version: u64,
        info: ParsedInfo,
        tracks: Vec<ParsedTrack>,
        mut frames: Vec<ParsedFrame>,
        cue_point_count: usize,
    ) -> Result<ParsedDocument> {
        if tracks.is_empty() {
            return Err(corrupt("finalize", "Segment contains no tracks"));
        }
        let mut numbers = BTreeSet::new();
        let mut uids = BTreeSet::new();
        let mut by_number = BTreeMap::new();
        for track in &tracks {
            if !numbers.insert(track.number) || !uids.insert(track.uid) {
                return Err(corrupt(
                    "finalize",
                    "Segment contains duplicate track identities",
                ));
            }
            by_number.insert(track.number, track);
        }

        for frame in &mut frames {
            self.operation.check("finalize_matroska_timing")?;
            let track = by_number.get(&frame.track_number).ok_or_else(|| {
                corrupt(
                    "finalize",
                    "Block references a Track Number not declared by Tracks",
                )
            })?;
            if frame.lace_count > 1 && !track.lacing {
                return Err(corrupt(
                    "finalize",
                    "Track forbids lacing but a Block uses lacing",
                ));
            }
            let base_ns = i128::from(frame.cluster_timestamp_ticks)
                .checked_add(i128::from(frame.relative_timestamp_ticks))
                .and_then(|ticks| ticks.checked_mul(i128::from(info.timestamp_scale_ns)))
                .ok_or_else(|| corrupt("finalize", "Block timestamp overflows nanoseconds"))?;
            let base_ns = i64::try_from(base_ns).map_err(|_| {
                corrupt(
                    "finalize",
                    "Block timestamp exceeds the supported nanosecond range",
                )
            })?;

            let block_duration_ns = match frame.block_duration_ticks {
                Some(ticks) => {
                    Some(ticks.checked_mul(info.timestamp_scale_ns).ok_or_else(|| {
                        corrupt("finalize", "BlockDuration overflows nanoseconds")
                    })?)
                }
                None => None,
            };
            let derived_duration = if let Some(block_duration) = block_duration_ns {
                if frame.lace_count == 1 {
                    Some(block_duration)
                } else {
                    (block_duration % frame.lace_count as u64 == 0)
                        .then_some(block_duration / frame.lace_count as u64)
                }
            } else {
                track.default_duration_ns
            };
            frame.duration_ns = derived_duration;
            let offset_ns =
                derived_duration.and_then(|duration| duration.checked_mul(frame.lace_index as u64));
            frame.presentation_ns = if frame.lace_index == 0 {
                Some(base_ns)
            } else {
                offset_ns
                    .and_then(|offset| i64::try_from(offset).ok())
                    .and_then(|offset| base_ns.checked_add(offset))
            };
            frame.sort_timestamp_ns = frame.presentation_ns.unwrap_or(base_ns);
        }

        let duration_ns = duration_to_ns(info.duration_ticks, info.timestamp_scale_ns)?;

        Ok(ParsedDocument {
            document_type,
            document_type_version,
            document_read_version,
            timestamp_scale_ns: info.timestamp_scale_ns,
            duration_ticks: info.duration_ticks,
            duration_ns,
            segment_uid: info.segment_uid,
            title: info.title,
            muxing_app: info.muxing_app,
            writing_app: info.writing_app,
            tracks,
            frames,
            cue_point_count,
        })
    }

    fn element(&mut self, position: usize, limit: usize) -> Result<Element> {
        self.operation.check("parse_ebml_element")?;
        self.element_count = self.element_count.checked_add(1).ok_or_else(|| {
            resource_limit("parse_element", "element count overflowed the parser limit")
        })?;
        if self.element_count > MAX_ELEMENTS {
            return Err(resource_limit(
                "parse_element",
                "element count exceeds the parser limit",
            ));
        }
        let (id, id_width) = read_id(self.data, position, limit)?;
        let size_position = position
            .checked_add(id_width)
            .ok_or_else(|| corrupt("parse_element", "element header offset overflowed"))?;
        let (size, size_width, unknown_size) = read_size(self.data, size_position, limit)?;
        let data_start = size_position
            .checked_add(size_width)
            .ok_or_else(|| corrupt("parse_element", "element data offset overflowed"))?;
        let end = if unknown_size {
            limit
        } else {
            let size = usize::try_from(size)
                .map_err(|_| corrupt("parse_element", "element data size exceeds this platform"))?;
            data_start
                .checked_add(size)
                .filter(|end| *end <= limit)
                .ok_or_else(|| corrupt("parse_element", "element data extends past its parent"))?
        };
        Ok(Element {
            id,
            header_start: position,
            data_start,
            end,
            unknown_size,
        })
    }

    fn for_each_child(
        &mut self,
        parent: Element,
        mut visitor: impl FnMut(&mut Self, Element) -> Result<()>,
    ) -> Result<()> {
        require_known(parent, "master element")?;
        let mut cursor = parent.data_start;
        while cursor < parent.end {
            let child = self.element(cursor, parent.end)?;
            if child.unknown_size {
                return Err(corrupt(
                    "parse_children",
                    "nested element has an unsupported unknown data size",
                ));
            }
            visitor(self, child)?;
            cursor = child.end;
        }
        if cursor != parent.end {
            return Err(corrupt(
                "parse_children",
                "child elements do not end on the parent boundary",
            ));
        }
        Ok(())
    }

    fn count_children(&mut self, parent: Element, wanted: u32) -> Result<usize> {
        let mut count = 0_usize;
        self.for_each_child(parent, |_parser, child| {
            if child.id == wanted {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| resource_limit("count_children", "child count overflowed"))?;
            }
            Ok(())
        })?;
        Ok(count)
    }

    fn binary(&self, element: Element) -> &[u8] {
        &self.data[element.data_start..element.end]
    }

    fn unsigned(&self, element: Element, field: &'static str) -> Result<u64> {
        read_unsigned(self.binary(element), field)
    }

    fn signed(&self, element: Element, field: &'static str) -> Result<i64> {
        read_signed(self.binary(element), field)
    }

    fn float(&self, element: Element, field: &'static str) -> Result<f64> {
        read_float(self.binary(element), field)
    }

    fn ascii(&self, element: Element, field: &'static str) -> Result<String> {
        let bytes = trim_trailing_nul(self.binary(element));
        if bytes.len() > MAX_TEXT_BYTES {
            return Err(
                resource_limit("read_ascii", "text exceeds the parser byte limit").with_context(
                    ErrorContext::new(COMPONENT, "identify_field").with_field("field", field),
                ),
            );
        }
        for chunk in bytes.chunks(READ_POLL_BYTES) {
            self.operation.check("read_ascii")?;
            if !chunk.iter().all(|byte| (0x20..=0x7E).contains(byte)) {
                return Err(
                    corrupt("read_ascii", "ASCII element contains a non-printable byte")
                        .with_context(
                            ErrorContext::new(COMPONENT, "identify_field")
                                .with_field("field", field),
                        ),
                );
            }
        }
        Ok(String::from_utf8(bytes.to_vec()).expect("validated ASCII is UTF-8"))
    }

    fn utf8(&self, element: Element, field: &'static str) -> Result<String> {
        let bytes = trim_trailing_nul(self.binary(element));
        if bytes.len() > MAX_TEXT_BYTES {
            return Err(
                resource_limit("read_utf8", "text exceeds the parser byte limit").with_context(
                    ErrorContext::new(COMPONENT, "identify_field").with_field("field", field),
                ),
            );
        }
        self.operation.check("read_utf8")?;
        let text = std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|source| {
                Error::with_source(
                    ErrorCategory::CorruptData,
                    Recoverability::UserCorrectable,
                    "UTF-8 element contains invalid text",
                    source,
                )
                .with_context(ErrorContext::new(COMPONENT, "read_utf8").with_field("field", field))
            })?;
        self.operation.check("read_utf8")?;
        Ok(text)
    }

    fn boolean(&self, element: Element, field: &'static str) -> Result<bool> {
        match self.unsigned(element, field)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(
                corrupt("read_boolean", "boolean element must contain zero or one").with_context(
                    ErrorContext::new(COMPONENT, "identify_field").with_field("field", field),
                ),
            ),
        }
    }
}

fn parse_lacing(
    payload: &[u8],
    data_start: usize,
    mode: u8,
) -> Result<(Vec<std::ops::Range<usize>>, usize)> {
    if data_start > payload.len() {
        return Err(corrupt(
            "parse_lacing",
            "Block data starts past its payload",
        ));
    }
    if mode == 0 {
        let ranges = std::iter::once(data_start..payload.len()).collect();
        return Ok((ranges, 1));
    }
    let lace_count_minus_one = *payload
        .get(data_start)
        .ok_or_else(|| corrupt("parse_lacing", "laced Block is missing its frame count"))?;
    let lace_count = usize::from(lace_count_minus_one) + 1;
    if lace_count < 2 {
        return Err(corrupt(
            "parse_lacing",
            "lacing must contain at least two frames",
        ));
    }
    let mut cursor = data_start + 1;
    let mut sizes = Vec::with_capacity(lace_count);
    match mode {
        0x02 => {
            for _ in 0..lace_count - 1 {
                let mut size = 0_usize;
                loop {
                    let value = *payload.get(cursor).ok_or_else(|| {
                        corrupt("parse_xiph_lacing", "Xiph lace size is truncated")
                    })?;
                    cursor += 1;
                    size = size
                        .checked_add(usize::from(value))
                        .ok_or_else(|| corrupt("parse_xiph_lacing", "Xiph lace size overflowed"))?;
                    if value != 255 {
                        break;
                    }
                }
                sizes.push(size);
            }
        }
        0x04 => {
            let remaining = payload.len() - cursor;
            if remaining % lace_count != 0 {
                return Err(corrupt(
                    "parse_fixed_lacing",
                    "fixed lace payload is not evenly divisible",
                ));
            }
            sizes.resize(lace_count - 1, remaining / lace_count);
        }
        0x06 => {
            let (first, width) = read_data_vint(payload, cursor, payload.len())?;
            cursor += width;
            let first = usize::try_from(first).map_err(|_| {
                corrupt(
                    "parse_ebml_lacing",
                    "first EBML lace size exceeds this platform",
                )
            })?;
            sizes.push(first);
            let mut previous = i128::try_from(first).map_err(|_| {
                corrupt(
                    "parse_ebml_lacing",
                    "EBML lace size exceeds the signed range",
                )
            })?;
            for _ in 1..lace_count - 1 {
                let (encoded, width) = read_data_vint(payload, cursor, payload.len())?;
                cursor += width;
                let bias = (1_i128 << (width * 7 - 1)) - 1;
                let delta = i128::from(encoded) - bias;
                let current = previous
                    .checked_add(delta)
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| {
                        corrupt(
                            "parse_ebml_lacing",
                            "EBML lace delta produces a negative size",
                        )
                    })?;
                sizes.push(usize::try_from(current).map_err(|_| {
                    corrupt("parse_ebml_lacing", "EBML lace size exceeds this platform")
                })?);
                previous = current;
            }
        }
        _ => return Err(corrupt("parse_lacing", "Block uses an unknown lacing mode")),
    }

    let declared = sizes.iter().try_fold(0_usize, |total, size| {
        total
            .checked_add(*size)
            .ok_or_else(|| corrupt("parse_lacing", "lace sizes overflowed"))
    })?;
    let remaining = payload
        .len()
        .checked_sub(cursor)
        .ok_or_else(|| corrupt("parse_lacing", "lace header extends past the Block payload"))?;
    if declared > remaining {
        return Err(corrupt(
            "parse_lacing",
            "lace frame sizes exceed the Block payload",
        ));
    }
    sizes.push(remaining - declared);
    let mut ranges = Vec::with_capacity(lace_count);
    for size in sizes {
        let end = cursor
            .checked_add(size)
            .filter(|end| *end <= payload.len())
            .ok_or_else(|| corrupt("parse_lacing", "lace frame extends past the Block payload"))?;
        ranges.push(cursor..end);
        cursor = end;
    }
    if cursor != payload.len() {
        return Err(corrupt(
            "parse_lacing",
            "lace frames do not consume the Block payload",
        ));
    }
    Ok((ranges, lace_count))
}

fn validate_packet_size(size: usize) -> Result<()> {
    if size > MAX_PACKET_BYTES {
        return Err(resource_limit(
            "parse_block",
            "packet size exceeds the bounded delivery limit",
        ));
    }
    Ok(())
}

fn checked_block_metadata_total(current: usize, additional: usize) -> Result<usize> {
    let total = current.checked_add(additional).ok_or_else(|| {
        resource_limit(
            "copy_block_metadata",
            "per-block metadata byte count overflowed",
        )
    })?;
    if total > MAX_BLOCK_METADATA_BYTES {
        return Err(resource_limit(
            "copy_block_metadata",
            "copied metadata exceeds the per-block byte limit",
        ));
    }
    Ok(total)
}

fn duration_to_ns(duration_ticks: Option<f64>, timestamp_scale_ns: u64) -> Result<Option<u64>> {
    duration_ticks
        .map(|duration| duration * timestamp_scale_ns as f64)
        .map(|duration| {
            if !(0.0..U64_EXCLUSIVE_F64).contains(&duration) {
                Err(corrupt(
                    "finalize",
                    "Segment Duration overflows nanoseconds",
                ))
            } else {
                Ok(duration.round() as u64)
            }
        })
        .transpose()
}

fn read_id(data: &[u8], position: usize, limit: usize) -> Result<(u32, usize)> {
    let first = *data
        .get(position)
        .filter(|_| position < limit)
        .ok_or_else(|| corrupt("read_element_id", "element ID is truncated"))?;
    if first == 0 {
        return Err(corrupt(
            "read_element_id",
            "element ID begins with a forbidden zero byte",
        ));
    }
    let width = first.leading_zeros() as usize + 1;
    if width > 4 {
        return Err(corrupt(
            "read_element_id",
            "element ID exceeds the Matroska four-byte limit",
        ));
    }
    let end = position
        .checked_add(width)
        .filter(|end| *end <= limit && *end <= data.len())
        .ok_or_else(|| corrupt("read_element_id", "element ID is truncated"))?;
    let mut id = 0_u32;
    for byte in &data[position..end] {
        id = (id << 8) | u32::from(*byte);
    }
    Ok((id, width))
}

fn peek_id(data: &[u8], position: usize, limit: usize) -> Result<u32> {
    read_id(data, position, limit).map(|value| value.0)
}

fn read_size(data: &[u8], position: usize, limit: usize) -> Result<(u64, usize, bool)> {
    let first = *data
        .get(position)
        .filter(|_| position < limit)
        .ok_or_else(|| corrupt("read_element_size", "element data size is truncated"))?;
    if first == 0 {
        return Err(corrupt(
            "read_element_size",
            "element data size begins with a forbidden zero byte",
        ));
    }
    let width = first.leading_zeros() as usize + 1;
    if width > 8 {
        return Err(corrupt(
            "read_element_size",
            "element data size exceeds the EBML eight-byte limit",
        ));
    }
    let end = position
        .checked_add(width)
        .filter(|end| *end <= limit && *end <= data.len())
        .ok_or_else(|| corrupt("read_element_size", "element data size is truncated"))?;
    let marker = 1_u8 << (8 - width);
    let mut value = u64::from(first & (marker - 1));
    for byte in &data[position + 1..end] {
        value = (value << 8) | u64::from(*byte);
    }
    let unknown = value == (1_u64 << (width * 7)) - 1;
    Ok((value, width, unknown))
}

fn read_data_vint(data: &[u8], position: usize, limit: usize) -> Result<(u64, usize)> {
    let first = *data
        .get(position)
        .filter(|_| position < limit)
        .ok_or_else(|| corrupt("read_data_vint", "VINT data is truncated"))?;
    if first == 0 {
        return Err(corrupt(
            "read_data_vint",
            "VINT data begins with a forbidden zero byte",
        ));
    }
    let width = first.leading_zeros() as usize + 1;
    if width > 8 {
        return Err(corrupt("read_data_vint", "VINT data exceeds eight bytes"));
    }
    let end = position
        .checked_add(width)
        .filter(|end| *end <= limit && *end <= data.len())
        .ok_or_else(|| corrupt("read_data_vint", "VINT data is truncated"))?;
    let marker = 1_u8 << (8 - width);
    let mut value = u64::from(first & (marker - 1));
    for byte in &data[position + 1..end] {
        value = (value << 8) | u64::from(*byte);
    }
    Ok((value, width))
}

fn read_unsigned(bytes: &[u8], field: &'static str) -> Result<u64> {
    if bytes.len() > 8 {
        return Err(corrupt(
            "read_unsigned",
            "unsigned integer element exceeds eight bytes",
        )
        .with_context(ErrorContext::new(COMPONENT, "identify_field").with_field("field", field)));
    }
    let mut value = 0_u64;
    for byte in bytes {
        value = (value << 8) | u64::from(*byte);
    }
    Ok(value)
}

fn read_signed(bytes: &[u8], field: &'static str) -> Result<i64> {
    if bytes.len() > 8 {
        return Err(
            corrupt("read_signed", "signed integer element exceeds eight bytes").with_context(
                ErrorContext::new(COMPONENT, "identify_field").with_field("field", field),
            ),
        );
    }
    if bytes.is_empty() {
        return Ok(0);
    }
    let mut value = if bytes[0] & 0x80 == 0 { 0_i64 } else { -1_i64 };
    for byte in bytes {
        value = (value << 8) | i64::from(*byte);
    }
    Ok(value)
}

fn read_float(bytes: &[u8], field: &'static str) -> Result<f64> {
    let value = match bytes {
        [] => 0.0,
        [a, b, c, d] => f64::from(f32::from_be_bytes([*a, *b, *c, *d])),
        [a, b, c, d, e, f, g, h] => f64::from_be_bytes([*a, *b, *c, *d, *e, *f, *g, *h]),
        _ => {
            return Err(corrupt(
                "read_float",
                "float element must contain zero, four, or eight bytes",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "identify_field").with_field("field", field),
            ))
        }
    };
    Ok(value)
}

fn trim_trailing_nul(mut bytes: &[u8]) -> &[u8] {
    while bytes.last() == Some(&0) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn require_known(element: Element, name: &'static str) -> Result<()> {
    if element.unknown_size {
        return Err(corrupt(
            "parse_master",
            "master element uses an unsupported unknown size",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "identify_master").with_field("element", name),
        ));
    }
    Ok(())
}

fn is_top_level(id: u32) -> bool {
    matches!(
        id,
        ID_SEEK_HEAD
            | ID_INFO
            | ID_TRACKS
            | ID_CLUSTER
            | ID_CUES
            | ID_ATTACHMENTS
            | ID_CHAPTERS
            | ID_TAGS
    )
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn resource_limit(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("limit", "parser-resource"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_packet_sizes_beyond_the_delivery_bound() {
        assert!(validate_packet_size(MAX_PACKET_BYTES).is_ok());
        let error = validate_packet_size(MAX_PACKET_BYTES + 1).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    }

    #[test]
    fn rejects_block_metadata_beyond_the_copy_bound() {
        assert!(checked_block_metadata_total(MAX_BLOCK_METADATA_BYTES, 0).is_ok());
        let error = checked_block_metadata_total(MAX_BLOCK_METADATA_BYTES, 1).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    }

    #[test]
    fn rejects_duration_at_the_exclusive_u64_boundary() {
        let error = duration_to_ns(Some(U64_EXCLUSIVE_F64), 1).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::CorruptData);
    }
}
