//! Transparent selection between replaceable proxy packets and authoritative source media.

use std::sync::Arc;

use superi_cache::key::MediaCacheIdentity;
use superi_cache::proxy::{
    DerivedMediaArtifact, DerivedMediaPurpose, DerivedMediaQuality as CacheQuality,
};
use superi_concurrency::jobs::{
    DerivedFallbackPolicy, DerivedMediaCandidate, DerivedMediaRequest, DerivedMediaSelection,
    DerivedQuality,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{RationalTime, TimeRounding};
use superi_media_io::demux::{
    MediaSource, Packet, SeekMode, SeekRequest, SourceIdentity, SourceInfo, StreamInfo, StreamKind,
};
use superi_media_io::encode::EncoderMediaFormat;
use superi_media_io::operation::OperationContext;
use superi_media_io::read::ReadOutcome;

use crate::derived_media::EncodedDerivedMedia;

/// One authoritative source request plus its explicit replaceable-media policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxySubstitutionRequest {
    source: SourceIdentity,
    source_revision: u64,
    quality: DerivedQuality,
    fallback: DerivedFallbackPolicy,
}

impl ProxySubstitutionRequest {
    /// Creates a request that never permits derived media to replace source identity.
    #[must_use]
    pub const fn new(
        source: SourceIdentity,
        source_revision: u64,
        quality: DerivedQuality,
        fallback: DerivedFallbackPolicy,
    ) -> Self {
        Self {
            source,
            source_revision,
            quality,
            fallback,
        }
    }

    /// Returns the authoritative project and source-content identity.
    #[must_use]
    pub const fn source(&self) -> &SourceIdentity {
        &self.source
    }

    /// Returns the exact source revision required for derived freshness.
    #[must_use]
    pub const fn source_revision(&self) -> u64 {
        self.source_revision
    }

    /// Returns the requested derived quality.
    #[must_use]
    pub const fn quality(&self) -> DerivedQuality {
        self.quality
    }

    /// Returns the explicit original-media fallback policy.
    #[must_use]
    pub const fn fallback(&self) -> DerivedFallbackPolicy {
        self.fallback
    }
}

/// One open media source plus explicit evidence of why its representation was selected.
pub struct ResolvedMediaSource {
    selection: DerivedMediaSelection,
    source: Box<dyn MediaSource>,
}

impl ResolvedMediaSource {
    /// Returns the deterministic proxy or original-media selection evidence.
    #[must_use]
    pub const fn selection(&self) -> DerivedMediaSelection {
        self.selection
    }
}

impl MediaSource for ResolvedMediaSource {
    fn info(&self) -> &SourceInfo {
        self.source.info()
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        self.source.read_packet(operation)
    }

    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime> {
        self.source.seek(request, operation)
    }
}

/// Resolves a fresh proxy when policy permits, otherwise lazily opens the authoritative source.
///
/// Only proxy-purpose artifacts with exact source ID, source fingerprint, and source revision enter
/// scheduler selection. The existing scheduler then owns exact, lower-quality, cache-ID tie, and
/// source-only policy. Invalid packet artifacts are treated as unavailable derived media, so they
/// cannot block deterministic original-media fallback.
pub fn resolve_proxy_source<F>(
    request: ProxySubstitutionRequest,
    artifacts: &[Arc<DerivedMediaArtifact<EncodedDerivedMedia>>],
    open_original: F,
    operation: &OperationContext,
) -> Result<ResolvedMediaSource>
where
    F: FnOnce(&OperationContext) -> Result<Box<dyn MediaSource>>,
{
    operation.check("resolve_proxy_source")?;
    let media = MediaCacheIdentity::new(request.source.media_id(), request.source.fingerprint())?;
    let prepared = artifacts
        .iter()
        .filter_map(|artifact| {
            PreparedProxy::new(
                Arc::clone(artifact),
                &request.source,
                media,
                request.source_revision,
            )
        })
        .collect::<Vec<_>>();
    let candidates = prepared
        .iter()
        .map(PreparedProxy::candidate)
        .collect::<Vec<_>>();
    let selection = DerivedMediaRequest::new(
        request.source.media_id(),
        request.source_revision,
        request.quality,
        request.fallback,
    )
    .select(&candidates);

    if let Some(cache_id) = selection.cache_id() {
        let selected = prepared
            .into_iter()
            .find(|candidate| candidate.artifact.cache_id() == cache_id)
            .ok_or_else(|| {
                internal(
                    "resolve_proxy_source",
                    "derived selection did not resolve to its prepared proxy artifact",
                )
            })?;
        operation.check("open_proxy_source")?;
        return Ok(ResolvedMediaSource {
            selection,
            source: Box::new(selected.open()),
        });
    }

    let source = open_original(operation)?;
    operation.check("open_original_source")?;
    if source.info().identity() != &request.source {
        return Err(conflict(
            "open_original_source",
            "original-media fallback returned a different source identity",
        ));
    }
    Ok(ResolvedMediaSource { selection, source })
}

struct PreparedProxy {
    artifact: Arc<DerivedMediaArtifact<EncodedDerivedMedia>>,
    quality: DerivedQuality,
    info: SourceInfo,
}

