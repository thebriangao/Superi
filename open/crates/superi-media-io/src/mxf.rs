//! In-tree Material Exchange Format source probing and demuxing.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
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
    SourceProbe, SourceProbeResult, SourceRequest, StreamEdit, StreamId, StreamInfo, StreamKind,
};
use crate::encode::{Encoder, EncoderConfig};
use crate::mxf_parser::{
    self, EssenceElement, IndexEntry, IndexSegment, MetadataSet, ParsedMxf, PartitionKind,
    Rational, Ul,
};
use crate::operation::OperationContext;
use crate::read::ReadOutcome;

const COMPONENT: &str = "superi-media-io.mxf";
const PARTITION_PREFIX: [u8; 13] = [
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01,
];
const MAX_RUN_IN_WITH_KEY: usize = 65_535 + 16;
const READ_CHUNK: usize = 64 * 1024;

const SET_PREFACE: u16 = 0x012f;
const SET_ESSENCE_CONTAINER_DATA: u16 = 0x0123;
const SET_MATERIAL_PACKAGE: u16 = 0x0136;
const SET_SOURCE_PACKAGE: u16 = 0x0137;
const SET_TIMELINE_TRACK: u16 = 0x013b;
const SET_SOURCE_CLIP: u16 = 0x0111;
const SET_FILLER: u16 = 0x0109;
const SET_MULTIPLE_DESCRIPTOR: u16 = 0x0144;

/// The in-tree MXF container backend.
pub struct MxfBackend {
    descriptor: BackendDescriptor,
    container: ContainerId,
}

impl MxfBackend {
    /// Creates the MXF source backend.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(
                BackendId::new("mxf")?,
                "Superi Material Exchange Format demuxer",
            )?,
            container: ContainerId::new("mxf")?,
        })
    }
}

impl MediaBackend for MxfBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_mxf_source")?;
        if find_header_key(probe.bytes()).is_some() {
            return Ok(SourceProbeResult::matched(
                self.container.clone(),
                ProbeConfidence::new(100)?,
            ));
        }
        if !probe.is_complete()
            && probe.bytes().len() < MAX_RUN_IN_WITH_KEY
            && probe.source_length() > probe.bytes().len() as u64
        {
            let requested = usize::try_from(probe.source_length())
                .unwrap_or(MAX_RUN_IN_WITH_KEY)
                .min(MAX_RUN_IN_WITH_KEY);
            return SourceProbeResult::need_more_data(requested.max(probe.bytes().len() + 1));
        }
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_mxf_source")?;
        let data = read_source(request.location(), operation)?;
        let fingerprint = sha256_fingerprint(&data, operation)?;
        verify_relink(request, &fingerprint)?;
        operation.check("parse_mxf_source")?;
        let parsed = catch_unwind(AssertUnwindSafe(|| mxf_parser::parse(&data)))
            .map_err(|_| corrupt("parse_container", "MXF parser entered an invalid state"))?
            .map_err(map_parser_error)?;
        operation.check("parse_mxf_source")?;
        Ok(Box::new(MxfSource::from_parsed(
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
        operation.check("create_mxf_decoder")?;
        Err(unsupported_codec_operation("create_decoder", "decode"))
    }

    fn create_encoder(
        &self,
        _config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_mxf_encoder")?;
        Err(unsupported_codec_operation("create_encoder", "encode"))
    }
}

struct MxfSource {
    info: SourceInfo,
    data: Arc<[u8]>,
    tracks: Vec<TrackState>,
}

struct TrackState {
    id: StreamId,
    kind: StreamKind,
    timebase: Timebase,
    samples: Vec<Sample>,
    cursor: usize,
}

#[derive(Clone)]
struct Sample {
    element: EssenceElement,
    presentation: i64,
    decode: i64,
    duration: u64,
    keyframe: bool,
    edit_unit: i64,
    index_entry: Option<IndexEntry>,
}

struct Candidate<'a> {
    package: Option<&'a MetadataSet>,
    package_umid: Option<[u8; 32]>,
    track: Option<&'a MetadataSet>,
    descriptor: Option<&'a MetadataSet>,
    body_sid: u32,
    index_sid: u32,
    track_number: u32,
    track_id: u32,
    elements: Vec<EssenceElement>,
}

impl MxfSource {
    fn from_parsed(
        media_id: superi_core::ids::MediaId,
        fingerprint: String,
        data: Arc<[u8]>,
        parsed: ParsedMxf,
        operation: &OperationContext,
    ) -> Result<Self> {
        operation.check("build_mxf_source")?;
        let graph = MetadataGraph::new(&data, &parsed.metadata_sets)?;
        let primary_package = primary_material_package(&graph)?;
        let candidates = build_candidates(&data, &parsed, &graph)?;
        let mut used_stream_ids = BTreeSet::new();
        let mut stream_infos = Vec::with_capacity(candidates.len());
        let mut tracks = Vec::with_capacity(candidates.len());

        for candidate in candidates {
            operation.check("build_mxf_source")?;
            let id_value = unique_stream_id(
                candidate.track_id,
                candidate.track_number,
                &mut used_stream_ids,
            )?;
            let id = StreamId::new(id_value);
            let kind = stream_kind(candidate.track_number, candidate.descriptor);
            let timebase = track_timebase(&data, &parsed, &candidate)?;
            let origin = candidate
                .track
                .map(|track| optional_i64(&data, track, 0x4b02))
                .transpose()?
                .flatten()
                .unwrap_or(0);
            let duration_value = track_duration(&data, &graph, &candidate)?;
            let duration = Duration::new(duration_value, timebase)?;
            let edits = build_material_edits(
                &data,
                &graph,
                primary_package,
                candidate.package_umid,
                candidate.track_id,
                timebase,
            )?;
            let codec = CodecId::new(codec_id(
                &data,
                candidate.descriptor,
                candidate.track_number,
            )?)?;
            let mut info = StreamInfo::new(id, kind, codec, timebase)
                .with_duration(duration)?
                .with_edits(edits)?;
            info = add_stream_metadata(info, &data, &candidate, origin, kind)?;
            let samples = build_samples(&parsed, &candidate, origin, duration_value)?;
            validate_samples(&samples, data.len())?;
            tracks.push(TrackState {
                id,
                kind,
                timebase,
                samples,
                cursor: 0,
            });
            stream_infos.push(info);
        }

        tracks.sort_by_key(|track| track.id);
        stream_infos.sort_by_key(StreamInfo::id);

        let identity = SourceIdentity::new(media_id, fingerprint)?;
        let mut info = SourceInfo::new(identity, stream_infos)?;
        if let Some(duration) = material_package_duration(&data, &graph, primary_package)?
            .or_else(|| source_duration(&tracks))
        {
            info = info.with_duration(duration);
        }
        info = add_source_metadata(info, &data, &parsed, &graph, primary_package)?;
        operation.check("build_mxf_source")?;
        Ok(Self { info, data, tracks })
    }

