//! Revisioned wire contract implemented by separately installed vendor RAW workers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::VendorRawFormat;

/// The only wire revision accepted by this release.
pub const PROTOCOL_REVISION: u32 = 1;

/// One request or response with a caller-assigned correlation identifier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    /// Monotonic identifier chosen by the host.
    pub id: u64,
    /// Revisioned request or response payload.
    pub payload: T,
}

/// Immutable capability declaration returned before any media operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    /// Wire revision implemented by the worker.
    pub protocol_revision: u32,
    /// Stable lowercase backend identity.
    pub backend_id: String,
    /// Diagnostic name shown to users.
    pub display_name: String,
    /// Worker adapter version.
    pub plugin_version: String,
    /// Vendor SDK version loaded by the separately installed worker.
    pub sdk_version: String,
    /// Vendor RAW families this worker can open and decode.
    pub formats: Vec<VendorRawFormat>,
}

/// A host request. Each request is encoded as one newline-terminated JSON object.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProtocolRequest {
    /// Negotiates the wire revision and immutable manifest.
    Handshake {
        /// Wire revision required by the host.
        protocol_revision: u32,
    },
    /// Inspects a bounded prefix without trusting the file extension.
    Probe {
        /// Optional diagnostic file name.
        source_name: Option<String>,
        /// Optional untrusted extension hint.
        extension: Option<String>,
        /// Complete source byte length observed by the host.
        source_length: u64,
        /// Whether `prefix_hex` contains the whole source.
        complete: bool,
        /// Bounded source prefix in lowercase hexadecimal.
        prefix_hex: String,
    },
    /// Opens a path or memory source.
    Open {
        /// Canonical Superi media identity.
        media_id: String,
        /// Explicit source location.
        location: SourceLocationWire,
        /// Prior content fingerprint required during relinking.
        expected_fingerprint: Option<String>,
    },
    /// Reads the next packet from an open source.
    ReadPacket {
        /// Opaque source handle created by the worker.
        source_handle: String,
    },
    /// Seeks an open source on its presentation timeline.
    Seek {
        /// Opaque source handle created by the worker.
        source_handle: String,
        /// Exact target and resolution policy.
        request: SeekWire,
    },
    /// Releases an open source handle.
    CloseSource {
        /// Opaque source handle created by the worker.
        source_handle: String,
    },
    /// Creates a decoder for one stream of an open source.
    CreateDecoder {
        /// Opaque source handle inserted by the host into stream state.
        source_handle: String,
        /// Exact stream configuration.
        stream: StreamWire,
    },
    /// Supplies one compressed vendor packet to a decoder.
    SendPacket {
        /// Opaque decoder handle created by the worker.
        decoder_handle: String,
        /// Complete packet and timing state.
        packet: PacketWire,
    },
    /// Receives one decoded frame or lifecycle state.
    ReceiveDecoder {
        /// Opaque decoder handle created by the worker.
        decoder_handle: String,
    },
    /// Signals end of input while retaining buffered output.
    FlushDecoder {
        /// Opaque decoder handle created by the worker.
        decoder_handle: String,
    },
    /// Clears decoder state after a seek.
    ResetDecoder {
        /// Opaque decoder handle created by the worker.
        decoder_handle: String,
    },
    /// Releases a decoder handle.
    CloseDecoder {
        /// Opaque decoder handle created by the worker.
        decoder_handle: String,
    },
}

/// A worker response. The response identifier must equal the request identifier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProtocolResponse {
    /// Successful handshake and immutable worker manifest.
    Handshake {
        /// Validated capability declaration.
        manifest: PluginManifest,
    },
    /// Bounded source probe result.
    Probe {
        /// Worker recognition result.
        result: ProbeResultWire,
    },
    /// Successfully opened source.
    Open {
        /// Opaque handle plus stable source description.
        source: SourceWire,
    },
    /// Packet read result.
    ReadPacket {
        /// Complete, partial, or end-of-source result.
        outcome: ReadPacketWire,
    },
    /// Exact seek result selected by the worker.
    Seek {
        /// Selected presentation coordinate.
        selected: TimeWire,
    },
    /// Successfully created decoder.
    DecoderCreated {
        /// Opaque decoder handle.
        decoder_handle: String,
    },
    /// Decoder receive result.
    DecoderOutput {
        /// Frame or lifecycle state.
        output: DecoderOutputWire,
    },
    /// Successful state mutation or close.
    Ack,
    /// Classified worker failure.
    Failure {
        /// Stable error data safe for the host to classify.
        error: ErrorWire,
    },
}

/// A local source location sent only to the explicitly selected worker.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceLocationWire {
    /// Local filesystem path represented as UTF-8.
    Path {
        /// Complete path selected by the caller.
        path: String,
    },
    /// Immutable in-memory source.
    Memory {
        /// Diagnostic source name.
        name: String,
        /// Complete bytes in lowercase hexadecimal.
        data_hex: String,
    },
}

/// Bounded content-probe decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProbeResultWire {
    /// Prefix does not match any declared format.
    NoMatch,
    /// Worker needs a larger bounded prefix.
    NeedMoreData {
        /// Required prefix length.
        minimum_bytes: usize,
    },
    /// Worker recognized one declared format.
    Match {
        /// Recognized vendor RAW family.
        format: VendorRawFormat,
        /// Confidence from 1 through 100.
        confidence: u8,
    },
}

/// One opened source and its immutable description.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceWire {
    /// Opaque worker-owned source handle.
    pub source_handle: String,
    /// Backend-defined stable content fingerprint.
    pub fingerprint: String,
    /// Streams in deterministic source order.
    pub streams: Vec<StreamWire>,
    /// Optional total source duration.
    pub duration: Option<DurationWire>,
    /// Lossless source metadata.
    pub metadata: MetadataWire,
}

