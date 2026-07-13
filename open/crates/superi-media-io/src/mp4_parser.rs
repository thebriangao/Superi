//! Bounds-checked ISO base media and QuickTime atom parsing used by the MP4/MOV backend.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::vfr::MAX_PRESENTATION_FRAMES;

#[derive(Debug)]
pub(crate) struct ParseError {
    message: &'static str,
}

impl ParseError {
    const fn new(message: &'static str) -> Self {
        Self { message }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for ParseError {}

type Result<T> = std::result::Result<T, ParseError>;

const MAX_TABLE_ENTRIES: usize = MAX_PRESENTATION_FRAMES;

#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedMetadata {
    pub(crate) title: Option<String>,
    pub(crate) year: Option<u32>,
    pub(crate) summary: Option<String>,
    pub(crate) poster: Option<Vec<u8>>,
}

impl ParsedMetadata {
    fn merge(&mut self, other: Self) {
        if other.title.is_some() {
            self.title = other.title;
        }
        if other.year.is_some() {
            self.year = other.year;
        }
        if other.summary.is_some() {
            self.summary = other.summary;
        }
        if other.poster.is_some() {
            self.poster = other.poster;
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedMovie {
    pub(crate) major_brand: [u8; 4],
    pub(crate) minor_version: u32,
    pub(crate) compatible_brands: Vec<[u8; 4]>,
    pub(crate) creation_time: u64,
    pub(crate) modification_time: u64,
    pub(crate) timescale: u32,
    pub(crate) duration: u64,
    pub(crate) tracks: Vec<ParsedTrack>,
    pub(crate) metadata: ParsedMetadata,
    pub(crate) fragmented: bool,
    pub(crate) event_message_count: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedTrack {
    pub(crate) id: u32,
    pub(crate) creation_time: u64,
    pub(crate) modification_time: u64,
    pub(crate) header_duration: u64,
    pub(crate) flags: u32,
    pub(crate) layer: i16,
    pub(crate) alternate_group: u16,
    pub(crate) volume: u16,
    pub(crate) matrix: [i32; 9],
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) media_creation_time: u64,
    pub(crate) media_modification_time: u64,
    pub(crate) timescale: u32,
    pub(crate) duration: u64,
    pub(crate) language: String,
    pub(crate) handler_type: [u8; 4],
    pub(crate) handler_name: String,
    pub(crate) codec: [u8; 4],
    pub(crate) codec_string: Option<String>,
    pub(crate) codec_configuration: Option<Vec<u8>>,
    pub(crate) edits: Vec<ParsedEdit>,
    pub(crate) samples: Vec<ParsedSample>,
    pub(crate) metadata: ParsedMetadata,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ParsedEdit {
    pub(crate) segment_duration: u64,
    pub(crate) media_time: i64,
    pub(crate) rate_integer: i16,
    pub(crate) rate_fraction: i16,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ParsedSample {
    pub(crate) id: u32,
    pub(crate) is_sync: bool,
    pub(crate) size: u64,
    pub(crate) offset: u64,
    pub(crate) decode_timestamp: i64,
    pub(crate) composition_timestamp: i64,
    pub(crate) duration: u64,
}

#[derive(Clone, Copy)]
struct Atom<'a> {
    kind: [u8; 4],
    start: usize,
    data: &'a [u8],
}

fn atoms(data: &[u8]) -> Result<Vec<Atom<'_>>> {
    let mut values = Vec::new();
    let mut offset = 0_usize;
    while offset < data.len() {
        if data.len().saturating_sub(offset) < 8 {
            return Err(ParseError::new("atom header is truncated"));
        }
        let size32 = u32::from_be_bytes(
            data[offset..offset + 4]
                .try_into()
                .map_err(|_| ParseError::new("atom size is truncated"))?,
        );
        let kind = data[offset + 4..offset + 8]
            .try_into()
            .map_err(|_| ParseError::new("atom kind is truncated"))?;
        let (header_size, size) = if size32 == 1 {
            if data.len().saturating_sub(offset) < 16 {
                return Err(ParseError::new("extended atom header is truncated"));
            }
            (
                16_usize,
                usize::try_from(u64::from_be_bytes(
                    data[offset + 8..offset + 16]
                        .try_into()
                        .map_err(|_| ParseError::new("extended atom size is truncated"))?,
                ))
                .map_err(|_| ParseError::new("atom size exceeds the platform address space"))?,
            )
        } else if size32 == 0 {
            (8_usize, data.len() - offset)
        } else {
            (8_usize, size32 as usize)
        };
        if size < header_size {
            return Err(ParseError::new("atom size is smaller than its header"));
        }
        let end = offset
            .checked_add(size)
            .ok_or_else(|| ParseError::new("atom byte range overflows"))?;
        if end > data.len() {
            return Err(ParseError::new("atom extends beyond its parent"));
        }
        values.push(Atom {
            kind,
            start: offset,
            data: &data[offset + header_size..end],
        });
        if end == data.len() {
            break;
        }
        offset = end;
    }
    Ok(values)
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| ParseError::new("field byte range overflows"))?;
        let value = self
            .data
            .get(self.position..end)
            .ok_or_else(|| ParseError::new("atom field is truncated"))?;
        self.position = end;
        Ok(value)
    }

    fn skip(&mut self, length: usize) -> Result<()> {
        self.take(length).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| ParseError::new("u16 field is truncated"))?,
        ))
    }

    fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    fn u24(&mut self) -> Result<u32> {
        let bytes = self.take(3)?;
        Ok((u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| ParseError::new("u32 field is truncated"))?,
        ))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| ParseError::new("u64 field is truncated"))?,
        ))
    }

    fn i64(&mut self) -> Result<i64> {
        Ok(self.u64()? as i64)
    }

    fn remaining(&self) -> &'a [u8] {
        &self.data[self.position..]
    }
}

