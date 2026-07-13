use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_media_io::backend::{BackendCapability, CodecCapability, CodecOperation};
use superi_media_io::decode::{CpuVideoBuffer, VideoFormat, VideoFrame, VideoPlane};
use superi_media_io::demux::{
    CodecId, MediaMetadata, MetadataValue, Packet, PacketTiming, SeekMode, SeekRequest,
    SourceIdentity, SourceInfo, SourceLocation, SourceRequest, StreamId, StreamInfo, StreamKind,
};
use superi_media_io::read::{CorruptionKind, CorruptionReport, ReadOutcome};

use crate::protocol::{
    ColorSpaceWire, CorruptionWire, DurationWire, FrameWire, MetadataValueWire, MetadataWire,
    PacketTimingWire, PacketWire, PlaneWire, ReadPacketWire, SeekModeWire, SeekWire,
    SourceLocationWire, SourceWire, StreamKindWire, StreamWire, TimeWire, TimebaseWire,
};
use crate::VendorRawFormat;

pub(crate) const SOURCE_HANDLE_METADATA_KEY: &str = "superi.vendor.source_handle";

pub(crate) fn vendor_capabilities(
    formats: &BTreeSet<VendorRawFormat>,
) -> Result<Vec<BackendCapability>> {
    let mut capabilities = vec![BackendCapability::Source];
    for format in formats {
        capabilities.push(BackendCapability::Decode(CodecId::new(format.code())?));
    }
    Ok(capabilities)
}

pub(crate) fn vendor_codec_capabilities(
    formats: &BTreeSet<VendorRawFormat>,
) -> Result<Vec<CodecCapability>> {
    formats
        .iter()
        .map(|format| {
            Ok(CodecCapability::new(
                CodecOperation::Decode,
                CodecId::new(format.code())?,
            ))
        })
        .collect()
}

pub(crate) fn source_location_to_wire(location: &SourceLocation) -> Result<SourceLocationWire> {
    match location {
        SourceLocation::Path(path) => {
            let path = path.to_str().ok_or_else(|| {
                invalid(
                    "encode_source_location",
                    "vendor plugin source path must be valid UTF-8",
                )
            })?;
            Ok(SourceLocationWire::Path {
                path: path.to_owned(),
            })
        }
        SourceLocation::Memory { name, data } => Ok(SourceLocationWire::Memory {
            name: name.clone(),
            data_hex: encode_hex(data),
        }),
        _ => Err(unsupported(
            "encode_source_location",
            "source location is not supported by vendor protocol revision 1",
        )),
    }
}

pub(crate) fn source_from_wire(
    request: &SourceRequest,
    source: SourceWire,
    formats: &BTreeSet<VendorRawFormat>,
) -> Result<(SourceInfo, String)> {
    require_nonempty("open_source", "source_handle", &source.source_handle)?;
    require_nonempty("open_source", "fingerprint", &source.fingerprint)?;
    if let Some(expected) = request.expected_fingerprint() {
        if expected != source.fingerprint {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "relinked vendor media does not match the expected source fingerprint",
            )
            .with_context(
                ErrorContext::new("superi-codecs-vendor.convert", "open_source")
                    .with_field("expected_fingerprint", expected)
                    .with_field("actual_fingerprint", source.fingerprint),
            ));
        }
    }

    let mut found_vendor_video = false;
    let mut streams = Vec::with_capacity(source.streams.len());
    for stream in source.streams {
        let mut converted = stream_from_wire(stream, formats, &mut found_vendor_video)?;
        if VendorRawFormat::from_code(converted.codec().as_str()).is_some() {
            if converted
                .metadata()
                .get(SOURCE_HANDLE_METADATA_KEY)
                .is_some()
            {
                return Err(protocol(
                    "open_source",
                    "vendor plugin returned host-reserved stream metadata",
                ));
            }
            converted = converted.with_metadata(
                SOURCE_HANDLE_METADATA_KEY,
                MetadataValue::Text(source.source_handle.clone()),
            )?;
        }
        streams.push(converted);
    }
    if !found_vendor_video {
        return Err(protocol(
            "open_source",
            "vendor plugin source contains no declared vendor RAW video stream",
        ));
    }

    let identity = SourceIdentity::new(request.media_id(), source.fingerprint)?;
    let mut info = SourceInfo::new(identity, streams)?;
    if let Some(duration) = source.duration {
        info = info.with_duration(duration_from_wire(duration)?);
    }
    for (key, value) in metadata_from_wire(source.metadata)? {
        info = info.with_metadata(key, value)?;
    }
    Ok((info, source.source_handle))
}