    fn next_track_index(&self) -> Option<usize> {
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| track.cursor < track.samples.len())
            .min_by(|(_, left), (_, right)| {
                let left_sample = &left.samples[left.cursor];
                let right_sample = &right.samples[right.cursor];
                time_cmp(
                    RationalTime::new(left_sample.decode, left.timebase),
                    RationalTime::new(right_sample.decode, right.timebase),
                )
                .then_with(|| left.id.cmp(&right.id))
            })
            .map(|(index, _)| index)
    }
}

impl MediaSource for MxfSource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        operation.check("read_mxf_packet")?;
        let Some(track_index) = self.next_track_index() else {
            return Ok(ReadOutcome::EndOfStream);
        };
        let track = &mut self.tracks[track_index];
        let sample = &track.samples[track.cursor];
        let start = usize::try_from(sample.element.value_offset)
            .map_err(|_| corrupt("read_packet", "essence offset cannot be represented"))?;
        let length = usize::try_from(sample.element.value_length)
            .map_err(|_| corrupt("read_packet", "essence length cannot be represented"))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| corrupt("read_packet", "essence byte range overflowed"))?;
        let bytes = self
            .data
            .get(start..end)
            .ok_or_else(|| corrupt("read_packet", "essence byte range exceeds the source"))?;
        let timing = PacketTiming::new(
            track.timebase,
            Some(sample.presentation),
            Some(sample.decode),
            Some(sample.duration),
        )?;
        let mut packet = Packet::new(track.id, Arc::from(bytes), timing)
            .with_keyframe(sample.keyframe)
            .with_metadata(
                "container.offset",
                MetadataValue::Unsigned(sample.element.value_offset),
            )?
            .with_metadata(
                "container.size",
                MetadataValue::Unsigned(sample.element.value_length),
            )?
            .with_metadata(
                "mxf.klv-offset",
                MetadataValue::Unsigned(sample.element.klv_offset),
            )?
            .with_metadata(
                "mxf.body-sid",
                MetadataValue::Unsigned(u64::from(sample.element.body_sid)),
            )?
            .with_metadata(
                "mxf.track-number",
                MetadataValue::Unsigned(u64::from(sample.element.track_number)),
            )?
            .with_metadata("mxf.edit-unit", MetadataValue::Signed(sample.edit_unit))?;
        if let Some(index) = sample.index_entry.as_ref() {
            packet = packet
                .with_metadata(
                    "mxf.temporal-offset",
                    MetadataValue::Signed(i64::from(index.temporal_offset)),
                )?
                .with_metadata(
                    "mxf.key-frame-offset",
                    MetadataValue::Signed(i64::from(index.key_frame_offset)),
                )?
                .with_metadata(
                    "mxf.index-flags",
                    MetadataValue::Unsigned(u64::from(index.flags)),
                )?
                .with_metadata(
                    "mxf.index-stream-offset",
                    MetadataValue::Unsigned(index.stream_offset),
                )?;
            if !index.slice_offsets.is_empty() {
                packet = packet.with_metadata(
                    "mxf.slice-offsets",
                    MetadataValue::Text(join_u32(&index.slice_offsets)),
                )?;
            }
            if !index.position_table.is_empty() {
                packet = packet.with_metadata(
                    "mxf.position-table",
                    MetadataValue::Text(join_rationals(&index.position_table)),
                )?;
            }
        }
        operation.check("read_mxf_packet")?;
        track.cursor += 1;
        Ok(ReadOutcome::Complete(packet))
    }

    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime> {
        operation.check("seek_mxf_source")?;
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
            .ok_or_else(|| unsupported_container("seek", "MXF source has no seekable packets"))?;
        let selected = select_seek_sample(&self.tracks[anchor_index], request)?;
        let actual = RationalTime::new(
            self.tracks[anchor_index].samples[selected].presentation,
            self.tracks[anchor_index].timebase,
        );
        let mut cursors = Vec::with_capacity(self.tracks.len());
        for (index, track) in self.tracks.iter().enumerate() {
            operation.check("seek_mxf_source")?;
            let cursor = if index == anchor_index {
                selected
            } else {
                track
                    .samples
                    .iter()
                    .position(|sample| {
                        time_cmp(
                            RationalTime::new(sample.presentation, track.timebase),
                            actual,
                        ) != Ordering::Less
                    })
                    .unwrap_or(track.samples.len())
            };
            cursors.push(cursor);
        }
        operation.check("seek_mxf_source")?;
        for (track, cursor) in self.tracks.iter_mut().zip(cursors) {
            track.cursor = cursor;
        }
        Ok(actual)
    }
}

struct MetadataGraph<'a> {
    data: &'a [u8],
    sets: &'a [MetadataSet],
    by_uid: BTreeMap<[u8; 16], usize>,
}

impl<'a> MetadataGraph<'a> {
    fn new(data: &'a [u8], sets: &'a [MetadataSet]) -> Result<Self> {
        let mut by_uid = BTreeMap::new();
        for (index, set) in sets.iter().enumerate() {
            if let Some(uid) = set.instance_uid(data) {
                if by_uid.insert(uid, index).is_some() {
                    return Err(corrupt(
                        "build_metadata_graph",
                        "MXF contains duplicate metadata instance identifiers",
                    ));
                }
            }
        }
        Ok(Self { data, sets, by_uid })
    }