fn full_box(reader: &mut Reader<'_>) -> Result<(u8, u32)> {
    Ok((reader.u8()?, reader.u24()?))
}

fn bounded_table_count(reader: &Reader<'_>, count: u32, bytes_per_entry: usize) -> Result<usize> {
    let count = usize::try_from(count)
        .map_err(|_| ParseError::new("table entry count exceeds this platform"))?;
    if count > MAX_TABLE_ENTRIES {
        return Err(ParseError::new(
            "table entry count exceeds the resource limit",
        ));
    }
    let required = count
        .checked_mul(bytes_per_entry)
        .ok_or_else(|| ParseError::new("table byte count overflows"))?;
    if required > reader.remaining().len() {
        return Err(ParseError::new("table entries extend beyond their atom"));
    }
    Ok(count)
}

#[derive(Clone, Copy, Default)]
struct MovieHeader {
    creation_time: u64,
    modification_time: u64,
    timescale: u32,
    duration: u64,
}

#[derive(Clone, Copy, Default)]
struct TrackHeader {
    id: u32,
    creation_time: u64,
    modification_time: u64,
    duration: u64,
    flags: u32,
    layer: i16,
    alternate_group: u16,
    volume: u16,
    matrix: [i32; 9],
    width: u16,
    height: u16,
}

#[derive(Clone, Default)]
struct MediaHeader {
    creation_time: u64,
    modification_time: u64,
    timescale: u32,
    duration: u64,
    language: String,
}

#[derive(Clone, Default)]
struct Handler {
    kind: [u8; 4],
    name: String,
}

#[derive(Default)]
struct SampleTable {
    codec: [u8; 4],
    codec_string: Option<String>,
    codec_configuration: Option<Vec<u8>>,
    timing: Vec<(u32, u32)>,
    composition: Vec<(u32, i64)>,
    chunks: Vec<SampleToChunk>,
    sizes: Vec<u64>,
    chunk_offsets: Vec<u64>,
    sync_samples: Option<BTreeSet<u32>>,
}

#[derive(Clone, Copy)]
struct SampleToChunk {
    first_chunk: u32,
    samples_per_chunk: u32,
}

#[derive(Clone, Copy, Default)]
struct TrackDefaults {
    duration: u32,
    size: u32,
    flags: u32,
}

struct ParsedMoov {
    header: MovieHeader,
    tracks: Vec<ParsedTrack>,
    defaults: BTreeMap<u32, TrackDefaults>,
    metadata: ParsedMetadata,
}

