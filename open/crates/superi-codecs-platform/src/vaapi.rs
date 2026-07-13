//! Linux VA-API system codec integration.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::collections::{BTreeMap, BTreeSet};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, ChromaSampling, CodecCapability, CodecOperation,
    HardwareAcceleration,
};
use superi_media_io::demux::{CodecId, MediaMetadata, MetadataValue, PacketTiming};

#[cfg(target_os = "linux")]
#[path = "vaapi_linux.rs"]
mod linux;

#[cfg(target_os = "linux")]
#[path = "vvc.rs"]
mod vvc;

#[cfg(target_os = "linux")]
#[path = "vvc_vaapi_linux.rs"]
mod vvc_vaapi_linux;

#[cfg(target_os = "linux")]
pub use linux::{registration, VaapiBackend, VaapiFrameBuffer};

#[cfg(not(target_os = "linux"))]
use superi_media_io::backend::BackendRegistration;

/// Stable codec identifier for H.264 media.
pub const H264_CODEC_ID: &str = "h264";
/// Stable codec identifier for HEVC media.
pub const HEVC_CODEC_ID: &str = "hevc";
/// Stable codec identifier for H.266/VVC media.
pub const VVC_CODEC_ID: &str = "vvc";

const COMPONENT: &str = "superi-codecs-platform.vaapi";
const ANNEX_B_START_CODE: [u8; 4] = [0, 0, 0, 1];

