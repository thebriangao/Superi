//! Source-scoped video and audio stream selection.
//!
//! Selection is explicit whenever a source has alternate tracks. A convenience
//! constructor exists only for sources with exactly one video and one audio
//! stream, so container serialization order never becomes an accidental user
//! preference. Selected descriptors and packets retain their original timing,
//! codec, metadata, and identity for playback, relinking, and export consumers.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::decode::DecoderConfig;
use crate::demux::{Packet, SourceIdentity, SourceInfo, StreamId, StreamInfo, StreamKind};

/// One explicit video and audio stream choice within a media source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamPairRequest {
    video_stream_id: StreamId,
    audio_stream_id: StreamId,
}

impl StreamPairRequest {
    /// Creates an explicit pair from source-local stream identifiers.
    #[must_use]
    pub const fn new(video_stream_id: StreamId, audio_stream_id: StreamId) -> Self {
        Self {
            video_stream_id,
            audio_stream_id,
        }
    }

    /// Returns the requested video stream identifier.
    #[must_use]
    pub const fn video_stream_id(self) -> StreamId {
        self.video_stream_id
    }

    /// Returns the requested audio stream identifier.
    #[must_use]
    pub const fn audio_stream_id(self) -> StreamId {
        self.audio_stream_id
    }
}

/// An immutable, source-validated video and audio selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairedStreamSelection {
    source_identity: SourceIdentity,
    video_stream: StreamInfo,
    audio_stream: StreamInfo,
}

impl PairedStreamSelection {
    /// Selects one explicit video and audio stream from a source.
    pub fn select(source: &SourceInfo, request: StreamPairRequest) -> Result<Self> {
        let video_stream = find_stream(
            source,
            request.video_stream_id,
            StreamKind::Video,
            "select_stream_pair",
        )?;
        let audio_stream = find_stream(
            source,
            request.audio_stream_id,
            StreamKind::Audio,
            "select_stream_pair",
        )?;
        Ok(Self {
            source_identity: source.identity().clone(),
            video_stream: video_stream.clone(),
            audio_stream: audio_stream.clone(),
        })
    }

    /// Selects a pair only when exactly one video and one audio stream exist.
    ///
    /// A source with multiple candidates must be selected explicitly. This
    /// prevents file ordering from silently choosing an alternate language,
    /// accessibility mix, stem, or fallback encoding.
    pub fn select_unambiguous(source: &SourceInfo) -> Result<Self> {
        let video_stream = unique_stream(source, StreamKind::Video)?;
        let audio_stream = unique_stream(source, StreamKind::Audio)?;
        Self::select(
            source,
            StreamPairRequest::new(video_stream.id(), audio_stream.id()),
        )
    }

    /// Returns the stable project and content identity this pair belongs to.
    #[must_use]
    pub const fn source_identity(&self) -> &SourceIdentity {
        &self.source_identity
    }

    /// Returns the exact selected video descriptor.
    #[must_use]
    pub const fn video_stream(&self) -> &StreamInfo {
        &self.video_stream
    }

    /// Returns the exact selected audio descriptor.
    #[must_use]
    pub const fn audio_stream(&self) -> &StreamInfo {
        &self.audio_stream
    }

    /// Returns the explicit IDs represented by this pair.
    #[must_use]
    pub const fn request(&self) -> StreamPairRequest {
        StreamPairRequest::new(self.video_stream.id(), self.audio_stream.id())
    }

    /// Creates a decoder configuration for the selected video stream.
    #[must_use]
    pub fn video_decoder_config(&self) -> DecoderConfig {
        DecoderConfig::new(self.video_stream.clone())
    }

    /// Creates a decoder configuration for the selected audio stream.
    #[must_use]
    pub fn audio_decoder_config(&self) -> DecoderConfig {
        DecoderConfig::new(self.audio_stream.clone())
    }

    /// Classifies one packet without copying or changing any packet field.
    #[must_use]
    pub fn route_packet(&self, packet: Packet) -> SelectedPacket {
        if packet.stream_id() == self.video_stream.id() {
            SelectedPacket::Video(packet)
        } else if packet.stream_id() == self.audio_stream.id() {
            SelectedPacket::Audio(packet)
        } else {
            SelectedPacket::Unselected(packet)
        }
    }