pub(crate) fn parse(data: &[u8]) -> Result<ParsedMovie> {
    let top_level = atoms(data)?;
    let mut major_brand = *b"qt  ";
    let mut minor_version = 0_u32;
    let mut compatible_brands = vec![*b"qt  "];
    let mut movie_header = None;
    let mut tracks = Vec::new();
    let mut defaults = BTreeMap::new();
    let mut metadata = ParsedMetadata::default();
    let mut moofs = Vec::new();
    let mut event_message_count = 0_usize;

    for atom in &top_level {
        match atom.kind {
            [b'f', b't', b'y', b'p'] => {
                let mut reader = Reader::new(atom.data);
                major_brand = reader
                    .take(4)?
                    .try_into()
                    .map_err(|_| ParseError::new("ftyp major brand is truncated"))?;
                minor_version = reader.u32()?;
                compatible_brands.clear();
                while reader.remaining().len() >= 4 {
                    compatible_brands.push(
                        reader
                            .take(4)?
                            .try_into()
                            .map_err(|_| ParseError::new("ftyp compatible brand is truncated"))?,
                    );
                }
                if !reader.remaining().is_empty() {
                    return Err(ParseError::new("ftyp brands are not four-byte aligned"));
                }
            }
            [b'm', b'o', b'o', b'v'] => {
                let parsed = parse_moov(atom.data)?;
                movie_header = Some(parsed.header);
                tracks = parsed.tracks;
                defaults = parsed.defaults;
                metadata.merge(parsed.metadata);
            }
            [b'm', b'o', b'o', b'f'] => moofs.push(*atom),
            [b'e', b'm', b's', b'g'] => event_message_count += 1,
            _ => {}
        }
    }
    let movie_header = movie_header.ok_or_else(|| ParseError::new("moov atom is missing"))?;
    for moof in &moofs {
        parse_moof(data, *moof, &defaults, &mut tracks)?;
    }
    for track in &mut tracks {
        if track.duration == 0 {
            track.duration = track
                .samples
                .last()
                .and_then(|sample| {
                    u64::try_from(sample.composition_timestamp)
                        .ok()?
                        .checked_add(sample.duration)
                })
                .unwrap_or(0);
        }
    }

    Ok(ParsedMovie {
        major_brand,
        minor_version,
        compatible_brands,
        creation_time: movie_header.creation_time,
        modification_time: movie_header.modification_time,
        timescale: movie_header.timescale,
        duration: movie_header.duration,
        tracks,
        metadata,
        fragmented: !moofs.is_empty(),
        event_message_count,
    })
}

fn parse_moov(data: &[u8]) -> Result<ParsedMoov> {
    let mut header = None;
    let mut tracks = Vec::new();
    let mut defaults = BTreeMap::new();
    let mut metadata = ParsedMetadata::default();
    for atom in atoms(data)? {
        match atom.kind {
            [b'm', b'v', b'h', b'd'] => header = Some(parse_mvhd(atom.data)?),
            [b't', b'r', b'a', b'k'] => tracks.push(parse_trak(atom.data)?),
            [b'm', b'v', b'e', b'x'] => defaults = parse_mvex(atom.data)?,
            [b'm', b'e', b't', b'a'] => metadata.merge(parse_meta(atom.data)?),
            [b'u', b'd', b't', b'a'] => metadata.merge(parse_udta(atom.data)?),
            _ => {}
        }
    }
    Ok(ParsedMoov {
        header: header.ok_or_else(|| ParseError::new("mvhd atom is missing"))?,
        tracks,
        defaults,
        metadata,
    })
}

fn parse_mvhd(data: &[u8]) -> Result<MovieHeader> {
    let mut reader = Reader::new(data);
    let (version, _) = full_box(&mut reader)?;
    let (creation_time, modification_time, timescale, duration) = match version {
        0 => (
            u64::from(reader.u32()?),
            u64::from(reader.u32()?),
            reader.u32()?,
            u64::from(reader.u32()?),
        ),
        1 => (reader.u64()?, reader.u64()?, reader.u32()?, reader.u64()?),
        _ => return Err(ParseError::new("mvhd version is unsupported")),
    };
    Ok(MovieHeader {
        creation_time,
        modification_time,
        timescale,
        duration,
    })
}

fn parse_trak(data: &[u8]) -> Result<ParsedTrack> {
    let mut header = None;
    let mut media = None;
    let mut edits = Vec::new();
    let mut metadata = ParsedMetadata::default();
    for atom in atoms(data)? {
        match atom.kind {
            [b't', b'k', b'h', b'd'] => header = Some(parse_tkhd(atom.data)?),
            [b'm', b'd', b'i', b'a'] => media = Some(parse_mdia(atom.data)?),
            [b'e', b'd', b't', b's'] => edits = parse_edts(atom.data)?,
            [b'm', b'e', b't', b'a'] => metadata.merge(parse_meta(atom.data)?),
            _ => {}
        }
    }
    let header = header.ok_or_else(|| ParseError::new("tkhd atom is missing"))?;
    let (media_header, handler, sample_table) =
        media.ok_or_else(|| ParseError::new("mdia atom is missing"))?;
    let samples = build_samples(&sample_table)?;
    Ok(ParsedTrack {
        id: header.id,
        creation_time: header.creation_time,
        modification_time: header.modification_time,
        header_duration: header.duration,
        flags: header.flags,
        layer: header.layer,
        alternate_group: header.alternate_group,
        volume: header.volume,
        matrix: header.matrix,
        width: header.width,
        height: header.height,
        media_creation_time: media_header.creation_time,
        media_modification_time: media_header.modification_time,
        timescale: media_header.timescale,
        duration: media_header.duration,
        language: media_header.language,
        handler_type: handler.kind,
        handler_name: handler.name,
        codec: sample_table.codec,
        codec_string: sample_table.codec_string,
        codec_configuration: sample_table.codec_configuration,
        edits,
        samples,
        metadata,
    })
}