    fn resolve(&self, uid: [u8; 16]) -> Option<&'a MetadataSet> {
        self.by_uid.get(&uid).map(|index| &self.sets[*index])
    }

    fn reference(&self, set: &MetadataSet, tag: u16) -> Result<Option<&'a MetadataSet>> {
        let Some(uid) = optional_uid(self.data, set, tag)? else {
            return Ok(None);
        };
        self.resolve(uid).map(Some).ok_or_else(|| {
            corrupt(
                "resolve_metadata_reference",
                "MXF metadata references a missing set",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "identify_metadata_reference")
                    .with_field("local_tag", format!("{tag:04x}"))
                    .with_field("instance_uid", hex(uid)),
            )
        })
    }

    fn references(&self, set: &MetadataSet, tag: u16) -> Result<Vec<&'a MetadataSet>> {
        let Some(uids) = optional_reference_batch(self.data, set, tag)? else {
            return Ok(Vec::new());
        };
        uids.into_iter()
            .map(|uid| {
                self.resolve(uid).ok_or_else(|| {
                    corrupt(
                        "resolve_metadata_reference",
                        "MXF metadata references a missing set",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "identify_metadata_reference")
                            .with_field("local_tag", format!("{tag:04x}"))
                            .with_field("instance_uid", hex(uid)),
                    )
                })
            })
            .collect()
    }
}

fn build_candidates<'a>(
    data: &[u8],
    parsed: &ParsedMxf,
    graph: &'a MetadataGraph<'a>,
) -> Result<Vec<Candidate<'a>>> {
    let mut grouped = BTreeMap::<(u32, u32), Vec<EssenceElement>>::new();
    for element in &parsed.essence_elements {
        grouped
            .entry((element.body_sid, element.track_number))
            .or_default()
            .push(element.clone());
    }
    for elements in grouped.values_mut() {
        elements.sort_by_key(|element| element.klv_offset);
    }

    let mut mappings = Vec::new();
    for set in graph
        .sets
        .iter()
        .filter(|set| set.set_kind == SET_ESSENCE_CONTAINER_DATA)
    {
        let Some(package_umid) = optional_umid(data, set, 0x2701)? else {
            continue;
        };
        mappings.push((
            package_umid,
            optional_u32(data, set, 0x3f07)?.unwrap_or(0),
            optional_u32(data, set, 0x3f06)?.unwrap_or(0),
        ));
    }

    let mut candidates = Vec::new();
    let mut claimed = BTreeSet::new();
    for package in graph
        .sets
        .iter()
        .filter(|set| set.set_kind == SET_SOURCE_PACKAGE)
    {
        let package_umid = optional_umid(data, package, 0x4401)?;
        let mapping = package_umid.and_then(|umid| {
            mappings
                .iter()
                .find(|(candidate, _, _)| *candidate == umid)
                .copied()
        });
        let descriptors = package_descriptors(graph, package)?;
        for track in graph.references(package, 0x4403)? {
            if track.set_kind != SET_TIMELINE_TRACK {
                continue;
            }
            let Some(track_number) = optional_u32(data, track, 0x4804)? else {
                continue;
            };
            if track_number == 0 {
                continue;
            }
            let track_id = optional_u32(data, track, 0x4801)?.unwrap_or(0);
            let descriptor = select_descriptor(data, &descriptors, track_id)?;
            let keys = grouped
                .keys()
                .filter(|(body_sid, number)| {
                    *number == track_number
                        && mapping
                            .map(|(_, mapped_body_sid, _)| {
                                mapped_body_sid == 0 || *body_sid == mapped_body_sid
                            })
                            .unwrap_or(true)
                })
                .copied()
                .collect::<Vec<_>>();
            for key in keys {
                let elements = grouped.get(&key).cloned().unwrap_or_default();
                if elements.is_empty() {
                    continue;
                }
                claimed.insert(key);
                candidates.push(Candidate {
                    package: Some(package),
                    package_umid,
                    track: Some(track),
                    descriptor,
                    body_sid: key.0,
                    index_sid: mapping.map(|(_, _, index_sid)| index_sid).unwrap_or(0),
                    track_number,
                    track_id,
                    elements,
                });
            }
        }
    }

    for (key, elements) in grouped {
        if claimed.contains(&key) {
            continue;
        }
        candidates.push(Candidate {
            package: None,
            package_umid: None,
            track: None,
            descriptor: None,
            body_sid: key.0,
            index_sid: 0,
            track_number: key.1,
            track_id: 0,
            elements,
        });
    }
    candidates.sort_by_key(|candidate| {
        (
            candidate
                .elements
                .first()
                .map(|element| element.klv_offset)
                .unwrap_or(u64::MAX),
            candidate.body_sid,
            candidate.track_number,
        )
    });
    if candidates.is_empty() {
        return Err(corrupt(
            "build_streams",
            "MXF contains no usable essence streams",
        ));
    }
    Ok(candidates)
}

fn package_descriptors<'a>(
    graph: &'a MetadataGraph<'a>,
    package: &MetadataSet,
) -> Result<Vec<&'a MetadataSet>> {
    let Some(descriptor) = graph.reference(package, 0x4701)? else {
        return Ok(Vec::new());
    };
    if descriptor.set_kind == SET_MULTIPLE_DESCRIPTOR {
        graph.references(descriptor, 0x3f02)
    } else {
        Ok(vec![descriptor])
    }
}

fn select_descriptor<'a>(
    data: &[u8],
    descriptors: &[&'a MetadataSet],
    track_id: u32,
) -> Result<Option<&'a MetadataSet>> {
    for descriptor in descriptors {
        if optional_u32(data, descriptor, 0x3006)? == Some(track_id) {
            return Ok(Some(descriptor));
        }
    }
    Ok((descriptors.len() == 1).then_some(descriptors[0]))
}

fn primary_material_package<'a>(graph: &'a MetadataGraph<'a>) -> Result<Option<&'a MetadataSet>> {
    if let Some(preface) = graph.sets.iter().find(|set| set.set_kind == SET_PREFACE) {
        if let Some(package) = graph.reference(preface, 0x3b08)? {
            if package.set_kind != SET_MATERIAL_PACKAGE {
                return Err(corrupt(
                    "resolve_primary_package",
                    "MXF primary package is not a material package",
                ));
            }
            return Ok(Some(package));
        }
    }
    Ok(graph
        .sets
        .iter()
        .find(|set| set.set_kind == SET_MATERIAL_PACKAGE))
}

