//! Stateful VVC access-unit parsing for the Linux VA-API backend.

use std::collections::BTreeMap;
use std::sync::Arc;

use oxideav_h266::aps::{parse_aps, AdaptationParameterSet};
use oxideav_h266::bitreader::BitReader;
use oxideav_h266::nal::{extract_rbsp, iter_annex_b, NalHeader, NalUnitType};
use oxideav_h266::picture_header::{
    parse_picture_header, parse_picture_header_stateful, PictureHeader,
};
use oxideav_h266::pps::{parse_pps, PicParameterSet};
use oxideav_h266::ref_pic_list::parse_ref_pic_lists;
use oxideav_h266::slice_header::{
    parse_slice_header_stateful, PhState, SliceType, StatefulSliceHeader,
};
use oxideav_h266::sps::{parse_sps, SeqParameterSet};
use oxideav_h266::vps::{parse_vps, VideoParameterSet};

const MAX_PARAMETER_SETS: usize = 64;
const MAX_SLICES_PER_PICTURE: usize = 600;
const MAX_SLICE_HEADER_PARSE_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct ParsedVvcSlice {
    pub(crate) header: StatefulSliceHeader,
    pub(crate) nal: Vec<u8>,
    pub(crate) slice_data_byte_offset: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedVvcPicture {
    pub(crate) token: u64,
    pub(crate) nal_unit_type: NalUnitType,
    pub(crate) temporal_id: u8,
    pub(crate) poc: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) sps: Arc<SeqParameterSet>,
    pub(crate) pps: Arc<PicParameterSet>,
    pub(crate) ph: Arc<PictureHeader>,
    pub(crate) aps: Vec<Arc<AdaptationParameterSet>>,
    pub(crate) slices: Vec<ParsedVvcSlice>,
}

impl ParsedVvcPicture {
    pub(crate) fn is_intra(&self) -> bool {
        self.slices
            .iter()
            .all(|slice| slice.header.sh_slice_type == SliceType::I)
    }
}

#[derive(Default)]
pub(crate) struct VvcBitstreamParser {
    vps: BTreeMap<u8, Arc<VideoParameterSet>>,
    sps: BTreeMap<u8, Arc<SeqParameterSet>>,
    pps: BTreeMap<u8, Arc<PicParameterSet>>,
    aps: BTreeMap<(u8, u8), Arc<AdaptationParameterSet>>,
    picture_header: Option<Arc<PictureHeader>>,
    previous_tid0_poc: Option<i32>,
}