fn parse_tkhd(data: &[u8]) -> Result<TrackHeader> {
    let mut reader = Reader::new(data);
    let (version, flags) = full_box(&mut reader)?;
    let (creation_time, modification_time, id, duration) = match version {
        0 => (
            u64::from(reader.u32()?),
            u64::from(reader.u32()?),
            reader.u32()?,
            {
                reader.skip(4)?;
                u64::from(reader.u32()?)
            },
        ),
        1 => (reader.u64()?, reader.u64()?, reader.u32()?, {
            reader.skip(4)?;
            reader.u64()?
        }),
        _ => return Err(ParseError::new("tkhd version is unsupported")),
    };
    reader.skip(8)?;
    let layer = reader.i16()?;
    let alternate_group = reader.u16()?;
    let volume = reader.u16()?;
    reader.skip(2)?;
    let mut matrix = [0_i32; 9];
    for value in &mut matrix {
        *value = reader.i32()?;
    }
    let width = (reader.u32()? >> 16) as u16;
    let height = (reader.u32()? >> 16) as u16;
    Ok(TrackHeader {
        id,
        creation_time,
        modification_time,
        duration,
        flags,
        layer,
        alternate_group,
        volume,
        matrix,
        width,
        height,
    })
}

fn parse_edts(data: &[u8]) -> Result<Vec<ParsedEdit>> {
    for atom in atoms(data)? {
        if atom.kind == *b"elst" {
            let mut reader = Reader::new(atom.data);
            let (version, _) = full_box(&mut reader)?;
            let count = reader.u32()?;
            let entry_size = match version {
                0 => 12,
                1 => 20,
                _ => return Err(ParseError::new("elst version is unsupported")),
            };
            let count = bounded_table_count(&reader, count, entry_size)?;
            let mut edits = Vec::with_capacity(count);
            for _ in 0..count {
                let (segment_duration, media_time) = match version {
                    0 => (u64::from(reader.u32()?), i64::from(reader.i32()?)),
                    1 => (reader.u64()?, reader.i64()?),
                    _ => unreachable!("edit-list version was validated"),
                };
                edits.push(ParsedEdit {
                    segment_duration,
                    media_time,
                    rate_integer: reader.i16()?,
                    rate_fraction: reader.i16()?,
                });
            }
            return Ok(edits);
        }
    }
    Ok(Vec::new())
}

fn parse_mdia(data: &[u8]) -> Result<(MediaHeader, Handler, SampleTable)> {
    let mut header = None;
    let mut handler = None;
    let mut sample_table = None;
    for atom in atoms(data)? {
        match atom.kind {
            [b'm', b'd', b'h', b'd'] => header = Some(parse_mdhd(atom.data)?),
            [b'h', b'd', b'l', b'r'] => handler = Some(parse_hdlr(atom.data)?),
            [b'm', b'i', b'n', b'f'] => sample_table = Some(parse_minf(atom.data)?),
            _ => {}
        }
    }
    Ok((
        header.ok_or_else(|| ParseError::new("mdhd atom is missing"))?,
        handler.ok_or_else(|| ParseError::new("hdlr atom is missing"))?,
        sample_table.ok_or_else(|| ParseError::new("stbl atom is missing"))?,
    ))
}

fn parse_mdhd(data: &[u8]) -> Result<MediaHeader> {
    let mut reader = Reader::new(data);
    let (version, _) = full_box(&mut reader)?;
    let (creation_time, modification_time, timescale, duration) = match version {
        0 => (
            u64::from(reader.u32()?),
            u64::from(reader.u32()?),
            reader.u32()?,
            u64::from(reader.u32()?),
        ),
        1 => (reader.u64()?, reader.u64()?, reader.u32()?, reader.u64()?),
        _ => return Err(ParseError::new("mdhd version is unsupported")),
    };
    let language = decode_language(reader.u16()?);
    Ok(MediaHeader {
        creation_time,
        modification_time,
        timescale,
        duration,
        language,
    })
}

fn decode_language(value: u16) -> String {
    let bytes = [
        (((value >> 10) & 0x1f) as u8).saturating_add(0x60),
        (((value >> 5) & 0x1f) as u8).saturating_add(0x60),
        ((value & 0x1f) as u8).saturating_add(0x60),
    ];
    String::from_utf8_lossy(&bytes).into_owned()
}