fn track_timebase(data: &[u8], parsed: &ParsedMxf, candidate: &Candidate<'_>) -> Result<Timebase> {
    if let Some(rate) = candidate
        .track
        .map(|track| optional_rational(data, track, 0x4b01))
        .transpose()?
        .flatten()
    {
        return rational_timebase(rate, "track_edit_rate");
    }
    if let Some(rate) = candidate
        .descriptor
        .map(|descriptor| optional_rational(data, descriptor, 0x3001))
        .transpose()?
        .flatten()
    {
        return rational_timebase(rate, "descriptor_sample_rate");
    }
    if let Some(segment) = matching_index_segments(parsed, candidate).first() {
        return rational_timebase(segment.edit_rate, "index_edit_rate");
    }
    Timebase::integer(1)
}

fn track_duration(
    data: &[u8],
    graph: &MetadataGraph<'_>,
    candidate: &Candidate<'_>,
) -> Result<u64> {
    if let Some(track) = candidate.track {
        if let Some(sequence) = graph.reference(track, 0x4803)? {
            if let Some(duration) = optional_i64(data, sequence, 0x0202)? {
                if duration >= 0 {
                    return Ok(duration as u64);
                }
            }
        }
    }
    if let Some(descriptor) = candidate.descriptor {
        if let Some(duration) = optional_i64(data, descriptor, 0x3002)? {
            if duration >= 0 {
                return Ok(duration as u64);
            }
        }
    }
    u64::try_from(candidate.elements.len()).map_err(|_| {
        corrupt(
            "read_stream_duration",
            "stream duration cannot be represented",
        )
    })
}

fn build_material_edits(
    data: &[u8],
    graph: &MetadataGraph<'_>,
    material_package: Option<&MetadataSet>,
    source_package_umid: Option<[u8; 32]>,
    source_track_id: u32,
    stream_timebase: Timebase,
) -> Result<Vec<StreamEdit>> {
    let (Some(package), Some(source_package_umid)) = (material_package, source_package_umid) else {
        return Ok(Vec::new());
    };
    for material_track in graph.references(package, 0x4403)? {
        let Some(rate) = optional_rational(data, material_track, 0x4b01)? else {
            continue;
        };
        let material_timebase = rational_timebase(rate, "material_edit_rate")?;
        let Some(sequence) = graph.reference(material_track, 0x4803)? else {
            continue;
        };
        let components = graph.references(sequence, 0x1001)?;
        let mut targets_stream = false;
        for component in &components {
            if component.set_kind == SET_SOURCE_CLIP
                && optional_umid(data, component, 0x1101)? == Some(source_package_umid)
                && optional_u32(data, component, 0x1102)? == Some(source_track_id)
            {
                targets_stream = true;
                break;
            }
        }
        if !targets_stream {
            continue;
        }
        let mut edits = Vec::with_capacity(components.len());
        for component in components {
            let duration = optional_i64(data, component, 0x0202)?.ok_or_else(|| {
                corrupt(
                    "build_stream_edits",
                    "material sequence component has no duration",
                )
            })?;
            if duration < 0 {
                return Err(corrupt(
                    "build_stream_edits",
                    "material sequence component has an unknown duration",
                ));
            }
            let segment_duration = Duration::new(duration as u64, material_timebase)?;
            let is_target = component.set_kind == SET_SOURCE_CLIP
                && optional_umid(data, component, 0x1101)? == Some(source_package_umid)
                && optional_u32(data, component, 0x1102)? == Some(source_track_id);
            let media_time = if is_target {
                let start = optional_i64(data, component, 0x1201)?.unwrap_or(0);
                Some(
                    RationalTime::new(start, material_timebase)
                        .checked_rescale(stream_timebase, TimeRounding::Exact)
                        .map_err(|_| {
                            corrupt(
                                "build_stream_edits",
                                "source clip start is not exact in the essence edit rate",
                            )
                        })?,
                )
            } else if component.set_kind == SET_FILLER || component.set_kind == SET_SOURCE_CLIP {
                None
            } else {
                return Err(unsupported_container(
                    "build_stream_edits",
                    "material sequence uses an unsupported structural component",
                ));
            };
            edits.push(StreamEdit::new(segment_duration, media_time, 1, 0));
        }
        return Ok(edits);
    }
    Ok(Vec::new())
}

fn build_samples(
    parsed: &ParsedMxf,
    candidate: &Candidate<'_>,
    origin: i64,
    stream_duration: u64,
) -> Result<Vec<Sample>> {
    let segments = matching_index_segments(parsed, candidate);
    let mut indexed = Vec::new();
    for segment in segments {
        for (entry_index, entry) in segment.entries.iter().enumerate() {
            let entry_index = i64::try_from(entry_index).map_err(|_| {
                corrupt(
                    "build_packet_index",
                    "index entry position cannot be represented",
                )
            })?;
            let position = segment
                .start_position
                .checked_add(entry_index)
                .ok_or_else(|| corrupt("build_packet_index", "index entry position overflowed"))?;
            indexed.push((position, entry.clone()));
        }
    }
    indexed.sort_by_key(|(position, _)| *position);
    let clip_wrapped_duration = if candidate.elements.len() == 1 {
        stream_duration.max(1)
    } else {
        1
    };
    candidate
        .elements
        .iter()
        .enumerate()
        .map(|(ordinal, element)| {
            let ordinal_i64 = i64::try_from(ordinal)
                .map_err(|_| corrupt("build_packets", "packet ordinal cannot be represented"))?;
            let (edit_unit, index_entry) = indexed
                .get(ordinal)
                .map(|(position, entry)| (*position, Some(entry.clone())))
                .unwrap_or((ordinal_i64, None));
            let decode = edit_unit
                .checked_sub(origin)
                .ok_or_else(|| corrupt("build_packets", "packet timestamp overflowed"))?;
            let presentation = decode
                .checked_add(
                    index_entry
                        .as_ref()
                        .map_or(0, |entry| i64::from(entry.temporal_offset)),
                )
                .ok_or_else(|| corrupt("build_packets", "packet timestamp overflowed"))?;
            let keyframe = index_entry
                .as_ref()
                .map(IndexEntry::is_random_access)
                .unwrap_or(true);
            Ok(Sample {
                element: element.clone(),
                presentation,
                decode,
                duration: clip_wrapped_duration,
                keyframe,
                edit_unit,
                index_entry,
            })
        })
        .collect()
}

