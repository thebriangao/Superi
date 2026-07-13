use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub(crate) type Ul = [u8; 16];

const PARTITION_PREFIX: [u8; 13] = [
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01,
];
const PRIMER_SUFFIX: [u8; 8] = [0x0d, 0x01, 0x02, 0x01, 0x01, 0x05, 0x01, 0x00];
const INDEX_SUFFIX: [u8; 8] = [0x0d, 0x01, 0x02, 0x01, 0x01, 0x10, 0x01, 0x00];
const RIP_SUFFIX: [u8; 8] = [0x0d, 0x01, 0x02, 0x01, 0x01, 0x11, 0x01, 0x00];
const GENERIC_CONTAINER_PREFIX: [u8; 12] = [
    0x06, 0x0e, 0x2b, 0x34, 0x01, 0x02, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01,
];
const MAX_RUN_IN: usize = 65_535;
const MAX_KLVS: usize = 10_000_000;
const MAX_LOCAL_ITEMS: usize = 1_000_000;
const MAX_BATCH_ITEMS: usize = 10_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParseError {
    message: &'static str,
    offset: usize,
}

impl ParseError {
    fn new(message: &'static str, offset: usize) -> Self {
        Self { message, offset }
    }

    pub(crate) const fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

type Result<T> = std::result::Result<T, ParseError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PartitionKind {
    Header,
    Body,
    Footer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PartitionPack {
    pub(crate) kind: PartitionKind,
    pub(crate) closed: bool,
    pub(crate) complete: bool,
    pub(crate) byte_offset: u64,
    pub(crate) content_offset: u64,
    pub(crate) major_version: u16,
    pub(crate) minor_version: u16,
    pub(crate) kag_size: u32,
    pub(crate) this_partition: u64,
    pub(crate) previous_partition: u64,
    pub(crate) footer_partition: u64,
    pub(crate) header_byte_count: u64,
    pub(crate) index_byte_count: u64,
    pub(crate) index_sid: u32,
    pub(crate) body_offset: u64,
    pub(crate) body_sid: u32,
    pub(crate) operational_pattern: Ul,
    pub(crate) essence_containers: Vec<Ul>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MetadataItem {
    pub(crate) local_tag: u16,
    pub(crate) property_ul: Option<Ul>,
    pub(crate) value_offset: u64,
    pub(crate) value_length: usize,
}

impl MetadataItem {
    pub(crate) fn value<'a>(&self, data: &'a [u8]) -> Result<&'a [u8]> {
        let start = usize::try_from(self.value_offset)
            .map_err(|_| ParseError::new("metadata offset cannot be represented", usize::MAX))?;
        let end = start
            .checked_add(self.value_length)
            .ok_or_else(|| ParseError::new("metadata byte range overflowed", start))?;
        data.get(start..end)
            .ok_or_else(|| ParseError::new("metadata byte range exceeds the source", start))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MetadataSet {
    pub(crate) key: Ul,
    pub(crate) set_kind: u16,
    pub(crate) byte_offset: u64,
    pub(crate) items: Vec<MetadataItem>,
}

impl MetadataSet {
    pub(crate) fn item(&self, canonical_tag: u16) -> Option<&MetadataItem> {
        self.items.iter().find(|item| {
            item.local_tag == canonical_tag
                || item
                    .property_ul
                    .and_then(canonical_tag_for_ul)
                    .is_some_and(|tag| tag == canonical_tag)
        })
    }

    pub(crate) fn instance_uid(&self, data: &[u8]) -> Option<[u8; 16]> {
        let value = self.item(0x3c0a)?.value(data).ok()?;
        value.try_into().ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Rational {
    pub(crate) numerator: i32,
    pub(crate) denominator: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndexEntry {
    pub(crate) temporal_offset: i8,
    pub(crate) key_frame_offset: i8,
    pub(crate) flags: u8,
    pub(crate) stream_offset: u64,
    pub(crate) slice_offsets: Vec<u32>,
    pub(crate) position_table: Vec<Rational>,
}

impl IndexEntry {
    pub(crate) const fn is_random_access(&self) -> bool {
        self.flags & 0x80 != 0 || self.key_frame_offset == 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IndexSegment {
    pub(crate) byte_offset: u64,
    pub(crate) edit_rate: Rational,
    pub(crate) start_position: i64,
    pub(crate) duration: i64,
    pub(crate) edit_unit_byte_count: u32,
    pub(crate) index_sid: u32,
    pub(crate) body_sid: u32,
    pub(crate) slice_count: u8,
    pub(crate) position_table_count: u8,
    pub(crate) entries: Vec<IndexEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EssenceElement {
    pub(crate) key: Ul,
    pub(crate) track_number: u32,
    pub(crate) body_sid: u32,
    pub(crate) klv_offset: u64,
    pub(crate) value_offset: u64,
    pub(crate) value_length: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RandomIndexEntry {
    pub(crate) body_sid: u32,
    pub(crate) byte_offset: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedMxf {
    pub(crate) run_in: u64,
    pub(crate) partitions: Vec<PartitionPack>,
    pub(crate) metadata_sets: Vec<MetadataSet>,
    pub(crate) index_segments: Vec<IndexSegment>,
    pub(crate) essence_elements: Vec<EssenceElement>,
    pub(crate) random_index: Vec<RandomIndexEntry>,
    pub(crate) primer_mapping_count: usize,
    pub(crate) dark_klv_count: u64,
}

pub(crate) fn parse(data: &[u8]) -> Result<ParsedMxf> {
    let run_in = find_header_partition(data)?;
    let mut offset = run_in;
    let mut partitions = Vec::new();
    let mut metadata_sets = Vec::new();
    let mut index_segments = Vec::new();
    let mut essence_elements = Vec::new();
    let mut random_index = Vec::new();
    let mut primer = BTreeMap::new();
    let mut primer_tags = BTreeSet::new();
    let mut current_body_sid = 0;
    let mut dark_klv_count = 0_u64;
    let mut klv_count = 0_usize;

    while offset < data.len() {
        klv_count = klv_count
            .checked_add(1)
            .ok_or_else(|| ParseError::new("KLV count overflowed", offset))?;
        if klv_count > MAX_KLVS {
            return Err(ParseError::new("MXF contains too many KLV packets", offset));
        }
        let klv = parse_klv(data, offset)?;
        let value = data
            .get(klv.value_offset..klv.end)
            .ok_or_else(|| ParseError::new("KLV value exceeds the source", offset))?;

        if let Some(kind) = partition_kind(&klv.key) {
            let partition = parse_partition(&klv.key, kind, value, offset, klv.end)?;
            current_body_sid = partition.body_sid;
            primer.clear();
            partitions.push(partition);
        } else if is_primer(&klv.key) {
            primer = parse_primer(value, klv.value_offset)?;
            primer_tags.extend(primer.keys().copied());
        } else if is_index_segment(&klv.key) {
            let set = parse_local_set(&klv.key, value, klv.value_offset, offset, &primer)?;
            index_segments.push(parse_index_segment(data, &set)?);
        } else if is_structural_set(&klv.key) {
            metadata_sets.push(parse_local_set(
                &klv.key,
                value,
                klv.value_offset,
                offset,
                &primer,
            )?);
        } else if is_generic_container_element(&klv.key) {
            let value_length = u64::try_from(value.len())
                .map_err(|_| ParseError::new("essence length cannot be represented", offset))?;
            essence_elements.push(EssenceElement {
                key: klv.key,
                track_number: u32::from_be_bytes(klv.key[12..16].try_into().expect("key slice")),
                body_sid: current_body_sid,
                klv_offset: offset as u64,
                value_offset: klv.value_offset as u64,
                value_length,
            });
        } else if is_random_index_pack(&klv.key) {
            random_index = parse_random_index(value, klv.value_offset)?;
        } else if !is_fill(&klv.key) {
            dark_klv_count = dark_klv_count
                .checked_add(1)
                .ok_or_else(|| ParseError::new("dark KLV count overflowed", offset))?;
        }
        offset = klv.end;
    }

    validate_structure(
        data.len(),
        run_in,
        &partitions,
        &metadata_sets,
        &index_segments,
        &essence_elements,
        &random_index,
    )?;
    Ok(ParsedMxf {
        run_in: run_in as u64,
        partitions,
        metadata_sets,
        index_segments,
        essence_elements,
        random_index,
        primer_mapping_count: primer_tags.len(),
        dark_klv_count,
    })
}

struct Klv {
    key: Ul,
    value_offset: usize,
    end: usize,
}

fn parse_klv(data: &[u8], offset: usize) -> Result<Klv> {
    let key_end = offset
        .checked_add(16)
        .ok_or_else(|| ParseError::new("KLV key offset overflowed", offset))?;
    let key: Ul = data
        .get(offset..key_end)
        .ok_or_else(|| ParseError::new("KLV key is truncated", offset))?
        .try_into()
        .expect("validated key length");
    let mut reader = Reader::new(&data[key_end..], key_end);
    let length = reader.read_ber_length()?;
    let value_offset = reader.position();
    let end = value_offset
        .checked_add(length)
        .ok_or_else(|| ParseError::new("KLV byte range overflowed", offset))?;
    if end > data.len() {
        return Err(ParseError::new("KLV value is truncated", value_offset));
    }
    Ok(Klv {
        key,
        value_offset,
        end,
    })
}

fn find_header_partition(data: &[u8]) -> Result<usize> {
    if data.len() < 16 {
        return Err(ParseError::new("MXF header partition was not found", 0));
    }
    let last = MAX_RUN_IN.min(data.len().saturating_sub(16));
    for offset in 0..=last {
        let key = &data[offset..offset + 16];
        if key.starts_with(&PARTITION_PREFIX) && key[13] == 0x02 && key[15] == 0x00 {
            return Ok(offset);
        }
    }
    Err(ParseError::new("MXF header partition was not found", 0))
}

fn partition_kind(key: &Ul) -> Option<PartitionKind> {
    if key[..13] != PARTITION_PREFIX || key[15] != 0x00 {
        return None;
    }
    match key[13] {
        0x02 => Some(PartitionKind::Header),
        0x03 => Some(PartitionKind::Body),
        0x04 => Some(PartitionKind::Footer),
        _ => None,
    }
}

fn parse_partition(
    key: &Ul,
    kind: PartitionKind,
    value: &[u8],
    byte_offset: usize,
    content_offset: usize,
) -> Result<PartitionPack> {
    let mut reader = Reader::new(value, byte_offset);
    let major_version = reader.read_u16()?;
    let minor_version = reader.read_u16()?;
    let kag_size = reader.read_u32()?;
    let this_partition = reader.read_u64()?;
    let previous_partition = reader.read_u64()?;
    let footer_partition = reader.read_u64()?;
    let header_byte_count = reader.read_u64()?;
    let index_byte_count = reader.read_u64()?;
    let index_sid = reader.read_u32()?;
    let body_offset = reader.read_u64()?;
    let body_sid = reader.read_u32()?;
    let operational_pattern = reader.read_array()?;
    let essence_containers = reader.read_batch(16, |reader| reader.read_array())?;
    if reader.remaining() != 0 {
        return Err(ParseError::new(
            "partition pack has trailing bytes",
            reader.position(),
        ));
    }
    if major_version == 0 || kag_size == 0 {
        return Err(ParseError::new(
            "partition version or KAG size is invalid",
            byte_offset,
        ));
    }
    Ok(PartitionPack {
        kind,
        closed: matches!(key[14], 0x02 | 0x04),
        complete: matches!(key[14], 0x03 | 0x04),
        byte_offset: byte_offset as u64,
        content_offset: content_offset as u64,
        major_version,
        minor_version,
        kag_size,
        this_partition,
        previous_partition,
        footer_partition,
        header_byte_count,
        index_byte_count,
        index_sid,
        body_offset,
        body_sid,
        operational_pattern,
        essence_containers,
    })
}

fn parse_primer(value: &[u8], value_offset: usize) -> Result<BTreeMap<u16, Ul>> {
    let mut reader = Reader::new(value, value_offset);
    let count = reader.read_count()?;
    let item_length = reader.read_u32()? as usize;
    if item_length != 18 {
        return Err(ParseError::new(
            "primer entries must be 18 bytes",
            reader.position(),
        ));
    }
    let required = count
        .checked_mul(item_length)
        .ok_or_else(|| ParseError::new("primer size overflowed", reader.position()))?;
    if required != reader.remaining() {
        return Err(ParseError::new(
            "primer batch size is inconsistent",
            reader.position(),
        ));
    }
    let mut mappings = BTreeMap::new();
    for _ in 0..count {
        let tag = reader.read_u16()?;
        let ul = reader.read_array()?;
        if tag == 0 || mappings.insert(tag, ul).is_some() {
            return Err(ParseError::new(
                "primer contains an invalid or duplicate local tag",
                reader.position(),
            ));
        }
    }
    Ok(mappings)
}

fn parse_local_set(
    key: &Ul,
    value: &[u8],
    value_offset: usize,
    klv_offset: usize,
    primer: &BTreeMap<u16, Ul>,
) -> Result<MetadataSet> {
    let ber_items = key[5] == 0x13;
    let mut reader = Reader::new(value, value_offset);
    let mut items = Vec::new();
    let mut tags = BTreeSet::new();
    while reader.remaining() != 0 {
        if items.len() >= MAX_LOCAL_ITEMS {
            return Err(ParseError::new(
                "local set contains too many properties",
                reader.position(),
            ));
        }
        let tag = reader.read_u16()?;
        if !tags.insert(tag) {
            return Err(ParseError::new(
                "local set contains a duplicate property",
                reader.position(),
            ));
        }
        let length = if ber_items {
            reader.read_ber_length()?
        } else {
            reader.read_u16()? as usize
        };
        let item_offset = reader.position();
        reader.skip(length)?;
        items.push(MetadataItem {
            local_tag: tag,
            property_ul: primer.get(&tag).copied(),
            value_offset: item_offset as u64,
            value_length: length,
        });
    }
    Ok(MetadataSet {
        key: *key,
        set_kind: u16::from_be_bytes([key[13], key[14]]),
        byte_offset: klv_offset as u64,
        items,
    })
}

fn parse_index_segment(data: &[u8], set: &MetadataSet) -> Result<IndexSegment> {
    let edit_rate = parse_rational(required_item(data, set, 0x3f0b)?, set.byte_offset as usize)?;
    let start_position = read_i64(required_item(data, set, 0x3f0c)?, set.byte_offset as usize)?;
    let duration = read_i64(required_item(data, set, 0x3f0d)?, set.byte_offset as usize)?;
    let edit_unit_byte_count =
        read_u32(required_item(data, set, 0x3f05)?, set.byte_offset as usize)?;
    let index_sid = read_u32(required_item(data, set, 0x3f06)?, set.byte_offset as usize)?;
    let body_sid = read_u32(required_item(data, set, 0x3f07)?, set.byte_offset as usize)?;
    let slice_count = set
        .item(0x3f08)
        .map(|item| item.value(data))
        .transpose()?
        .map_or(Ok(0), |value| read_u8(value, set.byte_offset as usize))?;
    let position_table_count = set
        .item(0x3f0e)
        .map(|item| item.value(data))
        .transpose()?
        .map_or(Ok(0), |value| read_u8(value, set.byte_offset as usize))?;
    let entries = match set.item(0x3f0a) {
        Some(item) => parse_index_entries(
            item.value(data)?,
            item.value_offset as usize,
            slice_count,
            position_table_count,
        )?,
        None => Vec::new(),
    };
    if edit_rate.numerator <= 0 || edit_rate.denominator <= 0 || duration < 0 {
        return Err(ParseError::new(
            "index edit rate or duration is invalid",
            set.byte_offset as usize,
        ));
    }
    if duration != 0 && !entries.is_empty() && entries.len() as i64 > duration {
        return Err(ParseError::new(
            "index entries exceed the declared duration",
            set.byte_offset as usize,
        ));
    }
    Ok(IndexSegment {
        byte_offset: set.byte_offset,
        edit_rate,
        start_position,
        duration,
        edit_unit_byte_count,
        index_sid,
        body_sid,
        slice_count,
        position_table_count,
        entries,
    })
}

fn parse_index_entries(
    value: &[u8],
    value_offset: usize,
    slice_count: u8,
    position_table_count: u8,
) -> Result<Vec<IndexEntry>> {
    let mut reader = Reader::new(value, value_offset);
    let count = reader.read_count()?;
    let item_length = reader.read_u32()? as usize;
    let minimum = 11_usize
        .checked_add(usize::from(slice_count).saturating_mul(4))
        .and_then(|value| value.checked_add(usize::from(position_table_count).saturating_mul(8)))
        .ok_or_else(|| ParseError::new("index entry size overflowed", reader.position()))?;
    if item_length < minimum {
        return Err(ParseError::new(
            "index entry is shorter than its declared layout",
            reader.position(),
        ));
    }
    let required = count
        .checked_mul(item_length)
        .ok_or_else(|| ParseError::new("index entry array size overflowed", reader.position()))?;
    if required != reader.remaining() {
        return Err(ParseError::new(
            "index entry batch size is inconsistent",
            reader.position(),
        ));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let entry_start = reader.position();
        let temporal_offset = reader.read_i8()?;
        let key_frame_offset = reader.read_i8()?;
        let flags = reader.read_u8()?;
        let stream_offset = reader.read_u64()?;
        let mut slice_offsets = Vec::with_capacity(slice_count as usize);
        for _ in 0..slice_count {
            slice_offsets.push(reader.read_u32()?);
        }
        let mut position_table = Vec::with_capacity(position_table_count as usize);
        for _ in 0..position_table_count {
            position_table.push(Rational {
                numerator: reader.read_i32()?,
                denominator: reader.read_i32()?,
            });
        }
        let consumed = reader.position() - entry_start;
        reader.skip(item_length - consumed)?;
        entries.push(IndexEntry {
            temporal_offset,
            key_frame_offset,
            flags,
            stream_offset,
            slice_offsets,
            position_table,
        });
    }
    Ok(entries)
}

fn parse_random_index(value: &[u8], value_offset: usize) -> Result<Vec<RandomIndexEntry>> {
    if value.len() < 4 || (value.len() - 4) % 12 != 0 {
        return Err(ParseError::new(
            "random index pack has an invalid size",
            value_offset,
        ));
    }
    let mut reader = Reader::new(value, value_offset);
    let entry_count = (value.len() - 4) / 12;
    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        entries.push(RandomIndexEntry {
            body_sid: reader.read_u32()?,
            byte_offset: reader.read_u64()?,
        });
    }
    let declared_length = reader.read_u32()? as usize;
    let actual_length = value
        .len()
        .checked_add(17)
        .ok_or_else(|| ParseError::new("random index length overflowed", value_offset))?;
    if declared_length != actual_length {
        return Err(ParseError::new(
            "random index pack length is inconsistent",
            reader.position() - 4,
        ));
    }
    Ok(entries)
}

fn validate_structure(
    source_length: usize,
    run_in: usize,
    partitions: &[PartitionPack],
    metadata_sets: &[MetadataSet],
    index_segments: &[IndexSegment],
    essence_elements: &[EssenceElement],
    random_index: &[RandomIndexEntry],
) -> Result<()> {
    if partitions.first().map(|partition| partition.kind) != Some(PartitionKind::Header) {
        return Err(ParseError::new(
            "MXF does not begin with a header partition",
            run_in,
        ));
    }
    if metadata_sets.is_empty() {
        return Err(ParseError::new("MXF header metadata is missing", run_in));
    }
    if essence_elements.is_empty() {
        return Err(ParseError::new(
            "MXF contains no generic-container essence",
            run_in,
        ));
    }
    let mut offsets = BTreeSet::new();
    let mut previous = None;
    for partition in partitions {
        if partition.this_partition != partition.byte_offset
            || !offsets.insert(partition.byte_offset)
        {
            return Err(ParseError::new(
                "partition byte offsets are inconsistent",
                partition.byte_offset as usize,
            ));
        }
        if let Some(previous_offset) = previous {
            if partition.previous_partition != previous_offset {
                return Err(ParseError::new(
                    "partition predecessor link is inconsistent",
                    partition.byte_offset as usize,
                ));
            }
        } else if partition.previous_partition != 0 {
            return Err(ParseError::new(
                "header partition has a predecessor",
                partition.byte_offset as usize,
            ));
        }
        previous = Some(partition.byte_offset);
    }
    let footer = partitions
        .iter()
        .find(|partition| partition.kind == PartitionKind::Footer)
        .map(|partition| partition.byte_offset);
    if let Some(footer_offset) = footer {
        if partitions.iter().any(|partition| {
            partition.footer_partition != 0 && partition.footer_partition != footer_offset
        }) {
            return Err(ParseError::new(
                "partition footer link is inconsistent",
                footer_offset as usize,
            ));
        }
    } else if partitions
        .iter()
        .any(|partition| partition.footer_partition != 0)
    {
        return Err(ParseError::new(
            "partition references a missing footer",
            source_length.saturating_sub(1),
        ));
    }
    for (index, partition) in partitions.iter().enumerate() {
        let declared_end = partition
            .content_offset
            .checked_add(partition.header_byte_count)
            .and_then(|offset| offset.checked_add(partition.index_byte_count))
            .ok_or_else(|| {
                ParseError::new(
                    "partition content range overflowed",
                    partition.byte_offset as usize,
                )
            })?;
        let available_end = partitions
            .get(index + 1)
            .map_or(source_length as u64, |next| next.byte_offset);
        if declared_end > available_end {
            return Err(ParseError::new(
                "partition content is truncated",
                partition.content_offset as usize,
            ));
        }
    }
    let body_sids = partitions
        .iter()
        .filter_map(|partition| (partition.body_sid != 0).then_some(partition.body_sid))
        .collect::<BTreeSet<_>>();
    if essence_elements
        .iter()
        .any(|element| element.body_sid == 0 || !body_sids.contains(&element.body_sid))
    {
        return Err(ParseError::new(
            "essence element is not associated with a body partition",
            run_in,
        ));
    }
    let mut index_ranges = BTreeMap::<(u32, u32), Vec<(i64, i64)>>::new();
    for segment in index_segments {
        let end = segment
            .start_position
            .checked_add(segment.duration)
            .ok_or_else(|| {
                ParseError::new(
                    "index timeline range overflowed",
                    segment.byte_offset as usize,
                )
            })?;
        let ranges = index_ranges
            .entry((segment.body_sid, segment.index_sid))
            .or_default();
        if ranges
            .iter()
            .any(|(start, prior_end)| segment.start_position < *prior_end && end > *start)
        {
            return Err(ParseError::new(
                "index table segments overlap",
                segment.byte_offset as usize,
            ));
        }
        ranges.push((segment.start_position, end));
    }
    if random_index.iter().any(|entry| {
        entry.byte_offset >= source_length as u64
            || !partitions
                .iter()
                .any(|partition| partition.byte_offset == entry.byte_offset)
    }) {
        return Err(ParseError::new(
            "random index references an unknown partition",
            source_length.saturating_sub(1),
        ));
    }
    Ok(())
}

fn is_primer(key: &Ul) -> bool {
    key[..4] == [0x06, 0x0e, 0x2b, 0x34] && key[8..] == PRIMER_SUFFIX
}

fn is_index_segment(key: &Ul) -> bool {
    key[..4] == [0x06, 0x0e, 0x2b, 0x34] && key[8..] == INDEX_SUFFIX
}

fn is_random_index_pack(key: &Ul) -> bool {
    key[..4] == [0x06, 0x0e, 0x2b, 0x34] && key[8..] == RIP_SUFFIX
}

fn is_structural_set(key: &Ul) -> bool {
    key[..5] == [0x06, 0x0e, 0x2b, 0x34, 0x02]
        && matches!(key[5], 0x13 | 0x53)
        && key[6] == 0x01
        && key[8..13] == [0x0d, 0x01, 0x01, 0x01, 0x01]
        && key[13] == 0x01
        && key[15] == 0x00
}

fn is_generic_container_element(key: &Ul) -> bool {
    key[..12] == GENERIC_CONTAINER_PREFIX && matches!(key[12], 0x14..=0x18)
}

fn is_fill(key: &Ul) -> bool {
    key[..4] == [0x06, 0x0e, 0x2b, 0x34]
        && key[8..] == [0x03, 0x01, 0x02, 0x10, 0x01, 0x00, 0x00, 0x00]
}

fn required_item<'a>(data: &'a [u8], set: &MetadataSet, tag: u16) -> Result<&'a [u8]> {
    set.item(tag)
        .ok_or_else(|| {
            ParseError::new(
                "index table is missing a required property",
                set.byte_offset as usize,
            )
        })?
        .value(data)
}

fn read_u8(value: &[u8], offset: usize) -> Result<u8> {
    value
        .first()
        .copied()
        .filter(|_| value.len() == 1)
        .ok_or_else(|| ParseError::new("metadata integer has the wrong length", offset))
}

fn read_u32(value: &[u8], offset: usize) -> Result<u32> {
    value
        .try_into()
        .map(u32::from_be_bytes)
        .map_err(|_| ParseError::new("metadata integer has the wrong length", offset))
}

fn read_i64(value: &[u8], offset: usize) -> Result<i64> {
    value
        .try_into()
        .map(i64::from_be_bytes)
        .map_err(|_| ParseError::new("metadata integer has the wrong length", offset))
}

fn parse_rational(value: &[u8], offset: usize) -> Result<Rational> {
    let bytes: [u8; 8] = value
        .try_into()
        .map_err(|_| ParseError::new("metadata rational has the wrong length", offset))?;
    Ok(Rational {
        numerator: i32::from_be_bytes(bytes[..4].try_into().expect("rational numerator")),
        denominator: i32::from_be_bytes(bytes[4..].try_into().expect("rational denominator")),
    })
}

fn canonical_tag_for_ul(ul: Ul) -> Option<u16> {
    const MAPPINGS: &[(Ul, u16)] = &[
        (
            [
                0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x15, 0x02, 0x00, 0x00,
                0x00, 0x00,
            ],
            0x3c0a,
        ),
        (
            [
                0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x01, 0x01, 0x03, 0x03, 0x02, 0x01, 0x00,
                0x00, 0x00,
            ],
            0x4402,
        ),
        (
            [
                0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x02, 0x01, 0x04, 0x01, 0x03, 0x00, 0x00,
                0x00, 0x00,
            ],
            0x4804,
        ),
        (
            [
                0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x02, 0x01, 0x07, 0x01, 0x02, 0x01, 0x00,
                0x00, 0x00,
            ],
            0x4802,
        ),
        (
            [
                0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x02, 0x06, 0x01, 0x01, 0x04, 0x02, 0x04,
                0x00, 0x00,
            ],
            0x4803,
        ),
        (
            [
                0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x05, 0x06, 0x01, 0x01, 0x03, 0x05, 0x00,
                0x00, 0x00,
            ],
            0x3006,
        ),
        (
            [
                0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x01, 0x04, 0x06, 0x01, 0x01, 0x00, 0x00,
                0x00, 0x00,
            ],
            0x3001,
        ),
        (
            [
                0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x05, 0x04, 0x02, 0x03, 0x01, 0x01, 0x01,
                0x00, 0x00,
            ],
            0x3d03,
        ),
    ];
    MAPPINGS
        .iter()
        .find_map(|(known, tag)| (*known == ul).then_some(*tag))
}

struct Reader<'a> {
    data: &'a [u8],
    base_offset: usize,
    position: usize,
}

impl<'a> Reader<'a> {
    const fn new(data: &'a [u8], base_offset: usize) -> Self {
        Self {
            data,
            base_offset,
            position: 0,
        }
    }

    fn position(&self) -> usize {
        self.base_offset.saturating_add(self.position)
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_array::<1>()?[0])
    }

    fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.read_array()?))
    }

    fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.read_array()?))
    }