impl PreparedProxy {
    fn new(
        artifact: Arc<DerivedMediaArtifact<EncodedDerivedMedia>>,
        source: &SourceIdentity,
        media: MediaCacheIdentity,
        source_revision: u64,
    ) -> Option<Self> {
        let generated_request = artifact.request();
        if generated_request.purpose() != DerivedMediaPurpose::Proxy
            || !artifact.is_fresh(media, source_revision)
        {
            return None;
        }
        let quality = scheduler_quality(generated_request.quality())?;
        let generated = artifact.payload();
        let config = generated.config();
        let kind = match config.media_format() {
            EncoderMediaFormat::Video(_) => StreamKind::Video,
            EncoderMediaFormat::Audio(_) => StreamKind::Audio,
            _ => return None,
        };
        let packets = generated.packets();
        if packets.is_empty()
            || !packets.iter().any(Packet::is_keyframe)
            || !packets
                .iter()
                .any(|packet| packet.timing().presentation_time().is_some())
            || packets.iter().any(|packet| {
                packet.stream_id() != config.stream_id()
                    || packet.timing().timebase() != config.timebase()
                    || packet.data().is_empty()
            })
        {
            return None;
        }
        let mut stream = StreamInfo::new(
            config.stream_id(),
            kind,
            config.codec().clone(),
            config.timebase(),
        );
        if let Some(configuration) = packets
            .iter()
            .find_map(|packet| packet.metadata().get("codec.configuration"))
        {
            stream = stream
                .with_metadata("codec.configuration", configuration.clone())
                .ok()?;
        }
        let info = SourceInfo::new(source.clone(), vec![stream]).ok()?;
        Some(Self {
            artifact,
            quality,
            info,
        })
    }

    fn candidate(&self) -> DerivedMediaCandidate {
        let request = self.artifact.request();
        DerivedMediaCandidate::new(
            self.artifact.cache_id(),
            request.media().media_id(),
            request.source_revision(),
            self.quality,
        )
    }

    fn open(self) -> GeneratedPacketSource {
        GeneratedPacketSource {
            info: self.info,
            artifact: self.artifact,
            cursor: 0,
        }
    }
}

struct GeneratedPacketSource {
    info: SourceInfo,
    artifact: Arc<DerivedMediaArtifact<EncodedDerivedMedia>>,
    cursor: usize,
}

impl MediaSource for GeneratedPacketSource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        operation.check("read_proxy_packet")?;
        let packets = self.artifact.payload().packets();
        let Some(packet) = packets.get(self.cursor).cloned() else {
            return Ok(ReadOutcome::EndOfStream);
        };
        self.cursor += 1;
        Ok(ReadOutcome::Complete(packet))
    }

    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime> {
        operation.check("seek_proxy_source")?;
        let packets = self.artifact.payload().packets();
        let selected = match request.mode() {
            SeekMode::Exact => {
                let target = packets
                    .iter()
                    .position(|packet| {
                        packet
                            .timing()
                            .presentation_time()
                            .is_some_and(|time| time == request.target())
                    })
                    .ok_or_else(|| {
                        invalid(
                            "seek_proxy_source",
                            "exact seek target is not a proxy packet boundary",
                        )
                    })?;
                let decode_start = packets[..=target]
                    .iter()
                    .rposition(Packet::is_keyframe)
                    .ok_or_else(|| {
                        invalid(
                            "seek_proxy_source",
                            "proxy contains no keyframe at or before the exact seek target",
                        )
                    })?;
                let actual = packet_time(&packets[target]);
                (decode_start, actual)
            }
            SeekMode::PreviousKeyframe => packets
                .iter()
                .enumerate()
                .filter(|(_, packet)| packet.is_keyframe())
                .filter_map(|(index, packet)| {
                    let time = packet.timing().presentation_time()?;
                    (time <= request.target()).then_some((index, time))
                })
                .max_by(|left, right| {
                    left.1
                        .partial_cmp(&right.1)
                        .expect("rational time ordering is total")
                        .then_with(|| right.0.cmp(&left.0))
                })
                .ok_or_else(|| {
                    invalid(
                        "seek_proxy_source",
                        "proxy contains no keyframe at or before the seek target",
                    )
                })?,
            SeekMode::NearestKeyframe => {
                let timebase = self.artifact.payload().config().timebase();
                let target = request
                    .target()
                    .checked_rescale(timebase, TimeRounding::NearestTiesEven)?
                    .value();
                packets
                    .iter()
                    .enumerate()
                    .filter(|(_, packet)| packet.is_keyframe())
                    .filter_map(|(index, packet)| {
                        let time = packet.timing().presentation_time()?;
                        let distance = (i128::from(time.value()) - i128::from(target)).abs();
                        Some((index, time, distance))
                    })
                    .min_by(|left, right| {
                        left.2
                            .cmp(&right.2)
                            .then_with(|| {
                                left.1
                                    .partial_cmp(&right.1)
                                    .expect("rational time ordering is total")
                            })
                            .then_with(|| left.0.cmp(&right.0))
                    })
                    .map(|(index, time, _)| (index, time))
                    .ok_or_else(|| invalid("seek_proxy_source", "proxy contains no keyframe"))?
            }
            _ => {
                return Err(unsupported(
                    "seek_proxy_source",
                    "proxy source does not recognize this seek mode",
                ));
            }
        };
        operation.check("seek_proxy_source")?;
        self.cursor = selected.0;
        Ok(selected.1)
    }
}

fn packet_time(packet: &Packet) -> RationalTime {
    packet
        .timing()
        .presentation_time()
        .expect("prepared proxy packets have presentation timestamps")
}

fn scheduler_quality(quality: CacheQuality) -> Option<DerivedQuality> {
    match quality {
        CacheQuality::Eighth => Some(DerivedQuality::Eighth),
        CacheQuality::Quarter => Some(DerivedQuality::Quarter),
        CacheQuality::Half => Some(DerivedQuality::Half),
        CacheQuality::Full => Some(DerivedQuality::Full),
        _ => None,
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(
        "superi-engine.proxy-substitution",
        operation,
    ))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(
        "superi-engine.proxy-substitution",
        operation,
    ))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(
        "superi-engine.proxy-substitution",
        operation,
    ))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message).with_context(
        ErrorContext::new("superi-engine.proxy-substitution", operation),
    )
}
