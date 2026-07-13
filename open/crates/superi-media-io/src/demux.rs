//! Codec-neutral compressed media and source contracts.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::MediaId;
use superi_core::time::{Duration, RationalTime, Timebase};

use crate::operation::OperationContext;
use crate::read::ReadOutcome;

macro_rules! string_id {
    ($name:ident, $component:literal) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Creates a stable lowercase identifier.
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                if !valid_name(&value) {
                    return Err(invalid(
                        concat!("create_", $component),
                        concat!($component, " must use lowercase ASCII letters, digits, dots, underscores, or hyphens"),
                    ));
                }
                Ok(Self(value))
            }

            /// Returns the stable identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

string_id!(BackendId, "backend_id");
string_id!(CodecId, "codec_id");
string_id!(ContainerId, "container_id");

/// A stable stream number within one media source.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StreamId(u32);

impl StreamId {
    /// Creates a source-local stream identifier.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the source-local numeric value.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// One lossless metadata value attached to a source, stream, packet, or frame.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MetadataValue {
    /// UTF-8 text.
    Text(String),
    /// Signed integer data.
    Signed(i64),
    /// Unsigned integer data.
    Unsigned(u64),
    /// Boolean data.
    Boolean(bool),
    /// Uninterpreted binary data.
    Bytes(Arc<[u8]>),
}

/// Deterministically ordered metadata with validated namespaced keys.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MediaMetadata(BTreeMap<String, MetadataValue>);

impl MediaMetadata {
    /// Creates an empty metadata collection.
    #[must_use]
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Inserts a value and returns the prior value for the key.
    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: MetadataValue,
    ) -> Result<Option<MetadataValue>> {
        let key = key.into();
        if !valid_name(&key) {
            return Err(invalid(
                "insert_metadata",
                "metadata keys must use lowercase ASCII letters, digits, dots, underscores, or hyphens",
            ));
        }
        Ok(self.0.insert(key, value))
    }

    /// Returns a value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&MetadataValue> {
        self.0.get(key)
    }

    /// Iterates in deterministic key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &MetadataValue)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }

    /// Returns true when no metadata is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The semantic media carried by one stream.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum StreamKind {
    /// Compressed or uncompressed video pictures.
    Video,
    /// Compressed or uncompressed audio samples.
    Audio,
    /// Timed subtitle or caption data.
    Subtitle,
    /// Timed or untimed data not covered by another kind.
    Data,
}

/// Codec-neutral information required to route packets from one stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamInfo {
    id: StreamId,
    kind: StreamKind,
    codec: CodecId,
    timebase: Timebase,
    metadata: MediaMetadata,
}

impl StreamInfo {
    /// Creates a stream descriptor.
    #[must_use]
    pub fn new(id: StreamId, kind: StreamKind, codec: CodecId, timebase: Timebase) -> Self {
        Self {
            id,
            kind,
            codec,
            timebase,
            metadata: MediaMetadata::new(),
        }
    }

    /// Returns the source-local stream identifier.
    #[must_use]
    pub const fn id(&self) -> StreamId {
        self.id
    }

    /// Returns the stream kind.
    #[must_use]
    pub const fn kind(&self) -> StreamKind {
        self.kind
    }

    /// Returns the codec identifier.
    #[must_use]
    pub fn codec(&self) -> &CodecId {
        &self.codec
    }

    /// Returns the exact timestamp timebase.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns preserved stream metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }

    /// Adds preserved stream metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: MetadataValue) -> Result<Self> {
        self.metadata.insert(key, value)?;
        Ok(self)
    }
}

/// Exact compressed-packet timestamps in one stream timebase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PacketTiming {
    timebase: Timebase,
    presentation: Option<i64>,
    decode: Option<i64>,
    duration: Option<u64>,
}

impl PacketTiming {
    /// Creates packet timing. Missing container timestamps remain absent.
    pub fn new(
        timebase: Timebase,
        presentation: Option<i64>,
        decode: Option<i64>,
        duration: Option<u64>,
    ) -> Result<Self> {
        if let Some(value) = duration {
            Duration::new(value, timebase)?;
        }
        Ok(Self {
            timebase,
            presentation,
            decode,
            duration,
        })
    }