    fn read_i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.read_array()?))
    }

    fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.read_array()?))
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self
            .position
            .checked_add(N)
            .ok_or_else(|| ParseError::new("reader position overflowed", self.position()))?;
        let bytes = self
            .data
            .get(self.position..end)
            .ok_or_else(|| ParseError::new("structured value is truncated", self.position()))?;
        self.position = end;
        Ok(bytes.try_into().expect("validated structured value length"))
    }

    fn skip(&mut self, count: usize) -> Result<()> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| ParseError::new("reader position overflowed", self.position()))?;
        if end > self.data.len() {
            return Err(ParseError::new(
                "structured value is truncated",
                self.position(),
            ));
        }
        self.position = end;
        Ok(())
    }

    fn read_ber_length(&mut self) -> Result<usize> {
        let first = self.read_u8()?;
        if first & 0x80 == 0 {
            return Ok(first as usize);
        }
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 8 {
            return Err(ParseError::new(
                "BER length uses an unsupported form",
                self.position().saturating_sub(1),
            ));
        }
        let mut value = 0_u64;
        for _ in 0..count {
            let byte = self.read_u8()?;
            value = value
                .checked_mul(256)
                .and_then(|value| value.checked_add(u64::from(byte)))
                .ok_or_else(|| ParseError::new("BER length overflowed", self.position()))?;
        }
        usize::try_from(value)
            .map_err(|_| ParseError::new("BER length cannot be represented", self.position()))
    }

    fn read_count(&mut self) -> Result<usize> {
        let count = self.read_u32()? as usize;
        if count > MAX_BATCH_ITEMS {
            return Err(ParseError::new(
                "batch contains too many items",
                self.position().saturating_sub(4),
            ));
        }
        Ok(count)
    }

    fn read_batch<T>(
        &mut self,
        expected_item_length: usize,
        mut read: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<Vec<T>> {
        let count = self.read_count()?;
        let item_length = self.read_u32()? as usize;
        if item_length != expected_item_length {
            return Err(ParseError::new(
                "batch item length is invalid",
                self.position().saturating_sub(4),
            ));
        }
        let required = count
            .checked_mul(item_length)
            .ok_or_else(|| ParseError::new("batch size overflowed", self.position()))?;
        if required > self.remaining() {
            return Err(ParseError::new("batch is truncated", self.position()));
        }
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(read(self)?);
        }
        Ok(values)
    }
}