    /// Revalidates this pair against a reopened or relinked source.
    ///
    /// A location change is intentionally absent from `SourceInfo`, so a moved
    /// file with the same project media ID and content fingerprint can rebind.
    /// Different project identity or content requires an explicit new selection.
    pub fn rebind(&self, source: &SourceInfo) -> Result<Self> {
        let expected_media_id = self.source_identity.media_id();
        let actual_media_id = source.identity().media_id();
        if expected_media_id != actual_media_id {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "relinked source has a different project media identity",
            )
            .with_context(
                ErrorContext::new("superi-media-io.selection", "rebind_stream_pair")
                    .with_field("expected_media_id", expected_media_id.to_string())
                    .with_field("actual_media_id", actual_media_id.to_string()),
            ));
        }

        let expected_fingerprint = self.source_identity.fingerprint();
        let actual_fingerprint = source.identity().fingerprint();
        if expected_fingerprint != actual_fingerprint {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "relinked source content does not match the selected stream pair",
            )
            .with_context(
                ErrorContext::new("superi-media-io.selection", "rebind_stream_pair")
                    .with_field("expected_fingerprint", expected_fingerprint)
                    .with_field("actual_fingerprint", actual_fingerprint),
            ));
        }

        Self::select(source, self.request())
    }
}

/// The relationship between one demuxed packet and a selected pair.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SelectedPacket {
    /// A packet belonging to the selected video stream.
    Video(Packet),
    /// A packet belonging to the selected audio stream.
    Audio(Packet),
    /// A packet for an alternate or supplementary stream.
    Unselected(Packet),
}

impl SelectedPacket {
    /// Returns the unchanged packet in any routing state.
    #[must_use]
    pub const fn packet(&self) -> &Packet {
        match self {
            Self::Video(packet) | Self::Audio(packet) | Self::Unselected(packet) => packet,
        }
    }

    /// Returns ownership of the unchanged packet.
    #[must_use]
    pub fn into_packet(self) -> Packet {
        match self {
            Self::Video(packet) | Self::Audio(packet) | Self::Unselected(packet) => packet,
        }
    }
}

fn find_stream<'a>(
    source: &'a SourceInfo,
    stream_id: StreamId,
    expected_kind: StreamKind,
    operation: &'static str,
) -> Result<&'a StreamInfo> {
    let Some(stream) = source
        .streams()
        .iter()
        .find(|stream| stream.id() == stream_id)
    else {
        return Err(Error::new(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "selected stream does not exist in the media source",
        )
        .with_context(
            selection_context(source, operation)
                .with_field("stream_id", stream_id.value().to_string())
                .with_field("expected_kind", stream_kind_code(expected_kind)),
        ));
    };
    if stream.kind() != expected_kind {
        return Err(Error::new(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "selected stream has the wrong media kind",
        )
        .with_context(
            selection_context(source, operation)
                .with_field("stream_id", stream_id.value().to_string())
                .with_field("expected_kind", stream_kind_code(expected_kind))
                .with_field("actual_kind", stream_kind_code(stream.kind())),
        ));
    }
    Ok(stream)
}

fn unique_stream(source: &SourceInfo, kind: StreamKind) -> Result<&StreamInfo> {
    let mut candidates = source
        .streams()
        .iter()
        .filter(|stream| stream.kind() == kind);
    let first = candidates.next();
    let remaining = candidates.count();
    let candidate_count = usize::from(first.is_some()) + remaining;
    match candidate_count {
        1 => Ok(first.expect("one candidate was counted")),
        0 => Err(Error::new(
            ErrorCategory::Unsupported,
            Recoverability::Degraded,
            "media source does not contain a stream required for paired selection",
        )
        .with_context(
            selection_context(source, "select_unambiguous_stream_pair")
                .with_field("stream_kind", stream_kind_code(kind))
                .with_field("candidate_count", "0"),
        )),
        _ => Err(Error::new(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "media source has multiple candidates and requires an explicit stream choice",
        )
        .with_context(
            selection_context(source, "select_unambiguous_stream_pair")
                .with_field("stream_kind", stream_kind_code(kind))
                .with_field("candidate_count", candidate_count.to_string()),
        )),
    }
}

fn selection_context(source: &SourceInfo, operation: &'static str) -> ErrorContext {
    ErrorContext::new("superi-media-io.selection", operation)
        .with_field("media_id", source.identity().media_id().to_string())
        .with_field("fingerprint", source.identity().fingerprint())
}

const fn stream_kind_code(kind: StreamKind) -> &'static str {
    match kind {
        StreamKind::Video => "video",
        StreamKind::Audio => "audio",
        StreamKind::Subtitle => "subtitle",
        StreamKind::Data => "data",
    }
}