impl VvcBitstreamParser {
    pub(crate) fn parse_access_unit(
        &mut self,
        token: u64,
        data: &[u8],
    ) -> Result<Option<ParsedVvcPicture>, String> {
        let mut picture: Option<ParsedVvcPicture> = None;
        let mut saw_unit = false;
        for unit in iter_annex_b(data) {
            saw_unit = true;
            if unit.header.nuh_layer_id != 0 {
                return Err("VVC VA-API Main 10 accepts single-layer bitstreams only".to_owned());
            }
            let rbsp = extract_rbsp(unit.payload());
            match unit.header.nal_unit_type {
                NalUnitType::VpsNut => {
                    let value = Arc::new(parse_vps(&rbsp).map_err(parser_error)?);
                    insert_bounded(
                        &mut self.vps,
                        value.vps_video_parameter_set_id,
                        value,
                        "VPS",
                    )?;
                }
                NalUnitType::SpsNut => {
                    let value = Arc::new(parse_sps(&rbsp).map_err(parser_error)?);
                    validate_sps(&value)?;
                    insert_bounded(&mut self.sps, value.sps_seq_parameter_set_id, value, "SPS")?;
                }
                NalUnitType::PpsNut => {
                    let mut value = parse_pps(&rbsp).map_err(parser_error)?;
                    // H.266 infers the picture-header placement flags to zero when picture
                    // partitioning is absent. oxideav-h266 0.0.8 currently initializes them
                    // to one, which shifts every following embedded PH and slice field.
                    if value.pps_no_pic_partition_flag {
                        value.pps_dbf_info_in_ph_flag = false;
                        value.pps_rpl_info_in_ph_flag = false;
                        value.pps_sao_info_in_ph_flag = false;
                        value.pps_alf_info_in_ph_flag = false;
                        value.pps_wp_info_in_ph_flag = false;
                        value.pps_qp_delta_info_in_ph_flag = false;
                    }
                    let value = Arc::new(value);
                    insert_bounded(&mut self.pps, value.pps_pic_parameter_set_id, value, "PPS")?;
                }
                NalUnitType::PrefixApsNut | NalUnitType::SuffixApsNut => {
                    let value = Arc::new(parse_aps(&rbsp).map_err(parser_error)?);
                    let key = (
                        value.aps_params_type.as_u8(),
                        value.aps_adaptation_parameter_set_id,
                    );
                    insert_bounded(&mut self.aps, key, value, "APS")?;
                }
                NalUnitType::PhNut => {
                    let (sps, pps) = self.parameter_sets_for_picture_header(&rbsp)?;
                    let ph = Arc::new(
                        parse_picture_header_stateful(&rbsp, &sps, &pps).map_err(parser_error)?,
                    );
                    self.picture_header = Some(ph);
                }
                nal_type if nal_type.is_vcl() => {
                    let parsed = self.parse_slice(unit.header, unit.raw, &rbsp)?;
                    match &mut picture {
                        None => {
                            let poc = derive_poc(
                                parsed.ph.as_ref(),
                                parsed.sps.as_ref(),
                                nal_type,
                                self.previous_tid0_poc,
                            )?;
                            picture = Some(ParsedVvcPicture {
                                token,
                                nal_unit_type: nal_type,
                                temporal_id: unit.header.temporal_id(),
                                poc,
                                width: parsed.pps.pps_pic_width_in_luma_samples,
                                height: parsed.pps.pps_pic_height_in_luma_samples,
                                sps: parsed.sps,
                                pps: parsed.pps,
                                ph: parsed.ph,
                                aps: self.aps.values().map(Arc::clone).collect(),
                                slices: vec![parsed.slice],
                            });
                        }
                        Some(current) => {
                            if current.ph.ph_pic_order_cnt_lsb != parsed.ph.ph_pic_order_cnt_lsb
                                || (current.nal_unit_type != nal_type
                                    && !current.pps.pps_mixed_nalu_types_in_pic_flag)
                            {
                                return Err(
                                    "one compressed packet contains more than one VVC picture"
                                        .to_owned(),
                                );
                            }
                            if current.slices.len() >= MAX_SLICES_PER_PICTURE {
                                return Err(
                                    "VVC picture exceeds the supported slice count".to_owned()
                                );
                            }
                            current.slices.push(parsed.slice);
                        }
                    }
                }
                NalUnitType::EosNut | NalUnitType::EobNut => {
                    self.picture_header = None;
                    self.previous_tid0_poc = None;
                }
                _ => {}
            }
        }
        if !saw_unit {
            return Err("VVC access unit contains no valid Annex B NAL units".to_owned());
        }
        if let Some(value) = &picture {
            if value.temporal_id == 0
                && !matches!(
                    value.nal_unit_type,
                    NalUnitType::RadlNut | NalUnitType::RaslNut
                )
            {
                self.previous_tid0_poc = Some(value.poc);
            }
        }
        Ok(picture)
    }