fn stream_from_wire(
    stream: StreamWire,
    formats: &BTreeSet<VendorRawFormat>,
    found_vendor_video: &mut bool,
) -> Result<StreamInfo> {
    let kind = match stream.kind {
        StreamKindWire::Video => StreamKind::Video,
        StreamKindWire::Audio => StreamKind::Audio,
        StreamKindWire::Subtitle => StreamKind::Subtitle,
        StreamKindWire::Data => StreamKind::Data,
    };
    let codec = CodecId::new(stream.codec)?;
    if let Some(format) = VendorRawFormat::from_code(codec.as_str()) {
        if kind != StreamKind::Video || !formats.contains(&format) {
            return Err(protocol(
                "open_source",
                "vendor plugin returned an undeclared RAW stream",
            ));
        }
        *found_vendor_video = true;
    } else if kind == StreamKind::Video {
        return Err(protocol(
            "open_source",
            "vendor plugin returned an unsupported video codec",
        ));
    }
    let timebase = timebase_from_wire(stream.timebase)?;
    let mut info = StreamInfo::new(StreamId::new(stream.id), kind, codec, timebase);
    if let Some(duration) = stream.duration {
        info = info.with_duration(Duration::new(duration, timebase)?)?;
    }
    for (key, value) in metadata_from_wire(stream.metadata)? {
        info = info.with_metadata(key, value)?;
    }
    Ok(info)
}

pub(crate) fn stream_to_wire(stream: &StreamInfo) -> Result<StreamWire> {
    let kind = match stream.kind() {
        StreamKind::Video => StreamKindWire::Video,
        StreamKind::Audio => StreamKindWire::Audio,
        StreamKind::Subtitle => StreamKindWire::Subtitle,
        StreamKind::Data => StreamKindWire::Data,
        _ => {
            return Err(unsupported(
                "encode_stream",
                "stream kind is not supported by vendor protocol revision 1",
            ))
        }
    };
    Ok(StreamWire {
        id: stream.id().value(),
        kind,
        codec: stream.codec().as_str().to_owned(),
        timebase: timebase_to_wire(stream.timebase()),
        duration: stream.duration().map(Duration::value),
        metadata: metadata_to_wire(stream.metadata())?,
    })
}

pub(crate) fn packet_from_wire(packet: PacketWire) -> Result<Packet> {
    let timing = packet_timing_from_wire(packet.timing)?;
    let mut output = Packet::new(
        StreamId::new(packet.stream_id),
        Arc::from(decode_hex(&packet.data_hex, "decode_packet")?),
        timing,
    )
    .with_keyframe(packet.keyframe);
    for (key, value) in metadata_from_wire(packet.metadata)? {
        output = output.with_metadata(key, value)?;
    }
    Ok(output)
}

pub(crate) fn packet_to_wire(packet: &Packet) -> Result<PacketWire> {
    let timing = packet.timing();
    Ok(PacketWire {
        stream_id: packet.stream_id().value(),
        data_hex: encode_hex(packet.data()),
        timing: PacketTimingWire {
            timebase: timebase_to_wire(timing.timebase()),
            presentation: timing.presentation_time().map(RationalTime::value),
            decode: timing.decode_time().map(RationalTime::value),
            duration: timing.duration().map(Duration::value),
        },
        keyframe: packet.is_keyframe(),
        metadata: metadata_to_wire(packet.metadata())?,
    })
}

fn packet_timing_from_wire(timing: PacketTimingWire) -> Result<PacketTiming> {
    PacketTiming::new(
        timebase_from_wire(timing.timebase)?,
        timing.presentation,
        timing.decode,
        timing.duration,
    )
}