fn matching_index_segments<'a>(
    parsed: &'a ParsedMxf,
    candidate: &Candidate<'_>,
) -> Vec<&'a IndexSegment> {
    let mut segments = parsed
        .index_segments
        .iter()
        .filter(|segment| {
            segment.body_sid == candidate.body_sid
                && (candidate.index_sid == 0 || segment.index_sid == candidate.index_sid)
        })
        .collect::<Vec<_>>();
    segments.sort_by_key(|segment| segment.start_position);
    segments
}

fn add_source_metadata(
    mut info: SourceInfo,
    data: &[u8],
    parsed: &ParsedMxf,
    graph: &MetadataGraph<'_>,
    primary_package: Option<&MetadataSet>,
) -> Result<SourceInfo> {
    let header = parsed
        .partitions
        .first()
        .ok_or_else(|| corrupt("build_source_info", "MXF header partition is missing"))?;
    info = info
        .with_metadata("container.kind", MetadataValue::Text("mxf".to_owned()))?
        .with_metadata("mxf.run-in-bytes", MetadataValue::Unsigned(parsed.run_in))?
        .with_metadata(
            "mxf.partition-count",
            MetadataValue::Unsigned(parsed.partitions.len() as u64),
        )?
        .with_metadata(
            "mxf.metadata-set-count",
            MetadataValue::Unsigned(parsed.metadata_sets.len() as u64),
        )?
        .with_metadata(
            "mxf.essence-element-count",
            MetadataValue::Unsigned(parsed.essence_elements.len() as u64),
        )?
        .with_metadata(
            "mxf.index-segment-count",
            MetadataValue::Unsigned(parsed.index_segments.len() as u64),
        )?
        .with_metadata(
            "mxf.random-index-entry-count",
            MetadataValue::Unsigned(parsed.random_index.len() as u64),
        )?
        .with_metadata(
            "mxf.primer-mapping-count",
            MetadataValue::Unsigned(parsed.primer_mapping_count as u64),
        )?
        .with_metadata(
            "mxf.dark-klv-count",
            MetadataValue::Unsigned(parsed.dark_klv_count),
        )?
        .with_metadata(
            "mxf.operational-pattern",
            MetadataValue::Text(hex(header.operational_pattern)),
        )?
        .with_metadata("mxf.header-closed", MetadataValue::Boolean(header.closed))?
        .with_metadata(
            "mxf.header-complete",
            MetadataValue::Boolean(header.complete),
        )?
        .with_metadata(
            "mxf.major-version",
            MetadataValue::Unsigned(u64::from(header.major_version)),
        )?
        .with_metadata(
            "mxf.minor-version",
            MetadataValue::Unsigned(u64::from(header.minor_version)),
        )?
        .with_metadata(
            "mxf.kag-size",
            MetadataValue::Unsigned(u64::from(header.kag_size)),
        )?
        .with_metadata(
            "mxf.footer-present",
            MetadataValue::Boolean(
                parsed
                    .partitions
                    .iter()
                    .any(|partition| partition.kind == PartitionKind::Footer),
            ),
        )?;
    let body_sids = parsed
        .partitions
        .iter()
        .filter_map(|partition| (partition.body_sid != 0).then_some(partition.body_sid))
        .collect::<BTreeSet<_>>();
    info = info.with_metadata(
        "mxf.body-sids",
        MetadataValue::Text(
            body_sids
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ),
    )?;
    info = info.with_metadata(
        "mxf.essence-containers",
        MetadataValue::Text(
            header
                .essence_containers
                .iter()
                .map(hex)
                .collect::<Vec<_>>()
                .join(","),
        ),
    )?;
    if let Some(package) = primary_package {
        if let Some(name) = optional_utf16(data, package, 0x4402)? {
            info = info.with_metadata("mxf.material-package-name", MetadataValue::Text(name))?;
        }
        if let Some(umid) = optional_umid(data, package, 0x4401)? {
            info =
                info.with_metadata("mxf.material-package-umid", MetadataValue::Text(hex(umid)))?;
        }
        info = add_raw_set_metadata(info, data, package, "mxf.material-package")?;
    }
    if let Some(preface) = graph.sets.iter().find(|set| set.set_kind == SET_PREFACE) {
        info = add_raw_set_metadata(info, data, preface, "mxf.preface")?;
    }
    let source_names = graph
        .sets
        .iter()
        .filter(|set| set.set_kind == SET_SOURCE_PACKAGE)
        .filter_map(|set| optional_utf16(data, set, 0x4402).transpose())
        .collect::<Result<Vec<_>>>()?;
    if !source_names.is_empty() {
        info = info.with_metadata(
            "mxf.source-package-names",
            MetadataValue::Text(source_names.join(",")),
        )?;
    }
    Ok(info)
}