    fn parse_slice(
        &mut self,
        nal_header: NalHeader,
        raw_nal: &[u8],
        rbsp: &[u8],
    ) -> Result<ActiveSlice, String> {
        let embedded_ph = first_bit(rbsp)?;
        let parse_source = &rbsp[..rbsp.len().min(MAX_SLICE_HEADER_PARSE_BYTES)];
        let (ph, ph_bits, sps, pps) = if embedded_ph {
            let shifted = copy_bits(parse_source, 1)?;
            let (sps, pps) = self.parameter_sets_for_picture_header(&shifted)?;
            let ph = Arc::new(
                parse_picture_header_stateful(&shifted, &sps, &pps).map_err(parser_error)?,
            );
            let consumed = ph.consumed_bits;
            self.picture_header = Some(Arc::clone(&ph));
            (ph, consumed, sps, pps)
        } else {
            let ph = Arc::clone(self.picture_header.as_ref().ok_or_else(|| {
                "VVC slice does not carry a picture header and no PH NAL is active".to_owned()
            })?);
            let pps = Arc::clone(
                self.pps
                    .get(&(ph.ph_pic_parameter_set_id as u8))
                    .ok_or_else(|| "VVC picture header references an unknown PPS".to_owned())?,
            );
            let sps = Arc::clone(
                self.sps
                    .get(&pps.pps_seq_parameter_set_id)
                    .ok_or_else(|| "VVC PPS references an unknown SPS".to_owned())?,
            );
            (ph, 0, sps, pps)
        };
        validate_picture_sets(&sps, &pps, &ph)?;

        let mut ph_state = PhState {
            ph_inter_slice_allowed_flag: ph.ph_inter_slice_allowed_flag,
            ph_intra_slice_allowed_flag: ph.ph_intra_slice_allowed_flag,
            ph_alf_enabled_flag: ph.ph_alf_enabled_flag,
            ph_lmcs_enabled_flag: ph.ph_lmcs_enabled_flag,
            ph_explicit_scaling_list_enabled_flag: ph.ph_explicit_scaling_list_enabled_flag,
            ph_temporal_mvp_enabled_flag: ph.ph_temporal_mvp_enabled_flag,
            ph_sao_luma_enabled_flag: ph.ph_sao_luma_enabled_flag,
            ph_sao_chroma_enabled_flag: ph.ph_sao_chroma_enabled_flag,
            num_extra_sh_bits: sps.num_extra_sh_bits,
            nal_unit_type: nal_header.nal_unit_type,
        };
        let parse_rbsp = if embedded_ph {
            ph_state.ph_lmcs_enabled_flag = false;
            ph_state.ph_explicit_scaling_list_enabled_flag = false;
            replace_embedded_picture_header(parse_source, ph_bits)?
        } else {
            parse_source.to_vec()
        };
        let (parse_rbsp, removed_rpl_bits) =
            strip_slice_rpl(parse_rbsp, &sps, &pps, &ph_state, nal_header.nal_unit_type)?;
        let mut header = parse_slice_header_stateful(&parse_rbsp, &sps, &pps, &ph_state)
            .map_err(parser_error)?;
        if embedded_ph {
            header.sh_picture_header_in_slice_header_flag = true;
            header.sh_lmcs_used_flag = ph.ph_lmcs_enabled_flag;
            header.sh_explicit_scaling_list_used_flag = ph.ph_explicit_scaling_list_enabled_flag;
        }
        if pps.pps_alf_info_in_ph_flag {
            header.sh_alf_enabled_flag = ph.ph_alf_enabled_flag;
            header.sh_num_alf_aps_ids_luma = ph.ph_num_alf_aps_ids_luma;
            header.sh_alf_aps_id_luma.clone_from(&ph.ph_alf_aps_id_luma);
            header.sh_alf_cb_enabled_flag = ph.ph_alf_cb_enabled_flag;
            header.sh_alf_cr_enabled_flag = ph.ph_alf_cr_enabled_flag;
            header.sh_alf_aps_id_chroma = ph.ph_alf_aps_id_chroma;
            header.sh_alf_cc_cb_enabled_flag = ph.ph_alf_cc_cb_enabled_flag;
            header.sh_alf_cc_cb_aps_id = ph.ph_alf_cc_cb_aps_id;
            header.sh_alf_cc_cr_enabled_flag = ph.ph_alf_cc_cr_enabled_flag;
            header.sh_alf_cc_cr_aps_id = ph.ph_alf_cc_cr_aps_id;
        }
        let alignment_position = header
            .byte_alignment_bit_pos
            .checked_add(ph_bits)
            .and_then(|position| position.checked_add(removed_rpl_bits))
            .ok_or_else(|| "VVC slice header bit offset overflowed".to_owned())?;
        let byte_offset = alignment_position
            .checked_add(8)
            .ok_or_else(|| "VVC slice data byte offset overflowed".to_owned())?
            / 8;
        let slice_data_byte_offset = raw_nal_offset(
            raw_nal,
            usize::try_from(byte_offset)
                .map_err(|_| "VVC RBSP byte offset cannot be represented".to_owned())?,
        )?;
        if slice_data_byte_offset as usize > raw_nal.len() {
            return Err("VVC slice header extends beyond its NAL unit".to_owned());
        }
        Ok(ActiveSlice {
            sps,
            pps,
            ph,
            slice: ParsedVvcSlice {
                header,
                nal: raw_nal.to_vec(),
                slice_data_byte_offset,
            },
        })
    }

