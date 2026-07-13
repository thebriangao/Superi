//! Windows Media Foundation codec backend and checked stream adapters.
//!
//! Media Foundation consumes H.264 and HEVC elementary streams in Annex B form, while MP4 and
//! MOV store length-prefixed NAL units. The checked configuration types in this module keep that
//! container conversion separate from the Windows FFI boundary. Windows transform discovery and
//! processing are compiled only for the opt-in Windows target.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::demux::CodecId;

/// Stable H.264 codec identifier shared with container readers.
pub const H264_CODEC_ID: &str = "h264";
/// Stable H.265 or HEVC codec identifier shared with container readers.
pub const HEVC_CODEC_ID: &str = "hevc";
/// Stable AAC codec identifier shared with container readers.
pub const AAC_CODEC_ID: &str = "aac";

const COMPONENT: &str = "superi-codecs-platform.media-foundation";
const ANNEX_B_START_CODE: [u8; 4] = [0, 0, 0, 1];

/// ProRes profile identities represented by distinct QuickTime sample-entry codes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ProResProfile {
    /// ProRes 422 Proxy (`apco`).
    Proxy,
    /// ProRes 422 LT (`apcs`).
    Lt,
    /// ProRes 422 (`apcn`).
    Standard,
    /// ProRes 422 HQ (`apch`).
    Hq,
    /// ProRes 4444 (`ap4h`).
    FourFourFourFour,
}

impl ProResProfile {
    /// Every ProRes profile supported by the stable platform-codec contract.
    pub const ALL: &'static [Self] = &[
        Self::Proxy,
        Self::Lt,
        Self::Standard,
        Self::Hq,
        Self::FourFourFourFour,
    ];

    /// Returns the stable codec identifier used by backend selection.
    #[must_use]
    pub fn codec_id(self) -> CodecId {
        CodecId::new(self.code()).expect("static ProRes codec identifiers are valid")
    }

    /// Returns the QuickTime sample-entry code used as a Media Foundation subtype.
    #[must_use]
    pub const fn fourcc(self) -> [u8; 4] {
        match self {
            Self::Proxy => *b"apco",
            Self::Lt => *b"apcs",
            Self::Standard => *b"apcn",
            Self::Hq => *b"apch",
            Self::FourFourFourFour => *b"ap4h",
        }
    }

    const fn code(self) -> &'static str {
        match self {
            Self::Proxy => "prores-422-proxy",
            Self::Lt => "prores-422-lt",
            Self::Standard => "prores-422",
            Self::Hq => "prores-422-hq",
            Self::FourFourFourFour => "prores-4444",
        }
    }
}

/// One compressed format routed through Media Foundation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MediaFoundationCodec {
    /// H.264 or AVC video.
    H264,
    /// H.265 or HEVC video.
    Hevc,
    /// One profile-specific ProRes video stream.
    ProRes(ProResProfile),
    /// AAC audio.
    Aac,
}

impl MediaFoundationCodec {
    /// Returns the stable codec identifier used by media I/O.
    #[must_use]
    pub fn codec_id(self) -> CodecId {
        match self {
            Self::H264 => CodecId::new(H264_CODEC_ID),
            Self::Hevc => CodecId::new(HEVC_CODEC_ID),
            Self::ProRes(profile) => return profile.codec_id(),
            Self::Aac => CodecId::new(AAC_CODEC_ID),
        }
        .expect("static Media Foundation codec identifiers are valid")
    }

    /// Resolves one stable media-I/O identifier.
    #[must_use]
    pub fn from_codec_id(codec: &CodecId) -> Option<Self> {
        match codec.as_str() {
            H264_CODEC_ID => Some(Self::H264),
            HEVC_CODEC_ID => Some(Self::Hevc),
            AAC_CODEC_ID => Some(Self::Aac),
            code => ProResProfile::ALL
                .iter()
                .copied()
                .find(|profile| profile.code() == code)
                .map(Self::ProRes),
        }
    }
}

/// One transform direction discovered and exposed by the Windows backend.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum MediaFoundationOperation {
    /// Compressed packets to decoded frames or audio blocks.
    Decode,
    /// Decoded frames or audio blocks to compressed packets.
    Encode,
}

impl MediaFoundationOperation {
    /// Both operation directions in stable order.
    pub const ALL: &'static [Self] = &[Self::Decode, Self::Encode];
}

/// Checked H.264 or HEVC decoder configuration converted to Annex B parameter sets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnexBConfiguration {
    nal_length_size: usize,
    parameter_sets: Vec<u8>,
}