pub(crate) fn read_outcome_from_wire(outcome: ReadPacketWire) -> Result<ReadOutcome<Packet>> {
    match outcome {
        ReadPacketWire::Complete { packet } => Ok(ReadOutcome::Complete(packet_from_wire(packet)?)),
        ReadPacketWire::Partial { packet, report } => Ok(ReadOutcome::Partial {
            value: packet_from_wire(packet)?,
            report: corruption_from_wire(report)?,
        }),
        ReadPacketWire::EndOfStream => Ok(ReadOutcome::EndOfStream),
    }
}

fn corruption_from_wire(report: CorruptionWire) -> Result<CorruptionReport> {
    let kind = match report.kind.as_str() {
        "truncated" => CorruptionKind::Truncated,
        "malformed" => CorruptionKind::Malformed,
        "checksum_mismatch" => CorruptionKind::ChecksumMismatch,
        "inconsistent_metadata" => CorruptionKind::InconsistentMetadata,
        _ => {
            return Err(protocol(
                "decode_corruption",
                "vendor plugin returned an unknown corruption kind",
            ))
        }
    };
    let recoverability = Recoverability::from_code(&report.recoverability).ok_or_else(|| {
        protocol(
            "decode_corruption",
            "vendor plugin returned an unknown corruption recoverability",
        )
    })?;
    let mut output = CorruptionReport::new(kind, recoverability);
    if let Some(stream_id) = report.stream_id {
        output = output.with_stream(StreamId::new(stream_id));
    }
    match (
        report.byte_offset,
        report.expected_bytes,
        report.actual_bytes,
    ) {
        (Some(offset), Some(expected), Some(actual)) => {
            output = output.with_byte_progress(offset, expected, actual)?;
        }
        (None, None, None) => {}
        _ => {
            return Err(protocol(
                "decode_corruption",
                "vendor plugin returned incomplete corruption byte progress",
            ))
        }
    }
    Ok(output)
}

pub(crate) fn frame_from_wire(frame: FrameWire) -> Result<VideoFrame> {
    let pixel_format = PixelFormat::from_code(&frame.pixel_format).ok_or_else(|| {
        unsupported(
            "decode_frame",
            "vendor plugin returned an unsupported pixel format",
        )
    })?;
    let color_space = color_space_from_wire(frame.color_space)?;
    let alpha_mode = AlphaMode::from_code(&frame.alpha_mode).ok_or_else(|| {
        protocol(
            "decode_frame",
            "vendor plugin returned an unknown alpha mode",
        )
    })?;
    let timestamp = time_from_wire(frame.timestamp)?;
    let duration = duration_from_wire(frame.duration)?;
    let planes = frame
        .planes
        .into_iter()
        .map(plane_from_wire)
        .collect::<Result<Vec<_>>>()?;
    let buffer = Arc::new(CpuVideoBuffer::new(
        frame.width,
        frame.height,
        pixel_format,
        planes,
    )?);
    let format = VideoFormat::new(
        frame.width,
        frame.height,
        pixel_format,
        color_space,
        alpha_mode,
    )?;
    let mut output = VideoFrame::new(format, timestamp, duration, buffer)?;
    for (key, value) in metadata_from_wire(frame.metadata)? {
        output = output.with_metadata(key, value)?;
    }
    Ok(output)
}

fn plane_from_wire(plane: PlaneWire) -> Result<VideoPlane> {
    VideoPlane::new(
        Arc::from(decode_hex(&plane.data_hex, "decode_frame_plane")?),
        plane.stride,
        plane.row_count,
    )
}

fn color_space_from_wire(color: ColorSpaceWire) -> Result<ColorSpace> {
    let primaries = ColorPrimaries::from_code(&color.primaries).ok_or_else(|| {
        protocol(
            "decode_color_space",
            "vendor plugin returned unknown color primaries",
        )
    })?;
    let transfer = TransferFunction::from_code(&color.transfer).ok_or_else(|| {
        protocol(
            "decode_color_space",
            "vendor plugin returned an unknown transfer function",
        )
    })?;
    let matrix = MatrixCoefficients::from_code(&color.matrix).ok_or_else(|| {
        protocol(
            "decode_color_space",
            "vendor plugin returned unknown matrix coefficients",
        )
    })?;
    let range = ColorRange::from_code(&color.range).ok_or_else(|| {
        protocol(
            "decode_color_space",
            "vendor plugin returned an unknown color range",
        )
    })?;
    Ok(ColorSpace::new(primaries, transfer, matrix, range))
}