    fn parameter_sets_for_picture_header(
        &self,
        rbsp: &[u8],
    ) -> Result<(Arc<SeqParameterSet>, Arc<PicParameterSet>), String> {
        let lead = parse_picture_header(rbsp).map_err(parser_error)?;
        let pps = Arc::clone(
            self.pps
                .get(&(lead.ph_pic_parameter_set_id as u8))
                .ok_or_else(|| "VVC picture header references an unknown PPS".to_owned())?,
        );
        let sps = Arc::clone(
            self.sps
                .get(&pps.pps_seq_parameter_set_id)
                .ok_or_else(|| "VVC PPS references an unknown SPS".to_owned())?,
        );
        Ok((sps, pps))
    }
}

struct ActiveSlice {
    sps: Arc<SeqParameterSet>,
    pps: Arc<PicParameterSet>,
    ph: Arc<PictureHeader>,
    slice: ParsedVvcSlice,
}

fn validate_sps(sps: &SeqParameterSet) -> Result<(), String> {
    if sps.sps_video_parameter_set_id != 0 {
        return Err("VVC VA-API Main 10 does not accept multilayer VPS references".to_owned());
    }
    if sps.sps_chroma_format_idc != 1 || sps.bit_depth_y() != 10 {
        return Err("VVC VA-API path accepts Main 10 4:2:0 bitstreams only".to_owned());
    }
    if sps.sps_pic_width_max_in_luma_samples == 0
        || sps.sps_pic_height_max_in_luma_samples == 0
        || sps.sps_pic_width_max_in_luma_samples > u32::from(u16::MAX)
        || sps.sps_pic_height_max_in_luma_samples > u32::from(u16::MAX)
    {
        return Err("VVC SPS dimensions exceed the VA-API domain".to_owned());
    }
    Ok(())
}

fn validate_picture_sets(
    sps: &SeqParameterSet,
    pps: &PicParameterSet,
    ph: &PictureHeader,
) -> Result<(), String> {
    validate_sps(sps)?;
    if pps.pps_seq_parameter_set_id != sps.sps_seq_parameter_set_id
        || ph.ph_pic_parameter_set_id != u32::from(pps.pps_pic_parameter_set_id)
    {
        return Err("VVC picture parameter-set identity is inconsistent".to_owned());
    }
    if pps.pps_pic_width_in_luma_samples == 0
        || pps.pps_pic_height_in_luma_samples == 0
        || pps.pps_pic_width_in_luma_samples > u32::from(u16::MAX)
        || pps.pps_pic_height_in_luma_samples > u32::from(u16::MAX)
    {
        return Err("VVC picture dimensions exceed the VA-API domain".to_owned());
    }
    Ok(())
}