fn add_stream_metadata(
    mut info: StreamInfo,
    data: &[u8],
    candidate: &Candidate<'_>,
    origin: i64,
    kind: StreamKind,
) -> Result<StreamInfo> {
    info = info
        .with_metadata(
            "mxf.track-number",
            MetadataValue::Unsigned(u64::from(candidate.track_number)),
        )?
        .with_metadata(
            "mxf.body-sid",
            MetadataValue::Unsigned(u64::from(candidate.body_sid)),
        )?
        .with_metadata(
            "mxf.index-sid",
            MetadataValue::Unsigned(u64::from(candidate.index_sid)),
        )?
        .with_metadata("mxf.origin", MetadataValue::Signed(origin))?
        .with_metadata(
            "mxf.essence-element-count",
            MetadataValue::Unsigned(candidate.elements.len() as u64),
        )?
        .with_metadata(
            "mxf.clip-wrapped",
            MetadataValue::Boolean(candidate.elements.len() == 1),
        )?;
    if candidate.track_id != 0 {
        info = info.with_metadata(
            "mxf.track-id",
            MetadataValue::Unsigned(u64::from(candidate.track_id)),
        )?;
    }
    if let Some(track) = candidate.track {
        if let Some(name) = optional_utf16(data, track, 0x4802)? {
            info = info.with_metadata("mxf.track-name", MetadataValue::Text(name))?;
        }
        info = add_raw_stream_set_metadata(info, data, track, "mxf.track")?;
    }
    if let Some(package) = candidate.package {
        if let Some(name) = optional_utf16(data, package, 0x4402)? {
            info = info.with_metadata("mxf.source-package-name", MetadataValue::Text(name))?;
        }
    }
    if let Some(umid) = candidate.package_umid {
        info = info.with_metadata("mxf.source-package-umid", MetadataValue::Text(hex(umid)))?;
    }
    if let Some(descriptor) = candidate.descriptor {
        info = info.with_metadata(
            "mxf.descriptor-set",
            MetadataValue::Text(format!("{:04x}", descriptor.set_kind)),
        )?;
        if let Some(rate) = optional_rational(data, descriptor, 0x3001)? {
            info = add_rational_metadata(info, "mxf.sample-rate", rate)?;
        }
        if let Some(container) = optional_ul(data, descriptor, 0x3004)? {
            info =
                info.with_metadata("mxf.essence-container", MetadataValue::Text(hex(container)))?;
        }
        if let Some(codec) = descriptor_codec_ul(data, descriptor)? {
            info = info.with_metadata("mxf.codec-ul", MetadataValue::Text(hex(codec)))?;
        }
        match kind {
            StreamKind::Video => {
                if let Some(width) = optional_u32(data, descriptor, 0x3203)? {
                    info = info
                        .with_metadata("video.width", MetadataValue::Unsigned(u64::from(width)))?;
                }
                if let Some(height) = optional_u32(data, descriptor, 0x3202)? {
                    info = info.with_metadata(
                        "video.height",
                        MetadataValue::Unsigned(u64::from(height)),
                    )?;
                }
                if let Some(aspect) = optional_rational(data, descriptor, 0x320e)? {
                    info = add_rational_metadata(info, "video.aspect-ratio", aspect)?;
                }
            }
            StreamKind::Audio => {
                if let Some(rate) = optional_rational(data, descriptor, 0x3d03)? {
                    info = add_rational_metadata(info, "audio.sample-rate", rate)?;
                }
                if let Some(channels) = optional_u32(data, descriptor, 0x3d07)? {
                    info = info.with_metadata(
                        "audio.channel-count",
                        MetadataValue::Unsigned(u64::from(channels)),
                    )?;
                }
                if let Some(bits) = optional_u32(data, descriptor, 0x3d01)? {
                    info = info.with_metadata(
                        "audio.quantization-bits",
                        MetadataValue::Unsigned(u64::from(bits)),
                    )?;
                }
            }
            _ => {}
        }
        info = add_raw_stream_set_metadata(info, data, descriptor, "mxf.descriptor")?;
    }
    Ok(info)
}

fn add_rational_metadata(
    mut info: StreamInfo,
    prefix: &str,
    value: Rational,
) -> Result<StreamInfo> {
    info = info.with_metadata(
        format!("{prefix}-numerator"),
        if value.numerator >= 0 {
            MetadataValue::Unsigned(value.numerator as u64)
        } else {
            MetadataValue::Signed(i64::from(value.numerator))
        },
    )?;
    info.with_metadata(
        format!("{prefix}-denominator"),
        if value.denominator >= 0 {
            MetadataValue::Unsigned(value.denominator as u64)
        } else {
            MetadataValue::Signed(i64::from(value.denominator))
        },
    )
}

fn add_raw_set_metadata(
    mut info: SourceInfo,
    data: &[u8],
    set: &MetadataSet,
    prefix: &str,
) -> Result<SourceInfo> {
    for item in &set.items {
        info = info.with_metadata(
            format!("{prefix}.tag-{:04x}", item.local_tag),
            MetadataValue::Bytes(Arc::from(item.value(data).map_err(map_parser_error)?)),
        )?;
    }
    Ok(info)
}

fn add_raw_stream_set_metadata(
    mut info: StreamInfo,
    data: &[u8],
    set: &MetadataSet,
    prefix: &str,
) -> Result<StreamInfo> {
    for item in &set.items {
        info = info.with_metadata(
            format!("{prefix}.tag-{:04x}", item.local_tag),
            MetadataValue::Bytes(Arc::from(item.value(data).map_err(map_parser_error)?)),
        )?;
    }
    Ok(info)
}

fn source_duration(tracks: &[TrackState]) -> Option<Duration> {
    let anchor = tracks
        .iter()
        .position(|track| track.kind == StreamKind::Video)
        .or_else(|| {
            tracks
                .iter()
                .position(|track| track.kind == StreamKind::Audio)
        })
        .or_else(|| (!tracks.is_empty()).then_some(0))?;
    let track = &tracks[anchor];
    let value = track
        .samples
        .iter()
        .filter_map(|sample| {
            sample
                .presentation
                .checked_add(i64::try_from(sample.duration).ok()?)
        })
        .max()?
        .max(0) as u64;
    Duration::new(value, track.timebase).ok()
}

fn material_package_duration(
    data: &[u8],
    graph: &MetadataGraph<'_>,
    package: Option<&MetadataSet>,
) -> Result<Option<Duration>> {
    let Some(package) = package else {
        return Ok(None);
    };
    for track in graph.references(package, 0x4403)? {
        let Some(rate) = optional_rational(data, track, 0x4b01)? else {
            continue;
        };
        let Some(sequence) = graph.reference(track, 0x4803)? else {
            continue;
        };
        let Some(duration) = optional_i64(data, sequence, 0x0202)? else {
            continue;
        };
        if duration < 0 {
            continue;
        }
        return Ok(Some(Duration::new(
            duration as u64,
            rational_timebase(rate, "material_duration_rate")?,
        )?));
    }
    Ok(None)
}