impl AnnexBConfiguration {
    /// Parses an AVCDecoderConfigurationRecord or HEVCDecoderConfigurationRecord.
    pub fn parse(codec: MediaFoundationCodec, configuration: &[u8]) -> Result<Self> {
        match codec {
            MediaFoundationCodec::H264 => parse_avcc(configuration),
            MediaFoundationCodec::Hevc => parse_hvcc(configuration),
            _ => Err(unsupported(
                "parse_annex_b_configuration",
                "Annex B configuration is defined only for H.264 and HEVC",
            )),
        }
    }

    /// Returns the length-field width stored before each container NAL unit.
    #[must_use]
    pub const fn nal_length_size(&self) -> usize {
        self.nal_length_size
    }

    /// Returns parameter sets with four-byte Annex B start codes.
    #[must_use]
    pub fn parameter_sets(&self) -> &[u8] {
        &self.parameter_sets
    }

    /// Converts one complete length-prefixed sample to Annex B.
    pub fn convert_sample(&self, sample: &[u8], prepend_parameter_sets: bool) -> Result<Vec<u8>> {
        if sample.is_empty() {
            return Err(corrupt(
                "convert_length_prefixed_sample",
                "compressed video sample must not be empty",
            ));
        }
        let mut output = Vec::with_capacity(
            sample
                .len()
                .checked_add(if prepend_parameter_sets {
                    self.parameter_sets.len()
                } else {
                    0
                })
                .ok_or_else(|| {
                    corrupt(
                        "convert_length_prefixed_sample",
                        "Annex B sample size overflowed",
                    )
                })?,
        );
        if prepend_parameter_sets {
            output.extend_from_slice(&self.parameter_sets);
        }

        let mut position = 0_usize;
        while position < sample.len() {
            let length_end = position.checked_add(self.nal_length_size).ok_or_else(|| {
                corrupt(
                    "convert_length_prefixed_sample",
                    "NAL length field range overflowed",
                )
            })?;
            let length_bytes = sample.get(position..length_end).ok_or_else(|| {
                corrupt(
                    "convert_length_prefixed_sample",
                    "compressed sample ends inside a NAL length field",
                )
            })?;
            let length = length_bytes
                .iter()
                .fold(0_usize, |value, byte| (value << 8) | usize::from(*byte));
            if length == 0 {
                return Err(corrupt(
                    "convert_length_prefixed_sample",
                    "compressed sample contains an empty NAL unit",
                ));
            }
            let nal_end = length_end.checked_add(length).ok_or_else(|| {
                corrupt(
                    "convert_length_prefixed_sample",
                    "NAL payload range overflowed",
                )
            })?;
            let nal = sample.get(length_end..nal_end).ok_or_else(|| {
                corrupt(
                    "convert_length_prefixed_sample",
                    "compressed sample ends inside a NAL payload",
                )
            })?;
            output.extend_from_slice(&ANNEX_B_START_CODE);
            output.extend_from_slice(nal);
            position = nal_end;
        }
        Ok(output)
    }
}

/// Parsed AAC decoder setup used to construct a Media Foundation audio type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AacConfiguration {
    audio_object_type: u8,
    sample_rate: u32,
    channel_count: u8,
    audio_specific_config: Vec<u8>,
}

impl AacConfiguration {
    /// Extracts and validates AudioSpecificConfig bytes from raw data or an MP4 `esds` payload.
    pub fn parse(configuration: &[u8]) -> Result<Self> {
        let audio_specific_config = extract_audio_specific_config(configuration)?;
        let mut bits = BitReader::new(audio_specific_config);
        let initial_object_type = bits.read_audio_object_type()?;
        let initial_sample_rate = bits.read_sample_rate()?;
        let channel_configuration = bits.read(4)? as u8;
        let mut audio_object_type = initial_object_type;
        let mut sample_rate = initial_sample_rate;
        if matches!(initial_object_type, 5 | 29) {
            sample_rate = bits.read_sample_rate()?;
            audio_object_type = bits.read_audio_object_type()?;
        }
        if audio_object_type != 2 {
            return Err(unsupported(
                "parse_aac_configuration",
                "Media Foundation AAC requires an AAC-LC core",
            ));
        }
        let channel_count = match channel_configuration {
            1..=6 => channel_configuration,
            _ => {
                return Err(unsupported(
                    "parse_aac_configuration",
                    "Media Foundation AAC supports only explicit one through six channel configurations",
                ))
            }
        };
        let output_channels = if initial_object_type == 29 {
            2
        } else {
            channel_count
        };
        Ok(Self {
            audio_object_type,
            sample_rate,
            channel_count: output_channels,
            audio_specific_config: audio_specific_config.to_vec(),
        })
    }

    /// Returns the validated AAC core object type.
    #[must_use]
    pub const fn audio_object_type(&self) -> u8 {
        self.audio_object_type
    }