    /// Returns the timestamp timebase.
    #[must_use]
    pub const fn timebase(self) -> Timebase {
        self.timebase
    }

    /// Returns the presentation timestamp when stored by the source.
    #[must_use]
    pub fn presentation_time(self) -> Option<RationalTime> {
        self.presentation
            .map(|value| RationalTime::new(value, self.timebase))
    }

    /// Returns the decode timestamp when stored by the source.
    #[must_use]
    pub fn decode_time(self) -> Option<RationalTime> {
        self.decode
            .map(|value| RationalTime::new(value, self.timebase))
    }

    /// Returns the packet duration when known.
    #[must_use]
    pub fn duration(self) -> Option<Duration> {
        self.duration
            .map(|value| Duration::new(value, self.timebase).expect("validated packet duration"))
    }
}

/// One immutable compressed packet and all container-provided side information.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Packet {
    stream_id: StreamId,
    data: Arc<[u8]>,
    timing: PacketTiming,
    keyframe: bool,
    metadata: MediaMetadata,
}

impl Packet {
    /// Creates a compressed packet.
    #[must_use]
    pub fn new(stream_id: StreamId, data: Arc<[u8]>, timing: PacketTiming) -> Self {
        Self {
            stream_id,
            data,
            timing,
            keyframe: false,
            metadata: MediaMetadata::new(),
        }
    }

    /// Marks whether this packet is independently decodable.
    #[must_use]
    pub fn with_keyframe(mut self, keyframe: bool) -> Self {
        self.keyframe = keyframe;
        self
    }

    /// Adds packet side metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: MetadataValue) -> Result<Self> {
        self.metadata.insert(key, value)?;
        Ok(self)
    }

    /// Returns the source-local stream identifier.
    #[must_use]
    pub const fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// Returns compressed bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns exact packet timing.
    #[must_use]
    pub const fn timing(&self) -> PacketTiming {
        self.timing
    }

    /// Returns true for an independently decodable packet.
    #[must_use]
    pub const fn is_keyframe(&self) -> bool {
        self.keyframe
    }

    /// Returns packet side metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }

    /// Returns mutable packet side metadata for demuxers that discover it incrementally.
    #[must_use]
    pub fn metadata_mut(&mut self) -> &mut MediaMetadata {
        &mut self.metadata
    }
}

/// Stable project identity plus a content identity used to detect relink mismatches.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceIdentity {
    media_id: MediaId,
    fingerprint: String,
}

impl SourceIdentity {
    /// Creates a source identity with a nonempty backend-defined fingerprint.
    pub fn new(media_id: MediaId, fingerprint: impl Into<String>) -> Result<Self> {
        let fingerprint = fingerprint.into();
        if fingerprint.trim().is_empty() {
            return Err(invalid(
                "create_source_identity",
                "source fingerprint must not be empty",
            ));
        }
        Ok(Self {
            media_id,
            fingerprint,
        })
    }

    /// Returns the persistent project media identifier.
    #[must_use]
    pub const fn media_id(&self) -> MediaId {
        self.media_id
    }

    /// Returns the content fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

/// Opened-source information shared across ingest, playback, and relinking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceInfo {
    identity: SourceIdentity,
    streams: Vec<StreamInfo>,
    duration: Option<Duration>,
    metadata: MediaMetadata,
}

impl SourceInfo {
    /// Creates source information with at least one uniquely identified stream.
    pub fn new(identity: SourceIdentity, streams: Vec<StreamInfo>) -> Result<Self> {
        if streams.is_empty() {
            return Err(invalid(
                "create_source_info",
                "media source must contain at least one stream",
            ));
        }
        let mut ids = BTreeSet::new();
        if streams.iter().any(|stream| !ids.insert(stream.id())) {
            return Err(invalid(
                "create_source_info",
                "media source contains duplicate stream identifiers",
            ));
        }
        Ok(Self {
            identity,
            streams,
            duration: None,
            metadata: MediaMetadata::new(),
        })
    }