fn derive_poc(
    ph: &PictureHeader,
    sps: &SeqParameterSet,
    nal_type: NalUnitType,
    previous_tid0_poc: Option<i32>,
) -> Result<i32, String> {
    if matches!(nal_type, NalUnitType::IdrWRadl | NalUnitType::IdrNLp) {
        return Ok(0);
    }
    let max_lsb = 1_i64
        .checked_shl(u32::from(sps.sps_log2_max_pic_order_cnt_lsb_minus4) + 4)
        .ok_or_else(|| "VVC POC LSB width is invalid".to_owned())?;
    let lsb = i64::from(ph.ph_pic_order_cnt_lsb);
    let msb = if ph.ph_poc_msb_cycle_present_flag {
        i64::from(ph.ph_poc_msb_cycle_val)
            .checked_mul(max_lsb)
            .ok_or_else(|| "VVC POC MSB cycle overflowed".to_owned())?
    } else if let Some(previous) = previous_tid0_poc {
        let previous = i64::from(previous);
        let previous_lsb = previous.rem_euclid(max_lsb);
        let previous_msb = previous - previous_lsb;
        if lsb < previous_lsb && previous_lsb - lsb >= max_lsb / 2 {
            previous_msb + max_lsb
        } else if lsb > previous_lsb && lsb - previous_lsb > max_lsb / 2 {
            previous_msb - max_lsb
        } else {
            previous_msb
        }
    } else {
        0
    };
    i32::try_from(msb + lsb).map_err(|_| "VVC POC exceeds the VA-API domain".to_owned())
}

// oxideav-h266 0.0.8 does not consume a slice-owned ref_pic_lists() block.
// Remove that bounded syntax only from the temporary header parse and add its
// exact bit length back when deriving the offset into the original NAL unit.
fn strip_slice_rpl(
    rbsp: Vec<u8>,
    sps: &SeqParameterSet,
    pps: &PicParameterSet,
    ph_state: &PhState,
    nal_unit_type: NalUnitType,
) -> Result<(Vec<u8>, u64), String> {
    if pps.pps_rpl_info_in_ph_flag {
        return Ok((rbsp, 0));
    }
    if sps.sps_subpic_info_present_flag || !pps.pps_no_pic_partition_flag || pps.partition.is_some()
    {
        return Err(
            "VVC VA-API decoder cannot parse RPLs for partitioned or subpicture input".to_owned(),
        );
    }
    let mut reader = BitReader::new(&rbsp);
    if reader.u1().map_err(parser_error)? != 0 {
        return Err("internal VVC slice normalization retained an embedded PH flag".to_owned());
    }
    for _ in 0..ph_state.num_extra_sh_bits {
        reader.u1().map_err(parser_error)?;
    }
    if ph_state.ph_inter_slice_allowed_flag {
        let slice_type =
            SliceType::from_ue(reader.ue().map_err(parser_error)?).map_err(parser_error)?;
        if slice_type != SliceType::I {
            return Err("VVC VA-API decoder does not yet support inter pictures".to_owned());
        }
    }
    if nal_unit_type.is_irap() {
        reader.u1().map_err(parser_error)?;
    }
    if sps.tool_flags.alf_enabled_flag && !pps.pps_alf_info_in_ph_flag {
        let alf_enabled = reader.u1().map_err(parser_error)? != 0;
        if alf_enabled {
            let luma_count = reader.u(3).map_err(parser_error)?;
            reader
                .skip(luma_count.saturating_mul(3))
                .map_err(parser_error)?;
            let mut chroma_enabled = false;
            if sps.sps_chroma_format_idc != 0 {
                let cb_enabled = reader.u1().map_err(parser_error)? != 0;
                let cr_enabled = reader.u1().map_err(parser_error)? != 0;
                chroma_enabled = cb_enabled || cr_enabled;
            }
            if chroma_enabled {
                reader.skip(3).map_err(parser_error)?;
            }
            if sps.tool_flags.ccalf_enabled_flag {
                if reader.u1().map_err(parser_error)? != 0 {
                    reader.skip(3).map_err(parser_error)?;
                }
                if reader.u1().map_err(parser_error)? != 0 {
                    reader.skip(3).map_err(parser_error)?;
                }
            }
        }
    }
    if ph_state.ph_lmcs_enabled_flag {
        reader.u1().map_err(parser_error)?;
    }
    if ph_state.ph_explicit_scaling_list_enabled_flag {
        reader.u1().map_err(parser_error)?;
    }
    let rpl_start = reader.bit_position();
    parse_ref_pic_lists(&mut reader, sps, pps).map_err(parser_error)?;
    let rpl_end = reader.bit_position();
    let removed = rpl_end
        .checked_sub(rpl_start)
        .ok_or_else(|| "VVC RPL bit range underflowed".to_owned())?;
    Ok((remove_bit_range(&rbsp, rpl_start, rpl_end)?, removed))
}