/// One codec-neutral stream description.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StreamWire {
    /// Source-local stream number.
    pub id: u32,
    /// Semantic stream kind.
    pub kind: StreamKindWire,
    /// Stable codec identity.
    pub codec: String,
    /// Exact timestamp timebase.
    pub timebase: TimebaseWire,
    /// Optional stream duration in `timebase` units.
    pub duration: Option<u64>,
    /// Lossless stream metadata.
    pub metadata: MetadataWire,
}

/// Stream semantic kind.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKindWire {
    /// Video pictures.
    Video,
    /// Audio samples.
    Audio,
    /// Timed text.
    Subtitle,
    /// Other data.
    Data,
}

/// Exact positive rational units per second.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimebaseWire {
    /// Units-per-second numerator.
    pub numerator: u32,
    /// Units-per-second denominator.
    pub denominator: u32,
}

/// Signed coordinate in an explicit timebase.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimeWire {
    /// Signed coordinate.
    pub value: i64,
    /// Coordinate timebase.
    pub timebase: TimebaseWire,
}

/// Nonnegative duration in an explicit timebase.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DurationWire {
    /// Nonnegative coordinate.
    pub value: u64,
    /// Duration timebase.
    pub timebase: TimebaseWire,
}

/// Exact seek request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SeekWire {
    /// Requested presentation coordinate.
    pub target: TimeWire,
    /// Resolution policy.
    pub mode: SeekModeWire,
}

/// Seek resolution policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeekModeWire {
    /// Exact presented frame boundary.
    Exact,
    /// Prior random-access packet.
    PreviousKeyframe,
    /// Nearest random-access packet.
    NearestKeyframe,
}

/// One packet with exact timing and metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PacketWire {
    /// Source-local stream number.
    pub stream_id: u32,
    /// Complete compressed bytes in lowercase hexadecimal.
    pub data_hex: String,
    /// Packet timing.
    pub timing: PacketTimingWire,
    /// Whether the packet is independently decodable.
    pub keyframe: bool,
    /// Lossless packet metadata.
    pub metadata: MetadataWire,
}

/// Exact compressed packet timestamps.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PacketTimingWire {
    /// Timestamp timebase.
    pub timebase: TimebaseWire,
    /// Optional presentation timestamp.
    pub presentation: Option<i64>,
    /// Optional decode timestamp.
    pub decode: Option<i64>,
    /// Optional packet duration.
    pub duration: Option<u64>,
}

/// Complete, partial, or exhausted packet result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadPacketWire {
    /// Complete packet.
    Complete {
        /// Packet data and semantics.
        packet: PacketWire,
    },
    /// Usable partial packet with corruption evidence.
    Partial {
        /// Packet data and semantics.
        packet: PacketWire,
        /// Structured corruption report.
        report: CorruptionWire,
    },
    /// No packet remains.
    EndOfStream,
}

/// Structured corruption evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorruptionWire {
    /// Stable corruption kind.
    pub kind: String,
    /// Stable recoverability code.
    pub recoverability: String,
    /// Optional source-local stream number.
    pub stream_id: Option<u32>,
    /// Optional starting byte offset.
    pub byte_offset: Option<u64>,
    /// Optional expected byte count.
    pub expected_bytes: Option<usize>,
    /// Optional actual byte count.
    pub actual_bytes: Option<usize>,
}

/// One decoded CPU frame.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrameWire {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Stable `superi-core` pixel format code.
    pub pixel_format: String,
    /// Complete color interpretation.
    pub color_space: ColorSpaceWire,
    /// Stable `superi-core` alpha mode code.
    pub alpha_mode: String,
    /// Exact presentation coordinate.
    pub timestamp: TimeWire,
    /// Exact frame duration.
    pub duration: DurationWire,
    /// CPU planes in canonical component order.
    pub planes: Vec<PlaneWire>,
    /// Lossless decoded-frame metadata.
    pub metadata: MetadataWire,
}

/// One immutable CPU image plane.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneWire {
    /// Complete plane bytes in lowercase hexadecimal.
    pub data_hex: String,
    /// Bytes between adjacent rows.
    pub stride: usize,
    /// Stored row count.
    pub row_count: u32,
}

/// Complete color-space tags.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ColorSpaceWire {
    /// Stable color primaries code.
    pub primaries: String,
    /// Stable transfer function code.
    pub transfer: String,
    /// Stable matrix coefficient code.
    pub matrix: String,
    /// Stable color range code.
    pub range: String,
}

/// Decoder receive result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecoderOutputWire {
    /// Decoded video frame.
    Frame {
        /// Complete frame data and semantics.
        frame: Box<FrameWire>,
    },
    /// Decoder needs another compressed packet.
    NeedInput,
    /// All output has been drained.
    EndOfStream,
}

/// Deterministically ordered lossless metadata.
pub type MetadataWire = BTreeMap<String, MetadataValueWire>;

/// One lossless metadata value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum MetadataValueWire {
    /// UTF-8 text.
    Text(String),
    /// Signed integer.
    Signed(i64),
    /// Unsigned integer.
    Unsigned(u64),
    /// Boolean.
    Boolean(bool),
    /// Uninterpreted bytes in lowercase hexadecimal.
    Bytes(String),
}

/// Stable classified failure returned by a worker.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorWire {
    /// Stable `superi-core` error category code.
    pub category: String,
    /// Stable `superi-core` recoverability code.
    pub recoverability: String,
    /// Concise diagnostic summary.
    pub message: String,
}