    /// Adds a source duration.
    #[must_use]
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Adds preserved source metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: MetadataValue) -> Result<Self> {
        self.metadata.insert(key, value)?;
        Ok(self)
    }

    /// Returns stable source identity.
    #[must_use]
    pub const fn identity(&self) -> &SourceIdentity {
        &self.identity
    }

    /// Returns streams in deterministic source order.
    #[must_use]
    pub fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    /// Returns total source duration when known.
    #[must_use]
    pub const fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// Returns preserved source metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }
}

/// A local source location. The project identity is stored separately for relinking.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SourceLocation {
    /// A local filesystem path.
    Path(PathBuf),
    /// Immutable caller-owned bytes and a diagnostic name.
    Memory { name: String, data: Arc<[u8]> },
}

impl SourceLocation {
    /// Returns the caller-provided file name when it is valid UTF-8.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.hint_path().file_name().and_then(|name| name.to_str())
    }

    /// Returns the extension hint when it is valid UTF-8.
    ///
    /// An extension is untrusted context. A backend must inspect bytes before it reports a match.
    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        self.hint_path()
            .extension()
            .and_then(|extension| extension.to_str())
    }

    fn hint_path(&self) -> &Path {
        match self {
            Self::Path(path) => path,
            Self::Memory { name, .. } => Path::new(name),
        }
    }
}

/// Validated confidence assigned by a backend to a content-based source match.
///
/// Values range from 1 through 100. Zero is represented by [`SourceProbeResult::NoMatch`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProbeConfidence(u8);

impl ProbeConfidence {
    /// Creates a nonzero confidence percentage.
    pub fn new(value: u8) -> Result<Self> {
        if !(1..=100).contains(&value) {
            return Err(invalid(
                "create_probe_confidence",
                "probe confidence must be between 1 and 100",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the confidence percentage.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// Bounded byte counts used while probing one source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceProbeLimits {
    initial_bytes: usize,
    maximum_bytes: usize,
}

impl SourceProbeLimits {
    /// Default initial prefix length, 4 KiB.
    pub const DEFAULT_INITIAL_BYTES: usize = 4 * 1024;
    /// Default maximum prefix length, 1 MiB.
    pub const DEFAULT_MAXIMUM_BYTES: usize = 1024 * 1024;

    /// Creates limits with a positive initial length no larger than the maximum.
    pub fn new(initial_bytes: usize, maximum_bytes: usize) -> Result<Self> {
        if initial_bytes == 0 {
            return Err(invalid(
                "create_source_probe_limits",
                "initial probe bytes must be greater than zero",
            ));
        }
        if initial_bytes > maximum_bytes {
            return Err(invalid(
                "create_source_probe_limits",
                "initial probe bytes must not exceed maximum probe bytes",
            ));
        }
        Ok(Self {
            initial_bytes,
            maximum_bytes,
        })
    }

    /// Returns the first prefix length shown to source backends.
    #[must_use]
    pub const fn initial_bytes(self) -> usize {
        self.initial_bytes
    }

    /// Returns the hard per-source byte limit.
    #[must_use]
    pub const fn maximum_bytes(self) -> usize {
        self.maximum_bytes
    }
}

impl Default for SourceProbeLimits {
    fn default() -> Self {
        Self {
            initial_bytes: Self::DEFAULT_INITIAL_BYTES,
            maximum_bytes: Self::DEFAULT_MAXIMUM_BYTES,
        }
    }
}

/// One immutable, bounded view presented to every eligible source backend.
pub struct SourceProbe<'a> {
    location: &'a SourceLocation,
    bytes: &'a [u8],
    source_length: u64,
    complete: bool,
}

impl<'a> SourceProbe<'a> {
    pub(crate) const fn new(
        location: &'a SourceLocation,
        bytes: &'a [u8],
        source_length: u64,
        complete: bool,
    ) -> Self {
        Self {
            location,
            bytes,
            source_length,
            complete,
        }
    }

    /// Returns the source prefix starting at byte zero.
    #[must_use]
    pub const fn bytes(&self) -> &[u8] {
        self.bytes
    }

    /// Returns the complete source length observed before probing.
    #[must_use]
    pub const fn source_length(&self) -> u64 {
        self.source_length
    }

    /// Returns true when the prefix contains the complete source.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    /// Returns the caller-provided file name hint when it is valid UTF-8.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.location.name()
    }

    /// Returns the caller-provided extension hint when it is valid UTF-8.
    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        self.location.extension()
    }
}

/// One backend response to a bounded source prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SourceProbeResult {
    /// The bytes do not match a format supported by this backend.
    NoMatch,
    /// The backend needs at least this many prefix bytes for a decision.
    NeedMoreData { minimum_bytes: NonZeroUsize },
    /// The backend recognized a container with explicit confidence.
    Match {
        /// Stable container or elementary-stream identity.
        container: ContainerId,
        /// Content-based detection confidence.
        confidence: ProbeConfidence,
    },
}