fn select_seek_sample(track: &TrackState, request: SeekRequest) -> Result<usize> {
    match request.mode() {
        SeekMode::Exact => track
            .samples
            .iter()
            .position(|sample| {
                RationalTime::new(sample.presentation, track.timebase) == request.target()
            })
            .ok_or_else(|| invalid_seek("exact seek target is not an MXF edit-unit boundary")),
        SeekMode::PreviousKeyframe => track
            .samples
            .iter()
            .enumerate()
            .filter(|(_, sample)| {
                sample.keyframe
                    && time_cmp(
                        RationalTime::new(sample.presentation, track.timebase),
                        request.target(),
                    ) != Ordering::Greater
            })
            .map(|(index, _)| index)
            .next_back()
            .ok_or_else(|| invalid_seek("no random-access edit unit exists before the target")),
        SeekMode::NearestKeyframe => {
            let target = request
                .target()
                .checked_rescale(track.timebase, TimeRounding::NearestTiesEven)?
                .value();
            track
                .samples
                .iter()
                .enumerate()
                .filter(|(_, sample)| sample.keyframe)
                .min_by_key(|(_, sample)| {
                    (sample.presentation.abs_diff(target), sample.presentation)
                })
                .map(|(index, _)| index)
                .ok_or_else(|| invalid_seek("MXF stream contains no random-access edit units"))
        }
    }
}

fn unique_stream_id(track_id: u32, track_number: u32, used: &mut BTreeSet<u32>) -> Result<u32> {
    for candidate in [track_id, track_number] {
        if candidate != 0 && used.insert(candidate) {
            return Ok(candidate);
        }
    }
    let candidate = (1..=u32::MAX)
        .find(|candidate| used.insert(*candidate))
        .ok_or_else(|| {
            corrupt(
                "assign_stream_id",
                "MXF stream identifier space is exhausted",
            )
        })?;
    Ok(candidate)
}

fn stream_kind(track_number: u32, descriptor: Option<&MetadataSet>) -> StreamKind {
    match (track_number >> 24) as u8 {
        0x15 => StreamKind::Video,
        0x16 => StreamKind::Audio,
        0x17 | 0x14 | 0x18 => StreamKind::Data,
        _ => match descriptor.map(|value| value.set_kind) {
            Some(0x0127..=0x0129 | 0x0151) => StreamKind::Video,
            Some(0x0142 | 0x0147 | 0x0148) => StreamKind::Audio,
            _ => StreamKind::Data,
        },
    }
}

fn codec_id(data: &[u8], descriptor: Option<&MetadataSet>, track_number: u32) -> Result<String> {
    if let Some(descriptor) = descriptor {
        if let Some(codec) = descriptor_codec_ul(data, descriptor)? {
            return Ok(format!("mxf-ul-{}", hex(codec)));
        }
    }
    Ok(format!("mxf-gc-{track_number:08x}"))
}

fn descriptor_codec_ul(data: &[u8], descriptor: &MetadataSet) -> Result<Option<Ul>> {
    for tag in [0x3005, 0x3201, 0x3d06] {
        if let Some(value) = optional_ul(data, descriptor, tag)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn rational_timebase(value: Rational, field: &'static str) -> Result<Timebase> {
    let numerator = u32::try_from(value.numerator).map_err(|_| {
        corrupt("read_timebase", "MXF edit rate numerator must be positive").with_context(
            ErrorContext::new(COMPONENT, "identify_timebase").with_field("field", field),
        )
    })?;
    let denominator = u32::try_from(value.denominator).map_err(|_| {
        corrupt(
            "read_timebase",
            "MXF edit rate denominator must be positive",
        )
        .with_context(ErrorContext::new(COMPONENT, "identify_timebase").with_field("field", field))
    })?;
    Timebase::new(numerator, denominator).map_err(|_| {
        corrupt("read_timebase", "MXF edit rate is invalid").with_context(
            ErrorContext::new(COMPONENT, "identify_timebase").with_field("field", field),
        )
    })
}

fn validate_samples(samples: &[Sample], source_length: usize) -> Result<()> {
    for sample in samples {
        let end = sample
            .element
            .value_offset
            .checked_add(sample.element.value_length)
            .ok_or_else(|| corrupt("validate_essence", "essence byte range overflowed"))?;
        if end > source_length as u64
            || usize::try_from(sample.element.value_offset).is_err()
            || usize::try_from(end).is_err()
        {
            return Err(
                corrupt("validate_essence", "essence byte range exceeds the source").with_context(
                    ErrorContext::new(COMPONENT, "identify_essence")
                        .with_field(
                            "track_number",
                            format!("{:08x}", sample.element.track_number),
                        )
                        .with_field("klv_offset", sample.element.klv_offset.to_string()),
                ),
            );
        }
    }
    Ok(())
}

fn optional_value<'a>(data: &'a [u8], set: &MetadataSet, tag: u16) -> Result<Option<&'a [u8]>> {
    set.item(tag)
        .map(|item| item.value(data).map_err(map_parser_error))
        .transpose()
}

fn optional_u32(data: &[u8], set: &MetadataSet, tag: u16) -> Result<Option<u32>> {
    optional_value(data, set, tag)?
        .map(|value| {
            value
                .try_into()
                .map(u32::from_be_bytes)
                .map_err(|_| invalid_property(set, tag, "MXF UInt32 property has the wrong length"))
        })
        .transpose()
}

fn optional_i64(data: &[u8], set: &MetadataSet, tag: u16) -> Result<Option<i64>> {
    optional_value(data, set, tag)?
        .map(|value| {
            value
                .try_into()
                .map(i64::from_be_bytes)
                .map_err(|_| invalid_property(set, tag, "MXF Int64 property has the wrong length"))
        })
        .transpose()
}

fn optional_rational(data: &[u8], set: &MetadataSet, tag: u16) -> Result<Option<Rational>> {
    optional_value(data, set, tag)?
        .map(|value| {
            let bytes: [u8; 8] = value.try_into().map_err(|_| {
                invalid_property(set, tag, "MXF rational property has the wrong length")
            })?;
            Ok(Rational {
                numerator: i32::from_be_bytes(bytes[..4].try_into().expect("rational numerator")),
                denominator: i32::from_be_bytes(
                    bytes[4..].try_into().expect("rational denominator"),
                ),
            })
        })
        .transpose()
}