    /// Returns the decoded sample rate after explicit SBR signaling.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the decoded channel count after explicit PS signaling.
    #[must_use]
    pub const fn channel_count(&self) -> u8 {
        self.channel_count
    }

    /// Returns exact AudioSpecificConfig bytes.
    #[must_use]
    pub fn audio_specific_config(&self) -> &[u8] {
        &self.audio_specific_config
    }

    /// Builds the `HEAACWAVEINFO` tail followed by AudioSpecificConfig bytes.
    #[must_use]
    pub fn media_foundation_user_data(&self) -> Vec<u8> {
        let mut data = vec![0, 0, 0xfe, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        data.extend_from_slice(&self.audio_specific_config);
        data
    }
}

fn parse_avcc(configuration: &[u8]) -> Result<AnnexBConfiguration> {
    if configuration.len() < 7 || configuration[0] != 1 {
        return Err(corrupt(
            "parse_avcc",
            "AVC decoder configuration is truncated or has an unsupported version",
        ));
    }
    let nal_length_size = usize::from((configuration[4] & 0x03) + 1);
    let mut position = 6_usize;
    let mut parameter_sets = Vec::new();
    let sequence_count = usize::from(configuration[5] & 0x1f);
    for _ in 0..sequence_count {
        append_parameter_set(
            configuration,
            &mut position,
            &mut parameter_sets,
            "parse_avcc",
        )?;
    }
    let picture_count = usize::from(*configuration.get(position).ok_or_else(|| {
        corrupt(
            "parse_avcc",
            "AVC decoder configuration is missing the picture-parameter-set count",
        )
    })?);
    position += 1;
    for _ in 0..picture_count {
        append_parameter_set(
            configuration,
            &mut position,
            &mut parameter_sets,
            "parse_avcc",
        )?;
    }
    Ok(AnnexBConfiguration {
        nal_length_size,
        parameter_sets,
    })
}

fn parse_hvcc(configuration: &[u8]) -> Result<AnnexBConfiguration> {
    if configuration.len() < 23 || configuration[0] != 1 {
        return Err(corrupt(
            "parse_hvcc",
            "HEVC decoder configuration is truncated or has an unsupported version",
        ));
    }
    let nal_length_size = usize::from((configuration[21] & 0x03) + 1);
    let array_count = usize::from(configuration[22]);
    let mut position = 23_usize;
    let mut parameter_sets = Vec::new();
    for _ in 0..array_count {
        position = position
            .checked_add(1)
            .ok_or_else(|| corrupt("parse_hvcc", "HEVC parameter-set array range overflowed"))?;
        let count = read_u16(configuration, &mut position, "parse_hvcc")?;
        for _ in 0..count {
            append_parameter_set(
                configuration,
                &mut position,
                &mut parameter_sets,
                "parse_hvcc",
            )?;
        }
    }
    if position != configuration.len() {
        return Err(corrupt(
            "parse_hvcc",
            "HEVC decoder configuration contains trailing or malformed parameter-set data",
        ));
    }
    Ok(AnnexBConfiguration {
        nal_length_size,
        parameter_sets,
    })
}

fn append_parameter_set(
    configuration: &[u8],
    position: &mut usize,
    output: &mut Vec<u8>,
    operation: &'static str,
) -> Result<()> {
    let length = usize::from(read_u16(configuration, position, operation)?);
    if length == 0 {
        return Err(corrupt(operation, "codec parameter set must not be empty"));
    }
    let end = position
        .checked_add(length)
        .ok_or_else(|| corrupt(operation, "codec parameter-set payload range overflowed"))?;
    let payload = configuration.get(*position..end).ok_or_else(|| {
        corrupt(
            operation,
            "codec configuration ends inside a parameter-set payload",
        )
    })?;
    output.extend_from_slice(&ANNEX_B_START_CODE);
    output.extend_from_slice(payload);
    *position = end;
    Ok(())
}

fn read_u16(data: &[u8], position: &mut usize, operation: &'static str) -> Result<u16> {
    let end = position
        .checked_add(2)
        .ok_or_else(|| corrupt(operation, "codec configuration range overflowed"))?;
    let bytes: [u8; 2] = data
        .get(*position..end)
        .ok_or_else(|| corrupt(operation, "codec configuration is truncated"))?
        .try_into()
        .expect("checked two-byte slice");
    *position = end;
    Ok(u16::from_be_bytes(bytes))
}

fn extract_audio_specific_config(configuration: &[u8]) -> Result<&[u8]> {
    if configuration.is_empty() {
        return Err(corrupt(
            "parse_aac_configuration",
            "AAC codec configuration must not be empty",
        ));
    }
    if configuration.len() >= 6 && configuration[..4] == [0, 0, 0, 0] {
        return find_decoder_specific_info(&configuration[4..])?.ok_or_else(|| {
            corrupt(
                "parse_aac_configuration",
                "MP4 esds metadata contains no AudioSpecificConfig descriptor",
            )
        });
    }
    Ok(configuration)
}

fn find_decoder_specific_info(mut descriptors: &[u8]) -> Result<Option<&[u8]>> {
    while !descriptors.is_empty() {
        let tag = descriptors[0];
        let (length, header_length) = descriptor_length(&descriptors[1..])?;
        let payload_start = 1_usize
            .checked_add(header_length)
            .ok_or_else(|| corrupt("parse_esds", "MPEG-4 descriptor header range overflowed"))?;
        let payload_end = payload_start
            .checked_add(length)
            .ok_or_else(|| corrupt("parse_esds", "MPEG-4 descriptor payload range overflowed"))?;
        let payload = descriptors
            .get(payload_start..payload_end)
            .ok_or_else(|| corrupt("parse_esds", "MPEG-4 descriptor payload is truncated"))?;
        match tag {
            0x05 => return Ok(Some(payload)),
            0x03 => {
                if payload.len() < 3 {
                    return Err(corrupt("parse_esds", "ES descriptor is truncated"));
                }
                let flags = payload[2];
                let mut nested = 3_usize;
                if flags & 0x80 != 0 {
                    nested = nested.saturating_add(2);
                }
                if flags & 0x40 != 0 {
                    let url_length = usize::from(*payload.get(nested).ok_or_else(|| {
                        corrupt("parse_esds", "ES descriptor URL length is missing")
                    })?);
                    nested = nested.saturating_add(1).saturating_add(url_length);
                }
                if flags & 0x20 != 0 {
                    nested = nested.saturating_add(2);
                }
                let nested = payload.get(nested..).ok_or_else(|| {
                    corrupt("parse_esds", "ES descriptor optional data is truncated")
                })?;
                if let Some(found) = find_decoder_specific_info(nested)? {
                    return Ok(Some(found));
                }
            }
            0x04 => {
                let nested = payload.get(13..).ok_or_else(|| {
                    corrupt(
                        "parse_esds",
                        "decoder configuration descriptor is truncated",
                    )
                })?;
                if let Some(found) = find_decoder_specific_info(nested)? {
                    return Ok(Some(found));
                }
            }
            _ => {}
        }
        descriptors = &descriptors[payload_end..];
    }
    Ok(None)
}

fn descriptor_length(data: &[u8]) -> Result<(usize, usize)> {
    let mut length = 0_usize;
    for (index, byte) in data.iter().copied().take(4).enumerate() {
        length = length
            .checked_shl(7)
            .and_then(|value| value.checked_add(usize::from(byte & 0x7f)))
            .ok_or_else(|| corrupt("parse_esds", "MPEG-4 descriptor length overflowed"))?;
        if byte & 0x80 == 0 {
            return Ok((length, index + 1));
        }
    }
    Err(corrupt(
        "parse_esds",
        "MPEG-4 descriptor length is truncated or exceeds four bytes",
    ))
}

struct BitReader<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    fn read(&mut self, count: usize) -> Result<u32> {
        if count > 24 || self.bit.saturating_add(count) > self.data.len().saturating_mul(8) {
            return Err(corrupt(
                "parse_audio_specific_config",
                "AudioSpecificConfig bitstream is truncated",
            ));
        }
        let mut value = 0_u32;
        for _ in 0..count {
            let byte = self.data[self.bit / 8];
            let shift = 7 - (self.bit % 8);
            value = (value << 1) | u32::from((byte >> shift) & 1);
            self.bit += 1;
        }
        Ok(value)
    }

    fn read_audio_object_type(&mut self) -> Result<u8> {
        let initial = self.read(5)? as u8;
        if initial == 31 {
            let extension = self.read(6)? as u8;
            return extension.checked_add(32).ok_or_else(|| {
                corrupt(
                    "parse_audio_specific_config",
                    "AAC audio object type overflowed",
                )
            });
        }
        Ok(initial)
    }

    fn read_sample_rate(&mut self) -> Result<u32> {
        const RATES: [u32; 13] = [
            96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025,
            8_000, 7_350,
        ];
        let index = self.read(4)? as usize;
        if index == 15 {
            let rate = self.read(24)?;
            if rate == 0 {
                return Err(corrupt(
                    "parse_audio_specific_config",
                    "AAC explicit sample rate must be greater than zero",
                ));
            }
            return Ok(rate);
        }
        RATES.get(index).copied().ok_or_else(|| {
            unsupported(
                "parse_audio_specific_config",
                "AAC sample-rate index is reserved",
            )
        })
    }
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
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

#[cfg(target_os = "windows")]
#[path = "media_foundation_windows.rs"]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::MediaFoundationBackend;