fn remove_bit_range(bytes: &[u8], start: u64, end: u64) -> Result<Vec<u8>, String> {
    let total = u64::try_from(bytes.len())
        .map_err(|_| "VVC bitstream length cannot be represented".to_owned())?
        .checked_mul(8)
        .ok_or_else(|| "VVC bitstream length overflowed".to_owned())?;
    if start > end || end > total {
        return Err("VVC bit removal range is invalid".to_owned());
    }
    let removed = end - start;
    let remaining = usize::try_from(total - removed)
        .map_err(|_| "VVC normalized bit length cannot be represented".to_owned())?;
    let start = usize::try_from(start)
        .map_err(|_| "VVC normalized bit offset cannot be represented".to_owned())?;
    let removed = usize::try_from(removed)
        .map_err(|_| "VVC removed bit length cannot be represented".to_owned())?;
    let mut output = vec![0_u8; remaining.div_ceil(8)];
    for destination in 0..remaining {
        let source = if destination < start {
            destination
        } else {
            destination + removed
        };
        let bit = (bytes[source / 8] >> (7 - source % 8)) & 1;
        set_bit(&mut output, destination, bit);
    }
    Ok(output)
}

fn raw_nal_offset(raw_nal: &[u8], rbsp_bytes: usize) -> Result<u32, String> {
    let payload = raw_nal
        .get(2..)
        .ok_or_else(|| "VVC NAL unit is missing its two-byte header".to_owned())?;
    let mut raw_offset = 0_usize;
    let mut decoded = 0_usize;
    while decoded < rbsp_bytes {
        let byte = *payload
            .get(raw_offset)
            .ok_or_else(|| "VVC slice header extends beyond its NAL unit".to_owned())?;
        if raw_offset >= 2
            && payload[raw_offset - 2] == 0
            && payload[raw_offset - 1] == 0
            && byte == 0x03
        {
            raw_offset += 1;
            continue;
        }
        raw_offset += 1;
        decoded += 1;
    }
    u32::try_from(
        raw_offset
            .checked_add(2)
            .ok_or_else(|| "VVC raw NAL offset overflowed".to_owned())?,
    )
    .map_err(|_| "VVC slice data byte offset exceeds VA-API limits".to_owned())
}

fn replace_embedded_picture_header(rbsp: &[u8], ph_bits: u64) -> Result<Vec<u8>, String> {
    let tail_start = 1_u64
        .checked_add(ph_bits)
        .ok_or_else(|| "VVC embedded picture-header size overflowed".to_owned())?;
    let tail = copy_bits(rbsp, tail_start)?;
    let mut output = vec![0_u8; (1 + tail.len() * 8).div_ceil(8)];
    for index in 0..tail.len() * 8 {
        let bit = (tail[index / 8] >> (7 - index % 8)) & 1;
        set_bit(&mut output, index + 1, bit);
    }
    Ok(output)
}