fn optional_uid(data: &[u8], set: &MetadataSet, tag: u16) -> Result<Option<[u8; 16]>> {
    optional_value(data, set, tag)?
        .map(|value| {
            value.try_into().map_err(|_| {
                invalid_property(set, tag, "MXF strong reference has the wrong length")
            })
        })
        .transpose()
}

fn optional_umid(data: &[u8], set: &MetadataSet, tag: u16) -> Result<Option<[u8; 32]>> {
    optional_value(data, set, tag)?
        .map(|value| {
            value.try_into().map_err(|_| {
                invalid_property(set, tag, "MXF package identifier has the wrong length")
            })
        })
        .transpose()
}

fn optional_ul(data: &[u8], set: &MetadataSet, tag: u16) -> Result<Option<Ul>> {
    optional_value(data, set, tag)?
        .map(|value| {
            value
                .try_into()
                .map_err(|_| invalid_property(set, tag, "MXF UL property has the wrong length"))
        })
        .transpose()
}

fn optional_utf16(data: &[u8], set: &MetadataSet, tag: u16) -> Result<Option<String>> {
    optional_value(data, set, tag)?
        .map(|value| {
            if value.len() % 2 != 0 {
                return Err(invalid_property(
                    set,
                    tag,
                    "MXF UTF-16 property has an odd byte length",
                ));
            }
            let mut units = value
                .chunks_exact(2)
                .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>();
            while units.last() == Some(&0) {
                units.pop();
            }
            String::from_utf16(&units)
                .map_err(|_| invalid_property(set, tag, "MXF UTF-16 property is not valid Unicode"))
        })
        .transpose()
}

fn optional_reference_batch(
    data: &[u8],
    set: &MetadataSet,
    tag: u16,
) -> Result<Option<Vec<[u8; 16]>>> {
    optional_value(data, set, tag)?
        .map(|value| {
            if value.len() < 8 {
                return Err(invalid_property(
                    set,
                    tag,
                    "MXF reference batch is truncated",
                ));
            }
            let count = u32::from_be_bytes(value[..4].try_into().expect("batch count")) as usize;
            let item_length =
                u32::from_be_bytes(value[4..8].try_into().expect("batch item length")) as usize;
            if item_length != 16 {
                return Err(invalid_property(
                    set,
                    tag,
                    "MXF reference batch item length is invalid",
                ));
            }
            let required = count
                .checked_mul(item_length)
                .and_then(|length| length.checked_add(8))
                .ok_or_else(|| invalid_property(set, tag, "MXF reference batch overflowed"))?;
            if required != value.len() {
                return Err(invalid_property(
                    set,
                    tag,
                    "MXF reference batch size is inconsistent",
                ));
            }
            Ok(value[8..]
                .chunks_exact(16)
                .map(|chunk| chunk.try_into().expect("validated reference length"))
                .collect())
        })
        .transpose()
}

fn invalid_property(set: &MetadataSet, tag: u16, message: &'static str) -> Error {
    corrupt("read_metadata_property", message).with_context(
        ErrorContext::new(COMPONENT, "identify_metadata_property")
            .with_field("set_kind", format!("{:04x}", set.set_kind))
            .with_field("local_tag", format!("{tag:04x}"))
            .with_field("set_offset", set.byte_offset.to_string()),
    )
}

fn find_header_key(bytes: &[u8]) -> Option<usize> {
    bytes[..bytes.len().min(MAX_RUN_IN_WITH_KEY)]
        .windows(16)
        .position(|key| key[..13] == PARTITION_PREFIX && key[13] == 0x02 && key[15] == 0x00)
}

fn read_source(location: &SourceLocation, operation: &OperationContext) -> Result<Arc<[u8]>> {
    match location {
        SourceLocation::Memory { data, .. } => {
            operation.check("read_mxf_source")?;
            Ok(Arc::clone(data))
        }
        SourceLocation::Path(path) => {
            let mut file = File::open(path).map_err(source_read_error)?;
            let mut bytes = Vec::new();
            let mut buffer = vec![0; READ_CHUNK];
            loop {
                operation.check("read_mxf_source")?;
                match file.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => bytes.extend_from_slice(&buffer[..read]),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(source_read_error(error)),
                }
            }
            operation.check("read_mxf_source")?;
            Ok(Arc::from(bytes))
        }
    }
}

fn source_read_error(source: io::Error) -> Error {
    Error::with_source(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "MXF source could not be read",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, "read_source"))
}

fn sha256_fingerprint(data: &[u8], operation: &OperationContext) -> Result<String> {
    let mut digest = Sha256::new();
    for chunk in data.chunks(READ_CHUNK) {
        operation.check("fingerprint_mxf_source")?;
        digest.update(chunk);
    }
    operation.check("fingerprint_mxf_source")?;
    Ok(format!("sha256:{}", hex(digest.finalize())))
}

fn verify_relink(request: &SourceRequest, fingerprint: &str) -> Result<()> {
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
                    .with_field("actual_fingerprint", fingerprint),
            ));
        }
    }
    Ok(())
}

fn map_parser_error(source: mxf_parser::ParseError) -> Error {
    let offset = source.offset();
    Error::with_source(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        "MXF container could not be parsed",
        source,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "parse_container")
            .with_field("byte_offset", offset.to_string()),
    )
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    output
}

fn join_u32(values: &[u32]) -> String {
    values
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn join_rationals(values: &[Rational]) -> String {
    values
        .iter()
        .map(|value| format!("{}/{}", value.numerator, value.denominator))
        .collect::<Vec<_>>()
        .join(",")
}

fn time_cmp(left: RationalTime, right: RationalTime) -> Ordering {
    left.partial_cmp(&right)
        .expect("validated timebases have a total ordering")
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

fn unsupported_codec_operation(operation: &'static str, capability: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        "the MXF container backend does not implement codec processing",
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("capability", capability))
}