/// Returns no VA-API registration on non-Linux targets.
#[cfg(not(target_os = "linux"))]
pub fn registration() -> Result<Option<BackendRegistration>> {
    Ok(None)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum H264Profile {
    ConstrainedBaseline,
    Main,
    High,
}

impl H264Profile {
    const fn code(self) -> &'static str {
        match self {
            Self::ConstrainedBaseline => "constrained_baseline",
            Self::Main => "main",
            Self::High => "high",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct DriverCapabilities {
    h264_decode: BTreeSet<H264Profile>,
    hevc_decode: bool,
    vvc_decode: bool,
    h264_encode: BTreeSet<H264Profile>,
}

fn capability_set(snapshot: DriverCapabilities) -> Result<BackendCapabilities> {
    let h264 = CodecId::new(H264_CODEC_ID)?;
    let hevc = CodecId::new(HEVC_CODEC_ID)?;
    let vvc = CodecId::new(VVC_CODEC_ID)?;
    let mut values = Vec::new();
    let mut details = Vec::new();
    if !snapshot.h264_decode.is_empty() {
        values.push(BackendCapability::Decode(h264.clone()));
        for profile in snapshot.h264_decode {
            details.push(
                CodecCapability::new(CodecOperation::Decode, h264.clone())
                    .with_profiles([profile.code()])?
                    .with_levels_runtime()
                    .with_bit_depths([8])?
                    .with_chroma_sampling([ChromaSampling::Cs420])?,
            );
        }
    }
    if snapshot.hevc_decode {
        values.push(BackendCapability::Decode(hevc.clone()));
        details.push(
            CodecCapability::new(CodecOperation::Decode, hevc)
                .with_profiles(["main"])?
                .with_levels_runtime()
                .with_bit_depths([8])?
                .with_chroma_sampling([ChromaSampling::Cs420])?,
        );
    }
    if snapshot.vvc_decode {
        values.push(BackendCapability::Decode(vvc.clone()));
        details.push(
            CodecCapability::new(CodecOperation::Decode, vvc)
                .with_profiles(["main_10"])?
                .with_levels_runtime()
                .with_bit_depths([10])?
                .with_chroma_sampling([ChromaSampling::Cs420])?,
        );
    }
    if !snapshot.h264_encode.is_empty() {
        values.push(BackendCapability::Encode(h264.clone()));
        for profile in snapshot.h264_encode {
            details.push(
                CodecCapability::new(CodecOperation::Encode, h264.clone())
                    .with_profiles([profile.code()])?
                    .with_levels_runtime()
                    .with_bit_depths([8])?
                    .with_chroma_sampling([ChromaSampling::Cs420])?,
            );
        }
    }
    BackendCapabilities::new(values)
        .with_hardware_acceleration(HardwareAcceleration::Hardware)
        .with_codec_capabilities(details)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PacketContext {
    timing: PacketTiming,
    keyframe: bool,
    metadata: MediaMetadata,
}

#[derive(Default)]
struct TimingLedger {
    next_token: u64,
    pending: BTreeMap<u64, PacketContext>,
}

impl TimingLedger {
    fn insert(
        &mut self,
        timing: PacketTiming,
        keyframe: bool,
        metadata: MediaMetadata,
    ) -> Result<u64> {
        let token = self.next_token;
        self.next_token = self.next_token.checked_add(1).ok_or_else(|| {
            internal(
                "allocate_vaapi_timing_token",
                "VA-API timing token counter overflowed",
            )
        })?;
        self.pending.insert(
            token,
            PacketContext {
                timing,
                keyframe,
                metadata,
            },
        );
        Ok(token)
    }

    fn remove(&mut self, token: u64) -> Result<PacketContext> {
        self.pending.remove(&token).ok_or_else(|| {
            corrupt(
                "restore_vaapi_timing",
                "VA-API returned an unknown timing token",
            )
        })
    }

    fn clear(&mut self) {
        self.pending.clear();
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum CodecLifecycle {
    #[default]
    Accepting,
    Flushing,
    Ended,
}

impl CodecLifecycle {
    fn ensure_accepting(self, operation: &'static str) -> Result<()> {
        if self == Self::Accepting {
            Ok(())
        } else {
            Err(conflict(
                operation,
                "VA-API codec requires reset before accepting input after flush",
            ))
        }
    }

    fn begin_flush(&mut self) {
        if *self == Self::Accepting {
            *self = Self::Flushing;
        }
    }

    fn finish_flush(&mut self) {
        *self = Self::Ended;
    }

    fn reset(&mut self) {
        *self = Self::Accepting;
    }

    fn ended(self) -> bool {
        self == Self::Ended
    }
}

fn validate_opaque_alpha(metadata: &MediaMetadata) -> Result<()> {
    match metadata.get("video.alpha-mode") {
        None | Some(MetadataValue::Unsigned(0)) => Ok(()),
        Some(MetadataValue::Text(value)) if value == "opaque" => Ok(()),
        Some(_) => Err(unsupported(
            "validate_vaapi_alpha",
            "VA-API H.264 and HEVC profiles cannot preserve a declared alpha payload",
        )),
    }
}

fn normalize_avc_access_unit(
    configuration: &[u8],
    packet: &[u8],
    include_parameter_sets: bool,
) -> Result<Vec<u8>> {
    if configuration.is_empty() {
        return require_annex_b(packet, "normalize_avc_access_unit");
    }
    if configuration.first() != Some(&1) {
        return Err(corrupt(
            "normalize_avc_access_unit",
            "H.264 codec.configuration is not an AVC decoder configuration record",
        ));
    }
    let (length_size, parameter_sets) = parse_avc_configuration(configuration)?;
    let mut normalized = Vec::new();
    if include_parameter_sets {
        append_parameter_sets(&mut normalized, &parameter_sets)?;
    }
    if is_annex_b(packet) {
        normalized.extend_from_slice(packet);
    } else {
        append_length_prefixed_units(
            &mut normalized,
            packet,
            length_size,
            "normalize_avc_access_unit",
        )?;
    }
    Ok(normalized)
}

fn normalize_hevc_access_unit(
    configuration: &[u8],
    packet: &[u8],
    include_parameter_sets: bool,
) -> Result<Vec<u8>> {
    if configuration.is_empty() {
        return require_annex_b(packet, "normalize_hevc_access_unit");
    }
    if configuration.first() != Some(&1) {
        return Err(corrupt(
            "normalize_hevc_access_unit",
            "HEVC codec.configuration is not an HEVC decoder configuration record",
        ));
    }
    let (length_size, parameter_sets) = parse_hevc_configuration(configuration)?;
    let mut normalized = Vec::new();
    if include_parameter_sets {
        append_parameter_sets(&mut normalized, &parameter_sets)?;
    }
    if is_annex_b(packet) {
        normalized.extend_from_slice(packet);
    } else {
        append_length_prefixed_units(
            &mut normalized,
            packet,
            length_size,
            "normalize_hevc_access_unit",
        )?;
    }
    Ok(normalized)
}

fn normalize_vvc_access_unit(
    configuration: &[u8],
    packet: &[u8],
    include_parameter_sets: bool,
) -> Result<Vec<u8>> {
    if configuration.is_empty() {
        return require_annex_b(packet, "normalize_vvc_access_unit");
    }
    let (length_size, parameter_sets) = parse_vvc_configuration(configuration)?;
    let mut normalized = Vec::new();
    if include_parameter_sets {
        append_parameter_sets(&mut normalized, &parameter_sets)?;
    }
    if is_annex_b(packet) {
        normalized.extend_from_slice(packet);
    } else {
        append_length_prefixed_units(
            &mut normalized,
            packet,
            length_size,
            "normalize_vvc_access_unit",
        )?;
    }
    Ok(normalized)
}

fn parse_avc_configuration(configuration: &[u8]) -> Result<(usize, Vec<&[u8]>)> {
    if configuration.len() < 7 {
        return Err(corrupt(
            "parse_avc_configuration",
            "AVC decoder configuration record is truncated",
        ));
    }
    let length_size = usize::from((configuration[4] & 0x03) + 1);
    let mut offset = 6;
    let mut units = Vec::new();
    let sps_count = usize::from(configuration[5] & 0x1f);
    parse_u16_units(configuration, &mut offset, sps_count, &mut units, "SPS")?;
    let pps_count = usize::from(*configuration.get(offset).ok_or_else(|| {
        corrupt(
            "parse_avc_configuration",
            "AVC decoder configuration record is missing its PPS count",
        )
    })?);
    offset += 1;
    parse_u16_units(configuration, &mut offset, pps_count, &mut units, "PPS")?;
    if units.is_empty() {
        return Err(corrupt(
            "parse_avc_configuration",
            "AVC decoder configuration record contains no parameter sets",
        ));
    }
    Ok((length_size, units))
}

fn parse_hevc_configuration(configuration: &[u8]) -> Result<(usize, Vec<&[u8]>)> {
    if configuration.len() < 23 {
        return Err(corrupt(
            "parse_hevc_configuration",
            "HEVC decoder configuration record is truncated",
        ));
    }
    let length_size = usize::from((configuration[21] & 0x03) + 1);
    let array_count = usize::from(configuration[22]);
    let mut offset: usize = 23;
    let mut units = Vec::new();
    for _ in 0..array_count {
        offset = offset.checked_add(1).ok_or_else(|| {
            corrupt(
                "parse_hevc_configuration",
                "HEVC configuration array offset overflowed",
            )
        })?;
        let count = read_be_u16(configuration, &mut offset, "HEVC NAL unit count")?;
        parse_u16_units(
            configuration,
            &mut offset,
            usize::from(count),
            &mut units,
            "HEVC parameter set",
        )?;
    }
    if units.is_empty() {
        return Err(corrupt(
            "parse_hevc_configuration",
            "HEVC decoder configuration record contains no parameter sets",
        ));
    }
    Ok((length_size, units))
}

fn parse_vvc_configuration(configuration: &[u8]) -> Result<(usize, Vec<&[u8]>)> {
    let first = *configuration.first().ok_or_else(|| {
        corrupt(
            "parse_vvc_configuration",
            "VVC decoder configuration record is empty",
        )
    })?;
    if first & 0xf8 != 0xf8 {
        return Err(corrupt(
            "parse_vvc_configuration",
            "codec.configuration is not a VVC decoder configuration record",
        ));
    }
    let length_size = usize::from(((first >> 1) & 0x03) + 1);
    let ptl_present = first & 1 != 0;
    let mut offset = 1_usize;
    if ptl_present {
        let packed = usize::from(read_be_u16(
            configuration,
            &mut offset,
            "VVC output-layer and sublayer fields",
        )?);
        let num_sublayers = (packed >> 4) & 0x07;
        take_bytes(configuration, &mut offset, 1, "VVC bit-depth fields")?;
        let constraint_bytes = usize::from(
            *take_bytes(configuration, &mut offset, 1, "VVC constraint length")?
                .first()
                .expect("one-byte checked slice")
                & 0x3f,
        );
        if constraint_bytes == 0 {
            return Err(corrupt(
                "parse_vvc_configuration",
                "VVC profile-tier-level constraint length must be nonzero",
            ));
        }
        take_bytes(configuration, &mut offset, 2, "VVC profile and level")?;
        take_bytes(
            configuration,
            &mut offset,
            constraint_bytes,
            "VVC general constraint info",
        )?;
        let sublayer_flags = if num_sublayers > 1 {
            *take_bytes(configuration, &mut offset, 1, "VVC sublayer level flags")?
                .first()
                .expect("one-byte checked slice")
        } else {
            0
        };
        let sublayer_levels = (0..num_sublayers.saturating_sub(1))
            .filter(|index| sublayer_flags & (1 << (7 - index)) != 0)
            .count();
        take_bytes(
            configuration,
            &mut offset,
            sublayer_levels,
            "VVC sublayer levels",
        )?;
        let subprofiles = usize::from(
            *take_bytes(configuration, &mut offset, 1, "VVC subprofile count")?
                .first()
                .expect("one-byte checked slice"),
        );
        take_bytes(
            configuration,
            &mut offset,
            subprofiles.checked_mul(4).ok_or_else(|| {
                corrupt(
                    "parse_vvc_configuration",
                    "VVC subprofile table size overflowed",
                )
            })?,
            "VVC subprofiles",
        )?;
        take_bytes(
            configuration,
            &mut offset,
            6,
            "VVC maximum dimensions and frame rate",
        )?;
    }
    let array_count = usize::from(
        *take_bytes(configuration, &mut offset, 1, "VVC array count")?
            .first()
            .expect("one-byte checked slice"),
    );
    let mut units = Vec::new();
    for _ in 0..array_count {
        let nal_type = *take_bytes(configuration, &mut offset, 1, "VVC array header")?
            .first()
            .expect("one-byte checked slice")
            & 0x1f;
        if !matches!(nal_type, 12..=16 | 23 | 24) {
            return Err(corrupt(
                "parse_vvc_configuration",
                "VVC decoder configuration contains an invalid NAL array type",
            ));
        }
        let count = if matches!(nal_type, 12 | 13) {
            1
        } else {
            usize::from(read_be_u16(
                configuration,
                &mut offset,
                "VVC NAL unit count",
            )?)
        };
        parse_u16_units(
            configuration,
            &mut offset,
            count,
            &mut units,
            "VVC parameter set",
        )?;
    }
    if units.is_empty() {
        return Err(corrupt(
            "parse_vvc_configuration",
            "VVC decoder configuration record contains no parameter sets",
        ));
    }
    if offset != configuration.len() {
        return Err(corrupt(
            "parse_vvc_configuration",
            "VVC decoder configuration record has trailing bytes",
        ));
    }
    Ok((length_size, units))
}

fn take_bytes<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    length: usize,
    label: &'static str,
) -> Result<&'a [u8]> {
    let end = offset.checked_add(length).ok_or_else(|| {
        corrupt(
            "parse_vvc_configuration",
            "VVC configuration offset overflowed",
        )
    })?;
    let value = bytes.get(*offset..end).ok_or_else(|| {
        corrupt(
            "parse_vvc_configuration",
            format!("VVC decoder configuration record is missing {label}"),
        )
    })?;
    *offset = end;
    Ok(value)
}

fn parse_u16_units<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    count: usize,
    units: &mut Vec<&'a [u8]>,
    label: &'static str,
) -> Result<()> {
    for _ in 0..count {
        let length = usize::from(read_be_u16(bytes, offset, label)?);
        if length == 0 {
            return Err(corrupt(
                "parse_codec_configuration",
                "codec configuration contains an empty parameter set",
            ));
        }
        let end = offset.checked_add(length).ok_or_else(|| {
            corrupt(
                "parse_codec_configuration",
                "codec parameter-set length overflowed",
            )
        })?;
        let unit = bytes.get(*offset..end).ok_or_else(|| {
            corrupt(
                "parse_codec_configuration",
                "codec parameter set extends beyond its configuration record",
            )
        })?;
        units.push(unit);
        *offset = end;
    }
    Ok(())
}

fn read_be_u16(bytes: &[u8], offset: &mut usize, label: &'static str) -> Result<u16> {
    let end = offset.checked_add(2).ok_or_else(|| {
        corrupt(
            "parse_codec_configuration",
            "codec configuration offset overflowed",
        )
    })?;
    let pair: [u8; 2] = bytes
        .get(*offset..end)
        .ok_or_else(|| {
            corrupt(
                "parse_codec_configuration",
                format!("codec configuration is missing {label}"),
            )
        })?
        .try_into()
        .expect("two-byte checked slice");
    *offset = end;
    Ok(u16::from_be_bytes(pair))
}

fn append_parameter_sets(output: &mut Vec<u8>, units: &[&[u8]]) -> Result<()> {
    for unit in units {
        append_annex_b_unit(output, unit, "append_parameter_sets")?;
    }
    Ok(())
}

fn append_length_prefixed_units(
    output: &mut Vec<u8>,
    packet: &[u8],
    length_size: usize,
    operation: &'static str,
) -> Result<()> {
    if !(1..=4).contains(&length_size) {
        return Err(corrupt(
            operation,
            "NAL length field size is outside 1 through 4",
        ));
    }
    let mut offset = 0;
    while offset < packet.len() {
        let length_end = offset
            .checked_add(length_size)
            .ok_or_else(|| corrupt(operation, "NAL length field offset overflowed"))?;
        let length_bytes = packet.get(offset..length_end).ok_or_else(|| {
            corrupt(
                operation,
                "compressed packet ends inside a NAL length field",
            )
        })?;
        let mut length = 0_usize;
        for byte in length_bytes {
            length = length
                .checked_mul(256)
                .and_then(|value| value.checked_add(usize::from(*byte)))
                .ok_or_else(|| corrupt(operation, "NAL unit length overflowed"))?;
        }
        if length == 0 {
            return Err(corrupt(
                operation,
                "compressed packet contains an empty NAL unit",
            ));
        }
        let unit_end = length_end
            .checked_add(length)
            .ok_or_else(|| corrupt(operation, "NAL unit end offset overflowed"))?;
        let unit = packet
            .get(length_end..unit_end)
            .ok_or_else(|| corrupt(operation, "NAL unit extends beyond the compressed packet"))?;
        append_annex_b_unit(output, unit, operation)?;
        offset = unit_end;
    }
    if output.is_empty() {
        return Err(corrupt(
            operation,
            "compressed packet contains no NAL units",
        ));
    }
    Ok(())
}

fn append_annex_b_unit(output: &mut Vec<u8>, unit: &[u8], operation: &'static str) -> Result<()> {
    output
        .len()
        .checked_add(ANNEX_B_START_CODE.len())
        .and_then(|length| length.checked_add(unit.len()))
        .ok_or_else(|| corrupt(operation, "Annex B access unit size overflowed"))?;
    output.extend_from_slice(&ANNEX_B_START_CODE);
    output.extend_from_slice(unit);
    Ok(())
}

fn require_annex_b(packet: &[u8], operation: &'static str) -> Result<Vec<u8>> {
    if is_annex_b(packet) {
        Ok(packet.to_vec())
    } else {
        Err(corrupt(
            operation,
            "length-prefixed packet requires codec.configuration metadata",
        ))
    }
}

fn is_annex_b(bytes: &[u8]) -> bool {
    bytes.starts_with(&ANNEX_B_START_CODE) || bytes.starts_with(&[0, 0, 1])
}

fn categorized(
    operation: &'static str,
    category: ErrorCategory,
    recoverability: Recoverability,
    message: impl Into<String>,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn corrupt(operation: &'static str, message: impl Into<String>) -> Error {
    categorized(
        operation,
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
}

fn unsupported(operation: &'static str, message: impl Into<String>) -> Error {
    categorized(
        operation,
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
}

fn conflict(operation: &'static str, message: impl Into<String>) -> Error {
    categorized(
        operation,
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
}

fn internal(operation: &'static str, message: impl Into<String>) -> Error {
    categorized(
        operation,
        ErrorCategory::Internal,
        Recoverability::Terminal,
        message,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use superi_core::time::Timebase;
    use superi_media_io::demux::{MediaMetadata, MetadataValue, PacketTiming};

    #[test]
    fn avcc_access_units_become_annex_b_with_parameter_sets() {
        let configuration = [1, 100, 0, 31, 0xff, 0xe1, 0, 2, 0x67, 0x64, 1, 0, 1, 0x68];
        let packet = [0, 0, 0, 2, 0x65, 0x88];

        let normalized = normalize_avc_access_unit(&configuration, &packet, true).unwrap();

        assert_eq!(
            normalized,
            [0, 0, 0, 1, 0x67, 0x64, 0, 0, 0, 1, 0x68, 0, 0, 0, 1, 0x65, 0x88,]
        );
    }

    #[test]
    fn malformed_length_prefixed_access_unit_is_rejected() {
        let configuration = [1, 100, 0, 31, 0xff, 0xe1, 0, 2, 0x67, 0x64, 1, 0, 1, 0x68];
        let error =
            normalize_avc_access_unit(&configuration, &[0, 0, 0, 9, 0x65], false).unwrap_err();

        assert_eq!(
            error.category(),
            superi_core::error::ErrorCategory::CorruptData
        );
    }

    #[test]
    fn annex_b_access_units_pass_through_without_configuration() {
        let packet = [0, 0, 0, 1, 0x65, 0x88];
        assert_eq!(
            normalize_avc_access_unit(&[], &packet, true).unwrap(),
            packet
        );
        assert_eq!(
            normalize_hevc_access_unit(&[], &packet, true).unwrap(),
            packet
        );
        assert_eq!(
            normalize_vvc_access_unit(&[], &packet, true).unwrap(),
            packet
        );
    }

    #[test]
    fn vvcc_access_units_become_annex_b_with_parameter_sets() {
        let configuration = [
            0xfe, 3, 0x8e, 0, 1, 0, 3, 0, 0x71, 0x01, 0x8f, 0, 1, 0, 3, 0, 0x79, 0x01, 0x90, 0, 1,
            0, 3, 0, 0x81, 0x01,
        ];
        let packet = [0, 0, 0, 3, 0, 0x39, 0x80];

        let normalized = normalize_vvc_access_unit(&configuration, &packet, true).unwrap();

        assert_eq!(
            normalized,
            [
                0, 0, 0, 1, 0, 0x71, 0x01, 0, 0, 0, 1, 0, 0x79, 0x01, 0, 0, 0, 1, 0, 0x81, 0x01, 0,
                0, 0, 1, 0, 0x39, 0x80,
            ]
        );
    }

    #[test]
    fn malformed_vvcc_array_is_rejected() {
        let configuration = [0xfe, 1, 0x8f, 0, 1, 0, 4, 0, 0x79];
        let error = normalize_vvc_access_unit(&configuration, &[0, 0, 0, 3, 0, 0x39, 0x80], true)
            .unwrap_err();

        assert_eq!(error.category(), ErrorCategory::CorruptData);
    }

    #[test]
    fn vvcc_profile_tier_level_uses_high_order_sublayer_flags() {
        let configuration = [
            0xff, 0, 0x30, 0, 1, 0, 0, 0, 0x80, 42, 0, 0, 0, 0, 0, 0, 0, 1, 0x8f, 0, 1, 0, 3, 0,
            0x79, 0x01,
        ];

        let (length_size, units) = parse_vvc_configuration(&configuration).unwrap();

        assert_eq!(length_size, 4);
        assert_eq!(units, [&[0, 0x79, 0x01][..]]);
    }

    #[test]
    fn hvcc_access_units_become_annex_b_with_parameter_sets() {
        let mut configuration = vec![0_u8; 23];
        configuration[0] = 1;
        configuration[21] = 0xff;
        configuration[22] = 3;
        configuration.extend_from_slice(&[
            0x20, 0, 1, 0, 2, 0x40, 1, 0x21, 0, 1, 0, 2, 0x42, 1, 0x22, 0, 1, 0, 2, 0x44, 1,
        ]);
        let packet = [0, 0, 0, 2, 0x26, 1];

        let normalized = normalize_hevc_access_unit(&configuration, &packet, true).unwrap();

        assert_eq!(
            normalized,
            [0, 0, 0, 1, 0x40, 1, 0, 0, 0, 1, 0x42, 1, 0, 0, 0, 1, 0x44, 1, 0, 0, 0, 1, 0x26, 1,]
        );
    }

    #[test]
    fn truncated_hvcc_array_is_rejected() {
        let mut configuration = vec![0_u8; 23];
        configuration[0] = 1;
        configuration[21] = 0xff;
        configuration[22] = 1;
        configuration.extend_from_slice(&[0x20, 0, 1, 0, 4, 0x40]);

        let error =
            normalize_hevc_access_unit(&configuration, &[0, 0, 0, 2, 0x26, 1], true).unwrap_err();
        assert_eq!(
            error.category(),
            superi_core::error::ErrorCategory::CorruptData
        );
    }

    #[test]
    fn timing_tokens_restore_negative_pts_and_metadata() {
        let negative_timing = PacketTiming::new(
            Timebase::new(1, 48_000).unwrap(),
            Some(-1_001),
            Some(-1_024),
            Some(2_002),
        )
        .unwrap();
        let mut metadata = MediaMetadata::new();
        metadata
            .insert("container.sample", MetadataValue::Unsigned(17))
            .unwrap();
        let mut ledger = TimingLedger::default();

        let negative_token = ledger
            .insert(negative_timing, true, metadata.clone())
            .unwrap();
        let positive_timing = PacketTiming::new(
            Timebase::new(1, 48_000).unwrap(),
            Some(2_002),
            Some(2_000),
            Some(1_001),
        )
        .unwrap();
        let positive_token = ledger
            .insert(positive_timing, false, MediaMetadata::new())
            .unwrap();

        let reordered = ledger.remove(positive_token).unwrap();
        assert_eq!(reordered.timing, positive_timing);
        assert!(!reordered.keyframe);

        let restored = ledger.remove(negative_token).unwrap();

        assert_eq!(restored.timing, negative_timing);
        assert!(restored.keyframe);
        assert_eq!(restored.metadata, metadata);
        assert!(ledger.remove(negative_token).is_err());
    }

    #[test]
    fn nonopaque_alpha_is_rejected_without_discarding_it() {
        let mut metadata = MediaMetadata::new();
        metadata
            .insert("video.alpha-mode", MetadataValue::Unsigned(1))
            .unwrap();

        let error = validate_opaque_alpha(&metadata).unwrap_err();
        assert_eq!(
            error.category(),
            superi_core::error::ErrorCategory::Unsupported
        );
    }

    #[test]
    fn capability_snapshot_never_advertises_unavailable_operations() {
        let capabilities = capability_set(DriverCapabilities {
            h264_decode: [H264Profile::Main, H264Profile::High].into_iter().collect(),
            hevc_decode: true,
            vvc_decode: true,
            h264_encode: [H264Profile::Main].into_iter().collect(),
        })
        .unwrap();

        assert_eq!(
            capabilities.iter().cloned().collect::<Vec<_>>(),
            vec![
                BackendCapability::Decode(CodecId::new(H264_CODEC_ID).unwrap()),
                BackendCapability::Decode(CodecId::new(HEVC_CODEC_ID).unwrap()),
                BackendCapability::Decode(CodecId::new(VVC_CODEC_ID).unwrap()),
                BackendCapability::Encode(CodecId::new(H264_CODEC_ID).unwrap()),
            ]
        );
        assert_eq!(
            capabilities.hardware_acceleration(),
            HardwareAcceleration::Hardware
        );
        assert_eq!(capabilities.codec_capabilities().count(), 5);
        let vvc = capabilities
            .codec_capabilities()
            .find(|detail| detail.codec().as_str() == VVC_CODEC_ID)
            .unwrap();
        assert_eq!(
            vvc.profiles().values().unwrap().iter().collect::<Vec<_>>(),
            ["main_10"]
        );
        assert_eq!(
            vvc.bit_depths()
                .values()
                .unwrap()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [10]
        );
        assert!(capability_set(DriverCapabilities::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn lifecycle_requires_reset_after_flush() {
        let mut lifecycle = CodecLifecycle::default();
        lifecycle.begin_flush();
        lifecycle.finish_flush();

        assert!(lifecycle.ensure_accepting("send_vaapi_packet").is_err());
        lifecycle.reset();
        assert!(lifecycle.ensure_accepting("send_vaapi_packet").is_ok());
    }

    #[test]
    fn metadata_clone_retains_binary_configuration() {
        let mut metadata = MediaMetadata::new();
        metadata
            .insert(
                "codec.configuration",
                MetadataValue::Bytes(Arc::from([1_u8, 2, 3])),
            )
            .unwrap();
        let mut ledger = TimingLedger::default();
        let timing =
            PacketTiming::new(Timebase::new(1, 1_000).unwrap(), Some(2), None, Some(1)).unwrap();

        let token = ledger.insert(timing, false, metadata.clone()).unwrap();
        assert_eq!(ledger.remove(token).unwrap().metadata, metadata);
    }
}