fn parse_hdlr(data: &[u8]) -> Result<Handler> {
    let mut reader = Reader::new(data);
    full_box(&mut reader)?;
    reader.skip(4)?;
    let kind = reader
        .take(4)?
        .try_into()
        .map_err(|_| ParseError::new("handler type is truncated"))?;
    reader.skip(12)?;
    let name_data = reader.remaining();
    let end = name_data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name_data.len());
    Ok(Handler {
        kind,
        name: String::from_utf8_lossy(&name_data[..end]).into_owned(),
    })
}

fn parse_minf(data: &[u8]) -> Result<SampleTable> {
    for atom in atoms(data)? {
        if atom.kind == *b"stbl" {
            return parse_stbl(atom.data);
        }
    }
    Err(ParseError::new("stbl atom is missing"))
}

fn parse_stbl(data: &[u8]) -> Result<SampleTable> {
    let mut table = SampleTable::default();
    for atom in atoms(data)? {
        match atom.kind {
            [b's', b't', b's', b'd'] => parse_stsd(atom.data, &mut table)?,
            [b's', b't', b't', b's'] => table.timing = parse_stts(atom.data)?,
            [b'c', b't', b't', b's'] => table.composition = parse_ctts(atom.data)?,
            [b's', b't', b's', b'c'] => table.chunks = parse_stsc(atom.data)?,
            [b's', b't', b's', b'z'] => table.sizes = parse_stsz(atom.data)?,
            [b's', b't', b'c', b'o'] => table.chunk_offsets = parse_stco(atom.data)?,
            [b'c', b'o', b'6', b'4'] => table.chunk_offsets = parse_co64(atom.data)?,
            [b's', b't', b's', b's'] => table.sync_samples = Some(parse_stss(atom.data)?),
            _ => {}
        }
    }
    Ok(table)
}

fn parse_stsd(data: &[u8], table: &mut SampleTable) -> Result<()> {
    let mut reader = Reader::new(data);
    full_box(&mut reader)?;
    let count = reader.u32()?;
    if count == 0 {
        return Err(ParseError::new("stsd contains no sample entry"));
    }
    let entries = atoms(reader.remaining())?;
    let entry = entries
        .first()
        .ok_or_else(|| ParseError::new("stsd sample entry is missing"))?;
    table.codec = entry.kind;
    table.codec_string = Some(String::from_utf8_lossy(&entry.kind).into_owned());
    let child_offset = match &entry.kind {
        b"av01" | b"avc1" | b"hvc1" | b"hev1" | b"vp08" | b"vp09" => 78,
        b"mp4a" => 28,
        _ => entry.data.len(),
    };
    if let Some(children) = entry
        .data
        .get(child_offset..)
        .and_then(|data| atoms(data).ok())
    {
        for child in children {
            if matches!(&child.kind, b"av1C" | b"avcC" | b"hvcC" | b"vpcC" | b"esds") {
                table.codec_configuration = Some(child.data.to_vec());
                break;
            }
        }
    }
    Ok(())
}

fn parse_stts(data: &[u8]) -> Result<Vec<(u32, u32)>> {
    let mut reader = Reader::new(data);
    full_box(&mut reader)?;
    let count = reader.u32()?;
    let count = bounded_table_count(&reader, count, 8)?;
    (0..count)
        .map(|_| Ok((reader.u32()?, reader.u32()?)))
        .collect()
}

fn parse_ctts(data: &[u8]) -> Result<Vec<(u32, i64)>> {
    let mut reader = Reader::new(data);
    let (version, _) = full_box(&mut reader)?;
    let count = reader.u32()?;
    if !matches!(version, 0 | 1) {
        return Err(ParseError::new("ctts version is unsupported"));
    }
    let count = bounded_table_count(&reader, count, 8)?;
    (0..count)
        .map(|_| {
            let sample_count = reader.u32()?;
            let offset = match version {
                0 => i64::from(reader.u32()?),
                1 => i64::from(reader.i32()?),
                _ => unreachable!("composition-offset version was validated"),
            };
            Ok((sample_count, offset))
        })
        .collect()
}

fn parse_stsc(data: &[u8]) -> Result<Vec<SampleToChunk>> {
    let mut reader = Reader::new(data);
    full_box(&mut reader)?;
    let count = reader.u32()?;
    let count = bounded_table_count(&reader, count, 12)?;
    (0..count)
        .map(|_| {
            let value = SampleToChunk {
                first_chunk: reader.u32()?,
                samples_per_chunk: reader.u32()?,
            };
            reader.skip(4)?;
            Ok(value)
        })
        .collect()
}

fn parse_stsz(data: &[u8]) -> Result<Vec<u64>> {
    let mut reader = Reader::new(data);
    full_box(&mut reader)?;
    let uniform = reader.u32()?;
    let count = reader.u32()?;
    let count = bounded_table_count(&reader, count, usize::from(uniform == 0) * 4)?;
    if uniform != 0 {
        return Ok(vec![u64::from(uniform); count]);
    }
    (0..count).map(|_| reader.u32().map(u64::from)).collect()
}