pub(crate) fn seek_to_wire(request: SeekRequest) -> Result<SeekWire> {
    Ok(SeekWire {
        target: time_to_wire(request.target()),
        mode: match request.mode() {
            SeekMode::Exact => SeekModeWire::Exact,
            SeekMode::PreviousKeyframe => SeekModeWire::PreviousKeyframe,
            SeekMode::NearestKeyframe => SeekModeWire::NearestKeyframe,
            _ => {
                return Err(unsupported(
                    "encode_seek",
                    "seek mode is not supported by vendor protocol revision 1",
                ))
            }
        },
    })
}

pub(crate) fn time_from_wire(time: TimeWire) -> Result<RationalTime> {
    Ok(RationalTime::new(
        time.value,
        timebase_from_wire(time.timebase)?,
    ))
}

fn time_to_wire(time: RationalTime) -> TimeWire {
    TimeWire {
        value: time.value(),
        timebase: timebase_to_wire(time.timebase()),
    }
}

fn duration_from_wire(duration: DurationWire) -> Result<Duration> {
    Duration::new(duration.value, timebase_from_wire(duration.timebase)?)
}

fn timebase_from_wire(timebase: TimebaseWire) -> Result<Timebase> {
    Timebase::new(timebase.numerator, timebase.denominator)
}

fn timebase_to_wire(timebase: Timebase) -> TimebaseWire {
    TimebaseWire {
        numerator: timebase.numerator(),
        denominator: timebase.denominator(),
    }
}

fn metadata_from_wire(metadata: MetadataWire) -> Result<BTreeMap<String, MetadataValue>> {
    metadata
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                MetadataValueWire::Text(value) => MetadataValue::Text(value),
                MetadataValueWire::Signed(value) => MetadataValue::Signed(value),
                MetadataValueWire::Unsigned(value) => MetadataValue::Unsigned(value),
                MetadataValueWire::Boolean(value) => MetadataValue::Boolean(value),
                MetadataValueWire::Bytes(value) => {
                    MetadataValue::Bytes(Arc::from(decode_hex(&value, "decode_metadata")?))
                }
            };
            Ok((key, value))
        })
        .collect()
}

fn metadata_to_wire(metadata: &MediaMetadata) -> Result<MetadataWire> {
    metadata
        .iter()
        .map(|(key, value)| {
            let value = match value {
                MetadataValue::Text(value) => MetadataValueWire::Text(value.clone()),
                MetadataValue::Signed(value) => MetadataValueWire::Signed(*value),
                MetadataValue::Unsigned(value) => MetadataValueWire::Unsigned(*value),
                MetadataValue::Boolean(value) => MetadataValueWire::Boolean(*value),
                MetadataValue::Bytes(value) => MetadataValueWire::Bytes(encode_hex(value)),
                _ => {
                    return Err(unsupported(
                        "encode_metadata",
                        "metadata value is not supported by vendor protocol revision 1",
                    ))
                }
            };
            Ok((key.to_owned(), value))
        })
        .collect()
}

pub(crate) fn media_id_text(media_id: MediaId) -> String {
    media_id.to_string()
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn decode_hex(value: &str, operation: &'static str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0 {
        return Err(protocol(
            operation,
            "vendor plugin returned hexadecimal with an odd length",
        ));
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_nibble(pair[0]).ok_or_else(|| {
            protocol(
                operation,
                "vendor plugin returned non-lowercase hexadecimal",
            )
        })?;
        let low = hex_nibble(pair[1]).ok_or_else(|| {
            protocol(
                operation,
                "vendor plugin returned non-lowercase hexadecimal",
            )
        })?;
        output.push((high << 4) | low);
    }
    Ok(output)
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn require_nonempty(operation: &'static str, field: &'static str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(
            protocol(operation, "vendor plugin returned an empty required field").with_context(
                ErrorContext::new("superi-codecs-vendor.convert", operation)
                    .with_field("field", field),
            ),
        );
    }
    Ok(())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-vendor.convert", operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-vendor.convert", operation))
}

pub(crate) fn protocol(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Terminal,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-vendor.convert", operation))
}