fn copy_bits(bytes: &[u8], start: u64) -> Result<Vec<u8>, String> {
    let total = u64::try_from(bytes.len())
        .map_err(|_| "VVC bitstream length cannot be represented".to_owned())?
        .checked_mul(8)
        .ok_or_else(|| "VVC bitstream length overflowed".to_owned())?;
    if start > total {
        return Err("VVC bit offset extends beyond the NAL unit".to_owned());
    }
    let count = usize::try_from(total - start)
        .map_err(|_| "VVC bit range cannot be represented".to_owned())?;
    let mut output = vec![0_u8; count.div_ceil(8)];
    let start = usize::try_from(start).map_err(|_| "VVC bit offset is too large".to_owned())?;
    for index in 0..count {
        let source = start + index;
        let bit = (bytes[source / 8] >> (7 - source % 8)) & 1;
        set_bit(&mut output, index, bit);
    }
    Ok(output)
}

fn set_bit(bytes: &mut [u8], index: usize, bit: u8) {
    bytes[index / 8] |= bit << (7 - index % 8);
}

fn first_bit(bytes: &[u8]) -> Result<bool, String> {
    bytes
        .first()
        .map(|byte| byte & 0x80 != 0)
        .ok_or_else(|| "VVC slice RBSP is empty".to_owned())
}

fn insert_bounded<K: Ord, V>(
    map: &mut BTreeMap<K, Arc<V>>,
    key: K,
    value: Arc<V>,
    label: &str,
) -> Result<(), String> {
    if !map.contains_key(&key) && map.len() >= MAX_PARAMETER_SETS {
        return Err(format!("VVC {label} store exceeds its bounded capacity"));
    }
    map.insert(key, value);
    Ok(())
}

fn parser_error(error: impl std::fmt::Display) -> String {
    format!("VVC bitstream parser rejected input: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_copy_preserves_unaligned_payload() {
        assert_eq!(copy_bits(&[0b1011_0100], 3).unwrap(), [0b1010_0000]);
    }

    #[test]
    fn embedded_picture_header_replacement_keeps_flag_and_tail() {
        let replaced = replace_embedded_picture_header(&[0b1101_0110], 3).unwrap();
        assert_eq!(replaced[0] & 0xf0, 0b0011_0000);
    }

    #[test]
    fn bit_range_removal_preserves_surrounding_bits() {
        assert_eq!(
            remove_bit_range(&[0b1011_0110], 2, 5).unwrap(),
            [0b1011_0000]
        );
    }

    #[test]
    fn raw_nal_offset_counts_emulation_prevention_bytes() {
        let nal = [0, 0x51, 0, 0, 3, 1, 0xaa];
        assert_eq!(raw_nal_offset(&nal, 3).unwrap(), 6);
    }

    #[test]
    fn empty_access_unit_is_rejected() {
        let error = VvcBitstreamParser::default()
            .parse_access_unit(7, &[])
            .unwrap_err();
        assert!(error.contains("no valid Annex B"));
    }

    #[test]
    fn real_fixture_parameter_sets_parse_when_requested() {
        let Some(path) = std::env::var_os("SUPERI_VVC_FIXTURE") else {
            return;
        };
        let data = std::fs::read(path).unwrap();
        let mut parser = VvcBitstreamParser::default();
        let mut access_unit = Vec::new();
        for unit in iter_annex_b(&data) {
            access_unit.extend_from_slice(&[0, 0, 0, 1]);
            access_unit.extend_from_slice(unit.raw);
            if unit.header.nal_unit_type.is_vcl() {
                break;
            }
        }
        let picture = parser.parse_access_unit(0, &access_unit).unwrap().unwrap();
        assert!(!parser.sps.is_empty());
        assert!(!parser.pps.is_empty());
        assert!(!parser.aps.is_empty());
        assert_eq!((picture.width, picture.height), (416, 240));
    }
}