impl SourceProbeResult {
    /// Requests a larger prefix.
    pub fn need_more_data(minimum_bytes: usize) -> Result<Self> {
        if minimum_bytes == 0 {
            return Err(invalid(
                "request_more_probe_data",
                "requested probe bytes must be greater than zero",
            ));
        }
        Ok(Self::NeedMoreData {
            minimum_bytes: NonZeroUsize::new(minimum_bytes)
                .expect("validated nonzero probe byte request"),
        })
    }

    /// Reports a recognized content format.
    #[must_use]
    pub const fn matched(container: ContainerId, confidence: ProbeConfidence) -> Self {
        Self::Match {
            container,
            confidence,
        }
    }
}

/// Request to open or relink a media source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRequest {
    media_id: MediaId,
    location: SourceLocation,
    expected_fingerprint: Option<String>,
}

impl SourceRequest {
    /// Creates a source request.
    #[must_use]
    pub fn new(media_id: MediaId, location: SourceLocation) -> Self {
        Self {
            media_id,
            location,
            expected_fingerprint: None,
        }
    }

    /// Requires the opened content to match an earlier source identity.
    pub fn with_expected_fingerprint(mut self, fingerprint: impl Into<String>) -> Result<Self> {
        let fingerprint = fingerprint.into();
        if fingerprint.trim().is_empty() {
            return Err(invalid(
                "set_expected_fingerprint",
                "expected fingerprint must not be empty",
            ));
        }
        self.expected_fingerprint = Some(fingerprint);
        Ok(self)
    }

    /// Returns the persistent project media identifier.
    #[must_use]
    pub const fn media_id(&self) -> MediaId {
        self.media_id
    }

    /// Returns the requested source location.
    #[must_use]
    pub const fn location(&self) -> &SourceLocation {
        &self.location
    }

    /// Returns the required prior fingerprint, when relinking must verify content.
    #[must_use]
    pub fn expected_fingerprint(&self) -> Option<&str> {
        self.expected_fingerprint.as_deref()
    }
}

/// How a source resolves a requested seek coordinate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SeekMode {
    /// Require the exact requested presentation coordinate.
    Exact,
    /// Seek to the closest independently decodable packet at or before the target.
    PreviousKeyframe,
    /// Seek to the independently decodable packet nearest the target.
    NearestKeyframe,
}

/// One exact seek request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SeekRequest {
    target: RationalTime,
    mode: SeekMode,
}

impl SeekRequest {
    /// Creates a seek request.
    #[must_use]
    pub const fn new(target: RationalTime, mode: SeekMode) -> Self {
        Self { target, mode }
    }

    /// Returns the requested presentation coordinate.
    #[must_use]
    pub const fn target(self) -> RationalTime {
        self.target
    }

    /// Returns the resolution mode.
    #[must_use]
    pub const fn mode(self) -> SeekMode {
        self.mode
    }
}

/// Open codec-neutral source used by ingest and playback consumers.
pub trait MediaSource: Send {
    /// Returns immutable source and stream information.
    fn info(&self) -> &SourceInfo;

    /// Returns one complete or usable partial packet, or explicit end of source.
    ///
    /// Implementations must poll the operation before work and between bounded reads. A partial
    /// packet must carry a corruption report and must never be returned as complete data.
    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>>;

    /// Seeks and returns the actual presentation coordinate selected by the source.
    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime>;
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-media-io.demux", operation))
}