fn parse_stco(data: &[u8]) -> Result<Vec<u64>> {
    let mut reader = Reader::new(data);
    full_box(&mut reader)?;
    let count = reader.u32()?;
    let count = bounded_table_count(&reader, count, 4)?;
    (0..count).map(|_| reader.u32().map(u64::from)).collect()
}

fn parse_co64(data: &[u8]) -> Result<Vec<u64>> {
    let mut reader = Reader::new(data);
    full_box(&mut reader)?;
    let count = reader.u32()?;
    let count = bounded_table_count(&reader, count, 8)?;
    (0..count).map(|_| reader.u64()).collect()
}

fn parse_stss(data: &[u8]) -> Result<BTreeSet<u32>> {
    let mut reader = Reader::new(data);
    full_box(&mut reader)?;
    let count = reader.u32()?;
    let count = bounded_table_count(&reader, count, 4)?;
    (0..count).map(|_| reader.u32()).collect()
}

fn expand_runs<T: Copy>(runs: &[(u32, T)], count: usize) -> Result<Vec<T>> {
    let mut values = Vec::with_capacity(count);
    for (run, value) in runs {
        let run = *run as usize;
        if values.len().saturating_add(run) > count {
            return Err(ParseError::new("sample-table run exceeds sample count"));
        }
        values.extend(std::iter::repeat(*value).take(run));
    }
    if values.len() != count {
        return Err(ParseError::new(
            "sample-table runs do not cover every sample",
        ));
    }
    Ok(values)
}

fn build_samples(table: &SampleTable) -> Result<Vec<ParsedSample>> {
    let count = table.sizes.len();
    if count == 0 {
        return Ok(Vec::new());
    }
    if table.timing.is_empty() || table.chunks.is_empty() || table.chunk_offsets.is_empty() {
        return Err(ParseError::new("required sample table is missing"));
    }
    let durations = expand_runs(&table.timing, count)?;
    let composition = if table.composition.is_empty() {
        vec![0_i64; count]
    } else {
        expand_runs(&table.composition, count)?
    };
    let mut offsets = Vec::with_capacity(count);
    let mut sample_index = 0_usize;
    for (chunk_zero, chunk_offset) in table.chunk_offsets.iter().copied().enumerate() {
        let chunk = u32::try_from(chunk_zero + 1)
            .map_err(|_| ParseError::new("chunk index exceeds u32"))?;
        let entry = table
            .chunks
            .iter()
            .rev()
            .find(|entry| entry.first_chunk <= chunk)
            .ok_or_else(|| ParseError::new("stsc does not describe a chunk"))?;
        let mut offset_in_chunk = 0_u64;
        for _ in 0..entry.samples_per_chunk {
            if sample_index >= count {
                break;
            }
            offsets.push(
                chunk_offset
                    .checked_add(offset_in_chunk)
                    .ok_or_else(|| ParseError::new("sample offset overflows"))?,
            );
            offset_in_chunk = offset_in_chunk
                .checked_add(table.sizes[sample_index])
                .ok_or_else(|| ParseError::new("chunk sample sizes overflow"))?;
            sample_index += 1;
        }
    }
    if offsets.len() != count {
        return Err(ParseError::new("chunk tables do not locate every sample"));
    }

    let mut decode = 0_i64;
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let duration = u64::from(durations[index]);
        let composition_timestamp = decode
            .checked_add(composition[index])
            .ok_or_else(|| ParseError::new("composition timestamp overflows"))?;
        let id =
            u32::try_from(index + 1).map_err(|_| ParseError::new("sample index exceeds u32"))?;
        samples.push(ParsedSample {
            id,
            is_sync: table
                .sync_samples
                .as_ref()
                .map_or(true, |sync| sync.contains(&id)),
            size: table.sizes[index],
            offset: offsets[index],
            decode_timestamp: decode,
            composition_timestamp,
            duration,
        });
        decode = decode
            .checked_add(i64::from(durations[index]))
            .ok_or_else(|| ParseError::new("decode timestamp overflows"))?;
    }
    Ok(samples)
}

fn parse_mvex(data: &[u8]) -> Result<BTreeMap<u32, TrackDefaults>> {
    let mut defaults = BTreeMap::new();
    for atom in atoms(data)? {
        if atom.kind == *b"trex" {
            let mut reader = Reader::new(atom.data);
            full_box(&mut reader)?;
            let track_id = reader.u32()?;
            reader.skip(4)?;
            defaults.insert(
                track_id,
                TrackDefaults {
                    duration: reader.u32()?,
                    size: reader.u32()?,
                    flags: reader.u32()?,
                },
            );
        }
    }
    Ok(defaults)
}

#[derive(Clone, Copy, Default)]
struct FragmentHeader {
    track_id: u32,
    base_data_offset: Option<u64>,
    default_duration: Option<u32>,
    default_size: Option<u32>,
    default_flags: Option<u32>,
}

struct FragmentRun {
    data_offset: Option<i32>,
    first_sample_flags: Option<u32>,
    samples: Vec<FragmentSample>,
}

#[derive(Clone, Copy, Default)]
struct FragmentSample {
    duration: Option<u32>,
    size: Option<u32>,
    flags: Option<u32>,
    composition_offset: i64,
}

fn parse_moof(
    source: &[u8],
    moof: Atom<'_>,
    defaults: &BTreeMap<u32, TrackDefaults>,
    tracks: &mut [ParsedTrack],
) -> Result<()> {
    for traf in atoms(moof.data)? {
        if traf.kind != *b"traf" {
            continue;
        }
        let mut header = None;
        let mut decode_time = None;
        let mut runs = Vec::new();
        for child in atoms(traf.data)? {
            match child.kind {
                [b't', b'f', b'h', b'd'] => header = Some(parse_tfhd(child.data)?),
                [b't', b'f', b'd', b't'] => decode_time = Some(parse_tfdt(child.data)?),
                [b't', b'r', b'u', b'n'] => runs.push(parse_trun(child.data)?),
                _ => {}
            }
        }
        let header = header.ok_or_else(|| ParseError::new("fragment tfhd atom is missing"))?;
        let track = tracks
            .iter_mut()
            .find(|track| track.id == header.track_id)
            .ok_or_else(|| ParseError::new("fragment references an unknown track"))?;
        let defaults = defaults.get(&header.track_id).copied().unwrap_or_default();
        let base = header.base_data_offset.unwrap_or(
            u64::try_from(moof.start).map_err(|_| ParseError::new("moof offset exceeds u64"))?,
        );
        let mut next_offset = base;
        let mut decode = decode_time.unwrap_or_else(|| {
            track
                .samples
                .last()
                .and_then(|sample| sample.decode_timestamp.checked_add(sample.duration as i64))
                .unwrap_or(0)
        });
        for run in runs {
            if let Some(relative) = run.data_offset {
                next_offset = base
                    .checked_add_signed(i64::from(relative))
                    .ok_or_else(|| ParseError::new("fragment data offset overflows"))?;
            }
            for (index, sample) in run.samples.into_iter().enumerate() {
                let duration = sample
                    .duration
                    .or(header.default_duration)
                    .unwrap_or(defaults.duration);
                let size = sample.size.or(header.default_size).unwrap_or(defaults.size);
                let flags = sample
                    .flags
                    .or_else(|| (index == 0).then_some(run.first_sample_flags).flatten())
                    .or(header.default_flags)
                    .unwrap_or(defaults.flags);
                let end = next_offset
                    .checked_add(u64::from(size))
                    .ok_or_else(|| ParseError::new("fragment sample range overflows"))?;
                if end > source.len() as u64 {
                    return Err(ParseError::new("fragment sample lies outside the source"));
                }
                let id = u32::try_from(track.samples.len() + 1)
                    .map_err(|_| ParseError::new("fragment sample index exceeds u32"))?;
                track.samples.push(ParsedSample {
                    id,
                    is_sync: flags & 0x0001_0000 == 0,
                    size: u64::from(size),
                    offset: next_offset,
                    decode_timestamp: decode,
                    composition_timestamp: decode
                        .checked_add(sample.composition_offset)
                        .ok_or_else(|| {
                            ParseError::new("fragment composition timestamp overflows")
                        })?,
                    duration: u64::from(duration),
                });
                next_offset = end;
                decode = decode
                    .checked_add(i64::from(duration))
                    .ok_or_else(|| ParseError::new("fragment decode timestamp overflows"))?;
            }
        }
    }
    Ok(())
}

fn parse_tfhd(data: &[u8]) -> Result<FragmentHeader> {
    let mut reader = Reader::new(data);
    let (_, flags) = full_box(&mut reader)?;
    let track_id = reader.u32()?;
    let base_data_offset = (flags & 0x000001 != 0).then(|| reader.u64()).transpose()?;
    if flags & 0x000002 != 0 {
        reader.skip(4)?;
    }
    let default_duration = (flags & 0x000008 != 0).then(|| reader.u32()).transpose()?;
    let default_size = (flags & 0x000010 != 0).then(|| reader.u32()).transpose()?;
    let default_flags = (flags & 0x000020 != 0).then(|| reader.u32()).transpose()?;
    Ok(FragmentHeader {
        track_id,
        base_data_offset,
        default_duration,
        default_size,
        default_flags,
    })
}

fn parse_tfdt(data: &[u8]) -> Result<i64> {
    let mut reader = Reader::new(data);
    let (version, _) = full_box(&mut reader)?;
    let value = match version {
        0 => u64::from(reader.u32()?),
        1 => reader.u64()?,
        _ => return Err(ParseError::new("tfdt version is unsupported")),
    };
    i64::try_from(value).map_err(|_| ParseError::new("fragment decode time exceeds i64"))
}

fn parse_trun(data: &[u8]) -> Result<FragmentRun> {
    let mut reader = Reader::new(data);
    let (version, flags) = full_box(&mut reader)?;
    let count = reader.u32()?;
    let data_offset = (flags & 0x000001 != 0).then(|| reader.i32()).transpose()?;
    let first_sample_flags = (flags & 0x000004 != 0).then(|| reader.u32()).transpose()?;
    if !matches!(version, 0 | 1) {
        return Err(ParseError::new("trun version is unsupported"));
    }
    let fields = usize::from(flags & 0x000100 != 0)
        + usize::from(flags & 0x000200 != 0)
        + usize::from(flags & 0x000400 != 0)
        + usize::from(flags & 0x000800 != 0);
    let count = bounded_table_count(&reader, count, fields * 4)?;
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        samples.push(FragmentSample {
            duration: (flags & 0x000100 != 0).then(|| reader.u32()).transpose()?,
            size: (flags & 0x000200 != 0).then(|| reader.u32()).transpose()?,
            flags: (flags & 0x000400 != 0).then(|| reader.u32()).transpose()?,
            composition_offset: if flags & 0x000800 != 0 {
                match version {
                    0 => i64::from(reader.u32()?),
                    1 => i64::from(reader.i32()?),
                    _ => unreachable!("fragment-run version was validated"),
                }
            } else {
                0
            },
        });
    }
    Ok(FragmentRun {
        data_offset,
        first_sample_flags,
        samples,
    })
}

fn parse_udta(data: &[u8]) -> Result<ParsedMetadata> {
    let mut metadata = ParsedMetadata::default();
    for atom in atoms(data)? {
        if atom.kind == *b"meta" {
            metadata.merge(parse_meta(atom.data)?);
        }
    }
    Ok(metadata)
}

fn parse_meta(data: &[u8]) -> Result<ParsedMetadata> {
    let content = if data.get(..4) == Some(&[0, 0, 0, 0]) {
        &data[4..]
    } else {
        data
    };
    for atom in atoms(content)? {
        if atom.kind == *b"ilst" {
            return parse_ilst(atom.data);
        }
    }
    Ok(ParsedMetadata::default())
}

fn parse_ilst(data: &[u8]) -> Result<ParsedMetadata> {
    let mut metadata = ParsedMetadata::default();
    for item in atoms(data)? {
        let Some(data_atom) = atoms(item.data)?
            .into_iter()
            .find(|atom| atom.kind == *b"data")
        else {
            continue;
        };
        let mut reader = Reader::new(data_atom.data);
        let data_type = reader.u32()?;
        reader.skip(4)?;
        let value = reader.remaining();
        match item.kind {
            [0xa9, b'n', b'a', b'm'] => {
                metadata.title = Some(String::from_utf8_lossy(value).into_owned());
            }
            [0xa9, b'd', b'a', b'y'] => {
                metadata.year = if data_type == 0 && value.len() == 4 {
                    Some(u32::from_be_bytes(value.try_into().map_err(|_| {
                        ParseError::new("metadata year is truncated")
                    })?))
                } else {
                    String::from_utf8_lossy(value).parse().ok()
                };
            }
            [b'd', b'e', b's', b'c'] => {
                metadata.summary = Some(String::from_utf8_lossy(value).into_owned());
            }
            [b'c', b'o', b'v', b'r'] => metadata.poster = Some(value.to_vec()),
            _ => {}
        }
    }
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::{parse_stsz, parse_stts, parse_trun, MAX_TABLE_ENTRIES};

    #[test]
    fn hostile_table_counts_are_rejected_before_allocation() {
        let oversized = u32::try_from(MAX_TABLE_ENTRIES + 1).unwrap();

        let mut stsz = vec![0_u8; 4];
        stsz.extend_from_slice(&1_u32.to_be_bytes());
        stsz.extend_from_slice(&oversized.to_be_bytes());
        assert!(parse_stsz(&stsz).is_err());

        let mut stts = vec![0_u8; 4];
        stts.extend_from_slice(&2_u32.to_be_bytes());
        assert!(parse_stts(&stts).is_err());

        let mut trun = vec![0_u8; 4];
        trun.extend_from_slice(&oversized.to_be_bytes());
        assert!(parse_trun(&trun).is_err());
    }
}
